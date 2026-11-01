//! Opt-in Shell menu host. COM interfaces and HWND/HMENU never cross threads.
use super::{OleApartment, ShellSelection, native_error};
use crate::domain::{Fault, FaultCode};
use std::{
    cell::RefCell,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
};
use windows::{
    Win32::{
        Foundation::{
            ERROR_SUCCESS, GetLastError, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT,
            SetLastError, WPARAM,
        },
        System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
        UI::{
            Shell::{
                CMF_NORMAL, CMIC_MASK_PTINVOKE, CMINVOKECOMMANDINFO, CMINVOKECOMMANDINFOEX,
                IContextMenu, IContextMenu2, IContextMenu3, IShellFolder, SEE_MASK_NOASYNC,
                SEE_MASK_UNICODE, SHBindToParent,
            },
            WindowsAndMessaging::*,
        },
    },
    core::{Interface, PCSTR, PCWSTR},
};

const FIRST_COMMAND: u32 = 1;
const LAST_COMMAND: u32 = 0x7fff;

#[derive(Clone, Copy, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum MenuPhase {
    Preparing,
    Tracking,
    Invoked,
    Cancelled,
    Inactive,
    Failed,
}

#[derive(Default)]
pub struct MenuSession {
    active: AtomicBool,
    cancelled: AtomicBool,
    tracking_window: Mutex<Option<usize>>,
}
impl MenuSession {
    pub fn active(&self) -> bool {
        self.active.load(Ordering::Acquire)
    }
    pub fn begin(self: &Arc<Self>) -> Result<MenuLease, Fault> {
        self.active
            .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .map_err(|_| Fault::new(FaultCode::Busy))?;
        self.cancelled.store(false, Ordering::Release);
        Ok(MenuLease(Arc::clone(self)))
    }
    pub fn cancel(&self) {
        self.cancelled.store(true, Ordering::Release);
        // Hold the handle registration lock until PostMessage has copied it.
        // Unregistration precedes HWND destruction, preventing stale-handle use.
        let registered = self
            .tracking_window
            .lock()
            .unwrap_or_else(|p| p.into_inner());
        if let Some(window) = *registered {
            let _ = unsafe {
                PostMessageW(
                    Some(HWND(window as *mut _)),
                    WM_CANCELMODE,
                    WPARAM(0),
                    LPARAM(0),
                )
            };
        }
    }
}
pub struct MenuLease(Arc<MenuSession>);
impl Drop for MenuLease {
    fn drop(&mut self) {
        self.0.active.store(false, Ordering::Release);
    }
}

#[derive(Clone)]
struct Forwarder {
    menu2: Option<IContextMenu2>,
    menu3: Option<IContextMenu3>,
}
thread_local! { static FORWARDER: RefCell<Option<Forwarder>> = const { RefCell::new(None) }; }
struct ForwardGuard;
impl ForwardGuard {
    fn new(menu: &IContextMenu) -> Self {
        FORWARDER.with(|slot| {
            *slot.borrow_mut() = Some(Forwarder {
                menu2: menu.cast().ok(),
                menu3: menu.cast().ok(),
            })
        });
        Self
    }
}
impl Drop for ForwardGuard {
    fn drop(&mut self) {
        FORWARDER.with(|slot| slot.borrow_mut().take());
    }
}
fn forwards(message: u32) -> bool {
    matches!(
        message,
        WM_INITMENUPOPUP | WM_UNINITMENUPOPUP | WM_DRAWITEM | WM_MEASUREITEM | WM_MENUCHAR
    )
}
unsafe extern "system" fn menu_window(hwnd: HWND, message: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if message == WM_CANCELMODE || message == WM_CLOSE {
        let _ = unsafe { EndMenu() };
        return LRESULT(0);
    }
    if forwards(message) {
        // Clone before calling COM: an extension can dispatch nested messages.
        let interfaces =
            FORWARDER.with(|slot| slot.try_borrow().ok().and_then(|value| value.clone()));
        if let Some(interfaces) = interfaces {
            if let Some(menu) = interfaces.menu3 {
                let mut result = LRESULT(0);
                if unsafe { menu.HandleMenuMsg2(message, w, l, Some(&mut result)) }.is_ok() {
                    return result;
                }
            }
            if message != WM_MENUCHAR
                && let Some(menu) = interfaces.menu2
                && unsafe { menu.HandleMenuMsg(message, w, l) }.is_ok()
            {
                return LRESULT(0);
            }
        }
    }
    unsafe { DefWindowProcW(hwnd, message, w, l) }
}

struct OwnerWindow {
    hwnd: HWND,
    instance: HINSTANCE,
    class: Vec<u16>,
}
impl OwnerWindow {
    fn new(parent: Option<HWND>, point: POINT) -> Result<Self, Fault> {
        let instance = HINSTANCE(unsafe { GetModuleHandleW(None) }.map_err(native_error)?.0);
        let class: Vec<_> = format!("ProjectPaneShellMenu-{}", unsafe { GetCurrentThreadId() })
            .encode_utf16()
            .chain([0])
            .collect();
        let descriptor = WNDCLASSW {
            lpfnWndProc: Some(menu_window),
            hInstance: instance,
            lpszClassName: PCWSTR(class.as_ptr()),
            ..Default::default()
        };
        if unsafe { RegisterClassW(&descriptor) } == 0 {
            return Err(Fault::new(FaultCode::IoError));
        }
        let result = unsafe {
            CreateWindowExW(
                WS_EX_TOOLWINDOW,
                PCWSTR(class.as_ptr()),
                PCWSTR::null(),
                WS_POPUP,
                point.x,
                point.y,
                1,
                1,
                parent,
                None,
                Some(instance),
                None,
            )
        };
        match result {
            Ok(hwnd) => Ok(Self {
                hwnd,
                instance,
                class,
            }),
            Err(error) => {
                let _ = unsafe { UnregisterClassW(PCWSTR(class.as_ptr()), Some(instance)) };
                Err(native_error(error))
            }
        }
    }
}
impl Drop for OwnerWindow {
    fn drop(&mut self) {
        let _ = unsafe { DestroyWindow(self.hwnd) };
        let _ = unsafe { UnregisterClassW(PCWSTR(self.class.as_ptr()), Some(self.instance)) };
    }
}
struct Popup(HMENU);
impl Drop for Popup {
    fn drop(&mut self) {
        let _ = unsafe { DestroyMenu(self.0) };
    }
}
struct Tracking<'a>(&'a MenuSession);
impl<'a> Tracking<'a> {
    fn new(session: &'a MenuSession, window: HWND) -> Self {
        *session
            .tracking_window
            .lock()
            .unwrap_or_else(|p| p.into_inner()) = Some(window.0 as usize);
        Self(session)
    }
}
impl Drop for Tracking<'_> {
    fn drop(&mut self) {
        self.0
            .tracking_window
            .lock()
            .unwrap_or_else(|p| p.into_inner())
            .take();
    }
}

fn context(paths: &[String], owner: HWND) -> Result<IContextMenu, Fault> {
    if paths.len() > 128 {
        return Err(Fault::new(FaultCode::ResourceLimit));
    }
    let selection = ShellSelection::new(paths)?;
    let folder: IShellFolder =
        unsafe { SHBindToParent(selection.items[0].0, None) }.map_err(native_error)?;
    unsafe { folder.GetUIObjectOf(owner, &selection.children(), None) }.map_err(native_error)
}
fn populate(menu: &IContextMenu) -> Result<(Popup, u32), Fault> {
    let popup = Popup(unsafe { CreatePopupMenu() }.map_err(native_error)?);
    // No CMF_CANRENAME: inline Shell rename has no view callback in this host.
    let result =
        unsafe { menu.QueryContextMenu(popup.0, 0, FIRST_COMMAND, LAST_COMMAND, CMF_NORMAL) };
    result.ok().map_err(native_error)?;
    let count = (result.0 as u32) & 0xffff;
    if count == 0 || count > LAST_COMMAND || unsafe { GetMenuItemCount(Some(popup.0)) } <= 0 {
        return Err(Fault::new(FaultCode::InvalidState));
    }
    Ok((popup, count))
}
fn command_offset(command: u32, count: u32) -> Result<Option<u16>, Fault> {
    if command == 0 {
        return Ok(None);
    }
    if !(FIRST_COMMAND..=LAST_COMMAND).contains(&command) || command - FIRST_COMMAND >= count {
        return Err(Fault::new(FaultCode::InvalidRequest));
    }
    Ok(Some((command - FIRST_COMMAND) as u16))
}
fn invocation(owner: HWND, point: POINT, offset: u16) -> CMINVOKECOMMANDINFOEX {
    CMINVOKECOMMANDINFOEX {
        cbSize: std::mem::size_of::<CMINVOKECOMMANDINFOEX>() as u32,
        fMask: SEE_MASK_UNICODE | SEE_MASK_NOASYNC | CMIC_MASK_PTINVOKE,
        hwnd: owner,
        lpVerb: PCSTR(offset as usize as *const u8),
        lpVerbW: PCWSTR(offset as usize as *const u16),
        nShow: SW_SHOWNORMAL.0,
        ptInvoke: point,
        ..Default::default()
    }
}

/// Call on a dedicated thread. Returning true means InvokeCommand accepted the
/// request, not that an extension's external work completed successfully.
pub fn show(
    lease: MenuLease,
    parent: usize,
    paths: Vec<String>,
    phase: impl Fn(MenuPhase),
) -> Result<bool, Fault> {
    let session = &lease.0;
    let _ole = OleApartment::new()?;
    let parent = HWND(parent as *mut _);
    if !unsafe { IsWindow(Some(parent)) }.as_bool() {
        return Err(Fault::new(FaultCode::InvalidState));
    }
    if session.cancelled.load(Ordering::Acquire) {
        return Ok(false);
    }
    if unsafe { GetForegroundWindow() } != parent {
        phase(MenuPhase::Inactive);
        return Ok(false);
    }
    let mut rect = RECT::default();
    unsafe { GetWindowRect(parent, &mut rect) }.map_err(native_error)?;
    let mut point = POINT::default();
    unsafe { GetCursorPos(&mut point) }.map_err(native_error)?;
    if point.x < rect.left || point.x >= rect.right || point.y < rect.top || point.y >= rect.bottom
    {
        point = POINT {
            x: rect.left + (rect.right - rect.left) / 2,
            y: rect.top + (rect.bottom - rect.top) / 2,
        };
    }
    let owner = OwnerWindow::new(Some(parent), point)?;
    phase(MenuPhase::Preparing);
    let menu = context(&paths, owner.hwnd)?;
    let _forwarder = ForwardGuard::new(&menu);
    let (popup, count) = populate(&menu)?;
    if session.cancelled.load(Ordering::Acquire) {
        return Ok(false);
    }
    if unsafe { GetForegroundWindow() } != parent {
        phase(MenuPhase::Inactive);
        return Ok(false);
    }
    let _ = unsafe { ShowWindow(owner.hwnd, SW_SHOWNOACTIVATE) };
    if !unsafe { SetForegroundWindow(owner.hwnd) }.as_bool() {
        return Err(Fault::new(FaultCode::Busy));
    }
    let command = {
        let _tracking = Tracking::new(session, owner.hwnd);
        if session.cancelled.load(Ordering::Acquire) {
            0
        } else {
            phase(MenuPhase::Tracking);
            unsafe { SetLastError(ERROR_SUCCESS) };
            let command = unsafe {
                TrackPopupMenuEx(
                    popup.0,
                    (TPM_RETURNCMD | TPM_RIGHTBUTTON).0,
                    point.x,
                    point.y,
                    owner.hwnd,
                    None,
                )
            }
            .0 as u32;
            if command == 0 && unsafe { GetLastError() } != ERROR_SUCCESS {
                return Err(Fault::new(FaultCode::IoError));
            }
            command
        }
    };
    // Return focus only when it is still ours; never pull it from another app.
    if unsafe { GetForegroundWindow() } == owner.hwnd {
        let _ = unsafe { SetForegroundWindow(parent) };
    }
    let Some(offset) = command_offset(command, count)? else {
        return Ok(false);
    };
    if session.cancelled.load(Ordering::Acquire) || !unsafe { IsWindow(Some(parent)) }.as_bool() {
        return Ok(false);
    }
    let info = invocation(parent, point, offset);
    unsafe {
        menu.InvokeCommand((&info as *const CMINVOKECOMMANDINFOEX).cast::<CMINVOKECOMMANDINFO>())
    }
    .map_err(native_error)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancellation_and_command_offsets_never_invoke_an_unselected_verb() {
        assert_eq!(command_offset(0, 4).unwrap(), None);
        assert_eq!(command_offset(1, 4).unwrap(), Some(0));
        assert_eq!(command_offset(4, 4).unwrap(), Some(3));
        assert!(command_offset(5, 4).is_err());
        assert!(command_offset(u32::MAX, 4).is_err());
        let info = invocation(HWND::default(), POINT { x: -20, y: 30 }, 3);
        assert_eq!(info.lpVerb.0 as usize, 3);
        assert_eq!(info.lpVerbW.0 as usize, 3);
        assert_ne!(info.fMask & SEE_MASK_NOASYNC, 0);
        assert!(forwards(WM_INITMENUPOPUP) && forwards(WM_MENUCHAR));
        assert!(!forwards(WM_COMMAND));
    }
    #[test]
    fn session_excludes_duplicates_and_preserves_cancellation_until_cleanup() {
        let session = Arc::new(MenuSession::default());
        let lease = session.begin().unwrap();
        assert!(session.active());
        assert!(session.begin().is_err());
        session.cancel();
        assert!(session.cancelled.load(Ordering::Acquire));
        drop(lease);
        assert!(!session.active());
        let next = session.begin().unwrap();
        assert!(!session.cancelled.load(Ordering::Acquire));
        drop(next);
    }
    #[test]
    fn cancellation_targets_only_the_registered_owned_window() {
        std::thread::spawn(|| {
            let session = Arc::new(MenuSession::default());
            let lease = session.begin().unwrap();
            let owner = OwnerWindow::new(None, POINT::default()).unwrap();
            let tracking = Tracking::new(&session, owner.hwnd);
            session.cancel();
            let mut message = MSG::default();
            assert!(
                unsafe {
                    PeekMessageW(
                        &mut message,
                        Some(owner.hwnd),
                        WM_CANCELMODE,
                        WM_CANCELMODE,
                        PM_REMOVE,
                    )
                }
                .as_bool()
            );
            assert_eq!(message.hwnd, owner.hwnd);
            drop(tracking);
            session.cancel();
            assert!(
                !unsafe {
                    PeekMessageW(
                        &mut message,
                        Some(owner.hwnd),
                        WM_CANCELMODE,
                        WM_CANCELMODE,
                        PM_REMOVE,
                    )
                }
                .as_bool()
            );
            drop(owner);
            drop(lease);
            assert!(!session.active());
        })
        .join()
        .unwrap();
    }
    #[test]
    fn native_shell_menu_builds_for_unicode_multi_selection_without_display_or_invocation() {
        std::thread::spawn(|| {
            let _ole = OleApartment::new().unwrap();
            let root = tempfile::tempdir().unwrap();
            let paths: Vec<_> = ["한글 첫 번째.txt", "second.txt"]
                .into_iter()
                .map(|name| {
                    let path = root.path().join(name);
                    std::fs::write(&path, b"owned fixture").unwrap();
                    path.to_str().unwrap().to_owned()
                })
                .collect();
            let owner = OwnerWindow::new(None, POINT::default()).unwrap();
            let window = owner.hwnd;
            let menu = context(&paths, window).unwrap();
            let forwarder = ForwardGuard::new(&menu);
            let (popup, count) = populate(&menu).unwrap();
            assert!(count > 0);
            assert!(unsafe { GetMenuItemCount(Some(popup.0)) } > 0);
            // No TrackPopupMenu or InvokeCommand, no interactive clipboard access.
            drop(popup);
            drop(forwarder);
            drop(menu);
            drop(owner);
            assert!(!unsafe { IsWindow(Some(window)) }.as_bool());
            for path in paths {
                assert_eq!(std::fs::read(path).unwrap(), b"owned fixture");
            }
        })
        .join()
        .unwrap();
    }
}
