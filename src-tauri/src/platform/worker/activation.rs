use super::{Listing, entry::StoredEntry, native_id};
use crate::domain::*;
use std::{collections::HashMap, ffi::OsStr, fs};

impl Listing {
    pub fn locate(&self, entry_id: &str) -> Result<LocatedEntry, Fault> {
        let name = native_id::decode(entry_id)?;
        Ok(LocatedEntry {
            revision: self.revision.max(1),
            index: self
                .visible
                .iter()
                .position(|i| *self.entries[*i].name == name),
        })
    }

    pub fn activation_target(&self, entry_id: &str) -> Result<ActivationTarget, Fault> {
        let name = native_id::decode(entry_id)?;
        let entry = self
            .entries
            .iter()
            .find(|entry| *entry.name == name)
            .ok_or_else(|| Fault::new(FaultCode::NotFound))?;
        self.target_for_entry(entry)
    }

    pub fn activation_targets(&self, entry_ids: &[String]) -> Result<Vec<ActivationTarget>, Fault> {
        if entry_ids.is_empty() || entry_ids.len() > MAX_ACTIVATION_BATCH {
            return Err(Fault::new(FaultCode::InvalidRequest));
        }
        // Only the bounded request is indexed, never the retained snapshot.
        // Keep decode errors until their original position so earlier missing or
        // inaccessible items have exactly the same priority as serial activation.
        let names: Vec<_> = entry_ids.iter().map(|id| native_id::decode(id)).collect();
        let mut requested: HashMap<&OsStr, Option<&StoredEntry>> = names
            .iter()
            .filter_map(|name| name.as_ref().ok().map(|name| (name.as_os_str(), None)))
            .collect();
        let mut remaining = requested.len();
        for entry in &self.entries {
            if remaining == 0 {
                break;
            }
            if let Some(found) = requested.get_mut(entry.name.as_ref())
                && found.is_none()
            {
                *found = Some(entry);
                remaining -= 1;
            }
        }
        let mut targets = Vec::with_capacity(entry_ids.len());
        let mut wire_bytes = 32;
        for name in &names {
            let name = name.as_ref().map_err(|fault| Fault::new(fault.code))?;
            let entry = requested
                .get(name.as_os_str())
                .and_then(|entry| *entry)
                .ok_or_else(|| Fault::new(FaultCode::NotFound))?;
            let target = self.target_for_entry(entry)?;
            wire_bytes += serde_json::to_vec(&target)
                .map_err(|_| Fault::new(FaultCode::ProtocolError))?
                .len()
                + 1;
            if wire_bytes >= MAX_FRAME - 2048 {
                return Err(Fault::new(FaultCode::PayloadLimit));
            }
            targets.push(target);
        }
        Ok(targets)
    }

    fn target_for_entry(&self, entry: &StoredEntry) -> Result<ActivationTarget, Fault> {
        let root = self
            .root
            .as_ref()
            .ok_or_else(|| Fault::new(FaultCode::InvalidState))?;
        if entry.name.to_str().is_none() {
            return Err(Fault::new(FaultCode::InvalidPath));
        }
        let path = root.join(&*entry.name);
        let text = path
            .to_str()
            .ok_or_else(|| Fault::new(FaultCode::InvalidPath))?;
        AbsolutePath::new(text.to_owned())?;
        let metadata = fs::symlink_metadata(&path)?;
        let kind = match entry.kind {
            EntryKind::Folder if metadata.is_dir() => ActivationKind::Folder,
            EntryKind::File if metadata.is_file() => ActivationKind::File,
            EntryKind::Folder | EntryKind::File | EntryKind::Link | EntryKind::Unknown => {
                return Err(Fault::new(FaultCode::InvalidPath));
            }
        };
        Ok(ActivationTarget {
            kind,
            path: text.to_owned(),
        })
    }
}

#[cfg(test)]
#[path = "activation_batch_tests.rs"]
mod batch_tests;
