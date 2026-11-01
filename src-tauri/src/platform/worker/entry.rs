use crate::domain::{Entry, EntryKind};
use std::{
    ffi::OsStr,
    fs::DirEntry,
    io,
    time::{Duration, SystemTime, UNIX_EPOCH},
};

const JS_DATE_LIMIT_MS: i64 = 8_640_000_000_000_000;
// Outside the JS Date range; keeps unknown timestamps in one i64 without
// sacrificing epoch zero or pre-epoch values, or allocating an Option tag.
pub(super) const UNKNOWN_MODIFIED_AT_MS: i64 = i64::MIN;

// One native component per entry; the listing retains the root only once.
pub(super) struct StoredEntry {
    pub(super) name: Box<OsStr>,
    pub(super) kind: EntryKind,
    pub(super) modified_at_ms: i64,
    pub(super) size_bytes: u64,
}
impl StoredEntry {
    pub(super) fn from_dir_entry(item: &DirEntry) -> Self {
        let kind = match item.file_type() {
            Ok(t) if t.is_symlink() => EntryKind::Link,
            Ok(t) if t.is_dir() => EntryKind::Folder,
            Ok(t) if t.is_file() => EntryKind::File,
            Ok(_) | Err(_) => EntryKind::Unknown,
        };
        let metadata = item.metadata();
        let size_bytes = metadata
            .as_ref()
            .ok()
            .filter(|m| kind == EntryKind::File && m.is_file())
            .map(|m| m.len())
            .filter(|size| *size <= 9_007_199_254_740_991)
            .unwrap_or(u64::MAX);
        Self {
            name: item.file_name().into_boxed_os_str(),
            kind,
            // DirEntry metadata does not follow links and is cached on Windows.
            modified_at_ms: modified_at_ms(metadata.and_then(|m| m.modified()))
                .unwrap_or(UNKNOWN_MODIFIED_AT_MS),
            size_bytes,
        }
    }

    pub(super) fn display(&self) -> Entry {
        Entry {
            id: super::native_id::encode(&self.name),
            name: self.name.to_string_lossy().into_owned(),
            kind: self.kind,
            modified_at_ms: (self.modified_at_ms != UNKNOWN_MODIFIED_AT_MS)
                .then_some(self.modified_at_ms),
            size_bytes: (self.kind == EntryKind::File && self.size_bytes <= 9_007_199_254_740_991)
                .then_some(self.size_bytes),
        }
    }
}

fn modified_at_ms(modified: io::Result<SystemTime>) -> Option<i64> {
    let (duration, sign) = match modified.ok()?.duration_since(UNIX_EPOCH) {
        Ok(duration) => (duration, 1),
        Err(error) => (error.duration(), -1),
    };
    duration_ms(duration).map(|millis| millis * sign)
}

fn duration_ms(duration: Duration) -> Option<i64> {
    if duration > Duration::from_millis(JS_DATE_LIMIT_MS.unsigned_abs()) {
        return None;
    }
    // Sub-millisecond precision is truncated toward zero on both sides of epoch.
    i64::try_from(duration.as_millis()).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conversion_preserves_epoch_and_signed_milliseconds() {
        for millis in [0_i64, 1, -1, 1_700_000_000_123, -1_234_567] {
            let duration = Duration::from_millis(millis.unsigned_abs());
            let time = if millis < 0 {
                UNIX_EPOCH - duration
            } else {
                UNIX_EPOCH + duration
            };
            assert_eq!(modified_at_ms(Ok(time)), Some(millis));
        }
    }

    #[test]
    fn conversion_truncates_submillisecond_precision_toward_zero() {
        for (nanos, expected) in [(999_999, 0), (1_999_999, 1)] {
            let duration = Duration::from_nanos(nanos);
            assert_eq!(modified_at_ms(Ok(UNIX_EPOCH + duration)), Some(expected));
            assert_eq!(modified_at_ms(Ok(UNIX_EPOCH - duration)), Some(-expected));
        }
    }

    #[test]
    fn conversion_rejects_unavailable_and_outside_date_range() {
        assert_eq!(
            modified_at_ms(Err(io::ErrorKind::PermissionDenied.into())),
            None
        );
        let limit = Duration::from_millis(JS_DATE_LIMIT_MS.unsigned_abs());
        for (duration, expected) in [
            (limit, Some(JS_DATE_LIMIT_MS)),
            (limit - Duration::from_millis(1), Some(JS_DATE_LIMIT_MS - 1)),
            (limit + Duration::from_nanos(1), None),
            (limit + Duration::from_millis(1), None),
            (Duration::MAX, None),
        ] {
            assert_eq!(duration_ms(duration), expected);
        }
    }

    #[test]
    fn stored_unknown_serializes_explicit_null() {
        let stored = StoredEntry {
            size_bytes: u64::MAX,
            name: OsStr::new("unknown").into(),
            kind: EntryKind::File,
            modified_at_ms: UNKNOWN_MODIFIED_AT_MS,
        };
        let wire = serde_json::to_value(stored.display()).unwrap();
        assert_eq!(wire.get("modifiedAtMs"), Some(&serde_json::Value::Null));
    }

    #[test]
    #[cfg(target_pointer_width = "64")]
    fn stored_entry_uses_40_bytes_including_size() {
        assert_eq!(std::mem::size_of::<StoredEntry>(), 40);
    }
}
