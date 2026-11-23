//! The tray flyout: one compact view in a frameless window near the
//! notification area
//!
//! Ported from app/src/ui/src/screens/Flyout.tsx and the flyout's part of
//! app/src/ui/src-tauri/src/windows.rs.

use gpui_kit::App;

use crate::windows;

/// A click on the tray icon: opens the flyout, or closes it when it is open
pub fn toggle(cx: &mut App) {
    windows::show_main(None, true, cx);
}

/// Quick Settings just opened: the flyout goes with them, above them,
/// without taking the focus
pub fn show_with_quick_settings(_cx: &mut App) {}
