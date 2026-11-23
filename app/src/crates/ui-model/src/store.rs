//! The core's state as the UI sees it: the last Snapshot with each Event
//! applied (each event replaces one part), plus the streams the UI
//! subscribes to (link samples, log lines, the receiver test's state). The
//! UI keeps no state of its own beyond this and view state.
//!
//! Revisions: every state event carries the next revision; link samples
//! and log lines are not state and carry none of their own. A gap means a
//! lost event, so the snapshot is fetched again (state.snapshot). Its answer
//! travels apart from the events and says nothing about which events it
//! already contains, so the state events that arrive while it is on its way
//! are kept and applied again on top of it, in order: each replaces a whole
//! part, so the newest one of each part ends up in place.
//!
//! Ported from app/src/ui/src/ipc/store.ts (the part without the transport).

use std::collections::{HashSet, VecDeque};

use ipc::{Event, LinkSample, LogLine, ReceiverState, ServerMessage, Snapshot};

/// Link samples kept: one minute at 4 per second
pub const LINK_KEEP: usize = 240;
/// Log lines kept in the window (the core keeps its own files)
pub const LOG_KEEP: usize = 5000;

/// What a message changed, so that only the views showing it draw again
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Change {
    Nothing,
    /// The snapshot or a part of it
    State,
    Link,
    Log,
    Receiver,
}

/// What [`Store::on_message`] did
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Applied {
    pub change: Change,
    /// An event was lost: ask for the snapshot ([`Store::begin_resync`])
    pub resync: bool,
}

/// A state.snapshot on its way: the revision when asked, and the state events since
struct Pending {
    token: u64,
    from: Option<u64>,
    events: Vec<(u64, Event)>,
}

#[derive(Default)]
pub struct Store {
    /// Revision of the snapshot plus the events applied; None before the first
    pub rev: Option<u64>,
    pub snap: Option<Snapshot>,
    link: VecDeque<LinkSample>,
    logs: VecDeque<LogLine>,
    /// Lines up to this time were cleared: the kept lines do not bring them back
    logs_cleared_at: Option<u64>,
    pub receiver: Option<ReceiverState>,
    pending: Option<Pending>,
    next_token: u64,
}

impl Store {
    pub fn new() -> Store {
        Store::default()
    }

    /// The last [`LINK_KEEP`] samples
    pub fn link(&self) -> &VecDeque<LinkSample> {
        &self.link
    }

    /// The last [`LOG_KEEP`] lines
    pub fn logs(&self) -> &VecDeque<LogLine> {
        &self.logs
    }

    pub fn clear_link(&mut self) {
        self.link.clear();
    }

    pub fn clear_logs(&mut self) {
        if let Some(last) = self.logs.back() {
            self.logs_cleared_at = Some(last.time_ms);
        }
        self.logs.clear();
    }

    /// A snapshot is to be asked for -> its token for [`Store::finish_resync`];
    /// None while one is on its way already
    pub fn begin_resync(&mut self) -> Option<u64> {
        if self.pending.is_some() {
            return None;
        }
        // A window that opens late has no revision yet: the next event after
        // the answer then sets it, without a gap check
        self.next_token += 1;
        self.pending = Some(Pending { token: self.next_token, from: self.rev, events: Vec::new() });
        Some(self.next_token)
    }

    /// The answer to the state.snapshot of `token` (None: it failed; not
    /// connected, the next connection brings a snapshot anyway) -> whether
    /// the state changed
    pub fn finish_resync(&mut self, token: u64, snap: Option<Snapshot>) -> bool {
        // A snapshot message of the core came first and replaced this
        if self.pending.as_ref().is_none_or(|p| p.token != token) {
            return false;
        }
        let Some(p) = self.pending.take() else { return false };
        let Some(snap) = snap else { return false };
        self.snap = Some(snap);
        self.rev = p.from;
        for (rev, ev) in p.events {
            if p.from.is_none_or(|from| rev > from) {
                self.apply_state(ev);
                if rev > self.rev.unwrap_or(0) {
                    self.rev = Some(rev);
                }
            }
        }
        true
    }

    pub fn on_message(&mut self, m: ServerMessage) -> Applied {
        match m {
            // The core's own snapshot (after Hello, or when this client fell
            // behind) has its true revision: a requested one is not needed
            ServerMessage::Snapshot { rev, state } => {
                self.pending = None;
                self.snap = Some(*state);
                self.rev = Some(rev);
                Applied { change: Change::State, resync: false }
            }
            ServerMessage::Event { rev, event } => self.apply_event(rev, *event),
            // welcome / reject / response: the connection handles them
            _ => Applied { change: Change::Nothing, resync: false },
        }
    }

    fn apply_event(&mut self, rev: u64, ev: Event) -> Applied {
        let only = |change| Applied { change, resync: false };
        match ev {
            Event::Link(sample) => {
                self.link.push_back(sample);
                while self.link.len() > LINK_KEEP {
                    self.link.pop_front();
                }
                return only(Change::Link);
            }
            Event::Log(line) => {
                self.logs.push_back(line);
                while self.logs.len() > LOG_KEEP {
                    self.logs.pop_front();
                }
                return only(Change::Log);
            }
            Event::Receiver(state) => {
                self.receiver = Some(*state);
                return only(Change::Receiver);
            }
            Event::Bye => {
                self.snap = None;
                return only(Change::State);
            }
            // The window closes (the shell ends this process); not state.
            // Headphone buttons and serial data are for other clients: not
            // state, no revision
            Event::UiClose | Event::MediaButton(_) | Event::RfcommData(_) | Event::RfcommClosed(_) => {
                return only(Change::Nothing);
            }
            _ => {}
        }
        let mut resync = false;
        if let Some(cur) = self.rev {
            if rev <= cur {
                return only(Change::Nothing); // already in the snapshot
            }
            resync = rev > cur + 1; // lost one: the snapshot fixes it
        }
        if let Some(p) = &mut self.pending {
            p.events.push((rev, ev.clone()));
        }
        self.rev = Some(rev);
        let changed = self.apply_state(ev);
        Applied { change: if changed { Change::State } else { Change::Nothing }, resync }
    }

    fn apply_state(&mut self, ev: Event) -> bool {
        // Before the first snapshot there is nothing to patch: it comes next
        let Some(snap) = &mut self.snap else { return false };
        match ev {
            Event::Adapter(v) => snap.adapter = v,
            Event::Devices(v) => snap.devices = v,
            Event::Scanning(v) => snap.scanning = v,
            Event::LeScanning(v) => snap.le_scanning = v,
            Event::Stream(v) => snap.stream = v,
            Event::Profiles { profiles, active_profile } => {
                snap.profiles = profiles;
                snap.active_profile = active_profile;
            }
            Event::Settings(v) => snap.settings = v,
            Event::Update(v) => snap.update = v,
            Event::Capture(v) => snap.capture = v,
            Event::V1Running(v) => snap.v1_running = v,
            Event::Verbose(v) => snap.verbose = v,
            Event::Crashes(v) => snap.crashes = v,
            Event::AudioEndpoints(v) => snap.audio_endpoints = v,
            Event::HidDevices(v) => snap.hid_devices = v,
            Event::HidCandidates(v) => snap.hid_candidates = v,
            Event::HidPasskey(v) => snap.hid_passkey = v,
            Event::HidCheck { checks, rolled_back } => {
                snap.hid_checks = checks;
                snap.hid_rolled_back = rolled_back;
            }
            Event::HidHealth(v) => snap.hid_health = v,
            _ => return false,
        }
        true
    }

    /// Puts the core's kept lines before the live ones (the live lines that
    /// came in meanwhile may be among them)
    pub fn merge_recent(&mut self, recent: Vec<LogLine>) {
        let key = |l: &LogLine| (l.time_ms, l.target.clone(), l.message.clone());
        let kept: Vec<LogLine> =
            recent.into_iter().filter(|l| self.logs_cleared_at.is_none_or(|at| l.time_ms > at)).collect();
        let last = kept.last().map(|l| l.time_ms);
        let seen: HashSet<_> = kept.iter().filter(|l| Some(l.time_ms) == last).map(key).collect();
        let live: Vec<LogLine> = self
            .logs
            .drain(..)
            .filter(|l| match last {
                None => true,
                Some(last) => l.time_ms > last || (l.time_ms == last && !seen.contains(&key(l))),
            })
            .collect();
        self.logs.extend(kept);
        self.logs.extend(live);
        while self.logs.len() > LOG_KEEP {
            self.logs.pop_front();
        }
    }
}
