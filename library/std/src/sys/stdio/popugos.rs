use crate::io::{self, BorrowedCursor};
use crate::sys::pal::abi;
use crate::sys::cvt;

pub struct Stdin;
pub struct Stdout;
pub struct Stderr;

impl Stdin {
    pub const fn new() -> Stdin {
        Stdin
    }
}

impl io::Read for Stdin {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        cvt(unsafe { abi::read(0, buf.as_mut_ptr(), buf.len()) }).map(|value| value as usize)
    }

    fn read_buf(&mut self, mut cursor: BorrowedCursor<'_, u8>) -> io::Result<()> {
        let read = cvt(unsafe {
            abi::read(0, cursor.as_mut().as_mut_ptr().cast(), cursor.capacity())
        })?;
        unsafe { cursor.advance(read as usize) };
        Ok(())
    }
}

impl Stdout {
    pub const fn new() -> Stdout {
        Stdout
    }
}

impl io::Write for Stdout {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        cvt(unsafe { abi::write(1, buf.as_ptr(), buf.len()) }).map(|value| value as usize)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl Stderr {
    pub const fn new() -> Stderr {
        Stderr
    }
}

impl io::Write for Stderr {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        cvt(unsafe { abi::write(2, buf.as_ptr(), buf.len()) }).map(|value| value as usize)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

pub fn is_ebadf(error: &io::Error) -> bool {
    error.raw_os_error() == Some(9)
}

pub const STDIN_BUF_SIZE: usize = crate::sys::io::DEFAULT_BUF_SIZE;

pub fn panic_output() -> Option<impl io::Write> {
    Some(Stderr::new())
}
