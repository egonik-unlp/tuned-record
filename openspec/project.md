# Project Context

<!--
  Template stub. Reemplazá los <TODO> con el contexto de tu proyecto.
  Este archivo lo lee el agente al generar context.md — mantenelo al día.
-->

## Purpose

Desktop tool for a singer (Linux, and Windows with an ASIO audio interface): record yourself over songs with a live pitch contour, follow
the pitch contour of a demucs-separated reference vocal (transposable, visible ahead of the
playhead), stack aligned takes into harmonies/choruses, and export them to a DAW.

## Tech Stack

Rust (edition 2024), single crate `tuned-record` (lib + bin). Audio: a platform-free engine cycle
driven by the `jack` crate over PipeWire (`pipewire-jack`) on Linux and by `cpal` with its ASIO host
on Windows (cross-compiled from Linux with MinGW, packaged with NSIS); lock-free rings with `rtrb`. GUI: `eframe`/`egui`. Pitch: own McLeod/NSDF on
`realfft` (live), `pyin` + MPM refinement (offline). Files: `symphonia` (decode), `rubato`
(resample), `hound` (WAV). Project manifest: `serde` + `toml`; cache keys: `blake3`.

## Documentation & Spec Stack

Este repo usa el harness SDD de tres capas — **consultar en este orden antes de explorar código**:

1. **Graphify** (`<package>/graphify-out/`) — grafo vivo del código. Query vía MCP `graphify`
   (`query_graph`, `get_node`, `get_neighbors`, `shortest_path`; configurado en `.mcp.json`); fallback `GRAPH_REPORT.md`.
2. **OpenSpec** (`openspec/specs/<capability>/spec.md`) — contrato funcional actual. Los changes pasan por
   `openspec/changes/<id>/` con el schema `sdd-with-knowledge` (6 artefactos).
3. **`knowledge/`** — contexto humano (decisiones, lore, convenciones), organizado en 3 capas:
   `knowledge/INDEX.md` (índice de módulos, entry point) → `knowledge/modules/<m>/INDEX.md` (navegación por
   intención: tabla "¿Qué quiero hacer?" intent→flow) → `knowledge/modules/<m>/use-cases/<capability>/`
   (contenido: spec/flow/decisions/notes; 1:1 con `openspec/specs/<capability>/`, resuelto por glob).

Regla divisoria: lo que Graphify responde (estructura) **no** se escribe en `knowledge/`.
Flujo completo en `openspec/README.md`; reglas duras en `openspec/CONVENTIONS.md`.

## Project Conventions

<TODO: code style, patrones de arquitectura, naming, testing, git workflow.>

## Domain Context

<TODO: entidades clave y sus relaciones, reglas de dominio no obvias.>

## Important Constraints

- Linux: needs `pipewire-jack` at runtime (the engine preloads PipeWire's libjack, so no
  `pw-jack` wrapper) and `libjack-jackd2-dev` to build (pkg-config `jack.pc`).
- Windows: needs an audio interface with a native ASIO driver; mic and output are channels of that
  one driver. Built on Linux only (`scripts/build-windows.sh`), no CI; the ASIO SDK is
  dual-licensed by Steinberg (own licence or GPL v3).
- The app's audio callback code must stay real-time safe: no allocation, locks or I/O. Audio and
  commands cross threads only through lock-free rings; retired buffers are dropped off-thread. On
  Windows the ASIO glue (cpal, asio-sys) takes uncontended locks of its own each period; that is
  accepted, outside the app's code.
- Every captured sample is placed on the song timeline with latency compensation
  (reported port latencies + per-machine calibration); takes and exports must stay
  sample-aligned to song t=0.
- A project's sample rate is fixed at creation and must match the audio device's (JACK server's
  or ASIO driver's) to play or record.
- Headphones are assumed (playback reaching the mic would be tracked as the singer's pitch).
