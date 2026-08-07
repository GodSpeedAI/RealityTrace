use chrono::{DateTime, Utc};

/// ClockPort per spec §7.1: injectable so parity tests and deterministic
/// envelopes can pin time.
pub trait ClockPort: Send + Sync {
    fn now_utc(&self) -> DateTime<Utc>;
}

/// System clock — the default adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct SystemClock;

impl ClockPort for SystemClock {
    fn now_utc(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Fixed clock for tests and daemon-parity fixtures.
#[derive(Debug, Clone, Copy)]
pub struct FixedClock(pub DateTime<Utc>);

impl ClockPort for FixedClock {
    fn now_utc(&self) -> DateTime<Utc> {
        self.0
    }
}

/// RFC 3339 UTC with microsecond precision, matching the envelope convention
/// used by the DomainForge producer (Slice 0B).
pub fn rfc3339_micros(dt: DateTime<Utc>) -> String {
    dt.to_rfc3339_opts(chrono::SecondsFormat::Micros, true)
}
