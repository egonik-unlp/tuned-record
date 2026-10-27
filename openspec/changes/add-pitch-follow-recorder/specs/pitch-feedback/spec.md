# Delta Spec: add-pitch-follow-recorder

**Capability:** pitch-feedback
**Change:** add-pitch-follow-recorder

## Purpose

Telling the singer how close they are to the guide: live cents deviation against the transposed reference, and a per-phrase score of how well they followed the reference's shape regardless of absolute pitch.

## ADDED Requirements

### Requirement: Live cents deviation
When both the live pitch and the transposed target (the reference, or the take chosen as target) at the playhead are voiced, the system SHALL show the deviation in cents between them with a direction cue ("sing higher" / "sing lower" outside the in-tune band) and color the live line by deviation band (in tune, near, off); otherwise it SHALL show no deviation and draw the live line in a neutral color. The plot SHALL shade a corridor of ±20 ¢ and ±50 ¢ around the transposed target, ahead of and behind the playhead, and mark the note to sing at the playhead with its name.

#### Scenario: Singing on target
- **GIVEN** the reference at the playhead is A3 with an offset of +4 st (target C#4)
- **WHEN** the user sings C#4 within ±20 cents
- **THEN** the deviation readout shows that value and the live line is drawn in the in-tune color

#### Scenario: Flat singing
- **GIVEN** the target at the playhead is E4
- **WHEN** the user sings 60 cents below E4
- **THEN** the readout shows −60 ¢, "off" and "sing higher", and the live line is outside the shaded corridor

#### Scenario: Reference unvoiced
- **GIVEN** the reference is unvoiced at the playhead
- **WHEN** the user sings
- **THEN** no deviation is shown and the live line is drawn in the neutral color

### Requirement: Reference phrases
The system SHALL divide the reference contour into phrases at unvoiced gaps of at least 250 ms.

#### Scenario: Two lines separated by a breath
- **GIVEN** a reference with voiced segments 10.0–13.5 s and 13.9–17.0 s
- **WHEN** the reference is analysed
- **THEN** it has two phrases split at the 400 ms gap

### Requirement: Phrase shape score
After the playhead passes the end of a phrase during which the user sang, the system SHALL compute a shape score as the RMS, over frames where both the user and the reference are voiced, of the cents deviation minus its median, and SHALL show it under that phrase; when fewer than half of the phrase's voiced reference frames have voiced user pitch, it SHALL show no score.

#### Scenario: Same shape at a different pitch
- **GIVEN** a user contour that follows a phrase's reference contour exactly, offset by +7 semitones
- **WHEN** the phrase ends
- **THEN** the shape score shown is 0 cents (within rounding)

#### Scenario: Transposition does not change the score
- **GIVEN** the same sung contour evaluated once at an offset of +3 st and once at −5 st
- **WHEN** the shape score is computed for both
- **THEN** both scores are equal

#### Scenario: Too little singing
- **GIVEN** the user was voiced for less than half of a phrase's voiced reference frames
- **WHEN** the phrase ends
- **THEN** no score is shown for that phrase

### Requirement: Scores kept with takes
The system SHALL store the phrase shape scores computed while recording a take with that take, and show them when the take's contour is visible.

#### Scenario: Reopening shows take scores
- **GIVEN** a take recorded with phrase scores
- **WHEN** the project is reopened
- **THEN** the take's phrase scores are shown under its phrases
