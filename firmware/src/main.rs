//! S3 4-Key: an M5Stack StickS3 running as a BLE HID keyboard.
//! Four external switches (A, S, D, F), side button = Backspace,
//! blue front button = pairing reset, screen = status.
mod ble;
mod board;
mod display;
mod power;

use ble::BleKeyboard;
use s3_4_key_core::{battery::BatteryStatus, hid::Key, input::Keyboard, ui::ConnectionStatus};

const TICK_MS: u64 = 10;
const BATTERY_PERIOD_TICKS: u64 = 500; // 5 s
const MAX_CONSECUTIVE_ERRORS: u32 = 5;

#[derive(Clone, Copy, PartialEq)]
enum BleStatus {
    Pairing,
    Reconnecting,
    Connected,
    Resetting,
    Failed,
}

struct Status {
    ble: BleStatus,
    battery: Option<BatteryStatus>,
    last_key: Option<Key>,
}

impl Status {
    const fn new() -> Self {
        Self {
            ble: BleStatus::Pairing,
            battery: None,
            last_key: None,
        }
    }

    fn to_ui(&self) -> s3_4_key_core::ui::UiState {
        s3_4_key_core::ui::UiState {
            battery: self.battery,
            connection: match self.ble {
                BleStatus::Pairing => ConnectionStatus::Pairing,
                BleStatus::Reconnecting => ConnectionStatus::Reconnecting,
                BleStatus::Connected => ConnectionStatus::Connected,
                BleStatus::Resetting => ConnectionStatus::Resetting,
                BleStatus::Failed => ConnectionStatus::Error,
            },
            last_key: self.last_key,
        }
    }
}

fn main() -> anyhow::Result<()> {
    esp_idf_svc::sys::link_patches();
    esp_idf_svc::log::EspLogger::initialize_default();
    log::info!("s3-4-key starting");

    let mut hardware = match board::Hardware::new() {
        Ok(hardware) => hardware,
        Err(error) => {
            log::error!("board init failed: {error:#}");
            restart();
        }
    };

    let mut ble = match BleKeyboard::new(Keyboard::new()) {
        Ok(ble) => ble,
        Err(error) => {
            log::error!("BLE init failed: {error:#}");
            restart();
        }
    };

    let mut status = Status::new();
    let mut dirty = true;
    let mut ticks: u64 = 0;
    let mut consecutive_errors: u32 = 0;

    wait_for_blue_release(&hardware);

    loop {
        let now_ms = ticks * TICK_MS;

        if ble.poisoned() {
            log::warn!("recovering keyboard core state");
            if ble.reset().is_err() {
                fail_fast(&mut consecutive_errors);
            }
            dirty = true;
        }

        let (raw_keys, blue) = hardware.inputs.read();
        let update = match ble.poll(now_ms, raw_keys, blue) {
            Ok(update) => update,
            Err(()) => {
                fail_fast(&mut consecutive_errors);
                s3_4_key_core::input::InputUpdate::default()
            }
        };
        consecutive_errors = 0;

        if update.pair_requested {
            status.last_key = None;
            status.ble = BleStatus::Resetting;
            render(&mut hardware, &status);
            match ble.reset_pairing() {
                Ok(()) => {
                    log::info!("pairing reset complete; advertising");
                    status.ble = BleStatus::Pairing;
                }
                Err(error) => {
                    log::error!("pairing reset failed: {error:#}");
                    status.ble = BleStatus::Failed;
                    fail_fast(&mut consecutive_errors);
                }
            }
            render(&mut hardware, &status);
            continue;
        }
        if let Some(key) = update.last_pressed {
            status.last_key = Some(key);
            dirty = true;
        }
        ble.send_report(update.report);

        let connected = ble.connected();
        if connected != (status.ble == BleStatus::Connected) {
            status.ble = if connected {
                BleStatus::Connected
            } else if ble.bond_count() > 0 {
                BleStatus::Reconnecting
            } else {
                BleStatus::Pairing
            };
            dirty = true;
        }

        if ticks % BATTERY_PERIOD_TICKS == 0 {
            match hardware.power.battery() {
                Ok(battery) if battery != status.battery => {
                    status.battery = battery;
                    dirty = true;
                }
                Ok(_) => {}
                Err(error) => log::warn!("battery read failed: {error}"),
            }
        }

        if dirty {
            dirty = false;
            render(&mut hardware, &status);
        }

        ticks = ticks.wrapping_add(1);
        esp_idf_svc::hal::delay::FreeRtos::delay_ms(TICK_MS as u32);
    }
}

/// A blue button held down through reset is treated as a stuck button, not a
/// pairing request; require a clean release before normal operation.
fn wait_for_blue_release(hardware: &board::Hardware) {
    if !hardware.inputs.read().1 {
        return;
    }
    log::info!("blue button held at boot; waiting for release");
    while hardware.inputs.read().1 {
        esp_idf_svc::hal::delay::FreeRtos::delay_ms(20);
    }
    log::info!("blue button released");
}

fn render(hardware: &mut board::Hardware, status: &Status) {
    if let Err(error) = hardware.display.render(status.to_ui()) {
        log::warn!("display render failed: {error}");
    }
}

fn fail_fast(consecutive_errors: &mut u32) {
    *consecutive_errors += 1;
    if *consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
        log::error!("too many consecutive errors; restarting");
        restart();
    }
}

/// Deliberate, logged reboot when the system cannot recover on its own.
fn restart() -> ! {
    // SAFETY: esp_restart resets the chip; we never return from it.
    unsafe { esp_idf_svc::sys::esp_restart() }
}
