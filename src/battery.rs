#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct BatteryStatus {
    pub millivolts: u16,
    pub percent: u8,
    pub charging: bool,
}

impl BatteryStatus {
    /// The StickS3 has voltage telemetry, not a coulomb-counting fuel gauge.
    /// This is deliberately an estimate; USB charging can temporarily raise it.
    pub fn from_millivolts(millivolts: u16, charging: bool) -> Option<Self> {
        if !(2500..=4500).contains(&millivolts) {
            return None;
        }
        Some(Self {
            millivolts,
            percent: estimate_percent(millivolts),
            charging,
        })
    }
}

pub fn estimate_percent(mv: u16) -> u8 {
    const CURVE: [(u16, u8); 11] = [
        (3300, 0),
        (3600, 5),
        (3700, 15),
        (3750, 25),
        (3800, 40),
        (3850, 55),
        (3900, 65),
        (3950, 73),
        (4000, 80),
        (4100, 90),
        (4200, 100),
    ];
    for pair in CURVE.windows(2) {
        let [(lo_mv, lo_pc), (hi_mv, hi_pc)] = [pair[0], pair[1]];
        if mv <= lo_mv {
            return lo_pc;
        }
        if mv < hi_mv {
            return lo_pc
                + ((u32::from(mv - lo_mv) * u32::from(hi_pc - lo_pc)) / u32::from(hi_mv - lo_mv))
                    as u8;
        }
    }
    100
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_is_bounded_and_monotonic() {
        let mut previous = 0;
        for mv in 0..=u16::MAX {
            let percent = estimate_percent(mv);
            assert!((previous..=100).contains(&percent));
            previous = percent;
        }
        assert_eq!(estimate_percent(3300), 0);
        assert_eq!(estimate_percent(4200), 100);
    }

    #[test]
    fn disconnected_or_bad_telemetry_is_not_a_fake_empty_battery() {
        assert_eq!(BatteryStatus::from_millivolts(0, true), None);
        assert_eq!(BatteryStatus::from_millivolts(5000, false), None);
        assert_eq!(
            BatteryStatus::from_millivolts(3800, true).unwrap().percent,
            40
        );
    }
}
