use crate::{battery::BatteryStatus, hid::Key};
use core::fmt::Write;
use embedded_graphics::{
    mono_font::{
        MonoTextStyle,
        ascii::{FONT_6X10, FONT_7X13, FONT_8X13, FONT_10X20},
    },
    pixelcolor::Rgb565,
    prelude::*,
    primitives::{PrimitiveStyle, Rectangle},
    text::{Baseline, Text},
};
use heapless::String;

pub const WIDTH: u32 = 135;
pub const HEIGHT: u32 = 240;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConnectionStatus {
    Pairing,
    Reconnecting,
    Connecting,
    Connected,
    Resetting,
    Error,
}

impl ConnectionStatus {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Pairing => "Pairing...",
            Self::Reconnecting => "Reconnecting...",
            Self::Connecting => "Connecting...",
            Self::Connected => "Connected",
            Self::Resetting => "Reset pairing...",
            Self::Error => "Bluetooth error",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct UiState {
    pub battery: Option<BatteryStatus>,
    pub connection: ConnectionStatus,
    pub last_key: Option<Key>,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            battery: None,
            connection: ConnectionStatus::Pairing,
            last_key: None,
        }
    }
}

pub fn draw<D: DrawTarget<Color = Rgb565>>(target: &mut D, state: UiState) -> Result<(), D::Error> {
    let muted = Rgb565::new(16, 35, 20);
    let blue = Rgb565::new(3, 35, 31);
    target.clear(Rgb565::BLACK)?;
    Text::with_baseline(
        "S3 4-Key",
        Point::new(7, 9),
        MonoTextStyle::new(&FONT_8X13, blue),
        Baseline::Top,
    )
    .draw(target)?;

    let mut battery = String::<32>::new();
    match state.battery {
        Some(b) => {
            let _ = write!(
                battery,
                "BAT ~{}%{}",
                b.percent,
                if b.charging { " +" } else { "" }
            );
        }
        None => {
            let _ = battery.push_str("BAT --");
        }
    }
    Text::with_baseline(
        &battery,
        Point::new(7, 35),
        MonoTextStyle::new(&FONT_7X13, Rgb565::WHITE),
        Baseline::Top,
    )
    .draw(target)?;
    if let Some(b) = state.battery {
        Rectangle::new(Point::new(7, 54), Size::new(121, 5))
            .into_styled(PrimitiveStyle::with_fill(muted))
            .draw(target)?;
        if b.percent > 0 {
            Rectangle::new(
                Point::new(7, 54),
                Size::new(121 * u32::from(b.percent) / 100, 5),
            )
            .into_styled(PrimitiveStyle::with_fill(if b.percent < 15 {
                Rgb565::RED
            } else {
                blue
            }))
            .draw(target)?;
        }
    }
    let status_color = if state.connection == ConnectionStatus::Connected {
        Rgb565::GREEN
    } else {
        blue
    };
    Text::with_baseline(
        state.connection.label(),
        Point::new(7, 73),
        MonoTextStyle::new(&FONT_7X13, status_color),
        Baseline::Top,
    )
    .draw(target)?;
    Text::with_baseline(
        "LAST KEY",
        Point::new(7, 111),
        MonoTextStyle::new(&FONT_6X10, muted),
        Baseline::Top,
    )
    .draw(target)?;
    let label = state.last_key.map(Key::label).unwrap_or("-");
    let x = (WIDTH as i32 - label.len() as i32 * 10) / 2;
    Text::with_baseline(
        label,
        Point::new(x, 142),
        MonoTextStyle::new(&FONT_10X20, Rgb565::WHITE),
        Baseline::Top,
    )
    .draw(target)?;
    Text::with_baseline(
        "press blue to pair",
        Point::new(7, 207),
        MonoTextStyle::new(&FONT_6X10, blue),
        Baseline::Top,
    )
    .draw(target)?;
    Text::with_baseline(
        "right = backspace",
        Point::new(7, 223),
        MonoTextStyle::new(&FONT_6X10, muted),
        Baseline::Top,
    )
    .draw(target)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    struct BoundsChecked;
    impl OriginDimensions for BoundsChecked {
        fn size(&self) -> Size {
            Size::new(WIDTH, HEIGHT)
        }
    }
    impl DrawTarget for BoundsChecked {
        type Color = Rgb565;
        type Error = Point;
        fn draw_iter<I: IntoIterator<Item = Pixel<Self::Color>>>(
            &mut self,
            pixels: I,
        ) -> Result<(), Point> {
            for Pixel(point, _) in pixels {
                if !self.bounding_box().contains(point) {
                    return Err(point);
                }
            }
            Ok(())
        }
    }

    #[test]
    fn all_labels_and_states_fit_the_actual_sticks3_screen() {
        for connection in [
            ConnectionStatus::Pairing,
            ConnectionStatus::Reconnecting,
            ConnectionStatus::Connecting,
            ConnectionStatus::Connected,
            ConnectionStatus::Resetting,
            ConnectionStatus::Error,
        ] {
            for last_key in [None, Some(Key::A), Some(Key::Backspace)] {
                for battery in [
                    None,
                    BatteryStatus::from_millivolts(4200, true),
                    BatteryStatus::from_millivolts(3300, false),
                ] {
                    draw(
                        &mut BoundsChecked,
                        UiState {
                            battery,
                            connection,
                            last_key,
                        },
                    )
                    .unwrap();
                }
            }
        }
    }
}
