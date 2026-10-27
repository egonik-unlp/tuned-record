//! Live pitch detection: McLeod on a sliding window, with octave-error suppression.
//!
//! Runs on the live pitch thread (not real-time), fed with mic samples tagged by song frame.

use super::mpm::Mpm;

pub const SHORT_WINDOW: usize = 1024;
pub const LONG_WINDOW: usize = 2048;
pub const HOP: usize = 128;
pub const MIN_HZ: f32 = 65.0;
pub const MAX_HZ: f32 = 1100.0;
/// Below this the 1024 window becomes unreliable; the 2048 window is used instead.
const LOW_SWITCH_HZ: f32 = 110.0;
const CLARITY_THRESHOLD: f32 = 0.7;
/// Mean-square level under which a window counts as silence (about -50 dBFS).
const POWER_THRESHOLD_MS: f32 = 1e-5;
/// 3 is enough to reject a single wrong frame and costs one hop of delay (5 would cost two).
const MEDIAN_LEN: usize = 3;

/// One live estimate. `frame` is the window centre on the timeline the samples were fed with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiveFrame {
    pub frame: i64,
    pub hz: Option<f32>,
    pub clarity: f32,
}

/// Median over the last [`MEDIAN_LEN`] raw estimates: a single wrong frame (e.g. an octave jump)
/// never wins the median. Output is delayed by `MEDIAN_LEN / 2` hops.
#[derive(Default)]
pub struct OctaveFilter {
    hist: [Option<f32>; MEDIAN_LEN],
    pos: usize,
    filled: usize,
}

impl OctaveFilter {
    /// Pushes a raw estimate (Hz) and returns the filtered value for the frame `MEDIAN_LEN / 2`
    /// hops ago.
    pub fn push(&mut self, raw: Option<f32>) -> Option<f32> {
        self.hist[self.pos] = raw.map(super::note::hz_to_midi);
        self.pos = (self.pos + 1) % MEDIAN_LEN;
        self.filled = (self.filled + 1).min(MEDIAN_LEN);
        let centre = self.hist[(self.pos + MEDIAN_LEN / 2) % MEDIAN_LEN];
        // Unvoiced centre stays unvoiced; isolated voiced blips amid silence are dropped.
        let centre = centre?;
        let mut voiced = [0f32; MEDIAN_LEN];
        let mut n = 0;
        for v in self.hist.iter().take(self.filled).flatten() {
            voiced[n] = *v;
            n += 1;
        }
        match n {
            0 | 1 => return None,
            // At a note's start or end only one neighbour is voiced: keep the centre as is.
            2 => return Some(super::note::midi_to_hz(centre)),
            _ => {}
        }
        let v = &mut voiced[..n];
        v.sort_by(|a, b| a.total_cmp(b));
        Some(super::note::midi_to_hz(v[n / 2]))
    }

    pub fn reset(&mut self) {
        *self = Self::default();
    }
}

pub struct LiveDetector {
    rate: usize,
    buf: std::collections::VecDeque<f32>,
    /// Song frame following the last buffered sample.
    next_frame: i64,
    since_hop: usize,
    short: Mpm,
    long: Mpm,
    filter: OctaveFilter,
    /// Frame positions waiting for their filtered value (filter delay).
    pending: std::collections::VecDeque<(i64, f32)>,
}

impl LiveDetector {
    pub fn new(rate: u32) -> Self {
        Self {
            rate: rate as usize,
            buf: std::collections::VecDeque::with_capacity(LONG_WINDOW),
            next_frame: 0,
            since_hop: 0,
            short: Mpm::new(SHORT_WINDOW),
            long: Mpm::new(LONG_WINDOW),
            filter: OctaveFilter::default(),
            pending: Default::default(),
        }
    }

    /// Feeds mic samples whose first sample sits at `first_frame`. A discontinuity (dropped
    /// block) resets the history.
    pub fn push(&mut self, samples: &[f32], first_frame: i64, out: &mut Vec<LiveFrame>) {
        if first_frame != self.next_frame {
            self.buf.clear();
            self.filter.reset();
            self.pending.clear();
            self.since_hop = 0;
        }
        self.next_frame = first_frame + samples.len() as i64;
        for (k, &s) in samples.iter().enumerate() {
            if self.buf.len() == LONG_WINDOW {
                self.buf.pop_front();
            }
            self.buf.push_back(s);
            self.since_hop += 1;
            if self.since_hop >= HOP && self.buf.len() >= SHORT_WINDOW {
                self.since_hop = 0;
                let end_frame = first_frame + k as i64 + 1;
                self.estimate(end_frame, out);
            }
        }
    }

    fn estimate(&mut self, end_frame: i64, out: &mut Vec<LiveFrame>) {
        let buf = self.buf.make_contiguous();
        let full = buf.len() == LONG_WINDOW;
        let mut est = detect(&mut self.short, &buf[buf.len() - SHORT_WINDOW..], self.rate);
        let mut centre = end_frame - SHORT_WINDOW as i64 / 2;
        // The short window can't see low notes and may report them an octave up, so anything
        // below twice the switch point is cross-checked with the long window.
        if est.is_none_or(|(hz, _)| hz < 2.0 * LOW_SWITCH_HZ)
            && full
            && let Some((hz, c)) = detect(&mut self.long, buf, self.rate)
            && hz < LOW_SWITCH_HZ
        {
            est = Some((hz, c));
            centre = end_frame - LONG_WINDOW as i64 / 2;
        }
        let clarity = est.map_or(0.0, |(_, c)| c);
        self.pending.push_back((centre, clarity));
        let filtered = self.filter.push(est.map(|(hz, _)| hz));
        if self.pending.len() > MEDIAN_LEN / 2 {
            let (frame, clarity) = self.pending.pop_front().unwrap();
            out.push(LiveFrame {
                frame,
                hz: filtered,
                clarity,
            });
        }
    }
}

fn detect(det: &mut Mpm, win: &[f32], rate: usize) -> Option<(f32, f32)> {
    let mean_square = win.iter().map(|v| v * v).sum::<f32>() / win.len() as f32;
    if mean_square < POWER_THRESHOLD_MS {
        return None;
    }
    det.detect(win, rate as f32, MIN_HZ, MAX_HZ, CLARITY_THRESHOLD)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pitch::note::cents_between;

    fn sine(hz: f32, rate: u32, secs: f32) -> Vec<f32> {
        let n = (rate as f32 * secs) as usize;
        (0..n)
            .map(|i| 0.5 * (2.0 * std::f32::consts::PI * hz * i as f32 / rate as f32).sin())
            .collect()
    }

    fn run(samples: &[f32]) -> Vec<LiveFrame> {
        let mut d = LiveDetector::new(48_000);
        let mut out = Vec::new();
        for (i, chunk) in samples.chunks(256).enumerate() {
            d.push(chunk, (i * 256) as i64, &mut out);
        }
        out
    }

    #[test]
    fn steady_a3_within_10_cents() {
        let frames = run(&sine(220.0, 48_000, 1.0));
        let voiced: Vec<f32> = frames.iter().filter_map(|f| f.hz).collect();
        assert!(
            voiced.len() > frames.len() * 9 / 10,
            "{} of {}",
            voiced.len(),
            frames.len()
        );
        for hz in voiced {
            assert!(cents_between(hz, 220.0).abs() < 10.0, "{hz}");
        }
    }

    #[test]
    fn update_rate_over_100_per_second() {
        let frames = run(&sine(330.0, 48_000, 1.0));
        assert!(frames.len() >= 100 * 9 / 10 * 3, "{}", frames.len());
    }

    #[test]
    fn low_note_uses_long_window() {
        let frames = run(&sine(70.0, 48_000, 1.0));
        let voiced: Vec<f32> = frames.iter().filter_map(|f| f.hz).collect();
        assert!(!voiced.is_empty());
        for hz in &voiced[voiced.len() / 2..] {
            assert!(cents_between(*hz, 70.0).abs() < 15.0, "{hz}");
        }
    }

    /// Tone needed after its onset before the detector emits a voiced frame (fed hop by hop:
    /// buffer granularity is already part of the reported capture latency). Part of the < 40 ms
    /// voice-to-line budget, see design.md → Live pitch.
    #[test]
    fn onset_detection_delay_under_15_ms() {
        let onset = 24_000;
        let mut s = vec![0.0; onset];
        s.extend(sine(220.0, 48_000, 0.5));
        let mut d = LiveDetector::new(48_000);
        let mut out = Vec::new();
        let mut fed = 0;
        for chunk in s.chunks(HOP) {
            d.push(chunk, fed as i64, &mut out);
            fed += chunk.len();
            if out.iter().any(|f| f.hz.is_some()) {
                break;
            }
        }
        let delay_ms = (fed - onset) as f32 / 48.0;
        eprintln!("onset detection delay: {delay_ms:.1} ms");
        assert!(delay_ms < 15.0, "{delay_ms} ms");
    }

    #[test]
    fn silence_is_unvoiced() {
        let frames = run(&vec![0.0; 48_000]);
        assert!(frames.iter().all(|f| f.hz.is_none()));
    }

    #[test]
    fn filter_drops_single_octave_frame() {
        let mut f = OctaveFilter::default();
        let mut out = Vec::new();
        for i in 0..20 {
            let raw = if i == 10 { 440.0 } else { 220.0 };
            out.push(f.push(Some(raw)));
        }
        for v in out.into_iter().flatten() {
            assert!(cents_between(v, 220.0).abs() < 1.0, "{v}");
        }
    }
}
