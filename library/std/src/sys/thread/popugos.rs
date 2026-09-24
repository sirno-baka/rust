use crate::ffi::CStr;
use crate::io;
use crate::mem::ManuallyDrop;
use crate::num::NonZero;
use crate::sys::pal::abi::{self, Timespec};
use crate::thread::ThreadInit;
use crate::time::Duration;

pub const DEFAULT_MIN_STACK_SIZE: usize = 128 * 1024;

pub struct Thread {
    tid: i32,
}

unsafe impl Send for Thread {}
unsafe impl Sync for Thread {}

impl Thread {
    // SAFETY: see thread::Builder::spawn_unchecked.
    pub unsafe fn new(_stack: usize, init: Box<ThreadInit>) -> io::Result<Thread> {
        let data = Box::into_raw(init).cast::<u8>();
        let tid = unsafe { abi::thread_create(thread_start, data) };
        if tid < 0 {
            unsafe { drop(Box::from_raw(data.cast::<ThreadInit>())) };
            Err(io::Error::from_raw_os_error(-tid))
        } else {
            Ok(Thread { tid })
        }
    }

    pub fn join(self) {
        let tid = ManuallyDrop::new(self).tid;
        let result = unsafe { abi::thread_join(tid) };
        assert!(result >= 0, "failed to join thread: {}", io::Error::from_raw_os_error(-result));
    }
}

impl Drop for Thread {
    fn drop(&mut self) {
        let result = unsafe { abi::thread_detach(self.tid) };
        debug_assert_eq!(result, 0);
    }
}

extern "C" fn thread_start(data: *mut u8) -> ! {
    let init = unsafe { Box::from_raw(data.cast::<ThreadInit>()) };
    let rust_start = init.init();
    rust_start();

    // PopugOS has key-based TLS; the std runtime owns destructor execution.
    unsafe { crate::sys::thread_local::key::destroy_tls() };
    unsafe { abi::thread_exit(0) }
}

pub fn available_parallelism() -> io::Result<NonZero<usize>> {
    Ok(NonZero::new(1).unwrap())
}

pub fn current_os_id() -> Option<u64> {
    let tid = unsafe { abi::gettid() };
    (tid >= 0).then_some(tid as u64)
}

pub fn set_name(_name: &CStr) {}

pub fn yield_now() {
    unsafe { abi::sched_yield() };
}

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
