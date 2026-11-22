//! Group `shell`: the tray icon with its menu, balloons and starting the UI,
//! for a core without a tray of its own (`bluecaster-core.exe --headless`).
//!
//! The core decides what the tray shows and what a click does; the agent
//! only shows it and reports what the user did. Everything goes as events
//! (no replies), so nothing waits on the agent:
//!
//! - core to agent ([`ToAgent`]): the tray's model ([`TrayModel`]: tooltip
//!   and menu), a balloon, "start the UI" with the page to open.
//! - agent to core ([`ToCore`]): [`ToCore::Ready`] when a connection's
//!   handler starts (the core sends its current model then: a new agent
//!   shows the tray at once), a menu item chosen, a click on the icon,
//!   Quick Settings opened.
//!
//! The agent shows a tray only after the core sent a model: an agent that
//! serves a core with its own tray (the normal resident mode) shows none.
//! The tray goes when the connection ends (the core exited or crashed) and
//! comes back with the next core's model.

use serde::{Deserialize, Serialize};

use crate::{LocalHost, RemoteHost};

pub const GROUP: &str = "shell";

/// One entry of the tray menu
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum TrayItem {
    Item { id: String, label: String, enabled: bool },
    Check { id: String, label: String, enabled: bool, checked: bool },
    Separator,
}

/// What the tray shows. No items: no icon at all (service mode, after the
/// user's Exit).
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrayModel {
    pub tooltip: String,
    pub items: Vec<TrayItem>,
}

impl TrayModel {
    /// The menu's selectable items, by id
    pub fn item_ids(&self) -> impl Iterator<Item = &str> {
        self.items.iter().filter_map(|i| match i {
            TrayItem::Item { id, .. } | TrayItem::Check { id, .. } => Some(id.as_str()),
            TrayItem::Separator => None,
        })
    }
}

/// A balloon from the tray icon
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct BalloonModel {
    pub title: String,
    pub text: String,
    /// Opened by a click on it (http or https only)
    pub url: Option<String>,
}

/// What the UI opens with: the agent starts `bluecaster.exe` next to its
/// own program with this page's argument only (never one the core names)
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum UiPage {
    Main,
    Flyout,
    QuickSettings,
}

impl UiPage {
    pub fn arg(self) -> Option<&'static str> {
        match self {
            UiPage::Main => None,
            UiPage::Flyout => Some("--flyout"),
            UiPage::QuickSettings => Some("--quick-settings"),
        }
    }
}

/// Events of the core to the agent
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ToAgent {
    Tray { model: TrayModel },
    Balloon { balloon: BalloonModel },
    LaunchUi { page: UiPage },
}

/// Events of the agent to the core
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "snake_case")]
pub enum ToCore {
    /// A new connection: the core sends its model
    Ready,
    /// A menu item of the model was chosen
    Menu { id: String },
    /// A left click on the tray icon
    Click,
    /// Windows' Quick Settings opened (the flyout may open above them)
    QuickSettingsOpened,
}

/// The facade's shell methods. They do not wait: false when there is no
/// one to show it (this process has no shell of its own, or no agent).
pub trait ShellHost {
    /// Shows the tray with `model`, or updates it
    fn show_tray(&self, model: &TrayModel) -> bool;
    /// A balloon from the tray icon
    fn show_balloon(&self, balloon: &BalloonModel) -> bool;
    /// Starts the UI (a running UI gets the page instead)
    fn launch_ui(&self, page: UiPage) -> bool;
}

/// Starts `bluecaster.exe` next to this program with `page`'s argument
pub fn start_ui(page: UiPage) -> bool {
    let Some(dir) = std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.to_path_buf())) else {
        return false;
    };
    let ui = dir.join("bluecaster.exe");
    if !ui.exists() {
        tracing::warn!("shell: the UI's program is not there");
        return false;
    }
    let mut cmd = std::process::Command::new(ui);
    cmd.args(page.arg());
    match cmd.spawn() {
        Ok(_) => true,
        Err(e) => {
            tracing::warn!("shell: cannot start the UI ({})", e.kind());
            false
        }
    }
}

/// In this process: the resident mode's own tray (resident.rs) shows the
/// tray and balloons; only the UI starts from here
impl ShellHost for LocalHost {
    fn show_tray(&self, _model: &TrayModel) -> bool {
        false
    }

    fn show_balloon(&self, _balloon: &BalloonModel) -> bool {
        false
    }

    fn launch_ui(&self, page: UiPage) -> bool {
        start_ui(page)
    }
}

impl RemoteHost {
    fn shell_event(&self, event: &ToAgent) -> bool {
        self.hub().event(GROUP, crate::wire::to_body(event))
    }
}

impl ShellHost for RemoteHost {
    fn show_tray(&self, model: &TrayModel) -> bool {
        self.shell_event(&ToAgent::Tray { model: model.clone() })
    }

    fn show_balloon(&self, balloon: &BalloonModel) -> bool {
        self.shell_event(&ToAgent::Balloon { balloon: balloon.clone() })
    }

    fn launch_ui(&self, page: UiPage) -> bool {
        self.shell_event(&ToAgent::LaunchUi { page })
    }
}

/// [`ShellHost::show_tray`] of this process's host
pub fn show_tray(model: &TrayModel) -> bool {
    crate::host().show_tray(model)
}

/// [`ShellHost::show_balloon`] of this process's host
pub fn show_balloon(balloon: &BalloonModel) -> bool {
    crate::host().show_balloon(balloon)
}

/// [`ShellHost::launch_ui`] of this process's host
pub fn launch_ui(page: UiPage) -> bool {
    crate::host().launch_ui(page)
}

/// Calls `f` with each event of the agent's shell (on the hub's thread:
/// `f` must not block). Nothing when this process has no agent.
pub fn on_agent_event(f: impl Fn(ToCore) + Send + Sync + 'static) -> bool {
    let Some(hub) = crate::hub() else { return false };
    hub.on_event(GROUP, move |body| match serde_json::from_value::<ToCore>(body) {
        Ok(e) => f(e),
        Err(_) => tracing::debug!("shell: an event of the agent that does not decode"),
    });
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn events_have_a_stable_shape() {
        let model = TrayModel {
            tooltip: "BlueCaster".into(),
            items: vec![
                TrayItem::Item { id: "open".into(), label: "Open".into(), enabled: true },
                TrayItem::Separator,
                TrayItem::Check { id: "profile:0".into(), label: "X".into(), enabled: true, checked: false },
            ],
        };
        let json = serde_json::to_value(ToAgent::Tray { model: model.clone() }).unwrap();
        assert_eq!(json["op"], "tray");
        assert_eq!(json["model"]["items"][1]["kind"], "separator");
        assert_eq!(serde_json::from_value::<ToAgent>(json).unwrap(), ToAgent::Tray { model: model.clone() });
        assert_eq!(model.item_ids().collect::<Vec<_>>(), ["open", "profile:0"]);
        assert_eq!(serde_json::to_value(ToCore::Menu { id: "exit".into() }).unwrap()["op"], "menu");
        assert_eq!(
            serde_json::to_value(ToAgent::LaunchUi { page: UiPage::QuickSettings }).unwrap()["page"],
            "quick_settings"
        );
        assert_eq!(UiPage::Flyout.arg(), Some("--flyout"));
        assert_eq!(UiPage::Main.arg(), None);
    }

    #[test]
    fn the_local_host_has_no_tray() {
        assert!(!LocalHost.show_tray(&TrayModel::default()));
        assert!(!LocalHost.show_balloon(&BalloonModel { title: String::new(), text: "x".into(), url: None }));
    }
}
