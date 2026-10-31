//! JACK driver (served by PipeWire through `pipewire-jack`): one client with a mic input and two
//! outputs, whose process callback runs the shared [`Cycle`] on the ports' buffers.

use std::sync::Arc;
use std::sync::atomic::Ordering::Relaxed;

use jack::{
    AudioIn, AudioOut, Client, ClientOptions, Control, LatencyType, Port, PortFlags, ProcessScope,
};

use super::core::Cycle;
use super::{Command, Routing, Shared, Streams, device_of, short_port};
use crate::audio::latency::compensation;
use crate::audio::mixer::Playback;
use crate::audio::speed::Ratio;

pub const CLIENT_NAME: &str = "tuned-record";
/// Buffer size requested from PipeWire (`PIPEWIRE_LATENCY`), set before the client opens.
pub const REQUESTED_BUFFER: u32 = 256;

struct Processor {
    input: Port<AudioIn>,
    out_l: Port<AudioOut>,
    out_r: Port<AudioOut>,
    shared: Arc<Shared>,
    cycle: Cycle,
}

impl jack::ProcessHandler for Processor {
    fn process(&mut self, _: &Client, ps: &ProcessScope) -> Control {
        let comp = self
            .cycle
            .render(self.out_l.as_mut_slice(ps), self.out_r.as_mut_slice(ps));
        // Fetch the mic buffer only after the outputs are written: pipewire-jack mixes an input
        // port's buffer when it is requested, so a self-patched loopback (out_l → mic_in) hears
        // this period's output, as `loopback_check` relies on.
        self.cycle.capture(self.input.as_slice(ps), comp);
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
        let (cycle, control, streams) = Cycle::new(shared.clone(), sample_rate);
        let processor = Processor {
            input,
            out_l,
            out_r,
            shared: shared.clone(),
            cycle,
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
            commands: control.commands,
            garbage: control.garbage,
            sample_rate,
            in_port,
            out_ports,
            playback_latency: 0,
            capture_latency: 0,
            routing: Routing::default(),
        };
        engine.keep_routed(input_port, output_device);
        Ok((engine, streams))
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

    /// Device lists are read live from JACK; nothing to rescan.
    pub fn rescan_devices(&mut self) {}

    /// Audio never waits for a device here: a lost server is reported through `server_gone`.
    pub fn waiting(&self) -> Option<String> {
        None
    }

    /// Device changes apply at once here.
    pub fn switching(&self) -> Option<String> {
        None
    }

    /// Takes are never cut short by the driver here.
    pub fn take_interrupted(&mut self) -> bool {
        false
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

    /// Practice speed the song clock runs at.
    pub fn speed(&self) -> Ratio {
        Ratio::from_bits(self.shared.speed.load(Relaxed))
    }

    /// Round-trip latency in ms as currently compensated.
    pub fn latency_ms(&self) -> f32 {
        self.shared.compensation.load(Relaxed) as f32 * 1000.0 / self.sample_rate as f32
    }
}
