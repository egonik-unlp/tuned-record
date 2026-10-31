# tuned-record

Sing over songs while seeing your pitch live, follow the pitch contour of a reference vocal
(a demucs-separated stem), and stack takes into harmonies and choruses. Every take stays on the
timeline as its own contour; export gives aligned WAVs for a DAW.

Runs on Linux, where audio goes through the JACK API on PipeWire, and on Windows with an audio
interface that has its own ASIO driver (see [Windows](#windows)).

## Setup

```sh
sudo apt install pipewire-jack libjack-jackd2-dev   # JACK on PipeWire + jack.pc for the build
sudo apt install g++ libclang-dev                   # builds the time stretcher (C++)
cargo build --release
```

`pipewire-jack` lets JACK programs talk to the running PipeWire server; `libjack-jackd2-dev` only
provides the pkg-config file the build needs (libjack is loaded at runtime). The app loads
PipeWire's libjack itself, so it does not need the `pw-jack` wrapper. Slowed practice uses
Signalsmith Stretch (MIT, C++), compiled during the build.

## Run

```sh
./target/release/tuned-record              # start screen
./target/release/tuned-record ~/songs/foo  # open a project folder directly
```

Without `pipewire-jack` the app starts but shows an "audio off" error.
The top bar shows the devices in use (🎤 input, 🔊 output); click it to pick the microphone channel
and the output device. The choice is remembered and reconnected whenever the device is present.
Next to it, an **input meter** shows the mic level (bar: average, tick: peak) whether or not the
song is playing. **"Input clipping — lower the gain"** appears when the input reaches full scale
and stays for 2 s. **"Input too quiet — raise the gain"** appears when you sing during playback
but so low that pitch tracking gets unreliable. It never appears for silence between phrases.
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
3. The guide scrolls past a fixed playhead as **note bars**: one bar per sung note, at its steady
   pitch (vibrato, wobble and the slides between notes are left out; notes are not snapped to
   A440, so a record tuned slightly off still lines up). Bars ahead have a dashed outline. The raw
   guide contour stays underneath, dimmed. The shaded corridor around each bar is where to sing
   (green: within 20 ¢), and the marker on the playhead names the note to sing now. A note
   counts as **reached** once you hold it within 20 ¢ for 100 ms (half the note, for notes
   shorter than that). Brief dropouts don't break the hold. A reached bar fills in and gets a
   ✓, and the readout says "✓ reached". Marks start over each time you play or jump. Transpose it with **−/+**, an **Interval** ("major 3rd
   above"), Ctrl+dragging the plot vertically (Shift: free), or **Snap to voice** while singing.
   Transposition only moves the drawing and the cents target, never the audio.
   **Navigate** with the song overview above the plot (click or drag to jump), by clicking the plot,
   dragging it while stopped, or with **⏴ Phrase / Phrase ⏵**; **⟲ Loop phrase** repeats one phrase
   for practice. Zoom time and pitch with the toolbar (or `[` `]` and `-` `=`); **Fit** frames the
   target. While stopped the plot stays still; the readout at the bottom still shows your note.
   **Slow it down** to learn a fast phrase: **Speed** 50 / 60 / 75 / 100 % in the transport (or
   `,` slower, `.` faster). The backing and guide play slower in the same key. The first time
   you choose a speed, it is prepared in the background (progress on its button, a few seconds
   per song) while playback stays at the previous speed. After that it is cached in the
   project. Loop and position are kept. Slowed playback is for practice only: takes are muted,
   and Record and Calibrate… ask you to go back to 100 % first. Every project opens at 100 %.
4. **Record** (R). Earlier takes play back while you record the next one; each keeps its contour
   on the plot in its own colour. The line under the transport says what **Play** plays. When you
   stop, the playhead goes back to where the take began, so Space replays it; **▶** on a take's row
   plays just that take with the backing. While singing, the live line is coloured by deviation from the
   current note bar (in tune ≤ 20 ¢, near ≤ 50 ¢, off), with the numbers in the readout bar. A held
   note is drawn as one line: dropouts shorter than 50 ms are bridged.
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
| , / . | practice speed slower / faster (50 · 60 · 75 · 100 %) |
| ↑ / ↓ | transpose the target ±1 semitone |
| S | snap guide to the note you're singing |
| G | audible guide on/off |
| Ctrl+E | export |
| Delete | delete the selected take (asks first) |
| Tab / Enter | move between controls / activate |

## Windows

The Windows app is built on this Linux machine and packaged as a setup program; nothing is built
on Windows. It needs an audio interface with its own (native) ASIO driver: the mic and the output
are channels of that one driver.

### Build (on Linux)

```sh
sudo apt install mingw-w64 nsis clang libclang-dev unzip curl
rustup target add x86_64-pc-windows-gnu
scripts/build-windows.sh           # → target/windows/tuned-record-setup-<version>.exe
scripts/build-windows.sh --smoke   # also starts the .exe under Wine for 20 s
```

The first run downloads Steinberg's ASIO SDK into `~/.cache/tuned-record/asiosdk`. The .exe is
linked statically (only Windows system DLLs). `vendor/asio-sys` is a patched copy of a cpal
dependency that makes the ASIO bindings build with MinGW (see `vendor/asio-sys/TUNED-RECORD.md`).

**Licence note:** the ASIO SDK compiled into the .exe is dual-licensed by Steinberg: under its own
licence (publishing software needs an agreement signed by Steinberg) or under GPL v3. Giving the
setup program to someone else is distribution under one of those two.

### Install (on Windows)

1. Run `tuned-record-setup-<version>.exe`. It is not signed, so Windows shows "Windows protected
   your PC": choose **More info → Run anyway**. It installs for the current user only (no admin
   rights) and adds tuned-record to the Start menu.
2. Start tuned-record, click the device button (🎤 / 🔊) and pick the interface's mic input. Mic
   and output always use the same ASIO driver; picking an input on another interface moves the
   output there too (the dialog asks first).
3. Calibrate once (below) with a cable from an output of the interface into the mic input.

The buffer size is set in the interface's ASIO control panel, not in the app. Changing it, or
unplugging the interface, doesn't need a restart: the app waits ("Audio is waiting for the
interface…") and continues where it was; a take being recorded stops and is kept up to that
point. Settings and the calibration live in `%APPDATA%\tuned-record\config.toml` and survive
reinstalling; uninstalling (Windows' installed apps list) leaves them and all recordings alone. A
crash leaves its message in `%APPDATA%\tuned-record\crash.log`.

To check alignment on Windows, copy `target/windows/loopback_check.exe` (built by the script) to
the Windows PC and run `loopback_check.exe --physical` in a terminal with the cable in place; it
uses the driver and input chosen in the app and also prints the driver's callback order.

## Latency calibration

**Calibrate…** plays clicks and measures them coming back. Connect the output to the input first
(a cable from line/headphone out to line/mic in). The measured residual (on top of the latency
JACK or the ASIO driver reports) is stored in `~/.config/tuned-record/config.toml`
(`%APPDATA%\tuned-record\config.toml` on Windows) and applied to every take.

## Checks

```sh
cargo test
cargo run --release --example loopback_check               # internal patch, silent
cargo run --release --example loopback_check -- --long     # 5-minute take
cargo run --release --example loopback_check -- --physical # real cable loopback
cargo run --release --example demo_project -- /tmp/demo            # synthetic demo project
cargo run --release --example voicing_report -- take.wav  # a take through the live detector + meter
cargo run --release --example note_report -- cache/reference.f0     # how the guide splits into notes
```

`loopback_check` also plays a stepped tone at 60 % and checks that the live pitch lands within
5 ms of song time.
