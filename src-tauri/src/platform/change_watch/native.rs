//! Windows notification FFI only: unique ownership, no raw handles in the public API.
//! Real filesystem tests exercise creation/rearm/drop; Miri cannot execute Win32 calls.
use std::io;
use windows_sys::Win32::{
    Foundation::{HANDLE, INVALID_HANDLE_VALUE, WAIT_OBJECT_0, WAIT_TIMEOUT},
    Storage::FileSystem::{
        FILE_NOTIFY_CHANGE_ATTRIBUTES, FILE_NOTIFY_CHANGE_DIR_NAME, FILE_NOTIFY_CHANGE_FILE_NAME,
        FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SECURITY, FILE_NOTIFY_CHANGE_SIZE,
        FindCloseChangeNotification, FindFirstChangeNotificationW, FindNextChangeNotification,
    },
    System::Threading::WaitForSingleObject,
};

pub(super) struct Notification(HANDLE);

impl Notification {
    pub(super) fn open(path: &str) -> io::Result<Self> {
        if path.contains('\0') {
            return Err(io::ErrorKind::InvalidInput.into());
        }
        let path: Vec<u16> = path.encode_utf16().chain(Some(0)).collect();
        let filter = FILE_NOTIFY_CHANGE_FILE_NAME
            | FILE_NOTIFY_CHANGE_DIR_NAME
            | FILE_NOTIFY_CHANGE_ATTRIBUTES
            | FILE_NOTIFY_CHANGE_SIZE
            | FILE_NOTIFY_CHANGE_LAST_WRITE
            | FILE_NOTIFY_CHANGE_SECURITY;
        // SAFETY: FFI category8: the terminated UTF16 allocation lives through
        // this synchronous call; subtree=false and flags are documented filters.
        let handle = unsafe { FindFirstChangeNotificationW(path.as_ptr(), 0, filter) };
        if handle == INVALID_HANDLE_VALUE || handle.is_null() {
            Err(io::Error::last_os_error())
        } else {
            Ok(Self(handle))
        }
    }

    pub(super) fn poll(&mut self) -> io::Result<bool> {
        // SAFETY: handle category7: self uniquely owns a live notification;
        // no close/rearm can run concurrently, and zero wait never blocks.
        match unsafe { WaitForSingleObject(self.0, 0) } {
            WAIT_TIMEOUT => Ok(false),
            WAIT_OBJECT_0 => {
                // SAFETY: FFI category8: the previous wait signaled this live
                // notification. Rearm once; the next poll waits before rearming.
                if unsafe { FindNextChangeNotification(self.0) } == 0 {
                    Err(io::Error::last_os_error())
                } else {
                    Ok(true)
                }
            }
            _ => Err(io::Error::last_os_error()),
        }
    }
}

impl Drop for Notification {
    fn drop(&mut self) {
        // SAFETY: handle category7: open transferred sole ownership to self;
        // this API, not CloseHandle, releases the notification exactly once.
        unsafe {
            FindCloseChangeNotification(self.0);
        }
    }
}
