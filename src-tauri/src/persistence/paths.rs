use super::StateError;
use std::{
    ffi::OsStr,
    fs, io,
    path::{Component, Path, PathBuf},
};

pub fn resolve_data_root(
    default: &Path,
    override_path: Option<&OsStr>,
) -> Result<PathBuf, StateError> {
    let root = override_path.map_or(default, Path::new);
    validate_local(root)?;
    #[cfg(windows)]
    ensure_local_drive(root)?;
    check_ancestors(root)?;
    let directory = root.join("foundation");
    check_ancestors(&directory)?;
    fs::create_dir_all(&directory)?;
    check_ancestors(&directory)?;
    Ok(directory)
}

#[cfg(windows)]
fn ensure_local_drive(path: &Path) -> Result<(), StateError> {
    use std::path::Prefix;
    use windows_sys::Win32::Storage::FileSystem::GetDriveTypeW;

    let drive = match path.components().next() {
        Some(Component::Prefix(prefix)) => match prefix.kind() {
            Prefix::Disk(drive) => drive,
            _ => return Err(StateError::InvalidPath),
        },
        _ => return Err(StateError::InvalidPath),
    };
    let root = [u16::from(drive), u16::from(b':'), u16::from(b'\\'), 0];
    // SAFETY: FFI boundary (#8): root is an initialized, aligned, NUL-terminated
    // UTF-16 drive root. This live stack array remains immutable throughout the
    // synchronous GetDriveTypeW call, which only reads it and retains no pointer.
    let kind = unsafe { GetDriveTypeW(root.as_ptr()) };
    if is_local_drive_type(kind) {
        Ok(())
    } else {
        Err(StateError::InvalidPath)
    }
}

#[cfg(windows)]
const fn is_local_drive_type(kind: u32) -> bool {
    const DRIVE_REMOVABLE: u32 = 2;
    const DRIVE_FIXED: u32 = 3;
    const DRIVE_RAMDISK: u32 = 6;
    matches!(kind, DRIVE_REMOVABLE | DRIVE_FIXED | DRIVE_RAMDISK)
}

fn validate_local(path: &Path) -> Result<(), StateError> {
    let bytes = path.as_os_str().as_encoded_bytes();
    if bytes.is_empty() || bytes.len() > 32768 || bytes.contains(&0) || !path.is_absolute() {
        return Err(StateError::InvalidPath);
    }
    if path
        .components()
        .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(StateError::InvalidPath);
    }
    #[cfg(windows)]
    {
        use std::path::Prefix;
        if !matches!(path.components().next(), Some(Component::Prefix(prefix)) if matches!(prefix.kind(), Prefix::Disk(_)))
        {
            return Err(StateError::InvalidPath);
        }
        for part in path.components() {
            if let Component::Normal(name) = part {
                let name = name.to_string_lossy();
                let stem = name.split('.').next().unwrap_or("").to_ascii_uppercase();
                let reserved = matches!(
                    stem.as_str(),
                    "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
                ) || ["COM", "LPT"].iter().any(|prefix| {
                    stem.strip_prefix(prefix).is_some_and(|suffix| {
                        matches!(
                            suffix,
                            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                        )
                    })
                });
                if reserved
                    || name
                        .chars()
                        .any(|ch| ch < ' ' || ['<', '>', ':', '"', '|', '?', '*'].contains(&ch))
                    || name.ends_with(['.', ' '])
                {
                    return Err(StateError::InvalidPath);
                }
            }
        }
    }
    Ok(())
}

pub(super) fn check_ancestors(path: &Path) -> Result<(), StateError> {
    for ancestor in path.ancestors() {
        match fs::symlink_metadata(ancestor) {
            Ok(metadata) => {
                if metadata.file_type().is_symlink() || is_reparse(&metadata) {
                    return Err(StateError::InvalidPath);
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
    }
    Ok(())
}

#[cfg(windows)]
fn is_reparse(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(not(windows))]
const fn is_reparse(_: &fs::Metadata) -> bool {
    false
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn drive_classification_accepts_only_writable_local_media() {
        for kind in [2, 3, 6] {
            assert!(is_local_drive_type(kind));
        }
        for kind in [0, 1, 4, 5, 7, u32::MAX] {
            assert!(!is_local_drive_type(kind));
        }
    }

    #[test]
    fn real_local_temp_root_passes_os_drive_query() {
        let directory = tempfile::tempdir().unwrap();
        assert!(ensure_local_drive(directory.path()).is_ok());
        assert_eq!(
            resolve_data_root(directory.path(), None).unwrap(),
            directory.path().join("foundation")
        );
    }
}
