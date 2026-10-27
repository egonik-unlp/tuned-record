# Design: add-pitch-follow-recorder

## Approach

Greenfield: there is no code graph yet (context.md → Nodos afectados), so this design references no `graph://` nodes. The module layout below is what the first graph will contain; later changes should reference its nodes.

### Shape

One Rust crate (`tuned-record`) with a `lib` (all logic, unit-testable without audio hardware) and a thin `bin` (window + JACK client). Linux only.

```
src/
  audio/      engine (JACK client, process callback), mixer, capture, latency math, calibration
  pitch/      live detector (McLeod), octave filter, offline analyzer (pYIN), contour types
  feedback/   cents deviation, phrase segmentation, shape score
  project/    manifest, folder layout, import (decode + resample), take storage, export
  ui/         eframe app, pitch plot painter, transport bar, guide panel, takes panel, dialogs
```

### Threads and data flow

```
 ┌───────────────────────────┐ rtrb: mic frames + song pos ┌────────────────────┐ crossbeam ┌──────────────┐
 │ JACK process callback (RT)│────────────────────────────▶│ live pitch thread  │──frames──▶│ UI thread    │
 │ • read mic, write outputs │                             │ McLeod + octave    │           │ (eframe)     │
 │ • mix backing/guide/takes │ rtrb: mic frames            │ filter, hop 128    │           │ plot @ vsync │
 │ • advance song clock      │──────────┐                  └────────────────────┘           └──────┬───────┘
 │ • read commands (rtrb)    │◀─────────┼──────── transport/mix commands (rtrb) ─────────────────────┘
 └───────────────────────────┘          ▼
                               ┌────────────────────┐        ┌────────────────────────────────────┐
                               │ disk writer thread │        │ worker thread (offline jobs)       │
                               │ hound, take_N.wav, │        │ import: symphonia → rubato → f32   │
                               │ flush ~1 s         │        │ pYIN on stem / finished takes      │
                               └────────────────────┘        │ export: render per-take + mixdown  │
                                                             └────────────────────────────────────┘
```

- **RT callback** never allocates, locks or does I/O. All audio it plays (backing, stem, takes) is preloaded as `Arc<[f32]>` at the engine rate; swaps happen by publishing new `Arc`s through a lock-free command ring and dropping old ones off the RT thread (a "garbage" ring back to the UI thread).
- **Mix state** (per-take volume/mute/solo, guide on/level) is sent as commands; the callback keeps its own copy.
- **Song clock**: `u64` frame counter owned by the callback; published to other threads through an atomic.

### Audio engine (JACK over PipeWire)

- `jack` crate client, 1 input + 2 output ports, auto-connected to system ports. Run with PipeWire's JACK libs (`pipewire-jack`). README documents: install package, then either `pw-jack tuned-record` or install the system-wide JACK lib replacement.
- Buffer: set `PIPEWIRE_LATENCY=256/<rate>` in the process environment before the client opens (PipeWire's JACK layer reads it as the node latency request). Actual buffer size is read back from the client and shown.
- Project sample rate is fixed at creation (= server rate). Opening a project at a different server rate shows an error and does not start the transport (no live resampling in this change).

### Latency compensation

At a callback where the output buffer starts at song frame `P`:

- what the user hears *now* was written `Lo` frames ago (`Lo` = playback port latency, max of range),
- the mic samples in this buffer were captured `Li` frames ago (`Li` = capture port latency),

so input sample `i` corresponds to what the user was hearing at song frame

```
s(i) = P + i − Lo − Li − C          C = calibration offset (frames, from config)
```

Takes are written at `s(i)`; the live contour is plotted at `s(i)` too, so the live line sits where the user *meant* to sing. `Lo`/`Li` are re-read on JACK latency callbacks.

**Calibration** (loopback): play a click at a known song frame, find it in the capture by cross-correlation, store the residual as `C` in `~/.config/tuned-record/config.toml`. This is a *residual* on top of reported latencies, so it stays small and survives buffer-size changes.

- *Decision (apply):* the calibration waits until the engine reports `recording` before watching for the stop position. Commands are applied at the next callback, so right after `Seek(0)` the published `song_pos` can still be the old position — without the wait, calibrating from e.g. 2:00 stopped immediately. Found by the loopback check.
- *Decision (apply):* separate drop counters — `dropped_live` (gaps in the live line only) and `dropped_disk` (audio lost from a take). The UI reports only the latter; a single counter made an undrained live stream look like lost recording audio.
- *Verification (apply):* `examples/loopback_check.rs` patches the client's output into its own input inside JACK, calibrates, records a take over a click backing, exports it and cross-correlates: **0 frames** off (also over a 5-minute take, `--long`). That patch has zero delay in PipeWire, so it validates the record/export path and the math but not a non-zero compensation; routing through a delaying `pw-loopback` node stalls the graph (a feedback cycle through another node isn't scheduled), so the non-zero case needs a physical cable (`--physical`).

### Live pitch

- McLeod Pitch Method, own implementation (`pitch::mpm`: NSDF via zero-padded `realfft` autocorrelation + key-maxima peak picking + parabolic interpolation), window 1024 frames (21 ms at 48 kHz), hop 128 (≈ 375 updates/s). Clarity threshold 0.7 gates voicing; a mean-square floor (~−50 dBFS) gates silence.
  - *Decision (apply):* the `pitch-detection` crate was dropped. Its McLeod read a pure 220 Hz sine as +10 ¢ with clarity 0.78 at 1024 frames (70 Hz at 2048: clarity 0.67, below threshold), which breaks the ±10 ¢ requirement. The own NSDF is within 5 ¢ with clarity > 0.9 on the same signals.
- Range: 1024 frames resolves down to ~94 Hz (lag ≤ half the window). Whenever the short window finds nothing or a pitch below 220 Hz, the 2048 window is also run and wins if it finds a pitch below 110 Hz — the short window can report low notes an octave up, so it can't be the only trigger (accepting more latency on low notes). See Riesgos.
- Octave filter: **3-frame** median on the MIDI value (a single wrong frame — e.g. an octave jump — never wins; a voiced frame needs a voiced neighbour, so isolated blips in silence are dropped).
  - *Decision (apply):* 3 instead of the planned 5: 5 costs two hops of delay, 3 costs one, and both reject a single wrong frame, which is what the spec asks.
- **Latency budget** at 256/48 kHz, as measured during apply:
  | Stage | Time |
  |---|---|
  | Capture latency `Li` reported by JACK (includes one buffer of block granularity) | 6.0 ms (288 frames) |
  | Tone needed before a voiced frame is emitted (MPM needs ≈ 2.3 periods ≈ 10.7 ms at 220 Hz, + hop + 1-hop median) — unit test `onset_detection_delay_under_15_ms` | 14.7 ms |
  | Live thread polling | ≤ 1 ms |
  | UI: the live thread requests a repaint on every batch; next frame at vsync | ≤ 16.7 ms (avg ≈ 8) |
  | **Total** | **≈ 38 ms worst case, ≈ 30 ms average** |

  Display scan-out after the frame is presented is monitor-dependent and not included. Window size barely matters for onset (1024, 768 and 512 all need ≈ 10 ms of tone): it's a physical limit of pitch estimation, not a tuning knob.
- The live thread runs the detector on the free-running **wall** frame clock (never discontinuous, so seeks don't reset it) and maps each estimate back to a song frame through the block headers (`pitch::live_thread`). Estimates made while stopped have no song frame; they feed the readout but are not drawn, so the plot stays frozen while stopped.

### Offline analysis (reference and takes)

- pYIN for the stem: frame 2048, hop 256 (5.3 ms), f0 range 65–1100 Hz, outputs f0 + voiced probability. Implementation: evaluate the `pyin` crate first; if it doesn't fit (API, accuracy, speed), port librosa's pYIN (YIN candidates + HMM Viterbi) in-house. Results are deterministic and unit-tested against synthetic signals.
  - *Decision (apply):* the `pyin` crate is used, at **0.25-semitone** bin resolution (not 0.1): Viterbi cost grows with the bin count, and 0.1 ran at 0.64× real time (a 4-min stem ≈ 2.6 min). pYIN's f0 is quantised to its bins, so each voiced frame is **refined with MPM** restricted to ±10 % around the pYIN value (falls back to the pYIN value if MPM finds nothing). Chunks of 30 s (+64 hops of context each side) run **in parallel** on all cores. Result: a 4-min 48 kHz stem in ≈ 6 s, refined pitch within 3 ¢ on test tones.
- Cache: `cache/reference.f0` keyed by BLAKE3 hash of the stored stem + analyzer version.
- Takes: while recording, the take's contour is the stream of live frames (instant feedback). After the take stops, the worker re-analyses the compensated take audio with pYIN and replaces the contour (more accurate, aligned by construction).

### Guide transposition

Offset stored as cents (`i32`). Step buttons/keys: ±100. Drag: snaps to 100-cent steps; Shift+drag is free. Snap-to-voice: `offset = round_to_semitone(midi_live − midi_ref(playhead))`; disabled when either is unvoiced. The offset is display + cents target only; guide audio is never pitch-shifted.

### Feedback

- Deviation: `1200·log2(f_live / f_ref_shifted)` at the playhead frame. Bands: in tune |d| ≤ 20 ¢, near ≤ 50 ¢, off > 50 ¢.
- Phrases: reference voiced segments split at unvoiced gaps ≥ 250 ms; very short segments (< 150 ms) merge into the neighbour.
- Shape score: user contour resampled onto reference frame times; `d = cents_user − cents_ref` over frames where both are voiced; `score = RMS(d − median(d))`; requires coverage ≥ 50 % of the phrase's voiced reference frames. Computed from the live contour at phrase end (displayed immediately) and recomputed from the pYIN take contour; the latter is the stored one.
- Timing drift: plain RMS, no DTW, in this change (see Alternativas).

### Project folder

```
<name>/
  project.toml        manifest: format version, sample rate, sources, guide (on, level, offset), takes[]
  sources/backing.wav f32, project rate (resampled copy of the import)
  sources/vocals.wav  f32, project rate
  cache/reference.f0  binary: header (hop, rate, analyzer ver, hash) + [f0, voiced_prob] per frame
  takes/take-NNN.wav  mono f32, starts at the take's own start frame (offset in manifest)
  takes/take-NNN.f0   contour, same format as reference
  takes/take-NNN.scores.json
```

- Manifest written atomically (write temp + rename) on every settings change, debounced ~300 ms.
- Take WAVs written by the disk thread with `hound`, `flush()` about every second so the header stays valid (crash leaves at most ~1 s lost). On open, a take WAV without a manifest entry is recovered as a new take.

### Export

Worker renders offline from project data (not from the live engine):

- Per take: mono f32 WAV, length = song length, silence until the take's start frame. All takes, regardless of mute/solo.
- Mixdown: stereo f32 WAV of backing + audible takes (mute/solo/volume as currently set), no guide. Float output satisfies "no clipping"; peak level is reported in the export dialog.
- Song length = max(backing length, stem length, last take end).
- Names: `<project>-mixdown.wav`, `<project>-take-NNN.wav` into a chosen folder.

### UI

- Scores are computed for **every playback pass**, not only while recording: the live contour of the current pass (`ui::session::Pass`, reset on play/seek/jumps) is scored as the playhead leaves each phrase. Scores stored with a take come from its pYIN contour.
- *Addition (apply):* `tuned-record <project dir>` opens a project at start-up (used to verify the UI against a generated project, `examples/demo_project.rs`).
- Vertical view follows the current voice (last 0.5 s only), else the median of the visible transposed reference, with hysteresis (starts when a quarter span off, glides to within half a semitone); the mouse wheel adds a manual shift, double-click resets it.

`eframe`/`egui` with a custom painter for the pitch plot (not `egui_plot`): full control over the ghost style, layered contours and per-frame coloring. Only the visible time window is drawn (binary search by time, ≤ ~1 point per pixel column per contour). Repaint is continuous while the transport runs or the mic is voiced. Visible-layer details are in ui-ux.md.

## Archivos a modificar

Greenfield (context.md → Archivos relevantes): every file is new.

| Archivo | Cambio |
|---|---|
| `Cargo.toml`, `src/main.rs`, `src/lib.rs` | New crate: `bin` wiring JACK + eframe, `lib` with all logic |
| `src/audio/*` | New: engine, mixer, capture, latency math, calibration |
| `src/pitch/*` | New: McLeod live detector, octave filter, pYIN analyzer, contour format |
| `src/feedback/*` | New: deviation, phrases, shape score |
| `src/project/*` | New: manifest, import, take storage, recovery, export |
| `src/ui/*` | New: app, plot painter, panels, dialogs |
| `README.md` | New: build + `pipewire-jack` setup + loopback calibration |
| `openspec/project.md` | Fill Purpose / Tech Stack / Constraints from this change |

## Alternativas consideradas

- **cpal** instead of JACK: rejected. Separate input and output callbacks with independent clocks; alignment would rely entirely on measurement and can drift. JACK gives one cycle and reported port latencies.
- **Native `pipewire` crate (pw_filter)**: rejected for now. Same synchronous model, but the Rust bindings are less mature and latency plumbing is heavier. The JACK API also works with a real jackd.
- **Python / web stack**: rejected by the user's performance requirement; the web also adds browser audio latency on Linux.
- **CREPE (ONNX) for the reference**: deferred. Higher accuracy on noisy stems but a model dependency; pYIN first, revisit if demucs artifacts hurt.
- **YIN for live**: MPM chosen for its better behaviour on short windows (fewer octave errors at 1024 frames).
- **`egui_plot`**: rejected for the pitch view; custom painting is needed for ghost/future styling and per-frame colors, and to control draw cost.
- **DTW-tolerant shape score**: deferred. Plain RMS is simpler, transposition-invariant by construction, and testable; drift tolerance can be added later without changing the stored contour data.
- **Referencing source files instead of copying**: rejected. Copies (resampled) make projects self-contained and avoid resampling on every open.
- **Live resampling when server rate ≠ project rate**: deferred; a clear error is enough for a single-machine tool.

## Riesgos

- **`pipewire-jack` missing** on the target machine today: the first run fails until it is installed. The engine's startup error must say so (spec'd).
- **`PIPEWIRE_LATENCY` not honored** (e.g. a device forcing a larger quantum): latency target missed. The actual buffer is displayed; the target is only claimed at 256 frames.
- **Low notes vs latency**: 65–90 Hz needs a 2048 window, adding ~21 ms on those notes; the < 40 ms criterion is measured on the 1024-window path.
- **pYIN on demucs stems**: reverb tails and bleed can create false voicing or octave errors in the reference. Mitigation: voiced-probability threshold exposed in code, octave filter reused; CREPE is the fallback change.
- **`pyin` crate fitness** is unverified; porting pYIN in-house is the fallback and costs more time.
- **Live vs pYIN take contours** may disagree slightly; scores shown during recording can change after re-analysis. Stored score = pYIN-based.
- **Mic bleed**: headphones are assumed; without them the live detector tracks the playback. Stated in the README.
- **No input monitoring**: the user hears their voice only acoustically or through their interface's direct monitoring; software monitoring is not part of this change.
- **Memory**: preloading a 5-minute stereo song + stem + 10 mono takes as f32 at 48 kHz ≈ 230 MB. Acceptable for desktop; streaming from disk is a later optimisation.
- **Scope creep toward a DAW**: take management stays at mute/solo/volume/show-hide/delete.
