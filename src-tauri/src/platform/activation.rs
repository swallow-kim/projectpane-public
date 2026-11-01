use crate::domain::{AbsolutePath, Fault, FaultCode};

pub(crate) fn open_file(path: &str) -> Result<(), Fault> {
    let path = AbsolutePath::new(path.to_owned())?;
    #[cfg(windows)]
    {
        let _apartment = StaApartment::initialize()?;
        use std::{os::windows::ffi::OsStrExt, ptr};
        use windows_sys::Win32::UI::{Shell::ShellExecuteW, WindowsAndMessaging::SW_SHOWNORMAL};
        let wide: Vec<u16> = path
            .as_path()
            .as_os_str()
            .encode_wide()
            .chain(Some(0))
            .collect();
        // SAFETY: both strings are NUL-terminated and live through this call;
        // optional pointers are null. The returned status is never dereferenced.
        let result = unsafe {
            ShellExecuteW(
                ptr::null_mut(),
                windows_sys::w!("open"),
                wide.as_ptr(),
                ptr::null(),
                ptr::null(),
                SW_SHOWNORMAL,
            )
        };
        if result.addr() <= 32 {
            return Err(Fault::new(FaultCode::IoError));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = path;
        Err(Fault::new(FaultCode::InvalidState))
    }
}

#[cfg(windows)]
struct StaApartment(std::marker::PhantomData<std::rc::Rc<()>>);

#[cfg(windows)]
impl StaApartment {
    fn initialize() -> Result<Self, Fault> {
        use windows_sys::Win32::System::Com::{
            COINIT_APARTMENTTHREADED, COINIT_DISABLE_OLE1DDE, CoInitializeEx,
        };
        let flags = u32::try_from(COINIT_APARTMENTTHREADED | COINIT_DISABLE_OLE1DDE)
            .map_err(|_| Fault::new(FaultCode::InvalidState))?;
        // SAFETY: COM receives the required null reserved argument on this
        // dedicated thread; successful initialization is balanced on the same thread.
        let result = unsafe { CoInitializeEx(std::ptr::null(), flags) };
        if result < 0 {
            return Err(Fault::new(FaultCode::IoError));
        }
        Ok(Self(std::marker::PhantomData))
    }
}

#[cfg(windows)]
impl Drop for StaApartment {
    fn drop(&mut self) {
        // SAFETY: this non-Send guard exists only after successful initialization
        // and balances exactly one COM initialization on its originating thread.
        unsafe { windows_sys::Win32::System::Com::CoUninitialize() };
    }
}

#[cfg(all(test, windows))]
#[test]
fn sta_initialization_balances_nested_guards_on_a_dedicated_thread() {
    std::thread::spawn(|| {
        let first = StaApartment::initialize().unwrap();
        let nested = StaApartment::initialize().unwrap();
        drop(nested);
        drop(first);
        let _fresh = StaApartment::initialize().unwrap();
    })
    .join()
    .unwrap();
}
