//! The main window's root: navigation on the left, the screen on the right,
//! the first-run setup over both while it runs.
//!
//! Ported from app/src/ui/src/app/App.tsx (MainWindow, Gate, ConnectionScreen).

use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::Sizable as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyView, App, Context, Entity, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    Render, SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};
use ui_model::i18n::{self, t};

use crate::app::{self, AppState, NoticeKind, RouteName};
use crate::core::{self, ConnStatus, Core, WantKind};
use crate::screens;
use crate::theme::{self, ActivePalette as _, ThemeSetting};
use crate::ui::{self, IconName, icon};

const NAV: [(RouteName, IconName, &str); 9] = [
    (RouteName::Home, IconName::Home, "ui.nav.home"),
    (RouteName::Codecs, IconName::Headphones, "ui.nav.codecs"),
    (RouteName::Devices, IconName::Bluetooth, "ui.nav.devices"),
    (RouteName::Adapter, IconName::UsbStick, "ui.nav.adapter"),
    (RouteName::Mixer, IconName::Speaker2, "ui.nav.mixer"),
    (RouteName::Diagnostics, IconName::Pulse, "ui.nav.diagnostics"),
    (RouteName::Settings, IconName::Settings, "ui.nav.settings"),
    (RouteName::About, IconName::Info, "ui.nav.about"),
    // Shown in developer mode only: last, out of the way
    (RouteName::Developer, IconName::Wrench, "ui.nav.developer"),
];

/// Below this width the navigation shows icons only, unless opened
const NARROW: f32 = 860.;
const NAV_WIDTH: f32 = 240.;
const NAV_COLLAPSED: f32 = 52.;

pub struct MainWindow {
    core: Entity<Core>,
    state: Entity<AppState>,
    /// None: by the window's width
    collapsed: Option<bool>,
    /// The screen on show, made again when the route changes (its view
    /// state starts over, and what it asked the core for is released)
    screen: Option<(RouteName, AnyView)>,
    setup: Option<AnyView>,
    /// Whether the setup was decided for the state that first arrived
    setup_decided: bool,
    language: String,
    theme: String,
}

impl MainWindow {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> MainWindow {
        // Windows' accent colour and theme, read again when the window gets
        // the focus, so a change in Settings shows
        cx.observe_window_activation(window, |_, _, cx| theme::refresh(cx)).detach();
        cx.observe_window_appearance(window, |_, _, cx| theme::refresh(cx)).detach();
        let core = app::core(cx);
        let state = app::state(cx);
        cx.observe(&core, |this, _, cx| {
            this.follow_settings(cx);
            cx.notify();
        })
        .detach();
        cx.observe(&state, |_, _, cx| cx.notify()).detach();
        // The log lines feed the connection flow, so the main window listens
        // while it is open (the core only sends them to subscribers)
        core::want(WantKind::Logs(ipc::LogLevel::Info), cx);
        let mut this = MainWindow {
            core,
            state,
            collapsed: None,
            screen: None,
            setup: None,
            setup_decided: false,
            language: String::new(),
            theme: String::new(),
        };
        this.follow_settings(cx);
        this
    }

    /// Language and theme come from the core's settings; the first-run
    /// setup is decided once when the state first arrives
    fn follow_settings(&mut self, cx: &mut Context<Self>) {
        let core = self.core.read(cx);
        let Some(snap) = core.snap() else { return };
        let (language, theme_name) = (snap.settings.language.clone(), snap.settings.theme.clone());
        // The first-run setup: decided once when the state first arrives; it
        // stays until finished or skipped, even when creating the profile
        // makes it no longer "needed". Only until it was finished or skipped
        // once (issue #86): with every device removed later, Home adds one. A
        // folder set up without it needs it no more.
        let decide = !self.setup_decided && !snap.settings.setup_done;
        let needs = decide && screens::setup::needs_setup(snap);
        self.setup_decided = true;
        if language != self.language {
            self.language = language;
            if i18n::set_language(&self.language) {
                cx.refresh_windows();
            }
        }
        if theme_name != self.theme {
            self.theme = theme_name;
            theme::apply(ThemeSetting::parse(&self.theme), cx);
        }
        if needs {
            self.state.update(cx, |s, cx| s.set_setup_active(true, cx));
        } else if decide {
            app::save_settings(|s| s.setup_done = true, cx);
        }
    }

    fn nav_shown(&self, name: RouteName, cx: &App) -> bool {
        let settings = self.core.read(cx).snap().map(|s| &s.settings);
        match name {
            RouteName::Mixer => app::mixer_shown(settings),
            RouteName::Developer => settings.is_some_and(|s| s.developer_mode),
            _ => true,
        }
    }

    /// The screen of the route, made when the route changed
    fn screen(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyView {
        let mut name = self.state.read(cx).route.name;
        // A screen that is not offered (any more) shows Home's place: Settings
        if name == RouteName::Setup || !self.nav_shown(name, cx) {
            name = RouteName::Settings;
        }
        match &self.screen {
            Some((n, view)) if *n == name => view.clone(),
            _ => {
                let view = screens::open(name, window, cx);
                self.screen = Some((name, view.clone()));
                view
            }
        }
    }

    fn render_nav(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = *cx.palette();
        let narrow = window.viewport_size().width < px(NARROW);
        let collapsed = self.collapsed.unwrap_or(narrow);
        let current = self.state.read(cx).route.name;
        let toggle = t("ui.nav.toggle");
        div()
            .id("nav")
            .flex()
            .flex_col()
            .flex_none()
            .gap(theme::S2)
            .w(px(if collapsed { NAV_COLLAPSED } else { NAV_WIDTH }))
            .h_full()
            .px(theme::S2)
            .py(theme::S3)
            .bg(p.bg_nav)
            .border_r_1()
            .border_color(p.border)
            .overflow_y_scroll()
            .child(
                div().flex().pb(theme::S2).child(ui::icon_button("nav-toggle", IconName::Navigation, toggle).on_click(
                    cx.listener(move |this, _, _, cx| {
                        this.collapsed = Some(!collapsed);
                        cx.notify();
                    }),
                )),
            )
            .child(div().flex().flex_col().flex_1().gap(px(2.)).children(
                NAV.iter().filter(|(name, _, _)| self.nav_shown(*name, cx)).map(|(name, icon_name, label)| {
                    let (name, on) = (*name, *name == current);
                    let label: SharedString = t(label).into();
                    div()
                        .id(name.as_str())
                        .relative()
                        .flex()
                        .items_center()
                        .gap(theme::S3)
                        .h(px(36.))
                        .px(theme::S3)
                        .rounded(theme::RADIUS)
                        .cursor_pointer()
                        .text_color(if on { p.text } else { p.text_2 })
                        .when(on, |d| {
                            d.bg(p.surface_2).font_weight(FontWeight::SEMIBOLD).child(
                                // Windows' selection mark at the item's left
                                div().absolute().left_0().top(px(9.)).bottom(px(9.)).w(px(3.)).rounded(px(2.)).bg(p.accent_fill),
                            )
                        })
                        .hover(|s| s.bg(p.surface_2).text_color(p.text))
                        .on_click(cx.listener(move |_, _, _, cx| app::navigate(name, cx)))
                        .child(icon(*icon_name))
                        .when(!collapsed, |d| d.child(div().whitespace_nowrap().child(label.clone())))
                        .when(collapsed, |d| d.tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(label.clone()).build(window, cx)))
                }),
            ))
            .child(screens::status::capture_sign(collapsed, cx))
            .child(screens::status::status_pills(collapsed, cx))
    }
}

impl Render for MainWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = *cx.palette();
        let ready = self.core.read(cx).ready();
        let setup_active = self.state.read(cx).setup_active || self.state.read(cx).route.name == RouteName::Setup;
        let root = div().size_full().relative().flex().bg(p.bg).text_color(p.text).text_size(theme::FS_14).font_family(theme::FONT);
        if !ready {
            self.screen = None;
            self.setup = None;
            return root.child(connection_screen(&self.core.read(cx).conn, false, cx)).into_any_element();
        }
        let body = if setup_active {
            self.screen = None;
            let setup = self.setup.get_or_insert_with(|| screens::setup::open(window, cx)).clone();
            root.child(div().size_full().child(setup))
        } else {
            self.setup = None;
            let screen = self.screen(window, cx);
            root.child(self.render_nav(window, cx)).child(
                div()
                    .flex()
                    .flex_col()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .child(screens::status::app_notices(cx))
                    .child(div().flex_1().min_h_0().child(screen)),
            )
        };
        body.child(notice_line(cx)).into_any_element()
    }
}

/// The screen until the core answered: connecting, missing, wrong version
pub fn connection_screen(conn: &ConnStatus, compact: bool, cx: &App) -> impl IntoElement + use<> {
    let p = cx.palette();
    let title = |key: &str| div().text_size(theme::FS_20).font_weight(FontWeight::SEMIBOLD).child(t(key));
    let base = div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(theme::S3)
        .p(if compact { theme::S4 } else { theme::S8 })
        .text_center();
    match conn {
        ConnStatus::CoreMissing { detail } => base
            .child(title("ui.conn.missing_title"))
            .child(div().child(t("ui.conn.missing_body")))
            .child(ui::mono(detail.clone()).text_color(p.text_2)),
        ConnStatus::Rejected { reason } => base
            .child(title("ui.conn.rejected_title"))
            .child(div().child(t("ui.conn.rejected_body")))
            .child(ui::mono(reason.clone()).text_color(p.text_2)),
        ConnStatus::ServiceNeedsUpdate { version, app } => {
            base.child(screens::service_addon::connection_notice(false, Some((version, app)), cx))
        }
        ConnStatus::ServiceNewer { version, app } => base.child(screens::service_addon::newer_notice(version, app, cx)),
        ConnStatus::ServiceStopped => base.child(screens::service_addon::connection_notice(true, None, cx)),
        ConnStatus::Connecting | ConnStatus::Connected { .. } => {
            base.child(Spinner::new().large()).child(title("ui.conn.connecting"))
        }
    }
}

/// The transient notice ("Copied", errors of buttons) at the window's bottom
pub fn notice_line(cx: &App) -> impl IntoElement + use<> {
    let p = cx.palette();
    let notice = app::state(cx).read(cx).notice.clone();
    div().absolute().bottom(theme::S4).left_0().right_0().flex().justify_center().when_some(notice, |d, n| {
        let (name, color, line) = match n.kind {
            NoticeKind::Error => (IconName::ErrorCircle, p.bad_text, p.bad_text),
            NoticeKind::Hint => (IconName::Info, p.text_2, p.border_strong),
            NoticeKind::Info => (IconName::CheckmarkCircle, p.good_text, p.border_strong),
        };
        d.child(
            div()
                .flex()
                .items_center()
                .gap(theme::S2)
                .px(theme::S4)
                .py(theme::S2)
                .mx(theme::S4)
                .bg(p.surface)
                .border_1()
                .border_color(line)
                .rounded(theme::RADIUS)
                .child(div().text_color(color).child(icon(name)))
                .child(n.text),
        )
    })
}
