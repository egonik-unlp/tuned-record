//! Layout and widgets: transport bar, messages, guide and takes panel, readout, start screen,
//! dialogs. State changes go through `App` methods.

use egui::{Align, Color32, Layout, RichText, Stroke, Ui, Vec2, WidgetInfo, WidgetType};

use super::app::{App, Calibration, NEEDS_FULL_SPEED, ONE_DRIVER, PendingDevice};
use super::plot::{self, PlotInput};
use super::theme;
use super::timeline::{self, OverviewInput};
use crate::audio::engine::{Routing, device_of, short_port};
use crate::audio::speed::SPEEDS;
use crate::feedback::Band;
use crate::pitch::level::InputLevel;
use crate::pitch::note::NoteReading;
use crate::project::worker::SourceKind;

const AUDIO_EXTENSIONS: &[&str] = &[
    "wav", "flac", "mp3", "ogg", "oga", "m4a", "aac", "aiff", "aif",
];

pub fn setup_style(ctx: &egui::Context) {
    ctx.set_visuals(egui::Visuals::dark());
    ctx.global_style_mut(|s| {
        // Thicker, high-contrast focus outline on every interactive widget.
        s.visuals.selection.stroke = Stroke::new(2.0, Color32::from_rgb(0x8a, 0xc4, 0xff));
        s.visuals.widgets.hovered.fg_stroke.width = 1.5;
        s.spacing.item_spacing = Vec2::new(8.0, 6.0);
    });
}

pub fn fmt_time(frames: i64, rate: u32) -> String {
    let ms = (frames.max(0) as f64 * 1000.0 / rate.max(1) as f64) as i64;
    format!(
        "{:02}:{:02}.{:03}",
        ms / 60_000,
        (ms / 1000) % 60,
        ms % 1000
    )
}

const INTERVALS: [&str; 13] = [
    "unison",
    "minor 2nd",
    "major 2nd",
    "minor 3rd",
    "major 3rd",
    "perfect 4th",
    "tritone",
    "perfect 5th",
    "minor 6th",
    "major 6th",
    "minor 7th",
    "major 7th",
    "octave",
];

/// E.g. 4 → "major 3rd above", -15 → "octave + minor 3rd below".
fn interval_name(semitones: i32) -> String {
    let n = semitones.unsigned_abs() as usize;
    let name = match n {
        0..=12 => INTERVALS[n].to_string(),
        13..=24 => format!("octave + {}", INTERVALS[n - 12]),
        _ => format!("{n} semitones"),
    };
    match semitones.signum() {
        1 => format!("{name} above"),
        -1 => format!("{name} below"),
        _ => name,
    }
}

fn fmt_offset(cents: i32) -> String {
    let st = cents / 100;
    let rest = cents % 100;
    if rest == 0 {
        format!("{st:+} st")
    } else {
        format!("{st:+} st {rest:+} ¢")
    }
}

/// A small toggle with a full accessible label (the visible text is abbreviated).
fn labeled_toggle(ui: &mut Ui, on: &mut bool, text: &str, label: String) -> bool {
    let r = ui.selectable_label(*on, text).on_hover_text(&label);
    let selected = *on;
    r.widget_info(|| WidgetInfo::selected(WidgetType::Checkbox, true, selected, &label));
    if r.clicked() {
        *on = !*on;
        return true;
    }
    false
}

fn labeled_button(
    ui: &mut Ui,
    enabled: bool,
    text: impl Into<egui::WidgetText>,
    label: &str,
) -> bool {
    let r = ui
        .add_enabled(enabled, egui::Button::new(text))
        .on_hover_text(label);
    let l = label.to_string();
    r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, enabled, &l));
    r.clicked()
}

/// Shortens a device name for the transport bar.
fn short_device(name: &str) -> String {
    const MAX: usize = 22;
    if name.chars().count() <= MAX {
        name.to_string()
    } else {
        format!("{}…", name.chars().take(MAX - 1).collect::<String>())
    }
}

/// The input and output devices in use: (button text, accessible label, both connected).
fn device_summary(routing: &Routing) -> (String, String, bool) {
    let input = routing.input.first();
    let output = routing.output.first().map(|p| device_of(p));
    let text = format!(
        "🎤 {}  🔊 {}",
        input.map_or("none".into(), |p| if ONE_DRIVER {
            // Same driver on both sides: the channel is what tells the inputs apart.
            format!("{} · {}", short_device(device_of(p)), short_port(p))
        } else {
            short_device(device_of(p))
        }),
        output.map_or("none".into(), short_device),
    );
    let label = format!(
        "Audio devices — recording from {}, playing into {} (click to change)",
        input.map_or("nothing", |p| p.as_str()),
        output.unwrap_or("nothing"),
    );
    (text, label, input.is_some() && output.is_some())
}

/// Lowest level shown by the input meter.
const METER_FLOOR_DB: f32 = -60.0;

/// Horizontal input meter: smoothed RMS as a bar, the decaying peak as a tick.
fn input_meter(ui: &mut Ui, level: &InputLevel, warn: bool) {
    let (rms, peak) = (level.rms_db(), level.peak_db());
    let frac = |db: f32| ((db - METER_FLOOR_DB) / -METER_FLOOR_DB).clamp(0.0, 1.0);
    let (rect, r) = ui.allocate_exact_size(Vec2::new(80.0, 12.0), egui::Sense::hover());
    let text = format!("Input level {rms:.0} dBFS, peak {peak:.0} dBFS");
    r.widget_info(|| WidgetInfo::labeled(WidgetType::ProgressIndicator, true, &text));
    r.on_hover_text(&text);
    let p = ui.painter();
    p.rect_filled(rect, 2.0, theme::GRID);
    let mut fill = rect;
    fill.set_width(rect.width() * frac(rms));
    let color = if level.clipping() {
        theme::OFF
    } else if warn {
        theme::NEAR
    } else {
        theme::IN_TUNE
    };
    p.rect_filled(fill, 2.0, color);
    let x = rect.left() + rect.width() * frac(peak);
    p.line_segment(
        [egui::pos2(x, rect.top()), egui::pos2(x, rect.bottom())],
        Stroke::new(2.0, theme::PLAYHEAD),
    );
    p.rect_stroke(
        rect,
        2.0,
        Stroke::new(1.0, theme::GRID_C),
        egui::StrokeKind::Inside,
    );
}

/// "Device · port" for a full port name.
fn port_label(port: &str) -> String {
    format!("{} · {}", device_of(port), short_port(port))
}

fn pick_audio(title: &str) -> Option<std::path::PathBuf> {
    rfd::FileDialog::new()
        .set_title(title)
        .add_filter("Audio", AUDIO_EXTENSIONS)
        .pick_file()
}

impl App {
    pub fn draw(&mut self, ui: &mut Ui) {
        egui::Panel::top("transport").show(ui, |ui| self.transport_bar(ui));
        egui::Panel::bottom("readout")
            .exact_size(44.0)
            .show(ui, |ui| self.readout(ui));
        if self.session.is_some() {
            egui::Panel::right("side")
                .resizable(true)
                .default_size(320.0)
                .min_size(260.0)
                .show(ui, |ui| {
                    egui::ScrollArea::vertical().show(ui, |ui| {
                        self.guide_panel(ui);
                        ui.separator();
                        self.takes_panel(ui);
                    });
                });
        }
        egui::CentralPanel::default().show(ui, |ui| {
            if self.session.is_none() {
                self.start_screen(ui);
            } else {
                self.overview(ui);
                self.plot_toolbar(ui);
            }
            self.plot(ui);
        });
        let ctx = ui.ctx().clone();
        self.dialogs(&ctx);
    }

    fn transport_bar(&mut self, ui: &mut Ui) {
        ui.horizontal(|ui| {
            let name = self
                .session
                .as_ref()
                .map_or_else(|| "no project".to_string(), |s| s.project.name());
            ui.label(RichText::new(name).strong());
            ui.separator();

            let ready = self.transport_ready();
            let playing = self.playing();
            if labeled_button(
                ui,
                ready || playing,
                if playing { "■ Stop" } else { "▶ Play" },
                "Play / stop (Space)",
            ) {
                self.toggle_play();
            }
            let recording = self.recording();
            let rec_text = if recording {
                let start = self
                    .recording_take
                    .and_then(|id| {
                        self.session
                            .as_ref()?
                            .takes
                            .iter()
                            .find(|t| t.entry.id == id)
                    })
                    .map(|t| t.entry.start_frame);
                let elapsed = start.map_or(0, |s| self.song_pos() - s);
                RichText::new(format!("⏺ REC {}", fmt_time(elapsed, self.rate)))
                    .color(theme::RECORD)
                    .strong()
            } else {
                RichText::new("⏺ Record")
            };
            let full_speed = self.speed.is_one();
            let rec_enabled = ready && self.session.is_some() && full_speed || recording;
            let rec_label = if full_speed || recording {
                "Record / stop recording (R)"
            } else {
                "Recording needs 100 % speed"
            };
            let r = ui
                .add_enabled(rec_enabled, egui::Button::new(rec_text))
                .on_hover_text(rec_label)
                .on_disabled_hover_text(rec_label);
            r.widget_info(|| WidgetInfo::labeled(WidgetType::Button, rec_enabled, rec_label));
            if r.clicked() {
                self.toggle_record();
            }
            if labeled_button(ui, ready && !recording, "⏮", "Return to start (Home)") {
                self.seek(0);
            }

            let rate = self.rate;
            let pos = self.song_pos();
            ui.label(RichText::new(fmt_time(pos, rate)).monospace());
            if let Some(s) = &self.session {
                ui.label(
                    RichText::new(format!("/ {}", fmt_time(s.song_frames(), rate)))
                        .monospace()
                        .color(theme::TEXT_DIM),
                );
            }

            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if labeled_button(
                    ui,
                    self.session.is_some(),
                    "Export…",
                    "Export takes and mixdown (Ctrl+E)",
                ) {
                    self.open_export_dialog();
                }
                if labeled_button(
                    ui,
                    self.engine.as_ref().is_some_and(|e| e.waiting().is_none()) && !recording,
                    "Calibrate…",
                    "Measure latency with a loopback",
                ) {
                    if self.speed.is_one() {
                        self.calibration = Some(Calibration::Idle);
                    } else {
                        self.hint(NEEDS_FULL_SPEED);
                    }
                }
                match &self.engine {
                    Some(e) => {
                        let mut status = format!(
                            "{} fr · {:.1} kHz · ~{:.1} ms",
                            e.buffer_size(),
                            e.sample_rate as f32 / 1000.0,
                            e.latency_ms()
                        );
                        let xr = e.shared.xruns.load(std::sync::atomic::Ordering::Relaxed);
                        let dropped = e
                            .shared
                            .dropped_disk
                            .load(std::sync::atomic::Ordering::Relaxed);
                        if xr > 0 || dropped > 0 {
                            status.push_str(&format!(
                                " · {xr} xruns · {dropped} blocks lost from takes"
                            ));
                        }
                        ui.label(RichText::new(status).monospace().color(theme::TEXT_DIM));
                        let (text, label, ok) = device_summary(&e.routing);
                        let (text, label) = match e.waiting() {
                            // In place of the devices: why there is no audio right now.
                            Some(why) => (
                                RichText::new(format!("⚠ {why}")).color(theme::NEAR),
                                format!("{why} (click to choose another audio device)"),
                            ),
                            None if ok => (RichText::new(text), label),
                            None => (RichText::new(text).color(theme::OFF), label),
                        };
                        if labeled_button(ui, true, text, &label) {
                            self.audio_dialog = true;
                            self.rescan_devices = true;
                        }
                        // Right to left: the meter sits just before the devices; its warning text
                        // goes on the line below (no room here).
                        if let Some(level) = &self.input_level {
                            input_meter(ui, level, self.input_warning().is_some());
                        }
                    }
                    None => {
                        ui.label(RichText::new("audio off").color(theme::OFF));
                    }
                }
            });
        });

        let warning = self.input_warning();
        if self.session.is_some() || warning.is_some() {
            // Own line: the top row has no room left for these.
            let speed_enabled = !self.recording() && !self.calibrating();
            ui.horizontal_wrapped(|ui| {
                if let Some(w) = warning {
                    let color = if w.contains("clipping") {
                        theme::OFF
                    } else {
                        theme::NEAR
                    };
                    ui.label(RichText::new(format!("⚠ {w}")).color(color).strong());
                    ui.separator();
                }
                if self.session.is_none() {
                    return;
                }
                self.speed_buttons(ui, speed_enabled);
                ui.separator();
                let Some(s) = &self.session else { return };
                let audition = self.audition.map(|a| a.take);
                let mut what = match audition {
                    Some(id) => format!(
                        "Listening to take {id}: {}",
                        s.plays_summary(audition, self.speed)
                    ),
                    None => format!("Play plays: {}", s.plays_summary(None, self.speed)),
                };
                if let Some((a, b)) = self.loop_range {
                    what.push_str(&format!(
                        " · looping {}–{}",
                        fmt_time(a, self.rate),
                        fmt_time(b, self.rate)
                    ));
                }
                ui.label(RichText::new(what).small().color(theme::TEXT_DIM));
                if let Some(h) = self.current_hint() {
                    ui.separator();
                    ui.label(RichText::new(h).small().color(theme::IN_TUNE));
                }
            });
        }
        if let Some(e) = &self.engine_error {
            ui.colored_label(theme::OFF, e);
        }
        let mut dismiss = None;
        for (i, m) in self.messages.iter().enumerate() {
            ui.horizontal(|ui| {
                if labeled_button(ui, true, "×", "Dismiss message") {
                    dismiss = Some(i);
                }
                ui.colored_label(if m.error { theme::OFF } else { theme::IN_TUNE }, &m.text);
            });
        }
        if let Some(i) = dismiss {
            self.messages.remove(i);
        }
        if let Some((label, f)) = &self.progress {
            ui.add(egui::ProgressBar::new(*f).text(format!("{label}… {:.0} %", f * 100.0)));
        }
    }

    /// Practice speed toggles; a speed being prepared shows its progress while the previous one
    /// stays selected.
    fn speed_buttons(&mut self, ui: &mut Ui, enabled: bool) {
        ui.label("Speed")
            .on_hover_text("Practice speed (, slower · . faster)");
        for sp in SPEEDS {
            let pct = sp.percent();
            let selected = sp == self.speed;
            let pending = self
                .pending_speed
                .as_ref()
                .filter(|p| p.speed == sp)
                .map(|p| (p.fraction * 100.0).round() as u32);
            let (text, label) = match pending {
                Some(f) => (
                    format!("{pct} % · {f} %"),
                    format!("Speed {pct} percent, rendering, {f} percent"),
                ),
                None => (format!("{pct} %"), format!("Speed {pct} percent")),
            };
            let r = ui
                .add_enabled(enabled, egui::Button::selectable(selected, text))
                .on_hover_text("Practice speed (, slower · . faster)");
            r.widget_info(|| WidgetInfo::selected(WidgetType::Button, enabled, selected, &label));
            if r.clicked() {
                self.set_speed(sp);
            }
        }
    }

    fn readout(&mut self, ui: &mut Ui) {
        ui.horizontal_centered(|ui| match self.live_reading() {
            Some((hz, dev)) => {
                let n = NoteReading::from_hz(hz);
                ui.label(RichText::new(n.label()).size(24.0).strong().monospace());
                ui.label(
                    RichText::new(format!("{:+.0} ¢", n.cents))
                        .size(18.0)
                        .monospace(),
                );
                ui.label(
                    RichText::new(format!("{hz:.1} Hz"))
                        .monospace()
                        .color(theme::TEXT_DIM),
                );
                ui.separator();
                let target = self.target_name();
                match dev {
                    Some(d) => {
                        let band = Band::of(d);
                        let cue = match band {
                            Band::InTune => "hold it",
                            _ if d < 0.0 => "⏶ sing higher",
                            _ => "⏷ sing lower",
                        };
                        ui.label(
                            RichText::new(format!(
                                "vs {target}: {d:+.0} ¢  {}  {cue}",
                                band.label()
                            ))
                            .size(18.0)
                            .strong()
                            .color(theme::band_color(band)),
                        );
                        if self.reading_reached() {
                            ui.label(
                                RichText::new("✓ reached")
                                    .size(18.0)
                                    .strong()
                                    .color(theme::BAR_REACHED),
                            );
                        }
                    }
                    None => {
                        ui.label(RichText::new(format!("vs {target}: —")).color(theme::TEXT_DIM));
                    }
                }
            }
            None => {
                ui.label(RichText::new("—").size(24.0).monospace());
                ui.label(RichText::new("sing to see your pitch").color(theme::TEXT_DIM));
            }
        });
    }

    /// "guide" or "take N": what the singer aims at.
    fn target_name(&self) -> String {
        self.session
            .as_ref()
            .and_then(|s| s.project.manifest.guide.target_take)
            .map_or_else(|| "guide".into(), |id| format!("take {id}"))
    }

    fn overview(&mut self, ui: &mut Ui) {
        let Some(s) = &self.session else { return };
        let playhead = self.playhead();
        let input = OverviewInput {
            session: s,
            playhead,
            window: self.view.window(playhead, self.rate),
            loop_range: self.loop_range,
            can_seek: self.engine.is_some() && !self.recording(),
            rate: self.rate,
            summary: format!(
                "Song overview, at {} of {}. Click or drag to jump.",
                fmt_time(playhead, self.rate),
                fmt_time(s.song_frames(), self.rate)
            ),
        };
        if let Some(f) = timeline::show(ui, &input) {
            self.seek(f);
        }
    }

    fn plot_toolbar(&mut self, ui: &mut Ui) {
        ui.horizontal_wrapped(|ui| {
            let can_move = self.engine.is_some() && !self.recording();
            if labeled_button(ui, can_move, "⏴ Phrase", "Previous phrase (Ctrl+←)") {
                self.jump_phrase(false);
            }
            if labeled_button(ui, can_move, "Phrase ⏵", "Next phrase (Ctrl+→)") {
                self.jump_phrase(true);
            }
            let mut looping = self.loop_range.is_some();
            if labeled_toggle(
                ui,
                &mut looping,
                "⟲ Loop phrase",
                "Loop the phrase at the playhead, or the selected take (L)".into(),
            ) {
                self.toggle_loop();
            }
            ui.separator();
            ui.label("Time");
            if labeled_button(ui, true, "−", "Show more time ([)") {
                self.view.zoom_time(1.25);
            }
            ui.label(RichText::new(format!("{:.0} s", self.view.visible_secs)).monospace());
            if labeled_button(ui, true, "+", "Show less time, in more detail (])") {
                self.view.zoom_time(0.8);
            }
            ui.separator();
            ui.label("Pitch");
            if labeled_button(ui, true, "−", "Show more notes (-)") {
                self.view.zoom_pitch(1.25);
            }
            ui.label(RichText::new(format!("{:.0} st", self.view.span)).monospace());
            if labeled_button(ui, true, "+", "Show fewer notes, larger (=)") {
                self.view.zoom_pitch(0.8);
            }
            if labeled_button(ui, true, "Fit", "Fit the pitch range to the target in view (F)") {
                self.view.request_fit();
            }
            ui.separator();
            ui.label(
                RichText::new(
                    "Click: jump · Drag: move · Wheel: up/down · Ctrl+wheel: zoom time · Ctrl+drag: transpose",
                )
                .small()
                .color(theme::TEXT_DIM),
            );
        });
    }

    fn guide_panel(&mut self, ui: &mut Ui) {
        ui.heading("Guide");
        let busy = self.progress.is_some();
        let (backing, vocals) = {
            let m = &self.session.as_ref().unwrap().project.manifest;
            (
                m.backing.as_ref().map(|s| s.original_name.clone()),
                m.vocals.as_ref().map(|s| s.original_name.clone()),
            )
        };
        for (kind, name) in [(SourceKind::Backing, backing), (SourceKind::Vocals, vocals)] {
            ui.horizontal(|ui| {
                let title = match kind {
                    SourceKind::Backing => "Backing",
                    SourceKind::Vocals => "Vocal stem",
                };
                ui.label(RichText::new(title).strong());
                ui.label(name.unwrap_or_else(|| "none".into()));
            });
            let label = format!("Import {}…", kind.label());
            if labeled_button(ui, !busy && !self.recording(), &label, &label)
                && let Some(p) = pick_audio(&label)
            {
                self.import(kind, p);
            }
        }

        let s = self.session.as_mut().unwrap();
        let has_stem = s.vocals.is_some();
        let mut changed = false;
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let mut audible = s.project.manifest.guide.audible;
            if ui
                .add_enabled(
                    has_stem,
                    egui::Checkbox::new(&mut audible, "Audible guide (G)"),
                )
                .changed()
            {
                s.project.manifest.guide.audible = audible;
                changed = true;
            }
        });
        let mut level = s.project.manifest.guide.level_db;
        if ui
            .add_enabled(
                has_stem,
                egui::Slider::new(&mut level, -40.0..=6.0)
                    .suffix(" dB")
                    .text("guide level"),
            )
            .changed()
        {
            s.project.manifest.guide.level_db = level;
            changed = true;
        }
        if changed {
            s.mark_dirty();
            self.push_playback();
        }

        ui.add_space(8.0);
        ui.heading("What to sing");
        let (target, choices) = {
            let s = self.session.as_ref().unwrap();
            let mut v = vec![(None, "Guide (original voice)".to_string())];
            v.extend(s.takes.iter().map(|t| (Some(t.entry.id), t.entry.name())));
            (s.project.manifest.guide.target_take, v)
        };
        let mut new_target = None;
        egui::ComboBox::from_label("Sing against")
            .selected_text(
                choices
                    .iter()
                    .find(|(id, _)| *id == target)
                    .map_or("Guide (original voice)", |(_, n)| n.as_str()),
            )
            .show_ui(ui, |ui| {
                for (id, name) in &choices {
                    if ui.selectable_label(*id == target, name).clicked() {
                        new_target = Some(*id);
                    }
                }
            });
        if let Some(t) = new_target {
            self.set_target(t);
        }
        let offset = self
            .session
            .as_ref()
            .unwrap()
            .project
            .manifest
            .guide
            .offset_cents;
        let mut new_offset = None;
        egui::ComboBox::from_label("Interval")
            .selected_text(if offset % 100 == 0 {
                interval_name(offset / 100)
            } else {
                fmt_offset(offset)
            })
            .show_ui(ui, |ui| {
                for st in (-12..=12).rev() {
                    if ui
                        .selectable_label(offset == st * 100, interval_name(st))
                        .clicked()
                    {
                        new_offset = Some(st * 100);
                    }
                }
            });
        if let Some(o) = new_offset {
            self.set_offset(o);
        }
        ui.horizontal(|ui| {
            ui.label("Transpose");
            if labeled_button(ui, true, "−", "Transpose guide down a semitone (↓)") {
                self.step_offset(-1);
            }
            ui.label(RichText::new(fmt_offset(offset)).monospace().strong());
            if labeled_button(ui, true, "+", "Transpose guide up a semitone (↑)") {
                self.step_offset(1);
            }
            if labeled_button(ui, offset != 0, "0", "Reset transposition") {
                self.set_offset(0);
            }
        });
        let can_snap = self.snap_pair().is_some();
        if labeled_button(
            ui,
            can_snap,
            "Snap to voice (S)",
            "Transpose the target to the note you are singing (S)",
        ) {
            self.snap_to_voice();
        }
        ui.label(
            RichText::new(
                "The shaded corridor on the plot is the target, transposed: stay inside it. \
                 Ctrl+drag the plot up/down to transpose (Shift: free).",
            )
            .small()
            .color(theme::TEXT_DIM),
        );
    }

    fn takes_panel(&mut self, ui: &mut Ui) {
        ui.heading("Takes");
        let rate = self.rate;
        let recording_take = self.recording_take;
        let can_listen = self.transport_ready() && !self.recording();
        let mut listen = None;
        let s = self.session.as_mut().unwrap();
        let target = s.project.manifest.guide.target_take;
        if s.takes.is_empty() {
            ui.label(RichText::new("Record a take to start layering.").color(theme::TEXT_DIM));
            return;
        }
        let mut changed: Vec<usize> = Vec::new();
        let mut delete = None;
        let mut select = None;
        for (i, t) in s.takes.iter_mut().enumerate() {
            let id = t.entry.id;
            let color = theme::take_color(id);
            let selected = self.selected_take == Some(id);
            egui::Frame::group(ui.style())
                .stroke(Stroke::new(
                    if selected { 2.0 } else { 1.0 },
                    if selected { color } else { theme::GRID_C },
                ))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::splat(12.0), egui::Sense::hover());
                        ui.painter().rect_filled(rect, 2.0, color);
                        let name = t.entry.name();
                        if ui
                            .selectable_label(selected, RichText::new(&name).strong())
                            .clicked()
                        {
                            select = Some(id);
                        }
                        let len = t.frames();
                        ui.label(
                            RichText::new(format!(
                                "{} · {}",
                                fmt_time(t.entry.start_frame, rate),
                                fmt_time(len, rate)
                            ))
                            .monospace()
                            .small()
                            .color(theme::TEXT_DIM),
                        );
                        if target == Some(id) {
                            ui.label(RichText::new("target").small().color(theme::TARGET));
                        }
                        if recording_take == Some(id) {
                            ui.colored_label(theme::RECORD, "⏺ recording…");
                        } else if !t.analysed || t.audio.is_none() {
                            ui.spinner().on_hover_text("analysing");
                        }
                    });
                    ui.horizontal(|ui| {
                        let name = t.entry.name();
                        if labeled_button(
                            ui,
                            can_listen && t.audio.is_some(),
                            "▶",
                            &format!("Listen to {name} alone with the backing"),
                        ) {
                            listen = Some(id);
                        }
                        if labeled_toggle(ui, &mut t.entry.muted, "M", format!("Mute {name}")) {
                            changed.push(i);
                        }
                        if labeled_toggle(ui, &mut t.entry.soloed, "S", format!("Solo {name}")) {
                            changed.push(i);
                        }
                        if labeled_toggle(
                            ui,
                            &mut t.entry.visible,
                            "👁",
                            format!("Show {name} contour"),
                        ) {
                            changed.push(i);
                        }
                        let mut vol = t.entry.volume_db;
                        let r = ui.add(
                            egui::Slider::new(&mut vol, -60.0..=6.0)
                                .suffix(" dB")
                                .show_value(true),
                        );
                        let label = format!("{} volume", t.entry.name());
                        r.widget_info(|| WidgetInfo::labeled(WidgetType::Slider, true, &label));
                        if r.changed() {
                            t.entry.volume_db = vol;
                            changed.push(i);
                        }
                        if labeled_button(
                            ui,
                            recording_take != Some(id),
                            "🗑",
                            &format!("Delete {}", t.entry.name()),
                        ) {
                            delete = Some(id);
                        }
                    });
                });
        }
        if let Some(id) = select {
            self.selected_take = Some(id);
        }
        for i in changed {
            self.take_changed(i);
        }
        if let Some(id) = delete {
            self.confirm_delete = Some(id);
        }
        if let Some(id) = listen {
            self.listen_to_take(id);
        }
    }

    fn start_screen(&mut self, ui: &mut Ui) {
        egui::Frame::group(ui.style()).show(ui, |ui| {
            ui.heading("tuned-record");
            ui.label("Create a project in an empty folder, or open an existing one. You can sing now to see your note in the readout below.");
            ui.horizontal(|ui| {
                if labeled_button(ui, !self.loading, "New project…", "Create a new project in an empty folder")
                    && let Some(d) = rfd::FileDialog::new().set_title("Choose an empty folder for the new project").pick_folder() {
                        self.new_project(d);
                    }
                if labeled_button(ui, !self.loading, "Open project…", "Open an existing project folder")
                    && let Some(d) = rfd::FileDialog::new().set_title("Open a project folder").pick_folder() {
                        self.open_project(d);
                    }
                if self.loading {
                    ui.spinner();
                }
            });
            if !self.config.recent_projects.is_empty() {
                ui.label(RichText::new("Recent").strong());
                for d in self.config.recent_projects.clone() {
                    if ui.add_enabled(!self.loading, egui::Link::new(d.display().to_string())).clicked() {
                        self.open_project(d);
                    }
                }
            }
        });
    }

    fn plot(&mut self, ui: &mut Ui) {
        let playhead = self.playhead();
        let overlay = match (&self.progress, &self.session) {
            (Some((label, f)), _) if label.starts_with("Analysing vocal") => {
                Some(format!("{label}… {:.0} %", f * 100.0))
            }
            (_, Some(s)) if s.vocals.is_none() && s.takes.is_empty() => {
                Some("Import a vocal stem to see the guide".into())
            }
            _ => None,
        };
        let target = self.target_name();
        let summary = match self.live_reading() {
            Some((hz, dev)) => {
                let n = NoteReading::from_hz(hz);
                let reached = if self.reading_reached() {
                    ", note reached"
                } else {
                    ""
                };
                let dev = dev.map_or(format!("no {target} note here"), |d| {
                    format!(
                        "{d:+.0} cents from {target}, {}{reached}",
                        Band::of(d).label()
                    )
                });
                format!(
                    "Pitch plot. Singing {} {:+.0} cents, {dev}.",
                    n.label(),
                    n.cents
                )
            }
            None => "Pitch plot. No pitch detected.".into(),
        };
        let offset = self
            .session
            .as_ref()
            .map(|s| s.project.manifest.guide.offset_cents)
            .unwrap_or(0);
        let note_now = self
            .session
            .as_ref()
            .and_then(|s| s.target_hz_at(playhead))
            .map_or(String::new(), |hz| {
                format!(" Note to sing: {}.", NoteReading::from_hz(hz).label())
            });
        let speed = if self.speed.is_one() {
            String::new()
        } else {
            format!(" Practice speed {} percent.", self.speed.percent())
        };
        let summary = format!(
            "{summary}{note_now} Guide offset {}.{speed}",
            fmt_offset(offset)
        );
        let input = PlotInput {
            session: self.session.as_ref(),
            pass: &self.pass,
            live: &self.live,
            playhead,
            playing: self.playing(),
            can_seek: self.engine.is_some() && !self.recording(),
            loop_range: self.loop_range,
            wall_now: self.live.back().map_or(0, |p| p.wall_frame),
            rate: self.rate,
            overlay,
            summary,
        };
        let r = plot::show(ui, &mut self.view, &input);
        if let Some(o) = r.offset {
            self.set_offset(o);
        }
        if let Some(f) = r.seek {
            self.seek(f);
        }
    }

    fn dialogs(&mut self, ctx: &egui::Context) {
        if let Some(id) = self.confirm_delete {
            let r = egui::Modal::new(egui::Id::new("confirm-delete")).show(ctx, |ui| {
                ui.heading(format!("Delete take {id}?"));
                ui.label("Its audio, contour and scores are removed from the project. This cannot be undone.");
                let mut choice = None;
                ui.horizontal(|ui| {
                    if ui.button("Delete").clicked() {
                        choice = Some(true);
                    }
                    if ui.button("Cancel").clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                        choice = Some(false);
                    }
                });
                choice
            });
            match r.inner {
                Some(true) => {
                    self.confirm_delete = None;
                    self.delete_take(id);
                }
                Some(false) => self.confirm_delete = None,
                None if r.should_close() => self.confirm_delete = None,
                None => {}
            }
        }

        if self.calibration.is_some() {
            let mut close = false;
            let mut run = false;
            egui::Modal::new(egui::Id::new("calibration")).show(ctx, |ui| {
                ui.heading("Latency calibration");
                ui.label(
                    "Connect the output back into the input (a cable from the headphone/line out to the mic/line in, \
                     or a PipeWire loopback), turn the input level up, then run. A few clicks are played and \
                     their capture is measured; the residual delay is stored for this machine.",
                );
                if let Some(e) = &self.engine {
                    ui.label(format!(
                        "Reported: playback {} + capture {} frames; current calibration {:+.2} ms.",
                        e.playback_latency,
                        e.capture_latency,
                        self.config.calibration_frames as f32 * 1000.0 / self.rate as f32
                    ));
                }
                match &self.calibration {
                    Some(Calibration::Running { .. }) | Some(Calibration::Waiting { .. }) => {
                        ui.horizontal(|ui| {
                            ui.spinner();
                            ui.label("Measuring…");
                        });
                    }
                    Some(Calibration::Done(msg)) => {
                        ui.colored_label(theme::IN_TUNE, msg);
                    }
                    Some(Calibration::Failed(msg)) => {
                        ui.colored_label(theme::OFF, msg);
                    }
                    _ => {}
                }
                ui.horizontal(|ui| {
                    let measuring = self.calibrating();
                    if ui.add_enabled(!measuring, egui::Button::new("Run")).clicked() {
                        run = true;
                    }
                    if ui.add_enabled(!measuring, egui::Button::new("Close")).clicked() {
                        close = true;
                    }
                });
            });
            if run {
                self.start_calibration();
            }
            if close {
                self.calibration = None;
            }
        }

        if self.audio_dialog
            && let Some(e) = &self.engine
        {
            let sources = e.input_sources();
            let devices = e.output_devices();
            let current_in = e.routing.input.first().cloned();
            let current_out = e.routing.output.first().map(|p| device_of(p).to_string());
            let recording = self.recording();
            let switching = e.switching();
            let mut new_in = None;
            let mut new_out = None;
            let mut close = false;
            // A pick on another driver moves both sides there: confirm it first.
            let pending = self.pending_device.as_ref().map(|p| match p {
                PendingDevice::Input(port) => (
                    device_of(port).to_string(),
                    format!("Output will move to {} too.", device_of(port)),
                ),
                PendingDevice::Output(d) => (
                    d.clone(),
                    format!("Microphone will move to {d} · in 1 too."),
                ),
            });
            let (mut confirm, mut cancel) = (false, false);
            egui::Modal::new(egui::Id::new("audio-devices")).show(ctx, |ui| {
                ui.heading("Audio devices");
                ui.add_enabled_ui(!recording && pending.is_none() && switching.is_none(), |ui| {
                    egui::ComboBox::from_label("Microphone")
                        .width(320.0)
                        .selected_text(current_in.as_deref().map_or("not connected".into(), port_label))
                        .show_ui(ui, |ui| {
                            for p in &sources {
                                if ui.selectable_label(current_in.as_ref() == Some(p), port_label(p)).clicked() {
                                    new_in = Some(p.clone());
                                }
                            }
                        });
                    egui::ComboBox::from_label("Output")
                        .width(320.0)
                        .selected_text(current_out.as_deref().unwrap_or("not connected"))
                        .show_ui(ui, |ui| {
                            for d in &devices {
                                if ui.selectable_label(current_out.as_ref() == Some(d), d).clicked() {
                                    new_out = Some(d.clone());
                                }
                            }
                        });
                });
                if recording {
                    ui.colored_label(theme::NEAR, "Stop recording to change devices.");
                }
                if let Some((driver, what)) = &pending {
                    ui.horizontal(|ui| {
                        ui.colored_label(theme::NEAR, format!("⚠ {what}"));
                        let switch = ui.button(format!("Switch to {driver}"));
                        // Focus lands on the confirmation when the row appears.
                        let first = egui::Id::new("audio-devices-confirm-shown");
                        if !ui.data(|d| d.get_temp::<bool>(first).unwrap_or(false)) {
                            switch.request_focus();
                            ui.data_mut(|d| d.insert_temp(first, true));
                        }
                        confirm = switch.clicked();
                        cancel = ui.button("Cancel").clicked();
                    });
                } else {
                    ui.data_mut(|d| d.remove::<bool>(egui::Id::new("audio-devices-confirm-shown")));
                }
                if let Some(d) = &switching {
                    ui.colored_label(theme::TEXT_DIM, format!("Switching to {d}…"));
                }
                let mut help = String::from(
                    "The app records one mono channel: pick the input your microphone is plugged into. \
                     The choice is remembered and used whenever the device is present. \
                     The latency calibration is not per device: run Calibrate… again after switching interfaces.",
                );
                if ONE_DRIVER {
                    help.push_str(" The buffer size is set in your interface's ASIO control panel.");
                }
                ui.label(RichText::new(help).color(theme::TEXT_DIM));
                let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
                if escape && pending.is_some() {
                    cancel = true;
                } else if ui.button("Close").clicked() || escape {
                    close = true;
                }
            });
            let current_driver = current_out.clone();
            let elsewhere = |driver: &str| ONE_DRIVER && current_driver.as_deref() != Some(driver);
            if let Some(p) = new_in {
                if elsewhere(device_of(&p)) {
                    self.pending_device = Some(PendingDevice::Input(p));
                } else {
                    self.select_input(p);
                }
            }
            if let Some(d) = new_out {
                if elsewhere(&d) {
                    self.pending_device = Some(PendingDevice::Output(d));
                } else {
                    self.select_output(d);
                }
            }
            if confirm {
                match self.pending_device.take() {
                    Some(PendingDevice::Input(p)) => self.select_input(p),
                    Some(PendingDevice::Output(d)) => self.select_output(d),
                    None => {}
                }
            }
            if cancel {
                self.pending_device = None;
            }
            if close {
                self.audio_dialog = false;
                self.pending_device = None;
            }
        }

        if self.export.is_some() {
            let mut close = false;
            let mut start = false;
            let rows: Vec<String> = self
                .session
                .as_ref()
                .map(|s| {
                    let n = s.project.name();
                    let mut v: Vec<String> = s
                        .takes
                        .iter()
                        .filter(|t| t.audio.is_some())
                        .map(|t| format!("{n}-take-{:03}.wav", t.entry.id))
                        .collect();
                    v.push(format!(
                        "{n}-mixdown.wav  (backing + audible takes, no guide)"
                    ));
                    v
                })
                .unwrap_or_default();
            let d = self.export.as_mut().unwrap();
            egui::Modal::new(egui::Id::new("export")).show(ctx, |ui| {
                ui.heading("Export");
                ui.label("Every file starts at song t=0 with the same length and rate: place them all at the start of a DAW timeline.");
                ui.horizontal(|ui| {
                    ui.label("Folder:");
                    ui.label(RichText::new(d.dir.display().to_string()).monospace());
                    if ui.add_enabled(!d.running, egui::Button::new("Choose…")).clicked()
                        && let Some(p) = rfd::FileDialog::new().set_title("Export folder").pick_folder() {
                            d.dir = p;
                        }
                });
                for r in &rows {
                    ui.label(RichText::new(r).monospace().small());
                }
                match &d.result {
                    Some(Ok(rep)) => {
                        let peak_db = 20.0 * rep.peak.max(1e-9).log10();
                        let note = if rep.peak > 1.0 { " — above full scale; lower the level in your DAW" } else { "" };
                        ui.colored_label(theme::IN_TUNE, format!("Wrote {} files. Mixdown peak {peak_db:+.1} dBFS{note}.", rep.files.len()));
                        if ui.button("Open folder").clicked() {
                            let _ = std::process::Command::new("xdg-open").arg(&d.dir).spawn();
                        }
                    }
                    Some(Err(e)) => {
                        ui.colored_label(theme::OFF, e);
                    }
                    None if d.running => {
                        ui.spinner();
                    }
                    None => {}
                }
                ui.horizontal(|ui| {
                    if ui.add_enabled(!d.running, egui::Button::new("Export")).clicked() {
                        start = true;
                    }
                    if ui.add_enabled(!d.running, egui::Button::new("Close")).clicked() {
                        close = true;
                    }
                });
            });
            if start {
                self.start_export();
            }
            if close {
                self.export = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interval_names() {
        assert_eq!(interval_name(0), "unison");
        assert_eq!(interval_name(4), "major 3rd above");
        assert_eq!(interval_name(-3), "minor 3rd below");
        assert_eq!(interval_name(12), "octave above");
        assert_eq!(interval_name(-15), "octave + minor 3rd below");
    }
}
