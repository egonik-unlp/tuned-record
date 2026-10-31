# Proposal: Windows support over ASIO, Linux unchanged

**Change ID:** add-windows-asio
**Date:** 2026-10-27
**Author:** Eduardo Gonik
**Estado:** Aprobado — 2026-10-27
<!-- Marca del gate de aprobación. La escribe SOLO /opsx:propose, y solo tras un Approve explícito
     del usuario en el gate: "Aprobado — <fecha>". Toda (re)generación de este archivo la resetea a
     "Pendiente de aprobación" — se aprueba un texto, no un archivo. -->

## Why

tuned-record only runs on Linux: the engine is a JACK client over PipeWire, and `project.md` says "Linux only". The owner wants a less technical person to practise and record takes with it on their own Windows PC, which has an audio interface with a native ASIO driver (context.md → Entrevista de intención). It's for personal use: the owner builds it here and sets it up for them.

Recording matters, not just practice, so Windows has to keep the property the app is built on: every captured sample lands on the song timeline within 1 ms after calibration. The person won't troubleshoot audio, so the app has to survive the things ASIO drivers do: a buffer-size change in the driver panel, a sample-rate change, an unplugged interface. Each of these must recover without restarting the app. It installs from a setup program.

The explore session checked cpal 0.18.2's ASIO host (context.md → Contexto knowledge). It runs the input and output callbacks on one thread, in registration order, for the same buffer index. That makes it effectively duplex, so the JACK-style cycle (capture and playback in one period, one song clock) carries over to Windows without a second clock domain.

## Scope

**Comunidades afectadas:** see context.md → Nodos afectados. They come from reading the code because Graphify was unavailable. In short: the audio engine (split into a shared core and per-platform drivers), the engine's consumers (`ui::app`, the device dialog in `ui::panels`, `main`, examples), the build configuration, and the project constraints.

**Archivos principales:** see context.md → Archivos relevantes. Core: `src/audio/engine.rs` (becomes `audio/engine/{mod,core,jack,asio}.rs`), `src/audio/latency.rs`, `src/main.rs`, `src/ui/app.rs`, `src/ui/panels.rs`, `src/config.rs`, `examples/loopback_check.rs`, `examples/probe.rs`, `Cargo.toml`, `openspec/project.md`, plus a new cross-build script and NSIS installer script.

What is in:

1. **Shared engine core.** The per-period cycle moves out of the JACK handler into platform-free code. The JACK driver on Linux calls it exactly as today, and the ASIO driver on Windows calls it from cpal's callbacks. Covered: commands, the song clock with practice speed, loop wrap, the playback mix, latency-compensated block stamping, the live and disk mic streams, retirement of old snapshots. The `Engine` API the UI uses stays the same on both platforms.
2. **ASIO driver (Windows).** cpal with the `asio` host:
   - One driver loaded per process, with the mic input as one channel of that driver.
   - Stereo output on the same driver.
   - The driver's native sample format converted into preallocated buffers.
   - Round-trip latency taken from the driver (`ASIOGetLatencies`, through cpal's timestamps), plus the stored calibration.
3. **Driver reset recovery (Windows).** When the driver asks for a reset, the streams are rebuilt off the audio thread. Triggers: buffer size or rate change, the driver disappearing or coming back. Transport position, play state, loop range and practice speed are restored. A take in progress is closed and kept up to the point of the reset; it doesn't resume. While the driver is gone, the app stays open and says so.
4. **Device selection on Windows.** The same dialog lists the ASIO drivers' input channels and outputs. Choosing a mic on another driver also moves the output to that driver, and the dialog says so. The choice is remembered in the config, which lives in `%APPDATA%` on Windows.
5. **Platform gating.** `jack`/`libloading`, the PipeWire libjack preload and `PIPEWIRE_LATENCY` are Linux-only; `cpal` with `asio` is Windows-only. On Windows the app has no console window. `loopback_check` works on Windows with `--physical`; the JACK self-patch stays Linux-only.
6. **Build and installer from this Linux box.** A script cross-compiles to `x86_64-pc-windows-gnu` with mingw-w64, including the ASIO SDK and the C++ time-stretcher, with no runtime DLLs to ship. An NSIS installer adds a Start-menu entry and an uninstaller. A Wine smoke test checks that the exe starts. No CI.
7. **Project constraints.** `project.md` and the `audio-engine` capability are reworded per platform.

| Capability | Delta | What changes |
|---|---|---|
| `audio-engine` | MODIFIED | "Duplex audio over JACK on PipeWire": duplex on Linux over JACK, on Windows over the interface's ASIO driver; "no audio" errors per platform. "Audio device selection": ASIO input channels / outputs; one driver for both. "Low-latency buffer request": Linux unchanged; on Windows the driver panel sets the buffer size, which the app shows. "Latency-compensated capture": the latency comes from the platform (JACK ports / ASIO driver). "Real-time safe processing": the rule applies to the app's code in the callback. On Windows the ASIO glue's own per-period dispatch lock is accepted as outside the app. |
| `audio-engine` | ADDED | "Driver reset recovery": the engine comes back after a driver reset or device loss without restarting the app, keeps the transport state, and closes an interrupted take cleanly |
| `windows-install` | ADDED | A setup program built on Linux installs the app, a Start-menu entry and an uninstaller. The app starts without a console window and keeps its config in `%APPDATA%` |

## Impact

- **Linux is the critical consumer.** Extracting the core touches the code that every alignment guarantee runs through. Before and after the split, `loopback_check` must give the same results (0.000 ms at 100 %, under 2 ms at 60 %), and the existing tests must pass. The JACK driver keeps its current ports, routing, `keep_routed` and latency refresh.
- **`Engine` consumers** (`ui::app`, `ui::panels`, `main`, examples) keep the same calls. `client()` and `port_names()` become Linux-only, and only the examples use them.
- **`server_gone` path:** on Linux a lost JACK server stays fatal for the session, as today. On Windows it becomes "driver gone, waiting", followed by automatic recovery.
- **Real-time safety:** the ASIO callbacks run the same allocation-free core. The format conversion uses buffers allocated at stream build. The live and disk streams keep their lock-free rings.
- **Dependencies / licences:**
  - `cpal` (Apache-2.0) and `asio-sys`, which needs the Steinberg ASIO SDK at build time. The SDK's terms are not verified; this is personal use.
  - The build needs libclang (already needed by `signalsmith-stretch`), mingw-w64, the Rust Windows target, and NSIS on this machine.
- **Sequencing:** the `audio-engine` capability exists only in the deltas of `add-pitch-follow-recorder` and `add-practice-aids`. This change is implemented and archived after both are archived, in that order, so its MODIFIED requirements apply to the main specs they leave behind.

## Non-goals

- Code signing. The installer stays unsigned; SmartScreen is clicked through once, following a one-line note.
- WASAPI, FlexASIO, or interfaces without a native ASIO driver.
- macOS.
- Auto-update. A new version is a new installer.
- CI. Builds happen on this Linux box.
- Resuming a take that a driver reset interrupted.
- Guided calibration for the Windows user. The owner calibrates their machine once with a cable.
- Any change to Linux behaviour.

## Criterios de aceptación

- On the owner's Windows machine with the ASIO interface:
  - the installer from this box installs the app and it starts without a console window;
  - the devices dialog lists the interface's inputs and outputs, and the choice survives a restart;
  - `loopback_check --physical` and Calibrate… land within 1 ms;
  - a take recorded over a backing with sharp transients lines up with it within 1 ms after calibration;
  - playback, the live pitch line, note bars, practice speeds and the level meter behave as on Linux.
- On that machine, changing the buffer size in the driver panel or unplugging and replugging the interface:
  - recovers without restarting the app;
  - keeps the position, play state, loop and speed;
  - if it happens while recording, the take is kept up to the reset and recording stops.
- On Linux:
  - `loopback_check` gives the same results as before the change (0.000 ms at 100 %, under 2 ms at 60 %);
  - the test suite passes;
  - nothing visible changes.
- The Windows build and installer are produced on this Linux box, and the exe passes a Wine start-up smoke test.

## Conflictos con decisiones existentes

There is no `knowledge/` yet, so there is no `decisions.md` to check against. Conflicts with `openspec/project.md` → Important Constraints, all intended:

- **"Linux only; needs `pipewire-jack`"**: becomes "Linux (JACK on PipeWire) and Windows (native ASIO driver)".
- **"The JACK process callback must stay real-time safe: no allocation, locks or I/O"**: kept for the app's code on both platforms. Narrowed explicitly: on Windows, `asio-sys` takes an uncontended mutex once per period in its own dispatch.
- **"A project's sample rate is fixed at creation and must match the server's"**: unchanged. On Windows "the server's" means the ASIO driver's current rate.

Spec conflicts with the open changes are handled by sequencing (see Impact).
