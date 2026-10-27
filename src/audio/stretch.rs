//! Offline time stretching for practice speeds (Signalsmith Stretch): pitch kept, tempo slowed.
//!
//! Rendered once per source and speed off the audio thread; the callback only reads the result.

use signalsmith_stretch::Stretch;

use super::mixer::Audio;
use super::speed::Ratio;

/// Identifies the stretcher and its settings, for cache keys: bump when either changes.
pub const STRETCHER_VERSION: &str = "signalsmith-stretch 0.1.3 preset_default v1";
/// Input frames processed per step (one progress report each).
const CHUNK: usize = 4096;

/// Renders `input` to play at `speed` with its pitch unchanged: the result is `q/p` times as
/// long, and its frame `k` lines up with input frame `k·p/q`. `progress` gets the fraction done
/// and returns false to cancel (then `None` is returned).
pub fn render(
    input: &Audio,
    rate: u32,
    speed: Ratio,
    mut progress: impl FnMut(f32) -> bool,
) -> Option<Audio> {
    let ch = input.channels.max(1);
    let n = input.frames();
    let (p, q) = (speed.p as u64, speed.q as u64);
    let out_n = (n as u64 * q / p) as usize;
    let mut st = Stretch::preset_default(ch as u32, rate);
    let (in_lat, out_lat) = (st.input_latency(), st.output_latency());
    let src = &input.samples[..n * ch];

    // Start centred on input frame 0, as `exact()` does: the first `in_lat` frames pre-roll, then
    // the rest plus `in_lat` frames of silence are fed, and the first `out_lat` output frames
    // (the stretcher's own delay) are dropped at the end.
    st.seek(&src[..in_lat.min(n) * ch], p as f64 / q as f64);
    let mut out = Vec::with_capacity((out_n + out_lat) * ch);
    let mut buf = Vec::with_capacity(CHUNK * ch);
    let (mut fed, mut produced) = (0usize, 0u64);
    while fed < n {
        let len = CHUNK.min(n - fed);
        let (a, b) = ((in_lat + fed).min(n), (in_lat + fed + len).min(n));
        buf.clear();
        buf.extend_from_slice(&src[a * ch..b * ch]);
        buf.resize(len * ch, 0.0);
        fed += len;
        let want = fed as u64 * q / p;
        let start = out.len();
        out.resize(start + (want - produced) as usize * ch, 0.0);
        produced = want;
        st.process(&buf, &mut out[start..]);
        if !progress(fed as f32 / n.max(1) as f32) {
            return None;
        }
    }
    let start = out.len();
    out.resize(start + out_lat * ch, 0.0);
    st.flush(&mut out[start..]);
    out.drain(..(out_lat * ch).min(out.len()));
    out.resize(out_n * ch, 0.0);
    Some(Audio {
        channels: ch,
        samples: out,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pitch::live::{LiveDetector, LiveFrame};
    use crate::pitch::note::cents_between;

    const RATE: u32 = 48_000;

    fn track(mono: &[f32]) -> Vec<LiveFrame> {
        let mut d = LiveDetector::new(RATE);
        let mut out = Vec::new();
        for (i, c) in mono.chunks(256).enumerate() {
            d.push(c, (i * 256) as i64, &mut out);
        }
        out
    }

    #[test]
    fn clicks_land_where_the_slowed_clock_expects_them() {
        let mut clicks = vec![0.0f32; RATE as usize * 4];
        for k in 1..7 {
            clicks[k * RATE as usize / 2] = 1.0;
        }
        let input = Audio::mono(clicks);
        for speed in [Ratio::new(1, 2), Ratio::new(3, 5), Ratio::new(3, 4)] {
            let out = render(&input, RATE, speed, |_| true).unwrap();
            assert_eq!(out.frames() as i64, speed.to_wall(input.frames() as i64));
            for k in 1..7 {
                let expect = speed.to_wall((k * RATE as usize / 2) as i64);
                let w = RATE as i64 / 20;
                let peak = (expect - w..expect + w)
                    .max_by(|&a, &b| {
                        out.samples[a as usize]
                            .abs()
                            .total_cmp(&out.samples[b as usize].abs())
                    })
                    .unwrap();
                // Within 1 ms.
                assert!(
                    (peak - expect).abs() <= 48,
                    "{speed:?} click {k}: {}",
                    peak - expect
                );
            }
        }
    }

    #[test]
    fn sustained_tone_keeps_its_pitch() {
        let tone: Vec<f32> = (0..RATE as usize * 2)
            .map(|i| 0.5 * (std::f32::consts::TAU * 220.0 * i as f32 / RATE as f32).sin())
            .collect();
        let out = render(&Audio::mono(tone), RATE, Ratio::new(1, 2), |_| true).unwrap();
        let frames = track(&out.samples);
        let mid = &frames[frames.len() / 4..frames.len() * 3 / 4];
        assert!(mid.iter().all(|f| f.hz.is_some()));
        for f in mid {
            assert!(
                cents_between(f.hz.unwrap(), 220.0).abs() < 5.0,
                "{:?}",
                f.hz
            );
        }
    }

    #[test]
    fn stereo_and_cancel() {
        let input = Audio {
            channels: 2,
            samples: vec![0.1; 2 * 20_000],
        };
        let out = render(&input, RATE, Ratio::new(3, 4), |_| true).unwrap();
        assert_eq!(out.channels, 2);
        assert_eq!(out.frames(), 20_000 * 4 / 3);
        assert!(render(&input, RATE, Ratio::new(3, 4), |_| false).is_none());
    }
}
