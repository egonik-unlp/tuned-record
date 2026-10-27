# Context: add-pitch-follow-recorder

## Entrevista de intención

### Q&A

| Pregunta | Respuesta |
|---|---|
| What is the tool for? | Recording my voice (possibly over songs) with a live pitch measurement and a live plot of pitch over time; importing a demucs-separated vocal stem to get its pitch contour so I can follow it. Main flow: layer many takes that pile up over time (choruses), imitating the reference's relative variation at a different pitch. The tool also records the audio. |
| What does "pile up" lead to — practice only, or output for a DAW? | Export. |
| Should the upcoming (future) part of the reference be visible ahead of the playhead? | Yes, that would be good. |
| How should the guide be transposed (chromatic, diatonic, free drag, snap-to-voice)? | All would be good; first version: chromatic ±semitones + drag + snap-to-voice, diatonic later. |
| Should the demucs vocal stem be playable as an audible guide, and can the backing be any mix? | Yes, both. |
| What feedback beyond the picture? | Live cents deviation against the (shifted) reference, and a per-phrase "how close was the shape" score. |
| Stack? | Rust — more performance needed. |
| Platform? | Linux only. |
| Audio API? | JACK API over PipeWire via `pipewire-jack`. |
| Export contents? | One aligned WAV per take + a mixdown. |
| Take model? | Whole takes (no punch-in). |
| What goes in the mixdown? | Backing + all unmuted takes. |
| Scope of this first change? | v0–v2 + phrase shape score: everything discussed except diatonic transpose and CREPE. |
| Do sessions persist? | Yes, as a project folder. |
| Perceived latency target, voice → line on screen? | < 40 ms. |
| Explicit non-goals? | Effects/mixing tools; running demucs inside the app. |
| Minimum per-take controls? | Mute, solo, volume, delete (plus show/hide contour). |
| How is take alignment verified? | Loopback test: exported take matches backing within ~1 ms. |
| Pitch correction and punch-in — where do they stand? | Both later, not now: out of this change but not ruled out. |

### Intent brief

**Intención:** A Linux desktop tool (Rust) for recording yourself singing over songs while seeing your pitch live, and for following the pitch contour of a reference vocal you separated with demucs beforehand. The main use is building choruses and harmonies by stacking takes: you copy the *relative* pitch movement of the reference at a different absolute pitch, see every take pile up on one song timeline, and export the result to a DAW. Nothing existing combines a live pitch contour, a guide with a visible future section, and aligned overdub recording.

**Alcance:** Audio I/O through the JACK API on PipeWire (`pipewire-jack`), with one duplex callback so playback and capture share a clock. Live pitch detection from the mic, shown as a scrolling contour on a note grid with the current note and cents. Import any backing track, plus a demucs vocal stem that gets analyzed offline into a cached reference contour. A fixed playhead with the reference's upcoming part shown as a ghost ahead of it. The guide stem can also be played as an audible guide. Recording whole takes with latency compensation; takes play back during later takes (overdub) and their contours accumulate in distinct colors. Per-take controls: mute, solo, volume, delete, show/hide contour. Guide transposition: chromatic ±semitones, free drag, snap-to-voice. Feedback: live cents deviation against the shifted guide, and a per-phrase shape score that ignores transposition (RMS of the deviation after subtracting the median offset). Sessions persist as a project folder: source references, cached f0, takes and their settings. Export: one WAV per take, aligned to song t=0, plus a mixdown of backing and unmuted takes.

**Comportamiento observable:** You open or create a project and load a backing track and a vocal stem. The reference contour appears and scrolls past a fixed playhead, with what's coming visible ahead. When you sing, your line is drawn live with a note/cents readout, colored by how far you are from the shifted guide. After each phrase, a shape score appears. Each recorded take stays on screen and in the mix for the next take. You can shift the guide by semitones, drag it, or snap it to your voice. Exporting produces aligned per-take WAVs and a full mixdown that drop into a DAW at bar 1. Closing and reopening the project brings everything back.

**Non-goals:** Effects or mixing tools (EQ, reverb, compression, panning); that work happens in the DAW after export. Running demucs inside the app; stems are separated beforehand and imported. Cross-platform support; Linux only.

**Criterios de aceptación:** Perceived latency from voice to on-screen line is under 40 ms. In a loopback test (output fed back into input), an exported take matches the backing within ~1 ms. A take recorded in an earlier session still plays aligned after the project is reopened. Per-take WAVs and the mixdown all start at song t=0 and have the same length/sample rate, so they import into a DAW without manual shifting. The shape score is the same for the same contour sung at different transpositions. Mute, solo, volume and delete per take behave as expected in both playback and the mixdown. Transposition (±st, drag, snap) moves both the ghost and the cents target.

**Incertidumbres abiertas:** Pitch correction and punch-in/partial re-recording are not part of this change, but they aren't ruled out for later. Diatonic (in-key) transposition and CREPE-based reference analysis are deferred to a later change. It's unclear whether pYIN is robust enough on demucs stems with artifacts (reverb tails, bleed). Whether the PipeWire quantum can be lowered per node to ~256 reliably on this machine (system default is 1024) is unverified. `pipewire-jack` isn't installed yet; setup steps (apt package, running under `pw-jack` or the system JACK lib replacement) still need to be defined. How the shape score handles timing drift (singing slightly early or late within a phrase) is undefined: plain RMS, or tolerant alignment (DTW)? The guide-stem playback level and whether its bleed is a concern even with headphones are unexplored. What "more performance" means as a criterion beyond the latency target was never specified.

## Nodos afectados

None — greenfield. The repo has no application code yet (no tracked files beyond the OpenSpec/Graphify harness), so there is no graph to query.

**Limitation:** the Graphify MCP failed to connect this session and `graphify-out/` (incl. `GRAPH_REPORT.md`) does not exist. This is expected with no code to index, not a missing configuration. Once the first Rust crate lands, the git hooks should produce `graphify-out/graph.json`; later changes should re-run `/opsx:explore` against it.

## Archivos relevantes

| Archivo | Rol |
|---|---|
| — | No source files exist yet; everything in this change is new. |

## Contexto knowledge

- `knowledge/` does not exist yet: no `INDEX.md`, no `decisions.md`, no modules or use-cases. No macro decisions to check against.
- `openspec/specs/` does not exist yet: no living capabilities this change modifies; all of its specs will be ADDED.
- `openspec/project.md` is still the template stub (Purpose, Tech Stack, Conventions, Domain, Constraints are `<TODO>`). This change effectively defines them: Rust, Linux-only, JACK API over PipeWire.
- Since this change establishes the first macro decisions (Rust, Linux-only, JACK-over-PipeWire, project-folder sessions), they are candidates for `knowledge/decisions.md` via `/documenter distill` after archive.

## Riesgos identificados

Observed on the target machine (2026-10-21):

- **`pipewire-jack` not installed.** `pw-jack` is absent and there are no PipeWire JACK libs; only `libjack-jackd2` is present, with no `jackd` server. A binary using the `jack` crate would link against jackd2's lib and fail to connect at runtime. Setup (install `pipewire-jack`, then run under `pw-jack` or install the system lib replacement) must be part of the change.
- **Default quantum threatens the < 40 ms target.** PipeWire runs at `clock.rate` 48000, `clock.quantum` 1024 (~21 ms per buffer). Buffer plus a 20–40 ms analysis window puts the live line at ~50–60 ms behind the voice. The app must request a smaller per-node quantum (e.g. `node.latency = 256/48000`); whether that is honored reliably on this system is unverified.
- **Sample-rate mismatch.** The device runs at 48 kHz while imported tracks are commonly 44.1 kHz, so resampling on import is required to keep sample-accurate alignment and same-rate exports.
- **Reference f0 quality on demucs stems.** Separation artifacts (reverb tails, instrument bleed) may cause octave errors or false voicing in pYIN; CREPE is deferred, so a voicing-confidence threshold and octave correction matter.
- **Live-detector octave errors and jitter** vs smoothing lag. This trades directly against the latency target.
- **Mic bleed.** Backing and guide-stem playback leaking into the mic would make the live detector track the playback. The design assumes headphones; this assumption should be stated explicitly.
- **Shape score vs timing drift.** Plain RMS after median-offset removal penalizes singing slightly early or late; tolerant alignment (e.g. DTW) is undecided.
- **Real-time safety in Rust.** The JACK process callback must not allocate, lock or do I/O; take writing and pitch analysis must be fed via lock-free ring buffers to separate threads.
- **Scope creep toward a DAW.** Take management is bounded to mute/solo/volume/delete/show-hide with whole takes; effects and editing are out.
