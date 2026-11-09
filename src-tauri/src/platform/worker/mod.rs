//! Read-only subprocess. A stuck OS call is contained in this process.
use crate::domain::*;
use std::{
    fs::{self, ReadDir},
    io::{self, BufRead, Read, Write},
    mem::size_of,
    path::Path,
};

mod activation;
mod entry;
mod native_id;
mod view;
mod wire;
use entry::StoredEntry;

#[cfg(test)]
mod payload_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod view_tests;

// Conservative per-allocation allowance, not a process-private-bytes estimate.
const ALLOCATION_OVERHEAD: usize = 32;
const VIEW_SCRATCH_BYTES: usize = 24 * 1024;
#[derive(Default)]
pub struct Listing {
    iterator: Option<ReadDir>,
    root: Option<Box<Path>>,
    entries: Vec<StoredEntry>,
    visible: Vec<usize>,
    view: ListingView,
    revision: u64,
    name_bytes: usize,
    cap_reason: Option<&'static str>,
}

impl Listing {
    pub fn handle(&mut self, request: Request) -> Result<Batch, Fault> {
        match request {
            Request::Open { path, view } => {
                view.validate()?;
                let path = AbsolutePath::new(path)?;
                *self = Self::default();
                self.view = view;
                self.revision = 1;
                self.iterator = Some(fs::read_dir(path.as_path())?);
                self.root = Some(path.into_boxed_path());
                self.advance(true)
            }
            Request::Advance => self.advance(true),
            Request::Progress => self.advance(false),
            Request::SetView { view } => {
                view.validate()?;
                if self.snapshot_bytes() - self.view.filter.capacity() + view.filter.capacity()
                    > MAX_SNAPSHOT_BYTES
                {
                    return Err(Fault::new(FaultCode::ResourceLimit));
                }
                if self.view != view {
                    self.view = view;
                    self.rebuild_view();
                }
                self.batch(0, BATCH_SIZE)
            }
            Request::Locate { .. } | Request::Activate { .. } | Request::ActivateMany { .. } => {
                Err(Fault::new(FaultCode::InvalidRequest))
            }
            Request::Window { offset, limit } => {
                if limit == 0 || limit > BATCH_SIZE || offset > self.visible.len() {
                    return Err(Fault::new(FaultCode::InvalidRequest));
                }
                self.batch(offset, limit)
            }
        }
    }

    pub fn snapshot_bytes(&self) -> usize {
        self.name_bytes
            + self.entries.capacity() * size_of::<StoredEntry>()
            + self.visible.capacity() * size_of::<usize>()
            + self.view.filter.capacity()
            + self.root.as_ref().map_or(0, |root| {
                root.as_os_str().len() + ALLOCATION_OVERHEAD + VIEW_SCRATCH_BYTES
            })
    }

    fn retain(&mut self, entry: StoredEntry) -> Result<bool, Fault> {
        if self.entries.len() == MAX_ENTRIES {
            self.cap_reason = Some("entry_limit");
            return Ok(false);
        }
        let display = entry.display();
        let encoded = serde_json::to_vec(&display)
            .map_err(|_| Fault::new(FaultCode::ProtocolError))?
            .len();
        if display.id.len() > MAX_ENTRY_ID || encoded > MAX_ENTRY_WIRE_BYTES {
            self.cap_reason = Some("payload_budget");
            return Ok(false);
        }
        let capacity = self.entries.capacity();
        let next_capacity = if self.entries.len() == capacity {
            (capacity * 2).clamp(BATCH_SIZE, MAX_ENTRIES)
        } else {
            capacity
        };
        let cost = entry.name.len() + ALLOCATION_OVERHEAD;
        let index_growth = next_capacity.saturating_sub(self.visible.capacity());
        if self.snapshot_bytes()
            + (next_capacity - capacity) * size_of::<StoredEntry>()
            + index_growth * size_of::<usize>()
            + cost
            > MAX_SNAPSHOT_BYTES
        {
            self.cap_reason = Some("snapshot_budget");
            return Ok(false);
        }
        if next_capacity != capacity {
            self.entries
                .try_reserve_exact(next_capacity - self.entries.len())
                .map_err(|_| Fault::new(FaultCode::ResourceLimit))?;
        }
        if index_growth > 0 {
            self.visible
                .try_reserve_exact(next_capacity - self.visible.len())
                .map_err(|_| Fault::new(FaultCode::ResourceLimit))?;
        }
        // Use the allocator-reported capacity, including any excess reservation.
        if self.snapshot_bytes() + cost > MAX_SNAPSHOT_BYTES {
            return Err(Fault::new(FaultCode::ResourceLimit));
        }
        self.name_bytes += cost;
        if self.matches_filter(&entry) {
            self.visible.push(self.entries.len());
        }
        self.entries.push(entry);
        Ok(true)
    }

    fn advance(&mut self, include_entries: bool) -> Result<Batch, Fault> {
        let start = self.entries.len();
        let offset = self.visible.len();
        let was_loading = self.iterator.is_some();
        while self.entries.len() - start < BATCH_SIZE {
            let Some(iterator) = &mut self.iterator else {
                break;
            };
            let Some(item) = iterator.next() else {
                self.iterator = None;
                break;
            };
            let item = item?;
            let entry = StoredEntry::from_dir_entry(&item);
            if !self.retain(entry)? {
                self.iterator = None;
                break;
            }
        }
        if was_loading && self.iterator.is_none() {
            self.rebuild_view();
            return self.batch(0, if include_entries { BATCH_SIZE } else { 0 });
        }
        self.batch(offset, if include_entries { BATCH_SIZE } else { 0 })
    }

    fn batch(&self, offset: usize, limit: usize) -> Result<Batch, Fault> {
        let mut entries = Vec::new();
        let mut bytes = 0;
        for index in self.visible.iter().skip(offset).take(limit) {
            let entry = self.entries[*index].display();
            let size = serde_json::to_vec(&entry)
                .map_err(|_| Fault::new(FaultCode::ProtocolError))?
                .len();
            if bytes + size > MAX_FRAME - 4096 {
                break;
            }
            bytes += size;
            entries.push(entry);
        }
        let result = Batch {
            revision: self.revision.max(1),
            offset,
            entries,
            total: self.visible.len(),
            complete: self.iterator.is_none(),
            capped: self.cap_reason.is_some(),
            snapshot_bytes: self.snapshot_bytes(),
            cap_reason: self.cap_reason.map(str::to_owned),
        };
        if serde_json::to_vec(&result)
            .map_err(|_| Fault::new(FaultCode::ProtocolError))?
            .len()
            >= MAX_FRAME - 2048
        {
            return Err(Fault::new(FaultCode::PayloadLimit));
        }
        Ok(result)
    }
}

pub fn run() -> io::Result<()> {
    let mut listing = Listing::default();
    let mut input = io::stdin().lock();
    let mut output = io::stdout().lock();
    loop {
        let mut line = Vec::new();
        // Bound malformed input before allocation; parent owns both pipe ends.
        let n = std::io::Read::by_ref(&mut input)
            .take(MAX_FRAME as u64 + 1)
            .read_until(b'\n', &mut line)?;
        if n == 0 {
            return Ok(());
        }
        if n > MAX_FRAME || !line.ends_with(b"\n") {
            return Err(io::ErrorKind::InvalidData.into());
        }
        let result = serde_json::from_slice::<Request>(&line)
            .map_err(|_| Fault::new(FaultCode::InvalidRequest))
            .and_then(|request| listing.respond(request));
        serde_json::to_writer(&mut output, &result)?;
        output.write_all(b"\n")?;
        output.flush()?;
    }
}
