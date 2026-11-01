use super::{Listing, entry::StoredEntry};
use crate::domain::*;
use std::{cmp::Ordering, path::Path};

impl Listing {
    pub(super) fn rebuild_view(&mut self) {
        self.visible.clear();
        let filter = self.view.filter.to_lowercase();
        self.visible
            .extend(self.entries.iter().enumerate().filter_map(|(i, entry)| {
                (filter.is_empty()
                    || entry
                        .name
                        .to_string_lossy()
                        .to_lowercase()
                        .contains(&filter))
                .then_some(i)
            }));
        // In-place unstable sort needs no temporary index allocation. Native-name
        // tie breaking makes the order total and deterministic anyway.
        let entries = &self.entries;
        let view = &self.view;
        self.visible
            .sort_unstable_by(|a, b| compare(&entries[*a], &entries[*b], view));
        self.revision += 1;
    }

    pub(super) fn matches_filter(&self, entry: &StoredEntry) -> bool {
        self.view.filter.is_empty()
            || entry
                .name
                .to_string_lossy()
                .to_lowercase()
                .contains(&self.view.filter.to_lowercase())
    }
}

fn compare(a: &StoredEntry, b: &StoredEntry, view: &ListingView) -> Ordering {
    if view.folders_first {
        let folders = (b.kind == EntryKind::Folder).cmp(&(a.kind == EntryKind::Folder));
        if folders != Ordering::Equal {
            return folders;
        }
    }
    let nulls = match view.sort {
        SortColumn::Size => (a.size_bytes == u64::MAX).cmp(&(b.size_bytes == u64::MAX)),
        SortColumn::Modified => (a.modified_at_ms == super::entry::UNKNOWN_MODIFIED_AT_MS)
            .cmp(&(b.modified_at_ms == super::entry::UNKNOWN_MODIFIED_AT_MS)),
        SortColumn::Name | SortColumn::Type => Ordering::Equal,
    };
    if nulls != Ordering::Equal {
        return nulls;
    }
    let order = match view.sort {
        SortColumn::Name => natural(
            &a.name.to_string_lossy().to_lowercase(),
            &b.name.to_string_lossy().to_lowercase(),
        ),
        SortColumn::Type => kind_rank(a.kind).cmp(&kind_rank(b.kind)).then_with(|| {
            let a = Path::new(&a.name)
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            let b = Path::new(&b.name)
                .extension()
                .unwrap_or_default()
                .to_string_lossy()
                .to_lowercase();
            natural(&a, &b)
        }),
        SortColumn::Size => a.size_bytes.cmp(&b.size_bytes),
        SortColumn::Modified => a.modified_at_ms.cmp(&b.modified_at_ms),
    }
    .then_with(|| super::native_id::compare(&a.name, &b.name));
    match view.direction {
        SortDirection::Asc => order,
        SortDirection::Desc => order.reverse(),
    }
}

const fn kind_rank(kind: EntryKind) -> u8 {
    match kind {
        EntryKind::Folder => 0,
        EntryKind::File => 1,
        EntryKind::Link => 2,
        EntryKind::Unknown => 3,
    }
}

fn natural(mut a: &str, mut b: &str) -> Ordering {
    while !a.is_empty() && !b.is_empty() {
        let an = a.bytes().take_while(u8::is_ascii_digit).count();
        let bn = b.bytes().take_while(u8::is_ascii_digit).count();
        if an > 0 && bn > 0 {
            let ad = a[..an].trim_start_matches('0');
            let bd = b[..bn].trim_start_matches('0');
            let order = ad.len().cmp(&bd.len()).then_with(|| ad.cmp(bd));
            if order != Ordering::Equal {
                return order;
            }
            a = &a[an..];
            b = &b[bn..];
        } else {
            let mut ac = a.chars();
            let mut bc = b.chars();
            let order = ac.next().cmp(&bc.next());
            if order != Ordering::Equal {
                return order;
            }
            a = ac.as_str();
            b = bc.as_str();
        }
    }
    a.len().cmp(&b.len())
}
