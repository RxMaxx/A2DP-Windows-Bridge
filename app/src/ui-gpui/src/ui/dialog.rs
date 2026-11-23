//! A modal question. Escape and the backdrop cancel; Tab stays inside.
//!
//! Ported from app/src/ui/src/components/controls.tsx (Dialog).

use std::rc::Rc;

use gpui_kit::component::WindowExt as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::dialog::DialogFooter;
use gpui_kit::{AnyElement, App, IntoElement, ParentElement as _, SharedString, Styled as _, Window, div};
use ui_model::i18n::t;

use super::{ButtonKind, IconName, button, icon};
use crate::theme;

/// What a question's dialog shows around its body
#[derive(Clone)]
pub struct Question {
    pub title: SharedString,
    /// The confirming button's text; None: only the closing button
    pub confirm: Option<SharedString>,
    pub confirm_icon: Option<IconName>,
    /// The other button's text (Cancel, or Close without a confirming button)
    pub cancel: SharedString,
    /// The confirming button destroys something
    pub danger: bool,
    pub width: f32,
}

impl Question {
    pub fn new(title: impl Into<SharedString>) -> Question {
        Question {
            title: title.into(),
            confirm: None,
            confirm_icon: None,
            cancel: t("modal.cancel").into(),
            danger: false,
            width: 480.,
        }
    }

    pub fn confirm(mut self, label: impl Into<SharedString>) -> Self {
        self.confirm = Some(label.into());
        self
    }

    pub fn confirm_icon(mut self, icon: IconName) -> Self {
        self.confirm_icon = Some(icon);
        self
    }

    pub fn cancel(mut self, label: impl Into<SharedString>) -> Self {
        self.cancel = label.into();
        self
    }

    pub fn danger(mut self) -> Self {
        self.danger = true;
        self
    }

    pub fn width(mut self, width: f32) -> Self {
        self.width = width;
        self
    }
}

/// Opens the dialog. `body` draws what is between the title and the
/// buttons; it runs again whenever the dialog is drawn, so it reads the
/// state it shows (make entities such as text inputs before, not in it).
/// `on_confirm` runs when the confirming button is chosen; the dialog then
/// closes. Close it from elsewhere with `window.close_dialog(cx)`.
pub fn ask(
    question: Question,
    body: impl Fn(&mut Window, &mut App) -> AnyElement + 'static,
    on_confirm: impl Fn(&mut Window, &mut App) + 'static,
    window: &mut Window,
    cx: &mut App,
) {
    let on_confirm = Rc::new(on_confirm);
    window.open_dialog(cx, move |dialog, window, cx| {
        let q = question.clone();
        let mut footer = DialogFooter::new()
            .child(Button::new("dialog-cancel").label(q.cancel).on_click(|_, window, cx| window.close_dialog(cx)));
        if let Some(label) = q.confirm {
            let on_confirm = on_confirm.clone();
            let kind = if q.danger { ButtonKind::Danger } else { ButtonKind::Strong };
            let mut confirm = button("dialog-confirm", label, kind).on_click(move |_, window, cx| {
                window.close_dialog(cx);
                on_confirm(window, cx);
            });
            if let Some(i) = q.confirm_icon {
                confirm = confirm.icon(icon(i));
            }
            footer = footer.child(confirm);
        }
        dialog
            .width(gpui_kit::px(q.width))
            .title(q.title)
            .child(div().flex().flex_col().gap(theme::S3).child(body(window, cx)))
            .footer(footer.into_any_element())
    });
}
