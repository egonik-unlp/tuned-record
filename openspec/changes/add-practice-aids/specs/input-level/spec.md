# Delta Spec: add-practice-aids

**Capability:** input-level
**Change:** add-practice-aids

## Purpose

Show the singer how hot their microphone input is, and warn them when it is clipping or too quiet for reliable pitch tracking.

## ADDED Requirements

### Requirement: Input level meter
The system SHALL show the level of the selected input in the top bar, updated at least 20 times per second, whenever the audio engine is running, whether or not the transport is playing.

#### Scenario: Singing into the mic
- **GIVEN** the engine is running and the transport is stopped
- **WHEN** the user sings
- **THEN** the meter rises with the voice and falls when they stop

### Requirement: Clipping warning
The system SHALL show a clipping warning, in text and not by colour alone, when input samples reach or exceed full scale, and SHALL keep it visible for at least 2 seconds after the last clipped sample.

#### Scenario: Overdriven input
- **GIVEN** an input like `take-003`, with a large share of samples at or above 0 dBFS
- **WHEN** it is fed to the engine
- **THEN** the clipping warning is shown and suggests lowering the input gain

#### Scenario: Correct level
- **GIVEN** a voice peaking below −3 dBFS
- **WHEN** the user sings
- **THEN** no clipping warning is shown

### Requirement: Too-quiet warning
The system SHALL show a too-quiet warning, in text, when the user is singing during playback but the input stays so low that pitch tracking is unreliable, and SHALL not show it during silence between phrases.

#### Scenario: Gain too low
- **GIVEN** playback is running and the input gain is so low that sung notes barely exceed the detector's level threshold
- **WHEN** the user sings a phrase
- **THEN** a too-quiet warning suggests raising the input gain

#### Scenario: Resting between phrases
- **GIVEN** a correctly levelled input
- **WHEN** the user is silent between phrases
- **THEN** no too-quiet warning is shown
