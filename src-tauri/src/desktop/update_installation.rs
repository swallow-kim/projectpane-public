//! Do not infer installation from the executable name or bundle metadata alone:
//! the same executable also ships in the portable ZIP.
use std::path::{Path, PathBuf};

fn same_executable(exe: &Path, directory: &Path) -> bool {
    let (Ok(exe), Ok(installed)) = (
        exe.canonicalize(),
        directory.join("projectpane.exe").canonicalize(),
    ) else {
        return false;
    };
    exe == installed && directory.join("uninstall.exe").is_file()
}

pub fn installed() -> bool {
    if std::env::var_os("PROJECTPANE_DATA_DIR").is_some()
        || std::env::var_os("WEBVIEW2_USER_DATA_FOLDER").is_some()
    {
        return false;
    }
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    installation_directory().is_some_and(|directory| same_executable(&exe, &directory))
}

#[cfg(windows)]
fn installation_directory() -> Option<PathBuf> {
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, RRF_RT_REG_SZ, RRF_SUBKEY_WOW6464KEY, RegGetValueW,
    };
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let key = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\ProjectPane");
    let name = wide("InstallLocation");
    let mut value = [0u16; 32768];
    let mut length = std::mem::size_of_val(&value) as u32;
    let result = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ | RRF_SUBKEY_WOW6464KEY,
            std::ptr::null_mut(),
            value.as_mut_ptr().cast(),
            &mut length,
        )
    };
    if result != 0 || length < 2 || length as usize > std::mem::size_of_val(&value) {
        return None;
    }
    let value = String::from_utf16(&value[..length as usize / 2 - 1]).ok()?;
    let value = value
        .strip_prefix('"')
        .and_then(|s| s.strip_suffix('"'))
        .unwrap_or(&value);
    Some(PathBuf::from(value))
}
#[cfg(not(windows))]
fn installation_directory() -> Option<PathBuf> {
    None
}

/// Restart Manager checks the exact installed worker binary, including workers
/// whose previous asynchronous cancellation returned an uncertain result.
#[cfg(windows)]
pub fn workers_closed() -> bool {
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let Some(parent) = exe.parent() else {
        return false;
    };
    worker_file_closed(&parent.join("projectpane-read-worker.exe"))
}
#[cfg(windows)]
fn worker_file_closed(worker: &Path) -> bool {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::RestartManager::{
        RmEndSession, RmGetList, RmRegisterResources, RmStartSession,
    };
    if !worker.is_file() {
        return false;
    }
    let worker: Vec<u16> = worker.as_os_str().encode_wide().chain(Some(0)).collect();
    let mut key = [0u16; 33];
    let mut session = 0;
    unsafe {
        if RmStartSession(&mut session, 0, key.as_mut_ptr()) != 0 {
            return false;
        }
        let mut needed = 0;
        let mut count = 0;
        let mut reason = 0;
        let registered = RmRegisterResources(
            session,
            1,
            &worker.as_ptr(),
            0,
            std::ptr::null(),
            0,
            std::ptr::null(),
        ) == 0;
        let clear = registered
            && RmGetList(
                session,
                &mut needed,
                &mut count,
                std::ptr::null_mut(),
                &mut reason,
            ) == 0
            && needed == 0;
        let ended = RmEndSession(session) == 0;
        clear && ended
    }
}
#[cfg(not(windows))]
pub fn workers_closed() -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn portable_copy_is_not_the_registered_executable() {
        let root = tempfile::tempdir().unwrap();
        let installed = root.path().join("installed");
        let portable = root.path().join("portable");
        for dir in [&installed, &portable] {
            std::fs::create_dir(dir).unwrap();
            std::fs::write(dir.join("projectpane.exe"), b"synthetic").unwrap();
        }
        assert!(!same_executable(
            &installed.join("projectpane.exe"),
            &installed
        ));
        std::fs::write(installed.join("uninstall.exe"), b"synthetic").unwrap();
        assert!(same_executable(
            &installed.join("projectpane.exe"),
            &installed
        ));
        assert!(!same_executable(
            &portable.join("projectpane.exe"),
            &installed
        ));
    }

    #[cfg(windows)]
    #[tokio::test]
    #[ignore = "explicit native fixture worker path required"]
    async fn native_update_shutdown_reaps_four_listing_workers_and_watch() {
        use projectpane_core::{
            domain::Identity,
            services::{Broker, watch::WatchHub},
        };
        use std::time::Duration;
        let source = std::env::var_os("PROJECTPANE_TEST_WORKER")
            .expect("explicit synthetic test worker required");
        let root = tempfile::tempdir().unwrap();
        let worker = root.path().join("projectpane-read-worker.exe");
        std::fs::copy(source, &worker).unwrap();
        let files = root.path().join("synthetic");
        std::fs::create_dir(&files).unwrap();
        std::fs::write(files.join("example.txt"), b"synthetic data").unwrap();
        let broker = Broker::new(worker.clone(), Duration::from_secs(8));
        let identity = |pane, generation| Identity {
            pane,
            generation,
            tab_id: "test".into(),
            request_id: "request".into(),
        };
        for pane in 0..4 {
            broker
                .start(identity(pane, 1), files.to_string_lossy().into())
                .await
                .unwrap();
        }
        let watch = WatchHub::new(worker.clone());
        watch
            .poll(vec![files.to_string_lossy().into()])
            .await
            .unwrap();
        assert!(
            !worker_file_closed(&worker),
            "the running fixture must prevent handoff"
        );
        broker.shutdown().await.unwrap();
        watch.shutdown().await.unwrap();
        assert!(
            worker_file_closed(&worker),
            "all owned workers must be confirmed stopped"
        );
        assert!(
            broker
                .start(identity(0, 2), files.to_string_lossy().into())
                .await
                .is_err()
        );
        assert!(
            watch
                .poll(vec![files.to_string_lossy().into()])
                .await
                .is_err()
        );
        assert_eq!(
            std::fs::read(files.join("example.txt")).unwrap(),
            b"synthetic data"
        );
    }
}
