//! Text-only path copying. Tests use a fake clipboard; never the user's clipboard.
use crate::domain::{
    Fault, FaultCode,
    projects::{PathRef, normalize_absolute_path, to_path_ref},
};
use windows::Win32::{
    Foundation::{GlobalFree, HANDLE, HGLOBAL, HWND},
    System::{
        DataExchange::{CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData},
        Memory::{GMEM_MOVEABLE, GlobalAlloc, GlobalLock, GlobalUnlock},
    },
};

const MAX_TEXT_BYTES: usize = 256 * 1024;
const CF_UNICODETEXT: u32 = 13;
fn invalid() -> Fault {
    Fault::new(FaultCode::InvalidPath)
}
fn io_error(_: windows::core::Error) -> Fault {
    Fault::new(FaultCode::IoError)
}

pub fn format_paths(paths: &[String], relative_to: Option<&str>) -> Result<String, Fault> {
    if paths.is_empty() || paths.len() > 128 {
        return Err(Fault::new(FaultCode::ResourceLimit));
    }
    if let Some(base) = relative_to {
        normalize_absolute_path(base).map_err(|_| invalid())?;
    }
    let mut lines = Vec::with_capacity(paths.len());
    let mut bytes = 0;
    for path in paths {
        let line = match relative_to {
            None => normalize_absolute_path(path).map_err(|_| invalid())?,
            Some(base) => match to_path_ref(base, path).map_err(|_| invalid())? {
                PathRef::Relative { value } if !value.is_empty() => value,
                // Never substitute an absolute path or invent a relative path
                // for a different drive/share or a mismatched panel root.
                _ => return Err(invalid()),
            },
        };
        bytes += line.len() + 2;
        if bytes > MAX_TEXT_BYTES {
            return Err(Fault::new(FaultCode::PayloadLimit));
        }
        lines.push(line);
    }
    Ok(lines.join("\r\n"))
}

trait Clipboard {
    type Data;
    fn prepare(&mut self, text: &str) -> Result<Self::Data, Fault>;
    fn open(&mut self) -> Result<(), Fault>;
    fn empty(&mut self) -> Result<(), Fault>;
    fn publish(&mut self, data: &mut Self::Data) -> Result<(), Fault>;
    fn close(&mut self) -> Result<(), Fault>;
}

fn write_text<C: Clipboard>(clipboard: &mut C, text: &str) -> Result<(), Fault> {
    if text.is_empty() || text.len() > MAX_TEXT_BYTES || text.contains('\0') {
        return Err(invalid());
    }
    // Complete allocations before opening/emptying the clipboard.
    let mut data = clipboard.prepare(text)?;
    clipboard.open()?;
    let result = clipboard
        .empty()
        .and_then(|()| clipboard.publish(&mut data));
    let closed = clipboard.close();
    result.and(closed)
}

struct TextMemory(Option<HGLOBAL>);
impl Drop for TextMemory {
    fn drop(&mut self) {
        if let Some(handle) = self.0.take() {
            let _ = unsafe { GlobalFree(Some(handle)) };
        }
    }
}
struct WindowsClipboard(HWND);
impl Clipboard for WindowsClipboard {
    type Data = TextMemory;
    fn prepare(&mut self, text: &str) -> Result<TextMemory, Fault> {
        let wide: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        let data = TextMemory(Some(
            unsafe { GlobalAlloc(GMEM_MOVEABLE, wide.len() * 2) }.map_err(io_error)?,
        ));
        let handle = data.0.unwrap();
        let destination = unsafe { GlobalLock(handle) }.cast::<u16>();
        if destination.is_null() {
            return Err(Fault::new(FaultCode::IoError));
        }
        // SAFETY: the allocation has exactly enough space for these UTF-16
        // units including NUL; it remains owned until successful publication.
        unsafe {
            std::ptr::copy_nonoverlapping(wide.as_ptr(), destination, wide.len());
        }
        // GlobalUnlock returns false when the lock count reaches zero.
        let _ = unsafe { GlobalUnlock(handle) };
        Ok(data)
    }
    fn open(&mut self) -> Result<(), Fault> {
        unsafe { OpenClipboard(Some(self.0)) }.map_err(io_error)
    }
    fn empty(&mut self) -> Result<(), Fault> {
        unsafe { EmptyClipboard() }.map_err(io_error)
    }
    fn publish(&mut self, data: &mut TextMemory) -> Result<(), Fault> {
        unsafe { SetClipboardData(CF_UNICODETEXT, Some(HANDLE(data.0.unwrap().0))) }
            .map_err(io_error)?;
        data.0 = None; // Windows now owns the movable allocation.
        Ok(())
    }
    fn close(&mut self) -> Result<(), Fault> {
        unsafe { CloseClipboard() }.map_err(io_error)
    }
}

pub fn copy_text(owner: usize, text: &str) -> Result<(), Fault> {
    if owner == 0 {
        return Err(Fault::new(FaultCode::InvalidState));
    }
    write_text(&mut WindowsClipboard(HWND(owner as *mut _)), text)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_text_allocation_is_nul_terminated_without_opening_clipboard() {
        let text = "한글 file.txt\r\nnext.txt";
        let data = WindowsClipboard(HWND::default()).prepare(text).unwrap();
        let expected: Vec<u16> = text.encode_utf16().chain(Some(0)).collect();
        let handle = data.0.unwrap();
        let pointer = unsafe { GlobalLock(handle) }.cast::<u16>();
        assert!(!pointer.is_null());
        assert_eq!(
            unsafe { std::slice::from_raw_parts(pointer, expected.len()) },
            expected
        );
        let _ = unsafe { GlobalUnlock(handle) };
        // Dropping this unpublished allocation frees it. No clipboard API ran.
    }
    #[test]
    fn absolute_and_relative_preserve_unicode_spaces_and_unc() {
        let paths = vec![
            r"c:\작업 폴더\한글 파일.txt".into(),
            r"C:\작업 폴더\other.txt".into(),
        ];
        assert_eq!(
            format_paths(&paths, None).unwrap(),
            "C:\\작업 폴더\\한글 파일.txt\r\nC:\\작업 폴더\\other.txt"
        );
        assert_eq!(
            format_paths(&paths, Some(r"C:\작업 폴더\")).unwrap(),
            "한글 파일.txt\r\nother.txt"
        );
        assert_eq!(
            format_paths(
                &[r"\\server\share\폴더\파일.txt".into()],
                Some(r"\\SERVER\share\폴더")
            )
            .unwrap(),
            "파일.txt"
        );
        assert_eq!(
            format_paths(&[r"C:\file.txt".into()], Some(r"C:\")).unwrap(),
            "file.txt"
        );
    }
    #[test]
    fn invalid_or_unrelated_bases_never_fabricate_relative_paths() {
        for base in [
            "",
            "relative",
            r"D:\",
            r"C:\other",
            r"C:\folder-more",
            r"\\other\share",
        ] {
            assert!(
                format_paths(&[r"C:\folder\a.txt".into()], Some(base)).is_err(),
                "{base}"
            );
        }
        assert!(format_paths(&[r"\\server\share2\a.txt".into()], Some(r"\\server\share")).is_err());
        assert!(format_paths(&[r"C:\folder".into()], Some(r"C:\folder")).is_err());
        for path in ["relative", "C:\\bad\nname", "C:\\bad\0name", r"\\?\C:\file"] {
            assert!(format_paths(&[path.into()], None).is_err());
        }
    }
    #[derive(Default)]
    struct Fake {
        fail: &'static str,
        calls: Vec<&'static str>,
        text: String,
    }
    impl Fake {
        fn step(&mut self, name: &'static str) -> Result<(), Fault> {
            self.calls.push(name);
            if self.fail == name {
                Err(Fault::new(FaultCode::IoError))
            } else {
                Ok(())
            }
        }
    }
    impl Clipboard for Fake {
        type Data = String;
        fn prepare(&mut self, text: &str) -> Result<String, Fault> {
            self.step("prepare")?;
            Ok(text.into())
        }
        fn open(&mut self) -> Result<(), Fault> {
            self.step("open")
        }
        fn empty(&mut self) -> Result<(), Fault> {
            self.step("empty")
        }
        fn publish(&mut self, data: &mut String) -> Result<(), Fault> {
            self.step("publish")?;
            self.text = data.clone();
            Ok(())
        }
        fn close(&mut self) -> Result<(), Fault> {
            self.step("close")
        }
    }
    #[test]
    fn clipboard_failure_never_reports_success_and_open_is_always_closed() {
        for (fail, expected) in [
            ("prepare", vec!["prepare"]),
            ("open", vec!["prepare", "open"]),
            ("empty", vec!["prepare", "open", "empty", "close"]),
            (
                "publish",
                vec!["prepare", "open", "empty", "publish", "close"],
            ),
            (
                "close",
                vec!["prepare", "open", "empty", "publish", "close"],
            ),
        ] {
            let mut fake = Fake {
                fail,
                ..Fake::default()
            };
            assert!(write_text(&mut fake, "한글 file.txt").is_err());
            assert_eq!(fake.calls, expected);
        }
        let mut fake = Fake::default();
        write_text(&mut fake, "한글 file.txt\r\nnext.txt").unwrap();
        assert_eq!(fake.text, "한글 file.txt\r\nnext.txt");
    }
    #[test]
    fn payload_limits_fail_before_touching_clipboard() {
        assert!(format_paths(&[], None).is_err());
        assert!(format_paths(&vec![r"C:\file".into(); 129], None).is_err());
        let mut fake = Fake::default();
        assert!(write_text(&mut fake, "a\0b").is_err());
        assert!(write_text(&mut fake, &"a".repeat(MAX_TEXT_BYTES + 1)).is_err());
        assert!(fake.calls.is_empty());
    }
}
