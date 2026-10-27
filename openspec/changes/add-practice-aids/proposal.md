# Proposal: Practice aids — slow practice, steady note targets, cleaner live line, input meter

**Change ID:** add-practice-aids
**Date:** 2026-10-27
**Author:** Eduardo Gonik
**Estado:** Aprobado — 2026-10-27

## Why

The singer can't reach the notes of the guide they imported, and can't tell when they have. Both problems were measured on the real material (context.md → Contexto knowledge):

- **Too fast to land on.** The guide is Bizarre Love Triangle '94: syllables of ~260 ms median, ~7 pitch changes per second of singing, a typical step of one semitone. Correcting pitch by ear takes ~150–200 ms, so at full speed a note is over before the singer has settled on it. The user described exactly this: "can't get onto the pitch before it changes".
- **A target that moves.** The in-tune corridor follows the raw pYIN contour, whose wobble inside held notes is ±20 ¢ (median std), as wide as the green band itself. A perfectly steady note drifts in and out of "in tune".
- **The voice shows up as dots.** The live line breaks at every unvoiced frame and starts again with a 1.5 px dot. On the one take available, 64 % of voiced runs were shorter than 20 ms, every short gap came from the clarity gate, and the input was 38 % clipped with a −12 dBFS noise floor. Nothing in the app showed the singer that their input was overdriven.

The intent interview (context.md → Entrevista de intención) settled the shape of the fix: a practice-only slow-down at fixed speeds, steady note bars over the dimmed contour with a per-note "reached" mark, deviation measured against the bar, a less flickery live line, and an input level meter, all in one change.

## Scope

**Comunidades afectadas:** (see context.md → Nodos afectados; derived from code because Graphify was unavailable): song clock / transport, live detector wall→song mapping, voicing and the live line, target/feedback, stretched pre-render (worker + cache), and the top bar / transport UI.

**Archivos principales:** see context.md → Archivos relevantes. Core: `src/audio/engine.rs`, `src/audio/mixer.rs`, `src/pitch/live_thread.rs`, `src/pitch/live.rs`, `src/feedback/mod.rs`, `src/ui/plot.rs`, `src/ui/session.rs`, `src/ui/app.rs`, `src/ui/panels.rs`, `src/project/worker.rs`, `Cargo.toml` (time-stretch dependency).

| Capability | Delta | What changes |
|---|---|---|
| `practice-speed` | ADDED | Fixed speeds of 50 / 60 / 75 / 100 % with pitch preserved; backing + guide pre-rendered stretched by a background job with progress and cached in the project; practice-only (recording blocked below 100 %, takes silent but still drawn); speed always starts at 100 %; `,` / `.` shortcuts; loop phrase works when slowed |
| `note-targets` | ADDED | The target contour (guide, or the take chosen as target) is segmented into notes with one steady pitch each, drawn as bars over the dimmed contour; a note is marked reached after ≥ 100 ms inside ±20 ¢ of its bar (notes shorter than 100 ms: half their length) |
| `input-level` | ADDED | Mic level meter in the top bar with a clipping warning and a too-quiet warning |
| `audio-engine` | MODIFIED | "Single song clock": the song position advances by the processed frames × the practice speed. "Latency-compensated capture": compensation holds at any speed |
| `live-pitch` | MODIFIED | "Live pitch estimation": voicing hysteresis (stays voiced through brief clarity dips). "Live contour display": short gaps between voiced frames are bridged, so a held note is one line |
| `pitch-feedback` | MODIFIED | "Live cents deviation": the corridor, live colour, readout deviation and direction cue are measured against the note bar, not the raw contour |

## Impact

- **Consumers of the song clock:** everything keyed to song frames reads `song_pos` and the mic block stamps: plot, overview, live line, pass scoring, take recording, loop. Slowing down changes the rate of song time, not its units, so consumers keep working provided the engine and `live_thread` scale correctly (context.md → Riesgos: alignment when slowed).
- **Consumers of the target:** `Session::target_hz_at` feeds the live colour, readout, corridor, note-to-sing marker and snap-to-voice. Moving to bars changes what all of them compare against. Snap-to-voice and transposition keep their semantics (offset applied to the bars). The phrase shape score keeps using the raw contour (non-goal).
- **Harmony mode:** when the target is a take, its contour is segmented into bars the same way.
- **Real-time safety:** speed reaches the callback as a command; stretched audio arrives as an immutable `Playback` snapshot through the existing `SetPlayback` path. No DSP is added to the callback.
- **Disk and memory:** one stretched render per speed and source, cached in `cache/` (≈ 150 MB for a 4-min stereo backing at 60 %); only the selected speed is loaded.
- **Dependencies / licence:** a time-stretch library is added; Rubber Band would make the binary GPL. Chosen in design.
- **Calibration:** runs at 100 % only.
- **Related use-cases:** none. `knowledge/` does not exist yet.

## Non-goals

- Recording while slowed (practice-only).
- A continuous speed slider; speeds other than 50 / 60 / 75 / 100 %.
- Stretching the user's takes (they are silent while slowed).
- Real-time time-stretching in the audio callback.
- Changing the guide analysis (pYIN / MPM refinement, CREPE).
- Changing the phrase shape score.
- Remembering the chosen speed per project.

## Criterios de aceptación

- At 50, 60 and 75 %, the guide sounds at the same pitch as at 100 % and the song plays proportionally slower.
- The first use of a speed for a project shows render progress without freezing the UI; later uses of the same speed start without re-rendering.
- While slowed, the live line of a voice singing along with the guide lands on the guide in time, as it does at 100 %.
- Changing speed keeps the playhead position; loop phrase repeats the phrase at the slowed speed.
- Below 100 %, pressing R does not record and tells the user to return to 100 %; takes are not heard but their contours stay drawn.
- Back at 100 %, a new take records aligned exactly as before this change (loopback check still within 1 ms).
- On Bizarre Love Triangle '94, the bars follow the melody as heard, and the dimmed raw contour stays visible underneath.
- Holding a steady note inside a bar is drawn in the in-tune colour for the whole note even where the raw guide wobbles beyond ±20 ¢.
- Holding a bar's pitch within ±20 ¢ for ≥ 100 ms marks that bar as reached; a note sung off-pitch is not marked.
- Singing a steady note on a correctly levelled input draws one continuous line, not a string of dots; silence and unvoiced consonants still draw nothing.
- A clipped input like `take-003` triggers the clipping warning; a correctly levelled voice shows no warning.

## Conflictos con decisiones existentes

No `knowledge/decisions.md` or use-case decisions exist yet. The change does modify requirements of the unarchived `add-pitch-follow-recorder` change (context.md → Riesgos identificados):

- **`audio-engine` → "Single song clock"**: "advanced only by the frames processed in the audio callback" becomes "advanced by the processed frames scaled by the practice speed". Modified explicitly.
- **`pitch-feedback` → "Live cents deviation"**: the corridor and deviation move from the transposed raw target to its note bars.
- **`live-pitch` → "Live pitch estimation" / "Live contour display"**: hysteresis must not violate the "Silence and consonants" scenario, which is kept unchanged.
- **Archive order:** these MODIFIED deltas merge into requirements that exist only in `add-pitch-follow-recorder`; that change must be archived before this one.
