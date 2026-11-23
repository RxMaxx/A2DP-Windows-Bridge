//! The first-run setup
//!
//! Ported from app/src/ui/src/screens/Setup.tsx.

use gpui_kit::{AnyView, App, AppContext as _, Window};
use ipc::Snapshot;

use crate::app::RouteName;
use crate::screens::placeholder::Placeholder;

/// Whether the first-run setup is needed: no profile yet
pub fn needs_setup(snap: &Snapshot) -> bool {
    snap.profiles.is_empty()
}

pub fn open(_window: &mut Window, cx: &mut App) -> AnyView {
    cx.new(|_| Placeholder::new(RouteName::Setup)).into()
}
