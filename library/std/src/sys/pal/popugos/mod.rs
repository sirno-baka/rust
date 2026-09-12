#![deny(unsafe_op_in_unsafe_fn)]
#![allow(dead_code)]

use crate::io;

pub mod abi;

core::arch::global_asm!(
    ".section .start,\"ax\",@progbits",
    ".global _start",
    ".type _start,@function",
    "_start:",
    "mov eax, dword ptr [esp]",
    "lea ecx, [esp + 4]",
    "and esp, -16",
    "sub esp, 8",
    "push ecx",
    "push eax",
    "call main",
    "add esp, 16",
    "mov ebx, eax",
    "mov eax, 252",
    "int 0x80",
    "ud2",
    ".size _start, .-_start",
);

pub fn unsupported<T>() -> io::Result<T> {
    Err(unsupported_err())
}

pub fn unsupported_err() -> io::Error {
    io::Error::UNSUPPORTED_PLATFORM
}

pub fn abort_internal() -> ! {
    unsafe { abi::exit(134) }
}

pub unsafe fn init(argc: isize, argv: *const *const u8, _sigpipe: u8) {
    unsafe { crate::sys::args::init(argc, argv) };
    let envp = if argv.is_null() {
        core::ptr::null()
    } else {
        unsafe { argv.add(argc.max(0) as usize + 1) }
    };
    unsafe { crate::sys::env::init(envp) };
}

pub unsafe fn cleanup() {}

pub trait IsNegative {
    fn is_negative(&self) -> bool;
    fn error_code(&self) -> i32;
}

impl IsNegative for i32 {
    fn is_negative(&self) -> bool {
        *self < 0
    }

    fn error_code(&self) -> i32 {
        -*self
    }
}

impl IsNegative for isize {
    fn is_negative(&self) -> bool {
        *self < 0
    }

    fn error_code(&self) -> i32 {
        i32::try_from(-*self).unwrap_or(i32::MAX)
    }
}

pub fn cvt<T: IsNegative>(value: T) -> io::Result<T> {
    if value.is_negative() {
        let error = value.error_code();
        crate::sys::io::set_errno(error);
        Err(io::Error::from_raw_os_error(error))
    } else {
        Ok(value)
    }
}

pub fn cvt_r<T, F>(mut f: F) -> io::Result<T>
where
    T: IsNegative,
    F: FnMut() -> T,
{
    loop {
        match cvt(f()) {
            Err(error) if error.is_interrupted() => {}
            result => return result,
        }
    }
}
