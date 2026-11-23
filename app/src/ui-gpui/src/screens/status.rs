//! What the main window shows above or beside every screen
//!
//! Ported from app/src/ui/src/app/indicators.tsx and App.tsx (StatusPill).

use gpui_kit::{App, IntoElement, div};

/// A small red sign while an HCI capture runs; it leads to Diagnostics
pub fn capture_sign(_collapsed: bool, _cx: &App) -> impl IntoElement + use<> {
    div()
}

/// Always visible: each device and its state
pub fn status_pills(_collapsed: bool, _cx: &App) -> impl IntoElement + use<> {
    div()
}

/// The notices above every screen: input devices, a crash, an update
pub fn app_notices(_cx: &App) -> impl IntoElement + use<> {
    div()
}
