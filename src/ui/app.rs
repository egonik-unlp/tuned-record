//! Application state and event handling. Drawing lives in `panels.rs` and `plot.rs`.

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::Ordering::Relaxed;
use std::time::{Duration, Instant};

use crossbeam_channel::{Receiver, Sender, unbounded};
use egui::{Key, Modifiers};

use super::plot::PlotView;
use super::session::{Pass, Session, TakeState};
use crate::audio::engine::{Command, Engine};
use crate::audio::latency::{click_track, residual_by_xcorr};
use crate::audio::mixer::{Audio, Playback, Track};
use crate::config::Config;
use crate::feedback::Band;
use crate::pitch::contour::Contour;
use crate::pitch::live::HOP as LIVE_HOP;
use crate::pitch::live_thread::{self, LivePoint};
use crate::pitch::note::{cents_between, hz_to_midi};
use crate::project::export::ExportReport;
use crate::project::takes::{self, DiskControl, DiskEvent};
use crate::project::worker::{self, Job, SourceKind, WorkerEvent};
use crate::project::{Project, TakeEntry};

const LIVE_KEEP_SECS: i64 = 15;
const SEEK_SECS: i64 = 5;
/// The live readout counts as current for this long after the last voiced frame.
const LIVE_FRESH: Duration = Duration::from_millis(150);
/// Lead-in before a phrase or take when jumping, looping or listening.
const PREROLL_SECS: f32 = 1.0;
/// Tail after a looped phrase and after a take being listened to.
const TAIL_SECS: f32 = 0.5;
const HINT_SECS: u64 = 8;

pub struct Message {
    pub text: String,
    pub error: bool,
}

pub enum Calibration {
    Idle,
    Running {
        /// Set once the engine reports recording: before that, `song_pos` may still be the
        /// position from before the seek to 0.
        started: bool,
        stop_at: i64,
        prev_pos: i64,
        click: Arc<Audio>,
    },
    Waiting {
        click: Arc<Audio>,
    },
    Done(String),
    Failed(String),
}

/// Listening to one take with the backing: the other takes and the guide are left out.
#[derive(Clone, Copy)]
pub struct Audition {
    pub take: u32,
    /// Stops the transport here.
    end: i64,
    /// Set once the engine reports playing, so a not-yet-started play isn't taken for a stop.
    started: bool,
}

pub struct ExportDialog {
    pub dir: PathBuf,
    pub running: bool,
    pub result: Option<Result<ExportReport, String>>,
}

pub struct App {
    pub config: Config,
    pub engine: Option<Engine>,
    pub engine_error: Option<String>,
    pub rate: u32,
    live_rx: Option<Receiver<LivePoint>>,
    disk_ctl: Option<Sender<DiskControl>>,
    disk_rx: Option<Receiver<DiskEvent>>,
    jobs: Sender<Job>,
    worker_rx: Receiver<WorkerEvent>,

    pub session: Option<Session>,
    pub loading: bool,
    pub live: VecDeque<LivePoint>,
    pub last_voiced: Option<(Instant, LivePoint)>,
    pub pass: Pass,
    pub recording_take: Option<u32>,
    pub progress: Option<(String, f32)>,
    pub messages: Vec<Message>,
    pub view: PlotView,
    pub selected_take: Option<u32>,
    pub confirm_delete: Option<u32>,
    pub calibration: Option<Calibration>,
    pub audio_dialog: bool,
    pub export: Option<ExportDialog>,
    /// Song frames repeated while playing, set with "Loop phrase".
    pub loop_range: Option<(i64, i64)>,
    pub audition: Option<Audition>,
    /// A short status line in the transport bar (e.g. what to do after a take).
    hint: Option<(String, Instant)>,
    last_latency_refresh: Instant,
    last_pos: i64,
}

impl App {
    /// `open` is a project folder to open at start-up (from the command line).
    pub fn new(cc: &eframe::CreationContext, open: Option<PathBuf>) -> Self {
        let ctx = cc.egui_ctx.clone();
        super::panels::setup_style(&ctx);
        let config = Config::load();

        let (jobs, job_rx) = unbounded();
        let (worker_tx, worker_rx) = unbounded();
        let c = ctx.clone();
        worker::spawn(job_rx, worker_tx, move || c.request_repaint());

        let mut app = Self {
            config,
            engine: None,
            engine_error: None,
            rate: 48_000,
            live_rx: None,
            disk_ctl: None,
            disk_rx: None,
            jobs,
            worker_rx,
            session: None,
            loading: false,
            live: VecDeque::new(),
            last_voiced: None,
            pass: Pass::default(),
            recording_take: None,
            progress: None,
            messages: Vec::new(),
            view: PlotView::default(),
            selected_take: None,
            confirm_delete: None,
            calibration: None,
            audio_dialog: false,
            export: None,
            loop_range: None,
            audition: None,
            hint: None,
            last_latency_refresh: Instant::now() - Duration::from_secs(10),
            last_pos: 0,
        };

        match Engine::start_routed(
            app.config.input_port.as_deref(),
            app.config.output_device.as_deref(),
        ) {
            Ok((engine, streams)) => {
                app.rate = engine.sample_rate;
                let (live_tx, live_rx) = unbounded();
                let c = ctx.clone();
                live_thread::spawn(streams.live, engine.sample_rate, live_tx, move || {
                    c.request_repaint()
                });
                let (ctl_tx, ctl_rx) = unbounded();
                let (disk_tx, disk_rx) = unbounded();
                takes::spawn_disk_thread(streams.disk, engine.sample_rate, ctl_rx, disk_tx);
                app.live_rx = Some(live_rx);
                app.disk_ctl = Some(ctl_tx);
                app.disk_rx = Some(disk_rx);
                app.engine = Some(engine);
            }
            Err(e) => app.engine_error = Some(e),
        }
        if let Some(dir) = open {
            app.open_project(dir);
        }
        app
    }

    // ---- messages ---------------------------------------------------------------------------

    pub fn error(&mut self, text: impl Into<String>) {
        self.messages.push(Message {
            text: text.into(),
            error: true,
        });
    }

    pub fn info(&mut self, text: impl Into<String>) {
        self.messages.push(Message {
            text: text.into(),
            error: false,
        });
    }

    pub fn hint(&mut self, text: impl Into<String>) {
        self.hint = Some((text.into(), Instant::now()));
    }

    pub fn current_hint(&self) -> Option<&str> {
        self.hint
            .as_ref()
            .filter(|(_, t)| t.elapsed() < Duration::from_secs(HINT_SECS))
            .map(|(s, _)| s.as_str())
    }

    // ---- transport --------------------------------------------------------------------------

    pub fn playing(&self) -> bool {
        self.engine
            .as_ref()
            .is_some_and(|e| e.shared.playing.load(Relaxed))
    }

    pub fn recording(&self) -> bool {
        self.engine
            .as_ref()
            .is_some_and(|e| e.shared.recording.load(Relaxed))
    }

    pub fn song_pos(&self) -> i64 {
        self.engine
            .as_ref()
            .map_or(0, |e| e.shared.song_pos.load(Relaxed))
    }

    /// Song frame the singer hears now.
    pub fn playhead(&self) -> i64 {
        let pos = self.song_pos();
        match &self.engine {
            Some(e) if self.playing() => pos - e.playback_latency as i64,
            _ => pos,
        }
    }

    /// Whether the transport may run: audio up, and the project (if any) at the server's rate.
    pub fn transport_ready(&self) -> bool {
        self.engine.is_some()
            && self.session.as_ref().is_none_or(|s| s.rate_ok)
            && !self.calibrating()
    }

    pub fn calibrating(&self) -> bool {
        matches!(
            self.calibration,
            Some(Calibration::Running { .. } | Calibration::Waiting { .. })
        )
    }

    fn send(&mut self, cmd: Command) {
        if let Some(e) = &mut self.engine
            && !e.send(cmd)
        {
            log::warn!("engine command queue full");
        }
    }

    pub fn toggle_play(&mut self) {
        if self.playing() {
            self.send(Command::Stop);
        } else if self.transport_ready() {
            if let Some((start, end)) = self.loop_range {
                let pos = self.song_pos();
                if pos < start || pos >= end {
                    self.send(Command::Seek(start));
                }
            }
            self.pass.reset();
            self.send(Command::Play);
        }
    }

    fn secs(&self, s: f32) -> i64 {
        (s * self.rate as f32) as i64
    }

    /// Plays one take with the backing from just before it starts, stopping after it ends.
    pub fn listen_to_take(&mut self, id: u32) {
        if self.recording() || !self.transport_ready() {
            return;
        }
        let Some(t) = self
            .session
            .as_ref()
            .and_then(|s| s.takes.iter().find(|t| t.entry.id == id))
        else {
            return;
        };
        let (start, end) = (t.entry.start_frame, t.entry.start_frame + t.frames());
        self.send(Command::Stop);
        self.audition = Some(Audition {
            take: id,
            end: end + self.secs(TAIL_SECS),
            started: false,
        });
        self.loop_range = None;
        self.send(Command::SetLoop(None));
        self.push_playback();
        self.send(Command::Seek((start - self.secs(PREROLL_SECS)).max(0)));
        self.pass.reset();
        self.send(Command::Play);
    }

    fn advance_audition(&mut self) {
        let playing = self.playing();
        let pos = self.song_pos();
        let Some(a) = &mut self.audition else { return };
        if !a.started {
            a.started = playing;
            return;
        }
        if playing && pos >= a.end {
            self.send(Command::Stop);
        } else if !playing {
            self.audition = None;
            self.push_playback();
        }
    }

    /// Where prev/next jumps land: a pre-roll before each phrase (or take, without a reference).
    fn jump_points(&self) -> Vec<i64> {
        let Some(s) = &self.session else {
            return Vec::new();
        };
        let starts: Vec<i64> = if s.phrases.is_empty() {
            s.takes.iter().map(|t| t.entry.start_frame).collect()
        } else {
            s.phrases.iter().map(|p| p.start).collect()
        };
        let pre = self.secs(PREROLL_SECS);
        let mut v: Vec<i64> = starts.into_iter().map(|p| (p - pre).max(0)).collect();
        v.sort_unstable();
        v.dedup();
        v
    }

    /// Jumps to just before the next (`forward`) or previous phrase.
    pub fn jump_phrase(&mut self, forward: bool) {
        let pos = self.song_pos();
        // Just after a jump, "previous" means the phrase before, not the one jumped to.
        let slack = self.secs(0.2);
        let points = self.jump_points();
        let target = if forward {
            points.into_iter().find(|&p| p > pos + slack)
        } else {
            points.into_iter().rev().find(|&p| p < pos - slack)
        };
        match target {
            Some(p) => self.seek(p),
            None if !forward => self.seek(0),
            None => {}
        }
    }

    /// Loops the phrase under (or after) the playhead, or the selected take; again to stop.
    pub fn toggle_loop(&mut self) {
        if self.loop_range.take().is_some() {
            self.send(Command::SetLoop(None));
            return;
        }
        let Some(s) = &self.session else { return };
        let span = s
            .phrase_at_or_after(self.playhead())
            .map(|p| (p.start, p.end))
            .or_else(|| {
                let t = s
                    .takes
                    .iter()
                    .find(|t| Some(t.entry.id) == self.selected_take)?;
                Some((t.entry.start_frame, t.entry.start_frame + t.frames()))
            });
        let Some((start, end)) = span else {
            self.hint("Nothing to loop: import a vocal stem (phrases) or select a take.");
            return;
        };
        let range = (
            (start - self.secs(PREROLL_SECS)).max(0),
            end + self.secs(TAIL_SECS),
        );
        self.loop_range = Some(range);
        self.send(Command::SetLoop(Some(range)));
        let pos = self.song_pos();
        if pos < range.0 || pos >= range.1 {
            self.seek(range.0);
        }
    }

    pub fn toggle_record(&mut self) {
        if self.recording() {
            self.send(Command::StopRecord);
            return;
        }
        if self.session.is_none() {
            self.error("Open or create a project before recording.");
            return;
        }
        if !self.transport_ready() {
            return;
        }
        if !self.playing() {
            self.pass.reset();
            self.send(Command::Play);
        }
        self.send(Command::StartRecord);
    }

    pub fn seek(&mut self, frame: i64) {
        if self.recording() || self.engine.is_none() {
            return;
        }
        let end = self
            .session
            .as_ref()
            .map_or(i64::MAX, |s| s.song_frames().max(0));
        self.send(Command::Seek(frame.clamp(0, end)));
        self.pass.reset();
    }

    pub fn seek_by_secs(&mut self, secs: i64) {
        let f = self.song_pos() + secs * self.rate as i64;
        self.seek(f);
    }

    pub fn push_playback(&mut self) {
        let audition = self.audition.map(|a| a.take);
        let pb = self
            .session
            .as_ref()
            .map(|s| s.playback(audition))
            .unwrap_or_default();
        self.send(Command::SetPlayback(Arc::new(pb)));
    }

    // ---- guide ------------------------------------------------------------------------------

    pub fn set_offset(&mut self, cents: i32) {
        if let Some(s) = &mut self.session {
            s.project.manifest.guide.offset_cents = cents;
            s.mark_dirty();
        }
    }

    pub fn step_offset(&mut self, semitones: i32) {
        if let Some(s) = &self.session {
            let c = s.project.manifest.guide.offset_cents;
            self.set_offset(c + semitones * 100);
        }
    }

    /// Live and reference pitch at the moment of the latest voiced live frame, if both voiced.
    pub fn snap_pair(&self) -> Option<(f32, f32)> {
        let (t, p) = self
            .last_voiced
            .filter(|(t, _)| t.elapsed() < LIVE_FRESH * 2)?;
        let _ = t;
        let r = self.session.as_ref()?.target()?;
        let at = if self.playing() {
            p.song_frame?
        } else {
            self.playhead()
        };
        Some((p.hz?, r.hz_at(at)?))
    }

    pub fn snap_to_voice(&mut self) {
        if let Some((live, reference)) = self.snap_pair() {
            let semis = (hz_to_midi(live) - hz_to_midi(reference)).round() as i32;
            self.set_offset(semis * 100);
        }
    }

    /// Chooses what to sing against: a take (`Some`) or the guide.
    pub fn set_target(&mut self, take: Option<u32>) {
        if let Some(s) = &mut self.session {
            s.project.manifest.guide.target_take = take;
            s.mark_dirty();
        }
    }

    pub fn toggle_guide(&mut self) {
        if let Some(s) = &mut self.session {
            s.project.manifest.guide.audible ^= true;
            s.mark_dirty();
            self.push_playback();
        }
    }

    /// Current live readout: (hz, deviation from the transposed target in cents if voiced).
    pub fn live_reading(&self) -> Option<(f32, Option<f32>)> {
        let (t, p) = self.last_voiced?;
        if t.elapsed() > LIVE_FRESH {
            return None;
        }
        let hz = p.hz?;
        let dev = self.session.as_ref().and_then(|s| {
            let at = if self.playing() {
                p.song_frame?
            } else {
                self.playhead()
            };
            Some(cents_between(hz, s.target_hz_at(at)?))
        });
        Some((hz, dev))
    }

    pub fn live_band(&self) -> Option<Band> {
        self.live_reading().and_then(|(_, d)| d).map(Band::of)
    }

    // ---- projects ---------------------------------------------------------------------------

    pub fn new_project(&mut self, dir: PathBuf) {
        match Project::create(&dir, self.rate) {
            Ok(p) => {
                self.config.add_recent(dir);
                let _ = self.config.save();
                self.install_session(Session::new(p, true));
            }
            Err(e) => self.error(format!("{e:#}")),
        }
    }

    pub fn open_project(&mut self, dir: PathBuf) {
        if self.recording() {
            return;
        }
        self.send(Command::Stop);
        self.loading = true;
        let _ = self.jobs.send(Job::Open { dir });
    }

    fn install_session(&mut self, s: Session) {
        if let Some(ctl) = &self.disk_ctl {
            let _ = ctl.send(DiskControl::SetTarget {
                takes_dir: s.project.takes_dir(),
                next_id: s.project.manifest.next_take_id,
            });
        }
        self.session = Some(s);
        self.selected_take = None;
        self.audition = None;
        self.loop_range = None;
        self.send(Command::SetLoop(None));
        self.pass.reset();
        self.send(Command::Seek(0));
        self.push_playback();
    }

    pub fn import(&mut self, kind: SourceKind, from: PathBuf) {
        let Some(s) = &self.session else { return };
        let _ = self.jobs.send(Job::Import {
            kind,
            from,
            project_dir: s.project.dir.clone(),
            rate: s.rate(),
        });
    }

    pub fn start_export(&mut self) {
        let (Some(s), Some(d)) = (&self.session, &mut self.export) else {
            return;
        };
        d.running = true;
        d.result = None;
        let _ = self.jobs.send(Job::Export {
            input: s.export_input(),
            dir: d.dir.clone(),
        });
    }

    pub fn open_export_dialog(&mut self) {
        if let Some(s) = &self.session {
            self.export = Some(ExportDialog {
                dir: s.project.dir.join("export"),
                running: false,
                result: None,
            });
        }
    }

    pub fn take_changed(&mut self, i: usize) {
        if let Some(s) = &mut self.session {
            s.sync_take(i);
        }
        self.push_playback();
    }

    pub fn delete_take(&mut self, id: u32) {
        let Some(s) = &mut self.session else { return };
        if self.recording_take == Some(id) {
            return;
        }
        match s.project.delete_take(id) {
            Ok(()) => {
                s.takes.retain(|t| t.entry.id != id);
                if s.project.manifest.guide.target_take == Some(id) {
                    s.project.manifest.guide.target_take = None;
                    s.mark_dirty();
                }
                if self.selected_take == Some(id) {
                    self.selected_take = None;
                }
                if self.audition.is_some_and(|a| a.take == id) {
                    self.send(Command::Stop);
                }
                self.push_playback();
            }
            Err(e) => self.error(format!("Could not delete take {id}: {e:#}")),
        }
    }

    // ---- audio devices ----------------------------------------------------------------------

    /// Records from `port` from now on (and whenever it is present at start-up).
    pub fn select_input(&mut self, port: String) {
        let Some(e) = &mut self.engine else { return };
        let r = e.set_input(&port);
        e.refresh_latency(self.config.calibration_frames);
        self.config.input_port = Some(port);
        let _ = self.config.save();
        if let Err(msg) = r {
            self.error(msg);
        }
    }

    /// Plays into `device` from now on (and whenever it is present at start-up).
    pub fn select_output(&mut self, device: String) {
        let Some(e) = &mut self.engine else { return };
        let r = e.set_output(&device);
        e.refresh_latency(self.config.calibration_frames);
        self.config.output_device = Some(device);
        let _ = self.config.save();
        if let Err(msg) = r {
            self.error(msg);
        }
    }

    // ---- calibration ------------------------------------------------------------------------

    pub fn start_calibration(&mut self) {
        if self.engine.is_none() || self.recording() {
            return;
        }
        let click = Arc::new(Audio::mono(click_track(self.rate as usize / 2, 4)));
        let prev_pos = self.song_pos();
        if let Some(ctl) = &self.disk_ctl {
            let _ = ctl.send(DiskControl::CaptureToMemory);
        }
        self.send(Command::Stop);
        self.send(Command::Seek(0));
        self.send(Command::SetPlayback(Arc::new(Playback {
            tracks: vec![Track {
                audio: click.clone(),
                start: 0,
                gain: 1.0,
            }],
        })));
        self.send(Command::Play);
        self.send(Command::StartRecord);
        let stop_at = click.frames() as i64 + self.rate as i64 / 2;
        self.calibration = Some(Calibration::Running {
            started: false,
            stop_at,
            prev_pos,
            click,
        });
    }

    fn advance_calibration(&mut self) {
        let recording = self.recording();
        let pos = self.song_pos();
        if let Some(Calibration::Running { started, .. }) = &mut self.calibration
            && !*started
        {
            *started = recording;
            return;
        }
        if let Some(Calibration::Running {
            stop_at,
            prev_pos,
            click,
            ..
        }) = &self.calibration
            && pos >= *stop_at
        {
            let (prev, click) = (*prev_pos, click.clone());
            self.send(Command::StopRecord);
            self.send(Command::Stop);
            self.push_playback();
            self.send(Command::Seek(prev));
            self.calibration = Some(Calibration::Waiting { click });
        }
    }

    fn finish_calibration(&mut self, start_frame: i64, samples: Vec<f32>) {
        let Some(Calibration::Waiting { click }) = self.calibration.take() else {
            return;
        };
        let reference = &click.samples;
        let mut aligned = vec![0.0f32; reference.len()];
        for (i, s) in samples.iter().enumerate() {
            let f = start_frame + i as i64;
            if f >= 0 && (f as usize) < aligned.len() {
                aligned[f as usize] = *s;
            }
        }
        let peak = aligned.iter().fold(0.0f32, |m, v| m.max(v.abs()));
        let residual = (peak > 0.01)
            .then(|| residual_by_xcorr(reference, &aligned, self.rate as usize / 10))
            .flatten();
        self.calibration = Some(match residual {
            Some(r) => {
                self.config.calibration_frames += r;
                let _ = self.config.save();
                let total = self.config.calibration_frames;
                if let Some(e) = &mut self.engine {
                    e.refresh_latency(total);
                }
                Calibration::Done(format!(
                    "Measured residual {:+.2} ms. Calibration offset is now {:+.2} ms (total compensation {:.1} ms).",
                    r as f32 * 1000.0 / self.rate as f32,
                    total as f32 * 1000.0 / self.rate as f32,
                    self.engine.as_ref().map_or(0.0, Engine::latency_ms),
                ))
            }
            None => Calibration::Failed(
                "No click was found in the capture. Check that the output is looped back into the input and the level is up.".into(),
            ),
        });
    }

    // ---- per-frame event processing ---------------------------------------------------------

    pub fn process_events(&mut self) {
        if let Some(e) = &mut self.engine {
            e.collect_garbage();
            if self.last_latency_refresh.elapsed() > Duration::from_secs(1) {
                // Rewiring mid-take would shift the rest of it by the new device's latency.
                if !e.shared.recording.load(Relaxed) {
                    e.keep_routed(
                        self.config.input_port.as_deref(),
                        self.config.output_device.as_deref(),
                    );
                }
                e.refresh_latency(self.config.calibration_frames);
                self.last_latency_refresh = Instant::now();
            }
            if e.shared.server_gone.load(Relaxed) && self.engine_error.is_none() {
                self.engine_error =
                    Some("The audio server stopped. Restart the app to reconnect.".into());
            }
        }
        self.drain_live();
        self.drain_disk();
        self.drain_worker();
        self.advance_calibration();
        self.advance_audition();

        if let Some(s) = &mut self.session
            && let Some(err) = s.save_if_due()
        {
            self.error(err);
        }
    }

    fn drain_live(&mut self) {
        let Some(rx) = &self.live_rx else { return };
        let points: Vec<LivePoint> = rx.try_iter().collect();
        let playing = self.playing();
        let pos = self.song_pos();
        if playing && (pos < self.last_pos || pos > self.last_pos + self.rate as i64) {
            self.pass.reset(); // jumped
        }
        self.last_pos = pos;

        for p in points {
            if p.hz.is_some() {
                self.last_voiced = Some((Instant::now(), p));
            }
            if let (true, Some(f)) = (playing, p.song_frame) {
                self.pass.add(f, p.hz, p.clarity);
                if let (Some(id), Some(s)) = (self.recording_take, &mut self.session)
                    && let Some(t) = s.takes.iter_mut().find(|t| t.entry.id == id)
                    && f >= t.entry.start_frame
                    && !t.analysed
                {
                    t.contour.push_at(f, p.hz, p.clarity);
                }
            }
            self.live.push_back(p);
        }
        if let Some(newest) = self.live.back().map(|p| p.wall_frame) {
            let keep = LIVE_KEEP_SECS * self.rate as i64;
            while self
                .live
                .front()
                .is_some_and(|p| p.wall_frame < newest - keep)
            {
                self.live.pop_front();
            }
        }
        if let Some(s) = &self.session
            && let Some(r) = &s.reference
        {
            self.pass
                .update_scores(r, &s.phrases, self.playhead(), s.rate());
        }
    }

    fn drain_disk(&mut self) {
        let Some(rx) = &self.disk_rx else { return };
        let events: Vec<DiskEvent> = rx.try_iter().collect();
        for ev in events {
            match ev {
                DiskEvent::Started { id, start_frame } => {
                    self.recording_take = Some(id);
                    if let Some(s) = &mut self.session {
                        let entry = TakeEntry::new(id, start_frame);
                        s.project.manifest.takes.push(entry.clone());
                        s.project.manifest.next_take_id = id + 1;
                        if let Some(err) = s.save_now() {
                            self.error(err);
                        }
                        let s = self.session.as_mut().unwrap();
                        s.takes.push(TakeState {
                            entry,
                            audio: None,
                            contour: Contour::new(start_frame, LIVE_HOP as u32),
                            analysed: false,
                            scores: Vec::new(),
                        });
                    }
                }
                DiskEvent::Finished { id, .. } => {
                    self.recording_take = None;
                    let Some(s) = &self.session else { continue };
                    let _ = self.jobs.send(Job::LoadTake {
                        id,
                        wav: s.project.take_wav(id),
                    });
                    let start = s
                        .takes
                        .iter()
                        .find(|t| t.entry.id == id)
                        .map(|t| t.entry.start_frame);
                    // Stopped: go back to where the take began, so Play replays it.
                    if let (false, Some(start)) = (self.playing(), start) {
                        self.seek((start - self.secs(PREROLL_SECS)).max(0));
                        self.hint(format!(
                            "Take {id} saved. Space plays it back with everything; ▶ on its row plays it alone with the backing."
                        ));
                    } else {
                        self.hint(format!("Take {id} saved."));
                    }
                }
                DiskEvent::Captured {
                    start_frame,
                    samples,
                } => self.finish_calibration(start_frame, samples),
                DiskEvent::Error(e) => {
                    self.recording_take = None;
                    self.error(e);
                }
            }
        }
    }

    fn drain_worker(&mut self) {
        let events: Vec<WorkerEvent> = self.worker_rx.try_iter().collect();
        for ev in events {
            match ev {
                WorkerEvent::Progress { label, fraction } => {
                    self.progress = Some((label, fraction))
                }
                WorkerEvent::Idle => self.progress = None,
                WorkerEvent::Error(e) => self.error(e),
                WorkerEvent::Opened(Err(e)) => {
                    self.loading = false;
                    self.error(e);
                }
                WorkerEvent::Opened(Ok(lp)) => {
                    self.loading = false;
                    let rate_ok = self
                        .engine
                        .as_ref()
                        .is_none_or(|e| e.sample_rate == lp.project.manifest.sample_rate);
                    if !rate_ok {
                        self.error(format!(
                            "This project was recorded at {} Hz but the audio server runs at {} Hz. \
                             Playback and recording are disabled; switch the server rate to use it.",
                            lp.project.manifest.sample_rate, self.rate
                        ));
                    }
                    if !lp.recovered.is_empty() {
                        self.info(format!("Recovered unsaved take(s): {:?}", lp.recovered));
                    }
                    self.config.add_recent(lp.project.dir.clone());
                    let _ = self.config.save();
                    let mut s = Session::new(lp.project, rate_ok);
                    s.backing = lp.backing;
                    s.vocals = lp.vocals;
                    let rate = s.rate();
                    for t in lp.takes {
                        let needs_analysis = t.contour.is_none();
                        let id = t.entry.id;
                        s.takes.push(TakeState {
                            contour: t.contour.unwrap_or_else(|| {
                                Contour::new(t.entry.start_frame, LIVE_HOP as u32)
                            }),
                            analysed: !needs_analysis,
                            scores: t.scores,
                            audio: Some(t.audio.clone()),
                            entry: t.entry,
                        });
                        if needs_analysis {
                            let f0_path = s.project.take_file(id, "f0");
                            let start_frame = s.takes.last().unwrap().entry.start_frame;
                            let _ = self.jobs.send(Job::AnalyseTake {
                                id,
                                audio: t.audio,
                                rate,
                                start_frame,
                                f0_path,
                            });
                        }
                    }
                    match (lp.reference, &s.vocals) {
                        (Some(r), _) => {
                            let had_scores: Vec<bool> =
                                s.takes.iter().map(|t| !t.scores.is_empty()).collect();
                            s.phrases = crate::feedback::phrases(&r, rate);
                            s.reference = Some(r);
                            for (i, had) in had_scores.into_iter().enumerate() {
                                if !had {
                                    s.rescore_take(i);
                                }
                            }
                        }
                        (None, Some((audio, hash))) => {
                            let _ = self.jobs.send(Job::AnalyseReference {
                                audio: audio.clone(),
                                rate,
                                cache: s.project.reference_cache(),
                                hash: *hash,
                            });
                        }
                        _ => {}
                    }
                    self.install_session(s);
                }
                WorkerEvent::Imported {
                    kind,
                    audio,
                    source,
                    hash,
                } => {
                    let Some(s) = &mut self.session else { continue };
                    match kind {
                        SourceKind::Backing => {
                            s.project.manifest.backing = Some(source);
                            s.backing = Some(audio);
                        }
                        SourceKind::Vocals => {
                            s.project.manifest.vocals = Some(source);
                            s.vocals = Some((audio.clone(), hash));
                            s.reference = None;
                            s.phrases.clear();
                            let _ = self.jobs.send(Job::AnalyseReference {
                                audio,
                                rate: s.rate(),
                                cache: s.project.reference_cache(),
                                hash,
                            });
                        }
                    }
                    if let Some(err) = s.save_now() {
                        self.error(err);
                    }
                    self.push_playback();
                }
                WorkerEvent::ReferenceReady(c) => {
                    if let Some(s) = &mut self.session {
                        s.set_reference(c);
                    }
                }
                WorkerEvent::TakeLoaded { id, audio } => {
                    let Some(s) = &mut self.session else { continue };
                    let Some(i) = s.take_index(id) else { continue };
                    s.takes[i].audio = Some(audio.clone());
                    let start_frame = s.takes[i].entry.start_frame;
                    let _ = self.jobs.send(Job::AnalyseTake {
                        id,
                        audio,
                        rate: s.rate(),
                        start_frame,
                        f0_path: s.project.take_file(id, "f0"),
                    });
                    self.push_playback();
                }
                WorkerEvent::TakeAnalysed { id, contour } => {
                    let Some(s) = &mut self.session else { continue };
                    let Some(i) = s.take_index(id) else { continue };
                    s.takes[i].contour = contour;
                    s.takes[i].analysed = true;
                    s.rescore_take(i);
                }
                WorkerEvent::Exported(r) => {
                    if let Some(d) = &mut self.export {
                        d.running = false;
                        d.result = Some(r);
                    }
                }
            }
        }
    }

    // ---- keyboard ---------------------------------------------------------------------------

    pub fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if self.confirm_delete.is_some() || self.export.is_some() || self.calibration.is_some() {
            return; // dialogs own the keyboard
        }
        let no_focus = ctx.memory(|m| m.focused().is_none());
        let pressed = |m: Modifiers, k: Key| ctx.input_mut(|i| i.consume_key(m, k));
        if pressed(Modifiers::NONE, Key::Space) {
            self.toggle_play();
        }
        if pressed(Modifiers::NONE, Key::R) {
            self.toggle_record();
        }
        if pressed(Modifiers::NONE, Key::Home) {
            self.seek(0);
        }
        if pressed(Modifiers::NONE, Key::S) {
            self.snap_to_voice();
        }
        if pressed(Modifiers::NONE, Key::G) {
            self.toggle_guide();
        }
        if pressed(Modifiers::COMMAND, Key::E) {
            self.open_export_dialog();
        }
        if pressed(Modifiers::NONE, Key::Delete)
            && let Some(id) = self.selected_take
        {
            self.confirm_delete = Some(id);
        }
        if pressed(Modifiers::NONE, Key::L) {
            self.toggle_loop();
        }
        if pressed(Modifiers::NONE, Key::F) {
            self.view.request_fit();
        }
        if pressed(Modifiers::NONE, Key::OpenBracket) {
            self.view.zoom_time(1.25);
        }
        if pressed(Modifiers::NONE, Key::CloseBracket) {
            self.view.zoom_time(0.8);
        }
        if pressed(Modifiers::NONE, Key::Minus) {
            self.view.zoom_pitch(1.25);
        }
        if pressed(Modifiers::NONE, Key::Equals) || pressed(Modifiers::NONE, Key::Plus) {
            self.view.zoom_pitch(0.8);
        }
        if no_focus {
            // Ctrl variants first: plain arrows would not match them, but keep the intent clear.
            if pressed(Modifiers::COMMAND, Key::ArrowLeft) {
                self.jump_phrase(false);
            }
            if pressed(Modifiers::COMMAND, Key::ArrowRight) {
                self.jump_phrase(true);
            }
            if pressed(Modifiers::NONE, Key::ArrowLeft) {
                self.seek_by_secs(-SEEK_SECS);
            }
            if pressed(Modifiers::NONE, Key::ArrowRight) {
                self.seek_by_secs(SEEK_SECS);
            }
            if pressed(Modifiers::NONE, Key::ArrowUp) {
                self.step_offset(1);
            }
            if pressed(Modifiers::NONE, Key::ArrowDown) {
                self.step_offset(-1);
            }
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        self.process_events();
        let ctx = ui.ctx().clone();
        self.handle_shortcuts(&ctx);
        self.draw(ui);
        if self.playing() || self.progress.is_some() || self.calibrating() || self.loading {
            ctx.request_repaint();
        }
    }

    fn on_exit(&mut self) {
        if self.recording() {
            self.send(Command::StopRecord);
            std::thread::sleep(Duration::from_millis(100));
        }
        if let Some(s) = &mut self.session {
            let _ = s.save_now();
        }
    }
}
