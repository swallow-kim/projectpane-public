//! Narrow Shell integration for physical filesystem items. External transfers are copy-only.
pub mod context_menu;
pub mod path_text;
// Research implementation only. No desktop command, feature flag, or clipboard
// entry point can enable external moves until recovery integration is complete.
#[cfg(test)]
pub(crate) mod external_move;
use crate::domain::{AbsolutePath, ActivationKind, ActivationTarget, Fault, FaultCode};
use std::{collections::HashSet, path::Path, ptr};
use windows::{
    Win32::{
        Foundation::{
            DRAGDROP_S_CANCEL, DRAGDROP_S_DROP, DRAGDROP_S_USEDEFAULTCURSORS, DV_E_FORMATETC, S_OK,
        },
        System::{
            Com::{
                CoTaskMemFree, DVASPECT_CONTENT, FORMATETC, IDataObject, STGMEDIUM, TYMED_HGLOBAL,
            },
            DataExchange::{GetClipboardSequenceNumber, RegisterClipboardFormatW},
            Memory::{GlobalLock, GlobalSize, GlobalUnlock},
            Ole::{
                DROPEFFECT, DROPEFFECT_COPY, DoDragDrop, IDropSource, IDropSource_Impl,
                OleFlushClipboard, OleGetClipboard, OleInitialize, OleSetClipboard,
                OleUninitialize, ReleaseStgMedium,
            },
            SystemServices::MODIFIERKEYS_FLAGS,
        },
        UI::Shell::{
            Common::ITEMIDLIST, ILFindLastID, SHCreateDataObject, SHOP_FILEPATH,
            SHObjectProperties, SHOpenFolderAndSelectItems, SHParseDisplayName,
        },
    },
    core::{BOOL, HRESULT, PCWSTR, implement, w},
};

const MAX_PATH_BYTES: usize = 256 * 1024;
const MAX_ITEMS: usize = 1024;
fn native_error(_: windows::core::Error) -> Fault {
    Fault::new(FaultCode::IoError)
}
struct OleApartment;
impl OleApartment {
    fn new() -> Result<Self, Fault> {
        // SAFETY: each successful initialization is balanced on this same thread.
        unsafe { OleInitialize(None) }.map_err(native_error)?;
        Ok(Self)
    }
}
impl Drop for OleApartment {
    fn drop(&mut self) {
        unsafe { OleUninitialize() };
    }
}
struct Pidl(*mut ITEMIDLIST);
impl Pidl {
    fn parse(path: &str) -> Result<Self, Fault> {
        AbsolutePath::new(path.to_owned())?;
        let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
        let mut pidl = ptr::null_mut();
        // SAFETY: input is NUL-terminated; the output allocation is owned by this guard.
        unsafe { SHParseDisplayName(PCWSTR(wide.as_ptr()), None, &mut pidl, 0, None) }
            .map_err(native_error)?;
        if pidl.is_null() {
            return Err(Fault::new(FaultCode::IoError));
        }
        Ok(Self(pidl))
    }
}
impl Drop for Pidl {
    fn drop(&mut self) {
        unsafe { CoTaskMemFree(Some(self.0.cast())) };
    }
}

pub fn validate_paths(paths: &[String]) -> Result<(), Fault> {
    if paths.is_empty()
        || paths.len() > MAX_ITEMS
        || paths.iter().map(String::len).sum::<usize>() > MAX_PATH_BYTES
    {
        return Err(Fault::new(FaultCode::ResourceLimit));
    }
    let mut unique = HashSet::new();
    for path in paths {
        AbsolutePath::new(path.clone())?;
        if !unique.insert(path.replace('/', "\\").to_lowercase()) {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
    }
    Ok(())
}

pub fn targets(paths: Vec<String>) -> Result<Vec<ActivationTarget>, Fault> {
    let paths = normalize_import_paths(paths)?;
    paths
        .into_iter()
        .map(|path| {
            use std::os::windows::fs::MetadataExt;
            let metadata = std::fs::symlink_metadata(&path).map_err(Fault::from)?;
            if metadata.file_attributes() & 0x400 != 0
                || (!metadata.is_dir() && !metadata.is_file())
            {
                return Err(Fault::new(FaultCode::InvalidPath));
            }
            Ok(ActivationTarget {
                kind: if metadata.is_dir() {
                    ActivationKind::Folder
                } else {
                    ActivationKind::File
                },
                path,
            })
        })
        .collect()
}

// Windows file drops can carry verbatim UNC paths. Convert only the documented
// filesystem prefixes; never treat a device path or file:// text as an address.
pub fn normalize_import_paths(paths: Vec<String>) -> Result<Vec<String>, Fault> {
    validate_paths(&paths)?;
    let normalized = paths
        .into_iter()
        .map(|path| {
            let ordinary = if let Some(rest) = path.strip_prefix(r"\\?\UNC\") {
                format!(r"\\{rest}")
            } else if let Some(rest) = path.strip_prefix(r"\\?\") {
                let bytes = rest.as_bytes();
                if bytes.len() < 3 || !bytes[0].is_ascii_alphabetic() || &bytes[1..3] != b":\\" {
                    return Err(Fault::new(FaultCode::InvalidPath));
                }
                rest.to_owned()
            } else {
                path.clone()
            };
            // Verbatim paths do not interpret dot segments. Do not invent a different
            // target by normalizing them as ordinary Win32 paths.
            if path.starts_with(r"\\?\") && ordinary.split('\\').any(|p| p == "." || p == "..") {
                return Err(Fault::new(FaultCode::InvalidPath));
            }
            crate::domain::projects::normalize_absolute_path(&ordinary)
                .map_err(|_| Fault::new(FaultCode::InvalidPath))
        })
        .collect::<Result<Vec<_>, _>>()?;
    validate_paths(&normalized)?;
    Ok(normalized)
}

struct ShellSelection {
    parent: Pidl,
    items: Vec<Pidl>,
}
impl ShellSelection {
    fn new(paths: &[String]) -> Result<Self, Fault> {
        validate_paths(paths)?;
        let parent = Path::new(&paths[0])
            .parent()
            .ok_or_else(|| Fault::new(FaultCode::InvalidPath))?;
        if paths
            .iter()
            .any(|path| Path::new(path).parent() != Some(parent))
        {
            return Err(Fault::new(FaultCode::InvalidPath));
        }
        let parent = Pidl::parse(
            parent
                .to_str()
                .ok_or_else(|| Fault::new(FaultCode::InvalidPath))?,
        )?;
        let items: Vec<Pidl> = paths
            .iter()
            .map(|path| Pidl::parse(path))
            .collect::<Result<_, _>>()?;
        Ok(Self { parent, items })
    }
    fn children(&self) -> Vec<*const ITEMIDLIST> {
        // SAFETY: each child pointer remains valid while its owning PIDL is alive.
        self.items
            .iter()
            .map(|item| unsafe { ILFindLastID(item.0) as *const ITEMIDLIST })
            .collect()
    }
}

fn shell_object(paths: &[String]) -> Result<IDataObject, Fault> {
    let selection = ShellSelection::new(paths)?;
    let children = selection.children();
    // SAFETY: parent and child PIDLs stay alive until the Shell copies them.
    unsafe { SHCreateDataObject(Some(selection.parent.0), Some(&children), None) }
        .map_err(native_error)
}

pub fn reveal(paths: Vec<String>) -> Result<(), Fault> {
    let _ole = OleApartment::new()?;
    let selection = ShellSelection::new(&paths)?;
    let children = selection.children();
    unsafe { SHOpenFolderAndSelectItems(selection.parent.0, Some(&children), 0) }
        .map_err(native_error)
}

pub fn properties(path: String) -> Result<(), Fault> {
    let _ole = OleApartment::new()?;
    AbsolutePath::new(path.clone())?;
    let wide: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
    // A fixed Shell command, with no shell command-line or arbitrary verb input.
    if unsafe { SHObjectProperties(None, SHOP_FILEPATH, PCWSTR(wide.as_ptr()), PCWSTR::null()) }
        .as_bool()
    {
        Ok(())
    } else {
        Err(Fault::new(FaultCode::IoError))
    }
}

pub fn copy_to_clipboard(paths: Vec<String>) -> Result<(), Fault> {
    let _ole = OleApartment::new()?;
    let object = shell_object(&paths)?;
    // Flush renders formats before this thread and its data object go away.
    unsafe { OleSetClipboard(&object).and_then(|()| OleFlushClipboard()) }.map_err(native_error)
}

fn format(id: u16) -> FORMATETC {
    FORMATETC {
        cfFormat: id,
        ptd: ptr::null_mut(),
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
    }
}

// CF_HDROP is untrusted cross-process data. Bound and parse the Unicode DROPFILES
// buffer ourselves instead of allowing an unterminated list to escape GlobalSize.
fn parse_drop_files(bytes: &[u8]) -> Result<Vec<String>, Fault> {
    let invalid = || Fault::new(FaultCode::InvalidRequest);
    if bytes.len() < 24 || bytes.len() > MAX_PATH_BYTES * 2 + 64 {
        return Err(invalid());
    }
    let offset = u32::from_le_bytes(bytes[..4].try_into().unwrap()) as usize;
    let wide = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
    if offset < 20 || !offset.is_multiple_of(2) || offset > bytes.len() - 4 || wide == 0 {
        return Err(invalid());
    }
    let mut paths = Vec::new();
    let mut name = Vec::new();
    for unit in bytes[offset..].as_chunks::<2>().0 {
        let unit = u16::from_le_bytes([unit[0], unit[1]]);
        if unit == 0 {
            if name.is_empty() {
                return normalize_import_paths(paths);
            }
            paths.push(String::from_utf16(&name).map_err(|_| invalid())?);
            name.clear();
            if paths.len() > MAX_ITEMS {
                return Err(invalid());
            }
        } else {
            name.push(unit);
            if name.len() > 32768 {
                return Err(invalid());
            }
        }
    }
    Err(invalid())
}

fn require_copy_effect(effect: &STGMEDIUM) -> Result<(), Fault> {
    if effect.tymed != TYMED_HGLOBAL.0 as u32 {
        return Err(Fault::new(FaultCode::InvalidRequest));
    }
    let handle = unsafe { effect.u.hGlobal };
    if unsafe { GlobalSize(handle) } < 4 {
        return Err(Fault::new(FaultCode::InvalidRequest));
    }
    let value = unsafe { GlobalLock(handle) };
    if value.is_null() {
        return Err(Fault::new(FaultCode::IoError));
    }
    let effect = unsafe { ptr::read_unaligned(value.cast::<u32>()) };
    let _ = unsafe { GlobalUnlock(handle) };
    if effect == DROPEFFECT_COPY.0 {
        Ok(())
    } else {
        Err(Fault::new(FaultCode::InvalidRequest))
    }
}

fn read_paths(object: &IDataObject) -> Result<Vec<String>, Fault> {
    // Missing preference is allowed; an unreadable, move or link preference is
    // not permission to copy. External moves require a completion protocol.
    let effect_format = unsafe { RegisterClipboardFormatW(w!("Preferred DropEffect")) };
    if effect_format == 0 {
        return Err(Fault::new(FaultCode::IoError));
    }
    match unsafe { object.GetData(&format(effect_format as u16)) } {
        Ok(mut effect) => {
            let result = require_copy_effect(&effect);
            unsafe { ReleaseStgMedium(&mut effect) };
            result?;
        }
        Err(error) if error.code() == DV_E_FORMATETC => {}
        Err(error) => return Err(native_error(error)),
    }
    read_file_paths(object)
}

fn read_file_paths(object: &IDataObject) -> Result<Vec<String>, Fault> {
    let mut data = unsafe { object.GetData(&format(15)) }
        .map_err(|_| Fault::new(FaultCode::InvalidRequest))?;
    let result = (|| {
        if data.tymed != TYMED_HGLOBAL.0 as u32 {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
        let handle = unsafe { data.u.hGlobal };
        let size = unsafe { GlobalSize(handle) };
        if !(24..=MAX_PATH_BYTES * 2 + 64).contains(&size) {
            return Err(Fault::new(FaultCode::ResourceLimit));
        }
        let value = unsafe { GlobalLock(handle) };
        if value.is_null() {
            return Err(Fault::new(FaultCode::IoError));
        }
        // SAFETY: locked storage is readable for the checked GlobalSize bytes.
        let result =
            parse_drop_files(unsafe { std::slice::from_raw_parts(value.cast::<u8>(), size) });
        let _ = unsafe { GlobalUnlock(handle) };
        result
    })();
    // SAFETY: GetData transfers ownership of exactly this medium; release on every outcome.
    unsafe { ReleaseStgMedium(&mut data) };
    result
}

pub fn clipboard_paths() -> Result<Vec<String>, Fault> {
    let _ole = OleApartment::new()?;
    let object = unsafe { OleGetClipboard() }.map_err(native_error)?;
    read_paths(&object)
}

/// Read only the window-station change counter, never clipboard content.
pub fn clipboard_sequence() -> Result<u32, Fault> {
    let sequence = unsafe { GetClipboardSequenceNumber() };
    if sequence == 0 {
        return Err(Fault::new(FaultCode::IoError));
    }
    Ok(sequence)
}

#[implement(IDropSource)]
struct CopySource;
impl IDropSource_Impl for CopySource_Impl {
    fn QueryContinueDrag(&self, escape: BOOL, keys: MODIFIERKEYS_FLAGS) -> HRESULT {
        if escape.as_bool() {
            DRAGDROP_S_CANCEL
        } else if keys.0 & 1 == 0 {
            DRAGDROP_S_DROP
        } else {
            S_OK
        }
    }
    fn GiveFeedback(&self, _: DROPEFFECT) -> HRESULT {
        DRAGDROP_S_USEDEFAULTCURSORS
    }
}

/// Run on the window's UI thread while the user's left button is held.
pub fn drag_copy(paths: Vec<String>) -> Result<bool, Fault> {
    let _ole = OleApartment::new()?;
    let object = shell_object(&paths)?;
    let source: IDropSource = CopySource.into();
    let mut effect = DROPEFFECT(0);
    let result = unsafe { DoDragDrop(&object, &source, DROPEFFECT_COPY, &mut effect) };
    if result == DRAGDROP_S_CANCEL {
        return Ok(false);
    }
    result.ok().map_err(native_error)?;
    Ok(effect == DROPEFFECT_COPY)
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows::Win32::System::{
        Com::STGMEDIUM_0,
        Memory::{GMEM_MOVEABLE, GlobalAlloc},
    };

    #[test]
    fn shell_preference_rejects_move_link_and_invalid_native_media() {
        std::thread::spawn(|| {
            let _ole = OleApartment::new().unwrap();
            assert!(require_copy_effect(&STGMEDIUM::default()).is_err());
            for effect in [1u32, 2, 4, 3, 0, u32::MAX] {
                let handle = unsafe { GlobalAlloc(GMEM_MOVEABLE, 4) }.unwrap();
                let value = unsafe { GlobalLock(handle) };
                assert!(!value.is_null());
                unsafe { ptr::write_unaligned(value.cast::<u32>(), effect) };
                let _ = unsafe { GlobalUnlock(handle) };
                let mut medium = STGMEDIUM {
                    tymed: TYMED_HGLOBAL.0 as u32,
                    u: STGMEDIUM_0 { hGlobal: handle },
                    ..Default::default()
                };
                let result = require_copy_effect(&medium);
                unsafe { ReleaseStgMedium(&mut medium) };
                if effect == 1 {
                    assert!(result.is_ok());
                } else {
                    assert!(result.is_err(), "effect {effect}");
                }
            }
        })
        .join()
        .unwrap();
    }
    #[test]
    fn drop_files_rejects_unterminated_invalid_and_oversized_buffers() {
        let mut data = vec![0u8; 20];
        data[..4].copy_from_slice(&20u32.to_le_bytes());
        data[16..20].copy_from_slice(&1u32.to_le_bytes());
        for unit in "C:\\한글.txt".encode_utf16().chain([0, 0]) {
            data.extend(unit.to_le_bytes());
        }
        assert_eq!(parse_drop_files(&data).unwrap(), vec!["C:\\한글.txt"]);
        assert!(parse_drop_files(&data[..data.len() - 2]).is_err());
        let mut malformed = data.clone();
        malformed[..4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(parse_drop_files(&malformed).is_err());
        malformed = data.clone();
        malformed[16..20].fill(0);
        assert!(parse_drop_files(&malformed).is_err());
        assert!(parse_drop_files(&vec![0; MAX_PATH_BYTES * 2 + 65]).is_err());
    }
    #[test]
    fn drop_files_normalizes_verbatim_wsl_without_decoding_names_or_devices() {
        let paths = [
            r"\\?\UNC\wsl.localhost\Ubuntu\home\example-user\한글 space.txt",
            r"\\wsl$\Ubuntu\home\example-user\literal%20.txt",
        ];
        let mut data = vec![0u8; 20];
        data[..4].copy_from_slice(&20u32.to_le_bytes());
        data[16..20].copy_from_slice(&1u32.to_le_bytes());
        for path in paths {
            for unit in path.encode_utf16().chain([0]) {
                data.extend(unit.to_le_bytes());
            }
        }
        data.extend(0u16.to_le_bytes());
        assert_eq!(
            parse_drop_files(&data).unwrap(),
            [
                r"\\wsl.localhost\Ubuntu\home\example-user\한글 space.txt",
                paths[1]
            ]
        );
        for bad in [
            r"\\?\GLOBALROOT\Device\x",
            r"\\.\PhysicalDrive0",
            r"\\?\UNC\wsl$\Ubuntu\a\..\b",
            "file://wsl.localhost/Ubuntu/a",
        ] {
            assert!(normalize_import_paths(vec![bad.into()]).is_err(), "{bad}");
        }
        assert!(
            normalize_import_paths(vec![
                r"\\?\UNC\wsl$\Ubuntu\a".into(),
                r"\\wsl$\Ubuntu\a".into()
            ])
            .is_err()
        );
    }
    #[test]
    fn rejects_empty_duplicate_relative_and_excessive_inputs() {
        for paths in [
            vec![],
            vec!["relative.txt".into()],
            vec!["C:\\one".into(), "c:\\ONE".into()],
            vec!["C:\\one".into(); 1025],
        ] {
            assert!(validate_paths(&paths).is_err());
        }
    }
    #[test]
    fn shell_file_object_roundtrip_keeps_unicode_paths_without_using_clipboard() {
        std::thread::spawn(|| {
            let _ole = OleApartment::new().unwrap();
            let root = tempfile::tempdir().unwrap();
            let file = root.path().join("한글 공백.txt");
            std::fs::write(&file, b"fixture").unwrap();
            let second = root.path().join("두 번째.txt");
            std::fs::write(&second, b"second").unwrap();
            let paths = vec![
                file.to_str().unwrap().to_owned(),
                second.to_str().unwrap().to_owned(),
            ];
            let selection = ShellSelection::new(&paths).unwrap();
            assert_eq!(selection.children().len(), 2);
            let object = shell_object(&paths).unwrap();
            assert_eq!(read_paths(&object).unwrap(), paths);
            let other = tempfile::tempdir().unwrap();
            let outside = other.path().join("other.txt");
            std::fs::write(&outside, b"other").unwrap();
            assert!(
                ShellSelection::new(&[paths[0].clone(), outside.to_str().unwrap().to_owned()])
                    .is_err()
            );
        })
        .join()
        .unwrap();
    }
}
