use core::convert::Infallible;
use embedded_graphics::{pixelcolor::Rgb565, prelude::*};
use esp_idf_svc::{
    hal::{
        delay::FreeRtos,
        gpio::{Output, PinDriver},
        spi::SpiSingleDeviceDriver,
    },
    sys::EspError,
};
use s3_4_key_core::ui::{self, HEIGHT, UiState, WIDTH};

/// ST7789P3, portrait 135x240, window offset (52,40), RGB565, inverted.
/// Pins and panel setup match the official M5GFX StickS3 profile.
pub struct Display {
    spi: SpiSingleDeviceDriver<'static>,
    dc: PinDriver<'static, Output>,
    reset: PinDriver<'static, Output>,
    backlight: PinDriver<'static, Output>,
    frame: FrameBuffer,
}

impl Display {
    pub fn new(
        spi: SpiSingleDeviceDriver<'static>,
        dc: PinDriver<'static, Output>,
        reset: PinDriver<'static, Output>,
        backlight: PinDriver<'static, Output>,
    ) -> Result<Self, EspError> {
        let mut display = Self {
            spi,
            dc,
            reset,
            backlight,
            frame: FrameBuffer(vec![0; (WIDTH * HEIGHT * 2) as usize]),
        };
        display.backlight.set_low()?;
        display.reset.set_high()?;
        FreeRtos::delay_ms(10);
        display.reset.set_low()?;
        FreeRtos::delay_ms(20);
        display.reset.set_high()?;
        FreeRtos::delay_ms(120);
        display.command(0x01, &[])?; // Software reset
        FreeRtos::delay_ms(150);
        // ST7789 initialization values from M5GFX/LovyanGFX's panel profile.
        for (command, data) in [
            (0x36, &[0x00][..]), // MADCTL: portrait, RGB
            (0x3a, &[0x55][..]), // RGB565
            (0xb7, &[0x35][..]),
            (0xbb, &[0x28][..]),
            (0xc0, &[0x0c][..]),
            (0xc2, &[0x01, 0xff][..]),
            (0xc3, &[0x10][..]),
            (0xc4, &[0x20][..]),
            (0xd0, &[0xa4, 0xa1][..]),
            (0xb0, &[0x00, 0xc0][..]),
            (
                0xe0,
                &[
                    0xd0, 0x00, 0x02, 0x07, 0x0a, 0x28, 0x32, 0x44, 0x42, 0x06, 0x0e, 0x12, 0x14,
                    0x17,
                ][..],
            ),
            (
                0xe1,
                &[
                    0xd0, 0x00, 0x02, 0x07, 0x0a, 0x28, 0x31, 0x54, 0x47, 0x0e, 0x1c, 0x17, 0x1b,
                    0x1e,
                ][..],
            ),
        ] {
            display.command(command, data)?;
        }
        display.command(0x21, &[])?; // Inversion on (required by this panel)
        display.command(0x11, &[])?; // Sleep out
        FreeRtos::delay_ms(130);
        display.command(0x38, &[])?; // Idle mode off
        display.command(0x13, &[])?; // Normal display mode
        display.command(0x29, &[])?; // Display on
        display.render(UiState::default())?;
        display.backlight.set_high()?;
        Ok(display)
    }

    fn command(&mut self, command: u8, data: &[u8]) -> Result<(), EspError> {
        self.dc.set_low()?;
        self.spi.write(&[command])?;
        if !data.is_empty() {
            self.dc.set_high()?;
            self.spi.write(data)?;
        }
        Ok(())
    }

    pub fn render(&mut self, state: UiState) -> Result<(), EspError> {
        let Ok(()) = ui::draw(&mut self.frame, state);
        self.command(0x2a, &[0, 52, 0, 186])?; // Columns 52..186
        self.command(0x2b, &[0, 40, 1, 23])?; // Rows 40..279
        self.command(0x2c, &[])?;
        self.dc.set_high()?;
        // Non-DMA writes and small chunks avoid requiring PSRAM to be DMA-capable.
        for chunk in self.frame.0.chunks(4096) {
            self.spi.write(chunk)?;
        }
        Ok(())
    }
}

struct FrameBuffer(Vec<u8>);
impl OriginDimensions for FrameBuffer {
    fn size(&self) -> Size {
        Size::new(WIDTH, HEIGHT)
    }
}
impl DrawTarget for FrameBuffer {
    type Color = Rgb565;
    type Error = Infallible;
    fn draw_iter<I: IntoIterator<Item = Pixel<Rgb565>>>(
        &mut self,
        pixels: I,
    ) -> Result<(), Infallible> {
        for Pixel(Point { x, y }, color) in pixels {
            if x >= 0 && y >= 0 && x < WIDTH as i32 && y < HEIGHT as i32 {
                let offset = 2 * (y as usize * WIDTH as usize + x as usize);
                self.0[offset..offset + 2].copy_from_slice(&color.into_storage().to_be_bytes());
            }
        }
        Ok(())
    }
    fn clear(&mut self, color: Rgb565) -> Result<(), Infallible> {
        let color = color.into_storage().to_be_bytes();
        for pixel in self.0.chunks_exact_mut(2) {
            pixel.copy_from_slice(&color);
        }
        Ok(())
    }
}
