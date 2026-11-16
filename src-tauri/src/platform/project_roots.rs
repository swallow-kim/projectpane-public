use crate::domain::{Fault, FaultCode, projects::normalize_absolute_path};
use serde::Serialize;
use std::collections::HashMap;

pub const MAX_ROOTS: usize = 512;
pub const MAX_ROOT_BYTES: usize = 256 * 1024;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum RootKind {
    Windows,
    Wsl,
    Network,
    Unknown,
}

#[derive(Debug, Serialize)]
pub struct RootClassification {
    pub path: String,
    pub kind: RootKind,
}

/// Display information only, never authorization for a file operation.
pub fn classify_project_roots(paths: Vec<String>) -> Result<Vec<RootClassification>, Fault> {
    classify_with(paths, drive_type)
}

fn classify_with(
    paths: Vec<String>,
    mut query_drive: impl FnMut(u8) -> u32,
) -> Result<Vec<RootClassification>, Fault> {
    if paths.len() > MAX_ROOTS
        || paths
            .iter()
            .try_fold(0usize, |total, path| total.checked_add(path.len()))
            .is_none_or(|total| total > MAX_ROOT_BYTES)
    {
        return Err(Fault::new(FaultCode::PayloadLimit));
    }
    let mut drives = HashMap::new();
    Ok(paths
        .into_iter()
        .map(|path| {
            let kind = match display_path(&path) {
                None => RootKind::Unknown,
                Some(normalized) if normalized.starts_with("\\\\") => {
                    let host = normalized[2..].split('\\').next().unwrap_or("");
                    if host.eq_ignore_ascii_case("wsl$")
                        || host.eq_ignore_ascii_case("wsl.localhost")
                    {
                        RootKind::Wsl
                    } else {
                        RootKind::Network
                    }
                }
                Some(normalized) => {
                    let drive = normalized.as_bytes()[0];
                    *drives
                        .entry(drive)
                        .or_insert_with(|| match query_drive(drive) {
                            4 => RootKind::Network, // DRIVE_REMOTE includes mapped network drives.
                            2 | 3 | 5 | 6 => RootKind::Windows, // Removable, fixed, optical, RAM disk.
                            _ => RootKind::Unknown,
                        })
                }
            };
            RootClassification { path, kind }
        })
        .collect())
}

fn display_path(path: &str) -> Option<String> {
    if path.len() > 32768 {
        return None;
    }
    let clean = path.replace('/', "\\");
    let ordinary = if let Some(rest) = clean.strip_prefix("\\\\?\\") {
        let candidate = if rest
            .get(..4)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("UNC\\"))
        {
            format!("\\\\{}", &rest[4..])
        } else if rest
            .as_bytes()
            .get(..3)
            .is_some_and(|prefix| prefix[0].is_ascii_alphabetic() && prefix[1..] == *b":\\")
        {
            rest.to_owned()
        } else {
            return None;
        };
        if candidate
            .split('\\')
            .any(|part| part == "." || part == "..")
        {
            return None;
        }
        candidate
    } else {
        clean
    };
    normalize_absolute_path(&ordinary).ok()
}

#[cfg(windows)]
fn drive_type(drive: u8) -> u32 {
    let root = [u16::from(drive), u16::from(b':'), u16::from(b'\\'), 0];
    // SAFETY: root is initialized, NUL-terminated UTF-16. This synchronous API
    // only reads the buffer and does not retain it. No directory is opened or
    // enumerated, and UNC servers are never passed to this API.
    unsafe { windows_sys::Win32::Storage::FileSystem::GetDriveTypeW(root.as_ptr()) }
}

#[cfg(not(windows))]
fn drive_type(_drive: u8) -> u32 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn reads_the_real_system_drive_without_opening_user_files() {
        let drive = std::env::var("SystemDrive").expect("Windows exposes SystemDrive");
        let path = format!("{drive}\\");
        let result = classify_project_roots(vec![path.clone()]).unwrap();
        assert_eq!(result[0].path, path);
        assert!(matches!(
            result[0].kind,
            RootKind::Windows | RootKind::Network
        ));
    }

    #[test]
    fn detects_mapped_drives_and_queries_each_drive_only_once() {
        let mut queries = Vec::new();
        let paths = vec![
            r"Z:\project",
            r"z:\another",
            r"\\?\Z:\third",
            r"C:\Work",
            r"c:\More",
        ];
        let result = classify_with(paths.iter().map(|p| (*p).into()).collect(), |drive| {
            queries.push(drive);
            if drive == b'Z' { 4 } else { 3 }
        })
        .unwrap();
        assert_eq!(queries, [b'Z', b'C']);
        assert_eq!(
            result.iter().map(|p| p.kind).collect::<Vec<_>>(),
            [
                RootKind::Network,
                RootKind::Network,
                RootKind::Network,
                RootKind::Windows,
                RootKind::Windows
            ]
        );
        assert_eq!(
            result.iter().map(|p| p.path.as_str()).collect::<Vec<_>>(),
            paths
        );
    }

    #[test]
    fn classifies_unc_and_wsl_without_contacting_servers() {
        let inputs = [
            (r"\\wsl$\Ubuntu\home\User", RootKind::Wsl),
            (r"\\WSL.LOCALHOST\Ubuntu\home\user", RootKind::Wsl),
            (r"\\?\unc\wsl$\Ubuntu\home\User", RootKind::Wsl),
            ("//wsl.localhost/Ubuntu/home/한글", RootKind::Wsl),
            (r"\\server\share", RootKind::Network),
            (r"\\?\UNC\192.168.1.2\share", RootKind::Network),
            (r"\\wsl.localhost.example.com\share", RootKind::Network),
        ];
        let result = classify_with(inputs.iter().map(|(p, _)| (*p).into()).collect(), |_| {
            panic!("UNC classification must not query a drive")
        })
        .unwrap();
        for (actual, (path, expected)) in result.iter().zip(inputs) {
            assert_eq!(actual.kind, expected);
            assert_eq!(actual.path, path);
        }
    }

    #[test]
    fn returns_unknown_for_invalid_paths_without_querying_the_os() {
        let paths = [
            "",
            "relative",
            r"C:relative",
            "/home/user",
            r"\\server",
            r"\\.\PhysicalDrive0",
            r"\\?\Volume{123}\",
            r"\\?\GLOBALROOT\Device",
            r"\\?\UNC\wsl$\Ubuntu\a\..\b",
            r"\\?\C:\a\.\b",
            "file://server/share",
            r"C:\CON",
            "C:\\bad\0name",
            r"\\server\share\..\..\escape",
        ];
        let result = classify_with(paths.iter().map(|p| (*p).into()).collect(), |_| {
            panic!("Invalid paths must not query a drive")
        })
        .unwrap();
        assert!(result.iter().all(|p| p.kind == RootKind::Unknown));
    }

    #[test]
    fn handles_all_documented_drive_types_and_unknown_results() {
        for (api, expected) in [
            (0, RootKind::Unknown),
            (1, RootKind::Unknown),
            (2, RootKind::Windows),
            (3, RootKind::Windows),
            (4, RootKind::Network),
            (5, RootKind::Windows),
            (6, RootKind::Windows),
            (99, RootKind::Unknown),
        ] {
            let result = classify_with(vec![r"C:\Work".into()], |_| api).unwrap();
            assert_eq!(result[0].kind, expected);
        }
    }

    #[test]
    fn bounds_request_work_and_preserves_duplicates_in_response() {
        assert!(classify_with(vec![String::new(); MAX_ROOTS + 1], |_| 3).is_err());
        assert!(classify_with(vec!["a".repeat(MAX_ROOT_BYTES + 1)], |_| 3).is_err());
        assert!(
            classify_with(Vec::new(), |_| panic!("no drive"))
                .unwrap()
                .is_empty()
        );
        let result = classify_with(vec![r"C:\Work".into(); MAX_ROOTS], |_| 3).unwrap();
        assert_eq!(result.len(), MAX_ROOTS);
        assert!(result.iter().all(|p| p.kind == RootKind::Windows));
        assert_eq!(
            serde_json::to_string(&result[0]).unwrap(),
            r#"{"path":"C:\\Work","kind":"windows"}"#
        );
    }
}
