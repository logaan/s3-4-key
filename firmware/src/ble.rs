//! BLE HID keyboard over NimBLE, with bond management and fault isolation.
extern crate alloc;

use alloc::sync::Arc;
use alloc::vec::Vec;
use esp32_nimble::{
    BLEAdvertisementData, BLECharacteristic, BLEDevice, BLEHIDDevice, enums::*,
    utilities::mutex::Mutex,
};
use s3_4_key_core::input::{InputUpdate, Keyboard};
use std::panic::{AssertUnwindSafe, catch_unwind};

/// Standard eight-byte keyboard report: modifiers, reserved, six key usages.
type Report = [u8; 8];

/// Report ID referenced by the Report Reference descriptors.
const KEYBOARD_ID: u8 = 1;

/// Boot-protocol-shaped keyboard report map (report ID 1).
const REPORT_MAP: &[u8] = &[
    0x05, 0x01, // Usage Page (Generic Desktop)
    0x09, 0x06, // Usage (Keyboard)
    0xa1, 0x01, // Collection (Application)
    0x85, 0x01, //   Report ID (1)
    0x05, 0x07, //   Usage Page (Keyboard/Keypad)
    0x19, 0xe0, 0x29, 0xe7, //   Modifier usages
    0x15, 0x00, 0x25, 0x01, //   Logical Minimum/Maximum
    0x75, 0x01, 0x95, 0x08, //   Report Size/Count
    0x81, 0x02, //   Input (Data, Var, Abs) - modifier byte
    0x95, 0x01, 0x75, 0x08, 0x81, 0x01, //   Input (Const) - reserved byte
    0x95, 0x06, 0x75, 0x08, 0x15, 0x00, 0x25, 0x65, 0x05, 0x07, 0x19, 0x00, 0x29, 0x65, 0x81,
    0x00, //   Input (Data, Array) - six key slots
    0xc0, // End Collection
];

const DEVICE_NAME: &str = "S3 4-Key";

pub trait KeyboardCore {
    fn poll(&mut self, now_ms: u64, keys: [bool; 5], blue: bool) -> InputUpdate;
}

impl KeyboardCore for Keyboard {
    fn poll(&mut self, now_ms: u64, keys: [bool; 5], blue: bool) -> InputUpdate {
        self.poll(now_ms, keys, blue)
    }
}

/// Connection state shared with NimBLE callback context.
struct Flags(core::sync::atomic::AtomicU8);

impl Flags {
    const CONNECTED: u8 = 1;
    const AUTHENTICATED: u8 = 2;

    const fn new() -> Self {
        Self(core::sync::atomic::AtomicU8::new(0))
    }
    fn set(&self, flag: u8, on: bool) {
        use core::sync::atomic::Ordering;
        if on {
            self.0.fetch_or(flag, Ordering::Release);
        } else {
            self.0.fetch_and(!flag, Ordering::Release);
        }
    }
    fn has(&self, flag: u8) -> bool {
        use core::sync::atomic::Ordering;
        self.0.load(Ordering::Acquire) & flag != 0
    }
}

static FLAGS: Flags = Flags::new();
static INPUT_CHR: Mutex<Option<Arc<Mutex<BLECharacteristic>>>> = Mutex::new(None);

/// Fault-isolated facade: a panic in the input core is converted into an error
/// state instead of taking the whole keyboard down.
pub struct BleKeyboard<ST> {
    st: Option<ST>,
    poisoned: bool,
}

impl<ST: KeyboardCore> BleKeyboard<ST> {
    pub fn new(st: ST) -> anyhow::Result<Self> {
        Self::setup()?;
        Ok(Self {
            st: Some(st),
            poisoned: false,
        })
    }

    fn setup() -> anyhow::Result<()> {
        let device = BLEDevice::take();
        device
            .security()
            .set_auth(AuthReq::all())
            .set_io_cap(SecurityIOCap::NoInputNoOutput)
            .resolve_rpa();

        let server = device.get_server();
        server
            .on_connect(|_, desc| {
                log::info!("BLE connect: {:?}", desc);
                FLAGS.set(Flags::CONNECTED, true);
            })
            .on_disconnect(|_, _| {
                log::info!("BLE disconnected");
                FLAGS.set(Flags::CONNECTED, false);
                FLAGS.set(Flags::AUTHENTICATED, false);
            })
            .on_authentication_complete(|_, desc, result| {
                log::info!("authentication complete: {:?} {:?}", desc, result);
                FLAGS.set(Flags::AUTHENTICATED, result.is_ok());
            });

        let mut hid = BLEHIDDevice::new(server);
        hid.report_map(REPORT_MAP);
        hid.hid_info(0x00, 0x01);
        hid.pnp(0x02, 0x1234, 0x5678, 0x0100);
        hid.manufacturer("s3-4-key");
        *INPUT_CHR.lock() = Some(hid.input_report(KEYBOARD_ID));
        let output = hid.output_report(KEYBOARD_ID);
        // Keyboard LED state (num lock etc.) is accepted and ignored.
        output.lock().on_write(|args| {
            log::debug!("LED report: {:02x?}", args.recv_data());
        });
        hid.set_battery_level(50);

        let advertising = device.get_advertising();
        advertising.lock().scan_response(false).set_data(
            BLEAdvertisementData::new()
                .name(DEVICE_NAME)
                .appearance(0x03c1)
                .add_service_uuid(hid.hid_service().lock().uuid()),
        )?;
        advertising.lock().start()?;
        log::info!("advertising as {DEVICE_NAME:?}");
        Ok(())
    }

    pub fn poisoned(&self) -> bool {
        self.poisoned
    }

    pub fn poll(&mut self, now_ms: u64, keys: [bool; 5], blue: bool) -> Result<InputUpdate, ()> {
        let Some(st) = self.st.as_mut() else {
            return Err(());
        };
        let update =
            catch_unwind(AssertUnwindSafe(|| st.poll(now_ms, keys, blue))).map_err(|_| {
                self.poisoned = true;
                log::error!("keyboard core panicked; output frozen until reset");
            })?;
        Ok(update)
    }

    pub fn connected(&self) -> bool {
        FLAGS.has(Flags::CONNECTED)
    }

    pub fn bond_count(&self) -> usize {
        BLEDevice::take()
            .bonded_addresses()
            .map_or(0, |bonds| bonds.len())
    }

    /// Sends a report; no-op when disconnected. Per-connection notify failures
    /// are logged by the NimBLE wrapper (they usually end in a disconnect event).
    pub fn send_report(&self, report: Report) {
        let Some(characteristic) = INPUT_CHR.lock().clone() else {
            return;
        };
        characteristic.lock().set_value(&report).notify();
    }

    /// Erases the stored computer pairing and re-enters pairing mode.
    pub fn reset_pairing(&mut self) -> anyhow::Result<()> {
        log::info!("pairing reset: disconnecting and erasing bonds");
        let device = BLEDevice::take();
        let handles: Vec<u16> = device
            .get_server()
            .connections()
            .map(|desc| desc.conn_handle())
            .collect();
        for handle in handles {
            let _ = device.get_server().disconnect(handle);
        }
        let bonds = self.bond_count();
        device.delete_all_bonds()?;
        log::info!("erased {bonds} bond(s)");
        self.reset()
    }

    /// Returns to advertising without rebuilding the GATT database.
    pub fn reset(&mut self) -> anyhow::Result<()> {
        self.poisoned = false;
        FLAGS.set(Flags::CONNECTED, false);
        FLAGS.set(Flags::AUTHENTICATED, false);
        let advertising = BLEDevice::take().get_advertising();
        {
            let mut advertising = advertising.lock();
            if advertising.is_advertising() {
                advertising.stop()?;
            }
            // A disconnect callback may have restarted advertising between the
            // stop above and here; only start if that has not happened.
            if !advertising.is_advertising() {
                advertising.start()?;
            }
        }
        Ok(())
    }
}
