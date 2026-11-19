//! UTC clock and the timestamp representation used by envelope builders.

/// A UTC instant in Unix seconds and nanoseconds. Nanoseconds above one billion
/// represent a leap second, as in Chrono; this preserves existing envelope bytes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Timestamp {
    seconds: i64,
    nanoseconds: u32,
}

impl Timestamp {
    /// Construct a timestamp. The extra nanosecond range is valid only at the
    /// end of a minute, where it denotes a leap second. UTC dates outside
    /// Chrono's supported years (-262143 through 262142) are rejected.
    pub fn from_unix_parts(seconds: i64, nanoseconds: u32) -> Option<Self> {
        let utc = time::OffsetDateTime::from_unix_timestamp(seconds).ok()?;
        if !(-262_143..=262_142).contains(&utc.year())
            || nanoseconds >= 2_000_000_000
            || (nanoseconds >= 1_000_000_000 && seconds.rem_euclid(60) != 59)
        {
            return None;
        }
        Some(Self {
            seconds,
            nanoseconds,
        })
    }
}

/// Injectable clock so deterministic envelopes can pin time.
pub trait ClockPort: Send + Sync {
    fn now_utc(&self) -> Timestamp;
}

/// System clock — the default adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl ClockPort for SystemClock {
    fn now_utc(&self) -> Timestamp {
        let now = time::OffsetDateTime::now_utc();
        Timestamp::from_unix_parts(now.unix_timestamp(), now.nanosecond())
            .expect("system clock is representable")
    }
}

/// Fixed clock for tests and daemon-parity fixtures.
#[derive(Debug, Clone, Copy)]
pub struct FixedClock(pub Timestamp);

impl ClockPort for FixedClock {
    fn now_utc(&self) -> Timestamp {
        self.0
    }
}

/// RFC 3339 UTC with exactly six fractional digits, truncated, not rounded.
pub fn rfc3339_micros(dt: Timestamp) -> String {
    let date = time::OffsetDateTime::from_unix_timestamp(dt.seconds)
        .expect("Timestamp constructor validates seconds");
    let year = date.year();
    let year = if (0..=9999).contains(&year) {
        format!("{year:04}")
    } else if year < 0 {
        format!("-{:04}", -year)
    } else {
        format!("+{year}")
    };
    let leap = dt.nanoseconds >= 1_000_000_000;
    format!(
        "{year}-{:02}-{:02}T{:02}:{:02}:{:02}.{:06}Z",
        u8::from(date.month()),
        date.day(),
        date.hour(),
        date.minute(),
        date.second() + u8::from(leap),
        (dt.nanoseconds % 1_000_000_000) / 1_000
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_chrono_microsecond_vectors() {
        for (seconds, nanos, expected) in [
            (0, 0, "1970-01-01T00:00:00.000000Z"),
            (-1, 999_999_999, "1969-12-31T23:59:59.999999Z"),
            (1_483_228_799, 1_123_456_789, "2016-12-31T23:59:60.123456Z"),
            (1_786_104_000, 123_456_789, "2026-08-07T12:00:00.123456Z"),
        ] {
            let timestamp = Timestamp::from_unix_parts(seconds, nanos).unwrap();
            assert_eq!(rfc3339_micros(FixedClock(timestamp).now_utc()), expected);
        }
        assert!(Timestamp::from_unix_parts(0, 1_000_000_000).is_none());
        assert!(Timestamp::from_unix_parts(59, 2_000_000_000).is_none());
        assert!(Timestamp::from_unix_parts(i64::MAX, 0).is_none());
    }

    #[test]
    fn chrono_year_bounds_and_signed_years() {
        fn seconds(year: i32, month: time::Month, day: u8, h: u8, m: u8, s: u8) -> i64 {
            time::Date::from_calendar_date(year, month, day)
                .unwrap()
                .with_hms(h, m, s)
                .unwrap()
                .assume_utc()
                .unix_timestamp()
        }
        let min = seconds(-262_143, time::Month::January, 1, 0, 0, 0);
        let max = seconds(262_142, time::Month::December, 31, 23, 59, 59);
        assert!(Timestamp::from_unix_parts(min - 1, 999_999_999).is_none());
        assert!(Timestamp::from_unix_parts(max + 1, 0).is_none());
        for (sec, nanos, expected) in [
            (min, 0, "-262143-01-01T00:00:00.000000Z"),
            (max, 999_999_999, "+262142-12-31T23:59:59.999999Z"),
            (
                seconds(-1, time::Month::January, 1, 0, 0, 0),
                0,
                "-0001-01-01T00:00:00.000000Z",
            ),
            (
                seconds(0, time::Month::January, 1, 0, 0, 0),
                0,
                "0000-01-01T00:00:00.000000Z",
            ),
            (
                seconds(10_000, time::Month::January, 1, 0, 0, 0),
                0,
                "+10000-01-01T00:00:00.000000Z",
            ),
        ] {
            assert_eq!(
                rfc3339_micros(Timestamp::from_unix_parts(sec, nanos).unwrap()),
                expected
            );
        }
    }
}
