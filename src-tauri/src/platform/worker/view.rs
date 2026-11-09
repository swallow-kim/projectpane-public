use super::{Listing, entry::StoredEntry};
use crate::domain::*;
use std::{cmp::Ordering, ffi::OsStr, path::Path};

impl Listing {
    pub(super) fn rebuild_view(&mut self) {
        self.visible.clear();
        let filter = self.view.filter.to_lowercase();
        self.visible.extend(
            self.entries.iter().enumerate().filter_map(|(i, entry)| {
                matches_normalized_filter(&entry.name, &filter).then_some(i)
            }),
        );
        // In-place unstable sort needs no temporary index allocation. Native-name
        // tie breaking makes the order total and deterministic anyway.
        let entries = &self.entries;
        let view = &self.view;
        self.visible
            .sort_unstable_by(|a, b| compare(&entries[*a], &entries[*b], view));
        self.revision += 1;
    }

    pub(super) fn matches_filter(&self, entry: &StoredEntry) -> bool {
        let filter = &self.view.filter;
        if filter.is_empty() {
            return true;
        }
        if entry.name.len() <= 64 && filter.is_ascii() && entry.name.as_encoded_bytes().is_ascii() {
            return contains_ascii_ignore_case(entry.name.as_encoded_bytes(), filter.as_bytes());
        }
        matches_normalized_filter(&entry.name, &filter.to_lowercase())
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
        SortColumn::Name => natural_ignore_case(&a.name, &b.name),
        SortColumn::Type => kind_rank(a.kind).cmp(&kind_rank(b.kind)).then_with(|| {
            let a = Path::new(&a.name).extension().unwrap_or_default();
            let b = Path::new(&b.name).extension().unwrap_or_default();
            natural_ignore_case(a, b)
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

// ASCII encoded bytes are identical on supported platforms. Non-ASCII names
// retain lossy Unicode conversion and contextual lowercasing (Greek sigma).
// No cached names or temporary sort arrays enlarge the snapshot budget.
fn natural_ignore_case(a: &OsStr, b: &OsStr) -> Ordering {
    let ab = a.as_encoded_bytes();
    let bb = b.as_encoded_bytes();
    if ab.is_ascii() && bb.is_ascii() {
        natural_ascii(ab, bb)
    } else {
        natural(
            &a.to_string_lossy().to_lowercase(),
            &b.to_string_lossy().to_lowercase(),
        )
    }
}
fn natural_ascii(mut a: &[u8], mut b: &[u8]) -> Ordering {
    while !a.is_empty() && !b.is_empty() {
        if a[0].is_ascii_digit() && b[0].is_ascii_digit() {
            let an = a.iter().take_while(|c| c.is_ascii_digit()).count();
            let bn = b.iter().take_while(|c| c.is_ascii_digit()).count();
            let ad = trim_zeros(&a[..an]);
            let bd = trim_zeros(&b[..bn]);
            let order = ad.len().cmp(&bd.len()).then_with(|| ad.cmp(bd));
            if order != Ordering::Equal {
                return order;
            }
            a = &a[an..];
            b = &b[bn..];
        } else {
            let order = a[0].to_ascii_lowercase().cmp(&b[0].to_ascii_lowercase());
            if order != Ordering::Equal {
                return order;
            }
            a = &a[1..];
            b = &b[1..];
        }
    }
    a.len().cmp(&b.len())
}
fn trim_zeros(bytes: &[u8]) -> &[u8] {
    &bytes[bytes.iter().take_while(|c| **c == b'0').count()..]
}
fn contains_ascii_ignore_case(name: &[u8], filter: &[u8]) -> bool {
    filter.is_empty()
        || name
            .windows(filter.len())
            .any(|part| part.eq_ignore_ascii_case(filter))
}
fn matches_normalized_filter(name: &OsStr, filter: &str) -> bool {
    if filter.is_empty() {
        return true;
    }
    let bytes = name.as_encoded_bytes();
    if bytes.len() <= 64 && bytes.is_ascii() && filter.is_ascii() {
        contains_ascii_ignore_case(bytes, filter.as_bytes())
    } else {
        name.to_string_lossy().to_lowercase().contains(filter)
    }
}
#[cfg(test)]
#[path = "view_fast_tests.rs"]
mod fast_tests;
