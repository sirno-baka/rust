use crate::ffi::CStr;
use crate::io;
use crate::num::NonZero;
use crate::sys::pal::abi::{self, Timespec};
use crate::time::Duration;

pub fn available_parallelism() -> io::Result<NonZero<usize>> {
    Ok(NonZero::new(1).unwrap())
}

pub fn current_os_id() -> Option<u64> {
    let pid = unsafe { abi::getpid() };
    (pid >= 0).then_some(pid as u64)
}

pub fn set_name(_name: &CStr) {}

pub fn yield_now() {}

pub fn sleep(duration: Duration) {
    let mut seconds = duration.as_secs();
    let mut nanoseconds = duration.subsec_nanos();
    loop {
        let chunk = seconds.min(i32::MAX as u64);
        let mut request = Timespec { tv_sec: chunk as i32, tv_nsec: nanoseconds as i32 };
        loop {
            let mut remaining = Timespec::default();
            let result = unsafe { abi::nanosleep(&request, &mut remaining) };
            if result >= 0 {
                break;
            }
            if result == -4 {
                request = remaining;
                continue;
            }
            return;
        }
        seconds -= chunk;
        nanoseconds = 0;
        if seconds == 0 {
            return;
        }
    }
}
