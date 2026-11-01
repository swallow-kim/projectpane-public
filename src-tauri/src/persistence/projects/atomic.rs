use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

pub(super) const MAX_BYTES: usize = 256 * 1024;
static TEMP_ID: AtomicU64 = AtomicU64::new(0);

pub(super) fn read(path: &Path) -> io::Result<Option<Vec<u8>>> {
    check(path)?;
    let file = match File::open(path) {
        Ok(file) => file,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error),
    };
    let metadata = file.metadata()?;
    if !metadata.is_file() || metadata.len() > MAX_BYTES as u64 {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "document_size"));
    }
    let mut bytes = Vec::new();
    file.take(MAX_BYTES as u64 + 1).read_to_end(&mut bytes)?;
    if bytes.len() > MAX_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "document_size"));
    }
    Ok(Some(bytes))
}

pub(super) fn check(path: &Path) -> io::Result<()> {
    super::super::paths::check_ancestors(path)
        .map_err(|_| io::Error::new(io::ErrorKind::PermissionDenied, "unsafe_persistence_path"))
}

/// Caller holds the catalog lock and validates the exact document schema.
/// Originals and backups that fail validation are never overwritten.
pub(super) fn write_json(
    path: &Path,
    bytes: &[u8],
    validate: impl Fn(&[u8]) -> io::Result<()>,
) -> io::Result<()> {
    if bytes.len() > MAX_BYTES {
        return Err(io::Error::new(io::ErrorKind::InvalidData, "document_size"));
    }
    validate(bytes)?;
    if let Some(original) = read(path)? {
        validate(&original)?;
        let backup = path.with_extension("json.bak");
        let retained = if let Some(previous) = read(&backup)? {
            validate(&previous)?;
            Some(prepare_bytes(&backup, &previous)?)
        } else {
            None
        };
        replace_bytes(&backup, &original)?;
        let result = replace_bytes(path, bytes);
        if let Some(retained) = retained {
            if result.is_err() {
                // If rollback is also denied, retain the flushed prior generation
                // at our unique temporary path instead of losing recovery evidence.
                let _ = super::native::replace(&retained, &backup);
            } else {
                let _ = std::fs::remove_file(retained);
            }
        }
        return result;
    }
    replace_bytes(path, bytes)
}

fn replace_bytes(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let temp_path = prepare_bytes(path, bytes)?;
    check(path)?;
    // Keep our recoverable temporary if replacement fails. Never adopt or sweep stale files.
    super::native::replace(&temp_path, path)
}

fn prepare_bytes(path: &Path, bytes: &[u8]) -> io::Result<PathBuf> {
    check(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::other("missing_parent"))?;
    let mut temporary = Temporary::create(parent)?;
    temporary.file.write_all(bytes)?;
    temporary.file.flush()?;
    temporary.file.sync_all()?;
    let Temporary {
        path: temp_path,
        file,
    } = temporary;
    drop(file);
    Ok(temp_path)
}

struct Temporary {
    path: PathBuf,
    file: File,
}

impl Temporary {
    fn create(parent: &Path) -> io::Result<Self> {
        for _ in 0..32 {
            let id = TEMP_ID.fetch_add(1, Ordering::Relaxed);
            let path = parent.join(format!(".projects-{}-{id}.tmp", std::process::id()));
            match OpenOptions::new().write(true).create_new(true).open(&path) {
                Ok(file) => return Ok(Self { path, file }),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::WouldBlock,
            "temp_collision_limit",
        ))
    }
}
