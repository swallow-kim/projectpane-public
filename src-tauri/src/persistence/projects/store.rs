use super::atomic;
use crate::persistence::resolve_data_root;
use std::{
    ffi::OsStr,
    fs::{self, File, OpenOptions},
    io,
    path::{Path, PathBuf},
    sync::Mutex,
};

static WRITER: Mutex<()> = Mutex::new(());

pub struct ProjectStore {
    directory: PathBuf,
}

pub struct Transaction<'a> {
    directory: &'a Path,
    _lock: File,
}

impl ProjectStore {
    pub fn new(default: &Path, override_path: Option<&OsStr>) -> io::Result<Self> {
        let foundation = resolve_data_root(default, override_path)
            .map_err(|_| io::Error::new(io::ErrorKind::PermissionDenied, "unsafe_data_root"))?;
        let directory = foundation.join("projects-v1");
        atomic::check(&directory)?;
        fs::create_dir_all(&directory)?;
        atomic::check(&directory)?;
        Ok(Self { directory })
    }

    /// A nonblocking process lock plus an OS file lock bounds the request queue.
    pub fn transaction<T>(
        &self,
        operation: impl FnOnce(&Transaction<'_>) -> io::Result<T>,
    ) -> io::Result<T> {
        let _writer = WRITER
            .try_lock()
            .map_err(|_| io::Error::new(io::ErrorKind::WouldBlock, "writer_busy"))?;
        let path = self.directory.join("catalog.lock");
        atomic::check(&path)?;
        let lock = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        lock.try_lock()
            .map_err(|_| io::Error::new(io::ErrorKind::WouldBlock, "writer_busy"))?;
        operation(&Transaction {
            directory: &self.directory,
            _lock: lock,
        })
    }
}

impl Transaction<'_> {
    pub fn read(&self, name: &str) -> io::Result<Option<Vec<u8>>> {
        atomic::read(&self.path(name)?)
    }

    pub fn read_backup(&self, name: &str) -> io::Result<Option<Vec<u8>>> {
        atomic::read(&self.path(name)?.with_extension("json.bak"))
    }

    pub fn write(
        &self,
        name: &str,
        bytes: &[u8],
        validate: impl Fn(&[u8]) -> io::Result<()>,
    ) -> io::Result<()> {
        atomic::write_json(&self.path(name)?, bytes, validate)
    }

    fn path(&self, name: &str) -> io::Result<PathBuf> {
        let valid = matches!(
            name,
            "manifest.json" | "settings.json" | "favorites.json" | "root-edit.json"
        ) || [".project.json", ".session.json"].iter().any(|suffix| {
            name.strip_suffix(suffix).is_some_and(|id| {
                id.len() == 36
                    && id.bytes().enumerate().all(|(i, byte)| {
                        if [8, 13, 18, 23].contains(&i) {
                            byte == b'-'
                        } else {
                            byte.is_ascii_hexdigit() && !byte.is_ascii_uppercase()
                        }
                    })
            })
        });
        if !valid {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "invalid_document_name",
            ));
        }
        Ok(self.directory.join(name))
    }
}
