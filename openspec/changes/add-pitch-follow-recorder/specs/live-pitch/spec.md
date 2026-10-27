# Delta Spec: add-pitch-follow-recorder

**Capability:** live-pitch
**Change:** add-pitch-follow-recorder

## Purpose

Live fundamental-frequency tracking of the singer's mic, shown as a scrolling contour on a note grid with the current note and cents, fast enough to sing against.

## ADDED Requirements

### Requirement: Live pitch estimation
The system SHALL estimate the fundamental frequency of the mic signal at least 100 times per second over the vocal range 65–1100 Hz, and SHALL mark frames without a confident pitch as unvoiced.

#### Scenario: Sustained sung note
- **GIVEN** the engine is running
- **WHEN** the user sings a steady A3 (220 Hz)
- **THEN** the estimated pitch stays within ±10 cents of 220 Hz while the note is held

#### Scenario: Silence and consonants
- **GIVEN** the engine is running
- **WHEN** the mic picks up silence or an unvoiced consonant
- **THEN** those frames are marked unvoiced and no pitch is drawn for them

### Requirement: Octave error suppression
The system SHALL suppress isolated octave jumps in the live estimate so that a steady note is not drawn with momentary one-octave excursions.

#### Scenario: Steady note with a spurious octave frame
- **GIVEN** the user holds a steady note
- **WHEN** the detector produces a single frame one octave off
- **THEN** the drawn contour does not show the octave excursion

### Requirement: Live contour display
The system SHALL draw the live pitch as a continuous line on a vertical axis graduated in semitones with labelled note names, positioned by song time while the transport runs, ending in a dot at the most recent voiced pitch. While the transport is stopped the plot stays frozen: no live line is drawn and the view does not follow the voice; the current note readout keeps updating.

#### Scenario: Monitoring with the transport stopped
- **GIVEN** no playback is running
- **WHEN** the user sings
- **THEN** the plot does not move and the current note readout shows the sung note

#### Scenario: Singing during playback
- **GIVEN** playback is running
- **WHEN** the user sings
- **THEN** the live line is drawn at the playhead, at the song time it was sung

### Requirement: Plot navigation and zoom
The system SHALL show an overview of the whole song (reference phrases, takes, loop range, the visible window and the playhead) that jumps to the clicked or dragged position; SHALL let the user jump by clicking the plot, move through the song by dragging it horizontally while stopped, jump to the previous/next phrase, and loop the phrase at the playhead; and SHALL let the user zoom time (2–120 s visible) and pitch (6–48 semitones visible), and fit the pitch range to the target in view. The view SHALL follow the target, not the singer, when a target is visible.

#### Scenario: Jump from the overview
- **GIVEN** a 3-minute song, stopped at 00:10.000
- **WHEN** the user clicks the overview at its midpoint
- **THEN** the song position becomes about 01:30.000 and the plot shows that part of the song

#### Scenario: Practise one phrase
- **GIVEN** the playhead is inside a phrase from 00:20.000 to 00:24.000
- **WHEN** the user loops the phrase and plays
- **THEN** playback repeats 00:19.000–00:24.500 until the loop is turned off

#### Scenario: Zoom on pitch
- **GIVEN** the plot shows 24 semitones
- **WHEN** the user fits the pitch range to a phrase spanning C4–G4
- **THEN** about 11 semitones are shown, centred on the phrase

### Requirement: Current note readout
The system SHALL display the nearest note name with octave and the deviation in cents from that note for the most recent voiced frame.

#### Scenario: Slightly sharp note
- **GIVEN** the engine is running
- **WHEN** the user sings 225 Hz
- **THEN** the readout shows A3 and approximately +39 cents

### Requirement: Voice-to-display latency
The time from a pitched sound reaching the mic to its pitch being drawn on screen MUST be under 40 ms when the engine runs at 256-frame buffers.

#### Scenario: Measured onset latency
- **GIVEN** the engine runs at 256-frame buffers at 48 kHz
- **WHEN** a pitched tone starts at the input
- **THEN** its pitch is drawn on screen less than 40 ms after the tone's onset
