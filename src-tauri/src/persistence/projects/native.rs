use std::{io, path::Path};

// Win32 contract: https://learn.microsoft.com/windows/win32/api/winbase/nf-winbase-movefileexw
#[cfg(windows)]
pub(super) fn replace(source: &Path, destination: &Path) -> io::Result<()> {
    use std::{os::windows::ffi::OsStrExt, thread, time::Duration};
    use windows_sys::Win32::Storage::FileSystem::{
        MOVEFILE_REPLACE_EXISTING, MOVEFILE_WRITE_THROUGH, MoveFileExW,
    };
    let source: Vec<u16> = source.as_os_str().encode_wide().chain(Some(0)).collect();
    let destination: Vec<u16> = destination
        .as_os_str()
        .encode_wide()
        .chain(Some(0))
        .collect();
    if source[..source.len() - 1].contains(&0) || destination[..destination.len() - 1].contains(&0)
    {
        return Err(io::Error::new(io::ErrorKind::InvalidInput, "path_nul"));
    }
    for attempt in 0..4 {
        // SAFETY: FFI category 8: both buffers are initialized, aligned, NUL-terminated
        // UTF-16 and live immutably throughout this synchronous call. No pointer is
        // retained. No COPY_ALLOWED flag: sibling replacement must stay on-volume.
        let result = unsafe {
            MoveFileExW(
                source.as_ptr(),
                destination.as_ptr(),
                MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
            )
        };
        if result != 0 {
            return Ok(());
        }
        let error = io::Error::last_os_error();
        if attempt == 3 || !matches!(error.raw_os_error(), Some(5 | 32 | 33)) {
            return Err(error);
        }
        thread::sleep(Duration::from_millis(15 * (attempt + 1)));
    }
    Err(io::Error::other("replacement_failed"))
}

#[cfg(not(windows))]
pub(super) fn replace(source: &Path, destination: &Path) -> io::Result<()> {
    std::fs::rename(source, destination)
}
