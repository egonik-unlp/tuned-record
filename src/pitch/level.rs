//! Input level: the meter values the live pitch thread publishes for the UI, and the warnings
//! derived from them (clipping, too quiet to track).

use std::collections::VecDeque;
use std::sync::atomic::Ordering::Relaxed;
use std::sync::atomic::{AtomicI64, AtomicU32};

use super::live::HOLD_CLARITY;

/// A sample at or above this magnitude counts as clipped (float inputs can exceed 1.0).
pub const CLIP_LEVEL: f32 = 0.999;
/// The clipping warning stays up this long after the last clipped sample.
pub const CLIP_HOLD_SECS: f32 = 2.0;
/// Peak fall-back speed of the meter.
const PEAK_DECAY_DB_PER_SEC: f32 = 20.0;
/// Time constant of the meter's RMS.
const RMS_SECS: f32 = 0.3;
/// The too-quiet rule looks at the pitched frames of this last stretch of singing...
pub const QUIET_WINDOW_SECS: f32 = 2.0;
/// ...needs at least this much of them...
pub const QUIET_MIN_PITCHED_SECS: f32 = 0.3;
/// ...and warns when their median level is below this (dBFS RMS).
pub const QUIET_DB: f32 = -40.0;

/// Meter state shared by the live pitch thread (writer) and the UI (reader).
pub struct InputLevel {
    rate: u32,
    /// Decaying block peak, linear (f32 bits).
    peak: AtomicU32,
    /// Smoothed mean square (f32 bits).
    mean_square: AtomicU32,
    /// Wall frame of the last clipped sample (`i64::MIN` when none yet).
    last_clip: AtomicI64,
    /// Wall frame just after the newest sample seen.
    now: AtomicI64,
}

impl InputLevel {
    pub fn new(rate: u32) -> Self {
        Self {
            rate: rate.max(1),
            peak: AtomicU32::new(0f32.to_bits()),
            mean_square: AtomicU32::new(0f32.to_bits()),
            last_clip: AtomicI64::new(i64::MIN),
            now: AtomicI64::new(0),
        }
    }

    /// Accounts for a block of mic samples whose first sample is at wall frame `first`.
    pub fn update(&self, block: &[f32], first: i64) {
        if block.is_empty() {
            return;
        }
        let secs = block.len() as f32 / self.rate as f32;
        let mut block_peak = 0f32;
        let mut sum = 0f32;
        for (i, &s) in block.iter().enumerate() {
            let a = s.abs();
            block_peak = block_peak.max(a);
            sum += s * s;
            if a >= CLIP_LEVEL {
                self.last_clip.store(first + i as i64, Relaxed);
            }
        }
        let decay = 10f32.powf(-PEAK_DECAY_DB_PER_SEC * secs / 20.0);
        let peak = f32::from_bits(self.peak.load(Relaxed)) * decay;
        self.peak.store(peak.max(block_peak).to_bits(), Relaxed);
        let k = (-secs / RMS_SECS).exp();
        let ms = f32::from_bits(self.mean_square.load(Relaxed));
        let ms = ms * k + sum / block.len() as f32 * (1.0 - k);
        self.mean_square.store(ms.to_bits(), Relaxed);
        self.now.store(first + block.len() as i64, Relaxed);
    }

    pub fn peak_db(&self) -> f32 {
        20.0 * f32::from_bits(self.peak.load(Relaxed)).max(1e-6).log10()
    }

    pub fn rms_db(&self) -> f32 {
        super::live::power_db(f32::from_bits(self.mean_square.load(Relaxed)))
    }

    /// Whether a sample clipped within the last [`CLIP_HOLD_SECS`].
    pub fn clipping(&self) -> bool {
        let last = self.last_clip.load(Relaxed);
        last != i64::MIN
            && self.now.load(Relaxed) - last <= (CLIP_HOLD_SECS * self.rate as f32) as i64
    }
}

/// Watches the singing during playback for an input too low to track reliably: pitched frames
/// (clarity ≥ [`HOLD_CLARITY`]) whose median level stays under [`QUIET_DB`]. Silence produces no
/// pitched frames, so pauses between phrases never warn.
pub struct QuietWatch {
    rate: u32,
    hop: i64,
    /// (wall frame, level dBFS) of recent pitched frames.
    pitched: VecDeque<(i64, f32)>,
}

impl QuietWatch {
    pub fn new(rate: u32, hop: usize) -> Self {
        Self {
            rate: rate.max(1),
            hop: hop as i64,
            pitched: VecDeque::new(),
        }
    }

    /// Feeds one live frame heard while the transport was playing.
    pub fn push(&mut self, wall_frame: i64, level_db: f32, clarity: f32) {
        if clarity >= HOLD_CLARITY {
            self.pitched.push_back((wall_frame, level_db));
        }
        let window = (QUIET_WINDOW_SECS * self.rate as f32) as i64;
        while self
            .pitched
            .front()
            .is_some_and(|&(f, _)| f < wall_frame - window)
        {
            self.pitched.pop_front();
        }
    }

    pub fn clear(&mut self) {
        self.pitched.clear();
    }

    pub fn too_quiet(&self) -> bool {
        let min = (QUIET_MIN_PITCHED_SECS * self.rate as f32) as i64 / self.hop.max(1);
        if (self.pitched.len() as i64) < min {
            return false;
        }
        let mut levels: Vec<f32> = self.pitched.iter().map(|p| p.1).collect();
        levels.sort_by(|a, b| a.total_cmp(b));
        levels[levels.len() / 2] < QUIET_DB
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pitch::live::{HOP, LiveDetector, LiveFrame};

    const RATE: u32 = 48_000;

    fn sine(amp: f32, secs: f32) -> Vec<f32> {
        let n = (RATE as f32 * secs) as usize;
        (0..n)
            .map(|i| amp * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / RATE as f32).sin())
            .collect()
    }

    /// Feeds samples in 256-frame blocks to both the meter and the detector.
    fn feed(samples: &[f32], level: &InputLevel, watch: &mut QuietWatch) {
        let mut det = LiveDetector::new(RATE);
        let mut frames: Vec<LiveFrame> = Vec::new();
        for (i, b) in samples.chunks(256).enumerate() {
            let first = (i * 256) as i64;
            level.update(b, first);
            frames.clear();
            det.push(b, first, &mut frames);
            for f in &frames {
                watch.push(f.frame, f.level_db, f.clarity);
            }
        }
    }

    #[test]
    fn clipped_sine_warns_clipping() {
        let s: Vec<f32> = sine(1.6, 1.0)
            .into_iter()
            .map(|v| v.clamp(-1.0, 1.0))
            .collect();
        let level = InputLevel::new(RATE);
        let mut w = QuietWatch::new(RATE, HOP);
        feed(&s, &level, &mut w);
        assert!(level.clipping());
        assert!(level.peak_db() > -0.1);
        assert!(!w.too_quiet());
    }

    #[test]
    fn clipping_warning_lasts_two_seconds() {
        let level = InputLevel::new(RATE);
        level.update(&[1.0], 0);
        let quiet = vec![0.01f32; 4_800];
        let mut f = 1;
        while f < (1.9 * RATE as f32) as i64 {
            level.update(&quiet, f);
            f += quiet.len() as i64;
        }
        assert!(level.clipping(), "still within 2 s");
        while f < (2.1 * RATE as f32) as i64 {
            level.update(&quiet, f);
            f += quiet.len() as i64;
        }
        assert!(!level.clipping(), "cleared after 2 s");
    }

    #[test]
    fn quiet_singing_warns_too_quiet() {
        // A sine at −45 dBFS RMS.
        let amp = 10f32.powf(-45.0 / 20.0) * std::f32::consts::SQRT_2;
        let level = InputLevel::new(RATE);
        let mut w = QuietWatch::new(RATE, HOP);
        feed(&sine(amp, 1.0), &level, &mut w);
        assert!(w.too_quiet());
        assert!(!level.clipping());
        assert!((level.rms_db() + 45.0).abs() < 1.0, "{}", level.rms_db());
    }

    #[test]
    fn levelled_voice_and_silence_do_not_warn() {
        let level = InputLevel::new(RATE);
        let mut w = QuietWatch::new(RATE, HOP);
        // Peaks at −6 dBFS, then rests.
        let mut s = sine(0.5, 1.0);
        s.extend(vec![0.0; RATE as usize * 3]);
        feed(&s, &level, &mut w);
        assert!(!level.clipping());
        assert!(!w.too_quiet());

        let level = InputLevel::new(RATE);
        let mut w = QuietWatch::new(RATE, HOP);
        feed(&vec![0.0; RATE as usize], &level, &mut w);
        assert!(!level.clipping());
        assert!(!w.too_quiet());
        assert!(level.peak_db() <= -119.0);
    }
}
