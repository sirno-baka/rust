#![allow(dead_code)]

use crate::fmt;
use crate::sync::atomic::{AtomicI32, Ordering};
use crate::{io, write};

static ERRNO: AtomicI32 = AtomicI32::new(0);

pub fn errno() -> i32 {
    ERRNO.load(Ordering::Relaxed)
}

pub fn set_errno(errno: i32) {
    ERRNO.store(errno, Ordering::Relaxed);
}

pub fn is_interrupted(errno: i32) -> bool {
    errno == 4
}

pub fn decode_error_kind(errno: i32) -> io::ErrorKind {
    match errno {
        1 | 13 => io::ErrorKind::PermissionDenied,
        2 => io::ErrorKind::NotFound,
        4 => io::ErrorKind::Interrupted,
        9 => io::ErrorKind::InvalidInput,
        11 => io::ErrorKind::WouldBlock,
        12 => io::ErrorKind::OutOfMemory,
        17 => io::ErrorKind::AlreadyExists,
        18 => io::ErrorKind::CrossesDevices,
        20 => io::ErrorKind::NotADirectory,
        21 => io::ErrorKind::IsADirectory,
        22 => io::ErrorKind::InvalidInput,
        28 => io::ErrorKind::StorageFull,
        32 => io::ErrorKind::BrokenPipe,
        34 => io::ErrorKind::InvalidInput,
        38 => io::ErrorKind::Unsupported,
        39 => io::ErrorKind::DirectoryNotEmpty,
        95 | 97 => io::ErrorKind::Unsupported,
        98 => io::ErrorKind::AddrInUse,
        99 => io::ErrorKind::AddrNotAvailable,
        103 => io::ErrorKind::ConnectionAborted,
        104 => io::ErrorKind::ConnectionReset,
        107 => io::ErrorKind::NotConnected,
        110 => io::ErrorKind::TimedOut,
        111 => io::ErrorKind::ConnectionRefused,
        115 => io::ErrorKind::WouldBlock,
        _ => io::ErrorKind::Uncategorized,
    }
}

pub fn format_error(errno: i32, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    write!(f, "PopugOS error {errno}")
}
