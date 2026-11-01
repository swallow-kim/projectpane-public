use super::{
    logger::{LogError, LogEvent},
    paths::check_ancestors,
};
use serde::Serialize;
use std::{
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::PathBuf,
    time::{SystemTime, UNIX_EPOCH},
};

pub(super) const MAX_LOG_BYTES: u64 = 64 * 1024;
pub(super) const LOG_NAMES: [&str; 3] = ["current.jsonl", "1.jsonl", "2.jsonl"];

pub(super) struct LogFiles {
    directory: PathBuf,
    _lock: File,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct Record<'a> {
    timestamp_ms: u64,
    #[serde(flatten)]
    event: &'a LogEvent,
}

impl LogFiles {
    pub(super) fn open(root: PathBuf) -> Result<Self, LogError> {
        let directory = root.join("logs");
        check_ancestors(&directory).map_err(|_| LogError::Io)?;
        fs::create_dir_all(&directory)?;
        let lock_path = root.join("logger.lock");
        check_ancestors(&lock_path).map_err(|_| LogError::Io)?;
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(lock_path)?;
        lock.try_lock().map_err(|_| LogError::Busy)?;
        let files = Self {
            directory,
            _lock: lock,
        };
        for name in LOG_NAMES {
            files.recover(name)?;
        }
        Ok(files)
    }

    fn recover(&self, name: &str) -> Result<(), LogError> {
        let path = self.directory.join(name);
        check_ancestors(&path).map_err(|_| LogError::Io)?;
        let file = match OpenOptions::new().read(true).write(true).open(path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error.into()),
        };
        let mut bytes = Vec::new();
        (&file).take(MAX_LOG_BYTES).read_to_end(&mut bytes)?;
        let end = bytes
            .iter()
            .rposition(|byte| *byte == b'\n')
            .map_or(0, |index| index + 1);
        let length = u64::try_from(end).map_err(|_| LogError::Io)?;
        if file.metadata()?.len() != length {
            file.set_len(length)?;
            file.sync_all()?;
        }
        Ok(())
    }

    pub(super) fn write(&self, event: &LogEvent) -> Result<(), LogError> {
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .and_then(|duration| u64::try_from(duration.as_millis()).ok())
            .unwrap_or(0);
        let mut bytes = serde_json::to_vec(&Record {
            timestamp_ms,
            event,
        })
        .map_err(|_| LogError::Io)?;
        bytes.push(b'\n');
        let length = u64::try_from(bytes.len()).map_err(|_| LogError::Io)?;
        if length > MAX_LOG_BYTES {
            return Err(LogError::Io);
        }
        let current = self.directory.join(LOG_NAMES[0]);
        check_ancestors(&current).map_err(|_| LogError::Io)?;
        let size = match fs::metadata(&current) {
            Ok(metadata) => metadata.len(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => 0,
            Err(error) => return Err(error.into()),
        };
        if size.saturating_add(length) > MAX_LOG_BYTES {
            self.rotate()?;
        }
        let mut file = OpenOptions::new().create(true).append(true).open(current)?;
        file.write_all(&bytes)?;
        file.flush()?;
        Ok(())
    }

    fn rotate(&self) -> Result<(), LogError> {
        for name in LOG_NAMES {
            check_ancestors(&self.directory.join(name)).map_err(|_| LogError::Io)?;
        }
        match fs::remove_file(self.directory.join(LOG_NAMES[2])) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(error.into()),
        }
        for index in (0..2).rev() {
            match fs::rename(
                self.directory.join(LOG_NAMES[index]),
                self.directory.join(LOG_NAMES[index + 1]),
            ) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }

    pub(super) fn sync(&self) -> Result<(), LogError> {
        for name in LOG_NAMES {
            match OpenOptions::new()
                .write(true)
                .open(self.directory.join(name))
            {
                Ok(file) => file.sync_all()?,
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
}
