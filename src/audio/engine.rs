//! JACK client (served by PipeWire through `pipewire-jack`): one process callback for capture and
//! playback, the song clock, and the hand-off of mic audio to the live-pitch and disk threads.
//!
//! The callback never allocates, locks or does I/O: commands and audio arrive through lock-free
//! rings, and replaced [`Playback`] snapshots are handed back to be dropped elsewhere.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI64, AtomicU32, Ordering::Relaxed};

use jack::{
    AudioIn, AudioOut, Client, ClientOptions, Control, LatencyType, Port, PortFlags, ProcessScope,
};

use super::latency::compensation;
use super::mixer::Playback;

pub const CLIENT_NAME: &str = "tuned-record";
/// Buffer size requested from PipeWire (`PIPEWIRE_LATENCY`), set before the client opens.
pub const REQUESTED_BUFFER: u32 = 256;
const COMMAND_CAPACITY: usize = 256;
const HEADER_CAPACITY: usize = 4096;
const LIVE_SECONDS: usize = 2;
const DISK_SECONDS: usize = 10;

pub enum Command {
    Play,
    Stop,
    Seek(i64),
    StartRecord,
    StopRecord,
    SetPlayback(Arc<Playback>),
    /// Song frames `[start, end)` to repeat while playing (never while recording); `None` clears.
    SetLoop(Option<(i64, i64)>),
}

/// State published by the audio thread, readable from any thread.
#[derive(Default)]
pub struct Shared {
    pub song_pos: AtomicI64,
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

struct StreamProducer {
    headers: rtrb::Producer<BlockHeader>,
    samples: rtrb::Producer<f32>,
}

impl StreamProducer {
    /// Writes samples then the header; returns false (writing nothing) if either ring is full.
    fn push(&mut self, header: BlockHeader, samples: &[f32]) -> bool {
        if self.headers.slots() == 0 || self.samples.slots() < samples.len() {
            return false;
        }
        if let Ok(mut chunk) = self.samples.write_chunk_uninit(samples.len()) {
            let (a, b) = chunk.as_mut_slices();
            let split = a.len();
            for (d, s) in a.iter_mut().zip(samples) {
                d.write(*s);
            }
            for (d, s) in b.iter_mut().zip(&samples[split..]) {
                d.write(*s);
            }
            // SAFETY: all `samples.len()` slots were initialised above.
            unsafe { chunk.commit_all() };
        }
        self.headers.push(header).is_ok()
    }
}

fn stream(capacity: usize) -> (StreamProducer, MicStream) {
    let (hp, hc) = rtrb::RingBuffer::new(HEADER_CAPACITY);
    let (sp, sc) = rtrb::RingBuffer::new(capacity);
    (
        StreamProducer {
            headers: hp,
            samples: sp,
        },
        MicStream {
            headers: hc,
            samples: sc,
        },
    )
}

struct Processor {
    input: Port<AudioIn>,
    out_l: Port<AudioOut>,
    out_r: Port<AudioOut>,
    shared: Arc<Shared>,
    commands: rtrb::Consumer<Command>,
    garbage: rtrb::Producer<Arc<Playback>>,
    playback: Arc<Playback>,
    live: StreamProducer,
    disk: StreamProducer,
    pos: i64,
    wall: u64,
    playing: bool,
    recording: bool,
    loop_range: Option<(i64, i64)>,
}

impl Processor {
    fn header(&self, kind: BlockKind, song_frame: i64, len: usize) -> BlockHeader {
        BlockHeader {
            kind,
            wall_frame: self.wall,
            song_frame,
            len: len as u32,
            playing: self.playing,
        }
    }

    fn stop_recording(&mut self, song_frame: i64) {
        if self.recording {
            self.recording = false;
            let h = self.header(BlockKind::TakeEnd, song_frame, 0);
            // The disk thread must see the end marker; retry is impossible here, so count a drop.
            if !self.disk.push(h, &[]) {
                self.shared.dropped_disk.fetch_add(1, Relaxed);
            }
        }
    }

    fn handle_commands(&mut self, comp: i64) {
        while let Ok(cmd) = self.commands.pop() {
            match cmd {
                Command::Play => self.playing = true,
                Command::Stop => {
                    self.stop_recording(self.pos - comp);
                    self.playing = false;
                }
                Command::Seek(f) => {
                    if !self.recording {
                        self.pos = f;
                    }
                }
                Command::StartRecord => {
                    if !self.recording {
                        let h = self.header(BlockKind::TakeStart, self.pos - comp, 0);
                        self.recording = self.disk.push(h, &[]);
                    }
                }
                Command::StopRecord => self.stop_recording(self.pos - comp),
                Command::SetLoop(range) => self.loop_range = range.filter(|(s, e)| s < e),
                Command::SetPlayback(new) => {
                    let old = std::mem::replace(&mut self.playback, new);
                    if let Err(rtrb::PushError::Full(old)) = self.garbage.push(old) {
                        // Never free on the audio thread; leaking is the lesser evil.
                        std::mem::forget(old);
                    }
                }
            }
        }
    }
}

impl jack::ProcessHandler for Processor {
    fn process(&mut self, _: &Client, ps: &ProcessScope) -> Control {
        let comp = self.shared.compensation.load(Relaxed);
        self.handle_commands(comp);
        let n = ps.n_frames() as usize;

        let out_l = self.out_l.as_mut_slice(ps);
        let out_r = self.out_r.as_mut_slice(ps);
        out_l.fill(0.0);
        out_r.fill(0.0);
        if self.playing {
            self.playback.mix_into(self.pos, out_l, out_r);
        }

        let mic = self.input.as_slice(ps);
        let song_first = self.pos - comp;
        let h = self.header(BlockKind::Data, song_first, n);
        if !self.live.push(h, mic) {
            self.shared.dropped_live.fetch_add(1, Relaxed);
        }
        if self.recording && self.playing && !self.disk.push(h, mic) {
            self.shared.dropped_disk.fetch_add(1, Relaxed);
        }

        if self.playing {
            self.pos += n as i64;
            if let Some((start, end)) = self.loop_range
                && !self.recording
                && self.pos >= end
            {
                self.pos = start;
            }
        }
        self.wall += n as u64;
        self.shared.song_pos.store(self.pos, Relaxed);
        self.shared.playing.store(self.playing, Relaxed);
        self.shared.recording.store(self.recording, Relaxed);
        Control::Continue
    }

    fn buffer_size(&mut self, _: &Client, size: jack::Frames) -> Control {
        self.shared.buffer_size.store(size, Relaxed);
        Control::Continue
    }
}

struct Notifications {
    shared: Arc<Shared>,
}

impl jack::NotificationHandler for Notifications {
    unsafe fn shutdown(&mut self, _: jack::ClientStatus, _: &str) {
        self.shared.server_gone.store(true, Relaxed);
    }

    fn xrun(&mut self, _: &Client) -> Control {
        self.shared.xruns.fetch_add(1, Relaxed);
        Control::Continue
    }
}

pub struct Engine {
    client: jack::AsyncClient<Notifications, Processor>,
    pub shared: Arc<Shared>,
    commands: rtrb::Producer<Command>,
    garbage: rtrb::Consumer<Arc<Playback>>,
    pub sample_rate: u32,
    in_port: String,
    out_ports: [String; 2],
    pub playback_latency: u32,
    pub capture_latency: u32,
    pub routing: Routing,
}

/// What the app's ports are connected to, as last read by [`Engine::refresh_routing`].
#[derive(Debug, Clone, Default)]
pub struct Routing {
    /// Full names of the ports feeding `mic_in`.
    pub input: Vec<String>,
    /// Full names of the ports `out_l` and `out_r` play into.
    pub output: Vec<String>,
}

/// The JACK client (device) part of a full port name.
pub fn device_of(port: &str) -> &str {
    port.split_once(':').map_or(port, |(d, _)| d)
}

/// The port part of a full port name.
pub fn short_port(port: &str) -> &str {
    port.split_once(':').map_or(port, |(_, p)| p)
}

/// Mic streams handed to the consumer threads.
pub struct Streams {
    pub live: MicStream,
    pub disk: MicStream,
}

/// Loads PipeWire's `libjack` (from `pipewire-jack`) so the app runs without the `pw-jack`
/// wrapper: the `jack` crate later opens `libjack.so.0` by name, and the dynamic loader hands back
/// the already-loaded library with that soname instead of the stock one. Without `pipewire-jack`
/// nothing is loaded and the system `libjack` is used as before.
fn preload_pipewire_jack() {
    static ONCE: std::sync::Once = std::sync::Once::new();
    ONCE.call_once(|| {
        let candidates = [
            // Debian/Ubuntu multiarch, Fedora/openSUSE 64-bit, Arch and others.
            format!(
                "/usr/lib/{}-linux-gnu/pipewire-0.3/jack/libjack.so.0",
                std::env::consts::ARCH
            ),
            "/usr/lib64/pipewire-0.3/jack/libjack.so.0".to_string(),
            "/usr/lib/pipewire-0.3/jack/libjack.so.0".to_string(),
        ];
        for path in candidates {
            // SAFETY: libjack's initialisers have no preconditions; the handle is kept for the
            // whole process, as the `jack` crate's own handle is.
            if let Ok(lib) = unsafe { libloading::Library::new(&path) } {
                log::info!("using PipeWire's JACK library {path}");
                std::mem::forget(lib);
                return;
            }
        }
    });
}

/// Must be called before any thread is spawned: asks PipeWire's JACK layer for small buffers.
pub fn request_low_latency() {
    if std::env::var_os("PIPEWIRE_LATENCY").is_none() {
        // SAFETY: called at the start of `main`, before other threads exist.
        unsafe { std::env::set_var("PIPEWIRE_LATENCY", format!("{REQUESTED_BUFFER}/48000")) };
    }
}

impl Engine {
    /// Starts on the default devices.
    pub fn start() -> Result<(Self, Streams), String> {
        Self::start_routed(None, None)
    }

    /// Starts recording from `input_port` and playing into `output_device` when present (see
    /// [`Engine::keep_routed`]).
    pub fn start_routed(
        input_port: Option<&str>,
        output_device: Option<&str>,
    ) -> Result<(Self, Streams), String> {
        preload_pipewire_jack();
        let (client, _) =
            Client::new(CLIENT_NAME, ClientOptions::NO_START_SERVER).map_err(|e| {
                format!(
                    "Audio could not start: no JACK server reachable ({e}). \
                 This app needs PipeWire's JACK support: install `pipewire-jack`."
                )
            })?;
        let sample_rate = client.sample_rate();
        let err = |e: jack::Error| format!("Audio could not start: {e}");
        let input = client
            .register_port("mic_in", AudioIn::default())
            .map_err(err)?;
        let out_l = client
            .register_port("out_l", AudioOut::default())
            .map_err(err)?;
        let out_r = client
            .register_port("out_r", AudioOut::default())
            .map_err(err)?;
        let in_port = input.name().map_err(err)?;
        let out_ports = [out_l.name().map_err(err)?, out_r.name().map_err(err)?];

        let shared = Arc::new(Shared::default());
        shared.buffer_size.store(client.buffer_size(), Relaxed);
        let (cmd_tx, cmd_rx) = rtrb::RingBuffer::new(COMMAND_CAPACITY);
        let (garbage_tx, garbage_rx) = rtrb::RingBuffer::new(COMMAND_CAPACITY);
        let (live_tx, live_rx) = stream(sample_rate as usize * LIVE_SECONDS);
        let (disk_tx, disk_rx) = stream(sample_rate as usize * DISK_SECONDS);

        let processor = Processor {
            input,
            out_l,
            out_r,
            shared: shared.clone(),
            commands: cmd_rx,
            garbage: garbage_tx,
            playback: Arc::new(Playback::default()),
            live: live_tx,
            disk: disk_tx,
            pos: 0,
            wall: 0,
            playing: false,
            recording: false,
            loop_range: None,
        };
        let client = client
            .activate_async(
                Notifications {
                    shared: shared.clone(),
                },
                processor,
            )
            .map_err(err)?;

        let mut engine = Self {
            client,
            shared,
            commands: cmd_tx,
            garbage: garbage_rx,
            sample_rate,
            in_port,
            out_ports,
            playback_latency: 0,
            capture_latency: 0,
            routing: Routing::default(),
        };
        engine.keep_routed(input_port, output_device);
        Ok((
            engine,
            Streams {
                live: live_rx,
                disk: disk_rx,
            },
        ))
    }

    /// Mono audio ports with `flags` of clients other than this app (or another instance of it),
    /// physical (hardware) ones first.
    fn foreign_ports(&self, flags: PortFlags) -> Vec<String> {
        let c = self.client.as_client();
        let audio = Some("32 bit float mono audio");
        let mut ports = c.ports(None, audio, flags | PortFlags::IS_PHYSICAL);
        for p in c.ports(None, audio, flags) {
            if !ports.contains(&p) {
                ports.push(p);
            }
        }
        ports.retain(|p| !p.starts_with(CLIENT_NAME));
        ports
    }

    /// Ports the mic input can record from. Sink monitors are left out: they carry the playback,
    /// which the live detector would follow instead of the voice.
    pub fn input_sources(&self) -> Vec<String> {
        let mut ports = self.foreign_ports(PortFlags::IS_OUTPUT);
        ports.retain(|p| !short_port(p).starts_with("monitor_"));
        ports
    }

    /// Devices (clients with audio inputs) the output can play into.
    pub fn output_devices(&self) -> Vec<String> {
        let mut devices: Vec<String> = Vec::new();
        for p in self.foreign_ports(PortFlags::IS_INPUT) {
            let d = device_of(&p);
            if !devices.iter().any(|x| x == d) {
                devices.push(d.to_string());
            }
        }
        devices
    }

    /// Records from `port` alone.
    pub fn set_input(&mut self, port: &str) -> Result<(), String> {
        let c = self.client.as_client();
        // Every candidate, not just `get_connections`: a link made moments ago may not be listed.
        for p in self.foreign_ports(PortFlags::IS_OUTPUT) {
            let _ = c.disconnect_ports_by_name(&p, &self.in_port);
        }
        let r = c
            .connect_ports_by_name(port, &self.in_port)
            .map_err(|e| format!("Could not record from {port}: {e}"));
        self.refresh_routing();
        r
    }

    /// Plays into `device` alone: left and right into its first two inputs (both into the first
    /// if it has only one).
    pub fn set_output(&mut self, device: &str) -> Result<(), String> {
        let c = self.client.as_client();
        let mut sinks = self.foreign_ports(PortFlags::IS_INPUT);
        for ours in &self.out_ports {
            for p in &sinks {
                let _ = c.disconnect_ports_by_name(ours, p);
            }
        }
        sinks.retain(|p| device_of(p) == device);
        let mut r = if sinks.is_empty() {
            Err(format!("{device} has no audio inputs"))
        } else {
            Ok(())
        };
        for (i, ours) in self.out_ports.iter().enumerate() {
            if let Some(dst) = sinks.get(i).or(sinks.first())
                && let Err(e) = c.connect_ports_by_name(ours, dst)
            {
                r = Err(format!("Could not play into {dst}: {e}"));
            }
        }
        self.refresh_routing();
        r
    }

    /// Re-reads what the app's ports are connected to.
    pub fn refresh_routing(&mut self) {
        let c = self.client.as_client();
        let conns = |name: &str| {
            c.port_by_name(name)
                .map_or(Vec::new(), |p| p.get_connections())
        };
        self.routing.input = conns(&self.in_port);
        self.routing.output = self.out_ports.iter().flat_map(|p| conns(p)).collect();
    }

    /// Moves the input (output) to the preferred port (device) when it is present and not in use,
    /// and connects an unconnected side to the first available one, e.g. after the device it used
    /// disappeared and came back.
    pub fn keep_routed(&mut self, input: Option<&str>, output: Option<&str>) {
        self.refresh_routing();
        let sources = self.input_sources();
        let want_in = input.filter(|p| sources.iter().any(|s| s == p));
        match want_in {
            Some(p) if !self.routing.input.iter().any(|s| s == p) => {
                let _ = self.set_input(p);
            }
            None if self.routing.input.is_empty() => {
                if let Some(p) = sources.first() {
                    let _ = self.set_input(p);
                }
            }
            _ => {}
        }
        let devices = self.output_devices();
        let want_out = output.filter(|d| devices.iter().any(|x| x == d));
        match want_out {
            Some(d) if !self.routing.output.iter().any(|p| device_of(p) == d) => {
                let _ = self.set_output(d);
            }
            None if self.routing.output.is_empty() => {
                if let Some(d) = devices.first() {
                    let _ = self.set_output(d);
                }
            }
            _ => {}
        }
    }

    /// Re-reads the reported port latencies and publishes the compensation (with `calibration`).
    pub fn refresh_latency(&mut self, calibration: i64) {
        let c = self.client.as_client();
        if let Some(p) = c.port_by_name(&self.in_port) {
            self.capture_latency = p.get_latency_range(LatencyType::Capture).1;
        }
        if let Some(p) = c.port_by_name(&self.out_ports[0]) {
            self.playback_latency = p.get_latency_range(LatencyType::Playback).1;
        }
        self.shared.compensation.store(
            compensation(self.playback_latency, self.capture_latency, calibration),
            Relaxed,
        );
    }

    /// The JACK client, for inspecting or rewiring connections.
    pub fn client(&self) -> &Client {
        self.client.as_client()
    }

    /// Full names of the mic input port and the two output ports.
    pub fn port_names(&self) -> (&str, &[String; 2]) {
        (&self.in_port, &self.out_ports)
    }

    pub fn send(&mut self, cmd: Command) -> bool {
        self.commands.push(cmd).is_ok()
    }

    /// Drops playback snapshots retired by the audio thread.
    pub fn collect_garbage(&mut self) {
        while self.garbage.pop().is_ok() {}
    }

    pub fn buffer_size(&self) -> u32 {
        self.shared.buffer_size.load(Relaxed)
    }

    /// Round-trip latency in ms as currently compensated.
    pub fn latency_ms(&self) -> f32 {
        self.shared.compensation.load(Relaxed) as f32 * 1000.0 / self.sample_rate as f32
    }
}
