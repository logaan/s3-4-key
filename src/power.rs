//! Minimal M5PM1 register driver for the StickS3, not older StickC PMICs.
//! Register meanings follow the M5Stack M5PM1 datasheet and M5Unified driver.
use crate::battery::BatteryStatus;

pub const ADDRESS: u8 = 0x6e;

pub trait RegisterIo {
    type Error;
    fn read(&mut self, register: u8, bytes: &mut [u8]) -> Result<(), Self::Error>;
    fn write(&mut self, register: u8, value: u8) -> Result<(), Self::Error>;
}

pub struct M5Pm1<I> {
    io: I,
}

impl<I: RegisterIo> M5Pm1<I> {
    pub fn new(io: I) -> Self {
        Self { io }
    }

    pub fn init(&mut self) -> Result<(), I::Error> {
        self.io.write(0x09, 0)?; // Disable I2C idle sleep; wake attempts are retried by the adapter.
        self.io.write(0x0a, 0)?; // Disable a watchdog left by previous firmware.
        // Set output latches before enabling drivers: G2 = LCD/L3B power, G3 = speaker amp.
        self.update(0x11, 0x0c, 0x04)?;
        self.update(0x13, 0x0c, 0)?; // Push-pull for PMIC GPIO2 and GPIO3.
        self.update(0x16, 0xf3, 0)?; // Two mux bits per pin: GPIO0, GPIO2 and GPIO3 -> GPIO.
        self.update(0x10, 0x0d, 0x0c)?; // GPIO0 charge-status input; GPIO2/GPIO3 outputs.
        self.update(0x14, 0x03, 0x01)?; // Pull up active-low open-drain CHG_STAT on PMIC GPIO0.
        self.update(0x06, 0x08, 0)?; // Unused 5 V boost/IR off, preserving charging and ESP power.
        // Do NOT alter the PMIC's power/reset button or download-mode configuration.
        Ok(())
    }

    pub fn battery(&mut self) -> Result<Option<BatteryStatus>, I::Error> {
        let mut bytes = [0; 2];
        self.io.read(0x22, &mut bytes)?;
        let millivolts = u16::from_le_bytes(bytes) & 0x0fff;
        let mut gpio = [0];
        self.io.read(0x12, &mut gpio)?;
        Ok(BatteryStatus::from_millivolts(millivolts, gpio[0] & 1 == 0))
    }

    fn update(&mut self, register: u8, mask: u8, value: u8) -> Result<(), I::Error> {
        let mut bytes = [0];
        self.io.read(register, &mut bytes)?;
        self.io.write(register, (bytes[0] & !mask) | (value & mask))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Registers([u8; 256]);
    impl RegisterIo for Registers {
        type Error = ();
        fn read(&mut self, reg: u8, bytes: &mut [u8]) -> Result<(), ()> {
            bytes.copy_from_slice(&self.0[usize::from(reg)..usize::from(reg) + bytes.len()]);
            Ok(())
        }
        fn write(&mut self, reg: u8, value: u8) -> Result<(), ()> {
            self.0[usize::from(reg)] = value;
            Ok(())
        }
    }

    #[test]
    fn initialization_enables_display_not_speaker_and_preserves_power_controls() {
        let mut pmic = M5Pm1::new(Registers([0xff; 256]));
        pmic.init().unwrap();
        assert_eq!(pmic.io.0[0x09], 0);
        assert_eq!(pmic.io.0[0x0a], 0);
        assert_eq!(pmic.io.0[0x11], 0xf7);
        assert_eq!(pmic.io.0[0x13], 0xf3);
        assert_eq!(pmic.io.0[0x16], 0x0c); // GPIO1 IRQ/mux untouched.
        assert_eq!(pmic.io.0[0x10], 0xfe);
        assert_eq!(pmic.io.0[0x06], 0xf7); // Never disable DCDC/LDO/charging.
        for reg in [0x07, 0x0c, 0x42, 0x45, 0x50, 0x51] {
            assert_eq!(pmic.io.0[reg], 0xff);
        }
    }

    #[test]
    fn reads_little_endian_millivolts_and_active_low_charging() {
        let mut pmic = M5Pm1::new(Registers([0; 256]));
        pmic.io.0[0x22..0x24].copy_from_slice(&3800_u16.to_le_bytes());
        assert_eq!(
            pmic.battery().unwrap(),
            BatteryStatus::from_millivolts(3800, true)
        );
        pmic.io.0[0x12] = 1;
        assert!(!pmic.battery().unwrap().unwrap().charging);
    }
}
