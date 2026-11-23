//! The arithmetic of the Diagnostics screen's link tab, as v1's link quality
//! dialog did it (wx_link_quality_dialog.cpp on the develop branch): losses since the
//! stream started or since the Reset button, drop shares, capture drops in
//! milliseconds. The core keeps cumulative counters only; the baselines are
//! the view's.

use ipc::{LinkCounters, LinkSample, LinkTotals};

use crate::series::to_fixed;

/// BR/EDR slot length
pub const SLOT_SECONDS: f64 = 625e-6;
/// 3-DH5 back to back on an EDR 3 Mbps link: 1021 bytes every 6 slots (2178 kbps)
pub const EDR3_MAX_KBPS: f64 = (1021.0 * 8.0) / (6.0 * SLOT_SECONDS) / 1000.0;

pub const ZERO_TOTALS: LinkTotals = LinkTotals {
    packets: 0,
    bytes: 0,
    dropped: 0,
    capture_frames: 0,
    capture_dropped: 0,
    capture_discontinuities: 0,
    capture_lost: 0,
};

/// The counters of a LinkTotals, in its field order
fn fields(t: &LinkTotals) -> [u64; 7] {
    [t.packets, t.bytes, t.dropped, t.capture_frames, t.capture_dropped, t.capture_discontinuities, t.capture_lost]
}

/// `now - base` per counter (never negative)
pub fn diff_totals(now: &LinkTotals, base: &LinkTotals) -> LinkTotals {
    now.since(base)
}

/// The counters when Reset was pressed
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkBaseline {
    pub totals: LinkTotals,
    /// LinkCounters.stream_starts then
    pub stream_starts: u32,
    /// LinkSample.time_ms then
    pub time_ms: u64,
}

/// The baseline Reset takes from a sample (None: the core sends no counters)
pub fn take_baseline(s: Option<&LinkSample>) -> Option<LinkBaseline> {
    let s = s?;
    let c = s.counters.as_ref()?;
    Some(LinkBaseline { totals: c.total, stream_starts: c.stream_starts, time_ms: s.time_ms })
}

/// What [`active_base`] counts from
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ActiveBase {
    pub base: LinkTotals,
    pub since_reset: bool,
}

/// What the losses count from: the reset baseline, or the stream start when
/// there is none, when a stream started after the reset (the later of the
/// two), or when the counters went below it (the core restarted).
pub fn active_base(c: &LinkCounters, reset: Option<&LinkBaseline>) -> ActiveBase {
    if let Some(reset) = reset
        && reset.stream_starts == c.stream_starts
        && fields(&c.total).iter().zip(fields(&reset.totals)).all(|(now, base)| *now >= base)
    {
        return ActiveBase { base: reset.totals, since_reset: true };
    }
    ActiveBase { base: c.stream_start, since_reset: false }
}

/// part / whole in percent; 0 when there is no whole
pub fn percent(part: f64, whole: f64) -> f64 {
    if whole > 0.0 { (100.0 * part) / whole } else { 0.0 }
}

/// A percentage for a gauge: 0..100
pub fn gauge_value(pct: Option<f64>) -> f64 {
    match pct {
        Some(p) if p.is_finite() => p.clamp(0.0, 100.0),
        _ => 0.0,
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LossStats {
    /// Media packets handed to L2CAP
    pub sent: u64,
    /// Lost before the radio
    pub dropped: u64,
    /// dropped / (sent + dropped), percent
    pub drop_pct: f64,
    pub capture_frames: u64,
    pub capture_dropped: u64,
    /// Capture drops as audio time; None while the capture rate is unknown
    pub capture_ms: Option<f64>,
    /// capture_dropped / capture_frames, percent
    pub capture_pct: f64,
    /// Gaps Windows reported in the captured audio
    pub capture_gaps: u64,
    /// The audio lost in them; None while the rate is unknown
    pub capture_lost_ms: Option<f64>,
    /// Counted since the Reset button (else since the stream started)
    pub since_reset: bool,
}

/// Frames as ms at `rate`; 0 frames are 0 ms at any rate
fn frames_ms(frames: u64, rate: u32) -> Option<f64> {
    if rate > 0 {
        Some((frames as f64 * 1000.0) / rate as f64)
    } else if frames == 0 {
        Some(0.0)
    } else {
        None
    }
}

/// The losses of a sample since the active baseline. `rate` is the capture
/// rate to convert dropped frames with when the sample has none (0 while not
/// streaming, e.g. the last stream's StreamInfo.source_sample_rate).
/// None when the core sends no counters.
pub fn loss_stats(s: &LinkSample, reset: Option<&LinkBaseline>, rate: Option<u32>) -> Option<LossStats> {
    let c = s.counters.as_ref()?;
    let ActiveBase { base, since_reset } = active_base(c, reset);
    let d = diff_totals(&c.total, &base);
    let r = s.source_rate.filter(|&r| r != 0).or(rate).unwrap_or(0);
    Some(LossStats {
        sent: d.packets,
        dropped: d.dropped,
        drop_pct: percent(d.dropped as f64, (d.packets + d.dropped) as f64),
        capture_frames: d.capture_frames,
        capture_dropped: d.capture_dropped,
        capture_ms: frames_ms(d.capture_dropped, r),
        capture_pct: percent(d.capture_dropped as f64, d.capture_frames as f64),
        capture_gaps: d.capture_discontinuities,
        capture_lost_ms: frames_ms(d.capture_lost, r),
        since_reset,
    })
}

/// Packets lost per sample (the first sample: 0), for the drop graph
pub fn drops_per_sample(samples: &[LinkSample]) -> Vec<u64> {
    samples
        .iter()
        .enumerate()
        .map(|(i, x)| {
            let Some(p) = i.checked_sub(1).map(|j| &samples[j]) else { return 0 };
            match (&x.counters, &p.counters) {
                (Some(a), Some(b)) => a.total.dropped.saturating_sub(b.total.dropped),
                _ => x.dropped.saturating_sub(p.dropped),
            }
        })
        .collect()
}

/// 12 345 678 -> "12.3 MB" (bytes as Explorer shows them, 1024-based)
pub fn format_bytes(n: u64) -> String {
    if n < 1024 {
        return format!("{n} B");
    }
    const UNITS: [&str; 3] = ["KB", "MB", "GB"];
    let mut v = n as f64 / 1024.0;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    format!("{} {}", to_fixed(v, if v >= 100.0 { 0 } else { 1 }), UNITS[i])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample(total: LinkTotals, start: LinkTotals) -> LinkSample {
        LinkSample {
            time_ms: 1000,
            source_rate: Some(48000),
            counters: Some(LinkCounters { total, stream_start: start, stream_starts: 1 }),
            ..Default::default()
        }
    }

    fn approx(a: f64, b: f64, digits: i32) {
        assert!((a - b).abs() < 10f64.powi(-digits) / 2.0, "{a} is not close to {b}");
    }

    #[test]
    fn knows_the_edr_3_mbps_ceiling() {
        approx(EDR3_MAX_KBPS, 2178.13, 1);
    }

    #[test]
    fn subtracts_counters_without_going_negative() {
        let now = LinkTotals { packets: 10, bytes: 100, ..ZERO_TOTALS };
        let base = LinkTotals { packets: 4, bytes: 200, ..ZERO_TOTALS };
        assert_eq!(diff_totals(&now, &base), LinkTotals { packets: 6, bytes: 0, ..ZERO_TOTALS });
    }

    #[test]
    fn counts_since_the_stream_started_without_a_reset() {
        let s = sample(
            LinkTotals { packets: 1100, dropped: 12, capture_frames: 96000, capture_dropped: 480, ..ZERO_TOTALS },
            LinkTotals { packets: 100, dropped: 2, ..ZERO_TOTALS },
        );
        let l = loss_stats(&s, None, None).unwrap();
        assert!(!l.since_reset);
        assert_eq!(l.sent, 1000);
        assert_eq!(l.dropped, 10);
        // 10 of 1010 packets
        approx(l.drop_pct, 0.990099, 5);
        // 480 frames at 48 kHz = 10 ms, of 96000 frames = 0.5 %
        assert_eq!(l.capture_ms, Some(10.0));
        approx(l.capture_pct, 0.5, 6);
    }

    #[test]
    fn counts_since_reset_until_a_newer_stream_starts_or_the_core_restarts() {
        let at =
            sample(LinkTotals { packets: 500, dropped: 5, ..ZERO_TOTALS }, LinkTotals { packets: 100, ..ZERO_TOTALS });
        let reset = take_baseline(Some(&at)).unwrap();
        assert_eq!(reset.totals.packets, 500);
        let later =
            sample(LinkTotals { packets: 800, dropped: 6, ..ZERO_TOTALS }, LinkTotals { packets: 100, ..ZERO_TOTALS });
        let l = loss_stats(&later, Some(&reset), None).unwrap();
        assert_eq!((l.since_reset, l.sent, l.dropped), (true, 300, 1));

        // A new stream after the reset: from its start
        let mut next = sample(
            LinkTotals { packets: 900, dropped: 6, ..ZERO_TOTALS },
            LinkTotals { packets: 850, dropped: 6, ..ZERO_TOTALS },
        );
        next.counters.as_mut().unwrap().stream_starts = 2;
        let c = next.counters.unwrap();
        assert_eq!(active_base(&c, Some(&reset)), ActiveBase { base: c.stream_start, since_reset: false });
        let l = loss_stats(&next, Some(&reset), None).unwrap();
        assert_eq!((l.since_reset, l.sent, l.dropped), (false, 50, 0));

        // Counters below the baseline: a new core
        let restarted = sample(LinkTotals { packets: 20, ..ZERO_TOTALS }, ZERO_TOTALS);
        let l = loss_stats(&restarted, Some(&reset), None).unwrap();
        assert_eq!((l.since_reset, l.sent), (false, 20));
    }

    #[test]
    fn converts_capture_drops_with_the_last_known_rate_when_not_streaming() {
        let idle = LinkSample {
            source_rate: Some(0),
            ..sample(LinkTotals { capture_frames: 44100, capture_dropped: 441, ..ZERO_TOTALS }, ZERO_TOTALS)
        };
        assert_eq!(loss_stats(&idle, None, Some(44100)).unwrap().capture_ms, Some(10.0));
        assert_eq!(loss_stats(&idle, None, None).unwrap().capture_ms, None);
        let clean = LinkSample {
            source_rate: Some(0),
            ..sample(LinkTotals { capture_frames: 44100, ..ZERO_TOTALS }, ZERO_TOTALS)
        };
        assert_eq!(loss_stats(&clean, None, None).unwrap().capture_ms, Some(0.0));
    }

    #[test]
    fn has_no_losses_without_counters_an_older_core() {
        let old = LinkSample { counters: None, ..sample(ZERO_TOTALS, ZERO_TOTALS) };
        assert_eq!(loss_stats(&old, None, None), None);
        assert_eq!(take_baseline(Some(&old)), None);
        assert_eq!(take_baseline(None), None);
    }

    #[test]
    fn drops_per_sample_from_the_counters_or_the_per_stream_fields() {
        let a = sample(LinkTotals { dropped: 3, capture_dropped: 1, ..ZERO_TOTALS }, ZERO_TOTALS);
        let b = sample(LinkTotals { dropped: 5, capture_dropped: 4, ..ZERO_TOTALS }, ZERO_TOTALS);
        let c = sample(LinkTotals { dropped: 5, capture_dropped: 4, ..ZERO_TOTALS }, ZERO_TOTALS);
        assert_eq!(drops_per_sample(&[a, b, c]), [0, 2, 0]);
        let old = |dropped: u64| LinkSample { counters: None, dropped, ..sample(ZERO_TOTALS, ZERO_TOTALS) };
        assert_eq!(drops_per_sample(&[old(1), old(4), old(0)]), [0, 3, 0]);
    }

    #[test]
    fn percentages_and_gauges() {
        assert_eq!(percent(1.0, 0.0), 0.0);
        assert_eq!(percent(1.0, 4.0), 25.0);
        assert_eq!(gauge_value(Some(-3.0)), 0.0);
        assert_eq!(gauge_value(Some(140.0)), 100.0);
        assert_eq!(gauge_value(None), 0.0);
        assert_eq!(gauge_value(Some(f64::NAN)), 0.0);
        assert_eq!(gauge_value(Some(42.5)), 42.5);
    }

    #[test]
    fn formats_file_sizes() {
        assert_eq!(format_bytes(64), "64 B");
        assert_eq!(format_bytes(1536), "1.5 KB");
        assert_eq!(format_bytes(12 * 1024 * 1024 + 300 * 1024), "12.3 MB");
        assert_eq!(format_bytes(512 * 1024 * 1024), "512 MB");
    }
}
