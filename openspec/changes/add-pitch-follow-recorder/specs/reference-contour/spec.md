# Delta Spec: add-pitch-follow-recorder

**Capability:** reference-contour
**Change:** add-pitch-follow-recorder

## Purpose

Importing a backing track and a demucs-separated vocal stem, turning the stem into a pitch contour, and showing it as a guide to follow: visible ahead of the playhead, audible on demand, and transposable.

## ADDED Requirements

### Requirement: Track import
The system SHALL import a backing track and a vocal stem from WAV, FLAC, MP3 or OGG files, resampling them to the engine sample rate, and SHALL place both at song t=0.

#### Scenario: Import at a different sample rate
- **GIVEN** the engine runs at 48 kHz
- **WHEN** the user imports a 44.1 kHz backing track and a 44.1 kHz vocal stem
- **THEN** both play back at the correct speed and pitch
- **AND** both start at song t=0

#### Scenario: Unsupported file
- **GIVEN** a file that cannot be decoded
- **WHEN** the user imports it
- **THEN** an error names the file and the reason, and the project is left unchanged

### Requirement: Offline reference analysis
The system SHALL compute the pitch contour of the imported vocal stem offline, with per-frame voicing, show progress while it runs, and cache the result in the project so it is not recomputed for the same stem.

#### Scenario: First import of a stem
- **GIVEN** a project without a vocal stem
- **WHEN** the user imports a vocal stem
- **THEN** analysis progress is shown
- **AND** when it finishes, the reference contour is drawn, with gaps where the stem is unvoiced

#### Scenario: Reopening uses the cache
- **GIVEN** a project whose stem was already analysed
- **WHEN** the project is reopened
- **THEN** the reference contour is shown without re-running the analysis

### Requirement: Fixed playhead with future guide
The system SHALL draw a playhead at a fixed horizontal position, with the reference contour scrolling under it, showing at least 4 seconds of the upcoming reference ahead of the playhead in a visually distinct (ghost) style.

#### Scenario: Seeing what comes next
- **GIVEN** playback is running at 01:00.000
- **WHEN** the user looks at the plot
- **THEN** the reference from at least 01:00.000 to 01:04.000 is visible to the right of the playhead in ghost style
- **AND** the reference already passed is visible to the left of the playhead

### Requirement: Audible guide
The system SHALL let the user switch playback of the vocal stem on and off and set its level, independently of the backing.

#### Scenario: Guide enabled
- **GIVEN** a project with a vocal stem and the guide switched on
- **WHEN** playback runs
- **THEN** the vocal stem is heard together with the backing at the guide level

#### Scenario: Guide disabled
- **GIVEN** the guide is switched off
- **WHEN** playback runs
- **THEN** only the backing and the audible takes are heard

### Requirement: Guide transposition
The system SHALL let the user offset the target by whole semitones (step up/down, or by choosing a named interval such as "major 3rd above"), by Ctrl-dragging the plot vertically, and by snapping it to the current live pitch; the offset SHALL move the drawn target and the cents target, and SHALL NOT alter the audible guide. While the reference is the target and the offset is not zero, the untransposed reference SHALL stay drawn (dimmed) as the melody to harmonise against.

#### Scenario: Semitone steps
- **GIVEN** the reference shows a note at C4
- **WHEN** the user steps the offset up 4 semitones
- **THEN** that note is drawn at E4 and the offset readout shows +4 st

#### Scenario: Snap to voice
- **GIVEN** the reference at the playhead is at C4 and the user is singing a voiced F4
- **WHEN** the user triggers snap-to-voice
- **THEN** the offset becomes +5 st and the reference at the playhead is drawn at F4

#### Scenario: Named interval
- **GIVEN** the offset is 0
- **WHEN** the user chooses "minor 3rd below"
- **THEN** the offset becomes −3 st and the melody stays drawn at its original pitch

#### Scenario: Audible guide unaffected
- **GIVEN** the guide is audible and the offset is +4 st
- **WHEN** playback runs
- **THEN** the vocal stem is heard at its original pitch

### Requirement: Harmony target
The system SHALL let the user choose what to sing against: the reference (default) or any take; the offset applies to the chosen target, and the deviation, live line colour, corridor and target marker SHALL follow it. The choice SHALL be saved with the project and fall back to the reference when the chosen take is deleted.

#### Scenario: Harmonising on a take
- **GIVEN** take 1 is the lead and take 1 is chosen as target with offset +3 st
- **WHEN** the user sings
- **THEN** the deviation is measured against take 1's pitch raised 3 semitones
