//! Narrow launch targets, never PATH, user-provided executables or shell text.
use crate::domain::{Fault, FaultCode, operations::validate_leaf_name, projects};
use serde::{Deserialize, Serialize};
use std::{
    os::windows::process::CommandExt,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tool {
    SevenZip,
    Winrar,
    Bandizip,
}
#[derive(Serialize)]
pub struct AvailableTool {
    pub id: Tool,
    pub label: &'static str,
}
fn path(tool: Tool) -> Option<PathBuf> {
    let relative = match tool {
        Tool::SevenZip => "7-Zip/7zG.exe",
        Tool::Winrar => "WinRAR/WinRAR.exe",
        Tool::Bandizip => "Bandizip/Bandizip.exe",
    };
    [
        "ProgramW6432",
        "ProgramFiles",
        "ProgramFiles(x86)",
        "LOCALAPPDATA",
    ]
    .into_iter()
    .filter_map(std::env::var_os)
    .map(PathBuf::from)
    .flat_map(|base| [base.join(relative), base.join("Programs").join(relative)])
    .find(|path| path.is_absolute() && path.is_file())
}
pub fn available() -> Vec<AvailableTool> {
    [
        (Tool::SevenZip, "7-Zip"),
        (Tool::Winrar, "WinRAR"),
        (Tool::Bandizip, "Bandizip"),
    ]
    .into_iter()
    .filter(|(tool, _)| path(*tool).is_some())
    .map(|(id, label)| AvailableTool { id, label })
    .collect()
}
fn arguments(tool: Tool, paths: &[String], destination: &str, extract: bool) -> Vec<String> {
    if extract {
        return match tool {
            Tool::SevenZip => vec![
                "x".into(),
                "-ad".into(),
                format!("-o{destination}"),
                "--".into(),
                paths[0].clone(),
            ],
            _ => vec![paths[0].clone()],
        }; // archive opens; the app owns extraction choices
    }
    let mut args: Vec<String> = match tool {
        Tool::SevenZip => vec!["a".into(), "-ad".into(), "-tzip".into(), "--".into()],
        Tool::Winrar => vec!["a".into(), "-afzip".into(), "-ep1".into(), "--".into()],
        Tool::Bandizip => vec!["cd".into()],
    };
    args.push(destination.into());
    args.extend_from_slice(paths);
    args
}
pub fn launch(
    tool: Tool,
    paths: Vec<String>,
    parent: String,
    name: String,
    extract: bool,
) -> Result<(), Fault> {
    super::explorer::validate_paths(&paths)?;
    if extract && paths.len() != 1 {
        return Err(Fault::new(FaultCode::InvalidRequest));
    }
    for value in &paths {
        super::file_operations::capture_source(value)?;
    }
    let parent = projects::normalize_absolute_path(&parent)
        .map_err(|_| Fault::new(FaultCode::InvalidPath))?;
    super::file_operations::capture_source(&parent)?;
    validate_leaf_name(&name)?;
    if !extract && !name.to_lowercase().ends_with(".zip") {
        return Err(Fault::new(FaultCode::InvalidPath));
    }
    let target = Path::new(&parent).join(name);
    match std::fs::symlink_metadata(&target) {
        Ok(_) => return Err(Fault::new(FaultCode::DestinationExists)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
        Err(e) => return Err(e.into()),
    }
    let executable = path(tool).ok_or_else(|| Fault::new(FaultCode::NotFound))?;
    let args = arguments(
        tool,
        &paths,
        target
            .to_str()
            .ok_or_else(|| Fault::new(FaultCode::InvalidPath))?,
        extract,
    );
    if args
        .iter()
        .map(|v| v.encode_utf16().count() + 3)
        .sum::<usize>()
        > 24000
    {
        return Err(Fault::new(FaultCode::PayloadLimit));
    }
    // GUI application owns its confirmation/progress/cancellation. Spawn success
    // means handed off, not archive completion. No elevated verb or flags.
    let mut child = Command::new(executable)
        .args(args)
        .current_dir(parent)
        .creation_flags(0x08000000)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(Fault::from)?;
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn archive_app_arguments_preserve_unicode_and_never_enable_deletion_or_silent_overwrite() {
        let paths = vec![
            r"C:\한글 공백\a&b.txt".into(),
            r"C:\한글 공백\-x.txt".into(),
        ];
        for tool in [Tool::SevenZip, Tool::Winrar, Tool::Bandizip] {
            let args = arguments(tool, &paths, r"D:\result.zip", false);
            assert_eq!(&args[args.len() - 2..], &paths);
            assert!(
                !args
                    .iter()
                    .any(|a| ["-y", "-df", "-sdel", "c"].contains(&a.as_str()))
            );
        }
    }
}
