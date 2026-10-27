//! Offline export: one mono WAV per take and a stereo mixdown, all starting at song t=0 with the
//! same length and rate, so they line up when dropped at the start of a DAW timeline.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;

use super::TakeEntry;
use super::audio_io::write_wav;
use crate::audio::mixer::{Audio, Playback, Track, db_to_gain, take_audible};

pub struct ExportInput {
    pub name: String,
    pub rate: u32,
    pub backing: Option<Arc<Audio>>,
    /// Length of the vocal stem, which counts towards the song length but is not exported.
    pub stem_frames: usize,
    pub takes: Vec<(TakeEntry, Arc<Audio>)>,
}

#[derive(Debug)]
pub struct ExportReport {
    pub files: Vec<PathBuf>,
    pub frames: usize,
    /// Mixdown peak (linear); above 1.0 means the float file holds levels past full scale.
    pub peak: f32,
}

impl ExportInput {
    pub fn song_frames(&self) -> usize {
        let takes_end = self
            .takes
            .iter()
            .map(|(t, a)| (t.start_frame + a.frames() as i64).max(0) as usize);
        self.backing
            .as_ref()
            .map_or(0, |b| b.frames())
            .max(self.stem_frames)
            .max(takes_end.max().unwrap_or(0))
    }

    /// Backing plus the takes audible under mute/solo, at their volumes; no guide.
    pub fn mixdown_playback(&self) -> Playback {
        let any_solo = self.takes.iter().any(|(t, _)| t.soloed);
        let mut tracks: Vec<Track> = self
            .backing
            .iter()
            .map(|b| Track {
                audio: b.clone(),
                start: 0,
                gain: 1.0,
            })
            .collect();
        for (t, a) in &self.takes {
            if take_audible(t.muted, t.soloed, any_solo) {
                tracks.push(Track {
                    audio: a.clone(),
                    start: t.start_frame,
                    gain: db_to_gain(t.volume_db),
                });
            }
        }
        Playback { tracks }
    }
}

pub fn export(
    input: &ExportInput,
    dir: &Path,
    mut progress: impl FnMut(f32),
) -> Result<ExportReport> {
    std::fs::create_dir_all(dir)?;
    let len = input.song_frames();
    let mut files = Vec::new();
    let steps = input.takes.len() + 1;

    for (k, (t, a)) in input.takes.iter().enumerate() {
        // Mono, padded from t=0; any part recorded before t=0 is cropped.
        let mut out = vec![0.0f32; len];
        let src = a.to_mono();
        for (i, &s) in src.iter().enumerate() {
            let f = t.start_frame + i as i64;
            if f >= 0 && (f as usize) < len {
                out[f as usize] = s;
            }
        }
        let path = dir.join(format!("{}-take-{:03}.wav", input.name, t.id));
        write_wav(&path, &Audio::mono(out), input.rate)?;
        files.push(path);
        progress((k + 1) as f32 / steps as f32);
    }

    let pb = input.mixdown_playback();
    let mut samples = Vec::with_capacity(len * 2);
    let (mut l, mut r) = (vec![0.0f32; 4096], vec![0.0f32; 4096]);
    let mut peak = 0.0f32;
    let mut pos = 0usize;
    while pos < len {
        let n = 4096.min(len - pos);
        l[..n].fill(0.0);
        r[..n].fill(0.0);
        pb.mix_into(pos as i64, &mut l[..n], &mut r[..n]);
        for i in 0..n {
            peak = peak.max(l[i].abs()).max(r[i].abs());
            samples.push(l[i]);
            samples.push(r[i]);
        }
        pos += n;
    }
    let path = dir.join(format!("{}-mixdown.wav", input.name));
    write_wav(
        &path,
        &Audio {
            channels: 2,
            samples,
        },
        input.rate,
    )?;
    files.push(path);
    progress(1.0);
    Ok(ExportReport {
        files,
        frames: len,
        peak,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::audio_io::read_wav;

    #[test]
    fn aligned_takes_and_mixdown() {
        let rate = 1000;
        let backing = Arc::new(Audio {
            channels: 2,
            samples: vec![0.25; 180 * 2],
        }); // "3:00" = 180 frames
        let take = |id, start, muted| {
            (
                TakeEntry {
                    muted,
                    ..TakeEntry::new(id, start)
                },
                Arc::new(Audio::mono(vec![0.5; 40])),
            )
        };
        let input = ExportInput {
            name: "song".into(),
            rate,
            backing: Some(backing),
            stem_frames: 170,
            takes: vec![take(1, 30, false), take(2, 30, true), take(3, 100, false)],
        };
        let dir = std::env::temp_dir().join(format!("tr-export-{}", std::process::id()));
        let rep = export(&input, &dir, |_| {}).unwrap();
        assert_eq!(
            rep.files.len(),
            4,
            "muted takes are still exported individually"
        );

        let (t1, r1) = read_wav(&dir.join("song-take-001.wav")).unwrap();
        assert_eq!((r1, t1.frames()), (rate, 180));
        assert_eq!(t1.samples[29], 0.0);
        assert_eq!(t1.samples[30], 0.5);
        assert_eq!(t1.samples[70], 0.0);

        let (mix, rm) = read_wav(&dir.join("song-mixdown.wav")).unwrap();
        assert_eq!((rm, mix.channels, mix.frames()), (rate, 2, 180));
        // Frame 40: backing + take 1 (take 2 muted). Frame 110: backing + take 3.
        assert_eq!(mix.samples[40 * 2], 0.75);
        assert_eq!(mix.samples[110 * 2], 0.75);
        assert_eq!(mix.samples[90 * 2], 0.25);
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn loud_mix_is_not_clipped() {
        let input = ExportInput {
            name: "loud".into(),
            rate: 1000,
            backing: Some(Arc::new(Audio::mono(vec![0.9; 10]))),
            stem_frames: 0,
            takes: vec![(TakeEntry::new(1, 0), Arc::new(Audio::mono(vec![0.9; 10])))],
        };
        let dir = std::env::temp_dir().join(format!("tr-export-loud-{}", std::process::id()));
        let rep = export(&input, &dir, |_| {}).unwrap();
        assert!((rep.peak - 1.8).abs() < 1e-5);
        let (mix, _) = read_wav(&dir.join("loud-mixdown.wav")).unwrap();
        assert!(
            (mix.samples[0] - 1.8).abs() < 1e-5,
            "float output keeps the summed signal"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
