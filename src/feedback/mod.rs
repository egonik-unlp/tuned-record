//! Feedback against the guide: live cents deviation and the per-phrase shape score.

use serde::{Deserialize, Serialize};

use crate::pitch::contour::Contour;
use crate::pitch::live::{BRIDGE_GAP_SECS, BRIDGE_SEMITONES};
use crate::pitch::note::{cents_between, hz_to_midi};

pub const IN_TUNE_CENTS: f32 = 20.0;
pub const NEAR_CENTS: f32 = 50.0;
/// Unvoiced gaps at least this long split the reference into phrases.
pub const PHRASE_GAP_SECS: f32 = 0.25;
/// Voiced runs shorter than this are merged into a neighbouring phrase.
pub const MIN_PHRASE_SECS: f32 = 0.15;
/// A phrase is scored only if the singer was voiced on at least this share of its voiced reference.
pub const MIN_COVERAGE: f32 = 0.5;
/// Note segmentation: the target is median-smoothed over this long first...
pub const NOTE_SMOOTH_SECS: f32 = 0.05;
/// ...a note holds while the pitch stays within this of its running median (semitones)...
pub const NOTE_BAND_ST: f32 = 0.6;
/// ...and ends when the pitch leaves that band for at least this long (shorter is wobble).
pub const NOTE_DEPARTURE_SECS: f32 = 0.08;
/// Segments shorter than this inside a voiced run are glides, not notes.
pub const MIN_NOTE_SECS: f32 = 0.08;
/// Neighbouring segments closer than this (semitones) are one note that drifted.
const NOTE_MERGE_ST: f32 = 0.5;
/// A note's pitch is the median of this central share of its frames (no attack or release).
const NOTE_CORE: f32 = 0.6;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Band {
    InTune,
    Near,
    Off,
}

impl Band {
    pub fn of(cents: f32) -> Self {
        let c = cents.abs();
        if c <= IN_TUNE_CENTS {
            Band::InTune
        } else if c <= NEAR_CENTS {
            Band::Near
        } else {
            Band::Off
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Band::InTune => "in tune",
            Band::Near => "near",
            Band::Off => "off",
        }
    }
}

/// Cents from the shifted guide to the singer: `offset_cents` is the guide transposition.
pub fn deviation_cents(live_hz: f32, reference_hz: f32, offset_cents: f32) -> f32 {
    cents_between(live_hz, reference_hz) - offset_cents
}

/// A phrase of the reference, as song frames `[start, end)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Phrase {
    pub start: i64,
    pub end: i64,
}

/// Splits the reference into phrases at unvoiced gaps of at least [`PHRASE_GAP_SECS`].
pub fn phrases(reference: &Contour, rate: u32) -> Vec<Phrase> {
    let hop = reference.hop as i64;
    if hop == 0 {
        return Vec::new();
    }
    let gap = (PHRASE_GAP_SECS * rate as f32) as i64;
    let min_len = (MIN_PHRASE_SECS * rate as f32) as i64;

    // Voiced runs as frame spans.
    let mut runs: Vec<Phrase> = Vec::new();
    let mut start: Option<i64> = None;
    for i in 0..=reference.len() {
        let voiced = reference.voiced(i).is_some();
        match (voiced, start) {
            (true, None) => start = Some(reference.frame_of(i)),
            (false, Some(s)) => {
                runs.push(Phrase {
                    start: s,
                    end: reference.frame_of(i),
                });
                start = None;
            }
            _ => {}
        }
    }

    // Join runs separated by short gaps.
    let mut joined: Vec<Phrase> = Vec::new();
    for r in runs {
        match joined.last_mut() {
            Some(p) if r.start - p.end < gap => p.end = r.end,
            _ => joined.push(r),
        }
    }

    // Fold very short phrases into the previous one (or the next, for the first).
    let mut out: Vec<Phrase> = Vec::new();
    let mut carry: Option<i64> = None;
    for mut p in joined {
        if let Some(s) = carry.take() {
            p.start = s;
        }
        if p.end - p.start < min_len {
            match out.last_mut() {
                Some(prev) => prev.end = p.end,
                None => carry = Some(p.start),
            }
        } else {
            out.push(p);
        }
    }
    out
}

/// Shape score in cents: RMS of the deviation minus its median, over frames where both the
/// reference and the singer are voiced. Transposition-invariant by construction. `None` when the
/// singer covered less than [`MIN_COVERAGE`] of the phrase's voiced reference.
pub fn shape_score(reference: &Contour, sung: &Contour, phrase: Phrase) -> Option<f32> {
    let mut d = Vec::new();
    let mut ref_voiced = 0usize;
    for i in reference.index_range(phrase.start, phrase.end) {
        let Some(r) = reference.voiced(i) else {
            continue;
        };
        ref_voiced += 1;
        if let Some(s) = sung.hz_at(reference.frame_of(i)) {
            d.push(cents_between(s, r));
        }
    }
    if ref_voiced == 0 || (d.len() as f32) < MIN_COVERAGE * ref_voiced as f32 {
        return None;
    }
    let mut sorted = d.clone();
    sorted.sort_by(|a, b| a.total_cmp(b));
    let median = sorted[sorted.len() / 2];
    let ms = d.iter().map(|v| (v - median).powi(2)).sum::<f32>() / d.len() as f32;
    Some(ms.sqrt())
}

/// A steady note of the target, as song frames `[start, end)`, at a fractional MIDI pitch (not
/// snapped to the 12-TET grid, and without the guide offset).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Note {
    pub start: i64,
    pub end: i64,
    pub midi: f32,
}

impl Note {
    pub fn contains(&self, frame: i64) -> bool {
        (self.start..self.end).contains(&frame)
    }
}

/// Segments a target contour into steady notes, ignoring vibrato, wobble and the glides between
/// notes (see design.md → Note targets).
pub fn notes(target: &Contour, rate: u32) -> Vec<Note> {
    let hop = target.hop as f32;
    if hop == 0.0 {
        return Vec::new();
    }
    let frames_of = |secs: f32| ((secs * rate as f32 / hop).round() as usize).max(1);
    let smooth = frames_of(NOTE_SMOOTH_SECS) | 1;
    let departure = frames_of(NOTE_DEPARTURE_SECS);
    let min_len = frames_of(MIN_NOTE_SECS);

    let mut out = Vec::new();
    for (start, run) in voiced_runs(target, frames_of(BRIDGE_GAP_SECS)) {
        let mut segments: Vec<(usize, usize, f32)> =
            split_run(&median_smooth(&run, smooth), departure)
                .into_iter()
                .map(|(a, b)| (a, b, core_median(&run[a..b])))
                .collect();
        // Drift: neighbours that end up close are the same note.
        let mut k = 1;
        while k < segments.len() {
            let (a, _, m0) = segments[k - 1];
            let (_, b, m1) = segments[k];
            if (m1 - m0).abs() < NOTE_MERGE_ST {
                segments[k - 1] = (a, b, core_median(&run[a..b]));
                segments.remove(k);
            } else {
                k += 1;
            }
        }
        let keep_all = segments.len() == 1;
        for (a, b, midi) in segments {
            if keep_all || b - a >= min_len {
                out.push(Note {
                    start: target.frame_of(start + a),
                    end: target.frame_of(start + b),
                    midi,
                });
            }
        }
    }
    out
}

/// Voiced runs of a contour as (first index, MIDI values). Unvoiced gaps of at most `bridge`
/// frames between pitches less than [`BRIDGE_SEMITONES`] apart are bridged (filled linearly), so
/// a dropout in the analysis does not split a note.
fn voiced_runs(c: &Contour, bridge: usize) -> Vec<(usize, Vec<f32>)> {
    let mut runs: Vec<(usize, Vec<f32>)> = Vec::new();
    for i in 0..c.len() {
        let Some(hz) = c.voiced(i) else { continue };
        let m = hz_to_midi(hz);
        let continues = |start: usize, v: &[f32]| {
            let gap = i - (start + v.len());
            gap == 0 || (gap <= bridge && (m - v[v.len() - 1]).abs() < BRIDGE_SEMITONES)
        };
        match runs.last_mut() {
            Some((start, v)) if continues(*start, v) => {
                let (last, gap) = (v[v.len() - 1], i - (*start + v.len()));
                for k in 1..=gap {
                    v.push(last + (m - last) * k as f32 / (gap + 1) as f32);
                }
                v.push(m);
            }
            _ => runs.push((i, vec![m])),
        }
    }
    runs
}

/// Running median over an odd window, shrinking at the edges.
fn median_smooth(v: &[f32], window: usize) -> Vec<f32> {
    let half = window / 2;
    let mut buf = Vec::with_capacity(window);
    (0..v.len())
        .map(|i| {
            buf.clear();
            buf.extend_from_slice(&v[i.saturating_sub(half)..(i + half + 1).min(v.len())]);
            median(&mut buf)
        })
        .collect()
}

fn median(v: &mut [f32]) -> f32 {
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

/// Splits one voiced run (smoothed MIDI) into segments `[a, b)`: a segment ends when the pitch
/// stays out of [`NOTE_BAND_ST`] of its running median for `departure` frames; the departed
/// frames start the next segment. Shorter excursions are absorbed (wobble).
fn split_run(v: &[f32], departure: usize) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut seg_start = 0;
    let mut members: Vec<f32> = Vec::new();
    let mut away: Option<usize> = None;
    for (i, &m) in v.iter().enumerate() {
        let centre = if members.is_empty() {
            m
        } else {
            median(&mut members.clone())
        };
        if (m - centre).abs() <= NOTE_BAND_ST {
            if let Some(a) = away.take() {
                members.extend_from_slice(&v[a..i]);
            }
            members.push(m);
        } else {
            let a = *away.get_or_insert(i);
            if i + 1 - a >= departure {
                out.push((seg_start, a));
                seg_start = a;
                members.clear();
                members.extend_from_slice(&v[a..=i]);
                away = None;
            }
        }
    }
    out.push((seg_start, v.len()));
    out.retain(|(a, b)| b > a);
    out
}

/// Median of the central [`NOTE_CORE`] share of a note's frames.
fn core_median(v: &[f32]) -> f32 {
    let trim = ((v.len() as f32 * (1.0 - NOTE_CORE) / 2.0) as usize).min((v.len() - 1) / 2);
    let mut core = v[trim..v.len() - trim].to_vec();
    median(&mut core)
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct PhraseScore {
    pub phrase: Phrase,
    pub cents: f32,
}

/// Scores every phrase the sung contour overlaps.
pub fn score_all(reference: &Contour, sung: &Contour, phrases: &[Phrase]) -> Vec<PhraseScore> {
    phrases
        .iter()
        .filter(|p| p.end > sung.start_frame && p.start < sung.end_frame())
        .filter_map(|&p| {
            shape_score(reference, sung, p).map(|cents| PhraseScore { phrase: p, cents })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pitch::note::midi_to_hz;

    const RATE: u32 = 48_000;
    const HOP: u32 = 480; // 10 ms

    /// Builds a contour from (duration in hops, Some(midi) / None) segments.
    fn contour(start: i64, segs: &[(usize, Option<f32>)]) -> Contour {
        let mut c = Contour::new(start, HOP);
        for &(n, m) in segs {
            for k in 0..n {
                // A little vibrato-like movement so the shape isn't flat.
                c.push(m.map(|m| midi_to_hz(m + 0.3 * (k as f32 / 5.0).sin())), 1.0);
            }
        }
        c
    }

    fn shifted(c: &Contour, semitones: f32) -> Contour {
        let mut s = c.clone();
        let f = 2f32.powf(semitones / 12.0);
        s.hz.iter_mut().filter(|h| **h > 0.0).for_each(|h| *h *= f);
        s
    }

    #[test]
    fn bands() {
        assert_eq!(Band::of(-15.0), Band::InTune);
        assert_eq!(Band::of(35.0), Band::Near);
        assert_eq!(Band::of(-80.0), Band::Off);
    }

    #[test]
    fn deviation_accounts_for_offset() {
        // Reference A3, offset +4 st → target C#4; singing C#4 is on target.
        let d = deviation_cents(midi_to_hz(61.0), midi_to_hz(57.0), 400.0);
        assert!(d.abs() < 0.01, "{d}");
    }

    #[test]
    fn two_phrases_split_by_breath() {
        // 10.0–13.5 s voiced, 400 ms gap, 13.9–17.0 s voiced.
        let c = contour(
            0,
            &[
                (1000, None),
                (350, Some(60.0)),
                (40, None),
                (310, Some(62.0)),
                (100, None),
            ],
        );
        let p = phrases(&c, RATE);
        assert_eq!(p.len(), 2);
        assert_eq!(p[0].start, 1000 * HOP as i64);
        assert_eq!(p[1].start, 1390 * HOP as i64);
    }

    #[test]
    fn short_gap_does_not_split() {
        let c = contour(0, &[(100, Some(60.0)), (10, None), (100, Some(62.0))]);
        assert_eq!(phrases(&c, RATE).len(), 1);
    }

    #[test]
    fn exact_follow_at_offset_scores_zero() {
        let r = contour(0, &[(200, Some(60.0))]);
        let p = phrases(&r, RATE)[0];
        let s = shape_score(&r, &shifted(&r, 7.0), p).unwrap();
        assert!(s < 0.01, "{s}");
    }

    #[test]
    fn score_is_transposition_invariant() {
        let r = contour(0, &[(200, Some(60.0))]);
        // A sung contour with its own shape errors.
        let mut sung = r.clone();
        for (i, h) in sung.hz.iter_mut().enumerate() {
            *h *= 2f32.powf(((i % 17) as f32 * 3.0) / 1200.0);
        }
        let p = phrases(&r, RATE)[0];
        let up = shape_score(&r, &shifted(&sung, 3.0), p).unwrap();
        let down = shape_score(&r, &shifted(&sung, -5.0), p).unwrap();
        assert!(up > 1.0);
        assert!((up - down).abs() < 0.05, "{up} vs {down}");
    }

    /// MIDI pitch (or unvoiced) at a fraction 0..1 through a segment.
    type Shape<'a> = &'a dyn Fn(f32) -> Option<f32>;

    /// A contour at the reference's hop (256 frames) from (seconds, pitch shape) segments.
    fn target(segs: &[(f32, Shape)]) -> Contour {
        let hop = 256;
        let mut c = Contour::new(0, hop);
        for (secs, f) in segs {
            let n = (secs * RATE as f32 / hop as f32).round() as usize;
            for k in 0..n {
                c.push(f(k as f32 / n as f32).map(midi_to_hz), 0.9);
            }
        }
        c
    }

    fn ms(frames: i64) -> f32 {
        frames as f32 * 1000.0 / RATE as f32
    }

    #[test]
    fn wobbly_held_note_is_one_note() {
        // A3 wobbling ±30 ¢ at 5.5 Hz for 400 ms.
        let c = target(&[
            (0.1, &|_| None),
            (0.4, &|t| {
                Some(57.0 + 0.3 * (t * 0.4 * 5.5 * std::f32::consts::TAU).sin())
            }),
            (0.1, &|_| None),
        ]);
        let n = notes(&c, RATE);
        assert_eq!(n.len(), 1, "{n:?}");
        assert!((ms(n[0].end - n[0].start) - 400.0).abs() < 15.0, "{n:?}");
        assert!((n[0].midi - 57.0).abs() < 0.1, "{n:?}");
    }

    #[test]
    fn step_with_glide_is_two_notes() {
        let c = target(&[
            (0.3, &|_| Some(57.0)),
            (0.06, &|t| Some(57.0 + 2.0 * t)),
            (0.3, &|_| Some(59.0)),
        ]);
        let n = notes(&c, RATE);
        assert_eq!(n.len(), 2, "{n:?}");
        assert!(
            (n[0].midi - 57.0).abs() < 0.1 && (n[1].midi - 59.0).abs() < 0.1,
            "{n:?}"
        );
        // Each note keeps most of its hold; nothing in between counts as a note.
        assert!(ms(n[0].end - n[0].start) > 280.0, "{n:?}");
        assert!(ms(n[1].end - n[1].start) > 260.0, "{n:?}");
    }

    #[test]
    fn analysis_dropout_does_not_split_a_note() {
        let c = target(&[
            (0.2, &|_| Some(64.0)),
            (0.03, &|_| None),
            (0.2, &|_| Some(64.1)),
        ]);
        let n = notes(&c, RATE);
        assert_eq!(n.len(), 1, "{n:?}");
    }

    #[test]
    fn slow_drift_stays_one_note() {
        // Creeps up 0.8 st over 600 ms, as a held note going sharp.
        let c = target(&[(0.6, &|t| Some(64.0 + 0.8 * t))]);
        let n = notes(&c, RATE);
        assert_eq!(n.len(), 1, "{n:?}");
    }

    #[test]
    fn short_isolated_run_is_one_note() {
        let c = target(&[(0.1, &|_| None), (0.03, &|_| Some(64.0)), (0.1, &|_| None)]);
        let n = notes(&c, RATE);
        assert_eq!(n.len(), 1, "{n:?}");
        assert!((n[0].midi - 64.0).abs() < 0.01);
    }

    #[test]
    fn notes_are_not_snapped_to_the_grid() {
        // A record tuned 30 ¢ flat.
        let c = target(&[(0.3, &|_| Some(56.7))]);
        let n = notes(&c, RATE);
        assert!((n[0].midi - 56.7).abs() < 0.01, "{n:?}");
    }

    #[test]
    fn too_little_singing_has_no_score() {
        let r = contour(0, &[(200, Some(60.0))]);
        let sung = contour(0, &[(80, Some(64.0)), (120, None)]);
        let p = phrases(&r, RATE)[0];
        assert_eq!(shape_score(&r, &sung, p), None);
    }
}
