# UI/UX: add-practice-aids

**Estado:** Aplica

**Justificación si "No aplica":** —

## Componentes y flujos

Builds on the surfaces of `add-pitch-follow-recorder` (transport bar, pitch plot, readout, top-bar device indicator). Nodes referenced by symbol (Graphify unavailable; see context.md).

| Componente | Cambio | Estados |
|---|---|---|
| Transport bar (`ui::panels::transport_bar`) | New speed selector: four toggle buttons `50 %` `60 %` `75 %` `100 %`, the active one highlighted. While a speed is rendering, the chosen button shows a progress percentage and the previous speed stays active. The "Play plays: …" line says "backing + guide at 60 % (practice — takes muted)" when slowed. Record button disabled below 100 % with a tooltip "Recording needs 100 % speed" | 100 % (normal), slowed (practice), rendering (progress on the pending button), render failed (hint with the error, stays at previous speed) |
| Record / Calibrate actions (`ui::app`) | Below 100 %, R, the Record button and Calibrate… do nothing except show the transient hint "Recording needs 100 % speed — press . to go back" | — |
| Pitch plot (`ui::plot::show`) | Guide drawn as **note bars** (rounded rectangles about one third of a semitone tall, solid behind the playhead, ghost/dashed outline ahead). The raw contour stays underneath, thinner and dimmed. The ±20 / ±50 ¢ corridor is drawn around the bars instead of the contour. Reached bars become filled with a ✓ at their right end. The note-to-sing marker at the playhead sits on the current bar. Live line: held notes drawn as one line (short dropouts bridged) | no target (no bars), bars ready, note reached, playing slowed (scrolls at the reduced rate) |
| Readout (`ui::panels::readout`) | Deviation and direction cue measured against the current bar ("vs guide: +12 ¢ in tune"); unchanged layout. Shows "✓ reached" next to the deviation while the current note is marked | unvoiced ("—"), no note here (deviation hidden), reached |
| Top bar input meter (`ui::panels`, next to 🎤) | New horizontal level meter (≈ 80 px) with a peak tick, and a text warning to its right: "Input clipping — lower the gain" or "Input too quiet — raise the gain" | idle (no signal), ok, clipping (held ≥ 2 s), too quiet (only during playback while singing), audio off (meter hidden) |
| Song overview (`ui::timeline`) | No change in this change (reached marks are not mirrored there) | — |

**Main practice flow:** open project → loop the difficult phrase (L) → press `,` until 60 % → first time: progress on the 60 % button, then playback slows in the same key → sing; bars ahead show the next notes, each bar fills with ✓ once held → step up with `.` to 75 %, then 100 % → record (R).

**Keyboard shortcuts added:** `,` one speed slower · `.` one speed faster (50 → 60 → 75 → 100 %). Both work while playing; README keyboard table updated.

## Wireframes / mockups

Sin mockup visual; ASCII layout of the changed areas:

```
┌──────────────────────────────────────────────────────────────────────────────────────────────┐
│ 🎤 Built-in · ch1 ▕██████████████▏▏ Input clipping — lower the gain    🔊 Built-in   256 fr …  │
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│ ▶ ■ ● ⏮  01:12.400   Speed [50 %][60 %][▣75 %][100 %]   Plays: backing + guide at 75 %        │
│                                                          (practice — takes muted)            │
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│ F4 ┤                         │                                                               │
│ E4 ┤ ▓▓▓▓▓▓▓▓✓   ~∿~         │        ┌┄┄┄┄┄┄┄┐            ← upcoming bar (ghost outline)    │
│    │ ━━━━━━━━━━━━━━━━━━━━━━━━●       ┆       ┆                                              │
│ D4 ┤            ▓▓▓▓▓▓✓      │  ┌┄┄┄┄┘       └┄┄┄┐ ← dimmed raw contour runs beneath bars   │
│ C4 ┤                    ▓▓▓▓▓│▓▓ ← current bar, corridor shaded around it                    │
│    │  reached ✓ = filled bar + check; not reached = outline only                             │
│    └─────────────────────────┼────────────────────────────────────────────────▶              │
│                         playhead                                                             │
├──────────────────────────────────────────────────────────────────────────────────────────────┤
│  E4  −8 ¢      vs guide: +6 ¢  in tune   ✓ reached                                           │
└──────────────────────────────────────────────────────────────────────────────────────────────┘

Rendering a new speed:   Speed [50 %][60 % · 42 %][▣100 %]     ← 100 % stays active until ready
Slowed, R pressed:       hint: "Recording needs 100 % speed — press . to go back"
```

## Accesibilidad

Reuses egui/AccessKit widgets from the existing UI; what is inherited and what is new:

| Aspecto | Decisión |
|---|---|
| Roles ARIA | Speed selector: four toggle buttons (AccessKit button with selected state) labelled "Speed 60 percent", grouped under "Practice speed"; the rendering one announces "rendering, 42 percent". Input meter: a progress-bar/meter role labelled "Input level", value in dBFS; warnings are plain labels so screen readers read them. The plot's text summary gains the current bar's note, "reached" state and the speed when slowed. |
| Navegación por teclado | `,` / `.` change speed from anywhere (same global-shortcut handling as the existing keys); speed buttons are in the transport Tab order after the position; Enter/Space activate. Disabled Record keeps focus order and explains itself via tooltip and the hint. |
| Foco visible | Inherits the thickened 2 px focus outline of the existing theme for the speed buttons. |
| Contraste | Reached state is not colour-only: filled bar + ✓ glyph vs outline only, and "✓ reached" in the readout. Ghost bars differ from passed bars by dashed outline, not only opacity. Warnings are text, not just a red meter. The dimmed raw contour may fall below 4.5:1 on purpose (secondary information); bars, labels and warnings stay ≥ 4.5:1. |
