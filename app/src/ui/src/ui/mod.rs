//! The controls the screens are made of: the component library's buttons,
//! switches, menus and dialogs in Windows 11's shapes (theme.rs), and the
//! lines and cards the screens share: [`button`], [`icon_button`],
//! [`toggle`], [`check`], [`segmented`], [`select`], [`field`], [`section`],
//! [`banner`], [`nav_row`] and [`help_tip`].

pub mod cable_notes;
pub mod capture_picker;
pub mod charts;
pub mod codec_cards;
pub mod codec_details;
pub mod device_picker;
pub mod device_settings;
pub mod device_switcher;
pub mod dialog;
pub mod icons;
pub mod import_pairings;
pub mod input_source;
pub mod media_eq;
pub mod mic_processing;
pub mod mixer;
pub mod paired_devices;
pub mod pairing_dialog;
pub mod stream;

use std::rc::Rc;

use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::component::switch::Switch;
use gpui_kit::component::tab::{Tab, TabBar};
use gpui_kit::component::{Disableable as _, Icon, Sizable as _};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, ClickEvent, Div, ElementId, FontWeight, InteractiveElement as _, IntoElement, ParentElement as _,
    SharedString, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};

pub use icons::IconName;

use crate::theme::{self, ActivePalette as _};

/// A Fluent icon (20 px grid); takes the text colour
pub fn icon(name: IconName) -> Icon {
    let svg = format!(
        r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor"><path d="{}"/></svg>"#,
        name.path()
    );
    Icon::default().data(svg.as_bytes())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ButtonKind {
    /// Windows' standard button
    #[default]
    Default,
    /// No fill until hovered (links to pages, less important actions)
    Subtle,
    Danger,
    /// The accent colour: the one thing to do next
    Strong,
}

/// A button with its text; add `.icon(icon(..))`, `.loading(busy)`,
/// `.disabled(..)` and `.on_click(..)` as needed
pub fn button(id: impl Into<ElementId>, label: impl Into<SharedString>, kind: ButtonKind) -> Button {
    let b = Button::new(id).label(label);
    match kind {
        ButtonKind::Default => b,
        ButtonKind::Subtle => b.ghost(),
        ButtonKind::Danger => b.danger(),
        ButtonKind::Strong => b.primary(),
    }
}

/// Icon-only button: the label is for screen readers and the tooltip
pub fn icon_button(id: impl Into<ElementId>, name: IconName, label: impl Into<SharedString>) -> Button {
    let label = label.into();
    Button::new(id).ghost().icon(icon(name)).accessibility_label(label.clone()).tooltip(label)
}

/// One choice of a [`select`] or [`segmented`]
#[derive(Debug, Clone, PartialEq)]
pub struct Choice<T> {
    pub value: T,
    pub label: SharedString,
    pub icon: Option<IconName>,
    pub disabled: bool,
    /// A select's choices with the same group (next to each other) go under
    /// one heading
    pub group: Option<SharedString>,
}

impl<T> Choice<T> {
    pub fn new(value: T, label: impl Into<SharedString>) -> Choice<T> {
        Choice { value, label: label.into(), icon: None, disabled: false, group: None }
    }

    pub fn icon(mut self, icon: IconName) -> Self {
        self.icon = Some(icon);
        self
    }

    pub fn disabled(mut self, disabled: bool) -> Self {
        self.disabled = disabled;
        self
    }

    pub fn group(mut self, group: impl Into<SharedString>) -> Self {
        self.group = Some(group.into());
        self
    }
}

/// A drop-down list: the chosen value on a button, the choices in a menu
/// with a check mark on the chosen one
pub fn select<T: Clone + PartialEq + 'static>(
    id: impl Into<ElementId>,
    value: &T,
    choices: Vec<Choice<T>>,
    disabled: bool,
    on_change: impl Fn(&T, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    select_named(id.into(), None, value, choices, disabled, on_change)
}

/// A [`select`] with a name for screen readers,
/// for a list without a visible label of its own beside it: it is announced
/// as "<label>: <the chosen value>"
pub fn select_labelled<T: Clone + PartialEq + 'static>(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    value: &T,
    choices: Vec<Choice<T>>,
    disabled: bool,
    on_change: impl Fn(&T, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    select_named(id.into(), Some(label.into()), value, choices, disabled, on_change)
}

fn select_named<T: Clone + PartialEq + 'static>(
    id: ElementId,
    label: Option<SharedString>,
    value: &T,
    choices: Vec<Choice<T>>,
    disabled: bool,
    on_change: impl Fn(&T, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let current = choices.iter().find(|c| c.value == *value).map(|c| c.label.clone()).unwrap_or_default();
    let name = label.map(|l| if current.is_empty() { l } else { format!("{l}: {current}").into() });
    let value = value.clone();
    let on_change = Rc::new(on_change);
    // The menu opens on the press itself; the click handler does nothing but
    // tell assistive technology that the list can be pressed
    let button = Button::new(id).label(current).dropdown_caret(true).disabled(disabled).on_click(|_, _, _| {});
    let button = match name {
        Some(name) => button.accessibility_label(name),
        None => button,
    };
    button.dropdown_menu(move |mut menu, _, _| {
        let mut group: Option<SharedString> = None;
        for c in &choices {
            if c.group != group {
                group = c.group.clone();
                menu = menu.separator();
                if let Some(g) = &group {
                    menu = menu.label(g.clone());
                }
            }
            let (v, on_change) = (c.value.clone(), on_change.clone());
            let chosen = c.value == value;
            let mut item = PopupMenuItem::new(c.label.clone()).checked(chosen).disabled(c.disabled);
            if let Some(i) = c.icon {
                item = item.icon(icon(i));
            }
            // The value it has chosen again changes nothing (as a list in a
            // browser): the flyout's device list would start its stream again
            menu = menu.item(item.on_click(move |_: &ClickEvent, window, cx| {
                if !chosen {
                    on_change(&v, window, cx);
                }
            }));
        }
        menu
    })
}

/// A few choices side by side, one chosen
pub fn segmented<T: Clone + PartialEq + 'static>(
    id: impl Into<ElementId>,
    value: &T,
    choices: Vec<Choice<T>>,
    on_change: impl Fn(&T, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let selected = choices.iter().position(|c| c.value == *value).unwrap_or(0);
    let values: Vec<T> = choices.iter().map(|c| c.value.clone()).collect();
    TabBar::new(id)
        .segmented()
        .selected_index(selected)
        .children(choices.into_iter().map(|c| {
            let mut tab = Tab::new().label(c.label).disabled(c.disabled);
            if let Some(i) = c.icon {
                tab = tab.prefix(icon(i).small());
            }
            tab
        }))
        .on_click(move |index: &usize, window, cx| {
            if let Some(v) = values.get(*index) {
                on_change(v, window, cx);
            }
        })
}

/// A page's tabs, each with its icon before its name (Diagnostics, Developer)
pub fn tab_bar<T: Clone + PartialEq + 'static>(
    id: impl Into<ElementId>,
    current: &T,
    tabs: Vec<Choice<T>>,
    on_select: impl Fn(&T, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let selected = tabs.iter().position(|c| c.value == *current).unwrap_or(0);
    let values: Vec<T> = tabs.iter().map(|c| c.value.clone()).collect();
    TabBar::new(id)
        .underline()
        .selected_index(selected)
        .children(tabs.into_iter().map(|c| {
            let tab = Tab::new().label(c.label);
            // The prefix sits outside the tab's padding: the icon gets its own
            match c.icon {
                Some(i) => tab.prefix(div().flex().items_center().pl(theme::S2).child(icon(i))),
                None => tab,
            }
        }))
        .on_click(move |index: &usize, window, cx| {
            if let Some(v) = values.get(*index) {
                on_select(v, window, cx);
            }
        })
}

/// Secondary text under a label or in a card (`.hint`)
pub fn hint(text: impl Into<SharedString>, cx: &App) -> Div {
    div().text_size(theme::FS_12).text_color(cx.palette().text_2).child(text.into())
}

/// Dimmed text (`.muted`)
pub fn muted(text: impl Into<SharedString>, cx: &App) -> Div {
    div().text_color(cx.palette().text_2).child(text.into())
}

/// Text in the monospace font (addresses, versions, log lines)
pub fn mono(text: impl Into<SharedString>) -> Div {
    div().font_family(theme::FONT_MONO).child(text.into())
}

/// A line of controls side by side that wraps (`.inline.wrap`)
pub fn inline() -> Div {
    div().flex().flex_wrap().items_center().gap(theme::S2)
}

/// A line of a card: its text on the left, the control on the right (`.row`)
pub fn row() -> Div {
    div().flex().items_center().justify_between().gap(theme::S4).min_h(px(40.))
}

fn row_text(label: SharedString, hint_text: Option<SharedString>, cx: &App) -> Div {
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_w_0()
        .gap(px(2.))
        .child(div().child(label))
        .when_some(hint_text, |d, h| d.child(hint(h, cx)))
}

/// A switch with its label and hint on the left (a setting's line). The
/// switch shows `checked`: the owner writes the new value and draws again.
pub fn toggle(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    hint_text: Option<SharedString>,
    checked: bool,
    disabled: bool,
    on_change: impl Fn(&bool, &mut Window, &mut App) + 'static,
    cx: &App,
) -> Div {
    let label = label.into();
    row()
        .child(row_text(label.clone(), hint_text, cx))
        .child(Switch::new(id).checked(checked).disabled(disabled).accessibility_label(label).on_change(on_change))
}

/// A small switch with its label after it, for options inside a line or a
/// dialog (every on/off option is a switch, issue #86)
pub fn check(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    checked: bool,
    disabled: bool,
    on_change: impl Fn(&bool, &mut Window, &mut App) + 'static,
) -> Switch {
    Switch::new(id).small().checked(checked).disabled(disabled).label(label.into()).on_change(on_change)
}

/// A labelled control with an optional hint and error: the label and hint
/// on the left and the control on the right, as a [`toggle`] (issue #86)
pub fn field(
    label: impl Into<SharedString>,
    hint_text: Option<SharedString>,
    error: Option<SharedString>,
    control: impl IntoElement,
    cx: &App,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(theme::S1)
        .child(row().child(row_text(label.into(), hint_text, cx)).child(control))
        .when_some(error, |d, e| d.child(field_error(e, cx)))
}

/// As [`field`] with the control below the label, for one that needs the width
pub fn field_stacked(
    label: impl Into<SharedString>,
    hint_text: Option<SharedString>,
    error: Option<SharedString>,
    control: impl IntoElement,
    cx: &App,
) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(theme::S2)
        .child(div().child(label.into()))
        .child(control)
        .when_some(hint_text, |d, h| d.child(hint(h, cx)))
        .when_some(error, |d, e| d.child(field_error(e, cx)))
}

fn field_error(text: SharedString, cx: &App) -> Div {
    div()
        .flex()
        .items_center()
        .gap(theme::S1)
        .text_size(theme::FS_12)
        .text_color(cx.palette().bad_text)
        .child(icon(IconName::ErrorCircle).small())
        .child(text)
}

/// A line that opens more settings: its name, its state, an arrow. The
/// whole line is the button.
pub fn nav_row(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    value: impl Into<SharedString>,
    disabled: bool,
    on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    cx: &App,
) -> impl IntoElement {
    let p = *cx.palette();
    let (label, value): (SharedString, SharedString) = (label.into(), value.into());
    row()
        .id(id)
        // A button for assistive technology, named "<label>: <value>"
        .role(gpui_kit::Role::Button)
        .aria_label(SharedString::from(format!("{label}: {value}")))
        .px(theme::S2)
        .rounded(theme::RADIUS)
        .when(!disabled, |d| d.cursor_pointer().hover(|s| s.bg(p.surface_2)).on_click(on_click))
        .when(disabled, |d| d.text_color(p.disabled_text))
        .child(div().child(label))
        .child(
            div()
                .flex()
                .items_center()
                .gap(theme::S2)
                .text_color(if disabled { p.disabled_text } else { p.text_2 })
                .child(value)
                .child(icon(IconName::ChevronRight).small()),
        )
}

/// A card (`.card`): children one below the other
pub fn card(cx: &App) -> Div {
    let p = cx.palette();
    div()
        .flex()
        .flex_col()
        .gap(theme::S3)
        .p(theme::S4)
        .bg(p.surface)
        .border_1()
        .border_color(p.border)
        .rounded(theme::RADIUS_CARD)
}

/// A card section with a heading, and its actions at the heading's right
pub fn section(title: impl Into<SharedString>, actions: Option<AnyElement>, cx: &App) -> Div {
    card(cx).child(
        div()
            .flex()
            .items_center()
            .justify_between()
            .gap(theme::S3)
            .child(div().text_size(theme::FS_16).font_weight(FontWeight::SEMIBOLD).child(title.into()))
            .when_some(actions, |d, a| d.child(a)),
    )
}

/// A screen: its heading, then its cards, scrolling as one
pub fn page(id: impl Into<ElementId>, title: impl Into<SharedString>) -> impl gpui_kit::ParentElement + IntoElement {
    div().id(id).size_full().flex().flex_col().gap(theme::S4).p(theme::S6).overflow_y_scrollbar().child(
        div()
            .font_family(theme::FONT_DISPLAY)
            .text_size(theme::FS_28)
            .font_weight(FontWeight::SEMIBOLD)
            .child(title.into()),
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BannerKind {
    Info,
    Warn,
    Error,
    Good,
}

/// What a click runs (a banner's close button)
pub type ClickHandler = Box<dyn Fn(&ClickEvent, &mut Window, &mut App)>;

/// A message in a card of its own: an icon, a title, text below it, then
/// buttons; `on_dismiss` adds a close button
pub fn banner(
    kind: BannerKind,
    title: Option<SharedString>,
    body: Option<AnyElement>,
    actions: Option<AnyElement>,
    on_dismiss: Option<(ElementId, ClickHandler)>,
    cx: &App,
) -> Div {
    let p = cx.palette();
    let (name, line, text) = match kind {
        BannerKind::Info => (IconName::Info, p.border_strong, p.text_2),
        BannerKind::Warn => (IconName::Warning, p.warn_line, p.warn_text),
        BannerKind::Error => (IconName::ErrorCircle, p.bad_line, p.bad_text),
        BannerKind::Good => (IconName::CheckmarkCircle, p.good_line, p.good_text),
    };
    div()
        .flex()
        .items_start()
        .gap(theme::S3)
        .p(theme::S3)
        .bg(p.surface)
        .border_1()
        .border_color(line)
        .rounded(theme::RADIUS_CARD)
        .child(div().text_color(text).pt(px(2.)).child(icon(name)))
        .child(
            div()
                .flex()
                .flex_col()
                .flex_1()
                .min_w_0()
                .gap(theme::S2)
                .when_some(title, |d, t| d.child(div().font_weight(FontWeight::SEMIBOLD).child(t)))
                .when_some(body, |d, b| d.child(b))
                .when_some(actions, |d, a| d.child(a)),
        )
        .when_some(on_dismiss, |d, (id, f)| {
            d.child(icon_button(id, IconName::Dismiss, ui_model::i18n::t("modal.close")).on_click(f))
        })
}

/// What changes when Windows' audio devices change (a cable installed by
/// its own setup, a device plugged in or out), or a profile's source or
/// microphone output does (the core gave it a cable that came: the list read
/// before does not have it, issue #86). A view keeps the last one and reads
/// the devices again when it differs (not at first).
pub fn audio_devices_key(snap: Option<&ipc::Snapshot>) -> String {
    let Some(snap) = snap else { return String::new() };
    let mut key = snap.audio_endpoints.to_string();
    for p in &snap.profiles {
        key.push('\n');
        key.push_str(&p.audio_device_id);
        key.push('|');
        key.push_str(&p.mic_device_id);
    }
    key
}
