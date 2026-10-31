# Design: add-windows-asio

> Graphify was unavailable when this was written (context.md → Nodos afectados), so the references below are module and type paths read from the code, not `graph://` nodes.

## Approach

### 1. Split the engine: one cycle, two drivers

`src/audio/engine.rs` becomes a module:

```
audio/engine/
  mod.rs    public types shared by both platforms: Command, Shared, BlockKind, BlockHeader,
            MicStream, Streams, Routing, device_of, short_port; `pub use` of the platform Engine
  core.rs   StreamProducer + stream(), and `Cycle`: the per-period work, platform-free
  jack.rs   #[cfg(target_os = "linux")]  today's JACK Processor/Notifications/Engine
  asio.rs   #[cfg(windows)]              cpal ASIO driver + supervisor thread
```

`Cycle` holds what `Processor` holds today minus the ports: `shared`, `commands`, `garbage`, `playback`, `live`, `disk`, `clock`, `wall`, `playing`, `recording`, `loop_range`. It exposes:

- `process(&mut self, mic: &[f32], out_l: &mut [f32], out_r: &mut [f32])`: the body of today's `ProcessHandler::process`, line for line (handle commands, zero and mix, stamp and push the mic block, advance and loop-wrap the clock, publish `Shared`);
- `stop_recording(&mut self)`: public, for the reset path (§3).

On Linux, `jack::Processor` becomes `{ input, out_l, out_r, cycle }`, and `process` calls `self.cycle.process(self.input.as_slice(ps), out_l, out_r)`. Ports, routing, `keep_routed`, `refresh_latency`, `preload_pipewire_jack`, `request_low_latency`, `client()` and `port_names()` stay in `jack.rs` unchanged. Moving code is the only change on Linux. The proof is the regression gate in §7.

**Decision (found while applying): the cycle runs in two halves, `render(out_l, out_r) -> comp` and `capture(mic, comp)`, and `process` simply calls both.** pipewire-jack mixes an input port's buffer at the moment `jack_port_get_buffer` is called for it. The JACK driver therefore requests the mic buffer only after `render` has written the outputs, as the old callback did. Otherwise the self-patched `loopback_check` (out_l → mic_in) hears silence: the first attempt passed the mic slice as an argument, evaluated before the outputs were written, and the check failed with an empty capture. ASIO takes its mic block from the ring, so it can use `process`.

Because `Cycle` has no JACK types, it gets the unit tests it never had: play/seek/stop, loop wrap only while not recording, `TakeStart`/`TakeEnd` markers, recording refused below 1/1, `SetSpeed` ignored while recording, retired snapshots handed back.

**Decision: core extraction (option A) over a parallel Windows engine.** A copy of the cycle would drift. The speed and compensation logic from `add-practice-aids` already shows how subtle it is (`captured_frame`, `SongClock` remainder carry).

### 2. ASIO driver (Windows)

**Threads and ownership.** The `Engine` owns one *audio supervisor* thread, which holds every cpal object: host, device and streams. ASIO drivers are COM objects tied to the thread that loads them, and cpal's `Stream` is not `Send` on every host. `Engine` methods talk to the supervisor through a `crossbeam_channel` (`SetDriver{input, output}`, `Shutdown`, plus replies). The UI-facing API is unchanged: `start_routed`, `send`, `collect_garbage`, `refresh_latency`, `keep_routed`, `input_sources`, `output_devices`, `set_input`, `set_output`, `routing`, `sample_rate`, `buffer_size`, `speed`, `latency_ms`. `client()`/`port_names()` stay Linux-only.

**Streams.** On one driver:

- an input stream with all of the driver's input channels, keeping the chosen channel;
- an output stream with two channels (the driver's first two outputs);
- both at the driver's current rate, with `BufferSize::Default` (the driver panel's size);
- the sample type is whatever the driver reports (`I16`, `I32` or `F32`), and both callbacks are generic over `T: SizedSample + FromSample<f32>` / `ToSample<f32>`.

**The input stream is built first.** asio-sys calls the callbacks in registration order, inside one `bufferSwitch`. So the mic block of period *k* normally exists before the output callback of period *k* runs.

- **Input callback:** de-interleaves the chosen channel to `f32` and pushes it into an `rtrb` ring of `f32` (*mic ring*, 4 × the largest period). No other state.
- **Output callback:**
  1. takes the `Cycle` (see below);
  2. pops `n` frames from the mic ring into a preallocated `mic` buffer, or zeros plus a counter if they aren't there;
  3. calls `cycle.process(mic, l, r)` on preallocated `l`/`r`;
  4. interleaves `l`/`r` into the driver's buffer as `T`;
  5. puts the `Cycle` back.
- **Allocation:** all scratch buffers are allocated when the streams are built. Their size comes from the driver's current buffer size, with 8192 frames as a floor. A callback larger than its scratch is processed in scratch-sized chunks, so nothing is ever allocated.

**Decision: callback order detected from the callback instants (as applied).** cpal gives both callbacks of one `bufferSwitch` the same callback instant, taken from the driver's `systemTime`. The input callback publishes its instant, and the output callback compares it with its own:
- **Equal:** the input of this period already ran, so the mic block pairs with this period's output.
- **Different:** the freshest mic block is one period old, so it is used and `n` frames of lag are published.

The output callback also drops anything older than one block from the mic ring. Leftovers from start-up, when `play()` started one stream a period before the other, therefore never add more lag. The lag is added to the reported capture latency (`Engine::callback_lag`, printed by `loopback_check`), so compensation is right even before calibration.

*Why not the ring fill level, as first planned:* a ring holding one block looks the same whether it holds this period's block or last period's, so it can't tell the order. A driver that sends a zero `systemTime` makes every period look "fresh". It would then report no lag when there is one, a constant offset that calibration absorbs.

**Decision: the `Cycle` lives in an atomic slot, not behind a lock.** It has to outlive any one pair of streams so that transport state survives a reset (§3), yet cpal callbacks are `'static` closures. It is kept as `Arc<CycleSlot>`, an `AtomicPtr<Cycle>`.

- The output callback `swap`s it out (null), uses it, and `store`s it back.
- The supervisor reclaims it only after dropping both streams. Dropping a cpal ASIO stream removes its callback from asio-sys's `BUFFER_CALLBACK` list, and that removal waits on the list's mutex, which asio-sys holds while dispatching. So once the drop returns, no callback is running and none will run.
- A null pointer seen in a callback (it can't happen; this is defensive) outputs silence and counts it.

This keeps the app's callback code lock-free, as the spec requires. The only lock in the path is asio-sys's own dispatch mutex, which the spec accepts.

**Latency.** cpal encodes `ASIOGetLatencies` in its timestamps:

- input: `callback − capture`;
- output: `playback − callback`.

During the first callbacks the driver turns these into frames and publishes them (`Shared.reported_capture`/`reported_playback`, plus `asio_lag_frames`). On Windows, `refresh_latency(calibration)` reads those atomics instead of JACK port ranges, then stores `compensation(...)` as today. `latency.rs` is unchanged.

**Buffer size display.** The output callback stores `n` in `Shared.buffer_size`, as JACK's `buffer_size` callback does.

### 3. Reset recovery (Windows)

The supervisor loop runs every 100 ms, or when a channel message or a stream error arrives. It has three states: `Running`, `Rebuilding` and `Waiting`.

- **Triggers:**
  - a cpal stream error callback, such as device not available or a backend-specific error carrying an ASIO reset;
  - a **heartbeat watchdog**: `Shared.wall` (published by the cycle) has not moved for 500 ms while streams exist;
  - a `SetDriver` request from the device dialog.

  The watchdog covers drivers and cpal paths that never report the reset (`kAsioResetRequest`, `kAsioBufferSizeChange`). The spike in task 1 checks exactly what cpal 0.18.2 surfaces.
- **Rebuild:**
  1. Drop both streams.
  2. Reclaim the `Cycle` from the slot.
  3. If the cycle was recording, call `cycle.stop_recording()`. It pushes `TakeEnd` at the last captured frame, the disk thread finalises the take exactly as for a normal stop, and the take keeps its alignment because every block before the reset was stamped normally.
  4. Put the `Cycle` back and build new streams on the same driver.
  5. Re-read the latencies and publish the new buffer size.

  Position, play state, loop and speed survive because they live in the `Cycle`. The clock doesn't advance while no callbacks run.
- **Sample rate:** new streams are always requested at the engine's original rate. ASIO lets the host set the rate, and cpal does it when it builds the stream. If the driver refuses, the supervisor stays in `Waiting` with the reason "the interface is at X Hz; set it back to Y Hz".
- **Waiting:** if the driver can't be opened, for example because the interface is unplugged, `Shared.audio_waiting` is set and the supervisor retries every second. The UI shows a waiting notice in place of the device line, and transport and record buttons are disabled through the existing `engine` checks. When the driver opens again it rebuilds and clears the flag.

On Linux, `server_gone` keeps its current meaning and handling.

**What cpal 0.18.2 actually reports (spike, read from `host/asio/stream.rs`; still to confirm on hardware):**
- `kAsioResetRequest`, `kAsioResyncRequest` and a sample-rate change (`SampleRateChanged` away from the configured rate) reach the **error callback** as `ErrorKind::StreamInvalidated`, from cpal's own timer thread. They are ignored while the stream is still starting, because some USB drivers fire spurious resets during `start()`. These are the rebuild triggers.
- `kAsioBufferSizeChange` is accepted and handled **inside** the stream: cpal updates its buffer size and keeps running. No error is raised and no rebuild happens; the callbacks simply arrive with the new `n`. The chunked scratch processing in §2 and the `buffer_size` published from the callback cover it. Drivers that answer a panel change with a reset request instead take the rebuild path.
- `kAsioLatenciesChanged` updates cpal's hardware-latency atomics, and the next timestamps carry the new values. The latency publication in §2 has to keep re-reading timestamps rather than reading them once at start.
- cpal also drops duplicate buffer callbacks with the same `buffer_index` (a known Focusrite/ReaRoute quirk).
- Nothing in cpal reports an unplugged device directly. The heartbeat watchdog stays.
- Besides asio-sys's dispatch mutex, cpal's per-stream callback takes its own uncontended `asio_streams` lock. Both are host glue outside the app's code, and the spec's real-time requirement says "locks" accordingly.

### 4. Device selection (Windows)

The routing model keeps its `"device:port"` strings so `panels.rs` and `config.rs` don't change shape:

- `input_sources()` returns every input channel of every ASIO driver: `"<driver name>:in <n>"`, 1-based. cpal doesn't expose ASIO channel names.
- `output_devices()` returns the driver names.
- `routing.input`/`routing.output` hold the channels in use.
- `set_input("<B>:in 2")` on the current driver only changes the channel the input callback keeps, through an atomic channel index read by the callback, with no rebuild. On another driver it sends `SetDriver`, which rebuilds on driver B (same path as §3), and the output moves with it.
- `set_output("<B>")` on another driver does the same, with the input moving to channel 1 of B.
- `keep_routed` asks for the configured driver when it is present but not in use, and otherwise does nothing.
- Listing inputs means loading each driver in turn on the supervisor thread, because cpal loads a driver to read its channel count. While a stream holds one driver, cpal's enumeration stops at the first *other* driver (`DriverAlreadyExists`). So the supervisor enumerates everything at start, before opening anything, and keeps that cache. A rescan, done when the dialog opens and never while recording, merges what it can still read with the cached entries for drivers it can't load right now.

The dialog shows a line under the input list when the picked input is on a different driver: "Output will move to <B> too". It applies on confirm (ui-ux.md).

### 5. Platform gating

- **`Cargo.toml`:**
  - `jack` and `libloading` move to `[target.'cfg(target_os = "linux")'.dependencies]`;
  - `cpal = { version = "0.18.2", features = ["asio"] }` goes under `[target.'cfg(windows)'.dependencies]`.
- **`main.rs`:**
  - `#![cfg_attr(windows, windows_subsystem = "windows")]`;
  - `request_low_latency()` only under `cfg(target_os = "linux")`;
  - on Windows, a panic hook writes the panic message and location to `%APPDATA%\tuned-record\crash.log`, because there is no console to show it on.
- **Examples:**
  - `loopback_check`: the JACK self-patch stays under `cfg(target_os = "linux")`. On Windows only `--physical` is accepted, using the configured devices from `Config` via `start_routed`. It prints `asio_lag_frames` and the reported latencies.
  - `probe`: drops `request_low_latency` on Windows.
- **Config:** `dirs::config_dir()` already resolves to `%APPDATA%` on Windows. Only the doc comments that say "JACK name" are generalised.

### 6. Build and installer (Linux → Windows)

- **Target:** `x86_64-pc-windows-gnu`. Needs `mingw-w64` (posix-threads variant of `x86_64-w64-mingw32-g++`), `rustup target add x86_64-pc-windows-gnu`, `nsis`, and `clang`/`libclang` (already present for `signalsmith-stretch`).
- **`.cargo/config.toml`:**
  - `[target.x86_64-pc-windows-gnu] linker = "x86_64-w64-mingw32-gcc"`;
  - `rustflags = ["-C", "link-args=-static -static-libgcc -static-libstdc++"]`, so libstdc++, libgcc and winpthread are linked statically and the exe imports only system DLLs.
- **`scripts/build-windows.sh`:**
  1. Checks the prerequisites and prints the missing package names.
  2. Sets `CPAL_ASIO_DIR` to a cached copy of the ASIO SDK under `~/.cache/tuned-record/asiosdk`, downloading it once.
  3. Sets `BINDGEN_EXTRA_CLANG_ARGS_x86_64_pc_windows_gnu="--target=x86_64-w64-mingw32 --sysroot=/usr/x86_64-w64-mingw32"`.
  4. Runs `cargo build --release --target x86_64-pc-windows-gnu`.
  5. Checks the DLL imports with `x86_64-w64-mingw32-objdump -p` against an allow-list of system DLLs.
  6. Runs `makensis` to produce `target/windows/tuned-record-setup-<version>.exe`.
  7. With `--smoke`, starts the exe under Wine for a few seconds and fails on a panic or crash log. Wine has no ASIO driver, so a "no ASIO driver" error window is the expected pass.
- **`installer/tuned-record.nsi`:**
  - per-user install, no admin (`RequestExecutionLevel user`), into `$LOCALAPPDATA\Programs\tuned-record`;
  - a Start-menu shortcut;
  - an uninstaller registered under `HKCU\Software\Microsoft\Windows\CurrentVersion\Uninstall\tuned-record`;
  - the version is taken from `Cargo.toml` by the script;
  - the uninstaller removes the program folder, the shortcut and the registry key. It leaves `%APPDATA%\tuned-record` (config and calibration) and all recordings alone, so reinstalling keeps the setup.
- **README:** a Windows section for the owner: prerequisites, the build command, and SmartScreen ("More info → Run anyway").

**Decision: MinGW cross-compile over MSVC.** It builds natively on Linux with distro packages, and asio-sys's `build.rs` supports it. The fallback, if the spike can't link the C++ parts under MinGW, is `cargo-xwin` (MSVC target with a downloaded MSVC CRT/SDK), recorded as such.

**Spike result (2026-10-27): MinGW works.** The spike built the whole crate (eframe, `signalsmith-stretch`, asio-sys/cpal) with a stub Windows engine. The resulting exe imports only Windows system DLLs, and it opens the full UI under Wine. What it took, which replaces the guesses above:
- **asio-sys needs a patch for MinGW.** Its bindgen output for the SDK class `AsioDrivers` fails its own layout assertion (`connID` at offset 12 in C++, 16 in Rust). Under the Itanium C++ ABI that MinGW uses, the derived fields go into the tail padding of the non-POD base `AsioDriverList`, which a Rust `_base` field can't express. MSVC doesn't reuse tail padding, which is why upstream never sees it. Rust never reads those fields; only the SDK's own C++ touches them. Fix: vendor `asio-sys` 0.4.0 as `vendor/asio-sys`, add `.opaque_type("AsioDrivers")` and `.opaque_type("AsioDriverList")` to its bindgen builder, and wire it in with `[patch.crates-io] asio-sys = { path = "vendor/asio-sys" }`. Worth proposing upstream; until then the vendored copy is pinned to cpal 0.18.2's asio-sys.
- **libstdc++ must be forced static.** The `cc` crate emits `cargo:rustc-link-lib=stdc++` (dynamic) for both asio-sys and signalsmith-stretch, so `-static-libstdc++` alone still left `libstdc++-6.dll` imported. Fix: `CXXSTDLIB_x86_64_pc_windows_gnu=static=stdc++`, plus `-L native=<mingw gcc lib dir>` (the script derives it from `x86_64-w64-mingw32-gcc-posix -print-libgcc-file-name`; here `/usr/lib/gcc/x86_64-w64-mingw32/13-posix`).
- **Use the posix-threads toolchain everywhere:** `linker = "x86_64-w64-mingw32-gcc-posix"`, `CC_x86_64_pc_windows_gnu=x86_64-w64-mingw32-gcc-posix`, `CXX_x86_64_pc_windows_gnu=x86_64-w64-mingw32-g++-posix`.
- **Bindgen** needs `BINDGEN_EXTRA_CLANG_ARGS_x86_64_pc_windows_gnu="--target=x86_64-w64-mingw32 --sysroot=/usr/x86_64-w64-mingw32"` (as planned).
- **The ASIO SDK** is downloaded by asio-sys's `build.rs` into `/tmp/asio_sdk` when `CPAL_ASIO_DIR` is unset. The script pins it to the cache dir instead, so a reboot doesn't trigger a new download.
- **Wine smoke test:** wgpu picks Direct3D 12 under Wine, and Wine's D3D shader compiler can't compile egui's shader (fatal panic). The smoke run sets `WGPU_BACKEND=vulkan`, after which the exe runs. Real Windows has the full compiler, so the app itself is unchanged. The smoke run uses its own `WINEPREFIX` under `target/`.
- The debug exe is 528 MB because of debug info. The installer packages the release build.

### 7. Linux regression gate

Before the split, capture `loopback_check` (self-patch, 100 % and 60 %) and the test suite. After the split, both must give the same numbers: 0.000 ms at 100 %, under 2 ms at 60 %, and every test passing. No Linux behaviour or UI may change.

### 8. Validation on Windows (owner's machine)

1. Install with the setup program.
2. Pick the interface in the device dialog and restart, to check the choice is remembered.
3. With a cable from output 1 to the mic input, run `loopback_check --physical`, Calibrate…, and a recorded take over a transient-rich backing. Each must land within 1 ms.
4. Change the driver buffer size during playback and during a take; unplug and replug.
5. Play through a song at 60 % and 75 % with the guide.

## Archivos a modificar

| Archivo | Cambio |
|---|---|
| `src/audio/engine.rs` | Replaced by `src/audio/engine/{mod,core,jack,asio}.rs` (§1, §2) |
| `src/audio/mod.rs` | Unchanged module name (`engine`); nothing else |
| `src/main.rs` | `windows_subsystem`, Linux-only `request_low_latency`, Windows panic hook (§5) |
| `src/ui/app.rs` | Waiting notice from `Shared.audio_waiting`; Windows start-error text; nothing else changes on Linux |
| `src/ui/panels.rs` | "Output will move to <driver>" line in the device dialog (Windows only, §4) |
| `src/config.rs` | Doc comments only (device strings are platform-neutral) |
| `examples/loopback_check.rs` | Self-patch gated to Linux; prints ASIO lag and latencies on Windows (§5) |
| `examples/probe.rs` | Linux-only `request_low_latency` |
| `Cargo.toml` | Target-specific dependencies (§5) |
| `.cargo/config.toml` (new) | Windows-gnu linker and static flags (§6) |
| `scripts/build-windows.sh` (new) | Cross-build, DLL check, installer, Wine smoke (§6) |
| `installer/tuned-record.nsi` (new) | NSIS per-user installer (§6) |
| `README.md` | Windows build and install section |
| `openspec/project.md` | Platform-qualified Purpose, Tech Stack and Important Constraints |

## Alternativas consideradas

- **Separate Windows engine (option B):** less refactoring, but two copies of the cycle that would drift. Rejected (§1).
- **JACK2 for Windows on top of ASIO:** one backend for both platforms, but it needs a JACK server installed and configured, which is too much for a non-technical user. Rejected.
- **WASAPI or FlexASIO via cpal:** works without an ASIO driver, but higher latency and no driver-reported round trip. Explicit non-goal.
- **`Arc<Mutex<Cycle>>` shared between the callback and the supervisor:** simplest code, but it puts a lock in the app's callback, against the real-time rule. Rejected for the atomic slot (§2).
- **Tagging mic blocks with the ASIO timestamp (`t_k`) to detect misordering:** precise, but more state per block. The mic ring's fill level gives the same answer for free (§2).
- **Detecting resets only from cpal's error callback:** not every driver or cpal path reports them. The heartbeat watchdog is added as a backstop (§3).
- **Opening the driver at the project's sample rate instead of the driver's current one:** would avoid the mismatch message for projects at another rate. Not requested; left out.
- **MSVC via `cargo-xwin`:** kept as the fallback if MinGW fails (§6).
- **Inno Setup:** nicer UI, but it runs only under Wine on Linux. NSIS's `makensis` is native.
- **Portable zip instead of an installer:** no Start-menu entry and no clean uninstall. Worse for the intended user.

## Riesgos

- **Cross-compile:** resolved by the spike (§6), at the cost of a vendored, patched `asio-sys`. Upgrading cpal means re-checking whether upstream fixed the MinGW layout issue, or re-applying the two-line patch.
- **Graphics on unusual Windows machines:** wgpu chooses DX12 or Vulkan. A machine with neither (very old GPU, remote desktop) would fail the way Wine did. Not expected on the target PC; if it happens, eframe's `glow` (OpenGL) renderer is the fallback.
- **What cpal surfaces on an ASIO reset:** unverified. The watchdog limits the damage to about 0.5 s of detection delay.
- **Callback order and the one-period lag:** measured and compensated (§2), but only verifiable on the owner's interface.
- **Enumerating drivers loads them:** listing inputs across several ASIO drivers loads each in turn. That must never happen while streams run (§4). With one interface, as on the target machine, it's moot.
- **Linux regression:** guarded by §7.
- **Unsigned installer and SmartScreen:** accepted (non-goal), documented in the README.
- **Steinberg ASIO SDK licence (verified 2026-10-27 from the SDK's `LICENSE.txt`):** dual-licensed. Under the proprietary Steinberg ASIO License, publishing software needs a License Agreement signed by Steinberg, and the SDK itself may not be redistributed. The alternative is GPL v3. The SDK is compiled into the .exe and downloaded at build time (never committed). Giving the setup program to another person is distribution under one of the two: either sign Steinberg's (free) agreement, or give that person the .exe under GPL v3 terms, which entitles them to the source. This is the owner's decision; it is noted in the README.
- **No console on Windows:** errors that today land in stderr are lost there. The crash log covers panics. Driver and engine errors are shown in the UI as they are today.
