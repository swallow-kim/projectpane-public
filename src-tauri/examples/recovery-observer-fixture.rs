#[cfg(windows)]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use projectpane_core::{
        domain::operations::recovery::RecoveryRecord, platform::file_operations::capture_source,
    };
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};
    use std::{
        fs::{self, OpenOptions},
        io::{self, Write},
        path::Path,
    };

    fn object(path: &Path) -> Result<Value, Box<dyn std::error::Error>> {
        let text = path
            .to_str()
            .ok_or_else(|| io::Error::other("unicode_path_required"))?;
        let object =
            capture_source(text).map_err(|_| io::Error::other("fixture_identity_failed"))?;
        Ok(serde_json::to_value(object)?)
    }
    fn captured(path: &Path) -> Result<Value, Box<dyn std::error::Error>> {
        let bytes = fs::read(path)?;
        let sha: [u8; 32] = Sha256::digest(&bytes).into();
        Ok(json!({"object":object(path)?,"content":{"bytes":bytes.len(),"sha256":sha}}))
    }
    let path = std::env::args_os()
        .nth(1)
        .ok_or_else(|| io::Error::other("new_fixture_directory_required"))?;
    let root = Path::new(&path);
    if !root.is_absolute()
        || root
            .components()
            .any(|part| matches!(part, std::path::Component::ParentDir))
    {
        return Err(io::Error::other("absolute_fixture_required").into());
    }
    let parent = root
        .parent()
        .ok_or_else(|| io::Error::other("fixture_parent_required"))?;
    let parent_object = object(parent)?;
    let canonical_parent = parent_object["path"]
        .as_str()
        .ok_or_else(|| io::Error::other("fixture_parent_required"))?;
    let leaf = root
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| io::Error::other("fixture_leaf_required"))?;
    projectpane_core::domain::operations::validate_leaf_name(leaf)
        .map_err(|_| io::Error::other("invalid_fixture_leaf"))?;
    let root = Path::new(canonical_parent).join(leaf);
    fs::create_dir(&root)?;
    let target_dir = root.join("target");
    let artifacts = target_dir.join(".projectpane-recovery-proof-1");
    fs::create_dir(&target_dir)?;
    fs::create_dir(&artifacts)?;
    let source = root.join("source");
    let target = target_dir.join("한글.txt");
    let staged = artifacts.join("staged");
    for (path, bytes) in [
        (&source, b"abc".as_slice()),
        (&target, b"original".as_slice()),
        (&staged, b"abc".as_slice()),
    ] {
        let mut file = OpenOptions::new().write(true).create_new(true).open(path)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    let artifact_object = object(&artifacts)?;
    let artifact_path = artifact_object["path"]
        .as_str()
        .ok_or_else(|| io::Error::other("artifact_path_required"))?;
    let backup_path = format!("{artifact_path}\\original");
    let value = json!({"schemaVersion":1,"id":"proof-1","phase":"ready",
        "source":captured(&source)?,"original":captured(&target)?,"staged":captured(&staged)?,
        "parent":object(&target_dir)?,"artifacts":artifact_object,"backupPath":backup_path});
    let record = serde_json::to_vec(&value)?;
    RecoveryRecord::parse(&record).map_err(|_| io::Error::other("invalid_fixture_record"))?;
    std::io::stdout().write_all(&record)?;
    Ok(())
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Windows required");
    std::process::exit(1);
}
