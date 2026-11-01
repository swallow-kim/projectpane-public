//! Native no-reparse handles. New children are created relative to a pinned
//! directory handle, with FILE_CREATE (never replace an existing name).
use super::{capture_source, identity::recheck};
use crate::domain::{EntryKind, Fault, FaultCode, operations::*};
use std::{
    collections::HashSet,
    fs::{File, OpenOptions},
    mem::size_of,
    os::windows::{
        fs::OpenOptionsExt,
        io::{AsRawHandle, FromRawHandle},
    },
    path::Path,
};
use windows_sys::{
    Wdk::{Foundation::OBJECT_ATTRIBUTES, Storage::FileSystem::*},
    Win32::{
        Foundation::{OBJ_CASE_INSENSITIVE, UNICODE_STRING},
        Storage::FileSystem::*,
        System::IO::IO_STATUS_BLOCK,
    },
};

pub(super) fn info(file: &File) -> Result<BY_HANDLE_FILE_INFORMATION, Fault> {
    let mut info = BY_HANDLE_FILE_INFORMATION::default();
    // SAFETY: borrowed live handle, initialized exclusive output.
    if unsafe { GetFileInformationByHandle(file.as_raw_handle(), &mut info) } == 0 {
        return Err(std::io::Error::last_os_error().into());
    }
    if info.dwFileAttributes & (0x400 | 0x4000) != 0 {
        return Err(Fault::new(FaultCode::InvalidPath)); // reparse / encrypted
    }
    Ok(info)
}
pub(super) fn open(source: &OperationSource, delete: bool) -> Result<File, Fault> {
    let access = FILE_GENERIC_READ | if delete { DELETE } else { 0 };
    // Deny directory metadata/reparse mutations as well as file byte changes.
    // Creating children relative to our live directory handle stays possible.
    let sharing = FILE_SHARE_READ;
    let file = OpenOptions::new()
        .access_mode(access)
        .share_mode(sharing)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&source.path)
        .map_err(Fault::from)?;
    let value = info(&file)?;
    if value.dwVolumeSerialNumber != source.volume
        || (u64::from(value.nFileIndexHigh) << 32 | u64::from(value.nFileIndexLow))
            != source.file_index
        || (value.dwFileAttributes & 0x10 != 0) != (source.kind == EntryKind::Folder)
    {
        return Err(Fault::new(FaultCode::StaleRequest));
    }
    recheck(source)?;
    Ok(file)
}
pub(super) fn ancestors(paths: &[&Path], skip: &HashSet<String>) -> Result<Vec<File>, Fault> {
    let mut seen = skip.clone();
    let mut files = Vec::new();
    for path in paths {
        for ancestor in path.ancestors() {
            let text = ancestor
                .to_str()
                .ok_or_else(|| Fault::new(FaultCode::InvalidPath))?;
            if !seen.insert(text.to_lowercase()) {
                continue;
            }
            if seen.len() > 8192 {
                return Err(Fault::new(FaultCode::ResourceLimit));
            }
            let source = capture_source(text)?;
            files.push(open(&source, false)?);
        }
    }
    Ok(files)
}
pub(super) fn create(parent: &File, name: &str, folder: bool) -> Result<File, Fault> {
    validate_leaf_name(name)?;
    let mut name: Vec<u16> = name.encode_utf16().collect();
    let text = UNICODE_STRING {
        Length: (name.len() * 2) as u16,
        MaximumLength: (name.len() * 2) as u16,
        Buffer: name.as_mut_ptr(),
    };
    let attributes = OBJECT_ATTRIBUTES {
        Length: size_of::<OBJECT_ATTRIBUTES>() as u32,
        RootDirectory: parent.as_raw_handle(),
        ObjectName: &text,
        Attributes: OBJ_CASE_INSENSITIVE,
        ..Default::default()
    };
    let mut io = IO_STATUS_BLOCK::default();
    let mut handle = std::ptr::null_mut();
    // SAFETY: synchronous call; parent and UTF-16 single leaf stay live. FILE_CREATE
    // atomically refuses existing entries. No path resolution below the parent.
    let status = unsafe {
        NtCreateFile(
            &mut handle,
            FILE_GENERIC_READ | if folder { 0 } else { FILE_GENERIC_WRITE },
            &attributes,
            &mut io,
            std::ptr::null(),
            FILE_ATTRIBUTE_NORMAL,
            FILE_SHARE_READ,
            FILE_CREATE,
            FILE_SYNCHRONOUS_IO_NONALERT
                | FILE_OPEN_REPARSE_POINT
                | if folder {
                    FILE_DIRECTORY_FILE
                } else {
                    FILE_NON_DIRECTORY_FILE
                },
            std::ptr::null(),
            0,
        )
    };
    if status < 0 {
        return Err(Fault::new(if status as u32 == 0xc0000035 {
            FaultCode::DestinationExists
        } else {
            FaultCode::IoError
        }));
    }
    // SAFETY: successful NtCreateFile transfers one unique owned handle.
    Ok(unsafe { File::from_raw_handle(handle) })
}
