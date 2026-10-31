# Delta Spec: add-windows-asio

**Capability:** audio-engine
**Change:** add-windows-asio

## RENAMED Requirements

- FROM: `### Requirement: Duplex audio over JACK on PipeWire`
- TO: `### Requirement: Duplex audio`

## MODIFIED Requirements

### Requirement: Duplex audio
The system SHALL process capture and playback in one synchronized cycle per audio period, with one mono mic input and a stereo output, at the audio device's sample rate. On Linux it SHALL run as a single JACK client (served by PipeWire through `pipewire-jack`) with one input port and two output ports. On Windows it SHALL run on one native ASIO driver, using one of its input channels as the mic and two of its output channels as the output.

#### Scenario: Engine starts on a PipeWire system with pipewire-jack
- **GIVEN** Linux with PipeWire running and `pipewire-jack` installed
- **WHEN** the application starts
- **THEN** a JACK client with one input and two output ports is visible in the PipeWire graph
- **AND** its ports are connected to the default capture and playback devices

#### Scenario: JACK server unavailable
- **GIVEN** Linux, and no JACK server can be reached (e.g. `pipewire-jack` is not installed)
- **WHEN** the application starts
- **THEN** it shows an error stating that audio could not start and that `pipewire-jack` is required
- **AND** it does not crash

#### Scenario: Engine starts on an ASIO interface
- **GIVEN** Windows with an audio interface whose native ASIO driver is installed
- **WHEN** the application starts
- **THEN** the engine runs on that driver, with the mic on one of its inputs and the output on its first two outputs
- **AND** the transport bar names the driver and the channels in use

#### Scenario: No ASIO driver
- **GIVEN** Windows with no ASIO driver installed, or none that can be opened
- **WHEN** the application starts
- **THEN** it shows an error stating that audio could not start and that an audio interface with an ASIO driver is required
- **AND** it does not crash

### Requirement: Audio device selection
The system SHALL show which device the mic input records from and which device the output plays into, SHALL let the user choose the mic input (one mono channel of any capture device, excluding sink monitors on Linux) and the output device, SHALL remember the choice in the application configuration and use it whenever that device is present, and SHALL reconnect an unconnected input or output to the chosen (or first available) device while not recording. On Windows, input and output SHALL always use the same ASIO driver: choosing a mic channel on another driver SHALL move the output to that driver, and the dialog SHALL say so before the choice is applied.

#### Scenario: Device shown
- **GIVEN** the engine is running
- **WHEN** the transport bar is shown
- **THEN** it names the input and output devices in use
- **AND** a side with no connection is shown as "none" in the error colour

#### Scenario: Choose a device
- **GIVEN** an audio interface with inputs 1 and 2 is connected
- **WHEN** the user picks its input 2 as microphone in the audio devices dialog
- **THEN** the mic input records from that channel alone
- **AND** the next start of the app records from it again

#### Scenario: Device comes back
- **GIVEN** the chosen output device disappears and the output is left unconnected
- **WHEN** the device reappears and the app is not recording
- **THEN** the output is connected to it again without user action

#### Scenario: No change while recording
- **GIVEN** a take is being recorded
- **WHEN** the user opens the audio devices dialog
- **THEN** the device choices are disabled

#### Scenario: Mic on another ASIO driver
- **GIVEN** Windows with two ASIO drivers installed and the engine running on driver A
- **WHEN** the user picks an input of driver B as microphone
- **THEN** the dialog states that the output will move to driver B as well
- **AND** after confirming, both the mic and the output run on driver B

### Requirement: Low-latency buffer request
On Linux the system SHALL request a buffer size of 256 frames for its node. On Windows the buffer size SHALL be the one set in the ASIO driver's control panel. On both, the system SHALL display the buffer size and sample rate actually in effect.

#### Scenario: Requested quantum honored
- **GIVEN** Linux, and the system default quantum is 1024
- **WHEN** the engine starts
- **THEN** the engine runs with 256-frame buffers
- **AND** the status display shows 256 frames at the server sample rate

#### Scenario: Requested quantum not honored
- **GIVEN** Linux, and the server refuses the 256-frame request
- **WHEN** the engine starts
- **THEN** the engine still runs with the server's buffer size
- **AND** the status display shows the actual buffer size and the resulting latency in ms

#### Scenario: ASIO driver buffer size
- **GIVEN** Windows, and the ASIO driver's control panel is set to 128 samples at 48 kHz
- **WHEN** the engine starts
- **THEN** the engine runs with 128-frame buffers
- **AND** the status display shows 128 frames at 48 kHz and the resulting latency in ms

### Requirement: Latency-compensated capture
The system MUST write each captured sample to the song position at which the user heard the corresponding playback, compensating the round-trip latency reported by the audio platform (the JACK ports on Linux, the ASIO driver on Windows) plus a user calibration offset stored in the application configuration. Below 100 % speed, the live pitch MUST be placed the same way, with the compensation converted to song time at the speed in effect.

#### Scenario: Loopback alignment
- **GIVEN** the output is fed back into the input (cable or PipeWire loopback) and a backing track with sharp transients is loaded
- **WHEN** a take is recorded over the backing and exported
- **THEN** the transients in the exported take are within 1 ms of the same transients in the backing

#### Scenario: Calibration measurement
- **GIVEN** a loopback is connected
- **WHEN** the user runs the latency calibration
- **THEN** the system plays a test click, measures the residual offset between the click and its capture, stores it as the calibration offset, and shows it in ms

#### Scenario: Live pitch aligned while slowed
- **GIVEN** a loopback is connected and the audible guide plays at 60 %
- **WHEN** the live detector tracks the guide coming back through the input
- **THEN** the live line is drawn over the guide's contour within 5 ms of song time

#### Scenario: Loopback alignment on an ASIO interface
- **GIVEN** Windows, an ASIO interface with one of its outputs cabled to the mic input, and a calibration measured on that interface
- **WHEN** a take is recorded over a backing with sharp transients and exported
- **THEN** the transients in the exported take are within 1 ms of the same transients in the backing

### Requirement: Real-time safe processing
The application's code in the audio callback MUST NOT allocate memory, take locks, or perform file or network I/O; captured audio and pitch input SHALL be handed to other threads through lock-free buffers. On Windows, the uncontended locks that the ASIO host glue (cpal and asio-sys) takes each period to dispatch the callbacks are outside the application's code and are accepted.

#### Scenario: Long take without loss
- **GIVEN** the engine runs at 256-frame buffers
- **WHEN** the user records a 5-minute take while the UI is redrawing
- **THEN** the saved take is exactly as long as the recorded duration, in frames
- **AND** no buffer overruns are reported for the session

## ADDED Requirements

### Requirement: Driver reset recovery
On Windows, when the ASIO driver requests a reset (buffer size or sample rate changed in its control panel) or the interface disappears, the system SHALL rebuild the audio streams without restarting the application, and SHALL restore the song position, play state, loop range and practice speed once audio runs again. A take being recorded when the reset happens MUST be stopped and kept up to the last frame captured before the reset, sample-aligned like any other take; it SHALL NOT resume on its own. While no driver can be opened, the application SHALL stay usable for non-audio actions, SHALL show that audio is waiting for the interface, and SHALL retry on its own. On Linux, losing the JACK server keeps its current behaviour.

#### Scenario: Buffer size changed while playing
- **GIVEN** Windows, playback running at 75 % with a loop range set
- **WHEN** the user changes the buffer size in the ASIO driver's control panel
- **THEN** audio resumes without restarting the app
- **AND** playback continues from where it was, at 75 %, inside the same loop range
- **AND** the status display shows the new buffer size

#### Scenario: Interface unplugged and plugged back
- **GIVEN** Windows, the engine running on an USB ASIO interface with the transport stopped at 01:12.000
- **WHEN** the interface is unplugged and, some seconds later, plugged back in
- **THEN** while it is unplugged the app stays open and shows that audio is waiting for the interface
- **AND** after it is plugged back, audio runs again without user action, with the transport stopped at 01:12.000

#### Scenario: Reset during a take
- **GIVEN** Windows, a take being recorded
- **WHEN** the ASIO driver requests a reset
- **THEN** recording stops and the take is saved up to the last frame captured before the reset
- **AND** the saved take is aligned to the backing like any other take
- **AND** the app does not start recording again on its own
