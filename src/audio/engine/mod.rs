//! The audio engine: one cycle per audio period for capture and playback, the song clock, and the
//! hand-off of mic audio to the live-pitch and disk threads.
//!
//! The per-period work lives in [`core`] and is platform-free; a driver feeds it each period's mic
//! samples and output buffers: JACK on Linux (served by PipeWire through `pipewire-jack`), the
//! interface's ASIO driver on Windows.
//! The cycle never allocates, locks or does I/O: commands and audio arrive through lock-free rings,
//! and replaced [`Playback`] snapshots are handed back to be dropped elsewhere.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32};

use super::mixer::Playback;
use super::speed::Ratio;

#[cfg(windows)]
mod asio;
mod core;
#[cfg(target_os = "linux")]
mod jack;

#[cfg(windows)]
pub use asio::Engine;
#[cfg(target_os = "linux")]
pub use jack::{CLIENT_NAME, Engine, REQUESTED_BUFFER, request_low_latency};

pub enum Command {
    Play,
    Stop,
    Seek(i64),
    StartRecord,
    StopRecord,
    SetPlayback(Arc<Playback>),
    /// Song frames `[start, end)` to repeat while playing (never while recording); `None` clears.
    SetLoop(Option<(i64, i64)>),
    /// Changes the practice speed and swaps in the playback rendered for it, in the same block,
    /// so the clock and the audio never disagree. Ignored while recording.
    SetSpeed {
        speed: Ratio,
        playback: Arc<Playback>,
    },
}

/// State published by the audio thread, readable from any thread.
#[derive(Default)]
pub struct Shared {
    pub song_pos: AtomicI64,
    /// Current practice speed ([`Ratio::to_bits`]).
    pub speed: AtomicU32,
    pub playing: AtomicBool,
    pub recording: AtomicBool,
    /// Frames between output and capture to compensate (latency + calibration).
    pub compensation: AtomicI64,
    pub xruns: AtomicU32,
    /// Mic blocks the live-pitch thread missed (gaps in the live line only).
    pub dropped_live: AtomicU32,
    /// Mic blocks lost from a recording because the disk thread fell behind (lost take audio).
    pub dropped_disk: AtomicU32,
    pub buffer_size: AtomicU32,
    pub server_gone: AtomicBool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    Data,
    TakeStart,
    TakeEnd,
}

/// Describes the samples that follow it in the paired sample ring.
#[derive(Debug, Clone, Copy)]
pub struct BlockHeader {
    pub kind: BlockKind,
    /// Free-running frame counter (advances while stopped too).
    pub wall_frame: u64,
    /// Latency-compensated song frame of the first sample.
    pub song_frame: i64,
    pub len: u32,
    pub playing: bool,
    /// Speed the song clock ran at during this block.
    pub speed: Ratio,
}

/// Consumer side of a mic stream (headers + samples).
pub struct MicStream {
    headers: rtrb::Consumer<BlockHeader>,
    samples: rtrb::Consumer<f32>,
}

impl MicStream {
    /// Pops the next block, replacing `buf` with its samples.
    pub fn pop(&mut self, buf: &mut Vec<f32>) -> Option<BlockHeader> {
        let h = self.headers.pop().ok()?;
        buf.clear();
        if h.len > 0 {
            // The producer writes samples before the header, so they are available.
            if let Ok(chunk) = self.samples.read_chunk(h.len as usize) {
                let (a, b) = chunk.as_slices();
                buf.extend_from_slice(a);
                buf.extend_from_slice(b);
                chunk.commit_all();
            }
        }
        Some(h)
    }
}

/// Mic streams handed to the consumer threads.
pub struct Streams {
    pub live: MicStream,
    pub disk: MicStream,
}

/// What the app's input and outputs are connected to, as last read by the engine.
#[derive(Debug, Clone, Default)]
pub struct Routing {
    /// Full names of the ports feeding the mic input.
    pub input: Vec<String>,
    /// Full names of the ports the left and right outputs play into.
    pub output: Vec<String>,
}

/// The device part of a full port name (`device:port`).
pub fn device_of(port: &str) -> &str {
    port.split_once(':').map_or(port, |(d, _)| d)
}

/// The port part of a full port name.
pub fn short_port(port: &str) -> &str {
    port.split_once(':').map_or(port, |(_, p)| p)
}
