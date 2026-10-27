# Design: add-practice-aids

## Approach

Graphify was unavailable when this change was consolidated (context.md → Nodos afectados), so nodes are referenced by symbol (`module::symbol`) instead of `graph://`. Four independent pieces share one change; each is described with its decisions.

```
                          ┌──────────── UI thread ─────────────────────────────────────────┐
 speed buttons ──────────▶│ Session::playback(speed) ─▶ Command::SetSpeed{ratio, Arc<Playback>}│──┐
                          │ Session::notes (segmented target) ─▶ plot bars, corridor, readout │  │
                          │ Pass::reached (per-note) ◀── drain_live ◀── LivePoint{hz,level}   │  │
                          └───────────────────────────────────────────────────────────────────┘  │
        worker: Job::Stretch ─▶ cache/stretch-*.wav ─▶ Arc<Audio> per speed                      │
                                                                                                 ▼
 ┌ JACK callback (RT) ──────────────────────────┐  mic blocks + {song, wall, ratio}  ┌ live-pitch thread ───────┐
 │ pos advances n·p/q (exact remainder carry)   │───────────────────────────────────▶│ wall→song via ratio      │
 │ mixes stretched tracks at (pos−start)·q/p    │                                    │ hysteresis voicing       │
 │ refuses StartRecord unless ratio = 1         │                                    │ level/peak → InputLevel  │
 └──────────────────────────────────────────────┘                                    └──────────────────────────┘
```

### 1. Practice speed

**Speeds are exact ratios.** 50 / 60 / 75 / 100 % are `1/2`, `3/5`, `3/4`, `1/1` (`p/q`). The callback keeps the song position as an integer and carries the remainder: `acc += n·p; pos += acc / q; acc %= q`. Song frames stay integers everywhere (contours, loop ranges, takes), and at `1/1` the arithmetic reduces to today's `pos += n` exactly, so recording alignment at 100 % is unchanged by construction.
- *Decision:* integer ratios instead of an `f32` speed. Rationale: no drift over a 4-minute song, no change to the song-frame type, and a bit-identical path at 100 %.

**Speed and audio change together.** A new `audio::engine::Command::SetSpeed { ratio, playback: Arc<Playback> }` swaps the clock ratio and the matching stretched snapshot in the same callback, so there is never a block where the clock runs at one speed and the audio at another. The old snapshot is retired through the existing garbage ring (as `SetPlayback` does). `Shared` publishes the ratio for the UI.
- `StartRecord` is ignored by the engine unless the ratio is `1/1` (defence in depth; the UI also blocks R and Calibrate… with a message).

**Reading stretched audio.** `audio::mixer::Track` gains the ratio its audio was rendered at. A stretched track holds `(len · q / p)` frames; for output frame `o` of a block starting at song frame `pos`, it reads index `⌊(pos − start) · q / p⌋ + o`. That is one contiguous read per block, like today, and no interpolation in the callback. At `1/1` this is today's `mix_into`.

**Stamping the mic.** `BlockHeader` carries the ratio of its block. The callback stamps `song_first = pos − comp · p / q` (compensation is wall latency converted to song time at the current speed), and `pitch::live_thread` maps an estimate inside the block with `song = s.song + (f.frame − s.wall) · p / q`. Across a speed change, the error is bounded by the compensation (~10–20 ms of the old speed) for one round-trip, which is acceptable for practice and never touches takes (recording is 100 % only).

**Rendering and caching.** A new `project::worker::Job::Stretch { source, ratio, cache }` renders a source (backing, guide stem) at a ratio, reporting progress through `WorkerEvent` like `AnalyseReference`. Output: `cache/stretch-<source>-<pct>.wav` (32-bit float, engine rate) plus a key (blake3 of the source copy + stretcher version + ratio) checked before reuse; a mismatch re-renders. `ui::session::Session` keeps the original tracks always (needed at 100 %) and only the stretched tracks of the selected speed; `Session::playback` takes the ratio: below `1/1` it returns stretched backing + stretched guide (when audible) and no takes.
- While a render runs, the transport stays at the previous speed (spec: "staying at the previous speed until the render is ready"); selecting another speed cancels the pending one.
- *Decision:* offline pre-render, not real-time stretching. Rationale: real-time safety comes for free (the callback only reads), seeking and looping stay trivial, and the cost is a one-time render per speed.

**Stretcher library.** Evaluate `signalsmith-stretch` (MIT, Rust wrapper over Signalsmith Stretch; builds C++ via `cc`) first: it targets polyphonic material, so it should hold up on a full backing mix. Accept it if a 4-min stereo backing renders at 50 % in under ~30 s and a sustained guide note keeps its pitch within ±5 ¢. Fallback: a homemade WSOLA in `pitch`/`audio` (fine on the vocal stem, audibly worse on a mix).
- *Decision:* Rubber Band (`rubberband` crate) is not used. Rationale: GPL would change the licence of the binary.

**Defaults.** The ratio starts at `1/1` on every project open and is not stored in the manifest (intent interview). `,` / `.` step through the four speeds. Loop wrap is in song frames and works unchanged; the 1 s pre-roll and 0.5 s tail stay in song time (they last longer in wall time when slowed, which is fine for practice).

### 2. Note targets

**Segmentation** lives in `feedback` (next to `feedback::phrases`, which already derives structure from the reference contour): `feedback::notes(contour, rate) -> Vec<Note>` with `Note { start, end, midi: f32 }`, computed in MIDI units without the transposition offset (applied when drawing and comparing, like today).
1. Convert voiced frames to MIDI and median-smooth over ~50 ms to remove single-frame jitter.
2. Within each voiced run, grow a note while the smoothed pitch stays within ±0.5 st of the note's running median; a departure that lasts ≥ 30 ms ends the note (a shorter one is wobble).
3. Segments shorter than 40 ms between two notes are glides: dropped. A whole voiced run shorter than 40 ms becomes one note.
4. The note's pitch is the median of the middle 60 % of its frames (attack and release glides excluded).

- *Decision:* note pitches are **not** snapped to the 12-TET grid. Rationale: records are not always tuned to A440 and demucs stems keep the record's tuning; snapping would make a correctly sung note look off. The note label shows the nearest note name.
- *Validation:* an example (`examples/`) prints the note count and the duration distribution for a contour file. On the Bizarre Love Triangle '94 guide the target is notes on the order of syllables (median ≈ 200–260 ms), not the 85 ms fragments a naive semitone-rounding produced during exploration.

`ui::session::Session` computes `notes` alongside `phrases` (in `Session::new`/when the reference loads) and recomputes them when the target changes (guide ↔ a take, or the target take is re-analysed). `Session::target_note_at(frame) -> Option<Note>` replaces the raw lookup; `Session::target_hz_at` returns the transposed note pitch. Every consumer moves with it: live colour, readout deviation and direction cue, the note-to-sing marker, snap-to-voice, and the corridor. `feedback::shape_score` keeps using the raw contour (non-goal).

**Drawing** (`ui::plot`): the raw target contour is drawn dimmed (thinner, lower alpha; ghost style ahead of the playhead kept), then the corridor (±50 ¢, ±20 ¢) as rectangles around each bar, then the bars themselves (solid behind the playhead, ghost ahead). Reached bars get a filled style plus a check glyph so the state is readable without colour.

**Reached** is tracked per pass in `ui::session::Pass` (already reset on a jump or restart). As `App::drain_live` feeds points, `Pass` keeps for the current note the length of the current in-band run (|deviation| ≤ 20 ¢ from the transposed note). The run survives unvoiced gaps shorter than 50 ms (the same bridge as the live line) and resets on any out-of-band voiced frame. When the run reaches 100 ms, or half the note length for notes shorter than 100 ms, the note's index joins `Pass::reached`.
- *Decision:* "continuously" means bridged gaps do not break the run. Rationale: otherwise a single dropout frame would undo a held note, which is the exact frustration this change addresses.

### 3. Cleaner live line

**Hysteresis** in `pitch::live::LiveDetector`: keep a voiced/unvoiced state. Unvoiced → voiced needs clarity ≥ 0.7 (today's `CLARITY_THRESHOLD`). While voiced, a frame stays voiced with clarity ≥ 0.5 (`HOLD_CLARITY`) **and** a pitch within 1 semitone of the last voiced pitch. The level gate (`POWER_THRESHOLD_MS`, ≈ −50 dBFS) is unchanged in both states. Implementation: `Mpm::detect` is already parameterised by the clarity threshold, so the detector calls it with 0.5 when in the voiced state.
- *Decision:* hold threshold 0.5 with the pitch-continuity condition, level gate unchanged. Rationale: on the measured take every gap came from the clarity gate, not the level gate; the continuity condition stops a held state from latching onto noise or a consonant at an unrelated pitch, which protects the "Silence and consonants" scenario. Unit tests: a tone with clarity dips stays voiced; white noise after a tone becomes unvoiced within a few hops; silence stays unvoiced.
- The median-of-3 `OctaveFilter` is unchanged; with fewer flickers it drops fewer isolated frames.

**Gap bridging** in `ui::plot::show`: when a voiced point follows a gap, it connects to the previous voiced point if the gap is shorter than 50 ms of wall time (`wall_frame`, so it behaves the same at every speed) and the pitch difference is under 1 semitone; otherwise the line breaks as today. Isolated starts keep their dot.

### 4. Input level

**Measurement off the RT thread.** `pitch::live_thread` already receives every mic block. It computes the block peak and keeps a shared `InputLevel` (atomics: decaying peak, short RMS, wall frame of the last clipped sample), and the detector exposes each frame's level, carried in `LivePoint` as `level_db`.
- *Decision:* no metering in the JACK callback. Rationale: the live thread sees the same samples; the callback stays as it is.

**Warnings** (UI, `ui::panels` top bar next to 🎤):
- *Clipping:* any sample with |x| ≥ 0.999 (float inputs can exceed 1.0; integer devices saturate at 1.0). Shown as text ("Input clipping — lower the gain") for at least 2 s after the last clipped sample.
- *Too quiet:* during playback, among the live frames of the last 2 s that are pitched (clarity ≥ 0.5), if there are at least 0.3 s of them and their median level is below −40 dBFS. Silence produces no pitched frames, so no warning between phrases.
- Thresholds are constants, validated by hand with a fresh take (open uncertainty in the intent brief).

## Archivos a modificar

| Archivo | Cambio |
|---|---|
| `src/audio/engine.rs` | `Command::SetSpeed`, ratio + remainder in the song clock, ratio in `BlockHeader`, compensation scaled, `StartRecord` refused below 1/1, ratio in `Shared` |
| `src/audio/mixer.rs` | `Track` ratio; stretched read in `mix_into` |
| `src/audio/latency.rs` | Helper converting compensation to song frames at a ratio |
| `src/pitch/live_thread.rs` | Ratio-aware wall→song mapping; `InputLevel` (peak, RMS, last clip) |
| `src/pitch/live.rs` | Voicing hysteresis (`HOLD_CLARITY`, pitch continuity); level in `LiveFrame` |
| `src/feedback/mod.rs` | `Note`, `notes()` segmentation |
| `src/ui/session.rs` | Stretched tracks per speed, `playback(ratio)`, `notes`, `target_note_at`, `Pass::reached` |
| `src/ui/app.rs` | Speed state and `,` / `.`, record/calibrate guard, stretch job handling, too-quiet logic, reached feeding |
| `src/ui/plot.rs` | Dimmed contour, bars, corridor around bars, reached style, gap bridging |
| `src/ui/panels.rs` | Speed buttons + render progress in the transport, input meter + warnings in the top bar |
| `src/project/worker.rs` | `Job::Stretch` + progress events |
| `src/project/mod.rs` | Stretch cache paths and keys |
| `Cargo.toml` | `signalsmith-stretch` (if the evaluation passes) |
| `examples/` | Note segmentation report for a contour file |
| `README.md` | Practice speeds, note bars, input meter, new keys |

## Alternativas consideradas

- **Real-time stretching in the callback.** Rejected: adds DSP state and allocation risks to the RT path and complicates seek/loop, for a practice-only feature where a one-time render is acceptable.
- **Rubber Band.** Rejected for licence (GPL). Kept in mind only if the user later accepts GPL.
- **Homemade WSOLA as the primary stretcher.** Fallback only: adequate on a solo voice, smeared transients and phasiness on a full mix.
- **Floating-point speed in the clock.** Rejected: drift and a non-identical 100 % path.
- **Recording while slowed (and stretching takes back).** Out of scope by decision (practice-only).
- **Semitone-quantised notes.** Rejected: off-A440 records and glide fragments (the exploration's rounding produced 85 ms median fragments).
- **Lowering the clarity threshold globally to 0.5.** Rejected: more false voicing in silence and consonants; hysteresis gets the benefit only inside notes.
- **Bridging only in the drawing.** Insufficient: the reached logic and the readout benefit from fewer dropouts too; bridging is kept as a second layer.
- **Metering in the JACK callback.** Unnecessary: the live thread already sees every sample.

## Riesgos

- **Archive order:** the MODIFIED deltas target requirements that live only in the unarchived `add-pitch-follow-recorder`; archive it first.
- **Segmentation quality** on demucs + pYIN: glides and short notes may split or merge wrongly. Mitigation: the example report on the real guide, tuning constants before the UI work depends on them.
- **Stretch quality and cost:** transients at 50 % may smear; render time and the C++ build requirement are unmeasured until the evaluation.
- **Hysteresis vs consonants:** covered by the continuity condition and tests, but unvalidated on a clean real input (no fresh take yet).
- **Speed changes mid-play:** brief live-line misalignment (≈ one round-trip latency) right after a change; takes are unaffected.
- **Memory:** originals + one stretched set (≈ 150 MB extra for a 4-min stereo backing at 60 %).
- **Phrase scores while slowed:** still computed in song frames; whether to show or mark them is undecided (not specified by this change).
