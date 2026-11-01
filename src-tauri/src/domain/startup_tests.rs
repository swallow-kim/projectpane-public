use super::StartupConfig;
use crate::domain::MAX_FRAME;
use std::ffi::OsString;

fn root() -> &'static str {
    if cfg!(windows) { "C:\\" } else { "/" }
}

fn parse(args: &[&str]) -> StartupConfig {
    StartupConfig::from_args(args.iter().map(OsString::from))
}

fn assert_invalid(config: &StartupConfig) {
    assert!(config.folders.is_empty());
    assert_eq!(config.error.as_deref(), Some("invalid_startup_args"));
    assert_eq!(
        serde_json::to_value(config).unwrap(),
        serde_json::json!({"folders": [], "error": "invalid_startup_args"})
    );
}

#[test]
fn empty_config_when_no_arguments() {
    let config = parse(&[]);
    assert_eq!(
        serde_json::to_value(config.clone()).unwrap(),
        serde_json::json!({"folders": [], "error": null})
    );
}

#[test]
fn preserves_four_paths_in_order_without_filesystem_checks() {
    let paths: Vec<String> = ["missing/../first", "with spaces", "한글📁", "with spaces"]
        .map(|name| format!("{}{name}", root()))
        .into();
    let args = paths.iter().flat_map(|path| ["--folder", path.as_str()]);
    let config = StartupConfig::from_args(args.map(OsString::from));
    assert_eq!(config.folders, paths);
    assert!(config.error.is_none());
    assert_eq!(
        serde_json::to_value(config).unwrap(),
        serde_json::json!({"folders": paths, "error": null})
    );
}

#[test]
fn rejects_non_exact_flags_and_missing_or_relative_values() {
    for args in [
        vec!["--folder"],
        vec!["--folder", "--folder"],
        vec!["--folder", ""],
        vec!["--folder", "relative"],
        vec!["--folder", "../relative"],
        vec!["--Folder", root()],
        vec!["-folder", root()],
        vec!["--folder=value"],
        vec!["--"],
        vec!["--help"],
        vec![root()],
    ] {
        assert_invalid(&parse(&args));
    }
}

#[test]
fn trailing_invalid_arguments_discard_previously_valid_folders() {
    for tail in [
        vec!["--unexpected-private-value"],
        vec!["--folder"],
        vec!["--folder", "relative-private-path"],
        vec![root()],
        vec!["--"],
    ] {
        let mut args = vec!["--folder", root(), "--folder", root()];
        args.extend(tail);
        assert_invalid(&parse(&args));
    }
}

#[test]
fn rejects_fifth_folder_without_partial_config() {
    let args = (0..5).flat_map(|_| [OsString::from("--folder"), OsString::from(root())]);
    assert_invalid(&StartupConfig::from_args(args));
}

#[test]
fn rejects_nul_in_path_without_partial_config() {
    let path = format!("{}private\0path", root());
    assert_invalid(&parse(&["--folder", root(), "--folder", &path]));
}

#[test]
fn accepts_exact_byte_limit_and_rejects_one_byte_over() {
    let path = format!("{}{}", root(), "a".repeat(32_768 - root().len()));
    let config = parse(&["--folder", &path]);
    assert_eq!(config.folders, std::slice::from_ref(&path));
    assert!(config.error.is_none());
    assert_invalid(&parse(&["--folder", root(), "--folder", &(path + "a")]));
}

#[test]
fn measures_multibyte_paths_in_utf8_bytes() {
    let path = format!("{}{}", root(), "한".repeat(11_000));
    assert!(path.chars().count() < 32_768);
    assert_invalid(&parse(&["--folder", &path]));
}

fn args_for_payload_size(size: usize) -> Vec<OsString> {
    let mut paths = vec![root().to_owned(); 4];
    let base = serde_json::to_vec(&StartupConfig {
        folders: paths.clone(),
        error: None,
    })
    .unwrap()
    .len();
    let extra = size - base;
    paths[0].push_str(&"\u{1}".repeat(30_000));
    paths[1].push_str(&"\u{1}".repeat((extra - 180_000) / 6));
    paths[1].push_str(&"a".repeat((extra - 180_000) % 6));
    assert!(paths.iter().all(|path| path.len() <= 32_768));
    assert_eq!(
        serde_json::to_vec(&StartupConfig {
            folders: paths.clone(),
            error: None
        })
        .unwrap()
        .len(),
        size
    );
    paths
        .into_iter()
        .flat_map(|path| [OsString::from("--folder"), OsString::from(path)])
        .collect()
}

#[test]
fn accepts_serialized_payload_just_below_frame_limit() {
    let config = StartupConfig::from_args(args_for_payload_size(MAX_FRAME - 1));
    assert!(config.error.is_none());
    assert_eq!(serde_json::to_vec(&config).unwrap().len(), MAX_FRAME - 1);
}

#[test]
fn rejects_json_escaping_expansion_at_and_above_frame_limit() {
    for size in [MAX_FRAME, MAX_FRAME + 1] {
        assert_invalid(&StartupConfig::from_args(args_for_payload_size(size)));
    }
}

#[cfg(windows)]
#[test]
fn preserves_unc_and_extended_paths_verbatim() {
    for path in [
        r"\\server\share\missing\..\한글",
        r"\\?\C:\missing\..\folder",
        r"\\?\UNC\server\share\missing",
    ] {
        let config = parse(&["--folder", path]);
        assert_eq!(config.folders, [path]);
        assert!(config.error.is_none());
    }
}

#[cfg(windows)]
#[test]
fn rejects_drive_relative_and_root_relative_windows_paths() {
    for path in [r"C:folder", r"\folder", "/folder", "C:"] {
        assert_invalid(&parse(&["--folder", path]));
    }
}

#[cfg(any(windows, unix))]
#[test]
fn rejects_non_unicode_values_and_flags_losslessly() {
    #[cfg(windows)]
    let invalid = {
        use std::os::windows::ffi::OsStringExt;
        OsString::from_wide(&[0xd800])
    };
    #[cfg(unix)]
    let invalid = {
        use std::os::unix::ffi::OsStringExt;
        OsString::from_vec(vec![0xff])
    };
    for tail in [
        vec![invalid.clone()],
        vec![OsString::from("--folder"), invalid],
    ] {
        let mut args = vec![OsString::from("--folder"), OsString::from(root())];
        args.extend(tail);
        assert_invalid(&StartupConfig::from_args(args));
    }
}
