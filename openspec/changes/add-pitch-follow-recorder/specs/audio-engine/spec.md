# Delta Spec: add-pitch-follow-recorder

**Capability:** audio-engine
**Change:** add-pitch-follow-recorder

## Purpose

Real-time audio I/O for the recorder: one synchronized playback/capture cycle over JACK on PipeWire, a single song clock, the playback mix, and latency-compensated capture so stacked takes line up.

## ADDED Requirements

### Requirement: Duplex audio over JACK on PipeWire
The system SHALL run as a single JACK client (served by PipeWire through `pipewire-jack`) with one mic input port and two output ports, processing capture and playback in the same process callback, at the server's sample rate.

#### Scenario: Engine starts on a PipeWire system with pipewire-jack
- **GIVEN** PipeWire is running and `pipewire-jack` is installed
- **WHEN** the application starts
- **THEN** a JACK client with one input and two output ports is visible in the PipeWire graph
- **AND** its ports are connected to the default capture and playback devices

#### Scenario: JACK server unavailable
- **GIVEN** no JACK server can be reached (e.g. `pipewire-jack` is not installed)
- **WHEN** the application starts
- **THEN** it shows an error stating that audio could not start and that `pipewire-jack` is required
- **AND** it does not crash

### Requirement: Audio device selection
The system SHALL show which device the mic input records from and which device the output plays into, SHALL let the user choose the mic input port (one mono channel of any capture device, excluding sink monitors) and the output device, SHALL remember the choice in the application configuration and use it whenever that device is present, and SHALL reconnect an unconnected input or output to the chosen (or first available) device while not recording.

#### Scenario: Device shown
- **GIVEN** the engine is running
- **WHEN** the transport bar is shown
- **THEN** it names the input and output devices in use
- **AND** a side with no connection is shown as "none" in the error colour

#### Scenario: Choose a device
- **GIVEN** an audio interface with inputs 1 and 2 is connected
- **WHEN** the user picks its input 2 as microphone in the audio devices dialog
- **THEN** the mic input records from that port alone
- **AND** the next start of the app records from it again

#### Scenario: Device comes back
- **GIVEN** the chosen output device disappears and the output is left unconnected
- **WHEN** the device reappears and the app is not recording
- **THEN** the output is connected to it again without user action

#### Scenario: No change while recording
- **GIVEN** a take is being recorded
- **WHEN** the user opens the audio devices dialog
- **THEN** the device choices are disabled

### Requirement: Low-latency buffer request
The system SHALL request a buffer size of 256 frames for its node and SHALL display the buffer size and sample rate actually in effect.

#### Scenario: Requested quantum honored
- **GIVEN** the system default quantum is 1024
- **WHEN** the engine starts
- **THEN** the engine runs with 256-frame buffers
- **AND** the status display shows 256 frames at the server sample rate

#### Scenario: Requested quantum not honored
- **GIVEN** the server refuses the 256-frame request
- **WHEN** the engine starts
- **THEN** the engine still runs with the server's buffer size
- **AND** the status display shows the actual buffer size and the resulting latency in ms

### Requirement: Single song clock
The system SHALL keep one song position, measured in frames since song t=0 and advanced only by the frames processed in the audio callback (or moved by a seek, or by wrapping to the start of a loop range while playing and not recording), and SHALL key all playback, capture and pitch data to it.

#### Scenario: Play from a position
- **GIVEN** a loaded project and a stopped transport
- **WHEN** the user seeks to 00:42.000 and starts playback
- **THEN** the backing is heard from 00:42.000
- **AND** the playhead, reference contour and take contours are drawn at 00:42.000

#### Scenario: Loop a phrase
- **GIVEN** a loop range of 00:09.000–00:14.500 and playback running inside it
- **WHEN** the song position reaches 00:14.500
- **THEN** playback continues from 00:09.000 (not while recording)

#### Scenario: Stop holds position
- **GIVEN** playback is running
- **WHEN** the user stops the transport
- **THEN** playback and the song position stop at the current frame

### Requirement: Playback mix
The system SHALL output the sum of the backing track, the guide stem when enabled, and the takes that are audible under the mute/solo rules, each at its own volume.

#### Scenario: Takes audible during playback
- **GIVEN** a project with a backing track and two unmuted takes
- **WHEN** playback runs over a region where both takes have audio
- **THEN** the backing and both takes are heard together

#### Scenario: Solo restricts takes
- **GIVEN** three takes with take 2 soloed
- **WHEN** playback runs
- **THEN** only take 2 is heard among the takes
- **AND** the backing is still heard

### Requirement: Latency-compensated capture
The system MUST write each captured sample to the song position at which the user heard the corresponding playback, compensating the round-trip latency reported by the JACK ports plus a user calibration offset stored in the application configuration.

#### Scenario: Loopback alignment
- **GIVEN** the output is fed back into the input (cable or PipeWire loopback) and a backing track with sharp transients is loaded
- **WHEN** a take is recorded over the backing and exported
- **THEN** the transients in the exported take are within 1 ms of the same transients in the backing

#### Scenario: Calibration measurement
- **GIVEN** a loopback is connected
- **WHEN** the user runs the latency calibration
- **THEN** the system plays a test click, measures the residual offset between the click and its capture, stores it as the calibration offset, and shows it in ms

### Requirement: Real-time safe processing
The audio callback MUST NOT allocate memory, take locks, or perform file or network I/O; captured audio and pitch input SHALL be handed to other threads through lock-free buffers.

#### Scenario: Long take without loss
- **GIVEN** the engine runs at 256-frame buffers
- **WHEN** the user records a 5-minute take while the UI is redrawing
- **THEN** the saved take is exactly as long as the recorded duration, in frames
- **AND** no buffer overruns are reported for the session
