use crate::sys::pal::abi::{self, Timespec};
use crate::time::Duration;

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct Instant(Duration);

#[derive(Copy, Clone, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub struct SystemTime(Duration);

pub const UNIX_EPOCH: SystemTime = SystemTime(Duration::ZERO);

fn now(clock: i32) -> Duration {
    let mut value = Timespec::default();
    let result = unsafe { abi::clock_gettime(clock, &mut value) };
    if result < 0 || value.tv_sec < 0 || !(0..1_000_000_000).contains(&value.tv_nsec) {
        crate::sys::abort_internal();
    }
    Duration::new(value.tv_sec as u64, value.tv_nsec as u32)
}

impl Instant {
    pub fn now() -> Instant {
        Instant(now(abi::CLOCK_MONOTONIC))
    }

    pub fn checked_sub_instant(&self, other: &Instant) -> Option<Duration> {
        self.0.checked_sub(other.0)
    }

    pub fn checked_add_duration(&self, other: &Duration) -> Option<Instant> {
        Some(Instant(self.0.checked_add(*other)?))
    }

    pub fn checked_sub_duration(&self, other: &Duration) -> Option<Instant> {
        Some(Instant(self.0.checked_sub(*other)?))
    }
}

impl SystemTime {
    pub const MAX: SystemTime = SystemTime(Duration::MAX);
    pub const MIN: SystemTime = SystemTime(Duration::ZERO);

    #[allow(dead_code)]
    pub fn new(seconds: i64, nanoseconds: i64) -> Result<SystemTime, crate::io::Error> {
        let seconds =
            u64::try_from(seconds).map_err(|_| crate::io::Error::from_raw_os_error(22))?;
        let nanoseconds =
            u32::try_from(nanoseconds).map_err(|_| crate::io::Error::from_raw_os_error(22))?;
        if nanoseconds >= 1_000_000_000 {
            return Err(crate::io::Error::from_raw_os_error(22));
        }
        Ok(SystemTime(Duration::new(seconds, nanoseconds)))
    }

    pub fn now() -> SystemTime {
        SystemTime(now(abi::CLOCK_REALTIME))
    }

    pub fn sub_time(&self, other: &SystemTime) -> Result<Duration, Duration> {
        self.0.checked_sub(other.0).ok_or_else(|| other.0 - self.0)
    }

    pub fn checked_add_duration(&self, other: &Duration) -> Option<SystemTime> {
        Some(SystemTime(self.0.checked_add(*other)?))
    }

    pub fn checked_sub_duration(&self, other: &Duration) -> Option<SystemTime> {
        Some(SystemTime(self.0.checked_sub(*other)?))
    }
}
