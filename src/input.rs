use crate::hid::{self, KEYS, Key, Report};

pub const DEBOUNCE_MS: u64 = 12;

#[derive(Clone, Copy, Debug, Default)]
struct Debouncer {
    stable: bool,
    candidate: bool,
    since_ms: u64,
}

impl Debouncer {
    /// Returns a new stable level, not a repeated event for a held button.
    fn update(&mut self, raw: bool, now_ms: u64) -> Option<bool> {
        if raw != self.candidate {
            self.candidate = raw;
            self.since_ms = now_ms;
        }
        if self.candidate != self.stable && now_ms.saturating_sub(self.since_ms) >= DEBOUNCE_MS {
            self.stable = self.candidate;
            Some(self.stable)
        } else {
            None
        }
    }
}

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub struct InputUpdate {
    pub report: Report,
    pub last_pressed: Option<Key>,
    pub pair_requested: bool,
}

pub struct Keyboard {
    keys: [Debouncer; 5],
    blue: Debouncer,
    blocked: [bool; 5],
}

impl Default for Keyboard {
    fn default() -> Self {
        Self::new()
    }
}

impl Keyboard {
    pub const fn new() -> Self {
        const UP: Debouncer = Debouncer {
            stable: false,
            candidate: false,
            since_ms: 0,
        };
        Self {
            keys: [UP; 5],
            blue: UP,
            blocked: [true; 5],
        }
    }

    /// Connection changes and pairing resets must not replay an offline/held key
    /// into a newly connected computer. Fresh presses work after stable release.
    pub fn suppress_until_release(&mut self) {
        self.blocked.fill(true);
    }

    pub fn poll(&mut self, now_ms: u64, raw_keys: [bool; 5], blue: bool) -> InputUpdate {
        let pair_requested = self.blue.update(blue, now_ms) == Some(true);
        if pair_requested {
            self.suppress_until_release();
        }
        let mut pressed = [false; 5];
        let mut last_pressed = None;
        for (i, raw) in raw_keys.into_iter().enumerate() {
            let edge = self.keys[i].update(raw, now_ms);
            if !raw && !self.keys[i].stable {
                self.blocked[i] = false;
            }
            if edge == Some(true) && !self.blocked[i] {
                last_pressed = Some(KEYS[i]);
            }
            pressed[i] = self.keys[i].stable && !self.blocked[i];
        }
        InputUpdate {
            report: hid::report(pressed),
            last_pressed,
            pair_requested,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hid::RELEASE_ALL;

    fn ready() -> Keyboard {
        let mut keyboard = Keyboard::new();
        keyboard.poll(0, [false; 5], false);
        keyboard
    }

    #[test]
    fn contact_bounce_never_emits_a_short_press() {
        let mut k = ready();
        for (time, down) in [(1, true), (4, false), (7, true), (10, false), (20, false)] {
            let update = k.poll(time, [down, false, false, false, false], false);
            assert_eq!(update.report, RELEASE_ALL);
            assert_eq!(update.last_pressed, None);
        }
    }

    #[test]
    fn stable_press_hold_and_release() {
        let mut k = ready();
        let down = [true, false, false, false, false];
        assert_eq!(k.poll(1, down, false).report, RELEASE_ALL);
        assert_eq!(k.poll(12, down, false).report, RELEASE_ALL);
        let update = k.poll(13, down, false);
        assert_eq!(update.last_pressed, Some(Key::A));
        assert_eq!(update.report[2], Key::A.usage());
        assert_eq!(k.poll(500, down, false).last_pressed, None);
        assert_eq!(k.poll(501, [false; 5], false).report[2], 4);
        assert_eq!(k.poll(513, [false; 5], false).report, RELEASE_ALL);
    }

    #[test]
    fn keys_held_at_boot_are_ignored_until_released() {
        let mut k = Keyboard::new();
        assert_eq!(k.poll(0, [true; 5], false).report, RELEASE_ALL);
        assert_eq!(k.poll(20, [true; 5], false).report, RELEASE_ALL);
        k.poll(21, [false; 5], false);
        k.poll(40, [false; 5], false);
        k.poll(41, [true; 5], false);
        assert_eq!(k.poll(60, [true; 5], false).report, hid::report([true; 5]));
    }

    #[test]
    fn reconnect_does_not_replay_a_held_key() {
        let mut k = ready();
        let down = [true, false, false, false, true];
        k.poll(1, down, false);
        k.poll(20, down, false);
        k.suppress_until_release();
        assert_eq!(k.poll(30, down, false).report, RELEASE_ALL);
        k.poll(40, [false; 5], false);
        k.poll(60, [false; 5], false);
        k.poll(70, down, false);
        assert_eq!(k.poll(90, down, false).report, hid::report(down));
    }

    #[test]
    fn reconnect_during_debounce_also_blocks_the_key() {
        let mut k = ready();
        let down = [true, false, false, false, false];
        k.poll(1, down, false);
        k.suppress_until_release();
        assert_eq!(k.poll(20, down, false).report, RELEASE_ALL);
    }

    #[test]
    fn blue_is_debounced_fires_once_and_releases_every_key() {
        let mut k = ready();
        k.poll(1, [true; 5], false);
        k.poll(20, [true; 5], false);
        assert!(!k.poll(21, [true; 5], true).pair_requested);
        let update = k.poll(33, [true; 5], true);
        assert!(update.pair_requested);
        assert_eq!(update.report, RELEASE_ALL);
        assert!(!k.poll(500, [true; 5], true).pair_requested);
        k.poll(501, [false; 5], false);
        k.poll(520, [false; 5], false);
        k.poll(521, [false; 5], true);
        assert!(k.poll(540, [false; 5], true).pair_requested);
    }

    #[test]
    fn pressing_backspace_updates_the_last_key() {
        let mut k = ready();
        let down = [false, false, false, false, true];
        k.poll(1, down, false);
        assert_eq!(k.poll(20, down, false).last_pressed, Some(Key::Backspace));
    }
}
