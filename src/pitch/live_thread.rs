//! The live pitch thread: drains the engine's live mic stream, runs the detector on the
//! free-running (wall) frame clock, and tags each estimate with its song frame when playing.

use std::collections::VecDeque;
use std::time::Duration;

use crossbeam_channel::Sender;

use super::live::{LiveDetector, LiveFrame};
use crate::audio::engine::{BlockKind, MicStream};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LivePoint {
    pub wall_frame: i64,
    /// Latency-compensated song frame, when the transport was running.
    pub song_frame: Option<i64>,
    pub hz: Option<f32>,
    pub clarity: f32,
}

#[derive(Clone, Copy)]
struct BlockSpan {
    wall: i64,
    song: i64,
    len: i64,
    playing: bool,
}

/// Spawns the thread; `notify` is called after each batch (e.g. to request a UI repaint).
pub fn spawn(
    mut stream: MicStream,
    rate: u32,
    out: Sender<LivePoint>,
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
                    };
                    spans.push_back(span);
                    while spans.len() > 64 {
                        spans.pop_front();
                    }
                    frames.clear();
                    det.push(&buf, span.wall, &mut frames);
                    for f in &frames {
                        let song_frame = spans
                            .iter()
                            .rev()
                            .find(|s| f.frame >= s.wall && f.frame < s.wall + s.len)
                            .filter(|s| s.playing)
                            .map(|s| s.song + (f.frame - s.wall));
                        let p = LivePoint {
                            wall_frame: f.frame,
                            song_frame,
                            hz: f.hz,
                            clarity: f.clarity,
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
