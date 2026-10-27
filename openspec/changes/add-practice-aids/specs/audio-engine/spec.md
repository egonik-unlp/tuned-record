# Delta Spec: add-practice-aids

**Capability:** audio-engine
**Change:** add-practice-aids

## MODIFIED Requirements

### Requirement: Single song clock
The system SHALL keep one song position, measured in frames since song t=0 and advanced only by the frames processed in the audio callback multiplied by the current practice speed (exactly the processed frames at 100 %), or moved by a seek, or by wrapping to the start of a loop range while playing and not recording, and SHALL key all playback, capture and pitch data to it.

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

#### Scenario: Slowed clock
- **GIVEN** playback at 50 % from 00:10.000
- **WHEN** 48 000 frames are processed at 48 kHz
- **THEN** the song position is 00:10.500

### Requirement: Latency-compensated capture
The system MUST write each captured sample to the song position at which the user heard the corresponding playback, compensating the round-trip latency reported by the JACK ports plus a user calibration offset stored in the application configuration. Below 100 % speed, the live pitch MUST be placed the same way, with the compensation converted to song time at the speed in effect.

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
