use esp_idf_svc::{
    hal::{
        delay::{FreeRtos, TickType},
        i2c::I2cDriver,
    },
    sys::EspError,
};
use s3_4_key_core::power::{ADDRESS, M5Pm1, RegisterIo};

pub type Power = M5Pm1<PmicBus>;

pub struct PmicBus(pub I2cDriver<'static>);

impl RegisterIo for PmicBus {
    type Error = EspError;

    fn read(&mut self, register: u8, bytes: &mut [u8]) -> Result<(), EspError> {
        for attempt in 0..3 {
            let result = self.0.write_read(
                ADDRESS,
                &[register],
                bytes,
                TickType::new_millis(50).ticks(),
            );
            match result {
                Ok(()) => return Ok(()),
                Err(error) if attempt == 2 => return Err(error),
                Err(_) => FreeRtos::delay_ms(5),
            }
        }
        unreachable!()
    }

    fn write(&mut self, register: u8, value: u8) -> Result<(), EspError> {
        for attempt in 0..3 {
            let result = self.0.write(
                ADDRESS,
                &[register, value],
                TickType::new_millis(50).ticks(),
            );
            match result {
                Ok(()) => return Ok(()),
                Err(error) if attempt == 2 => return Err(error),
                Err(_) => FreeRtos::delay_ms(5),
            }
        }
        unreachable!()
    }
}
