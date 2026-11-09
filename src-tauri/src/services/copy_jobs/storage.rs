use super::*;
use serde::de::DeserializeOwned;
use std::os::windows::ffi::OsStrExt;
use std::{
    fs::OpenOptions,
    io::{BufRead, Read, Seek, SeekFrom, Write},
    os::windows::fs::{MetadataExt, OpenOptionsExt},
};
const RECORD_LIMIT: u64 = 256 * 1024;
#[derive(Serialize, Deserialize)]
#[serde(tag = "event", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum Event {
    Intent { source: String, destination: String },
    Outcome { item: ItemOutcome },
    RootDone,
    Finished { snapshot: Snapshot },
}
pub(super) fn check(path: &Path) -> Result<(), Fault> {
    for p in path.ancestors() {
        match fs::symlink_metadata(p) {
            Ok(m) if m.file_attributes() & 0x400 != 0 => {
                return Err(Fault::new(FaultCode::InvalidPath));
            }
            Ok(_) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.into()),
        }
    }
    Ok(())
}
pub(super) fn pin_directory(path: &Path) -> Result<Vec<File>, Fault> {
    let mut pins = Vec::new();
    for ancestor in path.ancestors() {
        check(ancestor)?;
        let pin = OpenOptions::new()
            .access_mode(0x80)
            .share_mode(1)
            .custom_flags(0x02000000 | 0x00200000)
            .open(ancestor)
            .map_err(Fault::from)?;
        if pin.metadata().map_err(Fault::from)?.file_attributes() & 0x400 != 0 {
            return Err(Fault::new(FaultCode::UnsafeReparsePoint));
        }
        pins.push(pin);
    }
    Ok(pins)
}
pub(super) fn open_file(path: &Path, new: bool) -> Result<File, Fault> {
    check(path)?;
    let mut options = OpenOptions::new();
    options
        .write(true)
        .read(true)
        .share_mode(1)
        .custom_flags(0x00200000);
    if new {
        options.create_new(true);
    } else {
        options.create(true).truncate(false);
    }
    options.open(path).map_err(Fault::from)
}
pub(super) fn append_file(path: &Path) -> Result<File, Fault> {
    check(path)?;
    OpenOptions::new()
        .append(true)
        .share_mode(1)
        .custom_flags(0x00200000)
        .open(path)
        .map_err(Fault::from)
}
pub(super) fn read_file(path: &Path) -> Result<File, Fault> {
    check(path)?;
    OpenOptions::new()
        .read(true)
        .share_mode(3)
        .custom_flags(0x00200000)
        .open(path)
        .map_err(Fault::from)
}
pub(super) fn record_bytes(value: &impl Serialize) -> Result<Vec<u8>, Fault> {
    let mut bytes = serde_json::to_vec(value).map_err(|_| Fault::new(FaultCode::InvalidState))?;
    if bytes.len() as u64 >= RECORD_LIMIT {
        return Err(Fault::new(FaultCode::PayloadLimit));
    }
    bytes.push(b'\n');
    Ok(bytes)
}
pub(super) fn write_record(file: &mut File, value: &impl Serialize) -> Result<(), Fault> {
    file.write_all(&record_bytes(value)?).map_err(Fault::from)?;
    file.sync_data().map_err(Fault::from)
}

pub(super) fn publish_header(directory: &Path, header: &Header) -> Result<(), Fault> {
    let pending = directory.join(format!("{}.pending", header.id));
    let target = directory.join(format!("{}.header", header.id));
    let mut file = open_file(&pending, true)?;
    write_record(&mut file, header)?;
    file.sync_all().map_err(Fault::from)?;
    drop(file);
    let source: Vec<u16> = pending.as_os_str().encode_wide().chain([0]).collect();
    let target: Vec<u16> = target.as_os_str().encode_wide().chain([0]).collect();
    // SAFETY: live NUL-terminated paths. No REPLACE_EXISTING flag: an existing header is never overwritten.
    if unsafe {
        windows_sys::Win32::Storage::FileSystem::MoveFileExW(source.as_ptr(), target.as_ptr(), 0x8)
    } == 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    Ok(())
}
pub(super) fn write_capture_batch(
    file: &mut File,
    sources: &[OperationSource],
) -> Result<(), Fault> {
    for source in sources {
        let mut bytes =
            serde_json::to_vec(source).map_err(|_| Fault::new(FaultCode::InvalidState))?;
        if bytes.len() as u64 >= RECORD_LIMIT {
            return Err(Fault::new(FaultCode::PayloadLimit));
        }
        bytes.push(b'\n');
        file.write_all(&bytes).map_err(Fault::from)?;
    }
    file.sync_data().map_err(Fault::from)
}
pub(super) fn read_record<T: DeserializeOwned>(
    reader: &mut impl BufRead,
) -> Result<Option<T>, Fault> {
    let mut line = Vec::new();
    let read = reader
        .take(RECORD_LIMIT)
        .read_until(b'\n', &mut line)
        .map_err(Fault::from)?;
    if read == 0 {
        return Ok(None);
    }
    if line.last() != Some(&b'\n') {
        return Err(Fault::new(FaultCode::InvalidState));
    }
    serde_json::from_slice(&line)
        .map(Some)
        .map_err(|_| Fault::new(FaultCode::InvalidState))
}
pub(super) fn read_one<T: DeserializeOwned>(path: &Path) -> Result<T, Fault> {
    let mut reader = BufReader::new(read_file(path)?);
    let value = read_record(&mut reader)?.ok_or_else(|| Fault::new(FaultCode::InvalidState))?;
    if !reader.fill_buf().map_err(Fault::from)?.is_empty() {
        return Err(Fault::new(FaultCode::InvalidState));
    }
    Ok(value)
}
fn unknown(source: String, destination: String) -> ItemOutcome {
    ItemOutcome {
        source,
        destination: Some(destination),
        status: ItemStatus::Unknown,
        hresult: i32::MIN,
    }
}
pub(super) fn recover(directory: &Path, header: &Header) -> Result<Snapshot, Fault> {
    recover_until(directory, header, None)
}
pub(super) fn recover_until(
    directory: &Path,
    header: &Header,
    durable_end: Option<u64>,
) -> Result<Snapshot, Fault> {
    let mut snapshot = initial(header);
    snapshot.status = CopyStatus::InterruptedUnknown;
    let file = read_file(&directory.join(format!("{}.journal", header.id)))?;
    let mut reader = BufReader::new(file.take(durable_end.unwrap_or(u64::MAX)));
    let mut pending = None;
    loop {
        let event = match read_record::<Event>(&mut reader) {
            Ok(Some(event)) => event,
            Ok(None) => break,
            Err(_) => {
                snapshot.journal_saved = false;
                break;
            }
        };
        match event {
            Event::Intent {
                source,
                destination,
            } => {
                if pending.is_some() {
                    snapshot.journal_saved = false;
                    break;
                }
                pending = Some((source, destination));
            }
            Event::Outcome { item } => {
                if let Some((source, destination)) = &pending
                    && (source != &item.source || item.destination.as_ref() != Some(destination))
                {
                    snapshot.journal_saved = false;
                    break;
                }
                pending = None;
                snapshot.counts.add(item.status);
                snapshot.outcome_count += 1;
            }
            Event::RootDone => {
                if pending.is_some() || snapshot.roots_completed >= snapshot.roots_total {
                    snapshot.journal_saved = false;
                    break;
                }
                snapshot.roots_completed += 1;
            }
            Event::Finished { snapshot: finished } => {
                if pending.is_some() {
                    snapshot.counts.unknown += 1;
                    snapshot.outcome_count += 1;
                    pending = None;
                }
                if finished.id == header.id
                    && finished.project_id == header.project_id
                    && finished.roots_total == header.capture.count
                    && finished.roots_completed == snapshot.roots_completed
                    && finished.outcome_count == snapshot.outcome_count
                    && finished.counts == snapshot.counts
                    && !finished.status.active()
                    && finished.roots_completed <= finished.roots_total
                    && (finished.status != CopyStatus::Succeeded
                        || (finished.roots_completed == finished.roots_total
                            && finished.counts.failed == 0
                            && finished.counts.unknown == 0
                            && finished.failure.is_none()
                            && finished.journal_saved))
                    && reader.fill_buf().map_err(Fault::from)?.is_empty()
                {
                    return Ok(finished);
                }
                snapshot.journal_saved = false;
                break;
            }
        }
    }
    if pending.is_some() {
        snapshot.counts.unknown += 1;
        snapshot.outcome_count += 1;
    }
    Ok(snapshot)
}
#[cfg(test)]
pub(super) fn read_page(
    directory: &Path,
    id: &str,
    offset: u64,
    limit: usize,
    terminal: bool,
) -> Result<OutcomePage, Fault> {
    read_page_until(directory, id, offset, limit, terminal, None)
}
pub(super) fn read_page_until(
    directory: &Path,
    id: &str,
    offset: u64,
    limit: usize,
    terminal: bool,
    durable_end: Option<u64>,
) -> Result<OutcomePage, Fault> {
    let mut file = read_file(&directory.join(format!("{id}.journal")))?;
    let physical_end = file.metadata().map_err(Fault::from)?.len();
    let end = durable_end.unwrap_or(physical_end).min(physical_end);
    if offset > end {
        return Err(Fault::new(FaultCode::InvalidRequest));
    }
    if offset > 0 {
        file.seek(SeekFrom::Start(offset - 1))
            .map_err(Fault::from)?;
        let mut previous = [0];
        file.read_exact(&mut previous).map_err(Fault::from)?;
        if previous[0] != b'\n' {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
    }
    file.seek(SeekFrom::Start(offset)).map_err(Fault::from)?;
    let mut reader = BufReader::new(file);
    let mut items = Vec::new();
    let mut bytes = 128usize;
    let mut next_offset = offset;
    let mut pending = None;
    let mut failure = None;
    while items.len() < limit {
        let before = reader.stream_position().map_err(Fault::from)?;
        let record = if before >= end {
            Ok(None)
        } else {
            read_record::<Event>(&mut reader)
        };
        match record {
            Ok(Some(Event::Intent {
                source,
                destination,
            })) => {
                pending = Some((before, source, destination));
            }
            Ok(Some(Event::Outcome { item })) => {
                let size = serde_json::to_vec(&item)
                    .map_err(|_| Fault::new(FaultCode::InvalidState))?
                    .len()
                    + 1;
                if bytes + size >= RECORD_LIMIT as usize {
                    if items.is_empty() {
                        return Err(Fault::new(FaultCode::PayloadLimit));
                    }
                    break;
                }
                pending = None;
                bytes += size;
                items.push(item);
                next_offset = reader.stream_position().map_err(Fault::from)?;
            }
            Ok(Some(Event::Finished { .. })) => {
                if let Some((_, source, destination)) = pending.take() {
                    let item = unknown(source, destination);
                    let size = serde_json::to_vec(&item)
                        .map_err(|_| Fault::new(FaultCode::InvalidState))?
                        .len()
                        + 1;
                    if bytes + size >= RECORD_LIMIT as usize {
                        if items.is_empty() {
                            return Err(Fault::new(FaultCode::PayloadLimit));
                        }
                        break;
                    }
                    items.push(item);
                }
                next_offset = reader.stream_position().map_err(Fault::from)?;
            }
            Ok(Some(Event::RootDone)) => {
                if pending.is_none() {
                    next_offset = reader.stream_position().map_err(Fault::from)?;
                }
            }
            Ok(None) => {
                if let Some((start, source, destination)) = pending {
                    if terminal {
                        let item = unknown(source, destination);
                        let size = serde_json::to_vec(&item)
                            .map_err(|_| Fault::new(FaultCode::InvalidState))?
                            .len()
                            + 1;
                        if bytes + size >= RECORD_LIMIT as usize {
                            if items.is_empty() {
                                return Err(Fault::new(FaultCode::PayloadLimit));
                            }
                            break;
                        }
                        items.push(item);
                        next_offset = end;
                    } else {
                        next_offset = start;
                    }
                }
                break;
            }
            Err(error) => {
                if terminal {
                    if let Some((_, source, destination)) = pending {
                        let item = unknown(source, destination);
                        let size = serde_json::to_vec(&item)
                            .map_err(|_| Fault::new(FaultCode::InvalidState))?
                            .len()
                            + 1;
                        if bytes + size >= RECORD_LIMIT as usize {
                            if items.is_empty() {
                                return Err(Fault::new(FaultCode::PayloadLimit));
                            }
                            break;
                        }
                        items.push(item);
                    }
                    failure = Some(error);
                    next_offset = end;
                }
                break;
            }
        }
    }
    Ok(OutcomePage {
        items,
        next_offset,
        done: terminal && next_offset >= end,
        failure,
    })
}
