# Delta Spec: add-pitch-follow-recorder

**Capability:** audio-export
**Change:** add-pitch-follow-recorder

## Purpose

Getting the session out to a DAW: one aligned file per take plus a full mixdown, all droppable at the start of the timeline without manual shifting.

## ADDED Requirements

### Requirement: Aligned per-take export
The system SHALL export every take, regardless of mute or solo state, as its own WAV file that starts at song t=0 (silence before the take), lasts the full song length, and uses the engine sample rate.

#### Scenario: Take starting mid-song
- **GIVEN** a take recorded from 00:30.000 to 01:10.000 in a 3:00 song
- **WHEN** the user exports
- **THEN** the take's WAV is 3:00 long with silence until 00:30.000 and the take's audio from 00:30.000

#### Scenario: Muted takes still exported
- **GIVEN** three takes with take 2 muted
- **WHEN** the user exports
- **THEN** three per-take WAV files are written

### Requirement: Mixdown export
The system SHALL export a stereo mixdown WAV of the backing plus the takes audible under the mute/solo rules at their volumes, excluding the guide stem, starting at song t=0, with the same length and sample rate as the per-take files and without clipping.

#### Scenario: Mixdown reflects the mix
- **GIVEN** a backing and three takes with take 2 muted
- **WHEN** the user exports
- **THEN** the mixdown contains the backing, take 1 and take 3, but not take 2 and not the guide stem

#### Scenario: Loud mix does not clip
- **GIVEN** a backing and takes whose summed peaks exceed full scale
- **WHEN** the user exports
- **THEN** the mixdown file contains the summed signal without hard clipping

### Requirement: DAW-ready output
The system SHALL write all exported files into one user-chosen folder with names that identify the project and each take, so that importing them all at the start of a DAW timeline reproduces the session's alignment.

#### Scenario: Import into a DAW
- **GIVEN** an export of a project with two takes
- **WHEN** all files are placed at the start of a DAW timeline
- **THEN** the takes line up with the backing in the mixdown as they did in the app
