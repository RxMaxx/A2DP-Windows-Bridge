//! The tray flyout's page: one compact view (its window: flyout_window.rs)
//!
//! Ported from app/src/ui/src/screens/Flyout.tsx.

use gpui_kit::{AnyView, App, AppContext as _, Window};

use crate::app::RouteName;
use crate::screens::placeholder::Placeholder;

/// Makes the flyout's page
pub fn open(_window: &mut Window, cx: &mut App) -> AnyView {
    cx.new(|_| Placeholder::new(RouteName::Home)).into()
}
