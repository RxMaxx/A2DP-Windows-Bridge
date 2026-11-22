//! Keyboards and mice through BlueCaster (experimental, developer mode;
//! issue #413).
//!
//! Dangerous: the input exists only while this process runs and its stack
//! is up. If the only keyboard and mouse of the PC are connected this way
//! and BlueCaster stops, crashes or is not started yet (sign-in screen),
//! the user has no input at all. So turning it on takes more than a
//! setting: developer mode, a code the core hands out and the user types
//! back, and two statements the user confirms ([`Service::hid_confirm`]).
//! A settings file or an IPC client alone cannot switch it on by accident.
//!
//! While it is on the stack is kept running, also without a stream
//! ([`Inner::hid_keep`]): a keyboard has to work before any headphones
//! connect.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, Weak};
use std::time::{Duration, Instant};

use crate::messages::Msg;
use crate::service::{Inner, Service, ServiceEvent};

/// How long a confirmation code stays valid
const CODE_VALID: Duration = Duration::from_secs(120);
/// How often a stack that stopped is started again while the feature is on
const KEEP_EVERY: Duration = Duration::from_secs(5);
/// How long adding an LE device waits for the stack's LE to be ready
const LE_READY_WAIT: Duration = Duration::from_secs(3);
/// At most this many keyboards and mice
pub const MAX_DEVICES: usize = 8;

/// A keyboard or mouse the user added
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HidDevice {
    pub address: String,
    pub name: String,
    pub connected: bool,
    /// Connected over Bluetooth LE (false: classic Bluetooth)
    pub le: bool,
}

/// What the HID host did since the stack started (counts only: nothing of
/// what was typed is kept anywhere)
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct HidCounts {
    pub reports: u64,
    pub undecoded: u64,
    pub events: u64,
    pub injected: u64,
    pub blocked: u64,
    pub refused: u64,
}

#[derive(Default)]
pub(crate) struct HidState {
    /// The code handed out last, and when
    code: Mutex<Option<(String, Instant)>>,
    connected: Mutex<Vec<[u8; 6]>>,
    /// The thread of [`Inner::hid_keep`] runs
    keeping: AtomicBool,
    /// The adapter was released on purpose (its driver is being changed,
    /// the app quits): no stack is started behind the user's back until
    /// something else starts one
    hold: AtomicBool,
    /// The command line's --hid-device / --hid-dry-run, instead of the
    /// setting and the saved list
    cli: Mutex<Option<bt_host::HidConfig>>,
    /// The number the user types on the device being added, while its
    /// pairing shows one (never logged)
    passkey: Mutex<Option<([u8; 6], bt_host::Passkey)>>,
    /// The executable that runs the guard ([`Service::enable_hid_guard`]);
    /// None: no guard is started
    guard_exe: Mutex<Option<PathBuf>>,
    /// The guard running while input goes to Windows
    guard: Mutex<Option<HidGuard>>,
    /// What runs the input helper ([`Service::enable_input_helper`]);
    /// None: input goes to Windows from this process (and the guard)
    helper_config: Mutex<Option<input_helper::HelperConfig>>,
    /// The input helper's sink while input goes through it
    helper: Mutex<Option<Arc<input_helper::HelperSink>>>,
}

/// The guard process (win_sys::hid_guard) and the held keys it reads: it
/// releases them in Windows if this process dies with keys held
struct HidGuard {
    section: Arc<win_sys::hid_guard::HeldSection>,
    process: win_sys::hid_guard::Guard,
}

impl HidState {
    /// The held keys' section for a stack that passes input to Windows: the
    /// running guard's, or a new guard's. None: no guard (none configured,
    /// or it could not be started).
    fn guard_section(&self) -> Option<Arc<win_sys::hid_guard::HeldSection>> {
        let exe = self.guard_exe.lock().unwrap_or_else(|e| e.into_inner()).clone()?;
        let mut guard = self.guard.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(g) = guard.as_mut()
            && g.process.running()
        {
            return Some(g.section.clone());
        }
        // A guard that ended by itself is replaced (with a new section: the
        // old one may still list keys of before)
        *guard = None;
        let started = win_sys::hid_guard::HeldSection::create()
            .and_then(|s| Ok((win_sys::hid_guard::Guard::spawn(&exe, None, &s, None)?, s)));
        match started {
            Ok((process, section)) => {
                tracing::info!("keyboards and mice: guard started (releases held keys if BlueCaster stops)");
                let section = Arc::new(section);
                *guard = Some(HidGuard { section: section.clone(), process });
                Some(section)
            }
            Err(e) => {
                tracing::warn!("keyboards and mice: the guard did not start ({})", e.kind());
                None
            }
        }
    }

    /// The input helper's sink, started (one for the life of the feature:
    /// it restarts its helper by itself). None: no helper configured.
    fn helper_sink(&self) -> Option<Arc<input_helper::HelperSink>> {
        let config = self.helper_config.lock().unwrap_or_else(|e| e.into_inner()).clone()?;
        let mut helper = self.helper.lock().unwrap_or_else(|e| e.into_inner());
        let sink = helper.get_or_insert_with(|| {
            tracing::info!("keyboards and mice: input goes through the input helper");
            let sink = input_helper::HelperSink::new(config, Arc::new(input_helper::SessionLauncher));
            sink.on_sas(Box::new(secure_attention));
            sink
        });
        sink.start();
        Some(sink.clone())
    }

    /// The input helper releases everything and exits
    fn stop_helper(&self) {
        let sink = self.helper.lock().unwrap_or_else(|e| e.into_inner()).take();
        if let Some(sink) = sink {
            sink.stop();
            tracing::info!("keyboards and mice: input helper stopped");
        }
    }

    /// Input no longer goes to Windows: the guard exits without releasing
    /// anything
    fn stop_guard(&self) {
        if self.guard.lock().unwrap_or_else(|e| e.into_inner()).take().is_some() {
            tracing::info!("keyboards and mice: guard stopped");
        }
    }
}

/// Ctrl+Alt+Del on a keyboard of ours (the input helper saw it). Windows
/// takes the Secure Attention Sequence only from `SendSAS`, only from a
/// service, and only when the SoftwareSASGeneration policy admits services
/// (1 or 3): then it is generated here. Otherwise nothing happens (logged
/// once). This never changes the policy.
fn secure_attention() {
    static REFUSED_LOGGED: AtomicBool = AtomicBool::new(false);
    let service = crate::paths::service_mode() && win_sys::session_launch::in_service_session();
    let policy = win_sys::input_desktop::software_sas_policy();
    if service && matches!(policy, Some(1 | 3)) {
        match win_sys::input_desktop::send_sas_as_service() {
            Ok(()) => tracing::info!("keyboards and mice: Ctrl+Alt+Del passed to Windows"),
            Err(e) => tracing::warn!("keyboards and mice: Ctrl+Alt+Del was not passed on ({})", e.kind()),
        }
    } else if !REFUSED_LOGGED.swap(true, Ordering::Relaxed) {
        tracing::info!(
            "keyboards and mice: Ctrl+Alt+Del is not passed on ({})",
            if service { "the Windows policy does not allow services to" } else { "only the service can" }
        );
    }
}

/// How long adding a keyboard or mouse may show a number to type: from
/// hid.add until the pairing ends, at most this long
const PAIRING_WINDOW: Duration = Duration::from_secs(90);

impl HidState {
    /// A stack started: keeping it up is allowed again
    pub fn stack_started(&self) {
        self.hold.store(false, Ordering::Release);
    }

    pub fn hold(&self) {
        self.hold.store(true, Ordering::Release);
    }
}

/// hid_devices.txt: one "address|name" per line, as devices.txt, and
/// "address|name|le:public" or "address|name|le:random" for an LE device
/// (with its address type). Lines of earlier versions (two fields) are
/// classic devices.
pub fn devices_file(config: &Path) -> PathBuf {
    crate::paths::devices_dir(config).join("hid_devices.txt")
}

/// The report descriptors kept per device (bt_host::hid::descriptor_file)
pub fn descriptors_dir(config: &Path) -> PathBuf {
    crate::paths::devices_dir(config).join("hid_descriptors")
}

/// A keyboard or mouse of the list
#[derive(Debug, Clone, PartialEq, Eq)]
struct Entry {
    /// Upper case
    address: String,
    name: String,
    /// LE: its address type (0 public, 1 random); None: classic
    le: Option<u8>,
}

impl Entry {
    fn addr(&self) -> Option<[u8; 6]> {
        bt_host::parse_addr(&self.address)
    }

    fn le_addr(&self) -> Option<bt_host::LeAddr> {
        Some(bt_host::LeAddr { addr: self.addr()?, addr_type: self.le? })
    }
}

fn parse_line(line: &str) -> Option<Entry> {
    let mut fields = line.trim_end_matches(['\r', '\n']).splitn(3, '|');
    let address = fields.next()?;
    let name = fields.next()?;
    bt_host::parse_addr(address)?;
    let le = match fields.next().map(str::trim) {
        None | Some("") => None,
        Some("le:public") => Some(bt_host::LeAddr::PUBLIC),
        Some("le:random") => Some(bt_host::LeAddr::RANDOM),
        // A kind this version does not know: left out
        Some(_) => return None,
    };
    Some(Entry { address: address.to_uppercase(), name: name.to_string(), le })
}

fn load(config: &Path) -> Vec<Entry> {
    let Ok(text) = std::fs::read_to_string(devices_file(config)) else { return Vec::new() };
    text.lines().filter_map(parse_line).take(MAX_DEVICES).collect()
}

fn save(config: &Path, list: &[Entry]) -> std::io::Result<()> {
    let file = devices_file(config);
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let text: String = list
        .iter()
        .map(|e| {
            let name = e.name.replace(['|', '\r', '\n'], " ");
            match e.le {
                None => format!("{}|{name}\n", e.address),
                Some(t) => format!(
                    "{}|{name}|le:{}\n",
                    e.address,
                    if t == bt_host::LeAddr::PUBLIC { "public" } else { "random" }
                ),
            }
        })
        .collect();
    std::fs::write(file, text)
}

/// The LE entry `from` (upper case) becomes `to` with address type
/// `addr_type`, keeping its name; an entry of `to` already there stays and
/// `from` goes. Whether the list changed.
fn rename_entry(list: &mut Vec<Entry>, from: &str, to: &str, addr_type: u8) -> bool {
    let Some(i) = list.iter().position(|e| e.le.is_some() && e.address == from) else { return false };
    if list.iter().any(|e| e.le.is_some() && e.address == to) {
        list.remove(i);
    } else {
        list[i].address = to.to_string();
        list[i].le = Some(addr_type);
    }
    true
}

/// The classic and the LE addresses of a list
fn addresses(list: &[Entry]) -> (Vec<[u8; 6]>, Vec<bt_host::LeAddr>) {
    let classic = list.iter().filter(|e| e.le.is_none()).filter_map(Entry::addr).collect();
    let le = list.iter().filter_map(Entry::le_addr).collect();
    (classic, le)
}

/// The command line's devices as list entries (no names)
fn cli_entries(cli: &bt_host::HidConfig) -> Vec<Entry> {
    let classic = cli.devices.iter().map(|a| Entry { address: bt_host::addr_str(a), name: String::new(), le: None });
    let le = cli.le_devices.iter().map(|a| Entry {
        address: bt_host::addr_str(&a.addr),
        name: String::new(),
        le: Some(a.addr_type),
    });
    classic.chain(le).collect()
}

/// Six digits nobody can know before asking for them
fn new_code() -> String {
    use std::hash::{BuildHasher, Hasher};
    let mut h = std::collections::hash_map::RandomState::new().build_hasher();
    h.write_u128(std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_nanos()));
    format!("{:06}", h.finish() % 1_000_000)
}

impl Inner {
    /// What the next stack runs HID with: None, no HID at all
    pub(crate) fn hid_config(&self) -> Option<bt_host::HidConfig> {
        let mut config = self.hid_config_base();
        let helper = self.hid.helper_config.lock().unwrap_or_else(|e| e.into_inner()).clone();
        match (config.as_mut(), helper) {
            // Input to Windows through the helper, which releases what it
            // holds itself (no guard); in dry run only a recording helper
            // (a test switch of debug builds), which never injects
            (Some(c), Some(h)) if !c.dry_run || h.record.is_some() => {
                self.hid.stop_guard();
                c.sink = self.hid.helper_sink().map(|s| s as Arc<dyn hid_input::sink::InputSink>);
            }
            // Input to Windows from this process: the guard watches over
            // the keys held
            (Some(c), _) if !c.dry_run => {
                self.hid.stop_helper();
                c.held = self.hid.guard_section();
            }
            _ => {
                self.hid.stop_guard();
                self.hid.stop_helper();
            }
        }
        config
    }

    fn hid_config_base(&self) -> Option<bt_host::HidConfig> {
        let descriptor_dir = Some(descriptors_dir(self.config_dir()));
        if let Some(cli) = self.hid.cli.lock().unwrap_or_else(|e| e.into_inner()).clone() {
            return Some(bt_host::HidConfig { descriptor_dir, ..cli });
        }
        let on = self.settings.lock().unwrap_or_else(|e| e.into_inner()).hid_host();
        on.then(|| {
            let (devices, le_devices) = addresses(&load(self.config_dir()));
            bt_host::HidConfig { devices, le_devices, dry_run: false, descriptor_dir, ..Default::default() }
        })
    }

    /// The devices that may connect: the saved list, and the command line's
    /// (--hid-device, --hid-le-device) when it gave any
    fn hid_list(&self) -> Vec<Entry> {
        let mut list = load(self.config_dir());
        if let Some(cli) = self.hid.cli.lock().unwrap_or_else(|e| e.into_inner()).as_ref() {
            for e in cli_entries(cli) {
                if !list.iter().any(|l| l.address == e.address) {
                    list.push(e);
                }
            }
        }
        list
    }

    pub(crate) fn hid_wanted(&self) -> bool {
        self.hid.cli.lock().unwrap_or_else(|e| e.into_inner()).is_some()
            || self.settings.lock().unwrap_or_else(|e| e.into_inner()).hid_host()
    }

    pub(crate) fn on_hid_event(&self, e: bt_host::HidEvent) {
        // The number to type: to the UI only, never to the log
        match e {
            bt_host::HidEvent::Passkey(a, k) => {
                *self.hid.passkey.lock().unwrap_or_else(|e| e.into_inner()) = Some((a, k));
                tracing::info!("keyboards and mice: showing the number to type on the new device");
                self.send_event(ServiceEvent::HidPasskey);
                return;
            }
            bt_host::HidEvent::PasskeyDone(a) => {
                self.hid_passkey_clear(Some(a));
                return;
            }
            bt_host::HidEvent::Identity { listed, identity } => {
                self.hid_identity(listed, identity);
                return;
            }
            // Its pairing is over either way
            bt_host::HidEvent::Connected(a) | bt_host::HidEvent::ConnectFailed(a, _) => {
                self.hid_passkey_clear(Some(a));
                // Not waited for: the stack ends the window itself too, and
                // so does the timeout
                if let Ok(Some(host)) = self.host.try_lock().map(|h| h.clone()) {
                    host.hid_pairing_end(a, None);
                }
            }
            bt_host::HidEvent::Disconnected(_) => {}
        }
        {
            let mut c = self.hid.connected.lock().unwrap_or_else(|e| e.into_inner());
            match e {
                bt_host::HidEvent::Connected(a) => {
                    if !c.contains(&a) {
                        c.push(a);
                    }
                }
                bt_host::HidEvent::Disconnected(a) | bt_host::HidEvent::ConnectFailed(a, _) => c.retain(|&x| x != a),
                bt_host::HidEvent::Passkey(..)
                | bt_host::HidEvent::PasskeyDone(_)
                | bt_host::HidEvent::Identity { .. } => {}
            }
            tracing::info!("keyboards and mice: {} connected", c.len());
        }
        self.send_event(ServiceEvent::HidDevices);
    }

    /// An LE device on the list with a private address told its identity
    /// address: the saved list (and the command line's) has the identity
    /// from now on, so the next stack finds the device too
    fn hid_identity(&self, listed: bt_host::LeAddr, identity: bt_host::LeAddr) {
        let from = bt_host::addr_str(&listed.addr).to_uppercase();
        let to = bt_host::addr_str(&identity.addr).to_uppercase();
        let mut list = load(self.config_dir());
        if rename_entry(&mut list, &from, &to, identity.addr_type) {
            match save(self.config_dir(), &list) {
                Ok(()) => tracing::info!("keyboards and mice: an LE device is saved with its identity address"),
                Err(e) => tracing::warn!("keyboards and mice: the list was not saved ({e})"),
            }
        }
        if let Some(cli) = self.hid.cli.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            let known = cli.le_devices.iter().any(|a| a.addr == identity.addr);
            cli.le_devices.retain(|a| a.addr != listed.addr || !known);
            for a in cli.le_devices.iter_mut().filter(|a| a.addr == listed.addr) {
                *a = identity;
            }
        }
        for a in self.hid.connected.lock().unwrap_or_else(|e| e.into_inner()).iter_mut() {
            if *a == listed.addr {
                *a = identity.addr;
            }
        }
        self.send_event(ServiceEvent::HidDevices);
    }

    /// The number shown goes away (`addr`: only that device's)
    fn hid_passkey_clear(&self, addr: Option<[u8; 6]>) {
        let mut p = self.hid.passkey.lock().unwrap_or_else(|e| e.into_inner());
        if p.as_ref().is_some_and(|(a, _)| addr.is_none_or(|x| x == *a)) {
            *p = None;
            drop(p);
            tracing::debug!("keyboards and mice: the number to type is no longer shown");
            self.send_event(ServiceEvent::HidPasskey);
        }
    }

    /// A keyboard or mouse is connected: a new stack would cut the user's
    /// input for as long as the reconnection takes
    pub(crate) fn hid_links_up(&self) -> bool {
        !self.hid.connected.lock().unwrap_or_else(|e| e.into_inner()).is_empty()
    }

    /// The stack is gone, and every keyboard and mouse with it
    pub(crate) fn hid_stack_stopped(&self) {
        self.hid_passkey_clear(None);
        let had = !std::mem::take(&mut *self.hid.connected.lock().unwrap_or_else(|e| e.into_inner())).is_empty();
        if had {
            tracing::warn!("keyboards and mice: disconnected, Bluetooth stopped");
            self.send_event(ServiceEvent::HidDevices);
        }
    }

    /// Keeps a stack running while the feature is on: started now if none
    /// runs, and again whenever it stopped (a restart for a new stream
    /// starts its own). One thread; it ends when the feature is off.
    pub(crate) fn hid_keep(self: &Arc<Self>) {
        if !self.hid_wanted() || self.hid.keeping.swap(true, Ordering::AcqRel) {
            return;
        }
        let weak: Weak<Inner> = Arc::downgrade(self);
        let spawned = std::thread::Builder::new().name("hid-keep".into()).spawn(move || {
            let mut failed = false;
            loop {
                let Some(inner) = weak.upgrade() else { return };
                if !inner.hid_wanted() {
                    inner.hid.keeping.store(false, Ordering::Release);
                    tracing::debug!("keyboards and mice: off, the stack is no longer kept running");
                    return;
                }
                if inner.hid_may_start() && !inner.hid.hold.load(Ordering::Acquire) {
                    match inner.ensure_host() {
                        Ok(_) => {
                            failed = false;
                            tracing::info!("keyboards and mice: Bluetooth started for them");
                        }
                        // Once: without an adapter this repeats every few seconds
                        Err(e) if !failed => {
                            failed = true;
                            tracing::warn!("keyboards and mice: Bluetooth did not start ({})", e.key);
                        }
                        Err(_) => {}
                    }
                }
                drop(inner);
                std::thread::sleep(KEEP_EVERY);
            }
        });
        if spawned.is_err() {
            self.hid.keeping.store(false, Ordering::Release);
        }
    }
}

impl Service {
    /// The command line's keyboards and mice, classic and LE, instead of
    /// the setting and the saved list (before the first stack starts)
    pub fn set_hid_cli(&self, devices: Vec<[u8; 6]>, le_devices: Vec<bt_host::LeAddr>, dry_run: bool) {
        *self.inner.hid.cli.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(bt_host::HidConfig { devices, le_devices, dry_run, ..Default::default() });
    }

    /// The command line's keyboard lights (--hid-leds), used instead of
    /// Windows' lock keys in dry run (after [`Service::set_hid_cli`])
    pub fn set_hid_cli_leds(&self, leds: Option<u8>) {
        if let Some(cli) = self.inner.hid.cli.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            cli.leds = leds;
        }
    }

    /// While input goes to Windows, `exe` runs as the guard that releases
    /// held keys and buttons if this process dies (`exe --hid-guard ...`,
    /// win_sys::hid_guard). Without this call no guard is started.
    pub fn enable_hid_guard(&self, exe: PathBuf) {
        *self.inner.hid.guard_exe.lock().unwrap_or_else(|e| e.into_inner()) = Some(exe);
    }

    /// Input goes through the input helper (`exe --input-helper ...`,
    /// crate input-helper) instead of this process's `SendInput` and the
    /// guard: in a service as SYSTEM in the console session, else a plain
    /// child. `record`: debug builds only, an inheritable pipe handle the
    /// helper writes what it would inject to (it then never injects, and
    /// is used in dry run too).
    pub fn enable_input_helper(&self, exe: PathBuf, record: Option<usize>) {
        let record = record.filter(|_| input_helper::helper::RECORD_AVAILABLE);
        *self.inner.hid.helper_config.lock().unwrap_or_else(|e| e.into_inner()) =
            Some(input_helper::HelperConfig { exe, record });
    }

    /// Input helpers started so far, and whether one takes input now
    pub fn input_helper_state(&self) -> Option<(u64, bool)> {
        self.inner.hid.helper.lock().unwrap_or_else(|e| e.into_inner()).as_ref().map(|s| (s.started(), s.connected()))
    }

    /// A new code the user has to type to turn the feature on; the one
    /// before it no longer counts
    pub fn hid_code(&self) -> String {
        let code = new_code();
        *self.inner.hid.code.lock().unwrap_or_else(|e| e.into_inner()) = Some((code.clone(), Instant::now()));
        code
    }

    /// Whether the feature may be turned on now: developer mode is on, the
    /// user stated both that another keyboard and mouse are at hand and
    /// that the risk is understood, and typed the code handed out last.
    /// A code counts once.
    pub fn hid_confirm(&self, code: &str, other_input_at_hand: bool, risk_understood: bool) -> Result<(), Msg> {
        if !self.settings().debug_mode {
            return Err(Msg::new("error.hid.developer_only"));
        }
        if !other_input_at_hand || !risk_understood {
            return Err(Msg::new("error.hid.not_confirmed"));
        }
        let handed = self.inner.hid.code.lock().unwrap_or_else(|e| e.into_inner()).take();
        match handed {
            Some((c, at)) if at.elapsed() <= CODE_VALID && c == code.trim() => {
                tracing::warn!("keyboards and mice through BlueCaster: confirmed by the user, turning on");
                Ok(())
            }
            _ => Err(Msg::new("error.hid.wrong_code")),
        }
    }

    /// The keyboards and mice the user added
    pub fn hid_devices(&self) -> Vec<HidDevice> {
        let connected = self.inner.hid.connected.lock().unwrap_or_else(|e| e.into_inner()).clone();
        load(self.config_dir())
            .into_iter()
            .map(|e| {
                let connected = e.addr().is_some_and(|a| connected.contains(&a));
                HidDevice { connected, le: e.le.is_some(), address: e.address, name: e.name }
            })
            .collect()
    }

    /// Adds a keyboard or mouse and connects it (pairing it on the way).
    /// Only while the feature is on.
    /// `le`: an LE device, with its address type.
    pub fn hid_add(&self, address: &str, name: &str, le: Option<u8>) -> Result<(), Msg> {
        if !self.inner.hid_wanted() {
            return Err(Msg::new("error.hid.off"));
        }
        if bt_host::parse_addr(address).is_none() || le.is_some_and(|t| t > bt_host::LeAddr::RANDOM) {
            return Err(Msg::new("error.hid.connect_failed"));
        }
        // An LE device needs an adapter with LE: not saved without one
        if le.is_some() {
            self.hid_le_host()?;
        }
        let mut list = load(self.config_dir());
        let address = address.to_uppercase();
        let entry = Entry { address: address.clone(), name: name.to_string(), le };
        let full = list.len() >= MAX_DEVICES;
        match list.iter_mut().find(|e| e.address == address) {
            // Added again: as what it is now (classic or LE)
            Some(e) if e.le != le => *e = entry.clone(),
            Some(_) => {}
            None if full => return Err(Msg::new("error.hid.too_many")),
            None => list.push(entry.clone()),
        }
        if let Err(e) = save(self.config_dir(), &list) {
            tracing::warn!("keyboards and mice: the list was not saved ({e})");
        }
        tracing::info!(
            "keyboards and mice: {} on the list, connecting the new one ({})",
            list.len(),
            if le.is_some() { "LE" } else { "classic" }
        );
        self.inner.send_event(ServiceEvent::HidDevices);
        self.hid_connect_entry(&entry, true)
    }

    /// Connects a keyboard or mouse on the list (or given on the command
    /// line) again
    pub fn hid_connect(&self, address: &str) -> Result<(), Msg> {
        if !self.inner.hid_wanted() {
            return Err(Msg::new("error.hid.off"));
        }
        match self.inner.hid_list().into_iter().find(|e| e.address.eq_ignore_ascii_case(address)) {
            Some(e) => self.hid_connect_entry(&e, false),
            None => Err(Msg::new("error.hid.connect_failed")),
        }
    }

    /// The running stack with keyboards and mice, ready for LE ones
    fn hid_le_host(&self) -> Result<Arc<bt_host::BtHost>, Msg> {
        let host = self.inner.ensure_host()?;
        if !host.hid_enabled() {
            return Err(Msg::new("error.hid.restart_pending"));
        }
        let until = Instant::now() + LE_READY_WAIT;
        while !host.hid_le_ready() && !host.hid_le_unavailable() && Instant::now() < until {
            std::thread::sleep(Duration::from_millis(50));
        }
        if host.hid_le_ready() {
            Ok(host)
        } else {
            tracing::warn!("keyboards and mice: the adapter has no Bluetooth LE");
            Err(Msg::new("error.hid.le_unavailable"))
        }
    }

    /// `pair`: the user adds it (hid.add), and its pairing may show a number
    fn hid_connect_entry(&self, entry: &Entry, pair: bool) -> Result<(), Msg> {
        let host = match entry.le {
            Some(_) => self.hid_le_host()?,
            None => self.inner.ensure_host()?,
        };
        if !host.hid_enabled() {
            // A stack from before the feature was turned on, still in use
            return Err(Msg::new("error.hid.restart_pending"));
        }
        let (classic, le) = addresses(&self.inner.hid_list());
        host.hid_set_devices(classic, le);
        // Adding it: its pairing may show a number to type on it, until
        // the pairing ends or the window runs out
        let window = match entry.addr() {
            Some(a) if pair => {
                let generation = host.hid_pairing_begin(a, entry.le.is_some());
                let weak = Arc::downgrade(&host);
                let _ = std::thread::Builder::new().name("hid-pairing".into()).spawn(move || {
                    std::thread::sleep(PAIRING_WINDOW);
                    if let Some(h) = weak.upgrade() {
                        h.hid_pairing_end(a, Some(generation));
                    }
                });
                Some((a, generation))
            }
            _ => None,
        };
        let r = match (entry.le_addr(), entry.addr()) {
            (Some(a), _) => host.hid_le_connect(a),
            (None, Some(a)) => host.hid_connect(a),
            (None, None) => Err(0x12),
        };
        r.map_err(|status| {
            tracing::warn!("keyboards and mice: connecting failed (0x{status:02X})");
            if let Some((a, generation)) = window {
                host.hid_pairing_end(a, Some(generation));
            }
            Msg::new("error.hid.connect_failed")
        })
    }

    /// Connects a keyboard or mouse given on the command line as hid.add
    /// does (its pairing may show a number), without saving it
    pub fn hid_pair(&self, address: &str) -> Result<(), Msg> {
        if !self.inner.hid_wanted() {
            return Err(Msg::new("error.hid.off"));
        }
        match self.inner.hid_list().into_iter().find(|e| e.address.eq_ignore_ascii_case(address)) {
            Some(e) => self.hid_connect_entry(&e, true),
            None => Err(Msg::new("error.hid.connect_failed")),
        }
    }

    /// The number to type on the device being added, while one is shown:
    /// (address, six digits). For the UI only: never log it.
    pub fn hid_passkey(&self) -> Option<(String, String)> {
        self.inner
            .hid
            .passkey
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .map(|(a, k)| (bt_host::addr_str(&a), k.digits()))
    }

    /// Removes a keyboard or mouse: disconnected, off the list, its pairing
    /// forgotten
    pub fn hid_remove(&self, address: &str) {
        let mut list = load(self.config_dir());
        let removed: Vec<Entry> = list.iter().filter(|e| e.address.eq_ignore_ascii_case(address)).cloned().collect();
        list.retain(|e| !e.address.eq_ignore_ascii_case(address));
        if let Err(e) = save(self.config_dir(), &list) {
            tracing::warn!("keyboards and mice: the list was not saved ({e})");
        }
        if let Some(host) = self.host().filter(|h| h.hid_enabled()) {
            let (classic, le) = addresses(&self.inner.hid_list());
            host.hid_set_devices(classic, le);
        }
        if let Some(addr) = bt_host::parse_addr(address) {
            self.inner.hid.connected.lock().unwrap_or_else(|e| e.into_inner()).retain(|&a| a != addr);
            bt_host::hid::forget_descriptors(&descriptors_dir(self.config_dir()), &addr);
            let dir = self.config_dir();
            if removed.iter().any(|e| e.le.is_some()) {
                // Its LE bond, unless LE Audio headphones of that address
                // are saved (as Service::forget_device does)
                if !crate::devices::load_le(dir).iter().any(|d| d.address.eq_ignore_ascii_case(address))
                    && !self.host().is_some_and(|h| h.forget_le_bond(addr))
                {
                    match bt_host::le_bond_file::forget(&crate::paths::le_bonds_file(dir), &addr) {
                        Ok(n) => tracing::debug!("keyboards and mice: {n} LE bond(s) removed from the file"),
                        Err(e) => {
                            tracing::warn!("keyboards and mice: the LE bonds file cannot be changed ({})", e.kind())
                        }
                    }
                }
            } else if !crate::devices::load(dir).iter().any(|d| d.address.eq_ignore_ascii_case(address)) {
                // Unless headphones of the same address are saved too
                self.forget_link_key_only(addr);
            }
        }
        tracing::info!("keyboards and mice: one removed, {} left", list.len());
        self.inner.send_event(ServiceEvent::HidDevices);
    }

    /// Counts of the running stack's HID host; None while it has none
    pub fn hid_counts(&self) -> Option<HidCounts> {
        let host = self.host().filter(|h| h.hid_enabled())?;
        let c = host.hid_counters();
        Some(HidCounts {
            reports: c.reports.load(Ordering::Relaxed),
            undecoded: c.undecoded.load(Ordering::Relaxed),
            events: c.events.load(Ordering::Relaxed),
            injected: c.injected.load(Ordering::Relaxed),
            blocked: c.blocked.load(Ordering::Relaxed),
            refused: c.refused.load(Ordering::Relaxed),
        })
    }

    /// The setting (or developer mode) changed: off takes effect at once,
    /// on as soon as the stack can be restarted
    pub(crate) fn hid_setting_changed(&self, was_on: bool) {
        let on = self.settings().hid_host();
        if on == was_on {
            return;
        }
        if on {
            tracing::warn!(
                "keyboards and mice through BlueCaster turned ON (experimental): their input stops whenever BlueCaster or its Bluetooth stops"
            );
        } else {
            tracing::info!("keyboards and mice through BlueCaster turned off");
            // Now, whatever the stack is busy with: no more input
            if let Some(host) = self.host().filter(|h| h.hid_enabled()) {
                host.hid_set_devices(Vec::new(), Vec::new());
            }
            *self.inner.hid.code.lock().unwrap_or_else(|e| e.into_inner()) = None;
            // The links are closed with their keys released: nothing is
            // left for the guard; the helper releases what it may hold
            self.inner.hid.stop_guard();
            self.inner.hid.stop_helper();
        }
        // A stack with (or without) HID: at once while nothing uses it,
        // else at its next start
        if self.host().is_some() && !self.is_running() && !self.is_scanning() {
            self.inner.shutdown_host();
        }
        self.inner.hid_keep();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_six_digits() {
        for _ in 0..50 {
            let c = new_code();
            assert_eq!(c.len(), 6);
            assert!(c.bytes().all(|b| b.is_ascii_digit()));
        }
    }

    #[test]
    fn an_le_entry_takes_its_identity_address() {
        let e = |address: &str, le: Option<u8>| Entry { address: address.into(), name: "K".into(), le };
        let mut list = vec![e("4A:11:22:33:44:55", Some(1)), e("00:11:22:33:44:55", None)];
        assert!(rename_entry(&mut list, "4A:11:22:33:44:55", "00:A2:D0:00:00:05", 0));
        assert_eq!(list, [e("00:A2:D0:00:00:05", Some(0)), e("00:11:22:33:44:55", None)]);
        // A classic entry is not touched, nor an unknown address
        assert!(!rename_entry(&mut list, "00:11:22:33:44:55", "00:A2:D0:00:00:06", 0));
        assert!(!rename_entry(&mut list, "4A:11:22:33:44:55", "00:A2:D0:00:00:05", 0));
        // The identity on the list already: the private one goes
        list.push(e("5B:11:22:33:44:55", Some(1)));
        assert!(rename_entry(&mut list, "5B:11:22:33:44:55", "00:A2:D0:00:00:05", 0));
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn list_round_trip_drops_bad_lines_and_separators() {
        let dir = std::env::temp_dir().join(format!("bc-hid-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let entry = |a: &str, n: &str, le| Entry { address: a.to_string(), name: n.to_string(), le };
        let list = vec![entry("00:11:22:33:44:55", "Key|board\n", None), entry("C0:11:22:33:44:66", "Mouse", Some(1))];
        save(&dir, &list).unwrap();
        let file = devices_file(&dir);
        let mut text = std::fs::read_to_string(&file).unwrap();
        assert!(text.ends_with("C0:11:22:33:44:66|Mouse|le:random\n"));
        // A line of an earlier version, one of a later one, garbage
        text.push_str("aa:bb:cc:dd:ee:01|Old\nAA:BB:CC:DD:EE:02|New|bt9\nnot an address|x\n");
        std::fs::write(&file, text).unwrap();
        assert_eq!(
            load(&dir),
            vec![
                entry("00:11:22:33:44:55", "Key board ", None),
                entry("C0:11:22:33:44:66", "Mouse", Some(1)),
                entry("AA:BB:CC:DD:EE:01", "Old", None)
            ]
        );
        let (classic, le) = addresses(&load(&dir));
        assert_eq!(classic, vec![[0x00, 0x11, 0x22, 0x33, 0x44, 0x55], [0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0x01]]);
        assert_eq!(le, vec![bt_host::LeAddr { addr: [0xC0, 0x11, 0x22, 0x33, 0x44, 0x66], addr_type: 1 }]);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
