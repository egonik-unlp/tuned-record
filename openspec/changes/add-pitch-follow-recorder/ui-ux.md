# UI/UX: add-pitch-follow-recorder

**Estado:** Aplica

**Justificación si "No aplica":** —

## Componentes y flujos

Single-window desktop app (eframe/egui). Greenfield: no existing graph nodes to reference.

| Componente | Cambio | Estados |
|---|---|---|
| Start screen | New: create project in empty folder / open project folder / recent projects | empty, error (not a project folder, wrong sample rate) |
| Transport bar | New: play/stop, record, position / length (mm:ss.mmm), return to start; a line saying what Play plays (e.g. "backing + take 1, take 2") and whether a loop is on; transient hints (e.g. after a take); engine status (buffer frames, rate, latency ms); input/output device indicator that opens the audio devices dialog; calibration entry | audio stopped, playing, listening to one take, looping, recording (red, elapsed time), audio error (banner with `pipewire-jack` hint) |
| Song overview | New: the whole song as a strip above the plot: time ticks, reference phrases, one lane per take (colour), loop range, the window the plot shows, playhead; click/drag to jump | no sources (10 s scale), ready, looping |
| Plot toolbar | New: previous/next phrase, loop phrase toggle, time zoom −/+ (seconds shown), pitch zoom −/+ (semitones shown), Fit, a one-line mouse hint | loop on/off |
| Pitch plot | New: time ticks, note grid (semitone lines, labelled C notes, current-note row highlighted, target note label highlighted), fixed playhead at ~35 % width with a marker and name of the note to sing, shaded corridor (±20 ¢ / ±50 ¢) around the transposed target, reference contour (solid behind playhead, ghost ahead; dimmed untransposed melody while transposed), take contours (one color each), live line colored by deviation band ending in a dot, loop range shading, phrase score labels under phrases. Mouse: click jumps, drag moves (time while stopped, pitch always), wheel scrolls pitch, Ctrl+wheel zooms time, Ctrl+drag transposes | no sources (empty grid; frozen while stopped, the readout shows the note), analysing (progress overlay on reference lane), ready, playing, recording |
| Readout | New: current note + octave, cents from nearest note, deviation vs target ("vs guide" / "vs take N", ¢) with band label and direction cue ("sing higher" / "sing lower" / "hold it") | unvoiced (shows "—"), no target here (deviation hidden) |
| Guide panel | New: import backing, import vocal stem, guide audible toggle + level; "What to sing": target (guide or a take), named interval (unison … octave, above/below), transpose −/+ st, offset readout (e.g. "+4 st"), snap-to-voice | no stem, analysing, ready; snap disabled while either pitch is unvoiced |
| Takes panel | New: list of takes (color swatch, name, start time, length, "target" tag), listen (alone with the backing), mute, solo, volume, show/hide contour, delete (with confirm) | empty ("record a take to start layering"), recording (new row growing), re-analysing (spinner on row) |
| Calibration dialog | New: instructions to connect loopback, run, result in ms, save | idle, measuring, success, failed (no click found) |
| Audio devices dialog | New: microphone (device · channel) and output device pickers, remembered per machine; note to recalibrate after switching interfaces | connected, not connected, disabled while recording |
| Export dialog | New: choose folder, file list preview, run, peak level report | idle, rendering (progress), done (open folder), error |

**Main flow:** Start screen → create project → import backing + vocal stem → analysis progress → plot shows reference → (optional) transpose / snap → press record → sing with future ghost visible and live coloring → stop → take appears in panel and on plot (re-analysed in background) → repeat for harmonies → export.

**Keyboard shortcuts:** Space play/stop · R record/stop · Home to start · ←/→ seek 5 s · Ctrl+←/→ previous/next phrase · L loop phrase · [ / ] time zoom out/in · − / = pitch zoom out/in · F fit pitch · ↑/↓ transpose ±1 st · S snap-to-voice · G toggle audible guide · Ctrl+E export · Delete on selected take (asks to confirm).

**Iteration after first hands-on test (2026-10-22):** the user found the timeline impossible to navigate, matching pitch hard, "Play" unclear (no way to hear a take), and asked for pitch zoom and for the view to support harmonising. Answered with the song overview, plot toolbar and mouse model, the target corridor/marker/direction cue, the "Play plays: …" line, per-take listen and return-to-take-start after recording, and the harmony target + named intervals. The plot is frozen while stopped (no wall-clock scrolling); the readout keeps showing the sung note.

## Wireframes / mockups

Sin mockup visual; ASCII layout:

```
┌───────────────────────────────────────────────────────────────────────────────────────────┐
│ my-song   ▶ ■ ● REC 00:41.230   ⏮   │ 256 fr · 48 kHz · ~11 ms   [Calibrate]  [Export]    │
├───────────────────────────────────────────────────────────────┬───────────────────────────┤
│ A4 ┤                         │       ┄┄╭───╮┄┄                │ GUIDE                      │
│ G4 ┤        ╭───╮            │   ┄┄┄┄╯     ╲┄┄               │ backing.wav  vocals.wav   │
│ F4 ┤ ──╮   ╱     ╲___        │                                │ [x] audible  ──●──── -6dB  │
│ E4 ┤   ╰──╯          ╲   ●━━━●  ← live (green/amber/red)      │ transpose [-] +4 st [+]    │
│ D4 ┤  take 2           ╲_╱   │                                │ [Snap to voice]            │
│ C4 ┤ ══╗   ╔═╗               │                                ├───────────────────────────┤
│ B3 ┤   ╚═══╝ ╚══  take 1     │                                │ TAKES                      │
│    │   [12 ¢]    [31 ¢]      │  ← phrase shape scores         │ ■ take 1  M S ─●── 👁 🗑   │
│    └─────────────────────────┼──────────────────────────────▶ │ ■ take 2  M S ─●── 👁 🗑   │
│          past                playhead        future (ghost)   │ ■ take 3 ● recording…      │
├───────────────────────────────────────────────────────────────┴───────────────────────────┤
│  E4  −8 ¢      vs guide: +12 ¢  in tune                                                     │
└───────────────────────────────────────────────────────────────────────────────────────────┘
```

At narrow widths the side panel collapses below the plot; the plot keeps priority.

## Accesibilidad

egui exposes widgets through AccessKit (enabled in eframe), so standard buttons, sliders and checkboxes are announced by screen readers. The pitch plot is a custom-painted canvas and is inherently visual. Its essential information is mirrored in text (readout, offset readout, score labels, takes list).

| Aspecto | Decisión |
|---|---|
| Roles ARIA | Native desktop: AccessKit roles from egui widgets (button, slider, checkbox, list). Every icon-only button (mute, solo, show/hide, delete, transport) has an accessible label ("Mute take 2"). The plot gets a label plus a text summary (current note, deviation, offset). |
| Navegación por teclado | Full operation without a mouse: shortcuts above, plus Tab order transport → guide panel → takes list; arrow keys move within the takes list; Enter toggles the focused control. Delete always asks for confirmation. |
| Foco visible | egui's focus outline kept and thickened (2 px, high-contrast accent) on all interactive widgets, including takes rows. |
| Contraste | Dark theme by default; grid, labels and text ≥ 4.5:1 against the background. Deviation is never color-only: the readout shows the cents number and a band label ("in tune / near / off"). Take colors are chosen from a color-blind-safe categorical palette and each take is also identified by its name in the panel and a label at its contour's start. The ghost (future) reference differs from the past reference by dash pattern, not only by opacity. |
