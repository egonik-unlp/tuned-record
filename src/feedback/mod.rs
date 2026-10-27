//! Feedback against the guide: live cents deviation and the per-phrase shape score.

use serde::{Deserialize, Serialize};

use crate::pitch::contour::Contour;
use crate::pitch::note::cents_between;

pub const IN_TUNE_CENTS: f32 = 20.0;
pub const NEAR_CENTS: f32 = 50.0;
/// Unvoiced gaps at least this long split the reference into phrases.
pub const PHRASE_GAP_SECS: f32 = 0.25;
/// Voiced runs shorter than this are merged into a neighbouring phrase.
pub const MIN_PHRASE_SECS: f32 = 0.15;
/// A phrase is scored only if the singer was voiced on at least this share of its voiced reference.
pub const MIN_COVERAGE: f32 = 0.5;

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

    #[test]
    fn too_little_singing_has_no_score() {
        let r = contour(0, &[(200, Some(60.0))]);
        let sung = contour(0, &[(80, Some(64.0)), (120, None)]);
        let p = phrases(&r, RATE)[0];
        assert_eq!(shape_score(&r, &sung, p), None);
    }
}
