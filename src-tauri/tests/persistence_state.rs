use projectpane_core::persistence::{
    WindowBounds, WindowStateStore, WorkArea, clamp_window, resolve_data_root,
};
use std::{ffi::OsStr, fs};

fn bounds() -> WindowBounds {
    WindowBounds {
        x: -1200,
        y: 40,
        width: 900,
        height: 700,
        maximized: false,
    }
}

#[test]
fn round_trip_and_replacement_preserve_complete_state() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = WindowStateStore::new(dir.path().into());
    assert_eq!(store.load().unwrap(), None);
    store.save(bounds()).unwrap();
    let changed = WindowBounds {
        maximized: true,
        ..bounds()
    };
    store.save(changed).unwrap();
    assert_eq!(store.load().unwrap(), Some(changed));
    let value: serde_json::Value =
        serde_json::from_slice(&fs::read(dir.path().join("window-state-v1.json")).unwrap())
            .unwrap();
    assert_eq!(value["schemaVersion"], 1);
    assert_eq!(value["bounds"]["width"], 900);
}

#[test]
fn invalid_existing_state_is_preserved_and_disables_future_writes() {
    for bytes in [b"{broken".to_vec(), vec![b' '; 4097], br#"{"schemaVersion":2,"bounds":{"x":0,"y":0,"width":800,"height":600,"maximized":false}}"#.to_vec(), br#"{"schemaVersion":1,"bounds":{"x":0,"y":0,"width":0,"height":600,"maximized":false}}"#.to_vec(), br#"{"schemaVersion":1,"bounds":{"x":0,"y":0,"width":800,"height":600,"maximized":false,"path":"private"}}"#.to_vec()] {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("window-state-v1.json");
        fs::write(&file, &bytes).unwrap();
        let mut store = WindowStateStore::new(dir.path().into());
        assert!(store.load().is_err());
        assert!(store.save(bounds()).is_err());
        assert_eq!(fs::read(&file).unwrap(), bytes);
        fs::remove_file(&file).unwrap();
        assert!(store.save(bounds()).is_err());
    }
}

#[test]
fn save_without_load_cannot_replace_invalid_state_or_old_temp() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("window-state-v1.json");
    let abandoned = dir.path().join("window-state-v1.json.old.tmp");
    fs::write(&file, b"invalid").unwrap();
    fs::write(&abandoned, b"keep").unwrap();
    assert!(
        WindowStateStore::new(dir.path().into())
            .save(bounds())
            .is_err()
    );
    assert_eq!(fs::read(file).unwrap(), b"invalid");
    assert_eq!(fs::read(abandoned).unwrap(), b"keep");
}

#[test]
fn concurrent_saves_leave_one_complete_envelope() {
    let dir = tempfile::tempdir().unwrap();
    std::thread::scope(|scope| {
        for x in 0..12 {
            let path = dir.path();
            scope.spawn(move || {
                WindowStateStore::new(path.into())
                    .save(WindowBounds { x, ..bounds() })
                    .unwrap();
            });
        }
    });
    let loaded = WindowStateStore::new(dir.path().into())
        .load()
        .unwrap()
        .unwrap();
    assert!((0..12).contains(&loaded.x));
    assert_eq!(loaded.width, 900);
}

#[test]
fn geometry_prefers_intersection_then_primary_and_handles_extremes() {
    let areas = [
        WorkArea {
            x: -1920,
            y: 0,
            width: 1920,
            height: 1080,
            min_width: 640,
            min_height: 480,
        },
        WorkArea {
            x: 0,
            y: 0,
            width: 1280,
            height: 720,
            min_width: 640,
            min_height: 480,
        },
    ];
    assert_eq!(clamp_window(bounds(), &areas, Some(1)), Some(bounds()));
    let distant = WindowBounds {
        x: i32::MAX,
        y: i32::MIN,
        width: u32::MAX,
        height: u32::MAX,
        maximized: true,
    };
    assert_eq!(
        clamp_window(distant, &areas, Some(1)),
        Some(WindowBounds {
            x: 0,
            y: 0,
            width: 1280,
            height: 720,
            maximized: true
        })
    );
    assert_eq!(clamp_window(distant, &[], None), None);
}

#[test]
fn data_override_fails_closed_and_only_creates_foundation() {
    let default = tempfile::tempdir().unwrap();
    let alternate = tempfile::tempdir().unwrap();
    assert_eq!(
        resolve_data_root(default.path(), Some(alternate.path().as_os_str())).unwrap(),
        alternate.path().join("foundation")
    );
    assert!(!default.path().join("foundation").exists());
    for invalid in [
        "",
        "relative",
        "\\\\server\\share",
        "\\\\?\\C:\\data",
        "C:\\bad\0name",
    ] {
        assert!(resolve_data_root(default.path(), Some(OsStr::new(invalid))).is_err());
    }
}

#[cfg(windows)]
#[test]
fn failed_replacement_leaves_previous_complete_state_intact() {
    use std::{fs::OpenOptions, os::windows::fs::OpenOptionsExt};
    let directory = tempfile::tempdir().unwrap();
    let mut store = WindowStateStore::new(directory.path().into());
    store.save(bounds()).unwrap();
    let path = directory.path().join("window-state-v1.json");
    let original = fs::read(&path).unwrap();
    let held = OpenOptions::new()
        .read(true)
        .share_mode(3)
        .open(&path)
        .unwrap();
    assert!(store.save(WindowBounds { x: 25, ..bounds() }).is_err());
    assert_eq!(fs::read(&path).unwrap(), original);
    drop(held);
    assert_eq!(store.load().unwrap(), Some(bounds()));
}

#[test]
fn undersized_bounds_fit_even_when_minimum_exceeds_work_area() {
    let area = WorkArea {
        x: -40,
        y: -50,
        width: 100,
        height: 80,
        min_width: 640,
        min_height: 480,
    };
    let result = clamp_window(
        WindowBounds {
            width: 1,
            height: 1,
            ..bounds()
        },
        &[area],
        Some(99),
    )
    .unwrap();
    assert_eq!(
        result,
        WindowBounds {
            x: -40,
            y: -50,
            width: 100,
            height: 80,
            maximized: false
        }
    );
}

#[test]
fn largest_intersection_wins_over_primary() {
    let areas = [
        WorkArea {
            x: 0,
            y: 0,
            width: 1000,
            height: 1000,
            min_width: 1,
            min_height: 1,
        },
        WorkArea {
            x: 1000,
            y: 0,
            width: 1000,
            height: 1000,
            min_width: 1,
            min_height: 1,
        },
    ];
    let result = clamp_window(
        WindowBounds {
            x: 900,
            y: 0,
            width: 700,
            height: 600,
            maximized: false,
        },
        &areas,
        Some(0),
    )
    .unwrap();
    assert_eq!(result.x, 1000);
}

#[test]
fn locked_writer_cannot_change_state() {
    use std::fs::OpenOptions;
    let directory = tempfile::tempdir().unwrap();
    let store = WindowStateStore::new(directory.path().into());
    store.save(bounds()).unwrap();
    let lock = OpenOptions::new()
        .write(true)
        .open(directory.path().join("window-state.lock"))
        .unwrap();
    lock.try_lock().unwrap();
    assert!(matches!(
        store.save(bounds()),
        Err(projectpane_core::persistence::StateError::Busy)
    ));
}

#[cfg(windows)]
#[test]
fn junction_override_and_junction_ancestor_are_rejected() {
    use std::process::{Command, Stdio};
    let directory = tempfile::tempdir().unwrap();
    let target = tempfile::tempdir().unwrap();
    let junction = directory.path().join("junction");
    let status = Command::new("cmd")
        .args(["/c", "mklink", "/J"])
        .arg(&junction)
        .arg(target.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    assert!(resolve_data_root(directory.path(), Some(junction.as_os_str())).is_err());
    assert!(resolve_data_root(directory.path(), Some(junction.join("child").as_os_str())).is_err());
    assert!(!target.path().join("foundation").exists());
    assert!(!target.path().join("child").exists());
    fs::remove_dir(&junction).unwrap();
}

#[cfg(windows)]
#[test]
fn invalid_windows_names_and_long_override_make_no_directories() {
    let directory = tempfile::tempdir().unwrap();
    for name in [
        "CON",
        "NUL.txt",
        "COM1",
        "LPT²",
        "bad:stream",
        "bad?name",
        "bad.",
        "bad ",
    ] {
        let path = directory.path().join("never-create").join(name);
        assert!(resolve_data_root(directory.path(), Some(path.as_os_str())).is_err());
        assert!(!directory.path().join("never-create").exists());
    }
    let oversized = format!("C:\\{}", "a".repeat(32768));
    assert!(resolve_data_root(directory.path(), Some(OsStr::new(&oversized))).is_err());
}
