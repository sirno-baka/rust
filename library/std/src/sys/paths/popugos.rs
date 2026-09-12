use crate::ffi::{OsStr, OsString};
use crate::path::{self, PathBuf};
use crate::sys::os_str::Buf;
use crate::sys::pal::abi;
use crate::sys::{AsInner, FromInner, cvt};
use crate::{fmt, io};

const PATH_SEPARATOR: u8 = b':';

pub struct SplitPaths<'a> {
    bytes: &'a [u8],
    pos: usize,
    done: bool,
}

impl<'a> Iterator for SplitPaths<'a> {
    type Item = PathBuf;

    fn next(&mut self) -> Option<PathBuf> {
        if self.done {
            return None;
        }
        let rest = &self.bytes[self.pos..];
        let len = rest.iter().position(|&b| b == PATH_SEPARATOR).unwrap_or(rest.len());
        let part = &rest[..len];
        self.pos += len;
        if self.pos < self.bytes.len() {
            self.pos += 1;
        } else {
            self.done = true;
        }
        let os = OsString::from_inner(Buf { inner: part.to_vec() });
        Some(PathBuf::from(os))
    }
}

pub fn split_paths(unparsed: &OsStr) -> SplitPaths<'_> {
    SplitPaths {
        bytes: &unparsed.as_inner().inner,
        pos: 0,
        done: false,
    }
}

#[derive(Debug)]
pub struct JoinPathsError;

pub fn join_paths<I, T>(paths: I) -> Result<OsString, JoinPathsError>
where
    I: Iterator<Item = T>,
    T: AsRef<OsStr>,
{
    let mut joined = Vec::new();
    for (index, path) in paths.enumerate() {
        let bytes = &path.as_ref().as_inner().inner;
        if bytes.contains(&PATH_SEPARATOR) {
            return Err(JoinPathsError);
        }
        if index != 0 {
            joined.push(PATH_SEPARATOR);
        }
        joined.extend_from_slice(bytes);
    }
    Ok(OsString::from_inner(Buf { inner: joined }))
}

impl fmt::Display for JoinPathsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "path segment contains separator `:`")
    }
}

impl crate::error::Error for JoinPathsError {}

pub fn getcwd() -> io::Result<PathBuf> {
    let mut capacity = 256usize;
    loop {
        let mut buffer = vec![0u8; capacity];
        let result = unsafe { abi::getcwd(buffer.as_mut_ptr(), buffer.len()) };
        if result >= 0 {
            let mut len = result as usize;
            if len > buffer.len() {
                return Err(io::Error::from_raw_os_error(34));
            }
            if len == 0 {
                // Some Felix versions return 0 on success instead of the Linux
                // getcwd byte count. Find the NUL written into the buffer.
                len = buffer.iter().position(|&b| b == 0).unwrap_or(buffer.len());
            } else if buffer[len.saturating_sub(1)] == 0 {
                // Linux includes the trailing NUL in the returned length.
                len = len.saturating_sub(1);
            }
            buffer.truncate(len);
            return Ok(PathBuf::from(OsString::from_inner(Buf { inner: buffer })));
        }

        let error = -result;
        // ERANGE: grow and retry.
        if error != 34 {
            return Err(io::Error::from_raw_os_error(error));
        }
        capacity = capacity.checked_mul(2).ok_or_else(|| io::Error::from_raw_os_error(12))?;
    }
}

pub fn chdir(path: &path::Path) -> io::Result<()> {
    let bytes = &path.as_os_str().as_inner().inner;
    if bytes.contains(&0) {
        return Err(io::Error::from_raw_os_error(22));
    }
    let mut cpath = Vec::with_capacity(bytes.len() + 1);
    cpath.extend_from_slice(bytes);
    cpath.push(0);
    cvt(unsafe { abi::chdir(cpath.as_ptr()) }).map(drop)
}

pub fn current_exe() -> io::Result<PathBuf> {
    let arg0 = crate::env::args_os().next().ok_or_else(|| {
        io::const_error!(io::ErrorKind::NotFound, "argv[0] is unavailable")
    })?;
    let path = PathBuf::from(arg0);
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(getcwd()?.join(path))
    }
}

pub fn temp_dir() -> PathBuf {
    crate::env::var_os("TMPDIR").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/tmp"))
}

pub fn home_dir() -> Option<PathBuf> {
    crate::env::var_os("HOME").map(PathBuf::from)
}
