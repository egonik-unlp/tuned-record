//! The song overview: the whole song at a glance (guide phrases, takes, loop, the part the plot
//! shows, the playhead). Click or drag on it to jump.

use egui::{
    Align2, CursorIcon, FontId, Pos2, Rect, Sense, Stroke, StrokeKind, Ui, Vec2, WidgetInfo,
    WidgetType,
};

use super::session::Session;
use super::theme;

const TICKS: f32 = 12.0;
const LANE: f32 = 6.0;
const MAX_TAKE_LANES: usize = 4;
/// An empty or short project still gets a usable scale.
const MIN_SECS: f32 = 10.0;

pub struct OverviewInput<'a> {
    pub session: &'a Session,
    pub playhead: i64,
    /// Song frames the pitch plot shows.
    pub window: (i64, i64),
    pub loop_range: Option<(i64, i64)>,
    pub can_seek: bool,
    pub rate: u32,
    /// Text alternative for screen readers.
    pub summary: String,
}

/// Returns the song frame to jump to when the overview is clicked or dragged.
pub fn show(ui: &mut Ui, input: &OverviewInput) -> Option<i64> {
    let s = input.session;
    let rows = 1 + s.takes.len().clamp(1, MAX_TAKE_LANES);
    let height = TICKS + 2.0 + rows as f32 * (LANE + 2.0) + 2.0;
    let (resp, painter) = ui.allocate_painter(
        Vec2::new(ui.available_width(), height),
        Sense::click_and_drag(),
    );
    let resp = resp.on_hover_cursor(CursorIcon::PointingHand);
    let summary = input.summary.clone();
    resp.widget_info(|| WidgetInfo::labeled(WidgetType::Slider, input.can_seek, &summary));
    let rect = resp.rect;
    painter.rect_filled(rect, 3.0, theme::OVERVIEW_BG);

    let rate = input.rate.max(1) as f32;
    let len = s.song_frames().max((MIN_SECS * rate) as i64);
    let inner = rect.shrink2(Vec2::new(6.0, 0.0));
    let x_of = |f: i64| inner.left() + f.clamp(0, len) as f32 / len as f32 * inner.width();
    let frame_at =
        |x: f32| (((x - inner.left()) / inner.width()).clamp(0.0, 1.0) * len as f32) as i64;
    let span = |a: i64, b: i64| x_of(a)..=x_of(b).max(x_of(a) + 1.0);
    let lane = |row: usize| {
        let y = rect.top() + TICKS + 2.0 + row as f32 * (LANE + 2.0);
        y..=y + LANE
    };

    // Ticks, about every 80 px.
    let secs = len as f32 / rate;
    let step = [1.0f32, 5.0, 10.0, 15.0, 30.0, 60.0, 120.0, 300.0]
        .into_iter()
        .find(|st| st / secs * inner.width() >= 80.0)
        .unwrap_or(600.0);
    let mut t = 0.0;
    while t <= secs {
        let x = x_of((t * rate) as i64);
        painter.line_segment(
            [
                Pos2::new(x, rect.top() + TICKS - 3.0),
                Pos2::new(x, rect.top() + TICKS),
            ],
            Stroke::new(1.0, theme::GRID_C),
        );
        painter.text(
            Pos2::new(x + 2.0, rect.top()),
            Align2::LEFT_TOP,
            format!("{}:{:02}", (t / 60.0) as i64, (t % 60.0) as i64),
            FontId::monospace(9.0),
            theme::GRID_LABEL,
        );
        t += step;
    }

    if let Some((a, b)) = input.loop_range {
        painter.rect_filled(
            Rect::from_x_y_ranges(span(a, b), rect.y_range()),
            0.0,
            theme::LOOP,
        );
    }
    for p in &s.phrases {
        painter.rect_filled(
            Rect::from_x_y_ranges(span(p.start, p.end), lane(0)),
            1.0,
            theme::REFERENCE_GHOST,
        );
    }
    for (i, t) in s.takes.iter().enumerate() {
        let start = t.entry.start_frame;
        painter.rect_filled(
            Rect::from_x_y_ranges(
                span(start, start + t.frames()),
                lane(1 + i % MAX_TAKE_LANES),
            ),
            1.0,
            theme::take_color(t.entry.id),
        );
    }

    // The part of the song the pitch plot shows, and the playhead.
    let (w0, w1) = input.window;
    painter.rect_stroke(
        Rect::from_x_y_ranges(span(w0, w1), rect.top() + 1.0..=rect.bottom() - 1.0),
        2.0,
        Stroke::new(1.0, theme::GRID_LABEL),
        StrokeKind::Inside,
    );
    let x = x_of(input.playhead);
    painter.line_segment(
        [Pos2::new(x, rect.top()), Pos2::new(x, rect.bottom())],
        Stroke::new(2.0, theme::PLAYHEAD),
    );

    resp.interact_pointer_pos()
        .filter(|_| input.can_seek && (resp.clicked() || resp.dragged()))
        .map(|p| frame_at(p.x))
}
