//! Auracast (Developer screen): broadcasts the test tone or the system audio
//! to every receiver nearby that joins (stream.start_broadcast; the core
//! refuses it while developer mode is off). The broadcast is a stream like
//! any other: Stop and the status follow the core's stream, the choices
//! outlive the view (one set per core connection).

use std::time::Duration;

use gpui_kit::component::Disableable as _;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::{
    AnyElement, App, AppContext as _, AsyncApp, Context, Entity, EntityId, Global, IntoElement, ParentElement as _,
    Render, SharedString, Styled as _, Subscription, Task, WeakEntity, Window, div, px,
};
use ipc::{Method, StreamStatus};
use ui_model::i18n::t;
use ui_model::t;
use ui_model::view::auracast::{
    Channels, Choices, MAX_BIGS, PairMode, Quality, SOURCE_SETTLE_MS, Session, Source, auracast_adapter_choices,
    settled_source, show_adapter,
};

use crate::app;
use crate::core::Core;
use crate::screens::auracast_mixer::AuracastMixer;
use crate::theme::{self, ActivePalette as _};
use crate::ui::{self, ButtonKind, Choice, IconName, icon};

/// The choices and the broadcast asked for here, one set per core
/// connection: they follow the stream while no view shows them
pub struct AuracastSession {
    choices: Choices,
    s: Session,
    _follow: Subscription,
}

struct Sessions {
    core: EntityId,
    session: Entity<AuracastSession>,
}

impl Global for Sessions {}

fn stream_of(core: &Entity<Core>, cx: &App) -> StreamStatus {
    core.read(cx).snap().map(|s| s.stream.clone()).unwrap_or_default()
}

fn session(core: &Entity<Core>, cx: &mut App) -> Entity<AuracastSession> {
    if let Some(s) = cx.try_global::<Sessions>().filter(|s| s.core == core.entity_id()) {
        return s.session.clone();
    }
    let session = cx.new(|cx| {
        let follow = cx.observe(core, |this: &mut AuracastSession, core, cx| {
            let st = stream_of(&core, cx);
            let before = this.s.clone();
            this.s.follow(&st);
            if this.s != before {
                cx.notify();
            }
        });
        AuracastSession { choices: Choices::default(), s: Session::default(), _follow: follow }
    });
    cx.set_global(Sessions { core: core.entity_id(), session: session.clone() });
    session
}

pub struct Auracast {
    core: Entity<Core>,
    session: Entity<AuracastSession>,
    name: Entity<InputState>,
    code: Entity<InputState>,
    mixer: Entity<AuracastMixer>,
    settle: Option<Task<()>>,
    _subs: Vec<Subscription>,
}

impl Auracast {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Auracast {
        let core = app::core(cx);
        let session = session(&core, cx);
        let c = session.read(cx).choices.clone();
        let name = cx.new(|cx| InputState::new(window, cx).default_value(c.name.clone()));
        let code = cx.new(|cx| {
            InputState::new(window, cx).default_value(c.code.clone()).placeholder(t("ui.dev.auracast_code_none"))
        });
        let mixer = cx.new(|cx| AuracastMixer::new(window, cx));
        let subs = vec![
            cx.observe_in(&core, window, |this, _, window, cx| this.sync_mixer(window, cx)),
            cx.observe_in(&session, window, |this, _, window, cx| this.sync_mixer(window, cx)),
            cx.observe(&mixer, |_, _, cx| cx.notify()),
            cx.subscribe(&name, |this, input, ev: &InputEvent, cx| {
                if matches!(ev, InputEvent::Change) {
                    let v = input.read(cx).value().to_string();
                    this.set(cx, move |c| c.name = v);
                }
            }),
            cx.subscribe(&code, |this, input, ev: &InputEvent, cx| {
                if matches!(ev, InputEvent::Change) {
                    let v = input.read(cx).value().to_string();
                    this.set(cx, move |c| c.code = v);
                }
            }),
        ];
        let mut this = Auracast { core, session, name, code, mixer, settle: None, _subs: subs };
        this.sync_mixer(window, cx);
        this
    }

    fn stream(&self, cx: &App) -> StreamStatus {
        stream_of(&self.core, cx)
    }

    fn set(&mut self, cx: &mut Context<Self>, patch: impl FnOnce(&mut Choices)) {
        self.session.update(cx, |s, cx| {
            patch(&mut s.choices);
            cx.notify();
        });
    }

    /// The mixer shows with a matrix; it reads its state again when the rear
    /// or the broadcast's state changes
    fn sync_mixer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let st = self.stream(cx);
        let s = self.session.read(cx);
        let props = s.choices.matrix().then(|| (s.choices.rear != PairMode::Off, s.s.live(&st) && !s.s.asking));
        self.mixer.update(cx, |m, cx| m.set_props(props, window, cx));
        cx.notify();
    }

    fn start_stop(&mut self, cx: &mut Context<Self>) {
        let st = self.stream(cx);
        let session = self.session.clone();
        if session.read(cx).s.live(&st) {
            let task = app::run(app::request::<serde_json::Value>(Method::StreamStop, cx), cx);
            cx.spawn(async move |_, cx: &mut AsyncApp| {
                task.await;
                session.update(cx, |s, cx| {
                    s.s.mine = false;
                    cx.notify();
                });
            })
            .detach();
            return;
        }
        let request = session.update(cx, |s, cx| {
            s.s.asking = true;
            s.s.failed = None;
            s.s.on_air = false;
            s.s.mine = true;
            cx.notify();
            s.choices.start_request()
        });
        let task = app::run(app::request::<serde_json::Value>(request, cx), cx);
        let core = self.core.clone();
        cx.spawn(async move |_, cx: &mut AsyncApp| {
            let ok = task.await.is_some();
            session.update(cx, |s, cx| {
                if !ok {
                    s.s.mine = false;
                }
                s.s.asking = false;
                let st = stream_of(&core, cx);
                s.s.follow(&st);
                cx.notify();
            });
        })
        .detach();
    }

    /// The source: the running broadcast switches to it in place
    fn change_source(&mut self, source: Source, cx: &mut Context<Self>) {
        let before = self.session.read(cx).choices.source;
        self.set(cx, |c| c.source = source);
        let st = self.stream(cx);
        if !self.session.read(cx).s.live(&st) {
            return;
        }
        let task = app::run(app::request::<bool>(Method::AuracastSetSource { source: source.as_str().into() }, cx), cx);
        self.settle = Some(cx.spawn(async move |this: WeakEntity<Self>, cx: &mut AsyncApp| {
            let taken = task.await.unwrap_or(false);
            if !taken {
                // Not taken (a start or stop under way): the broadcast keeps its source
                let _ = this.update(cx, |this, cx| {
                    if this.session.read(cx).choices.source == source {
                        this.set(cx, |c| c.source = before);
                    }
                });
                return;
            }
            // A source that cannot be captured: the broadcast goes on with
            // the one before, which its stream info then still names
            cx.background_executor().timer(Duration::from_millis(SOURCE_SETTLE_MS)).await;
            let _ = this.update(cx, |this, cx| {
                let st = this.stream(cx);
                let s = this.session.read(cx);
                let playing = st.info.as_ref().and_then(|i| i.source.as_ref()).map(|s| s.kind.clone());
                if let Some(back) = settled_source(s.s.live(&st), s.choices.source, source, playing.as_deref()) {
                    this.set(cx, |c| c.source = back);
                }
            });
        }));
    }
}

/// A line with its label and a few choices side by side
fn choice<T: Clone + PartialEq + 'static>(
    id: &'static str,
    label: String,
    value: &T,
    disabled: bool,
    options: Vec<(T, String)>,
    on_change: impl Fn(&T, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    ui::row().child(div().child(label)).child(ui::segmented(
        id,
        value,
        options.into_iter().map(|(v, l)| Choice::new(v, l).disabled(disabled)).collect(),
        on_change,
    ))
}

/// A pair of matrix outputs: how it is sent, and its rate
#[allow(clippy::too_many_arguments)]
fn pair_row(
    id: &'static str,
    label: String,
    mode: PairMode,
    rate: Quality,
    disabled: bool,
    on_mode: impl Fn(&PairMode, &mut Window, &mut App) + 'static,
    on_rate: impl Fn(&Quality, &mut Window, &mut App) + 'static,
) -> impl IntoElement {
    let modes = [
        (PairMode::Stereo, "ui.dev.auracast_pair_stereo"),
        (PairMode::Separate, "ui.dev.auracast_pair_separate"),
        (PairMode::Subgroups, "ui.dev.auracast_pair_subgroups"),
        (PairMode::Off, "ui.dev.auracast_pair_off"),
    ];
    let rates = [(Quality::Hq, "ui.dev.auracast_48"), (Quality::Sq, "ui.dev.auracast_24")];
    ui::row().child(div().child(label)).child(
        div()
            .flex()
            .flex_wrap()
            .justify_end()
            .gap(theme::S2)
            .child(ui::segmented(
                SharedString::from(format!("{id}-mode")),
                &mode,
                modes.into_iter().map(|(v, k)| Choice::new(v, t(k)).disabled(disabled)).collect(),
                on_mode,
            ))
            .child(ui::segmented(
                SharedString::from(format!("{id}-rate")),
                &rate,
                rates
                    .into_iter()
                    .map(|(v, k)| Choice::new(v, t(k)).disabled(disabled || mode == PairMode::Off))
                    .collect(),
                on_rate,
            )),
    )
}

impl Render for Auracast {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = *cx.palette();
        let st = self.stream(cx);
        let Some(snap) = self.core.read(cx).snap().cloned() else { return div().into_any_element() };
        let (c, s) = {
            let x = self.session.read(cx);
            (x.choices.clone(), x.s.clone())
        };
        let live = s.live(&st);
        let locked = s.locked(&st);
        let (name_error, code_error) = (c.name_error(), c.code_error());
        let session = self.session.downgrade();
        let set = move |patch: Box<dyn FnOnce(&mut Choices)>, cx: &mut App| {
            let _ = session.update(cx, |s, cx| {
                patch(&mut s.choices);
                cx.notify();
            });
        };
        let set = std::rc::Rc::new(set);

        let start = ui::button(
            "auracast-start",
            if live { t("receiver.stop") } else { t("receiver.start") },
            if live { ButtonKind::Danger } else { ButtonKind::Strong },
        )
        .icon(icon(if live { IconName::Stop } else { IconName::Play }))
        .loading(s.asking)
        .disabled(c.start_disabled(live))
        .on_click(cx.listener(|this, _, _, cx| this.start_stop(cx)));

        // The adapter the broadcast goes on: offered while several adapters
        // are on WinUSB, or while one is chosen (to go back to the current one)
        let chosen = snap.settings.auracast_adapter_id.clone();
        let adapters = &snap.adapter.adapters;
        let adapter = show_adapter(adapters, &chosen).then(|| {
            let (value, options) = auracast_adapter_choices(adapters, &chosen);
            ui::field(
                t("ui.dev.auracast_adapter"),
                Some(t("ui.dev.auracast_adapter_hint").into()),
                None,
                ui::select(
                    "auracast-adapter",
                    &value,
                    options.into_iter().map(|(v, l)| Choice::new(v, l)).collect(),
                    locked,
                    |v: &String, _, cx| {
                        let v = v.clone();
                        app::save_settings(move |s| s.auracast_adapter_id = v, cx);
                    },
                ),
                cx,
            )
        });

        let status = s.status(&st).map(|(text, bad)| {
            let summary =
                live.then(|| st.info.as_ref().and_then(|i| i.le.as_ref()).map(|l| l.summary.clone())).flatten();
            div()
                .flex()
                .flex_col()
                .gap(theme::S1)
                .when(bad, |d| d.text_color(p.bad_text))
                .child(text)
                .when_some(summary.filter(|x| !x.is_empty()), |d, sum| d.child(ui::muted(sum, cx)))
        });

        let set_q = set.clone();
        let set_ch = set.clone();
        let form = ui::section(t("ui.dev.auracast"), Some(start.into_any_element()), cx)
            .children(adapter)
            .child(ui::field(
                t("ui.dev.auracast_name"),
                None,
                name_error.map(Into::into),
                Input::new(&self.name).disabled(locked).w(px(260.)),
                cx,
            ))
            .child(ui::field(
                t("ui.dev.auracast_code"),
                None,
                code_error.map(Into::into),
                Input::new(&self.code).disabled(locked).w(px(260.)),
                cx,
            ))
            .child(choice(
                "auracast-quality",
                t("ui.dev.auracast_quality"),
                &c.quality,
                locked || c.matrix(),
                vec![(Quality::Hq, t("ui.dev.auracast_hq")), (Quality::Sq, t("ui.dev.auracast_sq"))],
                move |v: &Quality, _, cx| {
                    let v = *v;
                    set_q(Box::new(move |c| c.quality = v), cx);
                },
            ))
            .child(choice(
                "auracast-channels",
                t("ui.dev.auracast_channels"),
                &c.channels,
                locked,
                vec![
                    (Channels::Stereo, t("ui.dev.auracast_stereo")),
                    (Channels::Mono, t("ui.dev.auracast_mono")),
                    (Channels::Split, t("ui.dev.auracast_split")),
                    (Channels::Matrix, t("ui.dev.auracast_matrix")),
                ],
                move |v: &Channels, _, cx| {
                    let v = *v;
                    set_ch(Box::new(move |c| c.channels = v), cx);
                },
            ))
            .child(choice(
                "auracast-source",
                t("receiver.source"),
                &c.source,
                s.source_locked(&st),
                vec![(Source::Tone, t("receiver.source_tone")), (Source::Loopback, t("receiver.source_system"))],
                {
                    let this = cx.entity().downgrade();
                    move |v: &Source, _, cx| {
                        let v = *v;
                        let _ = this.update(cx, |this, cx| this.change_source(v, cx));
                    }
                },
            ))
            .children(status);

        let mixer = c.matrix().then(|| {
            let bigs = c.bigs();
            let (set_fm, set_fr, set_rm, set_rr) = (set.clone(), set.clone(), set.clone(), set.clone());
            let children: Vec<AnyElement> = vec![
                pair_row(
                    "auracast-front",
                    t("ui.dev.auracast_front"),
                    c.front,
                    c.front_quality,
                    locked,
                    move |v, _, cx| {
                        let v = *v;
                        set_fm(Box::new(move |c| c.front = v), cx);
                    },
                    move |v, _, cx| {
                        let v = *v;
                        set_fr(Box::new(move |c| c.front_quality = v), cx);
                    },
                )
                .into_any_element(),
                pair_row(
                    "auracast-rear",
                    t("ui.dev.auracast_rear"),
                    c.rear,
                    c.rear_quality,
                    locked,
                    move |v, _, cx| {
                        let v = *v;
                        set_rm(Box::new(move |c| c.rear = v), cx);
                    },
                    move |v, _, cx| {
                        let v = *v;
                        set_rr(Box::new(move |c| c.rear_quality = v), cx);
                    },
                )
                .into_any_element(),
                ui::hint(t("ui.dev.auracast_speakers_hint"), cx).into_any_element(),
            ];
            let mut children = children;
            if bigs == 0 {
                children
                    .push(ui::hint(t("ui.dev.auracast_pairs_off_hint"), cx).text_color(p.bad_text).into_any_element());
            }
            if bigs > MAX_BIGS {
                children.push(
                    div()
                        .flex()
                        .items_center()
                        .gap(theme::S1)
                        .text_size(theme::FS_12)
                        .text_color(p.warn_text)
                        .child(t!("ui.dev.auracast_bigs_hint", bigs, MAX_BIGS))
                        .into_any_element(),
                );
            }
            self.mixer.update(cx, |m, cx| m.section(children, cx))
        });

        div()
            .flex()
            .flex_col()
            .gap(theme::S4)
            .child(ui::hint(t("ui.dev.auracast_intro"), cx))
            .child(form)
            .children(mixer.flatten())
            .into_any_element()
    }
}
