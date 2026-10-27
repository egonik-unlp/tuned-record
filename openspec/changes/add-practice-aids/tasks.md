# Tasks: add-practice-aids

<!-- Tasks de implementación en orden. Ver design.md para el approach técnico. -->

## Implementation

### 0. Prerequisites
- [ ] Archive `add-pitch-follow-recorder` (or confirm with the user how to proceed) so the MODIFIED deltas of this change have live specs to merge into (design.md → Riesgos)
- [ ] Fresh test take at the current input gain (a held note, then a phrase of Bizarre Love Triangle '94); re-run the voicing analysis to set a baseline for the hysteresis and meter thresholds

### 1. Input level (specs/input-level)
- [ ] Level per live frame (`LiveFrame` / `LivePoint`) and a shared `InputLevel` (decaying peak, RMS, last clipped wall frame) computed in the live-pitch thread (design.md → 4. Input level)
- [ ] Clipping and too-quiet rules with their constants; unit tests on synthetic input (clipped sine → clipping; sine at −45 dBFS while "playing" → too quiet; silence → no warning)
- [ ] Top-bar meter + warning text next to the 🎤 indicator (ui-ux.md → Top bar input meter)
- [ ] Check: replaying `take-003.wav` through a loopback triggers the clipping warning; a correctly levelled voice does not

### 2. Cleaner live line (specs/live-pitch)
- [ ] Voicing hysteresis in `LiveDetector` (enter 0.7, hold 0.5 + pitch within 1 st; level gate unchanged) (design.md → 3. Cleaner live line)
- [ ] Unit tests: tone with clarity dips stays voiced; white noise after a tone turns unvoiced within a few hops; silence stays unvoiced; existing onset-delay test still < 15 ms
- [ ] Gap bridging in the plot's live line (< 50 ms of wall time and < 1 st)
- [ ] Check on the fresh take from task 0: voiced-run count and median length before/after; a held note draws as one line

### 3. Note targets (specs/note-targets, specs/pitch-feedback)
- [ ] `feedback::notes` segmentation + `Note` (design.md → 2. Note targets); unit tests from the spec scenarios (wobbly held note → one note; step with glide → two notes, no glide note)
- [ ] Example in `examples/` that reports note count and duration distribution for a contour file; run on `zwei/cache/reference.f0` and tune the constants until notes are syllable-sized
- [ ] `Session::notes` (recomputed on target change), `target_note_at`; move `target_hz_at` and every consumer (live colour, readout, direction cue, note-to-sing marker, snap-to-voice) to the note pitch; shape score untouched
- [ ] Plot: dimmed raw contour, bars (solid behind / ghost ahead), corridor around bars (ui-ux.md → Pitch plot)
- [ ] `Pass::reached` with the ≥ 100 ms / half-of-short-note rule and 50 ms gap bridge; reset with the pass; unit tests from the spec scenarios (held 150 ms → reached, 60 ms → not, 80 ms note with 45 ms → reached, 60 ¢ flat → not)
- [ ] Reached style on bars (fill + ✓) and "✓ reached" in the readout
- [ ] Harmony target: notes segmented from the chosen take; check the bars follow it

### 4. Practice speed (specs/practice-speed, specs/audio-engine)
- [ ] Evaluate `signalsmith-stretch`: 4-min stereo backing at 50 % under ~30 s, sustained guide note within ±5 ¢ after stretching; else implement the WSOLA fallback (design.md → Stretcher library)
- [ ] Engine: `Command::SetSpeed { ratio, playback }`, remainder-carry song clock, ratio in `BlockHeader` and `Shared`, compensation scaled, `StartRecord` refused below 1/1; unit tests: 48 000 frames at 1/2 advance 24 000, 1/1 path identical to before, no drift over 4 min at 3/5
- [ ] Mixer: `Track` ratio and stretched read in `mix_into`; unit test that a stretched track is read at the right index after a seek
- [ ] Live thread: ratio-aware wall→song mapping; unit test with a synthetic block sequence at 3/5
- [ ] `Job::Stretch` with progress, cache files + key (source hash, stretcher version, ratio), re-render on mismatch, cancel when another speed is chosen
- [ ] Session: stretched tracks of the selected speed only, `playback(ratio)` (stretched backing + guide, no takes below 1/1)
- [ ] App: speed state (always 1/1 on open), `,` / `.`, stay at the previous speed until the render is ready, record/calibrate guard with hint, loop and position kept across speed changes
- [ ] Transport UI: speed buttons with render progress, "Plays: … at 60 % (practice — takes muted)", Record disabled with tooltip (ui-ux.md → Transport bar)
- [ ] Loopback check at 60 %: the live line tracks the returned guide within 5 ms of song time; `loopback_check` at 100 % still within 1 ms

### 5. Docs
- [ ] README: practice speeds and keys, note bars and reached marks, input meter and warnings

## Validation

- [ ] `cargo test` passes; `cargo clippy` without new warnings
- [ ] `cargo run --release --example loopback_check` (and `--physical` if a cable is available) still within 1 ms at 100 %
- [ ] Manual check with Bizarre Love Triangle '94: loop a verse phrase at 60 %, the guide sounds in the same key, bars follow the melody, held notes get ✓, then 75 % and 100 %, then record a take aligned as before
- [ ] Manual check: R and Calibrate… below 100 % show the hint and do nothing; takes silent while slowed and audible again at 100 %
- [ ] A11y: speed buttons and their selected/rendering state announced, meter labelled, warnings readable as text, reached state visible without colour (filled + ✓), `,` / `.` work from anywhere; Tab order transport → speed buttons (check with Orca if the accessibility bus is enabled; otherwise note it as in `add-pitch-follow-recorder`)

## Post-archive

- [ ] `/documenter distill <dated-change-id>` — **auto-ejecutado por `/opsx:archive`**.
      Solo correr a mano si el auto-trigger falló (re-run con el id completo con fecha,
      ej. `2026-10-27-add-practice-aids`). Sincroniza el use-case (knowledge/modules/<m>/use-cases/) con los deltas del change
      (spec narrativa, decisiones, lore). Humano revisa y acepta edits propuestos.
