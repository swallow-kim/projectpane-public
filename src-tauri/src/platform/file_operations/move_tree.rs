//! Bounded local NTFS tree move delegated to Windows Shell, never copy + delete.
//! A snapshot is not a filesystem transaction; changed trees fail closed when
//! observed. Network, links, encryption, hard links and directory ADS are out.
use super::{copy_tree, guarded_io, identity};
use crate::domain::{EntryKind, Fault, FaultCode, operations::*};
use std::{
    fs::{File, OpenOptions},
    os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
};
use windows_sys::Win32::{
    Foundation::{ERROR_HANDLE_EOF, INVALID_HANDLE_VALUE},
    Storage::FileSystem::*,
};

pub(super) fn prepare(
    roots: Vec<OperationSource>,
    destination: OperationSource,
) -> Result<PreparedAction, Fault> {
    let PreparedAction::CopyTree {
        roots,
        descendants,
        destination,
    } = copy_tree::prepare(roots, destination)?
    else {
        unreachable!()
    };
    check(&roots, &descendants, &destination)?;
    Ok(PreparedAction::MoveTree {
        roots,
        descendants,
        destination,
    })
}
pub(super) fn check(
    roots: &[OperationSource],
    descendants: &[OperationSource],
    destination: &OperationSource,
) -> Result<(), Fault> {
    copy_tree::check(roots, descendants, destination)?;
    compatible(destination)?;
    for source in roots.iter().chain(descendants) {
        compatible(source)?;
        if source.volume == destination.volume {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
    }
    Ok(())
}
pub(super) fn compatible(source: &OperationSource) -> Result<(), Fault> {
    if !identity::fixed_local(&source.path) {
        return Err(Fault::new(FaultCode::InvalidPath));
    }
    identity::recheck(source)?;
    let file: File = OpenOptions::new()
        .access_mode(FILE_READ_ATTRIBUTES)
        .share_mode(FILE_SHARE_READ | FILE_SHARE_WRITE | FILE_SHARE_DELETE)
        .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
        .open(&source.path)
        .map_err(Fault::from)?;
    let info = guarded_io::info(&file)?;
    if info.dwFileAttributes
        & (FILE_ATTRIBUTE_ENCRYPTED | FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_OFFLINE)
        != 0
        || (source.kind == EntryKind::File && info.nNumberOfLinks != 1)
    {
        return Err(Fault::new(FaultCode::InvalidPath));
    }
    let mut filesystem = [0u16; 32];
    // SAFETY: borrowed live handle and exclusive initialized output buffers.
    if unsafe {
        GetVolumeInformationByHandleW(
            file.as_raw_handle(),
            std::ptr::null_mut(),
            0,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            filesystem.as_mut_ptr(),
            filesystem.len() as u32,
        )
    } == 0
        || filesystem[..5] != [78, 84, 70, 83, 0]
    {
        return Err(Fault::new(FaultCode::InvalidPath));
    }
    if source.kind == EntryKind::Folder {
        no_directory_streams(&source.path)?;
    }
    Ok(())
}
pub(super) fn no_directory_streams(path: &str) -> Result<(), Fault> {
    let text: Vec<_> = path.encode_utf16().chain([0]).collect();
    let mut data = WIN32_FIND_STREAM_DATA::default();
    // SAFETY: NUL-terminated path and valid output; close successful enumeration.
    let handle = unsafe {
        FindFirstStreamW(
            text.as_ptr(),
            FindStreamInfoStandard,
            (&mut data as *mut WIN32_FIND_STREAM_DATA).cast(),
            0,
        )
    };
    if handle == INVALID_HANDLE_VALUE {
        let error = std::io::Error::last_os_error();
        return if error.raw_os_error() == Some(ERROR_HANDLE_EOF as i32) {
            Ok(())
        } else {
            Err(error.into())
        };
    }
    unsafe {
        FindClose(handle);
    }
    // Unlike a file's default stream, a directory's data stream is user data.
    // The observed Shell tree move drops it; refuse before any move is queued.
    Err(Fault::new(FaultCode::InvalidRequest))
}
