use super::{MAX_OBSERVATION_HANDLES, fingerprint, native};
use serde::Serialize;
use std::{
    fs::{File, OpenOptions},
    io::{self, Read, Seek, Write},
    os::windows::fs::OpenOptionsExt,
};
use windows_sys::Win32::Storage::FileSystem::{
    FILE_ATTRIBUTE_DIRECTORY, FILE_FLAG_OPEN_REPARSE_POINT, FILE_SHARE_READ,
};

#[cfg(test)]
mod tests;

/// Maximum default-stream bytes in a staging file, not a memory budget.
pub const MAX_STAGE_BYTES: u64 = 16 * 1024 * 1024;

/// Effects of this staging call only; existing source/destination files are not modified.
#[derive(Debug)]
pub enum StageFailure {
    NotCreated(io::ErrorKind),
    Unacknowledged(io::ErrorKind),
}

impl StageFailure {
    fn before(error: io::Error) -> Self {
        Self::NotCreated(error.kind())
    }

    fn after(error: io::Error) -> Self {
        Self::Unacknowledged(error.kind())
    }
}

/// Same-handle verified default-stream copy, not replacement or ownership authority.
/// File indices serialize as strings to avoid JavaScript integer precision loss.
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StageReceipt {
    volume: u32,
    source_index: String,
    staged_index: String,
    bytes: u64,
    sha256: String,
}

/// Retains source, staging and ancestor handles. Drop releases handles, never deletes.
#[must_use]
pub struct StagedFile {
    receipt: StageReceipt,
    _source: File,
    _staged: File,
    _guards: Vec<File>,
}

impl StagedFile {
    pub const fn receipt(&self) -> &StageReceipt {
        &self.receipt
    }
}

/// Creates and verifies a bounded default-stream copy in an existing local directory.
/// Both paths must already use normalized absolute Windows spelling.
/// This does not approve source content or preserve ADS, ACLs or other metadata.
///
/// # Errors
/// Refuses unsafe/unsupported source paths, size/handle limits and occupied destinations.
/// Every failure after creation is `Unacknowledged`; partial bytes are never removed.
pub fn create_stage(source: &str, destination: &str) -> Result<StagedFile, StageFailure> {
    create_with(source, destination, |input, output, length| {
        io::copy(&mut input.take(length), output)
    })
}

fn create_with(
    source: &str,
    destination: &str,
    copy: impl FnOnce(&mut File, &mut File, u64) -> io::Result<u64>,
) -> Result<StagedFile, StageFailure> {
    let mut guards =
        native::ancestors(source, MAX_OBSERVATION_HANDLES - 2).map_err(StageFailure::before)?;
    let available = MAX_OBSERVATION_HANDLES
        .checked_sub(guards.len() + 2)
        .ok_or(StageFailure::NotCreated(io::ErrorKind::InvalidData))?;
    let mut target_guards =
        native::ancestors(destination, available).map_err(StageFailure::before)?;
    let parent = target_guards
        .last()
        .ok_or(StageFailure::NotCreated(io::ErrorKind::InvalidData))?;
    let parent_identity = native::identity(parent).map_err(StageFailure::before)?;
    guards.append(&mut target_guards);
    let mut input = OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(source)
        .map_err(StageFailure::before)?;
    let before = native::identity(&input).map_err(StageFailure::before)?;
    if before.links != 1
        || before.attributes & FILE_ATTRIBUTE_DIRECTORY != 0
        || before.bytes > MAX_STAGE_BYTES
        || before.volume != parent_identity.volume
    {
        return Err(StageFailure::NotCreated(io::ErrorKind::InvalidData));
    }
    let expected = fingerprint(&mut input, before.bytes).map_err(StageFailure::before)?;
    if native::identity(&input).map_err(StageFailure::before)? != before {
        return Err(StageFailure::NotCreated(io::ErrorKind::InvalidData));
    }
    input.rewind().map_err(StageFailure::before)?;
    let mut output = OpenOptions::new()
        .read(true)
        .write(true)
        .create_new(true)
        .share_mode(FILE_SHARE_READ)
        .open(destination)
        .map_err(StageFailure::before)?;
    let created = native::identity(&output).map_err(StageFailure::after)?;
    let copied = copy(&mut input, &mut output, before.bytes).map_err(StageFailure::after)?;
    output.flush().map_err(StageFailure::after)?;
    output.sync_all().map_err(StageFailure::after)?;
    output.rewind().map_err(StageFailure::after)?;
    let actual = fingerprint(&mut output, before.bytes).map_err(StageFailure::after)?;
    let finished = native::identity(&output).map_err(StageFailure::after)?;
    if copied != before.bytes
        || actual != expected
        || finished.bytes != before.bytes
        || finished.volume != before.volume
        || finished.index != created.index
        || finished.links != 1
        || finished.index == before.index
        || native::identity(&input).map_err(StageFailure::after)? != before
    {
        return Err(StageFailure::Unacknowledged(io::ErrorKind::InvalidData));
    }
    Ok(StagedFile {
        receipt: StageReceipt {
            volume: finished.volume,
            source_index: before.index.to_string(),
            staged_index: finished.index.to_string(),
            bytes: finished.bytes,
            sha256: actual
                .sha256
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect(),
        },
        _source: input,
        _staged: output,
        _guards: guards,
    })
}
