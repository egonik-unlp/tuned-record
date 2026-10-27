//! The open project as the UI sees it: loaded audio, reference, takes, and the mix built from them.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::audio::mixer::{Audio, Playback, Track, db_to_gain, take_audible};
use crate::audio::speed::Ratio;
use crate::feedback::{self, IN_TUNE_CENTS, Note, Phrase, PhraseScore};
use crate::pitch::contour::Contour;
use crate::pitch::live::{BRIDGE_GAP_SECS, HOP as LIVE_HOP};
use crate::pitch::note::{cents_between, midi_to_hz};
use crate::project::export::ExportInput;
use crate::project::{Project, TakeEntry, takes};

const SAVE_DEBOUNCE: Duration = Duration::from_millis(300);
/// Pass scores are computed this long after the playhead leaves a phrase (live detector delay).
const SCORE_MARGIN_SECS: f32 = 0.1;
/// A note counts as reached after the voice stays in tune with it this long...
pub const REACHED_SECS: f32 = 0.1;
/// ...or, for a note shorter than that, this share of the note.
const REACHED_SHORT_SHARE: f32 = 0.5;

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

/// The sources rendered for one practice speed (see `audio::stretch`).
pub struct Stretched {
    pub speed: Ratio,
    pub backing: Option<Arc<Audio>>,
    pub vocals: Option<Arc<Audio>>,
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
    /// Sources rendered for the selected practice speed (none at 100 %).
    pub stretched: Option<Stretched>,
    /// The target segmented into steady notes (untransposed), see [`Session::refresh_notes`].
    pub notes: Vec<Note>,
    /// What `notes` were computed from: target take, contour start, hop and length.
    notes_key: Option<(Option<u32>, i64, u32, usize)>,
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
            stretched: None,
            notes: Vec::new(),
            notes_key: None,
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

    /// The backing and guide stem as rendered for `speed`: the originals at 100 %, the stretched
    /// copies below (none if they are not rendered for that speed).
    fn sources_at(&self, speed: Ratio) -> (Option<&Arc<Audio>>, Option<&Arc<Audio>>) {
        if speed.is_one() {
            (self.backing.as_ref(), self.vocals.as_ref().map(|(a, _)| a))
        } else {
            match &self.stretched {
                Some(st) if st.speed == speed => (st.backing.as_ref(), st.vocals.as_ref()),
                _ => (None, None),
            }
        }
    }

    /// Whether every source has been rendered for `speed`, so it can play.
    pub fn ready_at(&self, speed: Ratio) -> bool {
        let (b, v) = self.sources_at(speed);
        b.is_some() == self.backing.is_some() && v.is_some() == self.vocals.is_some()
    }

    /// What the engine should play at `speed`: backing, guide when audible, and the audible takes;
    /// or, when listening to one take (`audition`), the backing and that take only. Below 100 %
    /// (practice) the stretched backing and guide only: takes are recorded at 100 % and stay
    /// silent.
    pub fn playback(&self, audition: Option<u32>, speed: Ratio) -> Playback {
        let mut tracks = Vec::new();
        let (backing, vocals) = self.sources_at(speed);
        if let Some(b) = backing {
            tracks.push(Track {
                audio: b.clone(),
                start: 0,
                gain: 1.0,
                speed,
            });
        }
        let g = &self.project.manifest.guide;
        if let (Some(v), true, None) = (vocals, g.audible, audition) {
            tracks.push(Track {
                audio: v.clone(),
                start: 0,
                gain: db_to_gain(g.level_db),
                speed,
            });
        }
        for t in self.takes.iter().filter(|_| speed.is_one()) {
            let audible = audition.map_or_else(|| self.take_audible(t), |id| id == t.entry.id);
            if let (Some(a), true) = (&t.audio, audible) {
                tracks.push(Track {
                    audio: a.clone(),
                    start: t.entry.start_frame,
                    gain: db_to_gain(t.entry.volume_db),
                    speed,
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

    /// What Play outputs at `speed`, in words (for the transport bar).
    pub fn plays_summary(&self, audition: Option<u32>, speed: Ratio) -> String {
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
            .filter(|t| t.audio.is_some() && speed.is_one())
            .filter(|t| audition.map_or_else(|| self.take_audible(t), |id| id == t.entry.id))
            .map(|t| t.entry.name())
            .collect();
        if !takes.is_empty() {
            parts.push(takes.join(", "));
        }
        let what = if parts.is_empty() {
            "nothing yet — import a backing track, press G for the guide voice, or record a take"
                .into()
        } else {
            parts.join(" + ")
        };
        if speed.is_one() {
            what
        } else {
            format!("{what} at {} % (practice — takes muted)", speed.percent())
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

    /// Re-segments the target into notes when it changed (another target, or its contour was
    /// re-analysed or grew). Cheap when nothing changed; call once per frame.
    pub fn refresh_notes(&mut self) {
        let key = self.target().map(|c| {
            (
                self.project.manifest.guide.target_take,
                c.start_frame,
                c.hop,
                c.len(),
            )
        });
        if key != self.notes_key {
            self.notes = self
                .target()
                .map(|c| feedback::notes(c, self.rate()))
                .unwrap_or_default();
            self.notes_key = key;
        }
    }

    /// The target note sounding at a song frame, if any (none in gaps and glides).
    pub fn target_note_at(&self, frame: i64) -> Option<Note> {
        let i = self.notes.partition_point(|n| n.end <= frame);
        self.notes.get(i).copied().filter(|n| n.contains(frame))
    }

    /// Pitch to sing at a song frame: the target note there, transposed by the offset.
    pub fn target_hz_at(&self, frame: i64) -> Option<f32> {
        let n = self.target_note_at(frame)?;
        Some(midi_to_hz(n.midi + self.offset() / 100.0))
    }

    /// Phrase containing `frame`, or else the next one after it.
    pub fn phrase_at_or_after(&self, frame: i64) -> Option<Phrase> {
        self.phrases.iter().copied().find(|p| p.end > frame)
    }
}

/// The current playback pass: what was sung since the transport last started or jumped, the
/// phrase scores computed from it as the playhead leaves each phrase, and the notes reached.
#[derive(Default)]
pub struct Pass {
    pub contour: Contour,
    pub scores: Vec<PhraseScore>,
    /// Start frames of the target notes reached in this pass.
    pub reached: HashSet<i64>,
    run: Option<InTuneRun>,
}

/// The voice staying in tune with one note: first and latest in-band song frames.
#[derive(Clone, Copy)]
struct InTuneRun {
    note: i64,
    first: i64,
    last: i64,
}

impl Pass {
    pub fn reset(&mut self) {
        self.contour = Contour::new(0, LIVE_HOP as u32);
        self.scores.clear();
        self.reached.clear();
        self.run = None;
    }

    /// Follows a live frame against the target note at its song frame (`note`, transposed pitch
    /// `note_hz`) and marks the note reached once the voice has stayed within the in-tune band
    /// long enough. Unvoiced gaps shorter than the live-line bridge do not break the run; an
    /// out-of-tune frame or another note does.
    pub fn track_reached(
        &mut self,
        song_frame: i64,
        hz: Option<f32>,
        note: Option<(Note, f32)>,
        rate: u32,
    ) {
        let Some((note, note_hz)) = note else {
            self.run = None;
            return;
        };
        let bridge = (BRIDGE_GAP_SECS * rate as f32) as i64 + LIVE_HOP as i64;
        let run = self
            .run
            .filter(|r| r.note == note.start && song_frame - r.last <= bridge);
        let Some(hz) = hz else {
            self.run = run;
            return;
        };
        if cents_between(hz, note_hz).abs() > IN_TUNE_CENTS {
            self.run = None;
            return;
        }
        let r = run.map_or(
            InTuneRun {
                note: note.start,
                first: song_frame,
                last: song_frame,
            },
            |r| InTuneRun {
                last: song_frame,
                ..r
            },
        );
        self.run = Some(r);
        let need = (REACHED_SECS * rate as f32)
            .min((note.end - note.start) as f32 * REACHED_SHORT_SHARE) as i64;
        // Each frame stands for one hop of voice.
        if r.last - r.first + LIVE_HOP as i64 >= need {
            self.reached.insert(note.start);
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    const RATE: u32 = 48_000;
    const E4: f32 = 329.63;

    fn note(start_ms: i64, len_ms: i64, hz: f32) -> (Note, f32) {
        let n = Note {
            start: start_ms * 48,
            end: (start_ms + len_ms) * 48,
            midi: crate::pitch::note::hz_to_midi(hz),
        };
        (n, hz)
    }

    /// Feeds live frames every hop over `[from_ms, to_ms)` at `hz` (None = unvoiced).
    fn sing(p: &mut Pass, target: (Note, f32), from_ms: i64, to_ms: i64, hz: Option<f32>) {
        let mut f = from_ms * 48;
        while f < to_ms * 48 {
            let at = Some(target).filter(|(n, _)| n.contains(f));
            p.track_reached(f, hz, at, RATE);
            f += LIVE_HOP as i64;
        }
    }

    fn cents(hz: f32, c: f32) -> f32 {
        hz * 2f32.powf(c / 1200.0)
    }

    #[test]
    fn held_long_enough_is_reached() {
        let t = note(0, 400, E4);
        let mut p = Pass::default();
        sing(&mut p, t, 0, 100, None);
        sing(&mut p, t, 100, 250, Some(cents(E4, 8.0)));
        assert!(p.reached.contains(&t.0.start));
    }

    #[test]
    fn too_short_is_not_reached() {
        let t = note(0, 400, E4);
        let mut p = Pass::default();
        sing(&mut p, t, 0, 100, Some(cents(E4, 60.0)));
        sing(&mut p, t, 100, 160, Some(E4));
        sing(&mut p, t, 160, 400, Some(cents(E4, 60.0)));
        assert!(p.reached.is_empty());
    }

    #[test]
    fn short_note_needs_half_its_length() {
        let g4 = 392.0;
        let t = note(0, 80, g4);
        let mut p = Pass::default();
        sing(&mut p, t, 0, 35, None);
        sing(&mut p, t, 35, 80, Some(g4));
        assert!(p.reached.contains(&t.0.start));
    }

    #[test]
    fn off_pitch_is_not_reached() {
        let t = note(0, 400, E4);
        let mut p = Pass::default();
        sing(&mut p, t, 0, 400, Some(cents(E4, -60.0)));
        assert!(p.reached.is_empty());
    }

    #[test]
    fn short_dropouts_do_not_break_the_run() {
        let t = note(0, 400, E4);
        let mut p = Pass::default();
        // 40 ms in tune, 30 ms dropout, 40 ms in tune: 110 ms bridged run.
        sing(&mut p, t, 0, 40, Some(E4));
        sing(&mut p, t, 40, 70, None);
        sing(&mut p, t, 70, 110, Some(E4));
        assert!(p.reached.contains(&t.0.start));

        // A real pause (80 ms) restarts it.
        let mut p = Pass::default();
        sing(&mut p, t, 0, 60, Some(E4));
        sing(&mut p, t, 60, 140, None);
        sing(&mut p, t, 140, 200, Some(E4));
        assert!(p.reached.is_empty());
    }

    fn flat_contour(start: i64, secs: f32, midi: f32) -> Contour {
        let mut c = Contour::new(start, 256);
        for _ in 0..(secs * RATE as f32 / 256.0) as usize {
            c.push(Some(midi_to_hz(midi)), 0.9);
        }
        c
    }

    #[test]
    fn notes_follow_the_chosen_target() {
        let dir =
            std::env::temp_dir().join(format!("tuned-record-test-notes-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let mut s = Session::new(Project::create(&dir, RATE).unwrap(), true);
        s.reference = Some(flat_contour(0, 0.5, 57.0));
        s.takes.push(TakeState {
            entry: TakeEntry::new(2, 4_800),
            audio: None,
            contour: flat_contour(4_800, 0.4, 61.0),
            analysed: true,
            scores: Vec::new(),
        });
        s.refresh_notes();
        assert_eq!(s.notes.len(), 1);
        assert!((s.notes[0].midi - 57.0).abs() < 0.01);

        // Harmony against take 2, transposed +2 st.
        s.project.manifest.guide.target_take = Some(2);
        s.project.manifest.guide.offset_cents = 200;
        s.refresh_notes();
        assert_eq!(s.notes.len(), 1);
        assert!((s.notes[0].midi - 61.0).abs() < 0.01);
        assert_eq!(s.notes[0].start, 4_800);
        let hz = s.target_hz_at(10_000).unwrap();
        assert!(cents_between(hz, midi_to_hz(63.0)).abs() < 1.0, "{hz}");

        s.project.manifest.guide.target_take = None;
        s.refresh_notes();
        assert!((s.notes[0].midi - 57.0).abs() < 0.01);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn reset_clears_reached() {
        let t = note(0, 400, E4);
        let mut p = Pass::default();
        sing(&mut p, t, 0, 200, Some(E4));
        assert!(!p.reached.is_empty());
        p.reset();
        assert!(p.reached.is_empty());
    }
}
