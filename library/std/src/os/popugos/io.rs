//! Raw file descriptors for PopugOS networking types.

#![stable(feature = "popugos_io_ext", since = "1.100.0")]

use crate::sys::{AsInner, FromInner, IntoInner};
use crate::{net, sys};
use crate::marker::PhantomData;

#[stable(feature = "popugos_io_ext", since = "1.100.0")]
pub type RawFd = i32;

#[stable(feature = "popugos_io_ext", since = "1.100.0")]
pub trait AsRawFd {
    #[stable(feature = "popugos_io_ext", since = "1.100.0")]
    fn as_raw_fd(&self) -> RawFd;
}

#[stable(feature = "popugos_io_ext", since = "1.100.0")]
pub trait FromRawFd {
    #[stable(feature = "popugos_io_ext", since = "1.100.0")]
    unsafe fn from_raw_fd(fd: RawFd) -> Self;
}

#[stable(feature = "popugos_io_ext", since = "1.100.0")]
pub trait IntoRawFd {
    #[stable(feature = "popugos_io_ext", since = "1.100.0")]
    fn into_raw_fd(self) -> RawFd;
}

#[derive(Clone, Copy)]
#[repr(transparent)]
#[stable(feature = "popugos_io_ext", since = "1.100.0")]
pub struct BorrowedFd<'fd> {
    fd: RawFd,
    _lifetime: PhantomData<&'fd RawFd>,
}

impl BorrowedFd<'_> {
    #[stable(feature = "popugos_io_ext", since = "1.100.0")]
    pub unsafe fn borrow_raw(fd: RawFd) -> Self {
        Self { fd, _lifetime: PhantomData }
    }
}

#[stable(feature = "popugos_io_ext", since = "1.100.0")]
impl AsRawFd for BorrowedFd<'_> {
    fn as_raw_fd(&self) -> RawFd { self.fd }
}

#[stable(feature = "popugos_io_ext", since = "1.100.0")]
pub trait AsFd {
    #[stable(feature = "popugos_io_ext", since = "1.100.0")]
    fn as_fd(&self) -> BorrowedFd<'_>;
}

#[stable(feature = "popugos_io_ext", since = "1.100.0")]
pub struct OwnedFd { fd: RawFd }

#[stable(feature = "popugos_io_ext", since = "1.100.0")]
impl AsRawFd for OwnedFd { fn as_raw_fd(&self) -> RawFd { self.fd } }
#[stable(feature = "popugos_io_ext", since = "1.100.0")]
impl AsFd for OwnedFd { fn as_fd(&self) -> BorrowedFd<'_> { unsafe { BorrowedFd::borrow_raw(self.fd) } } }
#[stable(feature = "popugos_io_ext", since = "1.100.0")]
impl FromRawFd for OwnedFd { unsafe fn from_raw_fd(fd: RawFd) -> Self { Self { fd } } }
#[stable(feature = "popugos_io_ext", since = "1.100.0")]
impl IntoRawFd for OwnedFd { fn into_raw_fd(self) -> RawFd { let fd=self.fd; crate::mem::forget(self); fd } }
#[stable(feature = "popugos_io_ext", since = "1.100.0")]
impl Drop for OwnedFd { fn drop(&mut self) { let _ = unsafe { crate::sys::abi::close(self.fd) }; } }

macro_rules! impl_raw_fd {
    ($($ty:ident),+ $(,)?) => {$(
        #[stable(feature = "popugos_io_ext", since = "1.100.0")]
        impl AsRawFd for net::$ty {
            fn as_raw_fd(&self) -> RawFd {
                self.as_inner().as_raw_fd()
            }
        }

        #[stable(feature = "popugos_io_ext", since = "1.100.0")]
        impl AsFd for net::$ty {
            fn as_fd(&self) -> BorrowedFd<'_> {
                unsafe { BorrowedFd::borrow_raw(self.as_raw_fd()) }
            }
        }

        #[stable(feature = "popugos_io_ext", since = "1.100.0")]
        impl FromRawFd for net::$ty {
            unsafe fn from_raw_fd(fd: RawFd) -> Self {
                net::$ty::from_inner(unsafe { sys::net::$ty::from_raw_fd(fd) })
            }
        }

        #[stable(feature = "popugos_io_ext", since = "1.100.0")]
        impl IntoRawFd for net::$ty {
            fn into_raw_fd(self) -> RawFd {
                self.into_inner().into_raw_fd()
            }
        }
    )+};
}

impl_raw_fd!(TcpStream, TcpListener, UdpSocket);
