# Delta Spec: add-pitch-follow-recorder

**Capability:** take-layering
**Change:** add-pitch-follow-recorder

## Purpose

Recording whole takes on the song timeline and stacking them: every take stays in the mix and on screen as its own contour while the next one is recorded.

## ADDED Requirements

### Requirement: Whole-take recording
The system SHALL record a take from the song position where recording starts until it stops, store it as a new take placed at that position, and MUST NOT modify any existing take.

#### Scenario: Recording a new take
- **GIVEN** a project with one existing take
- **WHEN** the user starts recording at 00:30.000 and stops at 01:10.000
- **THEN** a second take exists spanning 00:30.000 to 01:10.000
- **AND** the first take is unchanged

### Requirement: Overdub playback
While recording, the system SHALL play the backing, the guide if enabled, and every audible existing take.

#### Scenario: Recording a harmony over a lead
- **GIVEN** a lead take covering the chorus
- **WHEN** the user records a new take over the chorus
- **THEN** the lead take is heard during the recording

### Requirement: Take contours
The system SHALL compute the pitch contour of each recorded take and draw every visible take's contour on the song timeline in its own distinct color, persistently alongside the reference and the live line.

#### Scenario: Contours pile up
- **GIVEN** two recorded takes over the same section
- **WHEN** the playhead passes that section
- **THEN** both take contours are drawn in two different colors
- **AND** the live line and the reference are drawn as well

### Requirement: Per-take controls
The system SHALL provide for each take: listen (play that take alone with the backing, from one second before it starts until just after it ends), mute, solo, volume, show/hide contour, and delete; deletion SHALL require confirmation and remove the take's audio and contour from the project. When a recording ends with the transport stopped, the playhead SHALL return to one second before the take's start so that Play replays it.

#### Scenario: Mute a take
- **GIVEN** two takes
- **WHEN** the user mutes take 1
- **THEN** take 1 is no longer heard and its contour remains visible

#### Scenario: Listen to a take
- **GIVEN** three takes, take 2 starting at 00:10.000
- **WHEN** the user presses listen on take 2
- **THEN** playback starts at 00:09.000 with the backing and take 2 only, and stops after take 2 ends

#### Scenario: Hide a contour
- **GIVEN** a take with a visible contour
- **WHEN** the user hides it
- **THEN** its contour is not drawn and its audio is still heard

#### Scenario: Delete a take
- **GIVEN** a take
- **WHEN** the user deletes it and confirms
- **THEN** the take is neither heard nor drawn, and it is gone after reopening the project
