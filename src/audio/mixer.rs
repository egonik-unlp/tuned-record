//! Track mixing, shared by the real-time callback and the offline export renderer.
//!
//! Nothing here allocates: a [`Playback`] snapshot is built off the audio thread and swapped in whole.

use std::sync::Arc;

use super::speed::Ratio;

/// Audio at the engine rate, interleaved, 1 or 2 channels.
#[derive(Debug)]
pub struct Audio {
    pub channels: usize,
    pub samples: Vec<f32>,
}

impl Audio {
    pub fn mono(samples: Vec<f32>) -> Self {
        Self {
            channels: 1,
            samples,
        }
    }

    pub fn frames(&self) -> usize {
        self.samples.len() / self.channels.max(1)
    }

    /// Mono downmix (for pitch analysis).
    pub fn to_mono(&self) -> Vec<f32> {
        if self.channels == 1 {
            return self.samples.clone();
        }
        self.samples
            .chunks_exact(self.channels)
            .map(|f| f.iter().sum::<f32>() / self.channels as f32)
            .collect()
    }
}

/// One audible source placed on the song timeline.
#[derive(Debug, Clone)]
pub struct Track {
    pub audio: Arc<Audio>,
    /// Song frame of the first sample.
    pub start: i64,
    pub gain: f32,
    /// Speed the audio was rendered for: `1/1` for the original, else a stretched copy that is
    /// `q/p` times as long and plays one frame per output frame while the clock runs at `p/q`.
    pub speed: Ratio,
}

impl Track {
    /// Song frame after the last sample.
    pub fn end(&self) -> i64 {
        self.start + self.speed.to_song(self.audio.frames() as i64)
    }
}

/// Everything the engine plays: an immutable snapshot of the audible tracks and their gains.
#[derive(Debug, Clone, Default)]
pub struct Playback {
    pub tracks: Vec<Track>,
}

impl Playback {
    /// Adds the tracks' audio for an output block starting at song frame `first` to the outputs.
    /// A track rendered at speed `p/q` is read from index `⌊(first − start)·q/p⌋`, one frame per
    /// output frame; at `1/1` that is song frame for song frame.
    pub fn mix_into(&self, first: i64, out_l: &mut [f32], out_r: &mut [f32]) {
        let n = out_l.len() as i64;
        for t in &self.tracks {
            let base = t.speed.to_wall(first - t.start);
            let len = t.audio.frames() as i64;
            let (a, b) = ((-base).max(0), (len - base).min(n));
            if a >= b || t.gain == 0.0 {
                continue;
            }
            let ch = t.audio.channels;
            let src = &t.audio.samples;
            for o in a..b {
                let s = (base + o) as usize * ch;
                let o = o as usize;
                let (l, r) = if ch == 1 {
                    (src[s], src[s])
                } else {
                    (src[s], src[s + 1])
                };
                out_l[o] += l * t.gain;
                out_r[o] += r * t.gain;
            }
        }
    }

    pub fn end(&self) -> i64 {
        self.tracks.iter().map(Track::end).max().unwrap_or(0)
    }
}

/// Which takes are heard under the mute/solo rules: if any take is soloed, only soloed takes;
/// otherwise every unmuted take.
pub fn take_audible(muted: bool, soloed: bool, any_solo: bool) -> bool {
    if any_solo { soloed } else { !muted }
}

/// dB → linear gain; anything at or below -60 dB is silence.
pub fn db_to_gain(db: f32) -> f32 {
    if db <= -60.0 {
        0.0
    } else {
        10f32.powf(db / 20.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn track(start: i64, ch: usize, samples: Vec<f32>, gain: f32) -> Track {
        Track {
            audio: Arc::new(Audio {
                channels: ch,
                samples,
            }),
            start,
            gain,
            speed: Ratio::ONE,
        }
    }

    #[test]
    fn stretched_track_is_read_at_the_slowed_index() {
        // A ramp rendered for 60 %: sample i holds i + 1, 5/3 as long as the 600-frame original.
        let t = Track {
            speed: Ratio::new(3, 5),
            ..track(100, 1, (1..=1_000).map(|i| i as f32).collect(), 1.0)
        };
        assert_eq!(t.end(), 700);
        let pb = Playback { tracks: vec![t] };
        // After a seek to song frame 400: index (400 − 100)·5/3 = 500.
        let mut l = vec![0.0; 4];
        let mut r = vec![0.0; 4];
        pb.mix_into(400, &mut l, &mut r);
        assert_eq!(l, vec![501.0, 502.0, 503.0, 504.0]);
        // A block starting 2 song frames early: index ⌊−2·5/3⌋ = −4, so 4 frames of silence.
        let mut l = vec![0.0; 6];
        let mut r = vec![0.0; 6];
        pb.mix_into(98, &mut l, &mut r);
        assert_eq!(l, vec![0.0, 0.0, 0.0, 0.0, 1.0, 2.0]);
    }

    #[test]
    fn mixes_overlapping_region_only() {
        let pb = Playback {
            tracks: vec![
                track(0, 2, vec![1.0, 2.0, 1.0, 2.0, 1.0, 2.0], 1.0),
                track(2, 1, vec![0.5; 4], 2.0),
            ],
        };
        let mut l = vec![0.0; 4];
        let mut r = vec![0.0; 4];
        pb.mix_into(1, &mut l, &mut r);
        assert_eq!(l, vec![1.0, 2.0, 1.0, 1.0]);
        assert_eq!(r, vec![2.0, 3.0, 1.0, 1.0]);
        assert_eq!(pb.end(), 6);
    }

    #[test]
    fn solo_rules() {
        assert!(take_audible(false, false, false));
        assert!(!take_audible(true, false, false));
        assert!(!take_audible(false, false, true));
        assert!(take_audible(true, true, true));
    }
}
