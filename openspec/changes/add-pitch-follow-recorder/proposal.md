# Proposal: Pitch-follow recorder

**Change ID:** add-pitch-follow-recorder
**Date:** 2026-10-21
**Author:** Eduardo Gonik
**Estado:** Aprobado — 2026-10-21

## Why

The user wants to record themselves singing over songs while seeing their pitch live, and to follow the pitch contour of a reference vocal they separated with demucs beforehand. The main flow is building choruses and harmonies by stacking takes: copy the reference's *relative* pitch movement at a different absolute pitch, watch every take pile up on one song timeline, and export the result to a DAW for mixing.

No existing tool combines these three things: a live pitch contour, a reference guide whose upcoming part is visible ahead of the playhead, and sample-aligned overdub recording. Pitch monitors show the voice but no guide; karaoke-style games quantize to notes and don't record usable takes; DAWs record but don't show a live pitch contour against a guide.

This is the first change in a greenfield repo (see context.md → Nodos afectados): it creates the application and fixes its base stack — Rust, Linux only, JACK API over PipeWire (`pipewire-jack`).

## Scope

**Comunidades afectadas:** none — greenfield (see context.md → Nodos afectados). No graph exists yet; the communities below are the ones this change introduces.

**Archivos principales:** none existing (see context.md → Archivos relevantes). Everything is new: a Rust crate plus `openspec/specs/` for the capabilities below.

New capabilities (flat slugs, all ADDED):

| Capability | What it covers |
|---|---|
| `audio-engine` | JACK duplex process callback over PipeWire; a single song clock; mixing backing + guide stem + takes to the output; capturing the mic; latency compensation for recorded takes; requesting a low per-node quantum |
| `live-pitch` | Live f0 detection from the mic (fast detector, e.g. McLeod/YIN) and its scrolling contour on a note grid with current note + cents readout |
| `reference-contour` | Importing any backing track and a demucs vocal stem (decode + resample to device rate); offline f0 of the stem (pYIN) cached on disk; the fixed playhead with the reference ghost visible ahead; audible guide playback; guide transposition (chromatic ±st, free drag, snap-to-voice) |
| `take-layering` | Recording whole takes in song time; overdub (earlier takes play during new ones); each take's contour kept on screen in its own color; per-take mute/solo/volume/delete/show-hide |
| `pitch-feedback` | Live cents deviation against the shifted guide (colors the live line); per-phrase shape score that is transposition-invariant (RMS of the deviation after removing the median offset), phrases cut at reference unvoiced gaps |
| `project-session` | A project folder holding source references, cached f0 analyses, takes and their settings; reopen and continue layering |
| `audio-export` | One WAV per take aligned to song t=0, plus a mixdown of backing + unmuted takes, same sample rate and length |

## Impact

- **Consumers:** none — no existing code or graph edges (greenfield).
- **System setup:** requires the `pipewire-jack` package on the host. It is not installed on the target machine today; only `libjack-jackd2` is present, with no `jackd`, so a JACK binary would fail to connect (context.md → Riesgos identificados). Setup instructions are part of this change.
- **Latency budget:** PipeWire runs at quantum 1024 / 48 kHz (~21 ms per buffer). Meeting the < 40 ms voice-to-line target depends on requesting a smaller per-node quantum (~256).
- **Hardware assumption:** headphones are required; playback bleeding into the mic would be tracked by the live detector.
- **Project baseline:** `openspec/project.md` is still the template; this change defines its stack. The macro decisions (Rust, Linux only, JACK over PipeWire, project-folder sessions) are candidates for `knowledge/decisions.md` via `/documenter distill` after archive.
- **Related use-cases:** none — `knowledge/` does not exist yet.

## Non-goals

- Effects or mixing tools (EQ, reverb, compression, panning) — that happens in the DAW after export.
- Running demucs inside the app — stems are separated beforehand and imported.
- Cross-platform support — Linux only.
- Deferred, not ruled out: pitch correction, punch-in / partial re-recording, diatonic (in-key) transposition, CREPE-based reference analysis.

## Criterios de aceptación

- Opening or creating a project and loading a backing track and a demucs vocal stem shows the reference contour scrolling past a fixed playhead, with the upcoming part visible ahead.
- Singing draws the live line with the current note and cents; perceived latency from voice to line on screen is under 40 ms.
- The live line is colored by its cents deviation from the shifted guide; transposition (±st, drag, snap-to-voice) moves both the ghost and the cents target.
- After each reference phrase, a shape score appears; the same contour sung at different transpositions yields the same score.
- Each recorded take stays on screen in its own color and plays back during later takes.
- In a loopback test (output fed back into input), an exported take matches the backing within ~1 ms.
- Mute, solo, volume and delete per take behave as expected in both playback and the mixdown.
- Export produces one WAV per take plus a mixdown of backing + unmuted takes, all starting at song t=0 with the same length and sample rate, importable into a DAW without manual shifting.
- Closing and reopening the project restores the sources, cached reference, takes and their settings; a take from an earlier session still plays aligned.

## Conflictos con decisiones existentes

Ninguno — `knowledge/decisions.md` and use-case decisions do not exist yet. Open risks that do not contradict prior decisions are listed in context.md → Riesgos identificados (pipewire-jack not installed, default quantum vs the latency target, 44.1→48 kHz resampling, pYIN robustness on demucs stems, live octave errors vs smoothing lag, mic bleed, shape score vs timing drift, real-time safety, scope creep toward a DAW).
