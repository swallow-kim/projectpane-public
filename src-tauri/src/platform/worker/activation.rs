use super::{Listing, native_id};
use crate::domain::*;
use std::fs;

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
