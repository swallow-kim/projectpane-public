//! Bounded ZIP central directory validation before invoking the Windows decoder.
//! ZIP64, encryption, non-UTF8 legacy names, links and extra streams require an
//! external archive application. No paths from the archive reach a shell.
use crate::domain::{Fault, FaultCode, MAX_FRAME, operations::validate_leaf_name};
use std::{
    collections::HashSet,
    fs::File,
    io::{Read, Seek, SeekFrom},
};
pub(super) const MAX_BYTES: u64 = 1024 * 1024 * 1024;
pub(super) struct Entry {
    pub name: String,
    pub folder: bool,
    pub bytes: u64,
    pub crc: u32,
}
fn invalid() -> Fault {
    Fault::new(FaultCode::InvalidRequest)
}
fn u16_at(b: &[u8], i: usize) -> Result<u16, Fault> {
    Ok(u16::from_le_bytes(
        b.get(i..i + 2).ok_or_else(invalid)?.try_into().unwrap(),
    ))
}
fn u32_at(b: &[u8], i: usize) -> Result<u32, Fault> {
    Ok(u32::from_le_bytes(
        b.get(i..i + 4).ok_or_else(invalid)?.try_into().unwrap(),
    ))
}
pub(super) fn entries(file: &mut File) -> Result<Vec<Entry>, Fault> {
    let size = file.metadata().map_err(Fault::from)?.len();
    if !(22..=MAX_BYTES).contains(&size) {
        return Err(Fault::new(FaultCode::ResourceLimit));
    }
    let tail_size = size.min(65557) as usize;
    file.seek(SeekFrom::End(-(tail_size as i64)))
        .map_err(Fault::from)?;
    let mut tail = vec![0; tail_size];
    file.read_exact(&mut tail).map_err(Fault::from)?;
    let offset = (0..=tail.len() - 22)
        .rev()
        .find(|i| {
            tail[*i..*i + 4] == *b"PK\x05\x06"
                && u16_at(&tail, *i + 20).is_ok_and(|n| *i + 22 + usize::from(n) == tail.len())
        })
        .ok_or_else(invalid)?;
    let end = &tail[offset..];
    let count = usize::from(u16_at(end, 10)?);
    let length = u32_at(end, 12)? as usize;
    let start = u64::from(u32_at(end, 16)?);
    if count > 2048
        || length >= MAX_FRAME
        || u16_at(end, 4)? != 0
        || u16_at(end, 6)? != 0
        || usize::from(u16_at(end, 8)?) != count
        || start.checked_add(length as u64) != Some(size - tail_size as u64 + offset as u64)
    {
        return Err(invalid());
    }
    file.seek(SeekFrom::Start(start)).map_err(Fault::from)?;
    let mut data = vec![0; length];
    file.read_exact(&mut data).map_err(Fault::from)?;
    let mut cursor = 0;
    let mut result = Vec::new();
    let mut names = HashSet::new();
    let mut total = 0u64;
    for _ in 0..count {
        let b = data.get(cursor..).ok_or_else(invalid)?;
        if b.get(..4) != Some(b"PK\x01\x02")
            || u16_at(b, 8)? & (1 | 64 | 8192) != 0
            || ![0, 8].contains(&u16_at(b, 10)?)
            || u16_at(b, 34)? != 0
        {
            return Err(invalid());
        }
        let size = u64::from(u32_at(b, 24)?);
        total = total.checked_add(size).ok_or_else(invalid)?;
        if total > MAX_BYTES || u64::from(u32_at(b, 42)?) >= start {
            return Err(Fault::new(FaultCode::ResourceLimit));
        }
        let attrs = u32_at(b, 38)?;
        if attrs & 0x400 != 0
            || matches!(
                (attrs >> 16) & 0xf000,
                0xa000 | 0x6000 | 0x2000 | 0x1000 | 0xc000
            )
        {
            return Err(invalid());
        }
        let name_len = usize::from(u16_at(b, 28)?);
        let raw = b.get(46..46 + name_len).ok_or_else(invalid)?;
        if u16_at(b, 8)? & 0x800 == 0 && !raw.is_ascii() {
            return Err(invalid());
        }
        let name = std::str::from_utf8(raw)
            .map_err(|_| invalid())?
            .replace('\\', "/");
        let folder = name.ends_with('/');
        let name = name.trim_end_matches('/').to_owned();
        validate_name(&name)?;
        if (folder && size != 0) || !names.insert(name.to_lowercase()) {
            return Err(invalid());
        }
        result.push(Entry {
            name,
            folder,
            bytes: size,
            crc: u32_at(b, 16)?,
        });
        cursor = cursor
            .checked_add(46 + name_len + usize::from(u16_at(b, 30)?) + usize::from(u16_at(b, 32)?))
            .ok_or_else(invalid)?;
    }
    if cursor != data.len() {
        return Err(invalid());
    }
    let files: HashSet<_> = result
        .iter()
        .filter(|e| !e.folder)
        .map(|e| e.name.to_lowercase())
        .collect();
    for e in &result {
        let mut path = e.name.as_str();
        while let Some((parent, _)) = path.rsplit_once('/') {
            if files.contains(&parent.to_lowercase()) {
                return Err(invalid());
            }
            path = parent;
        }
    }
    Ok(result)
}
pub(super) fn validate_name(name: &str) -> Result<(), Fault> {
    if name.is_empty() || name.len() > 32768 || name.split('/').count() > 64 {
        return Err(invalid());
    }
    for part in name.split('/') {
        validate_leaf_name(part)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn archive_names_cannot_escape_or_address_devices_and_streams() {
        for name in [
            "../a",
            "/a",
            "C:/a",
            "a/../b",
            "a//b",
            "a/CON.txt",
            "a:file",
            "a/.",
            "\\server\\share",
        ] {
            assert!(validate_name(name).is_err(), "{name}");
        }
        assert!(validate_name("한글 폴더/file.txt").is_ok());
    }

    // Central-directory parser fixture; decoder/CRC behavior has separate native
    // roundtrip tests. Each central record points before the central directory.
    fn directory(names: &[(&str, u32)], flags: u16, bytes: u32) -> tempfile::NamedTempFile {
        use std::io::Write;
        let mut data = vec![0; 30];
        for (name, attributes) in names {
            let mut record = vec![0; 46];
            record[..4].copy_from_slice(b"PK\x01\x02");
            record[8..10].copy_from_slice(&flags.to_le_bytes());
            record[24..28].copy_from_slice(&bytes.to_le_bytes());
            record[28..30].copy_from_slice(&(name.len() as u16).to_le_bytes());
            record[38..42].copy_from_slice(&attributes.to_le_bytes());
            data.extend(record);
            data.extend(name.as_bytes());
        }
        let length = (data.len() - 30) as u32;
        let mut end = vec![0; 22];
        end[..4].copy_from_slice(b"PK\x05\x06");
        end[8..10].copy_from_slice(&(names.len() as u16).to_le_bytes());
        end[10..12].copy_from_slice(&(names.len() as u16).to_le_bytes());
        end[12..16].copy_from_slice(&length.to_le_bytes());
        end[16..20].copy_from_slice(&30u32.to_le_bytes());
        data.extend(end);
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.write_all(&data).unwrap();
        file
    }
    #[test]
    fn central_directory_rejects_traversal_duplicates_links_encryption_and_bombs() {
        for names in [
            vec![("../escape", 0)],
            vec![("A", 0), ("a", 0)],
            vec![("a", 0), ("a/b", 0)],
            vec![("link", 0xa000_0000)],
            vec![("file:stream", 0)],
        ] {
            assert!(
                entries(directory(&names, 0x800, 1).as_file_mut()).is_err(),
                "{names:?}"
            );
        }
        assert!(entries(directory(&[("file", 0)], 1, 1).as_file_mut()).is_err());
        assert!(entries(directory(&[("file", 0)], 0x800, u32::MAX).as_file_mut()).is_err());
        assert!(entries(directory(&[("한글/a.txt", 0)], 0, 1).as_file_mut()).is_err());
        let parsed = entries(directory(&[("한글/a.txt", 0)], 0x800, 1).as_file_mut()).unwrap();
        assert_eq!(parsed[0].name, "한글/a.txt");
    }
}
