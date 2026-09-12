use crate::sys::AsInner;
use crate::sys::pal::abi;

pub trait RawTerminalFd {
    fn raw_terminal_fd(&self) -> i32;
}

impl RawTerminalFd for crate::fs::File {
    fn raw_terminal_fd(&self) -> i32 {
        self.as_inner().as_raw_fd()
    }
}

impl RawTerminalFd for crate::io::Stdin {
    fn raw_terminal_fd(&self) -> i32 { 0 }
}

impl RawTerminalFd for crate::io::StdinLock<'_> {
    fn raw_terminal_fd(&self) -> i32 { 0 }
}

impl RawTerminalFd for crate::io::Stdout {
    fn raw_terminal_fd(&self) -> i32 { 1 }
}

impl RawTerminalFd for crate::io::StdoutLock<'_> {
    fn raw_terminal_fd(&self) -> i32 { 1 }
}

impl RawTerminalFd for crate::io::Stderr {
    fn raw_terminal_fd(&self) -> i32 { 2 }
}

impl RawTerminalFd for crate::io::StderrLock<'_> {
    fn raw_terminal_fd(&self) -> i32 { 2 }
}

#[repr(C)]
struct Termios {
    c_iflag: u32,
    c_oflag: u32,
    c_cflag: u32,
    c_lflag: u32,
    c_line: u8,
    c_cc: [u8; 19],
}

pub fn is_terminal<T: RawTerminalFd + ?Sized>(value: &T) -> bool {
    // We don't inspect the returned termios; a successful TCGETS is enough.
    let mut termios = Termios {
        c_iflag: 0,
        c_oflag: 0,
        c_cflag: 0,
        c_lflag: 0,
        c_line: 0,
        c_cc: [0; 19],
    };
    unsafe {
        abi::ioctl(
            value.raw_terminal_fd(),
            abi::TCGETS,
            core::ptr::from_mut(&mut termios).cast(),
        ) >= 0
    }
}
