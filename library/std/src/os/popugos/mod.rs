//! PopugOS-specific extensions.

#![stable(feature = "popugos_os_ext", since = "1.100.0")]

#[stable(feature = "popugos_io_ext", since = "1.100.0")]
pub mod io {
    #[stable(feature = "popugos_io_ext", since = "1.100.0")]
    pub use crate::os::fd::*;
}
