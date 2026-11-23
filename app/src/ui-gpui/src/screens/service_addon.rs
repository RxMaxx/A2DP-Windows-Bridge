//! The service add-on's notices and its card on Settings
//!
//! Ported from app/src/ui/src/components/ServiceAddon.tsx.

use gpui_kit::{App, IntoElement, div};

/// The connection screen while the service add-on is stopped or another version
pub fn connection_notice(_stopped: bool, _versions: Option<(&String, &String)>, _cx: &App) -> impl IntoElement + use<> {
    div()
}

/// The connection screen while the service add-on is newer than this window
pub fn newer_notice(_version: &str, _app: &str, _cx: &App) -> impl IntoElement + use<> {
    div()
}
