use crate::domain::{EntryKind, Fault, FaultCode, operations::OperationSource, projects};
use std::{
    fs::{self, OpenOptions},
    os::windows::{
        fs::{MetadataExt, OpenOptionsExt},
        io::AsRawHandle,
    },
    path::Path,
};
use windows_sys::Win32::Storage::FileSystem::{
    BY_HANDLE_FILE_INFORMATION, FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
    FILE_READ_ATTRIBUTES, GetFileInformationByHandle, GetFinalPathNameByHandleW,
};

pub fn capture_source(path: &str) -> Result<OperationSource, Fault> {
    let normalized =
        projects::normalize_absolute_path(path).map_err(|_| Fault::new(FaultCode::InvalidPath))?;
    // UNC (including WSL), mapped and removable volumes use the same native
    // identity and no-reparse checks. Unsupported providers fail closed below.
    for ancestor in Path::new(&normalized).ancestors() {
        let metadata = fs::symlink_metadata(ancestor).map_err(Fault::from)?;
        if metadata.file_attributes() & 0x400 != 0 || metadata.file_type().is_symlink() {
            return Err(Fault::new(FaultCode::InvalidPath));
        }
    }
    let file = OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .share_mode(7)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&normalized)
        .map_err(Fault::from)?;
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    let mut path_buffer = vec![0_u16; 32768];
    // SAFETY: FFI category 8: the handle is borrowed from a live File. Both output
    // buffers are initialized and exclusively borrowed; neither call retains them.
    let (read, length) = unsafe {
        (
            GetFileInformationByHandle(file.as_raw_handle(), &mut info),
            GetFinalPathNameByHandleW(
                file.as_raw_handle(),
                path_buffer.as_mut_ptr(),
                path_buffer.len() as u32,
                0,
            ),
        )
    };
    if read == 0 || length == 0 {
        return Err(Fault::from(std::io::Error::last_os_error()));
    }
    if length as usize >= path_buffer.len() || info.dwFileAttributes & 0x400 != 0 {
        return Err(Fault::new(FaultCode::InvalidPath));
    }
    let canonical = String::from_utf16(&path_buffer[..length as usize])
        .map_err(|_| Fault::new(FaultCode::InvalidPath))?;
    let canonical = canonical_path(&canonical)?;
    if info.nFileIndexHigh == 0 && info.nFileIndexLow == 0 {
        return Err(Fault::new(FaultCode::InvalidPath));
    }
    let kind = if info.dwFileAttributes & 0x10 != 0 {
        EntryKind::Folder
    } else {
        EntryKind::File
    };
    Ok(OperationSource {
        path: canonical,
        kind,
        volume: info.dwVolumeSerialNumber,
        file_index: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
    })
}

pub(super) fn fixed_local(path: &str) -> bool {
    let b = path.as_bytes();
    if b.len() < 3 || b[1] != b':' || b[2] != b'\\' || !b[0].is_ascii_alphabetic() {
        return false;
    }
    let drive = [u16::from(b[0]), 58, 92, 0];
    // SAFETY: initialized NUL-terminated root; no retained pointer.
    unsafe { windows_sys::Win32::Storage::FileSystem::GetDriveTypeW(drive.as_ptr()) == 3 }
}

fn canonical_path(path: &str) -> Result<String, Fault> {
    let path = if let Some(unc) = path.strip_prefix(r"\\?\UNC\") {
        format!(r"\\{unc}")
    } else {
        path.strip_prefix(r"\\?\")
            .ok_or_else(|| Fault::new(FaultCode::InvalidPath))?
            .to_owned()
    };
    projects::normalize_absolute_path(&path).map_err(|_| Fault::new(FaultCode::InvalidPath))
}

pub(super) fn recheck(source: &OperationSource) -> Result<(), Fault> {
    if capture_source(&source.path)? != *source {
        return Err(Fault::new(FaultCode::StaleRequest));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_unc_and_drive_paths_keep_their_namespace() {
        for (raw, expected) in [
            (r"\\?\C:\한글 폴더\a.txt", r"C:\한글 폴더\a.txt"),
            (
                r"\\?\UNC\wsl.localhost\Ubuntu\tmp\a",
                r"\\wsl.localhost\Ubuntu\tmp\a",
            ),
            (r"\\?\UNC\server\share\a", r"\\server\share\a"),
        ] {
            assert_eq!(canonical_path(raw).unwrap(), expected);
        }
        assert!(canonical_path(r"\\?\GLOBALROOT\Device\HarddiskVolume1").is_err());
    }
}
