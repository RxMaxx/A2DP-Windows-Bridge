//! The main window by keyboard: Ctrl+Shift+D opens Diagnostics (not while the
//! setup shows), Tab walks the navigation and then the screen's controls in
//! order, Enter and Space follow a navigation link.

use gpui_kit::test::{TestQueryExt as _, TestWindowExt as _};
use gpui_kit::{AnyWindowHandle, AppContext as _, TestAppContext};
use serde_json::json;

use super::*;
use crate::testing::{Answer, Harness, snapshot};

fn open(cx: &mut TestAppContext, route: RouteName) -> (Harness, AnyWindowHandle) {
    let mut s = snapshot();
    s["settings"]["setup_done"] = json!(true);
    s["settings"]["cables_dialog_shown"] = json!(true);
    let mut h = Harness::start(cx, s);
    h.answer("audio.devices", Answer::Ok(json!([])));
    cx.update(|cx| {
        crate::bind_shortcuts(cx);
        app::navigate(route, cx);
    });
    let (w, _) = h.open_sized(cx, (1000., 1400.), |window, cx| cx.new(|cx| MainWindow::new(window, cx)));
    (h, w)
}

fn route(cx: &TestAppContext) -> RouteName {
    cx.read(|cx| app::state(cx).read(cx).route.name)
}

#[gpui_kit::test]
fn ctrl_shift_d_opens_diagnostics(cx: &mut TestAppContext) {
    let (mut h, w) = open(cx, RouteName::Settings);
    h.on(cx, w, |window, cx| window.press("ctrl-shift-d", cx));
    assert_eq!(route(cx), RouteName::Diagnostics);
}

#[gpui_kit::test]
fn ctrl_shift_d_does_nothing_while_the_setup_shows(cx: &mut TestAppContext) {
    let (mut h, w) = open(cx, RouteName::Settings);
    cx.update(|cx| app::state(cx).update(cx, |s, cx| s.set_setup_active(true, cx)));
    h.settle(cx);
    h.on(cx, w, |window, cx| window.press("ctrl-shift-d", cx));
    assert_eq!(route(cx), RouteName::Settings);
}

/// What has the keyboard: a control registered for tests by its id, else "?"
fn focused(h: &mut Harness, cx: &mut TestAppContext, w: AnyWindowHandle) -> String {
    h.on(cx, w, |window, _| {
        window
            .elements()
            .into_iter()
            .filter(|e| e.focused() == Some(true))
            .map(|e| format!("{:?}", e.path().last()))
            .next_back()
            .unwrap_or_else(|| "?".into())
    })
}

#[gpui_kit::test]
fn tab_walks_the_navigation_then_every_setting_in_order(cx: &mut TestAppContext) {
    let (mut h, w) = open(cx, RouteName::Settings);
    let mut seen = Vec::new();
    for _ in 0..60 {
        h.on(cx, w, |window, cx| window.press("tab", cx));
        seen.push(focused(&mut h, cx, w));
    }
    let at = |id: &str| seen.iter().position(|s| s.contains(&format!("\"{id}\"")));
    // Every control of the screen is reached, in the screen's order; "Start
    // minimized" is off while "Start with Windows" is, and skipped
    let order = [
        "nav-toggle",
        "language",
        "start-with-windows",
        "minimize-to-tray",
        "flyout-from",
        "notify-connected",
        "battery-low",
        "check-update",
        "check-updates",
        "include-prereleases",
        "developer-mode",
        "run-setup",
        "quit",
    ];
    assert_eq!(at("start-minimized"), None);
    let places: Vec<Option<usize>> = order.iter().map(|id| at(id)).collect();
    for (id, p) in order.iter().zip(&places) {
        assert!(p.is_some(), "Tab never reached {id}: {seen:?}");
    }
    let places: Vec<usize> = places.into_iter().flatten().collect();
    assert!(places.windows(2).all(|p| p[0] < p[1]), "out of order: {order:?} at {places:?} in {seen:?}");
    // The navigation's links come between its button and the screen: they
    // take the keyboard too (not registered for tests: "?")
    let links = seen[places[0] + 1..places[1]].iter().filter(|s| *s == "?").count();
    assert!(links >= 7, "{links} navigation links reached: {seen:?}");
}

#[gpui_kit::test]
fn enter_and_space_follow_a_navigation_link(cx: &mut TestAppContext) {
    let (mut h, w) = open(cx, RouteName::Settings);
    // The toggle, then Home's link
    h.on(cx, w, |window, cx| {
        window.press("tab", cx);
        window.press("tab", cx);
        window.press("tab", cx);
    });
    h.on(cx, w, |window, cx| window.press("enter", cx));
    assert_eq!(route(cx), RouteName::Codecs, "the second link is Codecs");
    h.on(cx, w, |window, cx| {
        window.press("shift-tab", cx);
        window.press("space", cx);
    });
    assert_eq!(route(cx), RouteName::Home);
}
