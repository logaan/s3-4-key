use crate::{
    display::Display,
    power::{PmicBus, Power},
};
use esp_idf_svc::hal::{
    delay::FreeRtos,
    gpio::{AnyInputPin, Input, Output, PinDriver, Pull},
    i2c::{I2cConfig, I2cDriver},
    peripherals::Peripherals,
    spi::{
        SpiDeviceDriver,
        config::{Config as SpiConfig, DriverConfig},
    },
    units::*,
};

pub struct Hardware {
    pub inputs: Inputs,
    pub power: Power,
    pub display: Display,
}

pub struct Inputs {
    keys: [PinDriver<'static, Input>; 5],
    blue: PinDriver<'static, Input>,
    // Retaining these drivers keeps each independent switch return LOW.
    _returns: [PinDriver<'static, Output>; 4],
}

impl Hardware {
    pub fn new() -> anyhow::Result<Self> {
        let p = Peripherals::take()?;
        let pins = p.pins;
        // Hat2-Bus top header (2x8, 2.54 mm). One row is GND/EXT_5V/Boot/G1/G8/
        // BAT/3V3_L2/5V_IN; the other row is a contiguous GPIO run, in order:
        // G5 G4 G6 G7 G43 G44 G2 G3. Every switch plugs into that GPIO row with
        // its two leads side by side, and the firmware holds each neighbour LOW.
        //
        // Open-drain LOW is sufficient for the tiny internal pull-up current.
        // Even an accidental HIGH latch would float, never source current into a switch.
        let mut returns = [
            PinDriver::output_od(pins.gpio4)?,  // A return
            PinDriver::output_od(pins.gpio7)?,  // S return
            PinDriver::output_od(pins.gpio44)?, // D return (UART0 RXD; console is on USB)
            PinDriver::output_od(pins.gpio3)?,  // F return (LOW matches the default JTAG strap)
        ];
        for pin in &mut returns {
            pin.set_low()?;
        }
        let inputs = Inputs {
            keys: [
                PinDriver::input(pins.gpio5, Pull::Up)?,  // A: G5 <-> G4 (LOW)
                PinDriver::input(pins.gpio6, Pull::Up)?,  // S: G6 <-> G7 (LOW)
                PinDriver::input(pins.gpio43, Pull::Up)?, // D: G43 <-> G44 (LOW); UART0 TXD, unused
                PinDriver::input(pins.gpio2, Pull::Up)?,  // F: G2 <-> G3 (LOW)
                PinDriver::input(pins.gpio12, Pull::Up)?, // Right button: Backspace
            ],
            blue: PinDriver::input(pins.gpio11, Pull::Up)?,
            _returns: returns,
        };
        let i2c = I2cDriver::new(
            p.i2c1,
            pins.gpio47,
            pins.gpio48,
            &I2cConfig::new().baudrate(100.kHz().into()),
        )?;
        let mut power = Power::new(PmicBus(i2c));
        power.init()?;
        FreeRtos::delay_ms(100); // Let the LCD's L3B power rail settle.
        let backlight = PinDriver::output(pins.gpio38)?;
        let spi = SpiDeviceDriver::new_single(
            p.spi3,
            pins.gpio40,
            pins.gpio39,
            None::<AnyInputPin>,
            Some(pins.gpio41),
            &DriverConfig::new(),
            &SpiConfig::new().baudrate(20.MHz().into()),
        )?;
        let display = Display::new(
            spi,
            PinDriver::output(pins.gpio45)?,
            PinDriver::output(pins.gpio21)?,
            backlight,
        )?;
        Ok(Self {
            inputs,
            power,
            display,
        })
    }
}

impl Inputs {
    pub fn read(&self) -> ([bool; 5], bool) {
        (
            core::array::from_fn(|i| self.keys[i].is_low()),
            self.blue.is_low(),
        )
    }
}
