# Context: add-practice-aids

## Entrevista de intención

### Q&A

| Pregunta | Respuesta |
|---|---|
| What is the problem? | The singer can't reach the notes of the imported vocal stem because everything happens too fast, and the live voice often shows up as a succession of dots, so it's hard to know whether a note has been reached. |
| What does "too fast" mean: a quick melody, can't get onto the pitch before it changes, or can't tell which note the wiggly contour asks for? | Can't get onto the pitch before it changes. |
| When the dots appear, what is the singer doing (held vs moving notes, soft/breathy, mic gain)? | Doesn't know. (Measured instead: an earlier take, `eins/takes/take-003.wav`, was 38 % clipped with a −12 dBFS noise floor; every short gap came from the clarity gate, and 64 % of voiced runs were < 20 ms.) |
| What material is this for? | The isolated vocal of Bizarre Love Triangle '94. (Measured from its cached reference contour: syllables ~260 ms median, ~7 pitch changes per second of singing, 1-semitone typical step, ±20 ¢ median wobble within held notes.) |
| Is slowed playback acceptable as practice-only (no recording while slowed)? | Yes, practice-only is fine. |
| Fixed speeds or a continuous slider? | Fixed speeds are enough (50 / 60 / 75 / 100 % proposed). |
| One change or two (slow practice / steady note targets)? | One change covering both. |
| Note bars on top of the dimmed real contour, or bars only? | Bars on top of the dimmed contour. |
| When does a note count as reached? | ≥ 100 ms inside the in-tune band (±20 ¢ of the bar) at any point during the note; notes shorter than 100 ms need half their length. |
| What do the live line colour and the readout deviation compare against? | The bar (the note's steady pitch), not the raw contour. |
| Include an input level meter with a clipping warning? | Yes, in this change. |
| Speed persistence and shortcuts? | Not answered explicitly. Assistant proposed: speed always starts at 100 % (not saved per project), `,` / `.` for slower / faster; the user did not object. |

### Intent brief

**Intención:** The singer can't land on the notes of a quick, syllabic guide (Bizarre Love Triangle '94: ~260 ms syllables, ~7 pitch changes/s, ±20 ¢ wobble). The live line breaks into dots, so they can't tell when a note has been reached. Practice needs more time per note, steady targets and a clear "reached" signal.
**Alcance:** (1) Slow practice: fixed speeds of 50 / 60 / 75 / 100 % with pitch preserved; backing + guide pre-rendered stretched by a worker job and cached; song time advances at the chosen speed (mic stamps and latency compensation scaled); practice-only. (2) Steady note targets: the guide is split into notes with one steady pitch each, drawn as bars over the dimmed real contour; a per-note "reached" indicator; live colour and readout deviation measured against the bar. (3) A cleaner live line: voicing hysteresis and bridging of short gaps. (4) An input level meter with clipping / too-quiet warnings.
**Comportamiento observable:** Speed buttons in the transport (and `,` / `.`). The first use of a speed shows progress while it renders. Slowed playback is in the same key, the plot scrolls slower, and loop phrase works. R while slowed does not record and says why; takes are silent while slowed (their contours still drawn). The guide shows steady bars over a dimmed contour. A bar is marked reached after ≥ 100 ms inside ±20 ¢ (notes under 100 ms: half their length). Holding a steady note inside a bar stays green even when the guide wobbles. A held note draws a continuous line, not dots. The top bar shows the mic level and warns when it clips or is too quiet.
**Non-goals:** Recording while slowed; a continuous speed slider; stretching the user's takes; real-time stretching in the audio callback; changing the guide analysis (pYIN / CREPE); changing the phrase shape score.
**Criterios de aceptación:** At every speed the guide sounds at the same pitch as at 100 %. The live line lines up with the guide when slowed. Switching speed keeps the position. Back at 100 %, takes record aligned as before. On Bizarre Love Triangle '94 the bars follow the melody you hear. A steady sung note on a clean input draws an unbroken line. Holding a bar in band for ≥ 100 ms marks it reached. The meter warns on a clipped input like take-003.
**Incertidumbres abiertas:** Whether the dots persist on a correctly levelled input (no fresh test take yet, so the hysteresis thresholds are untested on clean input). Which time-stretch library (signalsmith-stretch / Rubber Band / homemade WSOLA) and its quality on a full mix. Whether note segmentation of a demucs + pYIN contour is reliable on glides and short notes. The thresholds for the meter's "too quiet" / "clipping" warnings. The speed always starting at 100 % and the `,` / `.` shortcuts were assistant proposals the user didn't object to, not explicit answers.

## Nodos afectados

> **Limitation:** the Graphify MCP did not connect in this session and there is no `graphify-out/GRAPH_REPORT.md`
> (no `graphify-out/` directory exists). This snapshot was derived by reading the code directly, not from the
> subgraph; nodes are named by symbol (`module::symbol`) instead of `graph://`. Re-consolidate with
> `/opsx:explore add-practice-aids` once the graph is available.

**Song clock / transport (practice speed)**
- `audio::engine::Processor::process`: advances `pos += n` per block, mixes at `pos`, stamps mic blocks with `pos - comp`. Below 100 % the position advances by `n × speed` and the compensation is scaled.
- `audio::engine::Command` (`Seek`, `SetPlayback`, `SetLoop`, `StartRecord`) and `audio::engine::Shared` (`song_pos`, `playing`, `recording`, `compensation`): the path for setting the speed without breaking real-time safety. `SetPlayback` already swaps immutable snapshots and retires old ones off the audio thread.
- `audio::mixer::Playback` / `Track` / `mix_into`: reads samples by song frame; with pre-stretched audio it reads at `pos / speed`.
- `audio::latency::compensation`: latency is in wall frames; in song frames it is `comp × speed`.

**Wall → song mapping of the live detector**
- `pitch::live_thread::spawn`: `song_frame = s.song + (f.frame - s.wall)`; needs the speed of the block.
- `audio::engine::BlockHeader` (`wall_frame`, `song_frame`, `playing`): candidate to carry the per-block speed.

**Voicing and the live line (the dots)**
- `pitch::live::detect` / `LiveDetector::estimate`: two gates, level (`POWER_THRESHOLD_MS` ≈ −50 dBFS) and clarity (`CLARITY_THRESHOLD` 0.7). Where hysteresis goes.
- `pitch::live::OctaveFilter::push`: median of 3; drops isolated voiced frames (makes flicker worse).
- `ui::plot::show` ("Live line" block): breaks the line at every unvoiced frame and draws a 1.5 px dot after each break; where short-gap bridging goes.
- `ui::app::App::drain_live` / `live_reading` / `LIVE_FRESH` (150 ms): the readout already hides gaps < 150 ms; the line does not.

**Steady note targets**
- `pitch::contour::Contour` (pYIN reference contour, hop 256): source for note segmentation.
- `feedback::phrases` / `Phrase`: precedent for a segmentation derived from the reference contour (phrases split at gaps ≥ 250 ms).
- `feedback::Band`, `IN_TUNE_CENTS`, `NEAR_CENTS`, `deviation_cents`: bands reused against the bar.
- `ui::session::Session::target` / `target_hz_at`: today returns the raw transposed contour; the live colour, readout, corridor and note-to-sing marker all query it.
- `ui::plot::draw_contour` / `draw_band`: the ±20/±50 ¢ corridor follows the raw contour; it moves to the bar, with the contour dimmed underneath.
- `ui::session::Pass` / `PhraseScore`: per-phrase shape score (non-goal: unchanged).

**Stretched pre-render**
- `project::worker::Job` / `WorkerEvent` (`AnalyseReference` with `cache`): background job with progress and disk cache, the pattern to replicate for stretching.
- `project::Project::reference_cache`: precedent cache path (`cache/reference.f0`, keyed by stem hash + analyzer version).
- `ui::session::Session::playback`: builds the `Playback` (backing, guide, audible takes); below 100 % it uses the stretched tracks and silences takes.

**Level meter**
- `audio::engine::MicStream` (live stream) consumed by `pitch::live_thread`: the place outside the callback to compute level/clipping.
- `ui::panels` top bar (🎤/🔊 indicator) and `transport_bar`: UI for speeds and the meter; `readout`: deviation against the bar.

## Archivos relevantes

| Archivo | Rol |
|---|---|
| `src/audio/engine.rs` | JACK callback, `Command`, `Shared`, song clock advance, mic block stamping |
| `src/audio/mixer.rs` | `Playback` / `Track` / `mix_into`: reading tracks by song frame |
| `src/audio/latency.rs` | `compensation` (scales with speed) |
| `src/pitch/live_thread.rs` | Wall → song mapping of each live estimate |
| `src/pitch/live.rs` | Voicing gates, `OctaveFilter`, hysteresis |
| `src/pitch/contour.rs` | `Contour`: source for note segmentation |
| `src/feedback/mod.rs` | Bands, deviation, phrase segmentation (precedent for notes), "reached" |
| `src/ui/plot.rs` | Live line, corridor, contours, note bars, note-to-sing marker |
| `src/ui/session.rs` | `target` / `target_hz_at`, `playback()`, `Pass` |
| `src/ui/app.rs` | `drain_live`, `live_reading`, record guard, loop, keyboard shortcuts |
| `src/ui/panels.rs` | Top bar (meter), `transport_bar` (speeds), `readout` |
| `src/ui/timeline.rs` | Song overview (could reflect reached notes; optional) |
| `src/project/worker.rs` | New stretch job with progress |
| `src/project/mod.rs` | Project cache paths |
| `Cargo.toml` | New time-stretch dependency (to be chosen) |
| `openspec/changes/add-pitch-follow-recorder/specs/{audio-engine,live-pitch,pitch-feedback,reference-contour}/spec.md` | Requirements this change modifies (see Risks) |

## Contexto knowledge

There is no `knowledge/` yet (no `knowledge/INDEX.md`, no `knowledge/decisions.md`, no use-cases): no macro decisions or curated notes to check. There is no `openspec/specs/` either; the current contract lives only in the unarchived change `add-pitch-follow-recorder` (49/58 tasks). Relevant from that change:

- **`audio-engine` → "Single song clock"**: the position advances *only by the frames processed in the callback*. Practice speed changes that (it advances `n × speed`).
- **`audio-engine` → "Real-time safe processing"** and the `project.md` constraint: no allocation, locks or I/O in the callback; retired buffers dropped off-thread. Supports offline pre-rendering over real-time stretching.
- **`audio-engine` → "Latency-compensated capture"**: every sample is placed on the song timeline with compensation; must still hold when slowed.
- **`pitch-feedback` → "Live cents deviation"**: deviation and the ±20/±50 ¢ corridor around the *transposed target* (raw contour). This change moves it to the note bar.
- **`live-pitch` → "Live pitch estimation" / "Live contour display" / "Voice-to-display latency"**: unvoiced consonants must stay unvoiced; a continuous line ending in a dot; < 40 ms voice-to-line budget.
- **`reference-contour` → "Fixed playhead with future guide"**: ≥ 4 s of guide visible ahead, in song seconds.
- **That change's design.md**: median of 3 chosen over 5 for latency (one hop); `dropped_live` separate from `dropped_disk`; pYIN refined with MPM (±3 ¢ on test tones). The guide's ±20 ¢ wobble was measured on the real stem, so analysis precision is not the problem; the lack of a steady target is.

Measured during this exploration (throwaway script, not project code):
- Bizarre Love Triangle '94 guide (`~/Music/recordings/zwei/cache/reference.f0`): 47 % voiced; syllables median 260 ms; ~7 pitch changes per second of singing; typical step 1 st (p90 4 st); wobble inside held notes median 20 ¢ (std).
- Take `~/Music/recordings/eins/takes/take-003.wav` (Oct 22): 38 % of samples above 0 dBFS, −12 dBFS RMS floor with energy at 25–50 Hz; 100 % of short gaps from the clarity gate; 64 % of voiced runs < 20 ms. Hysteresis (enter 0.7 / −50 dBFS, hold 0.5 / −55 dBFS) cut the runs from 121 to 49 and raised the median from 11 to 48 ms. `take-001.wav` is entirely silent (wrong input).

## Riesgos identificados

- **Archive order:** the requirements to modify (`Single song clock`, `Live cents deviation`, `Live contour display`, `Live pitch estimation`) exist only as deltas of the unarchived change `add-pitch-follow-recorder`. This change's MODIFIED deltas need that one archived first (or must be written as ADDED); otherwise `openspec validate/archive` has nothing to merge against.
- **Conflict with "Single song clock":** "advanced only by the frames processed" stops being true when slowed. The requirement must be modified explicitly, not worked around.
- **Conflict with "Live cents deviation":** today the colour and readout compare against the transposed contour; moving to the bar is a contract change (and affects harmony mode: when the target is a take, its notes would need segmenting too).
- **Real-time safety:** the speed must reach the callback through a `Command`/atomic and the stretched tracks through `SetPlayback` (immutable snapshot). No stretching inside the callback (already a non-goal).
- **Alignment when slowed:** compensation and the wall→song mapping in `live_thread` must scale by the speed of the block the sample was captured in; changing speed mid-playback must not misalign the line. Latency calibration must run at 100 %.
- **Record guard:** R, calibration and any path that issues `StartRecord` must be blocked below 100 %; the loop is already disabled while recording.
- **Hysteresis vs consonants:** holding voicing down to clarity 0.5 can keep unvoiced consonants or noise voiced, contradicting the "Silence and consonants" scenario. Thresholds are unvalidated on a clean input (no fresh test take).
- **Real source of the dots:** the evidence is one old, clipped take; the source gain is now 0.60. If clipping was the main cause, hysteresis + bridging fix less than expected and the meter fixes more.
- **Note segmentation on demucs + pYIN:** glides, attacks and notes < 100 ms may be split or merged wrongly; the ±20 ¢ wobble is the same order as the band. Needs validation on the real guide.
- **Time-stretch quality and licence:** Rubber Band is GPL (affects the binary's licence); signalsmith-stretch is MIT but needs a C++ build; a homemade WSOLA sounds worse on a full mix. To be decided in design.
- **Memory and render time:** a 4-min stereo backing is ≈ 92 MB at 100 %, ≈ 150 MB at 60 %; keep only the selected speed in memory, the rest in the disk cache. The render must show progress and not block the UI.
- **Phrase scores when slowed:** the shape score is computed in song frames and would still appear at 60 %; whether it is shown, marked as "practice" or omitted is undecided (the non-goal only says the calculation does not change).
