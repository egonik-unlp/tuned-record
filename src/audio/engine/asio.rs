//! ASIO driver (Windows, cpal's `asio` host): one interface driver carries both the mic and the
//! outputs. Within each ASIO period the input callback hands the chosen channel to the output
//! callback through a ring, and the output callback runs the shared [`Cycle`].
//!
//! A supervisor thread owns every cpal object (ASIO drivers belong to the thread that loads them).
//! It rebuilds the streams after a driver reset or loss and keeps the cycle — and with it the
//! transport — across rebuilds; a take interrupted that way is closed where the audio stopped.

use std::sync::atomic::Ordering::{Acquire, Relaxed, Release};
use std::sync::atomic::{AtomicPtr, AtomicU32, AtomicU64, AtomicUsize};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{BufferSize, ErrorKind, FromSample, SampleFormat, SizedSample, StreamConfig};
use crossbeam_channel::{Receiver, Sender, select, unbounded};

use super::core::Cycle;
use super::{Command, Routing, Shared, Streams, device_of, short_port};
use crate::audio::latency::compensation;
use crate::audio::mixer::Playback;
use crate::audio::speed::Ratio;

/// Most frames handed to the cycle at once; longer driver buffers are processed in chunks.
const SCRATCH: usize = 8192;
/// Mic frames the ring between the input and output callbacks can hold.
const MIC_RING: usize = 4 * SCRATCH;
/// Streams running but no output period for this long: the driver stopped, rebuild.
const STALL: Duration = Duration::from_millis(500);
/// Pause between attempts to reopen a driver that is gone.
const RETRY: Duration = Duration::from_secs(1);
const TICK: Duration = Duration::from_millis(100);

/// Owns the [`Cycle`] between periods. The output callback takes it for the period and puts it
/// back; the supervisor takes it only after dropping the streams, when no callback can run (cpal
/// unregisters a stream's callback under the same lock asio-sys holds while dispatching). An
/// atomic pointer rather than a lock keeps the callback lock-free.
struct CycleSlot(AtomicPtr<Cycle>);

impl CycleSlot {
    fn new(cycle: Cycle) -> Self {
        Self(AtomicPtr::new(Box::into_raw(Box::new(cycle))))
    }

    fn take(&self) -> Option<Box<Cycle>> {
        let p = self.0.swap(std::ptr::null_mut(), Acquire);
        // SAFETY: a non-null pointer came from `Box::into_raw` (`new` or `put`), and the swap made
        // this caller its only owner.
        (!p.is_null()).then(|| unsafe { Box::from_raw(p) })
    }

    fn put(&self, cycle: Box<Cycle>) {
        let old = self.0.swap(Box::into_raw(cycle), Release);
        debug_assert!(old.is_null(), "two owners of the cycle");
    }
}

impl Drop for CycleSlot {
    fn drop(&mut self) {
        drop(self.take());
    }
}

/// What the callbacks publish (lock-free), read by the supervisor and the engine.
#[derive(Default)]
struct Live {
    /// Output periods run; the watchdog watches it move.
    heartbeat: AtomicU64,
    /// Callback instant (ns) of the last input period, to tell whether it ran before the output
    /// callback of the same period.
    input_instant: AtomicU64,
    capture_latency: AtomicU32,
    playback_latency: AtomicU32,
    /// Extra capture delay when the mic block used is one period old (output ran first), frames.
    lag: AtomicU32,
    /// 0-based input channel recorded from.
    channel: AtomicUsize,
    /// Output periods that found no mic block to pair with.
    mic_underruns: AtomicU32,
}

/// One ASIO driver as last enumerated.
#[derive(Debug, Clone)]
struct DriverInfo {
    name: String,
    ins: u16,
    outs: u16,
    /// Current rate, 0 when the driver doesn't say until it runs.
    rate: u32,
}

/// What the supervisor publishes for the engine (UI thread only, never the audio thread).
#[derive(Default)]
struct Status {
    drivers: Vec<DriverInfo>,
    /// Driver the supervisor runs on, or tries to.
    want: String,
    routing: Routing,
    waiting: Option<String>,
    switching: Option<String>,
    interrupted_takes: u32,
}

enum Request {
    Open { driver: String, channel: usize },
    Rescan,
    Shutdown,
}

struct Running {
    driver: String,
    _input: cpal::Stream,
    _output: cpal::Stream,
    beat: u64,
    beat_at: Instant,
}

struct Started {
    shared: Arc<Shared>,
    control: super::core::Control,
    streams: Streams,
    rate: u32,
}

struct Supervisor {
    host: cpal::Host,
    devices: Vec<cpal::Device>,
    rate: u32,
    slot: Arc<CycleSlot>,
    shared: Arc<Shared>,
    live: Arc<Live>,
    status: Arc<Mutex<Status>>,
    errors_tx: Sender<cpal::Error>,
    errors: Receiver<cpal::Error>,
    running: Option<Running>,
    next_try: Instant,
}

fn lock(status: &Mutex<Status>) -> MutexGuard<'_, Status> {
    status.lock().unwrap_or_else(|e| e.into_inner())
}

fn input_name(driver: &str, channel: usize) -> String {
    format!("{driver}:in {}", channel + 1)
}

/// 0-based channel of an input name (`driver:in N`).
fn channel_of(port: &str) -> Option<usize> {
    short_port(port)
        .strip_prefix("in ")?
        .parse::<usize>()
        .ok()?
        .checked_sub(1)
}

fn frames(d: Duration, rate: u32) -> u32 {
    (d.as_nanos() * rate as u128 / 1_000_000_000) as u32
}

fn nanos(t: cpal::StreamInstant) -> u64 {
    t.duration_since(cpal::StreamInstant::ZERO).as_nanos() as u64
}

fn info_of(d: &cpal::Device) -> Option<DriverInfo> {
    let name = d.description().ok()?.name().to_string();
    let ins = d.default_input_config().map_or(0, |c| c.channels());
    let outs = d.default_output_config().map_or(0, |c| c.channels());
    let rate = d.default_output_config().map_or(0, |c| c.sample_rate());
    Some(DriverInfo {
        name,
        ins,
        outs,
        rate,
    })
}

/// Usable for the app: something to record from and something to play into.
fn usable(d: &DriverInfo) -> bool {
    d.ins > 0 && d.outs > 0
}

fn input_stream<T>(
    device: &cpal::Device,
    config: StreamConfig,
    mut ring: rtrb::Producer<f32>,
    live: Arc<Live>,
    on_error: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample,
    f32: FromSample<T>,
{
    let channels = config.channels.max(1) as usize;
    let rate = config.sample_rate;
    device.build_input_stream::<T, _, _>(
        config,
        move |data: &[T], info: &cpal::InputCallbackInfo| {
            let ts = info.timestamp();
            if let Some(d) = ts.callback.checked_duration_since(ts.capture) {
                live.capture_latency.store(frames(d, rate), Relaxed);
            }
            let ch = live.channel.load(Relaxed).min(channels - 1);
            let n = (data.len() / channels).min(ring.slots());
            if let Ok(mut chunk) = ring.write_chunk_uninit(n) {
                let (a, b) = chunk.as_mut_slices();
                for (d, f) in a.iter_mut().chain(b).zip(data.chunks_exact(channels)) {
                    d.write(f[ch].to_sample::<f32>());
                }
                // SAFETY: the zip above initialised all `n` slots (`n` ≤ the frames in `data`).
                unsafe { chunk.commit_all() };
            }
            live.input_instant.store(nanos(ts.callback), Release);
        },
        on_error,
        None,
    )
}

/// Scratch for the output callback, allocated when the stream is built.
struct Scratch {
    mic: Vec<f32>,
    l: Vec<f32>,
    r: Vec<f32>,
}

#[allow(clippy::too_many_arguments)]
fn output_stream<T>(
    device: &cpal::Device,
    config: StreamConfig,
    mut ring: rtrb::Consumer<f32>,
    slot: Arc<CycleSlot>,
    shared: Arc<Shared>,
    live: Arc<Live>,
    on_error: impl FnMut(cpal::Error) + Send + 'static,
) -> Result<cpal::Stream, cpal::Error>
where
    T: SizedSample + FromSample<f32>,
{
    let channels = config.channels.max(1) as usize;
    let rate = config.sample_rate;
    let mut s = Scratch {
        mic: vec![0.0; SCRATCH],
        l: vec![0.0; SCRATCH],
        r: vec![0.0; SCRATCH],
    };
    device.build_output_stream::<T, _, _>(
        config,
        move |data: &mut [T], info: &cpal::OutputCallbackInfo| {
            let ts = info.timestamp();
            if let Some(d) = ts.playback.checked_duration_since(ts.callback) {
                live.playback_latency.store(frames(d, rate), Relaxed);
            }
            let n = data.len() / channels;
            shared.buffer_size.store(n as u32, Relaxed);
            // The input callback of this period normally ran just before (it was registered
            // first). If not, the freshest mic block is one period old: use it, and say so.
            let fresh = live.input_instant.load(Acquire) == nanos(ts.callback);
            live.lag.store(if fresh { 0 } else { n as u32 }, Relaxed);
            // Pair with the newest block only: drop anything older (a start-up remainder, when
            // one stream started a period before the other), so the lag stays at most a period.
            let stale = ring.slots().saturating_sub(n);
            if stale > 0
                && let Ok(chunk) = ring.read_chunk(stale)
            {
                chunk.commit_all();
            }

            let Some(mut cycle) = slot.take() else {
                data.fill(T::EQUILIBRIUM);
                return;
            };
            let mut done = 0;
            while done < n {
                let m = (n - done).min(SCRATCH);
                let got = ring.slots().min(m);
                if let Ok(chunk) = ring.read_chunk(got) {
                    let (a, b) = chunk.as_slices();
                    s.mic[..a.len()].copy_from_slice(a);
                    s.mic[a.len()..got].copy_from_slice(b);
                    chunk.commit_all();
                }
                if got < m {
                    s.mic[got..m].fill(0.0);
                    live.mic_underruns.fetch_add(1, Relaxed);
                }
                cycle.process(&s.mic[..m], &mut s.l[..m], &mut s.r[..m]);
                let out = &mut data[done * channels..(done + m) * channels];
                for (k, f) in out.chunks_exact_mut(channels).enumerate() {
                    f[0] = T::from_sample(s.l[k]);
                    if channels > 1 {
                        f[1] = T::from_sample(s.r[k]);
                    }
                    for x in f.iter_mut().skip(2) {
                        *x = T::EQUILIBRIUM;
                    }
                }
                done += m;
            }
            slot.put(cycle);
            live.heartbeat.fetch_add(1, Release);
        },
        on_error,
        None,
    )
}

impl Supervisor {
    fn device(&self, name: &str) -> Option<&cpal::Device> {
        self.devices
            .iter()
            .find(|d| d.description().is_ok_and(|x| x.name() == name))
    }

    /// Re-reads the drivers. While one is in use the others can't be loaded, so drivers that
    /// don't answer now keep what was learned about them before.
    fn enumerate(&mut self) {
        let found: Vec<cpal::Device> = self
            .host
            .devices()
            .map(|it| it.collect())
            .unwrap_or_default();
        let names: Vec<String> = found.iter().filter_map(info_of).map(|i| i.name).collect();
        let mut devices = found;
        for old in self.devices.drain(..) {
            if info_of(&old).is_some_and(|i| !names.contains(&i.name)) {
                devices.push(old);
            }
        }
        self.devices = devices;
        let infos = self.devices.iter().filter_map(info_of).collect();
        lock(&self.status).drivers = infos;
    }

    fn error_handler(&self) -> impl FnMut(cpal::Error) + Send + 'static {
        let tx = self.errors_tx.clone();
        let shared = self.shared.clone();
        move |e: cpal::Error| {
            if e.kind() == ErrorKind::Xrun {
                shared.xruns.fetch_add(1, Relaxed);
            } else {
                let _ = tx.send(e);
            }
        }
    }

    /// Builds and starts the streams on `driver`, input first so its callback runs first.
    fn open(&mut self, driver: &str) -> Result<(), String> {
        let device = self
            .device(driver)
            .cloned()
            .ok_or_else(|| format!("{driver} is not installed"))?;
        let in_cfg = device.default_input_config().map_err(|e| e.to_string())?;
        let out_cfg = device.default_output_config().map_err(|e| e.to_string())?;
        let config = |channels| StreamConfig {
            channels,
            sample_rate: self.rate,
            buffer_size: BufferSize::Default,
        };
        let (tx, rx) = rtrb::RingBuffer::new(MIC_RING);
        let live = self.live.clone();
        let input = match in_cfg.sample_format() {
            SampleFormat::I16 => input_stream::<i16>(
                &device,
                config(in_cfg.channels()),
                tx,
                live,
                self.error_handler(),
            ),
            SampleFormat::I24 => input_stream::<cpal::I24>(
                &device,
                config(in_cfg.channels()),
                tx,
                live,
                self.error_handler(),
            ),
            SampleFormat::I32 => input_stream::<i32>(
                &device,
                config(in_cfg.channels()),
                tx,
                live,
                self.error_handler(),
            ),
            SampleFormat::F32 => input_stream::<f32>(
                &device,
                config(in_cfg.channels()),
                tx,
                live,
                self.error_handler(),
            ),
            SampleFormat::F64 => input_stream::<f64>(
                &device,
                config(in_cfg.channels()),
                tx,
                live,
                self.error_handler(),
            ),
            f => return Err(format!("unsupported input sample format {f:?}")),
        }
        .map_err(|e| self.open_error(&device, e))?;

        let out_channels = out_cfg.channels().min(2);
        let (slot, shared, live) = (self.slot.clone(), self.shared.clone(), self.live.clone());
        let output = match out_cfg.sample_format() {
            SampleFormat::I16 => output_stream::<i16>(
                &device,
                config(out_channels),
                rx,
                slot,
                shared,
                live,
                self.error_handler(),
            ),
            SampleFormat::I24 => output_stream::<cpal::I24>(
                &device,
                config(out_channels),
                rx,
                slot,
                shared,
                live,
                self.error_handler(),
            ),
            SampleFormat::I32 => output_stream::<i32>(
                &device,
                config(out_channels),
                rx,
                slot,
                shared,
                live,
                self.error_handler(),
            ),
            SampleFormat::F32 => output_stream::<f32>(
                &device,
                config(out_channels),
                rx,
                slot,
                shared,
                live,
                self.error_handler(),
            ),
            SampleFormat::F64 => output_stream::<f64>(
                &device,
                config(out_channels),
                rx,
                slot,
                shared,
                live,
                self.error_handler(),
            ),
            f => return Err(format!("unsupported output sample format {f:?}")),
        }
        .map_err(|e| self.open_error(&device, e))?;

        input.play().map_err(|e| e.to_string())?;
        output.play().map_err(|e| e.to_string())?;
        let ch = self
            .live
            .channel
            .load(Relaxed)
            .min(in_cfg.channels().max(1) as usize - 1);
        self.live.channel.store(ch, Relaxed);
        {
            let mut st = lock(&self.status);
            st.routing = Routing {
                input: vec![input_name(driver, ch)],
                output: (1..=out_channels)
                    .map(|k| format!("{driver}:out {k}"))
                    .collect(),
            };
            st.waiting = None;
            st.switching = None;
        }
        self.running = Some(Running {
            driver: driver.to_string(),
            _input: input,
            _output: output,
            beat: self.live.heartbeat.load(Acquire),
            beat_at: Instant::now(),
        });
        log::info!("audio running on {driver} at {} Hz", self.rate);
        Ok(())
    }

    fn open_error(&self, device: &cpal::Device, e: cpal::Error) -> String {
        let at = info_of(device).map_or(0, |i| i.rate);
        if e.kind() == ErrorKind::UnsupportedConfig && at != 0 && at != self.rate {
            format!(
                "The interface is at {at} Hz — set it back to {} Hz in its control panel",
                self.rate
            )
        } else {
            e.to_string()
        }
    }

    /// Stops the streams; a take in progress ends where the audio stopped.
    fn close(&mut self) {
        let Some(running) = self.running.take() else {
            return;
        };
        drop(running);
        if let Some(mut cycle) = self.slot.take() {
            if self.shared.recording.load(Relaxed) {
                cycle.end_take();
                lock(&self.status).interrupted_takes += 1;
            }
            self.slot.put(cycle);
        }
    }

    /// Runs on `driver` from now on, or waits for it.
    fn switch_to(&mut self, driver: &str) {
        self.close();
        lock(&self.status).want = driver.to_string();
        self.try_open();
    }

    fn try_open(&mut self) {
        let want = lock(&self.status).want.clone();
        if let Err(e) = self.open(&want) {
            log::warn!("audio: {want}: {e}");
            let reason = if e.contains(" Hz — ") {
                e
            } else {
                "Audio is waiting for the interface…".to_string()
            };
            let mut st = lock(&self.status);
            st.waiting = Some(reason);
            st.switching = None;
            self.next_try = Instant::now() + RETRY;
        }
    }

    fn watchdog(&mut self) {
        if let Some(r) = &mut self.running {
            let beat = self.live.heartbeat.load(Acquire);
            if beat != r.beat {
                r.beat = beat;
                r.beat_at = Instant::now();
            } else if r.beat_at.elapsed() > STALL {
                log::warn!("audio: {} stopped calling back; rebuilding", r.driver);
                let driver = r.driver.clone();
                self.switch_to(&driver);
            }
        } else if Instant::now() >= self.next_try {
            self.enumerate();
            self.try_open();
        }
    }

    fn run(mut self, requests: Receiver<Request>) {
        loop {
            select! {
                recv(requests) -> r => match r {
                    Ok(Request::Open { driver, channel }) => {
                        self.live.channel.store(channel, Relaxed);
                        self.switch_to(&driver);
                    }
                    Ok(Request::Rescan) => {
                        if !self.shared.recording.load(Relaxed) {
                            self.enumerate();
                        }
                    }
                    Ok(Request::Shutdown) | Err(_) => break,
                },
                recv(self.errors) -> e => {
                    if let Ok(e) = e
                        && let Some(r) = &self.running
                    {
                        log::warn!("audio: {}: {e}; rebuilding", r.driver);
                        let driver = r.driver.clone();
                        self.switch_to(&driver);
                    }
                }
                default(TICK) => self.watchdog(),
            }
        }
        self.close();
    }
}

fn start_supervisor(
    prefer: Option<String>,
    channel: usize,
    requests: Receiver<Request>,
    status: Arc<Mutex<Status>>,
    live: Arc<Live>,
    reply: Sender<Result<Started, String>>,
) {
    let host = match cpal::host_from_id(cpal::HostId::Asio) {
        Ok(h) => h,
        Err(_) => {
            let _ = reply.send(Err(NO_DRIVER.into()));
            return;
        }
    };
    let (errors_tx, errors) = unbounded();
    let shared = Arc::new(Shared::default());
    let mut sup = Supervisor {
        host,
        devices: Vec::new(),
        rate: 0,
        // Replaced below once the rate is known.
        slot: Arc::new(CycleSlot(AtomicPtr::new(std::ptr::null_mut()))),
        shared: shared.clone(),
        live,
        status,
        errors_tx,
        errors,
        running: None,
        next_try: Instant::now(),
    };
    sup.enumerate();
    let drivers = lock(&sup.status).drivers.clone();
    let chosen = prefer
        .and_then(|p| drivers.iter().find(|d| d.name == p && usable(d)))
        .or_else(|| drivers.iter().find(|d| usable(d)))
        .cloned();
    let Some(chosen) = chosen else {
        let _ = reply.send(Err(NO_DRIVER.into()));
        return;
    };
    sup.rate = if chosen.rate > 0 { chosen.rate } else { 48_000 };
    let (cycle, control, streams) = Cycle::new(shared.clone(), sup.rate);
    sup.slot = Arc::new(CycleSlot::new(cycle));
    sup.live.channel.store(channel, Relaxed);
    lock(&sup.status).want = chosen.name.clone();
    if let Err(e) = sup.open(&chosen.name) {
        let _ = reply.send(Err(format!(
            "Audio could not start: {} could not be opened ({e}). Connect the interface and start \
             tuned-record again.",
            chosen.name
        )));
        return;
    }
    let started = Started {
        shared,
        control,
        streams,
        rate: sup.rate,
    };
    if reply.send(Ok(started)).is_ok() {
        sup.run(requests);
    }
}

const NO_DRIVER: &str = "Audio could not start: no ASIO driver found. tuned-record needs an audio \
                         interface with its own ASIO driver installed.";

pub struct Engine {
    pub shared: Arc<Shared>,
    commands: rtrb::Producer<Command>,
    garbage: rtrb::Consumer<Arc<Playback>>,
    pub sample_rate: u32,
    pub playback_latency: u32,
    pub capture_latency: u32,
    pub routing: Routing,
    status: Arc<Mutex<Status>>,
    live: Arc<Live>,
    requests: Sender<Request>,
    thread: Option<JoinHandle<()>>,
    interrupted_seen: u32,
}

impl Engine {
    /// Starts on the first usable ASIO driver.
    pub fn start() -> Result<(Self, Streams), String> {
        Self::start_routed(None, None)
    }

    /// Starts on the driver of `input_port` (or `output_device`) when it is installed, recording
    /// from that input channel.
    pub fn start_routed(
        input_port: Option<&str>,
        output_device: Option<&str>,
    ) -> Result<(Self, Streams), String> {
        let prefer = input_port
            .map(device_of)
            .or(output_device)
            .map(String::from);
        let channel = input_port
            .filter(|p| prefer.as_deref() == Some(device_of(p)))
            .and_then(channel_of)
            .unwrap_or(0);
        let status = Arc::new(Mutex::new(Status::default()));
        let live = Arc::new(Live::default());
        let (req_tx, req_rx) = unbounded();
        let (reply_tx, reply_rx) = unbounded();
        let (st, lv) = (status.clone(), live.clone());
        let thread = std::thread::Builder::new()
            .name("audio-supervisor".into())
            .spawn(move || start_supervisor(prefer, channel, req_rx, st, lv, reply_tx))
            .map_err(|e| format!("Audio could not start: {e}"))?;
        let started = reply_rx
            .recv()
            .map_err(|_| "Audio could not start: the audio thread stopped".to_string())??;
        let mut engine = Self {
            shared: started.shared,
            commands: started.control.commands,
            garbage: started.control.garbage,
            sample_rate: started.rate,
            playback_latency: 0,
            capture_latency: 0,
            routing: Routing::default(),
            status,
            live,
            requests: req_tx,
            thread: Some(thread),
            interrupted_seen: 0,
        };
        engine.refresh_routing();
        Ok((engine, started.streams))
    }

    /// Every input channel of every ASIO driver, as `driver:in N`.
    pub fn input_sources(&self) -> Vec<String> {
        lock(&self.status)
            .drivers
            .iter()
            .filter(|d| usable(d))
            .flat_map(|d| (0..d.ins as usize).map(|c| input_name(&d.name, c)))
            .collect()
    }

    /// The ASIO drivers that can play.
    pub fn output_devices(&self) -> Vec<String> {
        lock(&self.status)
            .drivers
            .iter()
            .filter(|d| usable(d))
            .map(|d| d.name.clone())
            .collect()
    }

    fn current_driver(&self) -> String {
        lock(&self.status).want.clone()
    }

    fn open(&self, driver: &str, channel: usize) {
        lock(&self.status).switching = Some(driver.to_string());
        let _ = self.requests.send(Request::Open {
            driver: driver.to_string(),
            channel,
        });
    }

    /// Records from `port`. On another driver the output moves there too (one driver at a time).
    pub fn set_input(&mut self, port: &str) -> Result<(), String> {
        let channel = channel_of(port).ok_or_else(|| format!("Unknown input {port}"))?;
        let driver = device_of(port);
        if driver == self.current_driver() {
            self.live.channel.store(channel, Relaxed);
            lock(&self.status).routing.input = vec![port.to_string()];
        } else {
            self.open(driver, channel);
        }
        self.refresh_routing();
        Ok(())
    }

    /// Plays into `device`; the mic moves to its first input (one driver at a time).
    pub fn set_output(&mut self, device: &str) -> Result<(), String> {
        if device != self.current_driver() {
            self.open(device, 0);
        }
        self.refresh_routing();
        Ok(())
    }

    /// Picks up what the audio thread published: devices in use and their state.
    pub fn refresh_routing(&mut self) {
        self.routing = lock(&self.status).routing.clone();
    }

    /// Moves to the preferred driver when it is installed and not in use; the channel follows
    /// `input` when it is on that driver.
    pub fn keep_routed(&mut self, input: Option<&str>, output: Option<&str>) {
        self.refresh_routing();
        let Some(driver) = input.map(device_of).or(output) else {
            return;
        };
        let (current, switching, installed) = {
            let st = lock(&self.status);
            let installed = st.drivers.iter().any(|d| d.name == driver && usable(d));
            (st.want.clone(), st.switching.is_some(), installed)
        };
        if switching || !installed {
            return;
        }
        let channel = input
            .filter(|p| device_of(p) == driver)
            .and_then(channel_of);
        if driver != current {
            self.open(driver, channel.unwrap_or(0));
        } else if let Some(c) = channel
            && c != self.live.channel.load(Relaxed)
        {
            let _ = self.set_input(input.unwrap_or_default());
        }
    }

    /// Re-reads the latencies the driver reports and publishes the compensation (with
    /// `calibration`). A mic block one period old (driver calls output first) counts as capture
    /// latency.
    pub fn refresh_latency(&mut self, calibration: i64) {
        self.capture_latency =
            self.live.capture_latency.load(Relaxed) + self.live.lag.load(Relaxed);
        self.playback_latency = self.live.playback_latency.load(Relaxed);
        self.shared.compensation.store(
            compensation(self.playback_latency, self.capture_latency, calibration),
            Relaxed,
        );
    }

    /// Frames of capture delay added because the driver runs output before input (0 normally).
    pub fn callback_lag(&self) -> u32 {
        self.live.lag.load(Relaxed)
    }

    /// Re-reads the installed drivers (not while recording).
    pub fn rescan_devices(&mut self) {
        let _ = self.requests.send(Request::Rescan);
    }

    /// Why audio is not running right now, while the driver is gone or being reopened.
    pub fn waiting(&self) -> Option<String> {
        lock(&self.status).waiting.clone()
    }

    /// The driver being switched to, while a switch is in progress.
    pub fn switching(&self) -> Option<String> {
        lock(&self.status).switching.clone()
    }

    /// True once for each take that a driver reset stopped.
    pub fn take_interrupted(&mut self) -> bool {
        let n = lock(&self.status).interrupted_takes;
        let new = n != self.interrupted_seen;
        self.interrupted_seen = n;
        new
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

    /// Practice speed the song clock runs at.
    pub fn speed(&self) -> Ratio {
        Ratio::from_bits(self.shared.speed.load(Relaxed))
    }

    /// Round-trip latency in ms as currently compensated.
    pub fn latency_ms(&self) -> f32 {
        self.shared.compensation.load(Relaxed) as f32 * 1000.0 / self.sample_rate as f32
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        let _ = self.requests.send(Request::Shutdown);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_names_round_trip() {
        let p = input_name("Focusrite USB ASIO", 1);
        assert_eq!(p, "Focusrite USB ASIO:in 2");
        assert_eq!(device_of(&p), "Focusrite USB ASIO");
        assert_eq!(channel_of(&p), Some(1));
        assert_eq!(channel_of("Focusrite USB ASIO:in 0"), None);
        assert_eq!(channel_of("Focusrite USB ASIO:out 1"), None);
    }

    #[test]
    fn cycle_slot_hands_over_ownership() {
        let shared = Arc::new(Shared::default());
        let (cycle, _control, _streams) = Cycle::new(shared, 48_000);
        let slot = CycleSlot::new(cycle);
        let c = slot.take().expect("owned by the slot");
        assert!(slot.take().is_none(), "taken while a period runs");
        slot.put(c);
        assert!(slot.take().is_some());
    }
}
