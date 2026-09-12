use crate::ffi::OsString;
use crate::io::Result;

pub fn hostname() -> Result<OsString> {
    Ok(crate::env::var_os("HOSTNAME").unwrap_or_else(|| OsString::from("popugos")))
}
