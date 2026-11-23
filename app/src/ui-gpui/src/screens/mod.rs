//! The screens of the main window, one module each, and the pieces around
//! them (status.rs: what the navigation and the top of every screen show).

pub mod flyout;
pub mod placeholder;
pub mod service_addon;
pub mod settings;
pub mod setup;
pub mod status;

use gpui_kit::{AnyView, App, AppContext as _, Window};

use crate::app::RouteName;

/// Makes the screen of `name`
pub fn open(name: RouteName, window: &mut Window, cx: &mut App) -> AnyView {
    match name {
        RouteName::Settings => cx.new(|cx| settings::Settings::new(window, cx)).into(),
        _ => cx.new(|_| placeholder::Placeholder::new(name)).into(),
    }
}
