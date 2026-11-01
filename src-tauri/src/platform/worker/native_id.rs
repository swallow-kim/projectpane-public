use crate::domain::{Fault, FaultCode, MAX_ENTRY_ID};
use std::ffi::{OsStr, OsString};

pub(super) fn compare(a: &OsStr, b: &OsStr) -> std::cmp::Ordering {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        a.encode_wide().cmp(b.encode_wide())
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        a.as_bytes().cmp(b.as_bytes())
    }
}

pub(super) fn encode(name: &OsStr) -> String {
    use std::fmt::Write;
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let mut id = String::from("w-");
        for unit in name.encode_wide() {
            let _ = write!(id, "{unit:04x}");
        }
        id
    }
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let mut id = String::from("u-");
        for byte in name.as_bytes() {
            let _ = write!(id, "{byte:02x}");
        }
        id
    }
}

pub(super) fn decode(id: &str) -> Result<OsString, Fault> {
    let invalid = || Fault::new(FaultCode::InvalidRequest);
    if id.len() > MAX_ENTRY_ID || id.len() <= 2 {
        return Err(invalid());
    }
    #[cfg(windows)]
    let name = {
        use std::os::windows::ffi::OsStringExt;
        let raw = id.strip_prefix("w-").ok_or_else(invalid)?;
        if !raw.len().is_multiple_of(4) {
            return Err(invalid());
        }
        let units: Result<Vec<u16>, Fault> = raw
            .as_bytes()
            .as_chunks::<4>()
            .0
            .iter()
            .map(|chunk| {
                let text = std::str::from_utf8(chunk).map_err(|_| invalid())?;
                u16::from_str_radix(text, 16).map_err(|_| invalid())
            })
            .collect();
        OsString::from_wide(&units?)
    };
    #[cfg(unix)]
    let name = {
        use std::os::unix::ffi::OsStringExt;
        let raw = id.strip_prefix("u-").ok_or_else(invalid)?;
        if !raw.len().is_multiple_of(2) {
            return Err(invalid());
        }
        let bytes: Result<Vec<u8>, Fault> = raw
            .as_bytes()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|chunk| {
                let text = std::str::from_utf8(chunk).map_err(|_| invalid())?;
                u8::from_str_radix(text, 16).map_err(|_| invalid())
            })
            .collect();
        OsString::from_vec(bytes?)
    };
    if encode(&name) != id {
        return Err(invalid());
    }
    Ok(name)
}
