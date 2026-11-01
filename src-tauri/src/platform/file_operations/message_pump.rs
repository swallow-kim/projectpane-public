use std::os::windows::io::{AsRawHandle, OwnedHandle};
use windows_sys::Win32::{
    Foundation::WAIT_FAILED,
    UI::WindowsAndMessaging::{
        DispatchMessageW, MSG, MWMO_INPUTAVAILABLE, MsgWaitForMultipleObjectsEx, PM_REMOVE,
        PeekMessageW, QS_ALLINPUT, TranslateMessage, WM_QUIT,
    },
};
pub(super) fn pump(wake: &OwnedHandle) -> bool {
    let handles = [wake.as_raw_handle()];
    let mut message = MSG::default();
    // SAFETY: FFI category 8: the live owned handle and initialized MSG stay valid
    // for these synchronous calls. Only this STA removes/dispatches its messages.
    // The finite idle wait observes channel disconnect without force termination.
    unsafe {
        if MsgWaitForMultipleObjectsEx(1, handles.as_ptr(), 250, QS_ALLINPUT, MWMO_INPUTAVAILABLE)
            == WAIT_FAILED
        {
            return false;
        }
        while PeekMessageW(&mut message, std::ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
            if message.message == WM_QUIT {
                return false;
            }
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    true
}
