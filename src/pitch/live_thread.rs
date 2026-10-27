//! The live pitch thread: drains the engine's live mic stream, runs the detector on the
//! free-running (wall) frame clock, and tags each estimate with its song frame when playing.

use std::collections::VecDeque;
use std::sync::Arc;
use std::time::Duration;

use crossbeam_channel::Sender;

use super::level::InputLevel;
use super::live::{LiveDetector, LiveFrame};
use crate::audio::engine::{BlockKind, MicStream};
use crate::audio::speed::Ratio;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LivePoint {
    pub wall_frame: i64,
    /// Latency-compensated song frame, when the transport was running.
    pub song_frame: Option<i64>,
    pub hz: Option<f32>,
    pub clarity: f32,
    /// Level of the analysis window, dBFS RMS.
    pub level_db: f32,
}

#[derive(Clone, Copy)]
struct BlockSpan {
    wall: i64,
    song: i64,
    len: i64,
    playing: bool,
    speed: Ratio,
}

/// Song frame of an estimate centred at wall frame `frame`, from the block that captured it:
/// the song clock advanced `p/q` song frames per wall frame during that block.
fn song_frame_of(spans: &VecDeque<BlockSpan>, frame: i64) -> Option<i64> {
    spans
        .iter()
        .rev()
        .find(|s| frame >= s.wall && frame < s.wall + s.len)
        .filter(|s| s.playing)
        .map(|s| s.song + s.speed.to_song(frame - s.wall))
}

/// Spawns the thread; `notify` is called after each batch (e.g. to request a UI repaint).
/// Every mic block also updates `level`, playing or not.
pub fn spawn(
    mut stream: MicStream,
    rate: u32,
    out: Sender<LivePoint>,
    level: Arc<InputLevel>,
    notify: impl Fn() + Send + 'static,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("live-pitch".into())
        .spawn(move || {
            let mut det = LiveDetector::new(rate);
            let mut spans: VecDeque<BlockSpan> = VecDeque::new();
            let mut buf = Vec::new();
            let mut frames: Vec<LiveFrame> = Vec::new();
            loop {
                let mut any = false;
                while let Some(h) = stream.pop(&mut buf) {
                    if h.kind != BlockKind::Data {
                        continue;
                    }
                    any = true;
                    let span = BlockSpan {
                        wall: h.wall_frame as i64,
                        song: h.song_frame,
                        len: h.len as i64,
                        playing: h.playing,
                        speed: h.speed,
                    };
                    spans.push_back(span);
                    while spans.len() > 64 {
                        spans.pop_front();
                    }
                    level.update(&buf, span.wall);
                    frames.clear();
                    det.push(&buf, span.wall, &mut frames);
                    for f in &frames {
                        let song_frame = song_frame_of(&spans, f.frame);
                        let p = LivePoint {
                            wall_frame: f.frame,
                            song_frame,
                            hz: f.hz,
                            clarity: f.clarity,
                            level_db: f.level_db,
                        };
                        if out.send(p).is_err() {
                            return;
                        }
                    }
                }
                if any {
                    notify();
                } else {
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        })
        .expect("spawn live-pitch thread")
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Blocks of 256 wall frames from a clock at `speed`, as the engine stamps them (compensation
    /// left out: it is a constant shift).
    fn blocks(speed: Ratio, count: usize) -> VecDeque<BlockSpan> {
        let mut clock = crate::audio::speed::SongClock::default();
        clock.seek(10_000);
        clock.set_ratio(speed);
        (0..count)
            .map(|i| {
                let s = BlockSpan {
                    wall: i as i64 * 256,
                    song: clock.pos(),
                    len: 256,
                    playing: true,
                    speed,
                };
                clock.advance(256);
                s
            })
            .collect()
    }

    #[test]
    fn slowed_blocks_map_wall_to_song_time() {
        let speed = Ratio::new(3, 5);
        let spans = blocks(speed, 60);
        // Every wall frame lands within two song frames of the exact slowed time: the block stamp
        // leaves out the clock's sub-frame remainder, and the offset inside the block rounds down.
        for wall in (0..60 * 256).step_by(37) {
            let song = song_frame_of(&spans, wall).unwrap();
            let exact = 10_000.0 + wall as f64 * 0.6;
            assert!(
                (song as f64 - exact).abs() < 2.0,
                "{wall}: {song} vs {exact}"
            );
        }
    }

    #[test]
    fn full_speed_is_unchanged() {
        let spans = blocks(Ratio::ONE, 4);
        assert_eq!(song_frame_of(&spans, 700), Some(10_700));
        assert_eq!(song_frame_of(&spans, 5_000), None);
    }
}
