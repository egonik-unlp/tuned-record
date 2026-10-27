# Delta Spec: add-practice-aids

**Capability:** note-targets
**Change:** add-practice-aids

## Purpose

Turn the wobbly target contour into steady notes to aim at, and tell the singer note by note whether they reached each one.

## ADDED Requirements

### Requirement: Target segmented into notes
The system SHALL segment the current target contour (the guide, or the take chosen as harmony target) into notes, each with a start, an end and one steady pitch representative of its sustained part, ignoring vibrato, wobble and the glides between notes. Notes SHALL follow the target's transposition offset.

#### Scenario: Wobbly held note
- **GIVEN** a guide note around A3 that wobbles ±30 cents for 400 ms
- **WHEN** the target is segmented
- **THEN** it yields one note of about 400 ms at A3 within ±10 cents

#### Scenario: Step between notes
- **GIVEN** a guide that holds A3 for 300 ms, glides for 60 ms and holds B3 for 300 ms
- **WHEN** the target is segmented
- **THEN** it yields two notes, A3 and B3, and no note for the glide

#### Scenario: Transposed target
- **GIVEN** a segmented guide note at A3 and an offset of +4 semitones
- **WHEN** the plot is drawn
- **THEN** the note is shown at C#4

#### Scenario: Take as target
- **GIVEN** the user chooses take 2 as the harmony target
- **WHEN** the plot is drawn
- **THEN** the notes shown are segmented from take 2's contour

### Requirement: Note bars over the dimmed contour
The system SHALL draw each target note as a bar spanning its duration at its pitch, ahead of the playhead in the ghost style and behind it solid, and SHALL keep drawing the raw target contour, dimmed, underneath the bars.

#### Scenario: Seeing the next notes
- **GIVEN** playback running and a segmented guide
- **WHEN** the user looks ahead of the playhead
- **THEN** the upcoming notes are visible as ghost-style bars, with the dimmed raw contour beneath them

### Requirement: Reached notes
The system SHALL mark a note as reached when, during the note, the live pitch stays within ±20 cents of the note's pitch for at least 100 ms continuously, or, for a note shorter than 100 ms, for at least half its duration. The mark SHALL be visible on the bar and SHALL be distinguishable without relying on colour alone. Reached marks SHALL belong to the current pass and be cleared when playback is restarted or jumps.

#### Scenario: Held long enough
- **GIVEN** a 400 ms note at E4
- **WHEN** the user sings E4 within ±20 cents for 150 ms during it
- **THEN** the bar is marked reached

#### Scenario: Too short
- **GIVEN** a 400 ms note at E4
- **WHEN** the user is within ±20 cents of E4 for only 60 ms during it
- **THEN** the bar is not marked reached

#### Scenario: Short note
- **GIVEN** an 80 ms note at G4
- **WHEN** the user is within ±20 cents of G4 for 45 ms during it
- **THEN** the bar is marked reached

#### Scenario: Off pitch
- **GIVEN** a 400 ms note at E4
- **WHEN** the user holds a note 60 cents below E4 for the whole note
- **THEN** the bar is not marked reached

#### Scenario: New pass
- **GIVEN** several bars marked reached in the current pass
- **WHEN** the user jumps back to the start of the phrase and plays again
- **THEN** the marks of the notes ahead of the playhead are cleared
