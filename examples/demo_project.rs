//! Writes a synthetic demo project: a backing pad, a "vocal stem" melody with phrases and
//! vibrato, and one harmony take a major third above. Open it with `tuned-record <dir>`.
//!
//!   cargo run --release --example demo_project -- /path/to/empty/folder

use tuned_record::audio::mixer::Audio;
use tuned_record::pitch::note::midi_to_hz;
use tuned_record::project::audio_io::write_wav;
use tuned_record::project::takes::write_start_sidecar;
use tuned_record::project::{Project, Source, TakeEntry};

const RATE: u32 = 48_000;

/// (start s, duration s, midi) notes of the melody; gaps between phrases.
const MELODY: &[(f32, f32, f32)] = &[
    (1.0, 0.5, 60.0),
    (1.5, 0.5, 62.0),
    (2.0, 0.5, 64.0),
    (2.5, 1.0, 65.0),
    (4.2, 0.4, 67.0),
    (4.6, 0.4, 65.0),
    (5.0, 0.4, 64.0),
    (5.4, 1.2, 62.0),
    (7.3, 0.6, 64.0),
    (7.9, 0.6, 67.0),
    (8.5, 1.5, 69.0),
    (10.6, 0.5, 67.0),
    (11.1, 0.5, 65.0),
    (11.6, 1.6, 64.0),
];

fn voice(len_secs: f32, transpose: f32, detune: impl Fn(f32) -> f32) -> Vec<f32> {
    let n = (len_secs * RATE as f32) as usize;
    let mut out = vec![0.0f32; n];
    let mut phase = 0.0f32;
    for (i, o) in out.iter_mut().enumerate() {
        let t = i as f32 / RATE as f32;
        let Some(&(s, d, m)) = MELODY.iter().find(|(s, d, _)| t >= *s && t < s + d) else {
            continue;
        };
        let vib = if t - s > 0.25 {
            0.25 * (2.0 * std::f32::consts::PI * 5.5 * t).sin()
        } else {
            0.0
        };
        let hz = midi_to_hz(m + transpose + vib + detune(t));
        phase += 2.0 * std::f32::consts::PI * hz / RATE as f32;
        let env = ((t - s) / 0.03).min(1.0) * ((s + d - t) / 0.05).min(1.0);
        *o = 0.3 * env * (phase.sin() + 0.4 * (2.0 * phase).sin() + 0.2 * (3.0 * phase).sin());
    }
    out
}

fn main() {
    let dir = std::path::PathBuf::from(
        std::env::args()
            .nth(1)
            .expect("usage: demo_project <empty folder>"),
    );
    let mut p = Project::create(&dir, RATE).expect("create project");
    let len = 15.0;

    let n = (len * RATE as f32) as usize;
    let pad: Vec<f32> = (0..n)
        .flat_map(|i| {
            let t = i as f32 / RATE as f32;
            let v = 0.05
                * ((2.0 * std::f32::consts::PI * 130.81 * t).sin()
                    + (2.0 * std::f32::consts::PI * 196.0 * t).sin());
            [v, v]
        })
        .collect();
    write_wav(
        &p.path("sources/backing.wav"),
        &Audio {
            channels: 2,
            samples: pad,
        },
        RATE,
    )
    .unwrap();
    let stem = voice(len, 0.0, |_| 0.0);
    write_wav(&p.path("sources/vocals.wav"), &Audio::mono(stem), RATE).unwrap();

    // Harmony take: +4 st, a bit flat in the second phrase, starting at 0.5 s.
    let start = (0.5 * RATE as f32) as i64;
    let take: Vec<f32> = voice(
        len,
        4.0,
        |t| if (4.2..6.6).contains(&t) { -0.3 } else { 0.0 },
    )[start as usize..]
        .to_vec();
    write_wav(&p.take_wav(1), &Audio::mono(take), RATE).unwrap();
    write_start_sidecar(&p.take_file(1, "start"), start).unwrap();

    p.manifest.backing = Some(Source {
        file: "sources/backing.wav".into(),
        original_name: "demo-backing.wav".into(),
    });
    p.manifest.vocals = Some(Source {
        file: "sources/vocals.wav".into(),
        original_name: "demo-vocals.wav".into(),
    });
    p.manifest.takes.push(TakeEntry::new(1, start));
    p.manifest.next_take_id = 2;
    p.manifest.guide.offset_cents = 400;
    p.save().unwrap();
    println!("demo project written to {}", dir.display());
}
