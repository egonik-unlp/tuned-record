//! The pitch plot: note grid, fixed playhead, the target (reference or a take, transposed) with
//! its in-tune corridor and a marker at the playhead, the reference (solid past / dashed ghost
//! ahead), take contours, the live line coloured by deviation band, and phrase score labels.

use std::collections::VecDeque;

use egui::{
    Align2, Color32, FontId, Mesh, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2, WidgetInfo,
    WidgetType,
};

use super::session::{Pass, Session};
use super::theme;
use crate::feedback::{Band, IN_TUNE_CENTS, NEAR_CENTS};
use crate::pitch::contour::Contour;
use crate::pitch::live_thread::LivePoint;
use crate::pitch::note::{cents_between, hz_to_midi, note_label};

/// Playhead position as a fraction of the plot width.
pub const PLAYHEAD_FRAC: f32 = 0.35;
pub const MIN_VISIBLE_SECS: f32 = 2.0;
pub const MAX_VISIBLE_SECS: f32 = 120.0;
/// Visible pitch range limits, in semitones.
pub const MIN_SPAN: f32 = 6.0;
pub const MAX_SPAN: f32 = 48.0;
const GUTTER: f32 = 44.0;
const SCORE_LANE: f32 = 22.0;
const TICK_LANE: f32 = 16.0;

pub struct PlotView {
    pub center: f32,
    /// Manual vertical shift (mouse wheel or drag), in semitones.
    pub shift: f32,
    /// Visible pitch range, in semitones.
    pub span: f32,
    /// Visible time, in seconds.
    pub visible_secs: f32,
    recentering: bool,
    fit_requested: bool,
    drag: Option<Drag>,
}

#[derive(Clone, Copy)]
enum Drag {
    /// Ctrl-drag: transposing the target. Offset at the start, accumulated upward pixels.
    Transpose { start_offset: i32, accum: f32 },
    /// Plain drag: moving the view. Playhead at the start, accumulated horizontal pixels.
    Pan { start_frame: i64, accum_x: f32 },
}

impl Default for PlotView {
    fn default() -> Self {
        Self {
            center: 60.0,
            shift: 0.0,
            span: 24.0,
            visible_secs: 10.0,
            recentering: false,
            fit_requested: false,
            drag: None,
        }
    }
}

impl PlotView {
    /// `factor` > 1 shows more time.
    pub fn zoom_time(&mut self, factor: f32) {
        self.visible_secs = (self.visible_secs * factor).clamp(MIN_VISIBLE_SECS, MAX_VISIBLE_SECS);
    }

    /// `factor` > 1 shows more semitones.
    pub fn zoom_pitch(&mut self, factor: f32) {
        self.span = (self.span * factor).clamp(MIN_SPAN, MAX_SPAN);
    }

    /// Fits the pitch range to the target visible on the next frame.
    pub fn request_fit(&mut self) {
        self.fit_requested = true;
    }

    /// Song frames visible around `playhead`.
    pub fn window(&self, playhead: i64, rate: u32) -> (i64, i64) {
        let visible = rate as f32 * self.visible_secs;
        (
            playhead - (visible * PLAYHEAD_FRAC) as i64,
            playhead + (visible * (1.0 - PLAYHEAD_FRAC)) as i64,
        )
    }
}

pub struct PlotInput<'a> {
    pub session: Option<&'a Session>,
    pub pass: &'a Pass,
    pub live: &'a VecDeque<LivePoint>,
    /// Song frame the listener hears now.
    pub playhead: i64,
    pub playing: bool,
    /// Whether clicking or dragging may move the playhead (not while recording).
    pub can_seek: bool,
    pub loop_range: Option<(i64, i64)>,
    /// Wall frame of the newest live point (decides whether the voice is current).
    pub wall_now: i64,
    pub rate: u32,
    pub overlay: Option<String>,
    /// Text alternative for screen readers.
    pub summary: String,
}

/// What the user asked for on the plot this frame.
#[derive(Default)]
pub struct PlotResponse {
    /// New guide offset (cents), from a Ctrl-drag.
    pub offset: Option<i32>,
    /// Song frame to move the playhead to, from a click, drag or horizontal scroll.
    pub seek: Option<i64>,
}

fn tick_label(frames: i64, rate: f32, tenths: bool) -> String {
    let secs = frames as f32 / rate;
    let m = (secs / 60.0).floor();
    let s = secs - m * 60.0;
    if tenths {
        format!("{m:.0}:{s:04.1}")
    } else {
        format!("{m:.0}:{:02.0}", s.floor())
    }
}

pub fn show(ui: &mut Ui, view: &mut PlotView, input: &PlotInput) -> PlotResponse {
    let mut out = PlotResponse::default();
    let size = ui.available_size().max(Vec2::new(200.0, 160.0));
    let (resp, painter) = ui.allocate_painter(size, Sense::click_and_drag());
    let summary = input.summary.clone();
    resp.widget_info(|| WidgetInfo::labeled(WidgetType::Other, true, &summary));
    let rect = resp.rect;
    painter.rect_filled(rect, 0.0, theme::BG);

    let area = Rect::from_min_max(
        Pos2::new(rect.left() + GUTTER, rect.top() + TICK_LANE),
        Pos2::new(rect.right(), rect.bottom() - SCORE_LANE),
    );
    let rate = input.rate.max(1) as f32;
    let pps = area.width() / view.visible_secs;
    let playhead_x = area.left() + area.width() * PLAYHEAD_FRAC;
    let x_of = |f: i64| playhead_x + (f - input.playhead) as f32 / rate * pps;
    let frame_at = |x: f32| input.playhead + ((x - playhead_x) / pps * rate) as i64;
    let t0 = frame_at(area.left());
    let t1 = frame_at(area.right());
    let session = input.session;
    let offset = session.map_or(0.0, |s| s.offset());
    let target = session.and_then(|s| s.target());
    // The reference is drawn transposed only when it is what the singer aims at.
    let guide_is_target = session.is_some_and(|s| s.project.manifest.guide.target_take.is_none());

    // Vertical view: follow the visible target, else the singer.
    // Only a current voice counts (the last half second), not stale points from earlier.
    // Stopped, the plot is frozen: the voice neither moves the view nor draws a line.
    let fresh = input.wall_now - input.rate as i64 / 2;
    let last_live = if input.playing {
        input
            .live
            .iter()
            .rev()
            .take_while(|p| p.wall_frame >= fresh)
            .find_map(|p| p.hz)
            .map(hz_to_midi)
    } else {
        None
    };
    let target_range = target.and_then(|c| {
        c.index_range(t0, t1)
            .filter_map(|i| c.voiced(i))
            .map(|h| hz_to_midi(h) + offset / 100.0)
            .fold(None, |r: Option<(f32, f32)>, m| {
                Some(r.map_or((m, m), |(lo, hi)| (lo.min(m), hi.max(m))))
            })
    });
    if std::mem::take(&mut view.fit_requested)
        && let Some((lo, hi)) = target_range
    {
        view.span = (hi - lo + 4.0).clamp(MIN_SPAN, MAX_SPAN);
        view.center = (lo + hi) / 2.0;
        view.shift = 0.0;
        view.recentering = false;
    }
    // Re-centre with hysteresis: start when the content drifts a quarter span off-centre, then
    // glide until it is within half a semitone.
    if let Some(t) = target_range.map(|(lo, hi)| (lo + hi) / 2.0).or(last_live) {
        let d = t - view.center;
        if d.abs() > view.span * 0.25 {
            view.recentering = true;
        }
        if view.recentering {
            view.center += d * 0.15;
            view.recentering = d.abs() > 0.5;
            ui.ctx().request_repaint();
        }
    }

    // Mouse: wheel moves the pitch view, Ctrl+wheel (or pinch) zooms time, horizontal wheel
    // and drag move through the song while stopped, click jumps, Ctrl+drag transposes.
    let row_h = area.height() / view.span;
    let may_move = input.can_seek && !input.playing;
    if resp.hovered() {
        let (scroll, zoom) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
        view.shift += scroll.y / 40.0;
        if zoom != 1.0 {
            view.zoom_time(1.0 / zoom);
        }
        if scroll.x != 0.0 && may_move {
            out.seek = Some(frame_at(playhead_x - scroll.x).max(0));
        }
    }
    if resp.double_clicked() {
        view.shift = 0.0;
    } else if resp.clicked()
        && input.can_seek
        && let Some(p) = resp.interact_pointer_pos()
    {
        out.seek = Some(frame_at(p.x).max(0));
    }
    let session_offset = session.map(|s| s.project.manifest.guide.offset_cents);
    if resp.drag_started() {
        view.drag = match (ui.input(|i| i.modifiers.command), session_offset) {
            (true, Some(start_offset)) => Some(Drag::Transpose {
                start_offset,
                accum: 0.0,
            }),
            _ => Some(Drag::Pan {
                start_frame: input.playhead,
                accum_x: 0.0,
            }),
        };
    }
    if resp.dragged() {
        let d = resp.drag_delta();
        match &mut view.drag {
            // Snapped to semitones, free with Shift.
            Some(Drag::Transpose {
                start_offset,
                accum,
            }) => {
                *accum -= d.y;
                let cents = *accum / row_h * 100.0;
                let delta = if ui.input(|i| i.modifiers.shift) {
                    cents.round() as i32
                } else {
                    (cents / 100.0).round() as i32 * 100
                };
                out.offset = Some(*start_offset + delta).filter(|&n| Some(n) != session_offset);
            }
            Some(Drag::Pan {
                start_frame,
                accum_x,
            }) => {
                view.shift += d.y / row_h;
                *accum_x += d.x;
                if may_move {
                    out.seek = Some((*start_frame - (*accum_x / pps * rate) as i64).max(0));
                }
            }
            None => {}
        }
    }
    if resp.drag_stopped() {
        view.drag = None;
    }

    let center = view.center + view.shift;
    let y_of = |midi: f32| area.center().y - (midi - center) * row_h;

    // Loop region.
    if let Some((a, b)) = input.loop_range {
        let r = Rect::from_x_y_ranges(
            x_of(a).max(area.left())..=x_of(b).min(area.right()),
            area.y_range(),
        );
        if r.width() > 0.0 {
            painter.rect_filled(r, 0.0, theme::LOOP);
        }
    }

    // Time ticks.
    let tick_secs = [0.5f32, 1.0, 2.0, 5.0, 10.0, 15.0, 30.0, 60.0]
        .into_iter()
        .find(|s| s * pps >= 70.0)
        .unwrap_or(120.0);
    let tick = (tick_secs * rate) as i64;
    let mut f = t0.max(0).div_euclid(tick) * tick;
    while f <= t1 {
        let x = x_of(f);
        if x >= area.left() {
            painter.line_segment(
                [Pos2::new(x, area.top() - 4.0), Pos2::new(x, area.top())],
                Stroke::new(1.0, theme::GRID_C),
            );
            painter.text(
                Pos2::new(x + 2.0, rect.top() + 1.0),
                Align2::LEFT_TOP,
                tick_label(f, rate, tick_secs < 1.0),
                FontId::monospace(10.0),
                theme::GRID_LABEL,
            );
        }
        f += tick;
    }

    // Grid.
    let target_now = session
        .and_then(|s| s.target_hz_at(input.playhead))
        .map(hz_to_midi);
    let target_row = target_now.map(|m| m.round() as i32);
    let lo = (center - view.span / 2.0).floor() as i32;
    let hi = (center + view.span / 2.0).ceil() as i32;
    let current_row = last_live.map(|m| m.round() as i32);
    for m in lo..=hi {
        let y = y_of(m as f32);
        if current_row == Some(m) {
            let band = Rect::from_center_size(
                Pos2::new(area.center().x, y),
                Vec2::new(area.width(), row_h),
            );
            painter.rect_filled(band, 0.0, theme::CURRENT_ROW);
        }
        let is_c = m.rem_euclid(12) == 0;
        painter.line_segment(
            [Pos2::new(area.left(), y), Pos2::new(area.right(), y)],
            Stroke::new(1.0, if is_c { theme::GRID_C } else { theme::GRID }),
        );
        if is_c || row_h >= 13.0 || current_row == Some(m) || target_row == Some(m) {
            let color = if current_row == Some(m) {
                theme::PLAYHEAD
            } else if target_row == Some(m) {
                theme::TARGET
            } else {
                theme::GRID_LABEL
            };
            painter.text(
                Pos2::new(rect.left() + 6.0, y),
                Align2::LEFT_CENTER,
                note_label(m),
                FontId::monospace(11.0),
                color,
            );
        }
    }

    // Step so that contours are drawn with about one point per pixel.
    let step_for = |c: &Contour| ((rate / pps / c.hop.max(1) as f32).floor() as usize).max(1);

    // The corridor to sing in: near band, then the in-tune band on top.
    if let Some(c) = target {
        for (half, color) in [
            (NEAR_CENTS, theme::BAND_NEAR),
            (IN_TUNE_CENTS, theme::BAND_IN_TUNE),
        ] {
            draw_band(
                &painter,
                c,
                t0,
                t1,
                step_for(c),
                offset,
                half,
                color,
                &x_of,
                &y_of,
            );
        }
    }

    if let Some(s) = session {
        if let Some(r) = &s.reference {
            if guide_is_target && offset != 0.0 {
                // The melody where it is sung, to harmonise against.
                draw_contour(&painter, r, t0, t1, step_for(r), 0.0, &x_of, &y_of, |_| {
                    ContourStyle::Dashed(theme::MELODY, 1.5)
                });
            }
            draw_contour(
                &painter,
                r,
                t0,
                t1,
                step_for(r),
                if guide_is_target { offset } else { 0.0 },
                &x_of,
                &y_of,
                |f| {
                    if f <= input.playhead {
                        ContourStyle::Solid(theme::REFERENCE, 2.0)
                    } else {
                        ContourStyle::Dashed(theme::REFERENCE_GHOST, 2.0)
                    }
                },
            );
        }
        for t in s.takes.iter().filter(|t| t.entry.visible) {
            let color = theme::take_color(t.entry.id);
            draw_contour(
                &painter,
                &t.contour,
                t0,
                t1,
                step_for(&t.contour),
                0.0,
                &x_of,
                &y_of,
                |_| ContourStyle::Solid(color, 1.5),
            );
            if let Some(i) = t
                .contour
                .index_range(t0.max(t.contour.start_frame), t1)
                .find(|&i| t.contour.voiced(i).is_some())
            {
                let p = Pos2::new(
                    x_of(t.contour.frame_of(i)),
                    y_of(hz_to_midi(t.contour.hz[i])) - 10.0,
                );
                painter.text(
                    p,
                    Align2::LEFT_BOTTOM,
                    t.entry.name(),
                    FontId::proportional(11.0),
                    color,
                );
            }
        }
    }

    // Live line, ending in a dot while the voice is current.
    let mut prev: Option<Pos2> = None;
    let mut dot: Option<(Pos2, Color32)> = None;
    for p in input.live.iter().filter(|_| input.playing) {
        let Some(at) = p.song_frame else {
            prev = None;
            dot = None;
            continue;
        };
        let x = x_of(at);
        let Some(hz) = p.hz.filter(|_| x >= area.left() && x <= area.right()) else {
            prev = None;
            dot = None;
            continue;
        };
        let pos = Pos2::new(x, y_of(hz_to_midi(hz)));
        let color = session
            .and_then(|s| s.target_hz_at(at))
            .map_or(theme::LIVE_NEUTRAL, |t| {
                theme::band_color(Band::of(cents_between(hz, t)))
            });
        if let Some(a) = prev.filter(|a| (a.y - pos.y).abs() < row_h * 3.0) {
            painter.line_segment([a, pos], Stroke::new(3.0, color));
        } else {
            painter.circle_filled(pos, 1.5, color);
        }
        prev = Some(pos);
        dot = Some((pos, color));
    }
    if let (Some((pos, color)), Some(_)) = (dot, last_live) {
        painter.circle_filled(pos, 5.0, color);
    }

    // Playhead, with the note to sing now.
    painter.line_segment(
        [
            Pos2::new(playhead_x, area.top()),
            Pos2::new(playhead_x, area.bottom()),
        ],
        Stroke::new(1.5, theme::PLAYHEAD),
    );
    if let Some(m) = target_now {
        let y = y_of(m);
        painter.line_segment(
            [
                Pos2::new(playhead_x - 14.0, y),
                Pos2::new(playhead_x + 14.0, y),
            ],
            Stroke::new(3.0, theme::TARGET),
        );
        painter.text(
            Pos2::new(playhead_x + 18.0, y),
            Align2::LEFT_CENTER,
            note_label(m.round() as i32),
            FontId::monospace(12.0),
            theme::TARGET,
        );
    }

    // Phrase scores: current pass in white, visible takes in their colours.
    if let Some(s) = session {
        let lane_y = rect.bottom() - SCORE_LANE / 2.0;
        for ph in s.phrases.iter().filter(|p| p.end > t0 && p.start < t1) {
            let mut items: Vec<(String, Color32)> = Vec::new();
            if let Some(sc) = input
                .pass
                .scores
                .iter()
                .find(|x| x.phrase == *ph)
                .filter(|x| x.cents.is_finite())
            {
                items.push((format!("{:.0} ¢", sc.cents), theme::PLAYHEAD));
            }
            for t in s.takes.iter().filter(|t| t.entry.visible) {
                if let Some(sc) = t.scores.iter().find(|x| x.phrase == *ph) {
                    items.push((format!("{:.0} ¢", sc.cents), theme::take_color(t.entry.id)));
                }
            }
            let mut x = x_of(ph.start).max(area.left() + 2.0);
            for (text, color) in items {
                let g = painter.layout_no_wrap(text, FontId::monospace(11.0), color);
                let w = g.size().x;
                painter.galley(Pos2::new(x, lane_y - g.size().y / 2.0), g, color);
                x += w + 8.0;
            }
            // Phrase extent marker.
            let (a, b) = (
                x_of(ph.start).max(area.left()),
                x_of(ph.end).min(area.right()),
            );
            painter.line_segment(
                [Pos2::new(a, area.bottom()), Pos2::new(b, area.bottom())],
                Stroke::new(2.0, theme::GRID_C),
            );
        }
    }

    if let Some(text) = &input.overlay {
        painter.text(
            area.center(),
            Align2::CENTER_CENTER,
            text,
            FontId::proportional(16.0),
            theme::PLAYHEAD,
        );
    }
    out
}

enum ContourStyle {
    Solid(Color32, f32),
    Dashed(Color32, f32),
}

#[allow(clippy::too_many_arguments)]
fn draw_contour(
    painter: &egui::Painter,
    c: &Contour,
    t0: i64,
    t1: i64,
    step: usize,
    offset_cents: f32,
    x_of: &dyn Fn(i64) -> f32,
    y_of: &dyn Fn(f32) -> f32,
    style: impl Fn(i64) -> ContourStyle,
) {
    let flush = |pts: &mut Vec<Pos2>, dashed: bool, color: Color32, w: f32| {
        if pts.len() >= 2 {
            if dashed {
                painter.extend(Shape::dashed_line(pts, Stroke::new(w, color), 6.0, 4.0));
            } else {
                painter.add(Shape::line(std::mem::take(pts), Stroke::new(w, color)));
            }
        }
        pts.clear();
    };
    let mut pts: Vec<Pos2> = Vec::new();
    let mut cur: Option<(bool, Color32, f32)> = None;
    let range = c.index_range(t0, t1);
    let mut i = range.start.saturating_sub(1);
    while i < range.end.min(c.len()) {
        let f = c.frame_of(i);
        let (dashed, color, w) = match style(f) {
            ContourStyle::Solid(c, w) => (false, c, w),
            ContourStyle::Dashed(c, w) => (true, c, w),
        };
        match c.voiced(i) {
            Some(hz) => {
                let p = Pos2::new(x_of(f), y_of(hz_to_midi(hz) + offset_cents / 100.0));
                if let Some((d, col, wd)) = cur.filter(|&(d, _, _)| d != dashed) {
                    // Style changes at the playhead: end this run where the next one starts.
                    pts.push(p);
                    flush(&mut pts, d, col, wd);
                }
                pts.push(p);
                cur = Some((dashed, color, w));
            }
            None => {
                if let Some((d, col, wd)) = cur.take() {
                    flush(&mut pts, d, col, wd);
                }
            }
        }
        i += step;
    }
    if let Some((d, col, wd)) = cur {
        flush(&mut pts, d, col, wd);
    }
}

/// Fills the band `±half_cents` around a contour transposed by `offset_cents`.
#[allow(clippy::too_many_arguments)]
fn draw_band(
    painter: &egui::Painter,
    c: &Contour,
    t0: i64,
    t1: i64,
    step: usize,
    offset_cents: f32,
    half_cents: f32,
    color: Color32,
    x_of: &dyn Fn(i64) -> f32,
    y_of: &dyn Fn(f32) -> f32,
) {
    let half = half_cents / 100.0;
    let mut mesh = Mesh::default();
    let mut prev: Option<(f32, f32)> = None;
    let range = c.index_range(t0, t1);
    let mut i = range.start.saturating_sub(1);
    while i < range.end.min(c.len()) {
        match c.voiced(i) {
            Some(hz) => {
                let (x, m) = (x_of(c.frame_of(i)), hz_to_midi(hz) + offset_cents / 100.0);
                if let Some((px, pm)) = prev {
                    let base = mesh.vertices.len() as u32;
                    for (vx, vm) in [
                        (px, pm + half),
                        (px, pm - half),
                        (x, m + half),
                        (x, m - half),
                    ] {
                        mesh.colored_vertex(Pos2::new(vx, y_of(vm)), color);
                    }
                    mesh.add_triangle(base, base + 1, base + 2);
                    mesh.add_triangle(base + 1, base + 3, base + 2);
                }
                prev = Some((x, m));
            }
            None => prev = None,
        }
        i += step;
    }
    if !mesh.is_empty() {
        painter.add(Shape::mesh(mesh));
    }
}
