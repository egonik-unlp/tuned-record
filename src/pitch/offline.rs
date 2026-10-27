//! Offline pitch analysis for the reference stem and finished takes (worker thread).
//!
//! pYIN at quarter-semitone resolution gives robust voicing and the pitch path; each voiced frame
//! is then refined with MPM restricted to ±10 % around it, since pYIN's output is quantised to its
//! bins. The signal is split into chunks (with context on both sides) analysed in parallel.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

use pyin::{Framing, PYINExecutor, PadMode};

use super::contour::Contour;
use super::live::{MAX_HZ, MIN_HZ};
use super::mpm::Mpm;

/// Bump when analysis parameters change, so cached contours are recomputed.
pub const ANALYZER_VERSION: u32 = 2;
pub const FRAME: usize = 2048;
pub const HOP: usize = 256;
/// pYIN bin width in semitones; finer is much slower (Viterbi cost grows with the bin count).
const RESOLUTION: f64 = 0.25;
const REFINE_RANGE: f32 = 1.1;
const CHUNK_SECS: usize = 30;
const CONTEXT_HOPS: usize = 64;

/// Analyses `samples` (mono, at `rate`) whose first sample is song frame `start_frame`.
/// `progress` receives values in 0..=1.
pub fn analyse(
    samples: &[f32],
    rate: u32,
    start_frame: i64,
    mut progress: impl FnMut(f32),
) -> Contour {
    let mut contour = Contour::new(start_frame, HOP as u32);
    contour.analyzer_version = ANALYZER_VERSION;
    let n_frames = samples.len().div_ceil(HOP);
    if samples.len() < FRAME {
        contour.hz = vec![0.0; n_frames];
        contour.prob = vec![0.0; n_frames];
        progress(1.0);
        return contour;
    }

    let chunk_frames = (CHUNK_SECS * rate as usize / HOP).max(1);
    let chunks: Vec<(usize, usize)> = (0..n_frames)
        .step_by(chunk_frames)
        .map(|first| (first, (first + chunk_frames).min(n_frames)))
        .collect();
    let next = AtomicUsize::new(0);
    let (tx, rx) = mpsc::channel();
    let workers = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .min(chunks.len());

    let mut results: Vec<Option<Vec<(f32, f32)>>> = vec![None; chunks.len()];
    std::thread::scope(|s| {
        for _ in 0..workers {
            let tx = tx.clone();
            let (next, chunks) = (&next, &chunks);
            s.spawn(move || {
                let mut exec = PYINExecutor::<f64>::new(
                    MIN_HZ as f64,
                    MAX_HZ as f64,
                    rate,
                    FRAME,
                    None,
                    Some(HOP),
                    Some(RESOLUTION),
                );
                let mut mpm = Mpm::new(FRAME);
                loop {
                    let i = next.fetch_add(1, Ordering::Relaxed);
                    let Some(&(first, last)) = chunks.get(i) else {
                        break;
                    };
                    let r = analyse_chunk(&mut exec, &mut mpm, samples, rate, first, last);
                    if tx.send((i, r)).is_err() {
                        break;
                    }
                }
            });
        }
        drop(tx);
        let mut done = 0;
        for (i, r) in rx {
            results[i] = Some(r);
            done += chunks[i].1 - chunks[i].0;
            progress(done as f32 / n_frames as f32);
        }
    });

    for (hz, prob) in results.into_iter().flatten().flatten() {
        contour.push((hz > 0.0).then_some(hz), prob);
    }
    contour
}

/// Frames `[first, last)` as `(hz or 0, voiced probability)`.
fn analyse_chunk(
    exec: &mut PYINExecutor<f64>,
    mpm: &mut Mpm,
    samples: &[f32],
    rate: u32,
    first: usize,
    last: usize,
) -> Vec<(f32, f32)> {
    let context = CONTEXT_HOPS * HOP;
    let a = (first * HOP).saturating_sub(context);
    let b = (last * HOP + context).min(samples.len());
    let mut chunk: Vec<f64> = samples[a..b].iter().map(|&s| s as f64).collect();
    if chunk.len() < FRAME {
        chunk.resize(FRAME, 0.0);
    }
    let (_, f0, voiced, prob) =
        exec.pyin(&chunk, f64::NAN, Framing::Center(PadMode::Constant(0.0)));
    let offset = (first * HOP - a) / HOP;
    let mut window = vec![0.0f32; FRAME];
    (0..last - first)
        .map(|i| {
            let k = offset + i;
            let p = prob.get(k).copied().unwrap_or(0.0) as f32;
            let coarse = f0
                .get(k)
                .copied()
                .filter(|v| v.is_finite() && voiced.get(k) == Some(&true));
            let Some(coarse) = coarse.map(|v| v as f32) else {
                return (0.0, p);
            };
            // Frame centred on its sample position, zero-padded at the edges.
            let centre = (first + i) * HOP;
            window.fill(0.0);
            let start = centre as i64 - FRAME as i64 / 2;
            for (j, w) in window.iter_mut().enumerate() {
                let s = start + j as i64;
                if s >= 0 && (s as usize) < samples.len() {
                    *w = samples[s as usize];
                }
            }
            let fine = mpm
                .detect(
                    &window,
                    rate as f32,
                    coarse / REFINE_RANGE,
                    coarse * REFINE_RANGE,
                    0.0,
                )
                .map(|(hz, _)| hz);
            (fine.unwrap_or(coarse), p)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pitch::note::cents_between;

    #[test]
    fn sine_then_silence() {
        let rate = 16_000;
        let mut s: Vec<f32> = (0..rate)
            .map(|i| 0.5 * (2.0 * std::f32::consts::PI * 220.0 * i as f32 / rate as f32).sin())
            .collect();
        s.extend(std::iter::repeat_n(0.0, rate as usize));
        let mut last = 0.0;
        let c = analyse(&s, rate, 0, |p| last = p);
        assert_eq!(last, 1.0);
        assert_eq!(c.len(), s.len().div_ceil(HOP));
        // Middle of the tone is voiced at 220 Hz (refined past pYIN's 25 ¢ bins); silence is unvoiced.
        let hz = c.hz_at(rate as i64 / 2).expect("voiced");
        assert!(cents_between(hz, 220.0).abs() < 3.0, "{hz}");
        assert_eq!(c.hz_at(rate as i64 * 3 / 2), None);
    }

    #[test]
    fn parallel_chunks_line_up() {
        // 70 s → 3 chunks; a pitch step at 45 s must land at the same song frame.
        let rate = 8_000;
        let s: Vec<f32> = (0..rate * 70)
            .map(|i| {
                let hz = if i < rate * 45 { 200.0 } else { 300.0 };
                0.5 * (2.0 * std::f32::consts::PI * hz * i as f32 / rate as f32).sin()
            })
            .collect();
        let c = analyse(&s, rate as u32, 1000, |_| {});
        let at = |secs: f32| c.hz_at(1000 + (secs * rate as f32) as i64).unwrap();
        assert!(cents_between(at(44.5), 200.0).abs() < 3.0);
        assert!(cents_between(at(45.5), 300.0).abs() < 3.0);
        assert!(cents_between(at(31.0), 200.0).abs() < 3.0);
    }
}
