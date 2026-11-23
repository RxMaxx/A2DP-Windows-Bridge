//! The channel hopping box typed into: Manual takes a list of Wi-Fi
//! channels (1-14, in any order and form ui_model::view::adapter
//! parse_afh_channels reads) when it is left or Enter is pressed; anything
//! else is not saved; Auto shows what it avoids and takes no typing.

use gpui_kit::test::TestWindowExt as _;
use gpui_kit::{AnyWindowHandle, TestAppContext};
use serde_json::json;

use super::*;
use crate::testing::{Harness, snapshot};

fn open(cx: &mut TestAppContext, afh: &str) -> (Harness, AnyWindowHandle, Entity<Adapter>) {
    let mut s = snapshot();
    s["settings"]["afh"] = json!(afh);
    let mut h = Harness::start(cx, s);
    let (w, a) = h.open_sized(cx, (1000., 1400.), |window, cx| cx.new(|cx| Adapter::new(window, cx)));
    (h, w, a)
}

/// The box takes the keys one at a time, as the user types (it shows the
/// adapter's state again whenever it draws)
fn type_channels(h: &mut Harness, cx: &mut TestAppContext, w: AnyWindowHandle, a: &Entity<Adapter>, text: &str) {
    let input = cx.read(|cx| a.read(cx).afh_input.clone());
    h.on(cx, w, |window, cx| input.update(cx, |s, cx| s.focus(window, cx)));
    // Select all and delete, then what the box shows is told before it draws
    cx.update_window(w, |_, window, cx| {
        window.press("ctrl-a", cx);
        window.dispatch_keystroke(gpui_kit::Keystroke::parse("backspace").unwrap(), cx);
    })
    .unwrap();
    h.settle(cx);
    h.type_text(cx, w, text);
}

fn saved_afh(h: &Harness) -> Vec<String> {
    h.calls("settings.set").iter().map(|p| p["settings"]["afh"].as_str().unwrap_or_default().to_owned()).collect()
}

#[gpui_kit::test]
fn manual_channels_are_saved_when_entered(cx: &mut TestAppContext) {
    let (mut h, w, a) = open(cx, "1,6");
    type_channels(&mut h, cx, w, &a, " 11 , 1 6 ");
    assert_eq!(cx.read(|cx| a.read(cx).afh_channels.clone()), " 11 , 1 6 ");
    assert!(saved_afh(&h).is_empty(), "saved while typing");
    h.key(cx, w, "enter");
    assert_eq!(saved_afh(&h), ["1,6,11"]);
    // Out of range or no number: kept in the box, not saved
    for bad in ["15", "0", "6;11", "abc"] {
        type_channels(&mut h, cx, w, &a, bad);
        h.key(cx, w, "enter");
        assert_eq!(saved_afh(&h).len(), 1, "{bad:?} saved");
        assert!(cx.read(|cx| afh_channels_bad(true, &a.read(cx).afh_channels)));
    }
    // An empty box saves nothing either
    type_channels(&mut h, cx, w, &a, "");
    h.key(cx, w, "enter");
    assert_eq!(saved_afh(&h).len(), 1);
    // Left with Tab
    type_channels(&mut h, cx, w, &a, "0x3,13");
    h.key(cx, w, "tab");
    assert_eq!(saved_afh(&h).last().map(String::as_str), Some("3,13"));
}

#[gpui_kit::test]
fn auto_takes_no_typing(cx: &mut TestAppContext) {
    let (mut h, w, a) = open(cx, "auto");
    type_channels(&mut h, cx, w, &a, "6");
    h.key(cx, w, "enter");
    assert!(saved_afh(&h).is_empty());
    assert_eq!(cx.read(|cx| a.read(cx).afh_input.read(cx).value().to_string()), "");
}
