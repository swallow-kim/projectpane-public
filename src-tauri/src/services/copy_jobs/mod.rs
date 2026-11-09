//! Copy-only durable, bounded-record jobs. Never replay an interrupted mutation.
#[cfg(test)]
mod benchmark_tests;
mod storage;
#[cfg(test)]
mod tests;
mod writer;
use crate::{
    domain::{
        EntryKind, Fault, FaultCode,
        operations::{ItemOutcome, ItemStatus, OperationSource, valid_token, validate_leaf_name},
    },
    platform::file_operations::{capture_source, copy_root},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsStr,
    fs::{self, File},
    io::BufReader,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use storage::*;
pub const CHUNK_ITEMS: usize = 128;
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Capture {
    pub id: String,
    pub count: u64,
    pub sealed: bool,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CopyStatus {
    Running,
    CancelRequested,
    Succeeded,
    Failed,
    Cancelled,
    InterruptedUnknown,
}
impl CopyStatus {
    pub fn active(self) -> bool {
        matches!(self, Self::Running | Self::CancelRequested)
    }
}
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Counts {
    pub changed: u64,
    pub skipped: u64,
    pub failed: u64,
    pub unknown: u64,
}
impl Counts {
    fn add(&mut self, status: ItemStatus) {
        match status {
            ItemStatus::Changed => self.changed += 1,
            ItemStatus::Skipped => self.skipped += 1,
            ItemStatus::Failed => self.failed += 1,
            ItemStatus::Unknown => self.unknown += 1,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Snapshot {
    pub id: String,
    pub project_id: Option<String>,
    pub status: CopyStatus,
    pub counts: Counts,
    pub roots_total: u64,
    pub roots_completed: u64,
    pub outcome_count: u64,
    pub journal_saved: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<Fault>,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictPolicy {
    Fail,
    Skip,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BeginJob {
    pub capture_id: String,
    pub destination: String,
    pub project_id: Option<String>,
    pub conflict_policy: ConflictPolicy,
    pub name: Option<String>,
}
#[derive(Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Header {
    schema: u32,
    id: String,
    capture: Capture,
    destination: OperationSource,
    project_id: Option<String>,
    conflict_policy: ConflictPolicy,
    name: Option<String>,
}
struct Job {
    snapshot: Snapshot,
    durable_end: Option<u64>,
    cancel: Arc<AtomicBool>,
}
struct State {
    busy_captures: BTreeSet<String>,
    referenced_captures: BTreeSet<String>,
    captures: BTreeMap<String, Capture>,
    jobs: BTreeMap<String, Job>,
    closing: bool,
}
pub struct CopyJobs {
    #[cfg(test)]
    legacy_sync: AtomicBool,
    #[cfg(test)]
    journal_syncs: std::sync::atomic::AtomicU64,
    directory: PathBuf,
    _lease: File,
    _pins: Vec<File>,
    state: Mutex<State>,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OutcomePage {
    pub items: Vec<ItemOutcome>,
    pub next_offset: u64,
    pub done: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub failure: Option<Fault>,
}
impl CopyJobs {
    pub fn open(default: &Path, over: Option<&OsStr>) -> Result<Arc<Self>, Fault> {
        let root = crate::persistence::resolve_data_root(default, over)
            .map_err(|_| Fault::new(FaultCode::InvalidPath))?;
        let directory = root.join("copy-jobs-v1");
        check(&directory)?;
        fs::create_dir_all(&directory)?;
        check(&directory)?;
        let pins = pin_directory(&directory)?;
        let lease = open_file(&directory.join("jobs.lock"), false)?;
        lease.try_lock().map_err(|_| Fault::new(FaultCode::Busy))?;
        let mut jobs = BTreeMap::new();
        for entry in fs::read_dir(&directory).map_err(Fault::from)? {
            let entry = entry.map_err(Fault::from)?;
            let name = entry.file_name();
            let Some(name) = name.to_str() else { continue };
            let Some(id) = name.strip_suffix(".header") else {
                continue;
            };
            if !valid_token(id) {
                return Err(Fault::new(FaultCode::InvalidState));
            }
            if jobs.len() >= 256 {
                let first = jobs.keys().next().cloned().unwrap();
                jobs.remove(&first);
            }
            // A damaged historical record must not disable unrelated future copies.
            // Keep all evidence, expose an unknown diagnostic, and never replay it.
            let snapshot = match read_one::<Header>(&entry.path()) {
                Ok(header) if valid_header(&header, id) => recover(&directory, &header)
                    .unwrap_or_else(|error| damaged(id, Some(&header), error)),
                Ok(_) => damaged(id, None, Fault::new(FaultCode::InvalidState)),
                Err(error) => damaged(id, None, error),
            };
            jobs.insert(
                id.to_owned(),
                Job {
                    snapshot,
                    durable_end: None,
                    cancel: Arc::new(AtomicBool::new(false)),
                },
            );
        }
        Ok(Arc::new(Self {
            #[cfg(test)]
            legacy_sync: AtomicBool::new(false),
            #[cfg(test)]
            journal_syncs: std::sync::atomic::AtomicU64::new(0),
            directory,
            _lease: lease,
            _pins: pins,
            state: Mutex::new(State {
                busy_captures: BTreeSet::new(),
                referenced_captures: BTreeSet::new(),
                captures: BTreeMap::new(),
                jobs,
                closing: false,
            }),
        }))
    }
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, State>, Fault> {
        self.state
            .lock()
            .map_err(|_| Fault::new(FaultCode::InvalidState))
    }
    pub fn begin_capture(&self) -> Result<Capture, Fault> {
        let mut state = self.lock()?;
        if state.closing {
            return Err(Fault::new(FaultCode::Busy));
        }
        if state.captures.len() >= 32 {
            return Err(Fault::new(FaultCode::ResourceLimit));
        }
        let id = crate::services::id::new_id().map_err(Fault::from)?;
        let value = Capture {
            id: id.clone(),
            count: 0,
            sealed: false,
        };
        let _ = open_file(&self.directory.join(format!("{id}.sources")), true)?;
        state.captures.insert(id, value.clone());
        Ok(value)
    }
    pub fn append(&self, id: &str, paths: Vec<String>) -> Result<Capture, Fault> {
        if paths.is_empty()
            || paths.len() > CHUNK_ITEMS
            || paths.iter().map(String::len).sum::<usize>() > 65536
        {
            return Err(Fault::new(FaultCode::ResourceLimit));
        }
        {
            let mut state = self.lock()?;
            if state.closing || state.busy_captures.contains(id) {
                return Err(Fault::new(FaultCode::Busy));
            }
            let capture = state
                .captures
                .get(id)
                .ok_or_else(|| Fault::new(FaultCode::StaleRequest))?;
            if capture.sealed {
                return Err(Fault::new(FaultCode::InvalidState));
            }
            state.busy_captures.insert(id.into());
        }
        // Network identity reads never hold the global progress/cancel lock.
        let result = (|| {
            let sources = paths
                .iter()
                .map(|p| capture_source(p))
                .collect::<Result<Vec<_>, _>>()?;
            let mut file = append_file(&self.directory.join(format!("{id}.sources")))?;
            write_capture_batch(&mut file, &sources)?;
            Ok(sources.len() as u64)
        })();
        let mut state = self.lock()?;
        state.busy_captures.remove(id);
        let capture = state
            .captures
            .get_mut(id)
            .ok_or_else(|| Fault::new(FaultCode::StaleRequest))?;
        match result {
            Ok(count) => {
                capture.count += count;
                Ok(capture.clone())
            }
            Err(error) => {
                capture.count = 0;
                capture.sealed = true;
                Err(error)
            }
        }
    }
    /// A command's resolved paths can be longer than its entry IDs. Split only
    /// capture-manifest writes by bytes; no destination mutation occurs here.
    pub fn append_bounded(&self, id: &str, paths: Vec<String>) -> Result<Capture, Fault> {
        if paths.is_empty() || paths.len() > CHUNK_ITEMS {
            return Err(Fault::new(FaultCode::ResourceLimit));
        }
        let result = (|| {
            let mut current = Vec::new();
            let mut bytes = 0;
            let mut last = None;
            for path in paths {
                if !current.is_empty() && bytes + path.len() > 65536 {
                    last = Some(self.append(id, std::mem::take(&mut current))?);
                    bytes = 0;
                }
                bytes += path.len();
                current.push(path);
            }
            if !current.is_empty() {
                last = Some(self.append(id, current)?);
            }
            last.ok_or_else(|| Fault::new(FaultCode::InvalidRequest))
        })();
        if result.is_err() {
            let _ = self.discard_capture(id);
        }
        result
    }
    pub fn seal(&self, id: &str) -> Result<Capture, Fault> {
        let mut state = self.lock()?;
        if state.busy_captures.contains(id) {
            return Err(Fault::new(FaultCode::Busy));
        }
        let capture = state
            .captures
            .get_mut(id)
            .ok_or_else(|| Fault::new(FaultCode::StaleRequest))?;
        if capture.count == 0 {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
        capture.sealed = true;
        Ok(capture.clone())
    }
    pub fn discard_capture(&self, id: &str) -> Result<(), Fault> {
        let mut state = self.lock()?;
        if state.busy_captures.contains(id) {
            return Err(Fault::new(FaultCode::Busy));
        }
        state
            .captures
            .remove(id)
            .ok_or_else(|| Fault::new(FaultCode::NotFound))?;
        if !state.referenced_captures.remove(id) {
            let path = self.directory.join(format!("{id}.sources"));
            check(&path)?;
            fs::remove_file(path).map_err(Fault::from)?;
        }
        // Durable jobs retain their source manifest; never replay or remove evidence.
        Ok(())
    }
    pub fn export_paths(&self, id: &str) -> Result<Vec<String>, Fault> {
        let capture = self
            .lock()?
            .captures
            .get(id)
            .filter(|c| c.sealed)
            .cloned()
            .ok_or_else(|| Fault::new(FaultCode::StaleRequest))?;
        let mut reader = BufReader::new(read_file(&self.directory.join(format!("{id}.sources")))?);
        let mut paths = Vec::new();
        let mut bytes = 0usize;
        for _ in 0..capture.count {
            let source: OperationSource =
                read_record(&mut reader)?.ok_or_else(|| Fault::new(FaultCode::InvalidState))?;
            if capture_source(&source.path)? != source {
                return Err(Fault::new(FaultCode::StaleRequest));
            }
            bytes = bytes
                .checked_add(source.path.len())
                .ok_or_else(|| Fault::new(FaultCode::ResourceLimit))?;
            if bytes > 16 * 1024 * 1024 {
                return Err(Fault::new(FaultCode::PayloadLimit));
            }
            paths.push(source.path);
        }
        Ok(paths)
    }
    pub fn list(&self) -> Result<Vec<Snapshot>, Fault> {
        Ok(self
            .lock()?
            .jobs
            .values()
            .map(|j| j.snapshot.clone())
            .collect())
    }
    pub fn snapshot(&self, id: &str) -> Result<Snapshot, Fault> {
        if !valid_token(id) {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
        if let Some(snapshot) = self.lock()?.jobs.get(id).map(|j| j.snapshot.clone()) {
            return Ok(snapshot);
        }
        let header: Header = read_one(&self.directory.join(format!("{id}.header")))?;
        recover(&self.directory, &header)
    }
    pub fn active(&self) -> bool {
        self.lock().map_or(true, |s| {
            s.jobs.values().any(|j| j.snapshot.status.active())
        })
    }
    pub fn close(&self, cancel: bool) -> Result<(), Fault> {
        let mut state = self.lock()?;
        state.closing = true;
        if cancel {
            for job in state.jobs.values_mut() {
                if job.snapshot.status.active() {
                    job.cancel.store(true, Ordering::Release);
                    job.snapshot.status = CopyStatus::CancelRequested;
                }
            }
        }
        Ok(())
    }
    pub fn keep_open(&self) -> Result<(), Fault> {
        self.lock()?.closing = false;
        Ok(())
    }
    pub fn cancel(&self, id: &str) -> Result<Snapshot, Fault> {
        let mut state = self.lock()?;
        let job = state
            .jobs
            .get_mut(id)
            .ok_or_else(|| Fault::new(FaultCode::NotFound))?;
        if job.snapshot.status.active() {
            job.cancel.store(true, Ordering::Release);
            job.snapshot.status = CopyStatus::CancelRequested;
        }
        Ok(job.snapshot.clone())
    }
    pub fn begin(self: &Arc<Self>, request: BeginJob) -> Result<Snapshot, Fault> {
        if request.project_id.as_ref().is_some_and(|s| !valid_token(s)) {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
        if let Some(name) = &request.name {
            validate_leaf_name(name)?;
        }
        let destination = capture_source(&request.destination)?;
        if destination.kind != EntryKind::Folder {
            return Err(Fault::new(FaultCode::InvalidPath));
        }
        let captured = self
            .lock()?
            .captures
            .get(&request.capture_id)
            .filter(|c| c.sealed && c.count > 0)
            .cloned()
            .ok_or_else(|| Fault::new(FaultCode::StaleRequest))?;
        // Bounded root-only preflight preserves the synchronous collision decision UI.
        // No descendants are enumerated and no destination bytes change here.
        let mut reader = BufReader::new(read_file(
            &self.directory.join(format!("{}.sources", captured.id)),
        )?);
        for _ in 0..captured.count {
            let source: OperationSource =
                read_record(&mut reader)?.ok_or_else(|| Fault::new(FaultCode::InvalidState))?;
            let leaf = if let Some(name) = request.name.as_deref() {
                name
            } else {
                Path::new(&source.path)
                    .file_name()
                    .and_then(|s| s.to_str())
                    .ok_or_else(|| Fault::new(FaultCode::InvalidPath))?
            };
            validate_leaf_name(leaf)?;
            let target = Path::new(&destination.path).join(leaf);
            match fs::symlink_metadata(&target) {
                Ok(_) => {
                    if matches!(request.conflict_policy, ConflictPolicy::Fail) {
                        return Err(Fault::new(FaultCode::DestinationExists));
                    }
                }
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        let mut state = self.lock()?;
        if state.closing || state.jobs.values().any(|j| j.snapshot.status.active()) {
            return Err(Fault::new(FaultCode::Busy));
        }
        if state.jobs.len() >= 256 {
            let old = state
                .jobs
                .iter()
                .find(|(_, job)| job.snapshot.journal_saved)
                .map(|(id, _)| id.clone())
                .ok_or_else(|| Fault::new(FaultCode::ResourceLimit))?;
            state.jobs.remove(&old);
        }
        let capture = state
            .captures
            .get(&request.capture_id)
            .filter(|c| c.sealed && c.count > 0)
            .ok_or_else(|| Fault::new(FaultCode::StaleRequest))?
            .clone();
        if request.name.is_some() && capture.count != 1 {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
        let id = crate::services::id::new_id().map_err(Fault::from)?;
        let header = Header {
            schema: 1,
            id: id.clone(),
            capture,
            destination,
            project_id: request.project_id,
            conflict_policy: request.conflict_policy,
            name: request.name,
        };
        let snapshot = initial(&header);
        let mut journal = open_file(&self.directory.join(format!("{id}.journal")), true)?;
        journal.sync_all().map_err(Fault::from)?;
        publish_header(&self.directory, &header)?;
        state.referenced_captures.insert(header.capture.id.clone());
        let cancel = Arc::new(AtomicBool::new(false));
        state.jobs.insert(
            id.clone(),
            Job {
                snapshot: snapshot.clone(),
                durable_end: Some(0),
                cancel: cancel.clone(),
            },
        );
        drop(state);
        let service = self.clone();
        if std::thread::Builder::new()
            .name("projectpane-copy-job".into())
            .spawn(move || service.run(header, &mut journal, &cancel))
            .is_err()
        {
            let mut failed = snapshot.clone();
            failed.status = CopyStatus::Failed;
            failed.failure = Some(Fault::new(FaultCode::WorkerUnavailable));
            failed.journal_saved = false;
            if let Ok(mut log) = append_file(&self.directory.join(format!("{id}.journal"))) {
                failed.journal_saved = true;
                if write_record(
                    &mut log,
                    &Event::Finished {
                        snapshot: failed.clone(),
                    },
                )
                .is_err()
                {
                    failed.journal_saved = false;
                }
            }
            self.publish(&failed, 0);
            return Err(Fault::new(FaultCode::WorkerUnavailable));
        }
        Ok(snapshot)
    }
    fn run(&self, header: Header, journal: &mut File, cancel: &AtomicBool) {
        #[cfg(test)]
        let legacy = self.legacy_sync.load(Ordering::Acquire);
        #[cfg(not(test))]
        let legacy = false;
        let mut log = match writer::JobLog::new(journal, legacy) {
            Ok(log) => log,
            Err(error) => {
                self.publish(&damaged(&header.id, Some(&header), error), 0);
                return;
            }
        };
        let mut snapshot = initial(&header);
        let result = (|| {
            let mut reader = BufReader::new(read_file(
                &self
                    .directory
                    .join(format!("{}.sources", header.capture.id)),
            )?);
            for _ in 0..header.capture.count {
                if cancel.load(Ordering::Acquire) {
                    return Err(Fault::new(FaultCode::Cancelled));
                }
                let source: OperationSource =
                    read_record(&mut reader)?.ok_or_else(|| Fault::new(FaultCode::InvalidState))?;
                {
                    let writer = std::cell::RefCell::new(&mut log);
                    let progress = std::cell::RefCell::new(&mut snapshot);
                    copy_root(
                        &source,
                        &header.destination,
                        header.name.as_deref(),
                        matches!(header.conflict_policy, ConflictPolicy::Skip),
                        cancel,
                        |source, destination| {
                            writer.borrow_mut().intent(source, destination)?;
                            // This barrier covers the previous results and the next intent.
                            self.publish(&progress.borrow(), writer.borrow().durable_end());
                            Ok(())
                        },
                        |item| {
                            let committed = writer
                                .borrow_mut()
                                .result(&Event::Outcome { item: item.clone() })?;
                            let mut value = progress.borrow_mut();
                            value.counts.add(item.status);
                            value.outcome_count += 1;
                            if committed {
                                self.publish(&value, writer.borrow().durable_end());
                            }
                            Ok(())
                        },
                    )?;
                }
                let committed = log.result(&Event::RootDone)?;
                snapshot.roots_completed += 1;
                if committed {
                    self.publish(&snapshot, log.durable_end());
                }
            }
            Ok(())
        })();
        snapshot.status = match &result {
            Ok(()) if snapshot.counts.unknown > 0 => CopyStatus::InterruptedUnknown,
            Ok(()) if snapshot.counts.failed > 0 => CopyStatus::Failed,
            Ok(()) => CopyStatus::Succeeded,
            Err(error) if error.code == FaultCode::Cancelled => CopyStatus::Cancelled,
            Err(_) => CopyStatus::Failed,
        };
        snapshot.failure = result.err();
        // Reading the OS cache cannot turn a failed barrier into durable evidence.
        // A poisoned writer never appends Finished onto a possibly torn tail.
        let completion = if log.poisoned() {
            Err(snapshot
                .failure
                .clone()
                .unwrap_or_else(|| Fault::new(FaultCode::IoError)))
        } else {
            log.finish(&snapshot)
        };
        #[cfg(test)]
        self.journal_syncs.store(log.syncs(), Ordering::Release);
        match completion {
            Ok(()) => self.publish(&snapshot, log.durable_end()),
            Err(error) => {
                let mut unknown = recover_until(&self.directory, &header, Some(log.durable_end()))
                    .unwrap_or_else(|failure| damaged(&header.id, Some(&header), failure));
                unknown.status = CopyStatus::InterruptedUnknown;
                unknown.journal_saved = false;
                unknown.failure = Some(error);
                self.publish(&unknown, log.durable_end());
            }
        }
    }
    fn publish(&self, snapshot: &Snapshot, durable_end: u64) {
        if let Ok(mut state) = self.lock()
            && let Some(job) = state.jobs.get_mut(&snapshot.id)
        {
            job.snapshot = snapshot.clone();
            job.durable_end = Some(durable_end);
            if job.cancel.load(Ordering::Acquire) && job.snapshot.status.active() {
                job.snapshot.status = CopyStatus::CancelRequested;
            }
        }
    }
    pub fn outcomes(&self, id: &str, offset: u64, limit: usize) -> Result<OutcomePage, Fault> {
        if !valid_token(id) || limit == 0 || limit > 128 {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
        let current = self
            .lock()?
            .jobs
            .get(id)
            .map(|job| (job.snapshot.clone(), job.durable_end));
        let (snapshot, durable_end) = match current {
            Some(value) => value,
            None => (self.snapshot(id)?, None),
        };
        read_page_until(
            &self.directory,
            id,
            offset,
            limit,
            !snapshot.status.active(),
            durable_end,
        )
    }
}
fn initial(header: &Header) -> Snapshot {
    Snapshot {
        id: header.id.clone(),
        project_id: header.project_id.clone(),
        status: CopyStatus::Running,
        counts: Counts::default(),
        roots_total: header.capture.count,
        roots_completed: 0,
        outcome_count: 0,
        journal_saved: true,
        failure: None,
    }
}

fn valid_header(header: &Header, id: &str) -> bool {
    header.schema == 1
        && header.id == id
        && valid_token(&header.capture.id)
        && header.capture.count > 0
        && header.capture.count <= 9_007_199_254_740_991
        && header.capture.sealed
        && header.destination.kind == EntryKind::Folder
        && header.destination.file_index != 0
        && crate::domain::projects::normalize_absolute_path(&header.destination.path).is_ok()
        && header.project_id.as_ref().is_none_or(|id| valid_token(id))
        && header
            .name
            .as_ref()
            .is_none_or(|name| validate_leaf_name(name).is_ok())
}
fn damaged(id: &str, header: Option<&Header>, error: Fault) -> Snapshot {
    Snapshot {
        id: id.into(),
        project_id: header.and_then(|h| h.project_id.clone()),
        status: CopyStatus::InterruptedUnknown,
        counts: Counts {
            unknown: 1,
            ..Counts::default()
        },
        roots_total: header.map_or(0, |h| h.capture.count),
        roots_completed: 0,
        outcome_count: 0,
        journal_saved: false,
        failure: Some(error),
    }
}
