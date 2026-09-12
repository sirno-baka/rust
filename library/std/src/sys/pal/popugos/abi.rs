#![allow(dead_code)]

use core::arch::asm;

pub const SYS_EXIT_GROUP: u32 = 252;
pub const SYS_READ: u32 = 3;
pub const SYS_WRITE: u32 = 4;
pub const SYS_OPEN: u32 = 5;
pub const SYS_CLOSE: u32 = 6;
pub const SYS_WAITPID: u32 = 7;
pub const SYS_UNLINK: u32 = 10;
pub const SYS_CHDIR: u32 = 12;
pub const SYS_LSEEK: u32 = 19;
pub const SYS_GETPID: u32 = 20;
pub const SYS_KILL: u32 = 37;
pub const SYS_RENAME: u32 = 38;
pub const SYS_MKDIR: u32 = 39;
pub const SYS_RMDIR: u32 = 40;
pub const SYS_DUP: u32 = 41;
pub const SYS_PIPE: u32 = 42;
pub const SYS_IOCTL: u32 = 54;
pub const SYS_FCNTL: u32 = 55;
pub const SYS_DUP2: u32 = 63;
pub const SYS_MMAP: u32 = 90;
pub const SYS_MUNMAP: u32 = 91;
pub const SYS_NANOSLEEP: u32 = 162;
pub const SYS_POLL: u32 = 168;
pub const SYS_GETCWD: u32 = 183;
pub const SYS_STAT64: u32 = 195;
pub const SYS_FSTAT64: u32 = 197;
pub const SYS_GETDENTS64: u32 = 220;
pub const SYS_CLOCK_GETTIME: u32 = 265;
pub const SYS_GETRANDOM: u32 = 355;
pub const SYS_SOCKET: u32 = 359;
pub const SYS_BIND: u32 = 361;
pub const SYS_CONNECT: u32 = 362;
pub const SYS_LISTEN: u32 = 363;
pub const SYS_ACCEPT4: u32 = 364;
pub const SYS_GETSOCKNAME: u32 = 367;
pub const SYS_GETPEERNAME: u32 = 368;
pub const SYS_SENDTO: u32 = 369;
pub const SYS_RECVFROM: u32 = 371;
pub const SYS_SHUTDOWN: u32 = 373;
pub const SYS_SPAWN: u32 = 0xF000;
pub const SYS_IFCONFIG: u32 = 0xF111;

pub const F_GETFL: u32 = 3;
pub const F_SETFL: u32 = 4;
pub const TCGETS: u32 = 0x5401;
pub const O_NONBLOCK: u32 = 0x800;
pub const WNOHANG: u32 = 1;

pub const CLOCK_REALTIME: i32 = 0;
pub const CLOCK_MONOTONIC: i32 = 1;

#[repr(C)]
pub struct MmapArgs {
    pub addr: u32,
    pub len: u32,
    pub prot: u32,
    pub flags: u32,
    pub fd: i32,
    pub offset: u32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Timespec {
    pub tv_sec: i32,
    pub tv_nsec: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct PollFd {
    pub fd: i32,
    pub events: i16,
    pub revents: i16,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct IfConfig {
    pub mode: u32,
    pub state: u32,
    pub ip: u32,
    pub prefix: u32,
    pub gateway: u32,
    pub dns: u32,
    pub mac: [u8; 6],
    pub pad: [u8; 2],
}

/// Parameters for Felix's private in-memory spawn syscall.
#[repr(C)]
pub struct ExecParams {
    pub stdin: i32,
    pub stdout: i32,
    pub stderr: i32,
    pub argc: u32,
    pub argv: *const *const u8,
    pub envc: u32,
    pub envp: *const *const u8,
    pub pgid: i32,
    pub foreground: u32,
}

#[repr(C, packed)]
#[derive(Clone, Copy, Default)]
pub struct Stat64 {
    pub st_dev: u64,
    pub pad0: [u8; 4],
    pub old_ino: u32,
    pub st_mode: u32,
    pub st_nlink: u32,
    pub st_uid: u32,
    pub st_gid: u32,
    pub st_rdev: u64,
    pub pad3: [u8; 4],
    pub st_size: i64,
    pub st_blksize: u32,
    pub st_blocks: u64,
    pub st_atime: u32,
    pub st_atime_nsec: u32,
    pub st_mtime: u32,
    pub st_mtime_nsec: u32,
    pub st_ctime: u32,
    pub st_ctime_nsec: u32,
    pub st_ino: u64,
}

#[inline]
pub unsafe fn syscall0(number: u32) -> i32 {
    let result: i32;
    unsafe {
        asm!(
            "int 0x80",
            inlateout("eax") number => result,
            options(nostack)
        );
    }
    result
}

#[inline]
pub unsafe fn syscall1(number: u32, arg1: u32) -> i32 {
    let result: i32;
    unsafe {
        asm!(
            "int 0x80",
            inlateout("eax") number => result,
            in("ebx") arg1,
            options(nostack)
        );
    }
    result
}

#[inline]
pub unsafe fn syscall2(number: u32, arg1: u32, arg2: u32) -> i32 {
    let result: i32;
    unsafe {
        asm!(
            "int 0x80",
            inlateout("eax") number => result,
            in("ebx") arg1,
            in("ecx") arg2,
            options(nostack)
        );
    }
    result
}

#[inline]
pub unsafe fn syscall3(number: u32, arg1: u32, arg2: u32, arg3: u32) -> i32 {
    let result: i32;
    unsafe {
        asm!(
            "int 0x80",
            inlateout("eax") number => result,
            in("ebx") arg1,
            in("ecx") arg2,
            in("edx") arg3,
            options(nostack)
        );
    }
    result
}

pub unsafe fn exit(status: i32) -> ! {
    unsafe {
        asm!(
            "int 0x80",
            in("eax") SYS_EXIT_GROUP,
            in("ebx") status,
            options(noreturn)
        );
    }
}

pub unsafe fn mmap(args: &MmapArgs) -> i32 {
    unsafe { syscall1(SYS_MMAP, core::ptr::from_ref(args).addr() as u32) }
}

pub unsafe fn munmap(addr: *mut u8, len: usize) -> i32 {
    unsafe { syscall2(SYS_MUNMAP, addr.addr() as u32, len as u32) }
}

pub unsafe fn read(fd: u32, buf: *mut u8, len: usize) -> i32 {
    unsafe { syscall3(SYS_READ, fd, buf.addr() as u32, len as u32) }
}

pub unsafe fn write(fd: u32, buf: *const u8, len: usize) -> i32 {
    unsafe { syscall3(SYS_WRITE, fd, buf.addr() as u32, len as u32) }
}

pub unsafe fn open(path: *const u8, flags: u32) -> i32 {
    unsafe { syscall2(SYS_OPEN, path.addr() as u32, flags) }
}

pub unsafe fn close(fd: i32) -> i32 {
    unsafe { syscall1(SYS_CLOSE, fd as u32) }
}

pub unsafe fn unlink(path: *const u8) -> i32 {
    unsafe { syscall1(SYS_UNLINK, path.addr() as u32) }
}

pub unsafe fn rename(old: *const u8, new: *const u8) -> i32 {
    unsafe { syscall2(SYS_RENAME, old.addr() as u32, new.addr() as u32) }
}

pub unsafe fn mkdir(path: *const u8) -> i32 {
    unsafe { syscall1(SYS_MKDIR, path.addr() as u32) }
}

pub unsafe fn rmdir(path: *const u8) -> i32 {
    unsafe { syscall1(SYS_RMDIR, path.addr() as u32) }
}

pub unsafe fn duplicate(fd: i32) -> i32 {
    unsafe { syscall1(SYS_DUP, fd as u32) }
}

pub unsafe fn lseek(fd: i32, offset: i32, whence: u32) -> i32 {
    unsafe { syscall3(SYS_LSEEK, fd as u32, offset as u32, whence) }
}

pub unsafe fn stat(path: *const u8, stat: *mut Stat64) -> i32 {
    unsafe { syscall2(SYS_STAT64, path.addr() as u32, stat.addr() as u32) }
}

pub unsafe fn fstat(fd: i32, stat: *mut Stat64) -> i32 {
    unsafe { syscall2(SYS_FSTAT64, fd as u32, stat.addr() as u32) }
}

pub unsafe fn getdents(fd: i32, buffer: *mut u8, len: usize) -> i32 {
    unsafe { syscall3(SYS_GETDENTS64, fd as u32, buffer.addr() as u32, len as u32) }
}

pub unsafe fn clock_gettime(clock: i32, time: *mut Timespec) -> i32 {
    unsafe { syscall2(SYS_CLOCK_GETTIME, clock as u32, time.addr() as u32) }
}

pub unsafe fn nanosleep(request: *const Timespec, remaining: *mut Timespec) -> i32 {
    unsafe { syscall2(SYS_NANOSLEEP, request.addr() as u32, remaining.addr() as u32) }
}

pub unsafe fn poll(fds: *mut PollFd, nfds: usize, timeout_ms: i32) -> i32 {
    unsafe { syscall3(SYS_POLL, fds.addr() as u32, nfds as u32, timeout_ms as u32) }
}

pub unsafe fn getcwd(buffer: *mut u8, len: usize) -> i32 {
    unsafe { syscall2(SYS_GETCWD, buffer.addr() as u32, len as u32) }
}

pub unsafe fn chdir(path: *const u8) -> i32 {
    unsafe { syscall1(SYS_CHDIR, path.addr() as u32) }
}

pub unsafe fn pipe(fds: *mut i32) -> i32 {
    unsafe { syscall1(SYS_PIPE, fds.addr() as u32) }
}

pub unsafe fn dup2(old_fd: i32, new_fd: i32) -> i32 {
    unsafe { syscall2(SYS_DUP2, old_fd as u32, new_fd as u32) }
}

pub unsafe fn fcntl(fd: i32, cmd: u32, arg: u32) -> i32 {
    unsafe { syscall3(SYS_FCNTL, fd as u32, cmd, arg) }
}

pub unsafe fn ioctl(fd: i32, request: u32, arg: *mut u8) -> i32 {
    unsafe { syscall3(SYS_IOCTL, fd as u32, request, arg.addr() as u32) }
}

pub unsafe fn getpid() -> i32 {
    unsafe { syscall0(SYS_GETPID) }
}

pub unsafe fn kill(pid: i32, signal: i32) -> i32 {
    unsafe { syscall2(SYS_KILL, pid as u32, signal as u32) }
}

pub unsafe fn waitpid(pid: i32, status: *mut i32, options: u32) -> i32 {
    unsafe { syscall3(SYS_WAITPID, pid as u32, status.addr() as u32, options) }
}

/// Felix-private spawn ABI: an ELF image and an `ExecParams` pointer.
pub unsafe fn spawn(image: *const u8, len: usize, params: *const ExecParams) -> i32 {
    unsafe {
        syscall3(
            SYS_SPAWN,
            image.addr() as u32,
            len as u32,
            params.addr() as u32,
        )
    }
}

pub unsafe fn getrandom(buffer: *mut u8, len: usize, flags: u32) -> i32 {
    unsafe { syscall3(SYS_GETRANDOM, buffer.addr() as u32, len as u32, flags) }
}

pub unsafe fn ifconfig_get(config: *mut IfConfig) -> i32 {
    unsafe { syscall2(SYS_IFCONFIG, 0, config.addr() as u32) }
}

pub unsafe fn socket(domain: u32, ty: u32, protocol: u32) -> i32 {
    unsafe { syscall3(SYS_SOCKET, domain, ty, protocol) }
}

pub unsafe fn bind(fd: i32, addr: *const u8, addrlen: u32) -> i32 {
    unsafe { syscall3(SYS_BIND, fd as u32, addr.addr() as u32, addrlen) }
}

pub unsafe fn connect(fd: i32, addr: *const u8, addrlen: u32) -> i32 {
    unsafe { syscall3(SYS_CONNECT, fd as u32, addr.addr() as u32, addrlen) }
}

pub unsafe fn listen(fd: i32, backlog: i32) -> i32 {
    unsafe { syscall2(SYS_LISTEN, fd as u32, backlog as u32) }
}

pub unsafe fn accept4(fd: i32, addr: *mut u8, addrlen: *mut u32, _flags: u32) -> i32 {
    unsafe { syscall3(SYS_ACCEPT4, fd as u32, addr.addr() as u32, addrlen.addr() as u32) }
}

pub unsafe fn getsockname(fd: i32, addr: *mut u8, addrlen: *mut u32) -> i32 {
    unsafe { syscall3(SYS_GETSOCKNAME, fd as u32, addr.addr() as u32, addrlen.addr() as u32) }
}

pub unsafe fn getpeername(fd: i32, addr: *mut u8, addrlen: *mut u32) -> i32 {
    unsafe { syscall3(SYS_GETPEERNAME, fd as u32, addr.addr() as u32, addrlen.addr() as u32) }
}

/// Felix currently exposes the connected send/receive form under the direct
/// Linux i386 sendto/recvfrom syscall numbers.
pub unsafe fn send(fd: i32, buffer: *const u8, len: usize) -> i32 {
    unsafe { syscall3(SYS_SENDTO, fd as u32, buffer.addr() as u32, len as u32) }
}

pub unsafe fn recv(fd: i32, buffer: *mut u8, len: usize) -> i32 {
    unsafe { syscall3(SYS_RECVFROM, fd as u32, buffer.addr() as u32, len as u32) }
}

pub unsafe fn shutdown(fd: i32, how: u32) -> i32 {
    unsafe { syscall2(SYS_SHUTDOWN, fd as u32, how) }
}
