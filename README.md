# tuned-record

Sing over songs while seeing your pitch live, follow the pitch contour of a reference vocal
(a demucs-separated stem), and stack takes into harmonies and choruses. Every take stays on the
timeline as its own contour; export gives aligned WAVs for a DAW.

Linux only. Audio goes through the JACK API on PipeWire.

## Setup

```sh
sudo apt install pipewire-jack libjack-jackd2-dev   # JACK on PipeWire + jack.pc for the build
cargo build --release
```

`pipewire-jack` lets JACK programs talk to the running PipeWire server; `libjack-jackd2-dev` only
provides the pkg-config file the build needs (libjack is loaded at runtime). The app loads
PipeWire's libjack itself, so it does not need the `pw-jack` wrapper.

## Run

```sh
./target/release/tuned-record              # start screen
./target/release/tuned-record ~/songs/foo  # open a project folder directly
```

Without `pipewire-jack` the app starts but shows an "audio off" error.
The top bar shows the devices in use (🎤 input, 🔊 output); click it to pick the microphone channel
and the output device. The choice is remembered and reconnected whenever the device is present.
The app asks PipeWire for 256-frame buffers (`PIPEWIRE_LATENCY`); the buffer size, rate and
round-trip latency actually in effect are shown in the top bar.

**Use headphones.** If the backing or guide reaches the mic, the live detector follows the
playback instead of you. The app does not play your voice back to you: listen acoustically or
through your interface's direct monitoring.

## Workflow

1. **New project…** in an empty folder (or **Open project…**). A project is a self-contained
   folder: imported files are copied in, resampled to the server rate.
2. **Import backing track…** (any mix) and **Import vocal stem…** (e.g. demucs `vocals.wav`).
   The stem is analysed once (pYIN) and cached in the project.
3. The guide contour scrolls past a fixed playhead; what's coming is drawn dashed ahead of it.
   The shaded corridor around it is where to sing (green: within 20 ¢), and the marker on the
   playhead names the note to sing now. Transpose it with **−/+**, an **Interval** ("major 3rd
   above"), Ctrl+dragging the plot vertically (Shift: free), or **Snap to voice** while singing.
   Transposition only moves the drawing and the cents target, never the audio.
   **Navigate** with the song overview above the plot (click or drag to jump), by clicking the plot,
   dragging it while stopped, or with **⏴ Phrase / Phrase ⏵**; **⟲ Loop phrase** repeats one phrase
   for practice. Zoom time and pitch with the toolbar (or `[` `]` and `-` `=`); **Fit** frames the
   target. While stopped the plot stays still; the readout at the bottom still shows your note.
4. **Record** (R). Earlier takes play back while you record the next one; each keeps its contour
   on the plot in its own colour. The line under the transport says what **Play** plays. When you
   stop, the playhead goes back to where the take began, so Space replays it; **▶** on a take's row
   plays just that take with the backing. While singing, the live line is coloured by deviation from the
   transposed guide (in tune ≤ 20 ¢, near ≤ 50 ¢, off), with the numbers in the readout bar.
5. **Harmonise**: under *What to sing*, sing against the guide or one of your takes, at an
   interval. The original melody stays drawn (dimmed) while the target is transposed.
6. After each phrase, a **shape score** (cents) appears under it: how closely you followed the
   guide's movement, ignoring the transposition (RMS of the deviation minus its median).
7. **Export…** writes `<project>-take-NNN.wav` (every take, mono) and `<project>-mixdown.wav`
   (backing + audible takes, no guide), all 32-bit float, starting at song t=0 with the same
   length — drop them at bar 1 in a DAW.

Settings and takes are saved continuously; takes are written to disk while recording.

## Keyboard

| Key | Action |
|---|---|
| Space | play / stop |
| R | record / stop recording |
| Home | back to start |
| ← / → | seek 5 s |
| Ctrl+← / Ctrl+→ | previous / next phrase |
| L | loop the phrase at the playhead (or the selected take) |
| [ / ] | show more / less time |
| - / = | show more / fewer notes |
| F | fit the pitch range to the target |
| ↑ / ↓ | transpose the target ±1 semitone |
| S | snap guide to the note you're singing |
| G | audible guide on/off |
| Ctrl+E | export |
| Delete | delete the selected take (asks first) |
| Tab / Enter | move between controls / activate |

## Latency calibration

**Calibrate…** plays clicks and measures them coming back. Connect the output to the input first
(a cable from line/headphone out to line/mic in). The measured residual (on top of the latency
JACK reports) is stored in `~/.config/tuned-record/config.toml` and applied to every take.

## Checks

```sh
cargo test
cargo run --release --example loopback_check               # internal patch, silent
cargo run --release --example loopback_check -- --long     # 5-minute take
cargo run --release --example loopback_check -- --physical # real cable loopback
cargo run --release --example demo_project -- /tmp/demo            # synthetic demo project
```
