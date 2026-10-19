/// Standard eight-byte keyboard report: modifiers, reserved, six key usages.
pub type Report = [u8; 8];
pub const RELEASE_ALL: Report = [0; 8];
pub const REPORT_ID: u8 = 1;

/// HID over GATT uses the Report Reference descriptor for the report ID;
/// the ID is NOT prepended to the eight-byte notification payload.
pub const REPORT_MAP: &[u8] = &[
    0x05, 0x01, // Usage Page (Generic Desktop)
    0x09, 0x06, // Usage (Keyboard)
    0xa1, 0x01, // Collection (Application)
    0x85, REPORT_ID, 0x05, 0x07, // Usage Page (Keyboard)
    0x19, 0xe0, 0x29, 0xe7, // Modifier usages
    0x15, 0x00, 0x25, 0x01, 0x75, 0x01, 0x95, 0x08, 0x81, 0x02, // Eight modifier bits
    0x95, 0x01, 0x75, 0x08, 0x81, 0x01, // Reserved byte
    0x95, 0x05, 0x75, 0x01, 0x05, 0x08, 0x19, 0x01, 0x29, 0x05, 0x91,
    0x02, // Keyboard LEDs (accepted but not displayed)
    0x95, 0x01, 0x75, 0x03, 0x91, 0x01, // LED padding
    0x95, 0x06, 0x75, 0x08, 0x15, 0x00, 0x25, 0x65, 0x05, 0x07, 0x19, 0x00, 0x29, 0x65, 0x81,
    0x00, // Six simultaneous key usages
    0xc0,
];

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Key {
    A,
    S,
    D,
    F,
    Backspace,
}

pub const KEYS: [Key; 5] = [Key::A, Key::S, Key::D, Key::F, Key::Backspace];

impl Key {
    pub const fn usage(self) -> u8 {
        match self {
            Self::A => 0x04,
            Self::S => 0x16,
            Self::D => 0x07,
            Self::F => 0x09,
            Self::Backspace => 0x2a,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::S => "S",
            Self::D => "D",
            Self::F => "F",
            Self::Backspace => "BACKSPACE",
        }
    }
}

pub fn report(pressed: [bool; 5]) -> Report {
    let mut report = RELEASE_ALL;
    let mut slot = 2;
    for (key, down) in KEYS.into_iter().zip(pressed) {
        if down {
            report[slot] = key.usage();
            slot += 1;
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_five_keys_fit_without_rollover_or_shift() {
        assert_eq!(report([true; 5]), [0, 0, 4, 0x16, 7, 9, 0x2a, 0]);
    }

    #[test]
    fn backspace_is_not_forward_delete() {
        assert_eq!(
            report([false, false, false, false, true]),
            [0, 0, 0x2a, 0, 0, 0, 0, 0]
        );
    }

    #[test]
    fn releases_remove_only_the_released_key() {
        assert_eq!(
            report([true, false, false, true, false]),
            [0, 0, 4, 9, 0, 0, 0, 0]
        );
        assert_eq!(report([false; 5]), RELEASE_ALL);
    }
}
