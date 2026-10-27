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
/// Clarity needed to start a note.
const CLARITY_THRESHOLD: f32 = 0.7;
/// Once a note is voiced, frames down to this clarity keep it voiced (hysteresis), as long as the
/// pitch stays within [`HOLD_SEMITONES`] of the held note.
pub const HOLD_CLARITY: f32 = 0.5;
const HOLD_SEMITONES: f32 = 1.0;
/// Mean-square level under which a window counts as silence (about -50 dBFS).
const POWER_THRESHOLD_MS: f32 = 1e-5;
/// Unvoiced gaps shorter than this between voiced frames less than [`BRIDGE_SEMITONES`] apart
/// count as part of one held note (live line, reached notes).
pub const BRIDGE_GAP_SECS: f32 = 0.05;
pub const BRIDGE_SEMITONES: f32 = 1.0;
/// 3 is enough to reject a single wrong frame and costs one hop of delay (5 would cost two).
const MEDIAN_LEN: usize = 3;

/// One live estimate. `frame` is the window centre on the timeline the samples were fed with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LiveFrame {
    pub frame: i64,
    pub hz: Option<f32>,
    /// Clarity of the best pitch candidate (0 below [`HOLD_CLARITY`] or in silence), voiced or not.
    pub clarity: f32,
    /// Level of the analysis window, dBFS RMS.
    pub level_db: f32,
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
    /// Frame positions waiting for their filtered value (filter delay): (frame, clarity, level).
    pending: std::collections::VecDeque<(i64, f32, f32)>,
    /// Pitch (MIDI) of the last voiced raw estimate, while a note is held.
    held: Option<f32>,
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
            held: None,
        }
    }

    /// Feeds mic samples whose first sample sits at `first_frame`. A discontinuity (dropped
    /// block) resets the history.
    pub fn push(&mut self, samples: &[f32], first_frame: i64, out: &mut Vec<LiveFrame>) {
        if first_frame != self.next_frame {
            self.buf.clear();
            self.filter.reset();
            self.pending.clear();
            self.held = None;
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
        let held = self.held;
        let short_win = &buf[buf.len() - SHORT_WINDOW..];
        let level_db = power_db(mean_square(short_win));
        let candidate = detect(&mut self.short, short_win, self.rate);
        let mut est = candidate.filter(|&e| voiced(e, held));
        let mut centre = end_frame - SHORT_WINDOW as i64 / 2;
        // The short window can't see low notes and may report them an octave up, so anything
        // below twice the switch point is cross-checked with the long window.
        if est.is_none_or(|(hz, _)| hz < 2.0 * LOW_SWITCH_HZ)
            && full
            && let Some((hz, c)) = detect(&mut self.long, buf, self.rate)
            && hz < LOW_SWITCH_HZ
            && voiced((hz, c), held)
        {
            est = Some((hz, c));
            centre = end_frame - LONG_WINDOW as i64 / 2;
        }
        self.held = est.map(|(hz, _)| super::note::hz_to_midi(hz));
        let clarity = est.or(candidate).map_or(0.0, |(_, c)| c);
        self.pending.push_back((centre, clarity, level_db));
        let filtered = self.filter.push(est.map(|(hz, _)| hz));
        if self.pending.len() > MEDIAN_LEN / 2 {
            let (frame, clarity, level_db) = self.pending.pop_front().unwrap();
            out.push(LiveFrame {
                frame,
                hz: filtered,
                clarity,
                level_db,
            });
        }
    }
}

/// Best pitch candidate with clarity ≥ [`HOLD_CLARITY`], above the silence level.
fn detect(det: &mut Mpm, win: &[f32], rate: usize) -> Option<(f32, f32)> {
    if mean_square(win) < POWER_THRESHOLD_MS {
        return None;
    }
    det.detect(win, rate as f32, MIN_HZ, MAX_HZ, HOLD_CLARITY)
}

/// Voicing with hysteresis: a confident candidate starts (or moves) a note; a weaker one only
/// continues the note `held` (MIDI) when its pitch stays close to it.
fn voiced((hz, clarity): (f32, f32), held: Option<f32>) -> bool {
    clarity >= CLARITY_THRESHOLD
        || held.is_some_and(|m| (super::note::hz_to_midi(hz) - m).abs() < HOLD_SEMITONES)
}

fn mean_square(win: &[f32]) -> f32 {
    win.iter().map(|v| v * v).sum::<f32>() / win.len() as f32
}

/// dBFS of a mean-square power (a full-scale square wave is 0 dB), floored at −120.
pub fn power_db(mean_square: f32) -> f32 {
    10.0 * mean_square.max(1e-12).log10()
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

    /// Deterministic white noise in -1..1 (xorshift).
    fn noise(n: usize, amp: f32, seed: u32) -> Vec<f32> {
        let mut x = seed.max(1);
        (0..n)
            .map(|_| {
                x ^= x << 13;
                x ^= x >> 17;
                x ^= x << 5;
                amp * (x as f32 / u32::MAX as f32 * 2.0 - 1.0)
            })
            .collect()
    }

    /// Clarity of the short-window candidate for a signal, as the detector would see it.
    fn clarity_of(samples: &[f32]) -> f32 {
        let mut m = Mpm::new(SHORT_WINDOW);
        let w = &samples[samples.len() - SHORT_WINDOW..];
        m.detect(w, 48_000.0, MIN_HZ, MAX_HZ, 0.0)
            .map_or(0.0, |e| e.1)
    }

    #[test]
    fn tone_with_clarity_dips_stays_voiced() {
        // 220 Hz with 40 ms of added noise every 200 ms: clarity drops into the hold range.
        let rate = 48_000;
        let mut s = sine(220.0, rate, 1.2);
        let n = noise(s.len(), 0.5, 7);
        let dip = 40 * 48;
        for k in 0..s.len() {
            if k > 24_000 && k % 9_600 < dip {
                s[k] += n[k];
            }
        }
        let noisy: Vec<f32> = (0..SHORT_WINDOW)
            .map(|k| sine(220.0, rate, 0.1)[k] + n[k])
            .collect();
        let c = clarity_of(&noisy);
        assert!(
            (HOLD_CLARITY..CLARITY_THRESHOLD).contains(&c),
            "test noise should land in the hold range, clarity {c}"
        );
        let frames = run(&s);
        let after_onset: Vec<&LiveFrame> = frames.iter().filter(|f| f.frame > 12_000).collect();
        let unvoiced = after_onset.iter().filter(|f| f.hz.is_none()).count();
        assert_eq!(unvoiced, 0, "{unvoiced} of {} unvoiced", after_onset.len());
        // Noisy windows estimate less precisely (about 70 ¢ at this clarity, 30 ¢ even at 0.8);
        // what the hold guarantees is staying on the note rather than jumping to another.
        let worst = after_onset
            .iter()
            .map(|f| cents_between(f.hz.unwrap(), 220.0).abs())
            .fold(0.0, f32::max);
        eprintln!("worst deviation during dips: {worst:.1} ¢");
        assert!(worst < 100.0 * HOLD_SEMITONES, "{worst}");
    }

    #[test]
    fn noise_after_tone_turns_unvoiced() {
        let mut s = sine(220.0, 48_000, 0.5);
        let tone_end = s.len() as i64;
        s.extend(noise(24_000, 0.5, 3));
        let frames = run(&s);
        // Frames whose window lies entirely in the noise.
        let in_noise: Vec<&LiveFrame> = frames
            .iter()
            .filter(|f| f.frame - (SHORT_WINDOW / 2) as i64 >= tone_end)
            .collect();
        let late_voiced = in_noise.iter().skip(3).filter(|f| f.hz.is_some()).count();
        assert_eq!(late_voiced, 0, "voiced frames in noise after 3 hops");
    }

    #[test]
    fn frames_report_level() {
        let frames = run(&sine(220.0, 48_000, 0.5));
        let last = frames.last().unwrap();
        // A 0.5-amplitude sine: mean square 0.125 → about −9 dBFS.
        assert!((last.level_db + 9.03).abs() < 0.5, "{}", last.level_db);
        let quiet = run(&vec![0.0; 24_000]);
        assert!(quiet.iter().all(|f| f.level_db <= -100.0));
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
