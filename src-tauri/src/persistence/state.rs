use super::{WindowBounds, paths::check_ancestors};
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    fs::{self, File, OpenOptions},
    io::{self, Read, Write},
    path::PathBuf,
    sync::{
        Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

const STATE_FILE: &str = "window-state-v1.json";
const MAX_BYTES: u64 = 4096;
static WRITER: Mutex<()> = Mutex::new(());
static TEMP_ID: AtomicU64 = AtomicU64::new(0);

#[derive(Debug)]
pub enum StateError {
    InvalidPath,
    InvalidState,
    Disabled,
    Busy,
    Io(io::Error),
}

impl fmt::Display for StateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidPath => "invalid persistence directory",
            Self::InvalidState => "invalid window state",
            Self::Disabled => "window state writes disabled",
            Self::Busy => "window state writer unavailable",
            Self::Io(_) => "window state IO failed",
        })
    }
}
impl std::error::Error for StateError {}
impl From<io::Error> for StateError {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    schema_version: u8,
    bounds: WindowBounds,
}

pub struct WindowStateStore {
    directory: PathBuf,
    disabled: AtomicBool,
}

impl WindowStateStore {
    pub const fn new(directory: PathBuf) -> Self {
        Self {
            directory,
            disabled: AtomicBool::new(false),
        }
    }

    pub fn load(&mut self) -> Result<Option<WindowBounds>, StateError> {
        let _writer = WRITER.lock().map_err(|_| StateError::Busy)?;
        self.read_checked()
    }

    fn read_checked(&self) -> Result<Option<WindowBounds>, StateError> {
        let result = self.read();
        if result.is_err() {
            self.disabled.store(true, Ordering::Relaxed);
        }
        result
    }

    fn read(&self) -> Result<Option<WindowBounds>, StateError> {
        let path = self.directory.join(STATE_FILE);
        check_ancestors(&path)?;
        let file = match File::open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error.into()),
        };
        if !file.metadata()?.is_file() || file.metadata()?.len() > MAX_BYTES {
            return Err(StateError::InvalidState);
        }
        let mut bytes = Vec::new();
        file.take(MAX_BYTES + 1).read_to_end(&mut bytes)?;
        if bytes.len() > 4096 {
            return Err(StateError::InvalidState);
        }
        let envelope: Envelope =
            serde_json::from_slice(&bytes).map_err(|_| StateError::InvalidState)?;
        if envelope.schema_version != 1 {
            return Err(StateError::InvalidState);
        }
        validate_bounds(envelope.bounds)?;
        Ok(Some(envelope.bounds))
    }

    pub fn save(&self, bounds: WindowBounds) -> Result<(), StateError> {
        let _writer = WRITER.lock().map_err(|_| StateError::Busy)?;
        if self.disabled.load(Ordering::Relaxed) {
            return Err(StateError::Disabled);
        }
        validate_bounds(bounds)?;
        self.read_checked()?;
        check_ancestors(&self.directory)?;
        fs::create_dir_all(&self.directory)?;
        let lock_path = self.directory.join("window-state.lock");
        check_ancestors(&lock_path)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(lock_path)?;
        lock.try_lock().map_err(|_| StateError::Busy)?;
        self.read_checked()?;
        let bytes = serde_json::to_vec(&Envelope {
            schema_version: 1,
            bounds,
        })
        .map_err(|_| StateError::InvalidState)?;
        let (temp_path, mut temp) = self.create_temp()?;
        temp.write_all(&bytes)?;
        temp.flush()?;
        temp.sync_all()?;
        drop(temp);
        // std uses replacement rename on Windows; never unlink the previous state first.
        fs::rename(temp_path, self.directory.join(STATE_FILE))?;
        Ok(())
    }

    fn create_temp(&self) -> Result<(PathBuf, File), StateError> {
        for _ in 0..32 {
            let id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let path = self
                .directory
                .join(format!("{STATE_FILE}.{}.{id}.tmp", std::process::id()));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => return Ok((path, file)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error.into()),
            }
        }
        Err(StateError::Busy)
    }
}

fn validate_bounds(bounds: WindowBounds) -> Result<(), StateError> {
    if bounds.width == 0
        || bounds.height == 0
        || bounds.width > i32::MAX.unsigned_abs()
        || bounds.height > i32::MAX.unsigned_abs()
    {
        return Err(StateError::InvalidState);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_collisions_stop_after_32_without_altering_stale_files() {
        let directory = tempfile::tempdir().unwrap();
        let _writer = WRITER.lock().unwrap();
        let first = TEMP_ID.load(Ordering::Relaxed);
        let stale: Vec<_> = (first..first + 32)
            .map(|id| {
                let path = directory
                    .path()
                    .join(format!("{STATE_FILE}.{}.{id}.tmp", std::process::id()));
                fs::write(&path, b"stale temp content").unwrap();
                path
            })
            .collect();
        let store = WindowStateStore::new(directory.path().into());
        assert!(matches!(store.create_temp(), Err(StateError::Busy)));
        assert_eq!(TEMP_ID.load(Ordering::Relaxed), first + 32);
        assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 32);
        for path in stale {
            assert_eq!(fs::read(path).unwrap(), b"stale temp content");
        }
    }
}
