//! The open project as the UI sees it: loaded audio, reference, takes, and the mix built from them.

use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::audio::mixer::{Audio, Playback, Track, db_to_gain, take_audible};
use crate::feedback::{self, Phrase, PhraseScore};
use crate::pitch::contour::Contour;
use crate::pitch::live::HOP as LIVE_HOP;
use crate::project::export::ExportInput;
use crate::project::{Project, TakeEntry, takes};

const SAVE_DEBOUNCE: Duration = Duration::from_millis(300);
/// Pass scores are computed this long after the playhead leaves a phrase (live detector delay).
const SCORE_MARGIN_SECS: f32 = 0.1;

pub struct TakeState {
    pub entry: TakeEntry,
    pub audio: Option<Arc<Audio>>,
    /// Best contour available: pYIN once analysed, the live contour while/after recording.
    pub contour: Contour,
    pub analysed: bool,
    pub scores: Vec<PhraseScore>,
}

impl TakeState {
    pub fn frames(&self) -> i64 {
        self.audio.as_ref().map_or_else(
            || self.contour.end_frame() - self.entry.start_frame,
            |a| a.frames() as i64,
        )
    }
}

pub struct Session {
    pub project: Project,
    /// False when the project's sample rate differs from the audio server's.
    pub rate_ok: bool,
    pub backing: Option<Arc<Audio>>,
    pub vocals: Option<(Arc<Audio>, [u8; 32])>,
    pub reference: Option<Contour>,
    pub phrases: Vec<Phrase>,
    pub takes: Vec<TakeState>,
    dirty_since: Option<Instant>,
}

impl Session {
    pub fn new(project: Project, rate_ok: bool) -> Self {
        Self {
            project,
            rate_ok,
            backing: None,
            vocals: None,
            reference: None,
            phrases: Vec::new(),
            takes: Vec::new(),
            dirty_since: None,
        }
    }

    pub fn rate(&self) -> u32 {
        self.project.manifest.sample_rate
    }

    pub fn set_reference(&mut self, c: Contour) {
        self.phrases = feedback::phrases(&c, self.rate());
        self.reference = Some(c);
        for i in 0..self.takes.len() {
            self.rescore_take(i);
        }
    }

    /// Recomputes and stores a take's phrase scores (pYIN contour only).
    pub fn rescore_take(&mut self, i: usize) {
        let Some(r) = &self.reference else { return };
        let t = &mut self.takes[i];
        if !t.analysed {
            return;
        }
        t.scores = feedback::score_all(r, &t.contour, &self.phrases);
        let _ = takes::save_scores(
            &self.project.take_file(t.entry.id, "scores.json"),
            &t.scores,
        );
    }

    pub fn take_index(&self, id: u32) -> Option<usize> {
        self.takes.iter().position(|t| t.entry.id == id)
    }

    /// Copies a take's UI state into the manifest and schedules a save.
    pub fn sync_take(&mut self, i: usize) {
        let e = self.takes[i].entry.clone();
        if let Some(m) = self.project.take_mut(e.id) {
            *m = e;
        }
        self.mark_dirty();
    }

    pub fn mark_dirty(&mut self) {
        self.dirty_since.get_or_insert_with(Instant::now);
    }

    /// Saves the manifest once changes have settled; returns an error message on failure.
    pub fn save_if_due(&mut self) -> Option<String> {
        if self
            .dirty_since
            .is_some_and(|t| t.elapsed() >= SAVE_DEBOUNCE)
        {
            self.dirty_since = None;
            if let Err(e) = self.project.save() {
                return Some(format!("Could not save the project: {e:#}"));
            }
        }
        None
    }

    pub fn save_now(&mut self) -> Option<String> {
        self.dirty_since = None;
        self.project
            .save()
            .err()
            .map(|e| format!("Could not save the project: {e:#}"))
    }

    pub fn any_solo(&self) -> bool {
        self.takes.iter().any(|t| t.entry.soloed)
    }

    pub fn take_audible(&self, t: &TakeState) -> bool {
        take_audible(t.entry.muted, t.entry.soloed, self.any_solo())
    }

    /// What the engine should play: backing, guide when audible, and the audible takes; or, when
    /// listening to one take (`audition`), the backing and that take only.
    pub fn playback(&self, audition: Option<u32>) -> Playback {
        let mut tracks = Vec::new();
        if let Some(b) = &self.backing {
            tracks.push(Track {
                audio: b.clone(),
                start: 0,
                gain: 1.0,
            });
        }
        let g = &self.project.manifest.guide;
        if let (Some((v, _)), true, None) = (&self.vocals, g.audible, audition) {
            tracks.push(Track {
                audio: v.clone(),
                start: 0,
                gain: db_to_gain(g.level_db),
            });
        }
        for t in &self.takes {
            let audible = audition.map_or_else(|| self.take_audible(t), |id| id == t.entry.id);
            if let (Some(a), true) = (&t.audio, audible) {
                tracks.push(Track {
                    audio: a.clone(),
                    start: t.entry.start_frame,
                    gain: db_to_gain(t.entry.volume_db),
                });
            }
        }
        Playback { tracks }
    }

    pub fn song_frames(&self) -> i64 {
        let b = self.backing.as_ref().map_or(0, |a| a.frames() as i64);
        let v = self.vocals.as_ref().map_or(0, |(a, _)| a.frames() as i64);
        let t = self
            .takes
            .iter()
            .map(|t| t.entry.start_frame + t.frames())
            .max()
            .unwrap_or(0);
        b.max(v).max(t)
    }

    pub fn export_input(&self) -> ExportInput {
        ExportInput {
            name: self.project.name(),
            rate: self.rate(),
            backing: self.backing.clone(),
            stem_frames: self.vocals.as_ref().map_or(0, |(a, _)| a.frames()),
            takes: self
                .takes
                .iter()
                .filter_map(|t| Some((t.entry.clone(), t.audio.clone()?)))
                .collect(),
        }
    }

    /// What Play outputs, in words (for the transport bar).
    pub fn plays_summary(&self, audition: Option<u32>) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.backing.is_some() {
            parts.push("backing".into());
        }
        let g = &self.project.manifest.guide;
        if self.vocals.is_some() && g.audible && audition.is_none() {
            parts.push("guide voice".into());
        }
        let takes: Vec<String> = self
            .takes
            .iter()
            .filter(|t| t.audio.is_some())
            .filter(|t| audition.map_or_else(|| self.take_audible(t), |id| id == t.entry.id))
            .map(|t| t.entry.name())
            .collect();
        if !takes.is_empty() {
            parts.push(takes.join(", "));
        }
        if parts.is_empty() {
            "nothing yet — import a backing track, press G for the guide voice, or record a take"
                .into()
        } else {
            parts.join(" + ")
        }
    }

    /// Guide offset in cents.
    pub fn offset(&self) -> f32 {
        self.project.manifest.guide.offset_cents as f32
    }

    /// The contour the singer aims at: the target take when one is chosen, else the reference.
    /// The offset applies to it.
    pub fn target(&self) -> Option<&Contour> {
        match self.project.manifest.guide.target_take {
            Some(id) => self
                .takes
                .iter()
                .find(|t| t.entry.id == id)
                .map(|t| &t.contour),
            None => self.reference.as_ref(),
        }
    }

    /// Pitch to sing at a song frame (target transposed by the offset), if voiced there.
    pub fn target_hz_at(&self, frame: i64) -> Option<f32> {
        let hz = self.target()?.hz_at(frame)?;
        Some(hz * 2f32.powf(self.offset() / 1200.0))
    }

    /// Phrase containing `frame`, or else the next one after it.
    pub fn phrase_at_or_after(&self, frame: i64) -> Option<Phrase> {
        self.phrases.iter().copied().find(|p| p.end > frame)
    }
}

/// The current playback pass: what was sung since the transport last started or jumped, and the
/// phrase scores computed from it as the playhead leaves each phrase.
#[derive(Default)]
pub struct Pass {
    pub contour: Contour,
    pub scores: Vec<PhraseScore>,
}

impl Pass {
    pub fn reset(&mut self) {
        self.contour = Contour::new(0, LIVE_HOP as u32);
        self.scores.clear();
    }

    pub fn add(&mut self, song_frame: i64, hz: Option<f32>, clarity: f32) {
        if self.contour.hop == 0 {
            self.reset();
        }
        self.contour.push_at(song_frame, hz, clarity);
    }

    /// Scores phrases the playhead has left that this pass covered.
    pub fn update_scores(
        &mut self,
        reference: &Contour,
        phrases: &[Phrase],
        playhead: i64,
        rate: u32,
    ) {
        if self.contour.is_empty() {
            return;
        }
        let margin = (SCORE_MARGIN_SECS * rate as f32) as i64;
        for p in phrases {
            let passed = p.end + margin <= playhead;
            let covered = p.end > self.contour.start_frame && p.start < self.contour.end_frame();
            if passed && covered && !self.scores.iter().any(|s| s.phrase == *p) {
                if let Some(cents) = feedback::shape_score(reference, &self.contour, *p) {
                    self.scores.push(PhraseScore { phrase: *p, cents });
                } else {
                    // Mark as evaluated (no score) so it's not recomputed every frame.
                    self.scores.push(PhraseScore {
                        phrase: *p,
                        cents: f32::NAN,
                    });
                }
            }
        }
    }
}
