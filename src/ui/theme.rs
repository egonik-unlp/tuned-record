//! Colours. Dark theme; take colours from a colour-blind-safe categorical palette (Okabe–Ito,
//! lightened for a dark background). Deviation is never shown by colour alone (see readout).

use egui::Color32;

use crate::feedback::Band;

pub const BG: Color32 = Color32::from_rgb(0x16, 0x18, 0x1c);
pub const GRID: Color32 = Color32::from_rgb(0x2a, 0x2e, 0x35);
pub const GRID_C: Color32 = Color32::from_rgb(0x45, 0x4b, 0x56);
pub const GRID_LABEL: Color32 = Color32::from_rgb(0xb4, 0xba, 0xc4);
pub const CURRENT_ROW: Color32 = Color32::from_rgba_premultiplied(0x30, 0x34, 0x3c, 0x60);
pub const PLAYHEAD: Color32 = Color32::from_rgb(0xe8, 0xea, 0xee);
pub const REFERENCE: Color32 = Color32::from_rgb(0xd6, 0xd9, 0xde);
pub const REFERENCE_GHOST: Color32 = Color32::from_rgb(0x9a, 0xa0, 0xaa);
/// The raw target contour under the note bars (secondary: below 4.5:1 on purpose).
pub const CONTOUR_DIM: Color32 = Color32::from_rgba_unmultiplied_const(0xd6, 0xd9, 0xde, 0x70);
pub const CONTOUR_DIM_GHOST: Color32 =
    Color32::from_rgba_unmultiplied_const(0x9a, 0xa0, 0xaa, 0x60);
/// Note bars: passed (solid outline), upcoming (dashed outline), reached (filled + ✓).
pub const BAR: Color32 = Color32::from_rgb(0xd6, 0xd9, 0xde);
pub const BAR_GHOST: Color32 = Color32::from_rgb(0x9a, 0xa0, 0xaa);
pub const BAR_REACHED: Color32 = Color32::from_rgb(0x4c, 0xd9, 0x64);
pub const LIVE_NEUTRAL: Color32 = Color32::from_rgb(0xf4, 0xf4, 0xf4);
pub const IN_TUNE: Color32 = Color32::from_rgb(0x4c, 0xd9, 0x64);
pub const NEAR: Color32 = Color32::from_rgb(0xff, 0xb0, 0x00);
pub const OFF: Color32 = Color32::from_rgb(0xff, 0x55, 0x55);
pub const RECORD: Color32 = Color32::from_rgb(0xe5, 0x39, 0x35);
pub const TEXT_DIM: Color32 = Color32::from_rgb(0x9a, 0xa0, 0xaa);
/// The note to sing now (marker at the playhead, its row label).
pub const TARGET: Color32 = Color32::from_rgb(0xff, 0xff, 0xff);
/// Corridor around the target: within the in-tune and near limits.
pub const BAND_IN_TUNE: Color32 = Color32::from_rgba_unmultiplied_const(0x4c, 0xd9, 0x64, 0x30);
pub const BAND_NEAR: Color32 = Color32::from_rgba_unmultiplied_const(0x9a, 0xa0, 0xaa, 0x1c);
/// The untransposed melody while the guide is transposed for a harmony.
pub const MELODY: Color32 = Color32::from_rgb(0x6a, 0x70, 0x7a);
pub const LOOP: Color32 = Color32::from_rgba_unmultiplied_const(0x56, 0xb4, 0xe9, 0x1c);
pub const OVERVIEW_BG: Color32 = Color32::from_rgb(0x1e, 0x21, 0x26);

const TAKES: [Color32; 7] = [
    Color32::from_rgb(0x56, 0xb4, 0xe9), // sky blue
    Color32::from_rgb(0xcc, 0x79, 0xa7), // reddish purple
    Color32::from_rgb(0xe6, 0x9f, 0x00), // orange
    Color32::from_rgb(0x6f, 0xa8, 0xff), // blue (lightened)
    Color32::from_rgb(0xf0, 0xe4, 0x42), // yellow
    Color32::from_rgb(0xd5, 0x5e, 0x00), // vermillion
    Color32::from_rgb(0x33, 0xc4, 0x9e), // bluish green (lightened)
];

pub fn take_color(id: u32) -> Color32 {
    TAKES[(id.saturating_sub(1) as usize) % TAKES.len()]
}

pub fn band_color(b: Band) -> Color32 {
    match b {
        Band::InTune => IN_TUNE,
        Band::Near => NEAR,
        Band::Off => OFF,
    }
}
