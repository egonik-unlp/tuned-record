# Tasks: add-windows-asio

<!-- Tasks de implementación en orden. Ver design.md para el approach técnico. -->

## Implementation

### 0. Prerequisites
- [ ] `add-pitch-follow-recorder` archived, then `add-practice-aids` archived, so the `audio-engine` main spec exists for this change's RENAMED/MODIFIED deltas (proposal.md → Impact, Sequencing)
      _2026-10-27: user chose to implement now on top of a checkpoint commit of add-practice-aids (`d7ac7eb`); archiving still has to happen in this order before this change is archived._
- [x] Linux baseline recorded before touching the engine: `cargo test` result and `loopback_check` numbers at 100 % and 60 % (design.md → 7)
      _Baseline at `d7ac7eb`: 70 tests pass; loopback_check (self-patch, 48 kHz / 256): calibration residual +0 frames, exported take vs backing +0.000 ms, 60 % live pitch steps [-1.90, 1.35, 1.03, -1.02] ms (worst 1.90), PASS._
- [x] Build prerequisites installed on this machine: `mingw-w64` (posix g++), `rustup target add x86_64-pc-windows-gnu`, `nsis` (design.md → 6)

### 1. Cross-compile spike (gate for everything below)
- [x] Make the crate build for `x86_64-pc-windows-gnu` with today's code: gate `jack`/`libloading` and the engine behind `cfg(target_os = "linux")` temporarily, add `cpal` with `asio` for Windows, `.cargo/config.toml` with the MinGW linker and static flags, ASIO SDK via `CPAL_ASIO_DIR`, bindgen clang args for the target (design.md → 6)
      _2026-10-27: done in a scratch copy with a stub Windows engine (lists ASIO drivers, then reports "spike build"); needed a vendored asio-sys with `AsioDrivers` opaque and static libstdc++ via `CXXSTDLIB` (design.md → 6, Spike result). The repo itself is untouched; tasks 2–8 bring these pieces in for real._
- [x] Confirm `signalsmith-stretch` (C++ + bindgen), `asio-sys` and eframe link, and that the exe imports only system DLLs (`objdump -p`)
      _All link; imports are Windows system DLLs only. Under Wine (with `WGPU_BACKEND=vulkan`) the exe opens the full UI and shows the audio error banner with no ASIO drivers._
- [x] Find out how cpal 0.18.2 reports an ASIO reset request / buffer-size change (error callback variant or nothing) and record it in design.md → 3
      _From the cpal source: reset/resync/rate change → error callback with `StreamInvalidated`; buffer-size change handled in-stream (no error); unplug not reported (watchdog stays). To confirm on the owner's interface during validation._
- [x] If MinGW can't link: switch to the `cargo-xwin` fallback and record why in design.md → Alternativas, before continuing
      _Not needed: MinGW links._

### 2. Engine core extraction (specs/audio-engine; Linux unchanged)
- [x] Split `src/audio/engine.rs` into `audio/engine/{mod,core,jack}.rs`; `Cycle::process(mic, out_l, out_r)` and `Cycle::stop_recording` in `core.rs`; JACK `Processor` wraps `Cycle` with its ports (design.md → 1)
- [x] Unit tests for `Cycle` without JACK: play/seek/stop, loop wrap only while not recording, take start/end markers, record refused below 100 %, `SetSpeed` ignored while recording, retired snapshots handed back
- [x] Linux regression gate: same `cargo test` result and the same `loopback_check` numbers as the baseline from task 0 (design.md → 7)
      _77 tests pass (70 + 7 new `Cycle` tests); loopback_check identical to baseline: +0 frames, +0.000 ms, 60 % steps [-1.90, 1.35, 1.03, -1.02] ms, PASS. Needed the render/capture split (design.md → 1)._

### 3. ASIO driver (specs/audio-engine → Duplex audio, Low-latency buffer request, Latency-compensated capture, Real-time safe processing)
- [x] `asio.rs`: supervisor thread owning host/device/streams; `Engine` API over a control channel; input stream built first, mic ring, generic sample-format callbacks, preallocated scratch with chunking (design.md → 2)
      _Callback order detected from callback instants rather than ring fill (design.md → 2, as applied). Compiles and passes clippy for `x86_64-pc-windows-gnu`; untested on real ASIO hardware until validation._
- [x] `CycleSlot` atomic handoff between the output callback and the supervisor; silence + counter on a null slot
- [x] Latencies from cpal timestamps and callback-order lag (`Shared.reported_capture`/`reported_playback`/`asio_lag_frames`); `refresh_latency` on Windows; buffer size published from the callback
- [x] Start-up errors for "no ASIO driver" and "driver could not be opened" with the texts in ui-ux.md

### 4. Reset recovery (specs/audio-engine → Driver reset recovery)
- [x] Supervisor states Running / Rebuilding / Waiting; triggers from the stream error callback, the 500 ms heartbeat watchdog and `SetDriver` (design.md → 3)
- [x] Rebuild path: drop streams, reclaim `Cycle`, stop an active recording via `Cycle::stop_recording`, rebuild at the original sample rate, re-read latencies; `Shared.audio_waiting` with 1 s retries; rate-mismatch reason
- [x] Unit test (platform-free) of the take closed by the reset path: blocks before the reset stamped normally, `TakeEnd` at the last captured frame, the disk thread finalises it
      _`core::tests::interrupted_take_is_saved_by_the_disk_thread` (real disk thread) and `end_take_closes_an_interrupted_recording`._

### 5. Device selection on Windows (specs/audio-engine → Audio device selection)
- [x] `input_sources` / `output_devices` / `routing` / `set_input` / `set_output` / `keep_routed` over ASIO drivers and channels; same-driver channel change through an atomic index, other driver through `SetDriver`; enumeration only at start and on dialog open, never while recording (design.md → 4)
      _Enumeration merges with a start-up cache, because cpal stops enumerating while another driver is loaded (design.md → 4)._
- [x] Config doc comments generalised (device strings are platform-neutral)

### 6. UI (ui-ux.md)
- [x] Device line names the driver and channel; waiting notice and rate-mismatch notice in place of the device line; transport and record disabled while waiting
      _Waiting text replaces the device button text (still clickable to pick another device); Play/Record (via `transport_ready`) and Calibrate… disabled while waiting._
- [x] Devices dialog: lists read on open; cross-driver confirm row ("Output will move to … too. [Switch] [Cancel]"), "Switching to …" state; ASIO buffer-size sentence in the help text
      _Rescan requested when the dialog opens; confirm row with focus on "Switch", Escape cancels the pending switch; combos disabled while a switch is pending or running._
- [x] Transient hint when a reset stops a take
- [x] Linux check: dialog, device line and errors look exactly as before
      _Checked by running the Linux build: top bar and devices dialog unchanged (`ONE_DRIVER` is false, `waiting()`/`switching()` always None on Linux)._

### 7. Platform gating and examples
- [x] `Cargo.toml` target-specific dependencies; `main.rs` `windows_subsystem`, Linux-only `request_low_latency`, Windows panic hook writing `crash.log` (design.md → 5)
      _Plus `vendor/asio-sys` with `[patch.crates-io]`, and `.cargo/config.toml` (linker, static flags, `[env]` for CC/CXX/CXXSTDLIB/bindgen)._
- [x] `loopback_check`: self-patch gated to Linux; Windows `--physical` uses the configured devices and prints ASIO lag and reported latencies; `probe` gated likewise
- [x] `cargo clippy` clean for both targets (`--target x86_64-pc-windows-gnu` on this machine)
      _Also `cargo test --target x86_64-pc-windows-gnu --lib` under Wine: 79 passed._

### 8. Build and installer (specs/windows-install)
- [x] `scripts/build-windows.sh`: prerequisite check, cached ASIO SDK, cross-build, DLL allow-list check, `makensis`, `--smoke` under Wine (design.md → 6)
      _Also builds `loopback_check.exe` into `target/windows/`. First run downloaded the SDK into `~/.cache/tuned-record/asiosdk`; DLL allow-list passes; `--smoke` passes._
- [x] `installer/tuned-record.nsi`: per-user install, Start-menu entry, HKCU uninstaller entry, version from `Cargo.toml`; uninstall leaves `%APPDATA%\tuned-record` and recordings alone
      _Checked under Wine (silent `/S`): installs to `AppData\Local\Programs\tuned-record`, Start-menu shortcut, HKCU uninstall entry (DisplayVersion 0.1.0); uninstall removes them and leaves `%APPDATA%\tuned-record\config.toml`._
- [x] README: Windows build prerequisites and command, install and SmartScreen note
      _Includes the ASIO SDK licence note (design.md → Riesgos)._
- [x] `openspec/project.md`: Purpose, Tech Stack and Important Constraints worded per platform (proposal.md → Conflictos)

## Validation

- [x] `cargo test` and `cargo clippy` clean on Linux; `cargo clippy --target x86_64-pc-windows-gnu` clean
      _78 tests on Linux; clippy clean on both targets; Windows lib tests under Wine: 80._
- [ ] Linux: `loopback_check` equals the baseline (0.000 ms at 100 %, < 2 ms at 60 %); a manual session looks and behaves as before
      _loopback_check part done after every engine change (identical to baseline). Still to do: a manual session by the owner._
- [x] `scripts/build-windows.sh --smoke` produces the setup program and the exe starts under Wine with the "no ASIO driver" error, no crash log
      _Screenshot under Wine shows "Audio could not start: no ASIO driver found. …"; `loopback_check.exe` exits with the same message._
- [ ] Owner's Windows machine — install: setup runs (SmartScreen once), Start-menu entry, no console window; device choice survives a restart; upgrade over an existing install keeps the mic channel and calibration; uninstall leaves recordings
- [ ] Owner's Windows machine — alignment: `loopback_check --physical` and Calibrate… within 1 ms; a take recorded over a transient-rich backing within 1 ms of it after calibration; ASIO lag reported (0 or one period) and noted here
- [ ] Owner's Windows machine — resets: buffer size changed in the driver panel while playing at 75 % with a loop (continues in place); during a take (take kept up to the reset, recording stopped, hint shown); unplug and replug while stopped (waiting notice, then back without user action, position kept)
- [ ] Owner's Windows machine — practice: playback, live pitch line, note bars, 60 % / 75 % speeds and the level meter behave as on Linux
- [ ] A11y (ui-ux.md → Accesibilidad): waiting notice and reset hint readable as text; confirm row reachable by keyboard with focus on "Switch" and Escape cancelling; disabled transport exposes its state (check with Narrator on Windows if available; Orca on Linux for the unchanged dialog)

## Post-archive

- [ ] `/documenter distill <dated-change-id>` — **auto-ejecutado por `/opsx:archive`**.
      Solo correr a mano si el auto-trigger falló (re-run con el id completo con fecha,
      ej. `2026-10-27-add-windows-asio`). Sincroniza el use-case (knowledge/modules/<m>/use-cases/) con los deltas del change
      (spec narrativa, decisiones, lore). Humano revisa y acepta edits propuestos.
