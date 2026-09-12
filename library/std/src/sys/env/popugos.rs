pub use super::common::Env;
use crate::collections::HashMap;
use crate::ffi::{CStr, OsStr, OsString, c_char};
use crate::io;
use crate::sync::Mutex;
use crate::sys::os_str::Buf;
use crate::sys::{AsInner, FromInner};

static ENV: Mutex<Option<HashMap<OsString, OsString>>> = Mutex::new(None);

pub unsafe fn init(envp: *const *const u8) {
    let mut guard = ENV.lock().unwrap();
    let map = guard.insert(HashMap::new());
    if envp.is_null() {
        return;
    }
    let mut cursor = envp;
    loop {
        let entry = unsafe { cursor.read() };
        if entry.is_null() {
            break;
        }
        let bytes = unsafe { CStr::from_ptr(entry.cast::<c_char>()) }.to_bytes();
        if let Some(separator) = bytes.iter().position(|byte| *byte == b'=')
            && separator != 0
        {
            let key = OsString::from_inner(Buf { inner: bytes[..separator].to_vec() });
            let value = OsString::from_inner(Buf { inner: bytes[separator + 1..].to_vec() });
            map.insert(key, value);
        }
        cursor = unsafe { cursor.add(1) };
    }
}

pub fn env() -> Env {
    let guard = ENV.lock().unwrap();
    let values = guard.as_ref().into_iter().flat_map(HashMap::iter).map(|(key, value)| {
        (key.clone(), value.clone())
    });
    Env::new(values.collect())
}

pub fn getenv(key: &OsStr) -> Option<OsString> {
    ENV.lock().unwrap().as_ref()?.get(key).cloned()
}

pub unsafe fn setenv(key: &OsStr, value: &OsStr) -> io::Result<()> {
    let mut guard = ENV.lock().unwrap();
    let map = guard.get_or_insert_with(HashMap::new);
    let key = OsString::from_inner(Buf { inner: key.as_inner().inner.to_vec() });
    let value = OsString::from_inner(Buf { inner: value.as_inner().inner.to_vec() });
    map.insert(key, value);
    Ok(())
}

pub unsafe fn unsetenv(key: &OsStr) -> io::Result<()> {
    if let Some(map) = ENV.lock().unwrap().as_mut() {
        map.remove(key);
    }
    Ok(())
}
