use crate::fmt;
use crate::io::{self, BorrowedCursor, IoSlice, IoSliceMut};
use crate::sys::pal::abi;
use crate::sys::cvt;

pub struct Pipe {
    fd: i32,
}

impl Pipe {
    pub(crate) unsafe fn from_raw_fd(fd: i32) -> Self {
        Self { fd }
    }

    pub(crate) fn as_raw_fd(&self) -> i32 {
        self.fd
    }

    pub fn try_clone(&self) -> io::Result<Self> {
        let fd = cvt(unsafe { abi::duplicate(self.fd) })?;
        Ok(unsafe { Self::from_raw_fd(fd) })
    }

    pub(crate) fn set_nonblocking(&self, enabled: bool) -> io::Result<()> {
        let flags = cvt(unsafe { abi::fcntl(self.fd, abi::F_GETFL, 0) })? as u32;
        let next = if enabled {
            flags | abi::O_NONBLOCK
        } else {
            flags & !abi::O_NONBLOCK
        };
        cvt(unsafe { abi::fcntl(self.fd, abi::F_SETFL, next) }).map(drop)
    }

    pub fn read(&self, buf: &mut [u8]) -> io::Result<usize> {
        cvt(unsafe { abi::read(self.fd as u32, buf.as_mut_ptr(), buf.len()) })
            .map(|n| n as usize)
    }

    pub fn read_buf(&self, mut cursor: BorrowedCursor<'_, u8>) -> io::Result<()> {
        let n = cvt(unsafe {
            abi::read(
                self.fd as u32,
                cursor.as_mut().as_mut_ptr().cast(),
                cursor.capacity(),
            )
        })?;
        unsafe { cursor.advance(n as usize) };
        Ok(())
    }

    pub fn read_vectored(&self, bufs: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        io::default_read_vectored(|buf| self.read(buf), bufs)
    }

    pub fn is_read_vectored(&self) -> bool {
        false
    }

    pub fn read_to_end(&self, out: &mut Vec<u8>) -> io::Result<usize> {
        let start = out.len();
        let mut buffer = [0u8; 1024];
        loop {
            let n = self.read(&mut buffer)?;
            if n == 0 {
                return Ok(out.len() - start);
            }
            out.extend_from_slice(&buffer[..n]);
        }
    }

    pub fn write(&self, buf: &[u8]) -> io::Result<usize> {
        cvt(unsafe { abi::write(self.fd as u32, buf.as_ptr(), buf.len()) })
            .map(|n| n as usize)
    }

    pub fn write_vectored(&self, bufs: &[IoSlice<'_>]) -> io::Result<usize> {
        io::default_write_vectored(|buf| self.write(buf), bufs)
    }

    pub fn is_write_vectored(&self) -> bool {
        false
    }

    #[allow(dead_code)]
    pub fn diverge(&self) -> ! {
        crate::sys::abort_internal()
    }
}

impl Drop for Pipe {
    fn drop(&mut self) {
        let _ = unsafe { abi::close(self.fd) };
    }
}

impl fmt::Debug for Pipe {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Pipe").field("fd", &self.fd).finish()
    }
}

pub fn pipe() -> io::Result<(Pipe, Pipe)> {
    let mut fds = [-1i32; 2];
    cvt(unsafe { abi::pipe(fds.as_mut_ptr()) })?;
    Ok((
        unsafe { Pipe::from_raw_fd(fds[0]) },
        unsafe { Pipe::from_raw_fd(fds[1]) },
    ))
}
