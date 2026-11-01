use crate::domain::projects;
use std::{
    fs::{File, OpenOptions},
    io,
    os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
    path::Path,
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, FILE_ATTRIBUTE_DIRECTORY, FILE_ATTRIBUTE_REPARSE_POINT,
    FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ, GetDriveTypeW,
    GetFileInformationByHandle,
};

#[derive(PartialEq, Eq)]
pub(super) struct Identity {
    pub volume: u32,
    pub index: u64,
    pub attributes: u32,
    pub links: u32,
    pub bytes: u64,
    modified: u64,
}

pub(super) fn invalid() -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        "unsupported_recovery_observation",
    )
}

pub(super) fn identity(file: &File) -> io::Result<Identity> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: FFI category8: the borrowed File handle is live and the initialized
    // output is exclusively borrowed. GetFileInformationByHandle retains neither.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(io::Error::last_os_error());
    }
    let index = (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow);
    if index == 0 || info.dwFileAttributes & FILE_ATTRIBUTE_REPARSE_POINT != 0 {
        return Err(invalid());
    }
    Ok(Identity {
        volume: info.dwVolumeSerialNumber,
        index,
        attributes: info.dwFileAttributes,
        links: info.nNumberOfLinks,
        bytes: (u64::from(info.nFileSizeHigh) << 32) | u64::from(info.nFileSizeLow),
        modified: (u64::from(info.ftLastWriteTime.dwHighDateTime) << 32)
            | u64::from(info.ftLastWriteTime.dwLowDateTime),
    })
}

pub(super) fn ancestors(path: &str, available: usize) -> io::Result<Vec<File>> {
    let normalized = projects::normalize_absolute_path(path).map_err(|_| invalid())?;
    let bytes = path.as_bytes();
    if normalized != path
        || bytes.len() < 3
        || !bytes[0].is_ascii_alphabetic()
        || bytes[1] != b':'
        || bytes[2] != b'\\'
    {
        return Err(invalid());
    }
    let drive = [u16::from(bytes[0]), u16::from(b':'), u16::from(b'\\'), 0];
    // SAFETY: FFI category8: initialized NUL-terminated UTF16 input lives through
    // this synchronous call; GetDriveTypeW does not retain the buffer.
    if unsafe { GetDriveTypeW(drive.as_ptr()) } != 3 {
        return Err(invalid());
    }
    let parents: Vec<_> = Path::new(path).ancestors().skip(1).collect();
    if parents.len() > available {
        return Err(invalid());
    }
    let mut guards = Vec::with_capacity(parents.len());
    for parent in parents.into_iter().rev() {
        let file = OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
            .open(parent)?;
        if identity(&file)?.attributes & FILE_ATTRIBUTE_DIRECTORY == 0 {
            return Err(invalid());
        }
        guards.push(file);
    }
    Ok(guards)
}
