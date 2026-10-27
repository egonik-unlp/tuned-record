//! Take files and the disk writer thread.
//!
//! The disk thread drains the engine's disk stream: a `TakeStart` block opens `take-NNN.wav`
//! (plus a `.start` sidecar holding its song frame, for crash recovery), data blocks are appended
//! and flushed about once a second, and `TakeEnd` finalises the file.

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use crossbeam_channel::{Receiver, Sender, TryRecvError};

use crate::audio::engine::{BlockKind, MicStream};
use crate::feedback::PhraseScore;

pub fn take_path(dir: &Path, id: u32, ext: &str) -> PathBuf {
    dir.join(format!("take-{id:03}.{ext}"))
}

/// `(id, path)` of every `take-NNN.wav` in `dir`, sorted by id.
pub fn list_take_wavs(dir: &Path) -> Result<Vec<(u32, PathBuf)>> {
    let mut v = Vec::new();
    for e in std::fs::read_dir(dir)? {
        let path = e?.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if let Some(id) = name
            .strip_prefix("take-")
            .and_then(|r| r.strip_suffix(".wav"))
            .and_then(|n| n.parse().ok())
        {
            v.push((id, path));
        }
    }
    v.sort();
    Ok(v)
}

pub fn write_start_sidecar(path: &Path, start_frame: i64) -> Result<()> {
    std::fs::write(path, start_frame.to_string())?;
    Ok(())
}

pub fn read_start_sidecar(path: &Path) -> Option<i64> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

pub fn save_scores(path: &Path, scores: &[PhraseScore]) -> Result<()> {
    std::fs::write(path, serde_json::to_string(scores)?)?;
    Ok(())
}

pub fn load_scores(path: &Path) -> Vec<PhraseScore> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str(&s).ok())
        .unwrap_or_default()
}

pub enum DiskControl {
    /// Where the next takes go, and the id to give the next one.
    SetTarget { takes_dir: PathBuf, next_id: u32 },
    /// Capture the next take into memory instead of a file (latency calibration).
    CaptureToMemory,
}

#[derive(Debug)]
pub enum DiskEvent {
    Started {
        id: u32,
        start_frame: i64,
    },
    Finished {
        id: u32,
        start_frame: i64,
        frames: u64,
    },
    Captured {
        start_frame: i64,
        samples: Vec<f32>,
    },
    Error(String),
}

enum Sink {
    None,
    File {
        id: u32,
        start: i64,
        frames: u64,
        since_flush: u64,
        writer: hound::WavWriter<BufWriter<File>>,
    },
    Memory {
        start: i64,
        samples: Vec<f32>,
    },
}

pub fn spawn_disk_thread(
    mut stream: MicStream,
    rate: u32,
    control: Receiver<DiskControl>,
    events: Sender<DiskEvent>,
) -> std::thread::JoinHandle<()> {
    std::thread::Builder::new()
        .name("disk".into())
        .spawn(move || {
            let mut target: Option<(PathBuf, u32)> = None;
            let mut to_memory = false;
            let mut sink = Sink::None;
            let mut buf = Vec::new();
            loop {
                loop {
                    match control.try_recv() {
                        Ok(DiskControl::SetTarget { takes_dir, next_id }) => {
                            target = Some((takes_dir, next_id))
                        }
                        Ok(DiskControl::CaptureToMemory) => to_memory = true,
                        Err(TryRecvError::Empty) => break,
                        Err(TryRecvError::Disconnected) => return,
                    }
                }
                let Some(h) = stream.pop(&mut buf) else {
                    std::thread::sleep(Duration::from_millis(5));
                    continue;
                };
                let result = match h.kind {
                    BlockKind::TakeStart => {
                        if to_memory {
                            to_memory = false;
                            sink = Sink::Memory {
                                start: h.song_frame,
                                samples: Vec::new(),
                            };
                            Ok(())
                        } else {
                            open_take(&mut target, h.song_frame, rate, &events).map(|s| sink = s)
                        }
                    }
                    BlockKind::Data => append(&mut sink, &buf, rate),
                    BlockKind::TakeEnd => finish(std::mem::replace(&mut sink, Sink::None), &events),
                };
                if let Err(e) = result {
                    let _ = events.send(DiskEvent::Error(format!("recording failed: {e}")));
                    sink = Sink::None;
                }
            }
        })
        .expect("spawn disk thread")
}

fn open_take(
    target: &mut Option<(PathBuf, u32)>,
    start: i64,
    rate: u32,
    events: &Sender<DiskEvent>,
) -> Result<Sink> {
    let Some((dir, next_id)) = target.as_mut() else {
        anyhow::bail!("no project open");
    };
    let id = *next_id;
    *next_id += 1;
    write_start_sidecar(&take_path(dir, id, "start"), start)?;
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: rate,
        bits_per_sample: 32,
        sample_format: hound::SampleFormat::Float,
    };
    let writer = hound::WavWriter::create(take_path(dir, id, "wav"), spec)?;
    let _ = events.send(DiskEvent::Started {
        id,
        start_frame: start,
    });
    Ok(Sink::File {
        id,
        start,
        frames: 0,
        since_flush: 0,
        writer,
    })
}

fn append(sink: &mut Sink, samples: &[f32], rate: u32) -> Result<()> {
    match sink {
        Sink::None => {}
        Sink::Memory { samples: m, .. } => m.extend_from_slice(samples),
        Sink::File {
            frames,
            since_flush,
            writer,
            ..
        } => {
            for &s in samples {
                writer.write_sample(s)?;
            }
            *frames += samples.len() as u64;
            *since_flush += samples.len() as u64;
            if *since_flush >= rate as u64 {
                // Rewrites the header so the file is valid up to here if the app dies.
                writer.flush()?;
                *since_flush = 0;
            }
        }
    }
    Ok(())
}

fn finish(sink: Sink, events: &Sender<DiskEvent>) -> Result<()> {
    match sink {
        Sink::None => {}
        Sink::Memory { start, samples } => {
            let _ = events.send(DiskEvent::Captured {
                start_frame: start,
                samples,
            });
        }
        Sink::File {
            id,
            start,
            frames,
            writer,
            ..
        } => {
            writer.finalize()?;
            let _ = events.send(DiskEvent::Finished {
                id,
                start_frame: start,
                frames,
            });
        }
    }
    Ok(())
}
