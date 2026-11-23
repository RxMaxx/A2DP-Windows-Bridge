//! The decisions behind the Auracast tab (Developer screen) and its matrix
//! mixer: the broadcast's choices and the request they make, when the
//! broadcast counts as live, the length checks, the adapter list, and the
//! mixer's fields.
//!
//! Ported from app/src/ui/src/screens/Auracast.tsx and screens/AuracastMixer.tsx.

use ipc::{AdapterInfo, DriverKind, Method, StreamState, StreamStatus};

use super::adapter::adapter_name;
use crate::format::js_trim;
use crate::i18n::{js_number, t, tm};
use crate::series::to_fixed;
use crate::t;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {
    Tone,
    Loopback,
}

impl Source {
    pub fn as_str(self) -> &'static str {
        match self {
            Source::Tone => "tone",
            Source::Loopback => "loopback",
        }
    }

    pub fn parse(s: &str) -> Option<Source> {
        match s {
            "tone" => Some(Source::Tone),
            "loopback" => Some(Source::Loopback),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Quality {
    Hq,
    Sq,
}

impl Quality {
    pub fn as_str(self) -> &'static str {
        match self {
            Quality::Hq => "hq",
            Quality::Sq => "sq",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channels {
    Stereo,
    Mono,
    Split,
    Matrix,
}

/// How a pair of matrix outputs is sent (le_audio::broadcast::PairMode)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PairMode {
    Stereo,
    Separate,
    Subgroups,
    Off,
}

impl PairMode {
    pub fn as_str(self) -> &'static str {
        match self {
            PairMode::Stereo => "stereo",
            PairMode::Separate => "separate",
            PairMode::Subgroups => "subgroups",
            PairMode::Off => "off",
        }
    }
}

/// The BIGs (broadcasts) a pair takes
pub fn pair_bigs(m: PairMode) -> u32 {
    match m {
        PairMode::Separate => 2,
        PairMode::Off => 0,
        PairMode::Stereo | PairMode::Subgroups => 1,
    }
}

/// How long after a change of source the stream info names the one that
/// plays (the core switches at its next 200 ms tick, then opens the capture)
pub const SOURCE_SETTLE_MS: u64 = 1500;
/// BIGs the ASUS BT600 runs at most (a third: LE Create BIG 0x0D)
pub const MAX_BIGS: u32 = 2;

/// The core's limits (le_audio::broadcast): UTF-8 bytes
pub const NAME_BYTES: (usize, usize) = (4, 32);
pub const CODE_BYTES: (usize, usize) = (4, 16);

/// The form's choices; they outlive the view (one set per core connection)
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choices {
    pub name: String,
    pub code: String,
    pub quality: Quality,
    pub channels: Channels,
    /// Matrix: the front (FL FR) and the rear (RL RR), each with its rate
    pub front: PairMode,
    pub rear: PairMode,
    pub front_quality: Quality,
    pub rear_quality: Quality,
    pub source: Source,
}

impl Default for Choices {
    fn default() -> Choices {
        Choices {
            name: "BlueCaster".to_owned(),
            code: String::new(),
            quality: Quality::Hq,
            channels: Channels::Stereo,
            front: PairMode::Stereo,
            rear: PairMode::Stereo,
            front_quality: Quality::Hq,
            rear_quality: Quality::Hq,
            source: Source::Tone,
        }
    }
}

/// The layout as stream.start_broadcast takes it
pub fn layout_of(channels: Channels, front: PairMode, rear: PairMode) -> Option<String> {
    match channels {
        Channels::Mono => None,
        Channels::Matrix => Some(format!("matrix_{}_{}", front.as_str(), rear.as_str())),
        Channels::Stereo => Some("stereo".to_owned()),
        Channels::Split => Some("split".to_owned()),
    }
}

/// The error of a name or code field, None when it is fine (an empty code: none)
pub fn length_error(text: &str, (min, max): (usize, usize), optional: bool) -> Option<String> {
    let n = if optional { text.len() } else { js_trim(text).len() };
    if optional && n == 0 {
        return None;
    }
    (n < min || n > max).then(|| t!("ui.dev.auracast_err_length", min, max))
}

impl Choices {
    pub fn matrix(&self) -> bool {
        self.channels == Channels::Matrix
    }

    /// The BIGs a matrix takes (0 for the other layouts)
    pub fn bigs(&self) -> u32 {
        if self.matrix() { pair_bigs(self.front) + pair_bigs(self.rear) } else { 0 }
    }

    pub fn name_error(&self) -> Option<String> {
        length_error(&self.name, NAME_BYTES, false)
    }

    pub fn code_error(&self) -> Option<String> {
        length_error(&self.code, CODE_BYTES, true)
    }

    /// Start is off while a field is wrong or a matrix sends nothing
    pub fn start_disabled(&self, live: bool) -> bool {
        !live && (self.name_error().is_some() || self.code_error().is_some() || (self.matrix() && self.bigs() == 0))
    }

    /// stream.start_broadcast; a matrix's rates come from its front and
    /// rear rows, not Quality
    pub fn start_request(&self) -> Method {
        let matrix = self.matrix();
        Method::StreamStartBroadcast {
            name: js_trim(&self.name).to_owned(),
            code: (!self.code.is_empty()).then(|| self.code.clone()),
            quality: if matrix { self.front_quality } else { self.quality }.as_str().to_owned(),
            rear_quality: matrix.then(|| self.rear_quality.as_str().to_owned()),
            mono: Some(self.channels == Channels::Mono),
            split: None,
            layout: layout_of(self.channels, self.front, self.rear),
            source: self.source.as_str().to_owned(),
            source_id: None,
            source_name: None,
        }
    }
}

/// A broadcast asked for here, as the stream goes
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Session {
    /// A broadcast asked for here, until the stream it started ends or
    /// another takes over
    pub mine: bool,
    /// Why the last broadcast asked for here ended with an error
    pub failed: Option<String>,
    /// stream.start_broadcast not answered yet: the stream before it still shows
    pub asking: bool,
    /// The broadcast asked for here went on air. Until then an Idle is the
    /// session before it ending: the core answers stream.start_broadcast,
    /// then stops what ran before (Idle), then sets the broadcast up
    pub on_air: bool,
}

impl Session {
    /// Follows the stream's state
    pub fn follow(&mut self, st: &StreamStatus) {
        if !self.mine || self.asking {
            return;
        }
        let broadcasting = st.message.key == "status.broadcasting";
        match st.state {
            StreamState::Error => {
                self.failed = Some(tm(Some(&st.message)));
                self.mine = false;
            }
            StreamState::Streaming if broadcasting => self.on_air = true,
            // Stopped, or another stream took over
            StreamState::Streaming => self.mine = false,
            StreamState::Idle if self.on_air => self.mine = false,
            _ => {}
        }
    }

    /// The running stream is the broadcast
    pub fn live(&self, st: &StreamStatus) -> bool {
        match st.state {
            StreamState::Streaming => st.message.key == "status.broadcasting",
            // Setting up, or starting again after the adapter stopped working
            StreamState::Connecting | StreamState::Reconnecting => self.mine && st.profile.is_none(),
            _ => false,
        }
    }

    /// What to show under the form: the broadcast's state, or why it ended
    /// (true: an error)
    pub fn status(&self, st: &StreamStatus) -> Option<(String, bool)> {
        if self.live(st) {
            return Some((tm(Some(&st.message)), false));
        }
        self.failed.clone().map(|f| (f, true))
    }

    /// The form's fields are locked
    pub fn locked(&self, st: &StreamStatus) -> bool {
        self.live(st) || self.asking
    }

    /// While its broadcasts come up the source cannot change yet
    pub fn source_locked(&self, st: &StreamStatus) -> bool {
        self.asking || (self.live(st) && st.state != StreamState::Streaming)
    }
}

/// After a change of source to `asked` and SOURCE_SETTLE_MS: a source that
/// cannot be captured leaves the broadcast on the one before, which its
/// stream info then still names -> the source to show again
pub fn settled_source(live: bool, chosen: Source, asked: Source, playing: Option<&str>) -> Option<Source> {
    let playing = Source::parse(playing?)?;
    (live && chosen == asked && playing != asked).then_some(playing)
}

fn hex4(n: u16) -> String {
    format!("{n:04X}")
}

/// The adapter for broadcasts (Settings::auracast_adapter_id): the current
/// one first, then each adapter on WinUSB by name and MAC address (two
/// adapters can share a name, "Bluetooth Controller"), VID:PID while its
/// address is not known yet, and a chosen one that is not there. The value
/// returned is the option the setting names, whatever its case.
pub fn auracast_adapter_choices(list: &[AdapterInfo], chosen: &str) -> (String, Vec<(String, String)>) {
    let winusb: Vec<&AdapterInfo> = list.iter().filter(|a| a.driver == DriverKind::Winusb).collect();
    let id = js_trim(chosen);
    let mut options = vec![(String::new(), t("ui.dev.auracast_adapter_current"))];
    options.extend(winusb.iter().map(|a| {
        let at = a.address.clone().unwrap_or_else(|| format!("{}:{}", hex4(a.vid), hex4(a.pid)));
        (a.instance_id.clone(), format!("{} · {at}", adapter_name(a)))
    }));
    let found = winusb.iter().find(|a| !id.is_empty() && a.instance_id.to_lowercase() == id.to_lowercase());
    if !id.is_empty() && found.is_none() {
        options.push((id.to_owned(), t("ui.dev.auracast_adapter_missing")));
    }
    (found.map(|a| a.instance_id.clone()).unwrap_or_else(|| id.to_owned()), options)
}

/// The adapter choice is offered while several adapters are on WinUSB, or
/// while one is chosen (to go back to the current one)
pub fn show_adapter(list: &[AdapterInfo], chosen: &str) -> bool {
    list.iter().filter(|a| a.driver == DriverKind::Winusb).count() > 1 || !js_trim(chosen).is_empty()
}

// ---- The matrix mixer ------------------------------------------------------

pub const INPUT_NAMES: [&str; 8] = ["FL", "FR", "FC", "LFE", "BL", "BR", "SL", "SR"];
pub const OUTPUT_NAMES: [&str; 4] = ["FL", "FR", "RL", "RR"];
/// Sound in air at about 20 °C: metres per millisecond
pub const M_PER_MS: f64 = 0.343;
pub const MAX_DELAY_MS: f64 = 500.;
pub const MAX_GAIN: f64 = 4.;
/// Changes go out after this pause (typing, dragging)
pub const MIX_SEND_MS: u64 = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    Ms,
    M,
}

/// `Math.round`: halves go up
fn js_round(n: f64) -> f64 {
    (n + 0.5).floor()
}

/// A gain as the grid shows it
pub fn gain_text(g: f64) -> String {
    if g == 0. {
        return "0".to_owned();
    }
    let s = to_fixed(g, 3);
    let s = s.trim_end_matches('0');
    s.strip_suffix('.').unwrap_or(s).to_owned()
}

/// A delay field's value in `unit` (ms kept to 0.01)
pub fn delay_in(ms: f64, unit: Unit) -> String {
    match unit {
        Unit::Ms => js_number(js_round(ms * 100.) / 100.),
        Unit::M => js_number(js_round(ms * M_PER_MS * 10000.) / 10000.),
    }
}

/// A field's number ("0,25" too), None when it is none
fn number(text: &str) -> Option<f64> {
    let text = text.replacen(',', ".", 1);
    let s = js_trim(&text);
    if s.is_empty() {
        return None;
    }
    s.parse::<f64>().ok().filter(|v| v.is_finite())
}

/// A delay typed in `unit`, in ms within range (None: not a number)
pub fn delay_from(text: &str, unit: Unit) -> Option<f64> {
    let v = number(text)?;
    let ms = match unit {
        Unit::Ms => v,
        Unit::M => v / M_PER_MS,
    };
    Some((js_round(ms * 100.) / 100.).clamp(0., MAX_DELAY_MS))
}

/// A number field's value within lo..=hi (None: not a number)
pub fn num(text: &str, lo: f64, hi: f64) -> Option<f64> {
    number(text).map(|v| v.clamp(lo, hi))
}

/// A preset's name in the list: the built-in ones translated
pub fn preset_label(name: &str, builtin: bool) -> String {
    if builtin { t(&format!("ui.dev.auracast_mix_preset_{name}")) } else { name.to_owned() }
}

/// A name, and not a built-in preset's (the core refuses those)
pub fn can_save_preset(name: &str, builtin_names: &[&str]) -> bool {
    let name = js_trim(name);
    !name.is_empty() && !builtin_names.contains(&name)
}

#[cfg(test)]
mod tests {
    use ipc::Msg;

    use super::*;

    // Auracast.test.ts
    #[test]
    fn names_the_layout_as_the_core_takes_it() {
        let (f, r) = (PairMode::Stereo, PairMode::Separate);
        assert_eq!(layout_of(Channels::Stereo, f, r).as_deref(), Some("stereo"));
        assert_eq!(layout_of(Channels::Mono, f, r), None);
        assert_eq!(layout_of(Channels::Split, f, r).as_deref(), Some("split"));
        assert_eq!(layout_of(Channels::Matrix, f, r).as_deref(), Some("matrix_stereo_separate"));
    }

    #[test]
    fn counts_the_bigs_of_each_pair_mode() {
        let all = [PairMode::Stereo, PairMode::Separate, PairMode::Subgroups, PairMode::Off];
        assert_eq!(all.map(pair_bigs), [1, 2, 1, 0]);
    }

    // AuracastMixer.test.ts
    #[test]
    fn shows_delays_in_ms_or_metres_and_reads_them_back_in_ms() {
        assert_eq!(delay_in(1.234, Unit::Ms), "1.23");
        assert_eq!(delay_in(10., Unit::M), "3.43");
        assert_eq!(delay_from("3.43", Unit::M), Some(10.));
        assert_eq!(delay_from("0,25", Unit::Ms), Some(0.25));
        assert_eq!(delay_from("900", Unit::Ms), Some(500.));
        assert_eq!(delay_from("-1", Unit::Ms), Some(0.));
        assert_eq!(delay_from("", Unit::Ms), None);
        assert_eq!(delay_from("x", Unit::M), None);
    }

    #[test]
    fn shows_gains_without_trailing_zeros() {
        assert_eq!(gain_text(0.), "0");
        assert_eq!(gain_text(1.), "1");
        assert_eq!(gain_text(0.70794), "0.708");
        assert_eq!(gain_text(-0.5), "-0.5");
        assert_eq!(gain_text(10.), "10");
    }

    #[test]
    fn fields_within_range() {
        assert_eq!(num("13", -60., 12.), Some(12.));
        assert_eq!(num("-3,5", -60., 12.), Some(-3.5));
        assert_eq!(num(" ", -60., 12.), None);
    }

    #[test]
    fn presets() {
        assert_eq!(preset_label("Mine", false), "Mine");
        assert!(can_save_preset(" Mine ", &["stereo"]));
        assert!(!can_save_preset("stereo", &["stereo"]));
        assert!(!can_save_preset("  ", &[]));
    }

    // Developer.test.tsx "Auracast tab"
    #[test]
    fn takes_a_matrix_rates_from_the_front_and_rear_rows_not_quality() {
        let c = Choices {
            quality: Quality::Sq,
            channels: Channels::Matrix,
            rear_quality: Quality::Sq,
            ..Choices::default()
        };
        let Method::StreamStartBroadcast { quality, rear_quality, layout, .. } = c.start_request() else {
            panic!("not a broadcast")
        };
        assert_eq!((quality.as_str(), rear_quality.as_deref()), ("hq", Some("sq")));
        assert_eq!(layout.as_deref(), Some("matrix_stereo_stereo"));
    }

    #[test]
    fn starts_a_broadcast_with_the_checked_name_and_code() {
        let mut c = Choices { name: "abc".into(), ..Choices::default() };
        assert_eq!(c.name_error(), Some(t!("ui.dev.auracast_err_length", 4, 32)));
        assert!(c.start_disabled(false));
        assert!(!c.start_disabled(true));
        c.name = "Living room".into();
        // Name and password are checked in bytes
        c.code = "ああああああ".into();
        assert_eq!(c.code_error(), Some(t!("ui.dev.auracast_err_length", 4, 16)));
        c.code = "secret".into();
        assert!(!c.start_disabled(false));
        c.quality = Quality::Sq;
        c.channels = Channels::Mono;
        assert_eq!(
            c.start_request(),
            Method::StreamStartBroadcast {
                name: "Living room".into(),
                code: Some("secret".into()),
                quality: "sq".into(),
                rear_quality: None,
                mono: Some(true),
                split: None,
                layout: None,
                source: "tone".into(),
                source_id: None,
                source_name: None,
            }
        );
        // A matrix with both pairs off sends nothing
        let off = Choices { channels: Channels::Matrix, front: PairMode::Off, rear: PairMode::Off, ..c };
        assert!(off.start_disabled(false));
    }

    fn stream(state: StreamState, key: &str, profile: Option<&str>) -> StreamStatus {
        StreamStatus { state, message: Msg::new(key), profile: profile.map(str::to_owned), ..StreamStatus::default() }
    }

    #[test]
    fn keeps_a_broadcast_whose_adapter_stopped_working_while_it_starts_again() {
        let mut s = Session { mine: true, ..Session::default() };
        let on_air = stream(StreamState::Streaming, "status.broadcasting", None);
        s.follow(&on_air);
        assert!(s.on_air && s.live(&on_air));
        let again = stream(StreamState::Reconnecting, "status.reconnect_attempt", None);
        s.follow(&again);
        assert!(s.mine && s.live(&again) && s.locked(&again) && s.source_locked(&again));
        s.follow(&on_air);
        assert!(s.live(&on_air) && !s.source_locked(&on_air));
        // Stopped
        s.follow(&stream(StreamState::Idle, "status.idle", None));
        assert!(!s.mine);
    }

    #[test]
    fn an_idle_before_going_on_air_is_the_stream_before_ending() {
        let mut s = Session { mine: true, ..Session::default() };
        s.follow(&stream(StreamState::Idle, "status.idle", None));
        assert!(s.mine);
        // Another stream took over
        s.follow(&stream(StreamState::Streaming, "status.streaming", Some("Headphones")));
        assert!(!s.mine);
        // A profile's reconnect is not the broadcast
        let mut s = Session { mine: true, ..Session::default() };
        assert!(!s.live(&stream(StreamState::Connecting, "status.connecting", Some("Headphones"))));
        assert!(s.live(&stream(StreamState::Connecting, "status.connecting", None)));
        s.asking = true;
        s.follow(&stream(StreamState::Error, "error.btstack_init", None));
        assert!(s.mine && s.failed.is_none());
    }

    #[test]
    fn shows_why_a_broadcast_did_not_start() {
        let mut s = Session { mine: true, ..Session::default() };
        let failed = stream(StreamState::Error, "error.btstack_init", None);
        s.follow(&failed);
        assert!(!s.mine);
        assert_eq!(s.status(&failed), Some((t("error.btstack_init"), true)));
        assert_eq!(s.status(&stream(StreamState::Idle, "status.idle", None)), Some((t("error.btstack_init"), true)));
    }

    #[test]
    fn a_source_that_cannot_be_captured_shows_the_one_that_plays() {
        assert_eq!(settled_source(true, Source::Loopback, Source::Loopback, Some("tone")), Some(Source::Tone));
        assert_eq!(settled_source(true, Source::Loopback, Source::Loopback, Some("loopback")), None);
        assert_eq!(settled_source(false, Source::Loopback, Source::Loopback, Some("tone")), None);
        assert_eq!(settled_source(true, Source::Tone, Source::Loopback, Some("tone")), None);
        assert_eq!(settled_source(true, Source::Loopback, Source::Loopback, Some("app")), None);
    }

    fn adapter(instance_id: &str, address: Option<&str>, driver: DriverKind) -> AdapterInfo {
        AdapterInfo {
            instance_id: instance_id.into(),
            vid: 0x8087,
            pid: 0x0033,
            display_name: "Intel AX211 (0x8087:0x0033)".into(),
            device_name: String::new(),
            vendor: "Intel".into(),
            driver,
            driver_service: String::new(),
            realtek_chip: None,
            firmware_present: None,
            firmware_files: vec![],
            firmware_invalid: None,
            firmware_download: false,
            firmware_from_driver: false,
            firmware_driver: false,
            vendor_note: None,
            restart_needed: false,
            le_audio: None,
            address: address.map(str::to_owned),
            bt_version: None,
            windows_name: None,
        }
    }

    #[test]
    fn chooses_the_adapter_for_broadcasts_by_name_and_mac_address() {
        let list = [
            adapter("USB\\A", Some("00:1A:7D:DA:71:13"), DriverKind::Winusb),
            adapter("USB\\B", None, DriverKind::Winusb),
            adapter("USB\\C", Some("11:22:33:44:55:66"), DriverKind::Bluetooth),
        ];
        let (value, options) = auracast_adapter_choices(&list, "");
        assert_eq!(value, "");
        let labels: Vec<&str> = options.iter().map(|(_, l)| l.as_str()).collect();
        assert_eq!(labels[0], t("ui.dev.auracast_adapter_current"));
        assert!(labels[1].ends_with(" · 00:1A:7D:DA:71:13"));
        assert!(labels[2].ends_with(" · 8087:0033"));
        assert_eq!(labels.len(), 3);
        // Whatever its case; a chosen one that is not there
        assert_eq!(auracast_adapter_choices(&list, "usb\\b").0, "USB\\B");
        let (value, options) = auracast_adapter_choices(&list, " USB\\Z ");
        assert_eq!(value, "USB\\Z");
        assert_eq!(options.last().map(|(_, l)| l.clone()), Some(t("ui.dev.auracast_adapter_missing")));
        // Offered with several adapters, or one chosen
        assert!(show_adapter(&list, ""));
        assert!(!show_adapter(&list[..1], ""));
        assert!(show_adapter(&list[..1], "USB\\A"));
    }
}
