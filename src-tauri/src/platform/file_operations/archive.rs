use super::{
    archive_pipe::ArchivePipe, copy_tree, guarded_io as guard, identity::recheck,
    preflight::absent, zip_directory,
};
use crate::domain::{EntryKind, Fault, FaultCode, MAX_FRAME, operations::*};
use serde::Serialize;
use std::{
    collections::{HashMap, HashSet},
    fs::{self, File, OpenOptions},
    io::{Read, Write},
    path::Path,
    sync::atomic::{AtomicBool, Ordering},
};

pub(super) fn prepare(
    roots: Vec<OperationSource>,
    destination: OperationSource,
    name: String,
    extract: bool,
) -> Result<PreparedAction, Fault> {
    let mut descendants = Vec::new();
    let mut pending: Vec<_> = if extract {
        vec![]
    } else {
        roots
            .iter()
            .filter(|s| s.kind == EntryKind::Folder)
            .cloned()
            .collect()
    };
    let mut bytes = 4096
        + serde_json::to_vec(&roots)
            .map_err(|_| Fault::new(FaultCode::InvalidRequest))?
            .len();
    while let Some(folder) = pending.pop() {
        recheck(&folder)?;
        for child in fs::read_dir(&folder.path).map_err(Fault::from)? {
            if roots.len() + descendants.len() >= MAX_OPERATION_RESULTS {
                return Err(Fault::new(FaultCode::ResourceLimit));
            }
            let path = child.map_err(Fault::from)?.path();
            let source = super::capture_source(
                path.to_str()
                    .ok_or_else(|| Fault::new(FaultCode::InvalidPath))?,
            )?;
            bytes += serde_json::to_vec(&source)
                .map_err(|_| Fault::new(FaultCode::InvalidRequest))?
                .len()
                + 1;
            if bytes >= MAX_FRAME {
                return Err(Fault::new(FaultCode::PayloadLimit));
            }
            if source.kind == EntryKind::Folder {
                pending.push(source.clone());
            }
            descendants.push(source);
        }
    }
    check(&roots, &descendants, &destination, &name, extract)?;
    Ok(PreparedAction::Archive {
        roots,
        descendants,
        destination,
        name,
        extract,
    })
}
pub(super) fn check(
    roots: &[OperationSource],
    descendants: &[OperationSource],
    destination: &OperationSource,
    name: &str,
    extract: bool,
) -> Result<(), Fault> {
    validate_leaf_name(name)?;
    recheck(destination)?;
    let output = Path::new(&destination.path).join(name);
    absent(&output)?;
    let mut total = 0u64;
    let mut names = HashSet::new();
    for source in roots.iter().chain(descendants) {
        recheck(source)?;
        if source.kind == EntryKind::Folder {
            if output.starts_with(&source.path) {
                return Err(Fault::new(FaultCode::InvalidRequest));
            }
            copy_tree::check_children(source, descendants)?;
        } else {
            total = total
                .checked_add(fs::metadata(&source.path).map_err(Fault::from)?.len())
                .ok_or_else(|| Fault::new(FaultCode::ResourceLimit))?;
        }
        if !extract && !names.insert(entry_name(source, roots)?.to_lowercase()) {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
    }
    if total > zip_directory::MAX_BYTES {
        return Err(Fault::new(FaultCode::ResourceLimit));
    }
    Ok(())
}
fn entry_name(source: &OperationSource, roots: &[OperationSource]) -> Result<String, Fault> {
    let root = roots
        .iter()
        .find(|r| {
            source.path == r.path
                || (r.kind == EntryKind::Folder && Path::new(&source.path).starts_with(&r.path))
        })
        .ok_or_else(|| Fault::new(FaultCode::InvalidPath))?;
    let parent = Path::new(&root.path)
        .parent()
        .ok_or_else(|| Fault::new(FaultCode::InvalidPath))?;
    let name = Path::new(&source.path)
        .strip_prefix(parent)
        .map_err(|_| Fault::new(FaultCode::InvalidPath))?
        .to_str()
        .ok_or_else(|| Fault::new(FaultCode::InvalidPath))?
        .replace('\\', "/");
    zip_directory::validate_name(&name)?;
    Ok(name)
}
#[derive(Serialize)]
struct Input<'a> {
    path: &'a str,
    name: String,
    folder: bool,
}
pub(super) fn execute(
    roots: &[OperationSource],
    descendants: &[OperationSource],
    destination: &OperationSource,
    name: &str,
    extract: bool,
    cancel: &AtomicBool,
    progress: &super::NativeProgress,
) -> Result<OperationReport, Fault> {
    let sources: Vec<_> = roots.iter().chain(descendants).collect();
    let mut handles = sources
        .iter()
        .map(|s| guard::open(s, false))
        .collect::<Result<Vec<_>, _>>()?;
    let skip: HashSet<_> = sources
        .iter()
        .filter(|s| s.kind == EntryKind::Folder)
        .map(|s| s.path.to_lowercase())
        .collect();
    let parents: Vec<_> = roots
        .iter()
        .filter_map(|s| Path::new(&s.path).parent())
        .chain([Path::new(&destination.path)])
        .collect();
    let _ancestors = guard::ancestors(&parents, &skip)?;
    check(roots, descendants, destination, name, extract)?;
    let parent = guard::open(destination, false)?;
    let zone = if extract {
        zone(&roots[0].path)?
    } else {
        let mut marked = false;
        for source in &sources {
            if source.kind == EntryKind::File {
                marked |= zone(&source.path)?.is_some();
            }
        }
        marked.then(|| b"[ZoneTransfer]\r\nZoneId=3\r\n".to_vec())
    };
    let entries = if extract {
        zip_directory::entries(&mut handles[0])?
    } else {
        vec![]
    };
    let inputs: Vec<_> = sources
        .iter()
        .map(|s| {
            Ok(Input {
                path: &s.path,
                name: format!(
                    "{}{}",
                    entry_name(s, roots)?,
                    if s.kind == EntryKind::Folder { "/" } else { "" }
                ),
                folder: s.kind == EntryKind::Folder,
            })
        })
        .collect::<Result<_, Fault>>()?;
    let request = if extract {
        serde_json::json!({"extract":true,"source":roots[0].path,"count":entries.len()})
    } else {
        serde_json::json!({"extract":false,"items":inputs})
    };
    if cancel.load(Ordering::Acquire) {
        return Err(Fault::new(FaultCode::Cancelled));
    }
    let mut pipe = ArchivePipe::start(&request, cancel)?;
    // The exclusive native creation is the first destination mutation.
    let mut target = guard::create(&parent, name, extract)?;
    let target_path = Path::new(&destination.path).join(name);
    let result = (|| -> Result<(), Fault> {
        if extract {
            let mut dirs =
                HashMap::from([(String::new(), target.try_clone().map_err(Fault::from)?)]);
            for (index, entry) in entries.iter().enumerate() {
                if cancel.load(Ordering::Acquire) {
                    return Err(Fault::new(FaultCode::Cancelled));
                }
                let parts: Vec<_> = entry.name.split('/').collect();
                let depth = if entry.folder {
                    parts.len()
                } else {
                    parts.len() - 1
                };
                let mut key = String::new();
                for part in &parts[..depth] {
                    let next = if key.is_empty() {
                        part.to_string()
                    } else {
                        format!("{key}/{part}")
                    }
                    .to_lowercase();
                    if !dirs.contains_key(&next) {
                        let directory = guard::create(&dirs[&key], part, true)?;
                        dirs.insert(next.clone(), directory);
                    }
                    key = next;
                }
                if !entry.folder {
                    let mut output = guard::create(&dirs[&key], parts.last().unwrap(), false)?;
                    // Preserve download provenance before writing any file bytes.
                    mark(
                        &target_path.join(entry.name.replace('/', "\\")),
                        zone.as_deref(),
                    )?;
                    let mut remain = entry.bytes;
                    let mut crc = crc32fast::Hasher::new();
                    let mut buf = [0u8; 65536];
                    while remain > 0 {
                        let count = (remain as usize).min(buf.len());
                        pipe.read_exact(&mut buf[..count]).map_err(Fault::from)?;
                        output.write_all(&buf[..count]).map_err(Fault::from)?;
                        crc.update(&buf[..count]);
                        remain -= count as u64;
                    }
                    output.sync_all().map_err(Fault::from)?;
                    if crc.finalize() != entry.crc {
                        return Err(Fault::new(FaultCode::IoError));
                    }
                }
                progress.update(entries.len().max(1) as u32, (index + 1) as u32);
            }
        } else {
            mark(&target_path, zone.as_deref())?;
            let mut bytes = 0u64;
            let mut buf = [0u8; 65536];
            loop {
                let n = pipe.read(&mut buf).map_err(Fault::from)?;
                if n == 0 {
                    break;
                }
                bytes += n as u64;
                if bytes > zip_directory::MAX_BYTES {
                    return Err(Fault::new(FaultCode::ResourceLimit));
                }
                target.write_all(&buf[..n]).map_err(Fault::from)?;
            }
            target.sync_all().map_err(Fault::from)?;
            let observed = zip_directory::entries(&mut target)?;
            #[cfg(test)]
            eprintln!(
                "zip created {} entries, expected {}",
                observed.len(),
                inputs.len()
            );
            if observed.len() != inputs.len() {
                return Err(Fault::new(FaultCode::ProtocolError));
            }
        }
        pipe.finish()?;
        Ok(())
    })();
    #[cfg(test)]
    if let Err(error) = &result {
        eprintln!("archive execution fault: {error:?}");
    }
    let succeeded = result.is_ok();
    let cancelled = !succeeded && cancel.load(Ordering::Acquire);
    Ok(OperationReport {
        failure: None,
        user_skipped: vec![],
        user_decisions_only: None,
        counts: OutcomeCounts {
            changed: u32::from(succeeded),
            unknown: u32::from(!succeeded),
            ..Default::default()
        },
        items: vec![ItemOutcome {
            source: roots[0].path.clone(),
            destination: Some(target_path.to_string_lossy().into_owned()),
            status: if succeeded {
                ItemStatus::Changed
            } else {
                ItemStatus::Unknown
            },
            hresult: if succeeded { 0 } else { -1 },
        }],
        evidence: FinishEvidence {
            perform_succeeded: succeeded,
            aborted: Some(cancelled),
            outcomes_complete: true,
            cancellation_confirmed: cancelled,
        },
    })
}

fn zone(path: &str) -> Result<Option<Vec<u8>>, Fault> {
    let file = match File::open(format!("{path}:Zone.Identifier")) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.into()),
    };
    let mut data = vec![];
    file.take(65537)
        .read_to_end(&mut data)
        .map_err(Fault::from)?;
    if data.len() > 65536 {
        return Err(Fault::new(FaultCode::ResourceLimit));
    }
    Ok(Some(data))
}
fn mark(path: &Path, zone: Option<&[u8]>) -> Result<(), Fault> {
    if let Some(zone) = zone {
        // Main file is held without delete sharing and every parent is pinned.
        // Failure to preserve the zone is a failed extraction, never a fallback.
        let mut stream = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(format!("{}:Zone.Identifier", path.display()))
            .map_err(Fault::from)?;
        stream.write_all(zone).map_err(Fault::from)?;
        stream.sync_all().map_err(Fault::from)?;
    }
    Ok(())
}

#[cfg(test)]
#[path = "archive_tests.rs"]
mod tests;
