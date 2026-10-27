//! Runs a recorded take through the live detector and the input meter, as if it came from the
//! mic: voiced runs (count, median length), the share of short runs, and the level warnings.
//! Use it to check the voicing hysteresis and the meter thresholds on a real recording.
//! Run with: cargo run --release --example voicing_report -- <take.wav>
use std::path::PathBuf;

use tuned_record::pitch::level::{InputLevel, QuietWatch};
use tuned_record::pitch::live::{HOP, LiveDetector, LiveFrame};
use tuned_record::project::audio_io::read_wav;

/// Same block size the engine runs at by default.
const BLOCK: usize = 256;

fn main() -> anyhow::Result<()> {
    let path: PathBuf = std::env::args()
        .nth(1)
        .ok_or_else(|| anyhow::anyhow!("usage: voicing_report <take.wav>"))?
        .into();
    let (audio, rate) = read_wav(&path)?;
    let mono = audio.to_mono();
    let peak = mono.iter().fold(0f32, |m, v| m.max(v.abs()));
    let clipped = mono.iter().filter(|v| v.abs() >= 0.999).count();
    println!(
        "{}: {:.1} s at {rate} Hz, peak {:.1} dBFS, {:.1} % of samples at or above full scale",
        path.display(),
        mono.len() as f32 / rate as f32,
        20.0 * peak.max(1e-6).log10(),
        100.0 * clipped as f32 / mono.len().max(1) as f32
    );

    let mut det = LiveDetector::new(rate);
    let level = InputLevel::new(rate);
    let mut quiet = QuietWatch::new(rate, HOP);
    let mut frames: Vec<LiveFrame> = Vec::new();
    let mut all: Vec<LiveFrame> = Vec::new();
    let (mut clip_blocks, mut quiet_blocks, mut blocks) = (0, 0, 0);
    for (i, b) in mono.chunks(BLOCK).enumerate() {
        let first = (i * BLOCK) as i64;
        level.update(b, first);
        frames.clear();
        det.push(b, first, &mut frames);
        for f in &frames {
            quiet.push(f.frame, f.level_db, f.clarity);
        }
        all.extend_from_slice(&frames);
        blocks += 1;
        clip_blocks += level.clipping() as usize;
        quiet_blocks += quiet.too_quiet() as usize;
    }

    // Voiced runs, in hops.
    let mut runs: Vec<usize> = Vec::new();
    let mut n = 0;
    for f in &all {
        if f.hz.is_some() {
            n += 1;
        } else if n > 0 {
            runs.push(n);
            n = 0;
        }
    }
    if n > 0 {
        runs.push(n);
    }
    let hop_ms = HOP as f32 * 1000.0 / rate as f32;
    let voiced: usize = runs.iter().sum();
    println!(
        "voiced {:.0} % of {} frames, {} runs",
        100.0 * voiced as f32 / all.len().max(1) as f32,
        all.len(),
        runs.len()
    );
    if !runs.is_empty() {
        let mut sorted = runs.clone();
        sorted.sort_unstable();
        let short = runs.iter().filter(|&&r| (r as f32 * hop_ms) < 20.0).count();
        println!(
            "run length: median {:.0} ms, p90 {:.0} ms; runs under 20 ms: {:.0} %",
            sorted[sorted.len() / 2] as f32 * hop_ms,
            sorted[sorted.len() * 9 / 10] as f32 * hop_ms,
            100.0 * short as f32 / runs.len() as f32
        );
    }
    let mut levels: Vec<f32> = all
        .iter()
        .filter(|f| f.hz.is_some())
        .map(|f| f.level_db)
        .collect();
    levels.sort_by(|a, b| a.total_cmp(b));
    if !levels.is_empty() {
        println!(
            "voiced level: median {:.1} dBFS (p10 {:.1}, p90 {:.1})",
            levels[levels.len() / 2],
            levels[levels.len() / 10],
            levels[levels.len() * 9 / 10]
        );
    }
    println!(
        "warnings (as if playing throughout): clipping {:.0} % of the time, too quiet {:.0} %",
        100.0 * clip_blocks as f32 / blocks.max(1) as f32,
        100.0 * quiet_blocks as f32 / blocks.max(1) as f32
    );
    Ok(())
}
