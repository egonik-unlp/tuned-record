# UI/UX: add-windows-asio

**Estado:** Aplica

**Justificación si "No aplica":** —

## Componentes y flujos

On Linux nothing visible changes. On Windows, the surfaces from `add-pitch-follow-recorder` and `add-practice-aids` stay as they are: the top-bar device line, the audio devices dialog, the transport, the plot and Calibrate…. Only the text and states below are added. Nodes are referenced by symbol, because Graphify was unavailable (context.md).

| Componente | Cambio | Estados |
|---|---|---|
| Top-bar device line (`ui::panels`, `device_summary`) | On Windows it names the ASIO driver and the channels: `🎤 <driver> · in 2` and `🔊 <driver>`. The buffer size and latency text is unchanged; it shows what the driver panel set | running; **waiting**: the device line is replaced by "Audio is waiting for the interface…" in the warning colour, and transport and record are disabled; **rate mismatch while waiting**: "The interface is at 44100 Hz — set it back to 48000 Hz in its control panel" |
| Audio devices dialog (`ui::panels`, "audio-devices" modal) | Windows lists: Microphone = every input channel of every ASIO driver (`<driver> · in N`), Output = driver names. The lists are read when the dialog opens, not every frame. Picking a mic or an output on **another driver** doesn't apply at once: an inline confirm row appears, "Output will move to <driver> too. [Switch] [Cancel]" (or "Microphone will move to <driver> · in 1 too" when picking an output). On the same driver a choice applies at once, as on Linux | idle; confirm pending (the two combos are disabled until Switch or Cancel); switching ("Switching to <driver>…" for the length of the rebuild); recording (choices disabled, as today) |
| Help text in the devices dialog | On Windows it adds: "The buffer size is set in your interface's ASIO control panel." The calibration sentence stays | — |
| Audio error banner (`ui::panels`, `engine_error`) | Windows start-up errors: "Audio could not start: no ASIO driver found. tuned-record needs an audio interface with its own ASIO driver installed." and "Audio could not start: <driver> could not be opened (<reason>). Connect the interface and start tuned-record again." | shown when the engine failed to start (as today) |
| Recording interrupted by a reset (`ui::app`) | When a driver reset stops a take, a transient hint appears: "Recording stopped: the audio interface was reset. The take was kept up to that point." The take shows up in the takes list like any other | — |
| Windows shell | Start-menu entry "tuned-record", no console window, an uninstaller under Windows' installed apps | — |

**Flow: the owner sets up the other person's PC (once):**
1. Run the setup program; at SmartScreen choose "More info → Run anyway".
2. Start tuned-record from the Start menu.
3. Open 🎤, pick the interface's mic input and output.
4. Plug a cable from output 1 into the mic input and run Calibrate…; unplug the cable.
5. Open the project.

From then on, the other person only opens it and plays.

**Flow: unplug during practice:**
1. Playing at 75 %; the USB cable comes out.
2. The device line shows "Audio is waiting for the interface…" and transport and record grey out; the position is kept.
3. The cable goes back in. Within about a second the device line returns and the transport is enabled. If it was playing, playback continues from where it stopped, at 75 %.

## Wireframes / mockups

Sin mockup visual. ASCII of the changed states:

```
Running (Windows):
│ 🎤 Focusrite USB ASIO · in 2 ▕██████▏  🔊 Focusrite USB ASIO   128 fr · 48 kHz · 7.1 ms │

Waiting (interface unplugged, or driver resetting longer than a moment):
│ ⚠ Audio is waiting for the interface…                              ▶ ■ ● greyed out │

Audio devices dialog, mic picked on another driver:
┌ Audio devices ────────────────────────────────────────────────┐
│ Microphone  [ Behringer UMC ASIO · in 1           ▾]          │
│ Output      [ Focusrite USB ASIO                  ▾] (disabled)│
│ ⚠ Output will move to Behringer UMC ASIO too.  [Switch] [Cancel] │
│                                                               │
│ The app records one mono channel… The buffer size is set in   │
│ your interface's ASIO control panel.                          │
│                                                     [Close]   │
└───────────────────────────────────────────────────────────────┘
```

## Accesibilidad

These reuse egui/AccessKit widgets already in the app. What's new:

| Aspecto | Decisión |
|---|---|
| Roles ARIA | The waiting notice and the "Recording stopped…" hint are plain labels, so screen readers read them; the waiting notice replaces the device line in the same place in the reading order. The confirm row is a label followed by two buttons labelled "Switch to <driver>" and "Cancel". Disabled transport buttons keep their labels and expose the disabled state. |
| Navegación por teclado | When the confirm row appears, focus moves to "Switch"; Tab reaches "Cancel"; Escape cancels the pending switch (a second Escape closes the dialog, as today). Global shortcuts that act on the transport do nothing while waiting, as when no engine is running. |
| Foco visible | Inherits the existing 2 px focus outline. |
| Contraste | Waiting and confirm states are text plus the ⚠ glyph in the existing warning colour (≥ 4.5:1 on the panel background), never colour alone. |
