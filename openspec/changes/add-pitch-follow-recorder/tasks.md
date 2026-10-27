# Tasks: add-pitch-follow-recorder

<!-- Tasks de implementación en orden. Ver design.md para el approach técnico. -->

## Implementation

### 0. Setup
- [x] Install `pipewire-jack` on the dev machine and confirm a JACK client can connect (design.md → Audio engine)
- [x] Create the `tuned-record` crate (lib + bin) with the module layout from design.md → Shape; add dependencies (jack, rtrb, crossbeam-channel, realfft, pyin, symphonia, rubato, hound, eframe, serde/toml, blake3)
- [x] README: build, `pipewire-jack` setup, headphones assumption, loopback calibration

### 1. Audio engine (specs/audio-engine)
- [x] JACK client with 1 in / 2 out, auto-connect, `PIPEWIRE_LATENCY=256/<rate>` request, readback of actual buffer/rate; startup error naming `pipewire-jack`
- [x] RT-safe process callback: command ring in, `Arc` buffer swaps with off-thread drop, song clock atomic, transport (play/stop/seek)
- [x] Mixer: backing + guide (on/level) + takes with volume/mute/solo rules
- [x] Capture path with latency compensation `s(i) = P + i − Lo − Li − C` (design.md → Latency compensation); unit-test the math
- [x] Calibration: click playback, cross-correlation detection, persist `C` to `~/.config/tuned-record/config.toml`
- [x] Audio device selection: list mic sources (no sink monitors) and output devices, switch, remember in config, reconnect lost links while not recording; device indicator + dialog in the transport bar
- [ ] Manual check: device indicator and dialog render; switching input/output is heard; unplugging and replugging a USB interface reconnects it

### 2. Live pitch (specs/live-pitch)
- [x] Live pitch thread: McLeod window 1024 / hop 128, clarity gate, 2048 window for low notes
- [x] Octave filter (median + single-frame jump rejection); unit tests with synthetic sines and injected octave frames
- [x] Note/cents conversion and readout values; unit tests (e.g. 225 Hz → A3 +39 ¢)

### 3. Project and import (specs/project-session, specs/reference-contour)
- [x] Project folder create/open, `project.toml` manifest with atomic debounced writes, sample-rate check on open
- [x] Import: symphonia decode (WAV/FLAC/MP3/OGG) → rubato resample → f32 copy into `sources/`; error on undecodable files
- [x] pYIN analyzer: evaluate the `pyin` crate, otherwise port; synthetic-signal tests; BLAKE3-keyed `cache/reference.f0`
- [x] Background worker with progress reporting to the UI

### 4. Takes (specs/take-layering)
- [x] Record start/stop creating a new take at the start frame; disk writer thread with `hound` and periodic flush
- [x] Live contour stored during recording; post-stop pYIN re-analysis replacing it
- [x] Per-take controls wired to mixer and manifest: mute, solo, volume, show/hide, delete (with file removal)
- [x] Crash recovery: take WAV without manifest entry → recovered take on open

### 5. Feedback (specs/pitch-feedback)
- [x] Guide offset (cents): step, drag (semitone snap, Shift free), snap-to-voice; applies to drawing and cents target only
- [x] Live deviation + bands (20 ¢ / 50 ¢)
- [x] Phrase segmentation (≥ 250 ms gaps, < 150 ms merge); unit tests
- [x] Shape score (RMS of deviation minus median, 50 % coverage rule); unit tests for offset invariance (+3 st vs −5 st equal, exact follow = 0)
- [x] Store scores per take (`take-NNN.scores.json`), recomputed from the pYIN contour

### 6. Export (specs/audio-export)
- [x] Offline renderer: per-take mono f32 WAV padded from t=0 to song length (all takes)
- [x] Mixdown: stereo f32 of backing + audible takes, no guide, peak report
- [x] Export naming and folder selection; test that per-take and mixdown lengths/rates match

### 7. UI (ui-ux.md)
- [x] Start screen (create/open/recent) and error states
- [x] Transport bar with engine status, record state, calibration and export entries
- [x] Pitch plot painter: note grid, fixed playhead, reference past/ghost (dashed), take contours, live line by band, phrase score labels; draw only the visible window
- [x] Readout bar (note, cents, deviation, band label)
- [x] Guide panel (imports, audible toggle/level, transpose, snap) and analysis progress overlay
- [x] Takes panel (rows, controls, delete confirm, recording/re-analysing states)
- [x] Calibration and export dialogs
- [x] Keyboard shortcuts from ui-ux.md
- [x] Fill `openspec/project.md` (Purpose, Tech Stack, Constraints)

### 7. Usability iteration (after first hands-on test)
- [x] Freeze the plot while stopped (no wall-clock scroll); readout keeps the sung note
- [x] Song overview strip: phrases, take lanes, loop, visible window, playhead; click/drag to jump
- [x] Plot navigation: click jumps, drag pans (time while stopped), wheel/Ctrl+wheel, previous/next phrase (Ctrl+←/→), time ticks
- [x] Zoom: time 2–120 s ([ / ]), pitch 6–48 st (− / =), Fit (F); view follows the target, not the singer
- [x] Loop phrase (L): engine `SetLoop`, wraps while playing and not recording
- [x] Target corridor (±20/±50 ¢), target marker + note name at the playhead, live dot, direction cue in the readout
- [x] "Play plays: …" line; per-take listen (backing + that take); playhead back to take start after recording
- [x] Harmony target (guide or a take, saved in `project.toml`), named intervals, untransposed melody kept visible
- [ ] Manual check with a real song: navigate, loop a phrase, record a lead, then a harmony against it

## Validation

- [x] `cargo fmt --check` clean
- [x] `cargo clippy --all-targets -- -D warnings` clean
- [x] `cargo test` green (latency math, pitch, octave filter, phrases, shape score, export alignment, import resampling) — 38 tests
- [ ] Loopback test: exported take vs backing transients within 1 ms (specs/audio-engine)
      _Partial: `examples/loopback_check` (internal JACK self-patch) → 0 frames off, PASS; but that patch has zero delay, so non-zero compensation is unverified. Pending: run with `--physical` and a cable._
- [ ] Voice-to-line latency < 40 ms measured at 256 frames (test tone + screen capture)
      _Partial: budget measured ≈ 38 ms worst / ≈ 30 ms avg (design.md → Live pitch; detector 14.7 ms by unit test, Li 6.0 ms reported). Pending: the screen-capture measurement._
- [x] 5-minute take: length exact, no overruns reported — `loopback_check --long`: 300.6 s, 0 xruns, 0 blocks lost, last 10 s still 0 frames off
- [ ] Reopen test: take from an earlier session still aligned; settings restored
      _Partial: reopening the generated demo project restores sources, cached reference/take contours, scores and guide offset (screenshots). Pending: a take recorded in the app, reopened._
- [ ] Export imported into a DAW at bar 1 lines up with the mixdown
- [ ] A11y: every control reachable and operable by keyboard; focus outline visible; icon buttons have accessible labels (check with Orca); deviation and take identity readable without color
      _Pending: the desktop accessibility bus is disabled on the dev machine (org.a11y.Status IsEnabled=false), so the AccessKit tree could not be inspected. Labels are set in code for all icon buttons/sliders; deviation shows numbers + band label; takes are named._
- [ ] Manual end-to-end: import song + stem → lead take → harmony take at +4 st → export

## Post-archive

- [ ] `/documenter distill <dated-change-id>` — **auto-ejecutado por `/opsx:archive`**.
      Solo correr a mano si el auto-trigger falló (re-run con el id completo con fecha,
      ej. `2026-05-26-add-foo`). Sincroniza el use-case (knowledge/modules/<m>/use-cases/) con los deltas del change
      (spec narrativa, decisiones, lore). Humano revisa y acepta edits propuestos.
