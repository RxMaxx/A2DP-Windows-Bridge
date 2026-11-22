//! Where decoded input goes: the HID engine (bt-host) hands every batch of
//! Windows inputs to one [`InputSink`], chosen by the core when the stack
//! starts:
//!
//! - [`LocalSink`]: `SendInput` in this process, with the guard's copy of
//!   the held keys (win_sys::hid_guard). The resident app and the command
//!   line.
//! - [`DryRunSink`]: counted only (the emulator test).
//! - the input helper's sink (crate input-helper): queued for a helper
//!   process that passes the input to Windows on every desktop (a headless
//!   core, later the service).
//!
//! A sink never blocks the BTstack thread for longer than a short lock.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use win_sys::hid_guard::{HeldSection, HeldSet};
use win_sys::input_inject::Inject;

/// What the HID host did, for diagnostics (counts only)
#[derive(Debug, Default)]
pub struct HidCounters {
    /// Input reports received
    pub reports: AtomicU64,
    /// Reports dropped: no usable report descriptor yet
    pub undecoded: AtomicU64,
    /// Key, button, move and wheel events decoded from the reports
    pub events: AtomicU64,
    /// Events Windows took
    pub injected: AtomicU64,
    /// Events Windows refused (a window of higher privilege has the focus,
    /// or the secure desktop is shown)
    pub blocked: AtomicU64,
    /// Events that never reached Windows: no input helper was running, or
    /// pointer moves dropped while the helper fell behind
    pub dropped: AtomicU64,
    /// Connections refused: the device is not on the list
    pub refused: AtomicU64,
    /// Keyboard light reports sent to keyboards
    pub leds: AtomicU64,
    /// Passing input to Windows is on
    pub active: AtomicBool,
}

/// Takes the Windows inputs of the HID engine
pub trait InputSink: std::fmt::Debug + Send + Sync {
    /// `injects` in order, as one batch. Counts what Windows took
    /// (`injected`), refused (`blocked`) or what never got there
    /// (`dropped`) in `counters`, now or later. Must not block.
    fn pass(&self, injects: &[Inject], counters: &Arc<HidCounters>);

    /// Windows' Num, Caps and Scroll Lock state (a `crate::leds` mask);
    /// None: not known (nothing is sent to the keyboards then)
    fn lock_keys(&self) -> Option<u8>;

    /// The stack stopped: let go of anything still held that the key-ups
    /// already passed did not cover (the input helper's sink; the others
    /// pass every key-up themselves)
    fn release_all(&self) {}
}

/// Input counted only, never passed to Windows
#[derive(Debug, Default)]
pub struct DryRunSink;

impl InputSink for DryRunSink {
    fn pass(&self, _injects: &[Inject], _counters: &Arc<HidCounters>) {}

    fn lock_keys(&self) -> Option<u8> {
        None
    }
}

/// `SendInput` in this process; the keys held are written to the guard's
/// section ([`HeldSection`]) when one is given
#[derive(Debug, Default)]
pub struct LocalSink {
    held: Option<Arc<HeldSection>>,
    held_set: Mutex<HeldSet>,
}

impl LocalSink {
    pub fn new(held: Option<Arc<HeldSection>>) -> LocalSink {
        LocalSink { held, held_set: Mutex::default() }
    }

    /// Follows what Windows took; the guard's copy is written when it changed
    fn track_held(&self, injected: &[Inject]) {
        let Some(section) = &self.held else { return };
        let mut set = self.held_set.lock().unwrap_or_else(|e| e.into_inner());
        if set.apply(injected) {
            section.store(&set);
        }
    }
}

impl InputSink for LocalSink {
    fn pass(&self, injects: &[Inject], c: &Arc<HidCounters>) {
        let taken = match win_sys::input_inject::send(injects) {
            Ok(n) => n,
            Err(e) => {
                // Not per event: a held key repeats this many times a second
                if c.blocked.load(Ordering::Relaxed) == 0 {
                    tracing::warn!("hid: Windows refused input ({e}); further refusals are only counted");
                }
                let win_sys::input_inject::InjectError::Partial { inserted, .. } = e;
                inserted
            }
        };
        // The first `taken` went in, in order
        self.track_held(&injects[..taken.min(injects.len())]);
        c.injected.fetch_add(taken as u64, Ordering::Relaxed);
        c.blocked.fetch_add(injects.len().saturating_sub(taken) as u64, Ordering::Relaxed);
    }

    fn lock_keys(&self) -> Option<u8> {
        Some(win_sys::input_inject::lock_keys())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_run_counts_nothing_and_knows_no_lights() {
        let c = Arc::new(HidCounters::default());
        DryRunSink.pass(&[Inject::Key { scan: 0x1E, extended: false, up: false }], &c);
        assert_eq!(c.injected.load(Ordering::Relaxed) + c.blocked.load(Ordering::Relaxed), 0);
        assert_eq!(DryRunSink.lock_keys(), None);
        // An empty batch through the local sink never reaches SendInput
        LocalSink::new(None).pass(&[], &c);
        assert_eq!(c.injected.load(Ordering::Relaxed), 0);
    }
}
