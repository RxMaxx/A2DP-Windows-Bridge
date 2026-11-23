//! What a window of another toolkit needs from Windows itself: the title
//! bar's light or dark look.

use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};

/// Draws the window's title bar dark or light, whatever Windows' own app
/// theme is. False when Windows refused (before Windows 10 20H1).
pub fn set_dark_title_bar(hwnd: isize, dark: bool) -> bool {
    let value: i32 = dark.into();
    // SAFETY: the attribute takes a BOOL (4 bytes), read from `value` during
    // the call; a handle that is no window makes the call fail
    let result = unsafe {
        DwmSetWindowAttribute(
            HWND(hwnd as *mut _),
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            (&value as *const i32).cast(),
            size_of::<i32>() as u32,
        )
    };
    result.is_ok()
}
