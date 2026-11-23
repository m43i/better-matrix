//! Sets the Windows taskbar and caption icons from the executable's icon resource.

use std::{io, ptr};

use raw_window_handle::RawWindowHandle;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::UI::WindowsAndMessaging::{ICON_BIG, ICON_SMALL, LoadIconW, SendMessageW, WM_SETICON};

#[expect(unsafe_code, reason = "Rune exposes the HWND for native window settings")]
pub fn set(window: &rune::WindowHandle) {
    let result = window.with_raw_handle(|handle| {
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return Ok(());
        };
        // SAFETY: Rune lends us the live HWND on its owning thread. LoadIconW returns a shared
        // handle owned by Windows, so the window can keep using it after this callback returns.
        unsafe {
            let module = GetModuleHandleW(ptr::null());
            let icon = LoadIconW(module, windows_sys::w!("BETTER_MATRIX_ICON"));
            if icon.is_null() {
                return Err(io::Error::last_os_error().to_string());
            }
            let hwnd = handle.hwnd.get() as _;
            SendMessageW(hwnd, WM_SETICON, ICON_BIG as usize, icon as isize);
            SendMessageW(hwnd, WM_SETICON, ICON_SMALL as usize, icon as isize);
        }
        Ok(())
    });
    if let Err(error) = result.and_then(|result| result) {
        eprintln!("better-matrix: could not set the window icon: {error}");
    }
}
