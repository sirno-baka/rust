use crate::ffi::OsString;
use crate::fmt;
use crate::fs::TryLockError;
use crate::hash::{Hash, Hasher};
use crate::io::{self, BorrowedCursor, IoSlice, IoSliceMut, SeekFrom};
use crate::path::{Component, Path, PathBuf};
use crate::sys::os_str::Buf;
use crate::sys::pal::abi::{self, Stat64};
use crate::sys::time::SystemTime;
use crate::sys::{AsInner, FromInner, cvt, unsupported, unsupported_err};

pub use crate::sys::fs::common::Dir;

const O_RDONLY: u32 = 0;
const O_WRONLY: u32 = 1;
const O_RDWR: u32 = 2;
const O_CREAT: u32 = 0x40;
const O_TRUNC: u32 = 0x200;
const O_APPEND: u32 = 0x400;
const SEEK_SET: u32 = 0;
const SEEK_CUR: u32 = 1;
const SEEK_END: u32 = 2;
const S_IFMT: u32 = 0o170000;
const S_IFDIR: u32 = 0o040000;
const S_IFREG: u32 = 0o100000;

fn with_c_path<T>(path: &Path, f: impl FnOnce(*const u8) -> io::Result<T>) -> io::Result<T> {
    let bytes = &path.as_os_str().as_inner().inner;
    if bytes.contains(&0) {
        return Err(io::Error::from_raw_os_error(22));
    }
    let mut value = Vec::with_capacity(bytes.len() + 1);
    value.extend_from_slice(bytes);
    value.push(0);
    f(value.as_ptr())
}

#[derive(Debug)]
struct FileDesc(i32);

impl Drop for FileDesc {
    fn drop(&mut self) {
        let _ = unsafe { abi::close(self.0) };
    }
}

#[derive(Debug)]
pub struct File(FileDesc);

#[derive(Clone)]
pub struct FileAttr(Stat64);

pub struct ReadDir {
    root: PathBuf,
    fd: FileDesc,
    buffer: Box<[u8]>,
    position: usize,
    filled: usize,
}

pub struct DirEntry {
    root: PathBuf,
    name: OsString,
    file_type: FileType,
}

#[derive(Clone, Debug)]
pub struct OpenOptions {
    read: bool,
    write: bool,
    append: bool,
    truncate: bool,
    create: bool,
    create_new: bool,
}

#[derive(Copy, Clone, Debug, Default)]
pub struct FileTimes {}

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct FilePermissions {
    mode: u32,
}

#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub struct FileType {
    mode: u32,
}

impl Hash for FileType {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.mode.hash(state);
    }
}

#[derive(Debug)]
pub struct DirBuilder {}

impl FileAttr {
    pub fn size(&self) -> u64 {
        self.0.st_size.max(0) as u64
    }

    pub fn perm(&self) -> FilePermissions {
        FilePermissions { mode: self.0.st_mode }
    }

    pub fn file_type(&self) -> FileType {
        FileType { mode: self.0.st_mode & S_IFMT }
    }

    pub fn modified(&self) -> io::Result<SystemTime> {
        SystemTime::new(self.0.st_mtime.into(), self.0.st_mtime_nsec.into())
    }

    pub fn accessed(&self) -> io::Result<SystemTime> {
        SystemTime::new(self.0.st_atime.into(), self.0.st_atime_nsec.into())
    }

    pub fn created(&self) -> io::Result<SystemTime> {
        SystemTime::new(self.0.st_ctime.into(), self.0.st_ctime_nsec.into())
    }
}

impl FilePermissions {
    pub fn readonly(&self) -> bool {
        self.mode & 0o222 == 0
    }

    pub fn set_readonly(&mut self, readonly: bool) {
        if readonly {
            self.mode &= !0o222;
        } else {
            self.mode |= 0o200;
        }
    }
}

impl FileTimes {
    pub fn set_accessed(&mut self, _time: SystemTime) {}
    pub fn set_modified(&mut self, _time: SystemTime) {}
}

impl FileType {
    pub fn is_dir(&self) -> bool {
        self.mode == S_IFDIR
    }

    pub fn is_file(&self) -> bool {
        self.mode == S_IFREG
    }

    pub fn is_symlink(&self) -> bool {
        false
    }
}

impl fmt::Debug for ReadDir {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.root.fmt(f)
    }
}

impl Iterator for ReadDir {
    type Item = io::Result<DirEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.position >= self.filled {
                let result = cvt(unsafe {
                    abi::getdents(self.fd.0, self.buffer.as_mut_ptr(), self.buffer.len())
                });
                self.position = 0;
                self.filled = match result {
                    Ok(0) => return None,
                    Ok(value) => value as usize,
                    Err(error) => return Some(Err(error)),
                };
            }
            if self.filled - self.position < 20 {
                self.position = self.filled;
                return Some(Err(io::Error::from_raw_os_error(22)));
            }
            let entry = unsafe { self.buffer.as_ptr().add(self.position) };
            let record_len = unsafe { entry.add(16).cast::<u16>().read_unaligned() } as usize;
            if record_len < 20 || record_len > self.filled - self.position {
                self.position = self.filled;
                return Some(Err(io::Error::from_raw_os_error(22)));
            }
            let kind = unsafe { entry.add(18).read() };
            let name_region = &self.buffer[self.position + 19..self.position + record_len];
            let name_len = name_region.iter().position(|byte| *byte == 0).unwrap_or(name_region.len());
            let name = &name_region[..name_len];
            self.position += record_len;
            if name == b"." || name == b".." {
                continue;
            }
            return Some(Ok(DirEntry {
                root: self.root.clone(),
                name: OsString::from_inner(Buf { inner: name.to_vec() }),
                file_type: FileType {
                    mode: if kind == 4 { S_IFDIR } else if kind == 8 { S_IFREG } else { 0 },
                },
            }));
        }
    }
}

impl DirEntry {
    pub fn path(&self) -> PathBuf {
        self.root.join(&self.name)
    }

    pub fn file_name(&self) -> OsString {
        self.name.clone()
    }

    pub fn metadata(&self) -> io::Result<FileAttr> {
        stat(&self.path())
    }

    pub fn file_type(&self) -> io::Result<FileType> {
        Ok(self.file_type)
    }
}

impl OpenOptions {
    pub fn new() -> OpenOptions {
        OpenOptions {
            read: false,
            write: false,
            append: false,
            truncate: false,
            create: false,
            create_new: false,
        }
    }

    pub fn read(&mut self, value: bool) { self.read = value; }
    pub fn write(&mut self, value: bool) { self.write = value; }
    pub fn append(&mut self, value: bool) { self.append = value; }
    pub fn truncate(&mut self, value: bool) { self.truncate = value; }
    pub fn create(&mut self, value: bool) { self.create = value; }
    pub fn create_new(&mut self, value: bool) { self.create_new = value; }

    fn flags(&self) -> io::Result<u32> {
        let access = match (self.read, self.write, self.append) {
            (true, false, false) => O_RDONLY,
            (false, true, false) => O_WRONLY,
            (true, true, false) => O_RDWR,
            (false, _, true) => O_WRONLY | O_APPEND,
            (true, _, true) => O_RDWR | O_APPEND,
            _ => return Err(io::Error::from_raw_os_error(22)),
        };
        if !self.write && !self.append && (self.create || self.create_new || self.truncate) {
            return Err(io::Error::from_raw_os_error(22));
        }
        let creation = if self.create || self.create_new { O_CREAT } else { 0 }
            | if self.truncate { O_TRUNC } else { 0 };
        Ok(access | creation)
    }
}

impl File {
    pub(crate) fn as_raw_fd(&self) -> i32 {
        self.0.0
    }

    pub fn open(path: &Path, options: &OpenOptions) -> io::Result<File> {
        if options.create_new && exists(path)? {
            return Err(io::Error::from_raw_os_error(17));
        }
        let flags = options.flags()?;
        with_c_path(path, |path| {
            let fd = cvt(unsafe { abi::open(path, flags) })?;
            Ok(File(FileDesc(fd)))
        })
    }

    pub fn file_attr(&self) -> io::Result<FileAttr> {
        let mut value = Stat64::default();
        cvt(unsafe { abi::fstat(self.0.0, &mut value) })?;
        Ok(FileAttr(value))
    }

    pub fn fsync(&self) -> io::Result<()> { unsupported() }
    pub fn datasync(&self) -> io::Result<()> { unsupported() }
    pub fn lock(&self) -> io::Result<()> { unsupported() }
    pub fn lock_shared(&self) -> io::Result<()> { unsupported() }
    pub fn try_lock(&self) -> Result<(), TryLockError> { Err(TryLockError::Error(unsupported_err())) }
    pub fn try_lock_shared(&self) -> Result<(), TryLockError> { Err(TryLockError::Error(unsupported_err())) }
    pub fn unlock(&self) -> io::Result<()> { unsupported() }
    pub fn truncate(&self, _size: u64) -> io::Result<()> { unsupported() }

    pub fn read(&self, buffer: &mut [u8]) -> io::Result<usize> {
        cvt(unsafe { abi::read(self.0.0 as u32, buffer.as_mut_ptr(), buffer.len()) })
            .map(|value| value as usize)
    }

    pub fn read_vectored(&self, buffers: &mut [IoSliceMut<'_>]) -> io::Result<usize> {
        io::default_read_vectored(|buffer| self.read(buffer), buffers)
    }

    pub fn is_read_vectored(&self) -> bool { false }

    pub fn read_buf(&self, mut cursor: BorrowedCursor<'_, u8>) -> io::Result<()> {
        let read = cvt(unsafe {
            abi::read(self.0.0 as u32, cursor.as_mut().as_mut_ptr().cast(), cursor.capacity())
        })?;
        unsafe { cursor.advance(read as usize) };
        Ok(())
    }

    pub fn write(&self, buffer: &[u8]) -> io::Result<usize> {
        cvt(unsafe { abi::write(self.0.0 as u32, buffer.as_ptr(), buffer.len()) })
            .map(|value| value as usize)
    }

    pub fn write_vectored(&self, buffers: &[IoSlice<'_>]) -> io::Result<usize> {
        io::default_write_vectored(|buffer| self.write(buffer), buffers)
    }

    pub fn is_write_vectored(&self) -> bool { false }
    pub fn flush(&self) -> io::Result<()> { Ok(()) }

    pub fn seek(&self, position: SeekFrom) -> io::Result<u64> {
        let (offset, whence) = match position {
            SeekFrom::Start(value) => (i32::try_from(value).map_err(|_| io::Error::from_raw_os_error(22))?, SEEK_SET),
            SeekFrom::End(value) => (i32::try_from(value).map_err(|_| io::Error::from_raw_os_error(22))?, SEEK_END),
            SeekFrom::Current(value) => (i32::try_from(value).map_err(|_| io::Error::from_raw_os_error(22))?, SEEK_CUR),
        };
        cvt(unsafe { abi::lseek(self.0.0, offset, whence) }).map(|value| value as u64)
    }

    pub fn size(&self) -> Option<io::Result<u64>> { Some(self.file_attr().map(|attr| attr.size())) }
    pub fn tell(&self) -> io::Result<u64> { self.seek(SeekFrom::Current(0)) }

    pub fn duplicate(&self) -> io::Result<File> {
        cvt(unsafe { abi::duplicate(self.0.0) }).map(|fd| File(FileDesc(fd)))
    }

    pub fn set_permissions(&self, _permissions: FilePermissions) -> io::Result<()> { unsupported() }
    pub fn set_times(&self, _times: FileTimes) -> io::Result<()> { unsupported() }
}

impl DirBuilder {
    pub fn new() -> DirBuilder { DirBuilder {} }

    pub fn mkdir(&self, path: &Path) -> io::Result<()> {
        with_c_path(path, |path| cvt(unsafe { abi::mkdir(path) }).map(drop))
    }
}

pub fn readdir(path: &Path) -> io::Result<ReadDir> {
    let fd = with_c_path(path, |path| cvt(unsafe { abi::open(path, O_RDONLY) }))?;
    Ok(ReadDir {
        root: path.to_path_buf(),
        fd: FileDesc(fd),
        buffer: vec![0; crate::sys::io::DEFAULT_BUF_SIZE].into_boxed_slice(),
        position: 0,
        filled: 0,
    })
}

pub fn unlink(path: &Path) -> io::Result<()> {
    with_c_path(path, |path| cvt(unsafe { abi::unlink(path) }).map(drop))
}

pub fn rename(old: &Path, new: &Path) -> io::Result<()> {
    with_c_path(old, |old| with_c_path(new, |new| cvt(unsafe { abi::rename(old, new) }).map(drop)))
}

pub fn rmdir(path: &Path) -> io::Result<()> {
    with_c_path(path, |path| cvt(unsafe { abi::rmdir(path) }).map(drop))
}

pub fn stat(path: &Path) -> io::Result<FileAttr> {
    with_c_path(path, |path| {
        let mut value = Stat64::default();
        cvt(unsafe { abi::stat(path, &mut value) })?;
        Ok(FileAttr(value))
    })
}

pub fn lstat(path: &Path) -> io::Result<FileAttr> { stat(path) }
pub fn exists(path: &Path) -> io::Result<bool> { crate::sys::fs::common::exists(path) }
pub fn remove_dir_all(path: &Path) -> io::Result<()> { crate::sys::fs::common::remove_dir_all(path) }
pub fn copy(from: &Path, to: &Path) -> io::Result<u64> { crate::sys::fs::common::copy(from, to) }
pub fn set_perm(_path: &Path, _perm: FilePermissions) -> io::Result<()> { unsupported() }
pub fn set_perm_nofollow(_path: &Path, _perm: FilePermissions) -> io::Result<()> { unsupported() }
pub fn set_times(_path: &Path, _times: FileTimes) -> io::Result<()> { unsupported() }
pub fn set_times_nofollow(_path: &Path, _times: FileTimes) -> io::Result<()> { unsupported() }
pub fn readlink(_path: &Path) -> io::Result<PathBuf> { unsupported() }
pub fn symlink(_original: &Path, _link: &Path) -> io::Result<()> { unsupported() }
pub fn link(_original: &Path, _link: &Path) -> io::Result<()> { unsupported() }
pub fn canonicalize(path: &Path) -> io::Result<PathBuf> {
    // PopugOS currently has no symlink implementation, so canonicalization is
    // purely lexical after verifying that the path exists.
    let _ = stat(path)?;
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        crate::sys::paths::getcwd()?.join(path)
    };

    let mut out = PathBuf::from("/");
    for component in absolute.components() {
        match component {
            Component::Prefix(_) | Component::RootDir => {}
            Component::CurDir => {}
            Component::ParentDir => {
                let _ = out.pop();
            }
            Component::Normal(part) => out.push(part),
        }
    }
    Ok(out)
}
