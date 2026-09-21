//! The time source of the consent routes.

use time::OffsetDateTime;

/// Supplies the current time to the consent routes and the layer.
///
/// The pure evaluation in `terrace-legal-model` takes the time as an argument. This trait is the
/// one place a server decides where it comes from, so a test can fix it.
pub trait Clock: Send + Sync + 'static {
    /// Returns the current time.
    fn now(&self) -> OffsetDateTime;
}

/// The system clock, in UTC.
#[derive(Debug, Clone, Copy, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> OffsetDateTime {
        OffsetDateTime::now_utc()
    }
}

impl<F> Clock for F
where
    F: Fn() -> OffsetDateTime + Send + Sync + 'static,
{
    fn now(&self) -> OffsetDateTime {
        self()
    }
}
