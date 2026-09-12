pub use super::common::Args;
use crate::ffi::{CStr, OsString, c_char};
use crate::ptr;
use crate::sync::atomic::{AtomicIsize, AtomicPtr, Ordering};
use crate::sys::FromInner;
use crate::sys::os_str::Buf;

static ARGC: AtomicIsize = AtomicIsize::new(0);
static ARGV: AtomicPtr<*const u8> = AtomicPtr::new(ptr::null_mut());

pub unsafe fn init(argc: isize, argv: *const *const u8) {
    ARGC.store(argc.max(0), Ordering::Relaxed);
    ARGV.store(argv.cast_mut(), Ordering::Relaxed);
}

pub fn args() -> Args {
    let argv = ARGV.load(Ordering::Relaxed);
    if argv.is_null() {
        return Args::new(Vec::new());
    }
    let argc = ARGC.load(Ordering::Relaxed);
    let mut result = Vec::with_capacity(argc as usize);
    for index in 0..argc {
        let argument = unsafe { argv.offset(index).read() };
        if argument.is_null() {
            break;
        }
        let bytes = unsafe { CStr::from_ptr(argument.cast::<c_char>()) }.to_bytes().to_vec();
        result.push(OsString::from_inner(Buf { inner: bytes }));
    }
    Args::new(result)
}
