//! Segments a contour file into target notes and prints how many there are and how long they
//! last, to tune the segmentation constants against a real guide.
//! Run with: cargo run --release --example note_report -- <contour.f0> [sample rate, default 48000]
use std::path::PathBuf;

use tuned_record::feedback::notes;
use tuned_record::pitch::contour::Contour;
use tuned_record::pitch::note::note_label;

fn main() -> anyhow::Result<()> {
    let mut args = std::env::args().skip(1);
    let path: PathBuf = args
        .next()
        .ok_or_else(|| anyhow::anyhow!("usage: note_report <contour.f0> [rate]"))?
        .into();
    let rate: u32 = args.next().map_or(Ok(48_000), |r| r.parse())?;
    let c = Contour::load(&path)?;
    let voiced = c.hz.iter().filter(|&&h| h > 0.0).count();
    let ms = |frames: i64| frames as f32 * 1000.0 / rate as f32;
    println!(
        "{}: {:.1} s, hop {} ({:.1} ms), voiced {:.0} %",
        path.display(),
        ms(c.end_frame() - c.start_frame) / 1000.0,
        c.hop,
        ms(c.hop as i64),
        100.0 * voiced as f32 / c.len().max(1) as f32
    );

    let n = notes(&c, rate);
    if n.is_empty() {
        println!("no notes");
        return Ok(());
    }
    let mut lens: Vec<f32> = n.iter().map(|x| ms(x.end - x.start)).collect();
    lens.sort_by(|a, b| a.total_cmp(b));
    let pct = |p: usize| lens[(lens.len() - 1) * p / 100];
    let covered: f32 = lens.iter().sum::<f32>() / ms(voiced as i64 * c.hop as i64);
    println!(
        "{} notes ({:.1} per voiced second), {:.0} % of voiced time inside a note",
        n.len(),
        n.len() as f32 / (ms(voiced as i64 * c.hop as i64) / 1000.0),
        100.0 * covered
    );
    println!(
        "duration ms: p10 {:.0} · p25 {:.0} · median {:.0} · p75 {:.0} · p90 {:.0}",
        pct(10),
        pct(25),
        pct(50),
        pct(75),
        pct(90)
    );
    let under = |t: f32| 100.0 * lens.iter().filter(|&&l| l < t).count() as f32 / lens.len() as f32;
    println!(
        "shorter than 100 ms: {:.0} %, than 60 ms: {:.0} %",
        under(100.0),
        under(60.0)
    );
    let steps: Vec<f32> = n
        .windows(2)
        .filter(|w| w[1].start - w[0].end < rate as i64 / 4)
        .map(|w| (w[1].midi - w[0].midi).abs())
        .collect();
    if !steps.is_empty() {
        let small = steps.iter().filter(|&&s| s < 0.5).count();
        println!(
            "adjacent notes less than half a semitone apart: {:.0} % (likely over-split)",
            100.0 * small as f32 / steps.len() as f32
        );
    }
    println!("first notes:");
    for x in n.iter().take(12) {
        println!(
            "  {:>8.0} ms  {:>4.0} ms  {} {:+.0} ¢",
            ms(x.start),
            ms(x.end - x.start),
            note_label(x.midi.round() as i32),
            (x.midi - x.midi.round()) * 100.0
        );
    }
    Ok(())
}
