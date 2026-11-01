use std::io;

#[cfg(windows)]
pub(crate) fn new_id() -> io::Result<String> {
    let mut value = windows_sys::core::GUID::default();
    // SAFETY: FFI category 8: a valid initialized GUID with the SDK repr(C) layout
    // is exclusively borrowed for this synchronous output call; no pointer escapes.
    let result = unsafe { windows_sys::Win32::System::Com::CoCreateGuid(&mut value) };
    if result < 0 {
        return Err(io::Error::other("uuid_unavailable"));
    }
    let tail = value.data4;
    Ok(format!(
        "{:08x}-{:04x}-{:04x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        value.data1,
        value.data2,
        value.data3,
        tail[0],
        tail[1],
        tail[2],
        tail[3],
        tail[4],
        tail[5],
        tail[6],
        tail[7]
    ))
}
#[cfg(not(windows))]
pub(crate) fn new_id() -> io::Result<String> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "windows_uuid_required",
    ))
}
