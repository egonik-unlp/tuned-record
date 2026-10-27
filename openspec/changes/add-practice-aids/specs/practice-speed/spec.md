# Delta Spec: add-practice-aids

**Capability:** practice-speed
**Change:** add-practice-aids

## Purpose

Practice a part at a reduced speed with its pitch unchanged, so that short notes last long enough to be found by ear and held, without affecting how takes are recorded.

## ADDED Requirements

### Requirement: Fixed practice speeds
The system SHALL offer exactly four playback speeds, 50 %, 60 %, 75 % and 100 %, selectable from the transport and with the keyboard (`,` one step slower, `.` one step faster). Every project opening SHALL start at 100 %; the chosen speed MUST NOT be saved with the project.

#### Scenario: Choosing a speed
- **GIVEN** a project with a backing track and a vocal stem, playing at 100 %
- **WHEN** the user selects 60 %
- **THEN** the transport shows 60 % as the active speed
- **AND** pressing `.` selects 75 % and pressing `,` twice from there selects 50 %

#### Scenario: Speed is not remembered
- **GIVEN** the user practised a project at 75 % and closed it
- **WHEN** the project is reopened
- **THEN** the speed is 100 %

### Requirement: Pitch-preserving slowed playback
At a speed below 100 %, the system SHALL play the backing track and the audible guide slower by that factor with their pitch unchanged, and SHALL advance the playhead, the plot and the song overview at the same reduced rate.

#### Scenario: Same key, slower
- **GIVEN** the guide holds A3 (220 Hz) at 00:30.000
- **WHEN** the user plays from 00:29.000 at 60 %
- **THEN** that note is heard at 220 Hz within ±5 cents
- **AND** it is reached after 1/0.6 ≈ 1.67 s of wall-clock time

#### Scenario: Plot scrolls at the reduced rate
- **GIVEN** playback at 50 %
- **WHEN** 2 s of wall-clock time pass
- **THEN** the playhead has advanced 1 s of song time

### Requirement: Stretched audio rendered once and cached
The first time a speed below 100 % is selected for a project's sources, the system SHALL render the slowed audio in the background, showing progress, keeping the UI responsive and staying at the previous speed until the render is ready. The rendered audio SHALL be cached in the project folder and reused while the sources are unchanged.

#### Scenario: First use of a speed
- **GIVEN** a project whose backing and guide have never been rendered at 60 %
- **WHEN** the user selects 60 %
- **THEN** a progress indicator is shown while the plot, transport and navigation keep responding
- **AND** when the render finishes, playback uses 60 %

#### Scenario: Reuse
- **GIVEN** the project was already rendered at 60 % in an earlier session
- **WHEN** the user selects 60 %
- **THEN** the speed applies without rendering again

#### Scenario: Source replaced
- **GIVEN** a cached 60 % render exists
- **WHEN** the user imports a different backing track
- **THEN** the next selection of 60 % renders again from the new backing

### Requirement: Practice-only below full speed
Below 100 %, the system MUST NOT start a recording or a latency calibration, and SHALL tell the user to return to 100 % when they try. Takes SHALL NOT be heard below 100 %, and their contours SHALL stay drawn.

#### Scenario: Record pressed while slowed
- **GIVEN** the speed is 75 %
- **WHEN** the user presses R
- **THEN** no take is created and a message says recording needs 100 %

#### Scenario: Takes while slowed
- **GIVEN** a project with two audible takes and the speed at 60 %
- **WHEN** playback runs
- **THEN** only the backing and the audible guide are heard
- **AND** both takes' contours are drawn at their song positions

#### Scenario: Back to full speed
- **GIVEN** the user practised at 60 %
- **WHEN** they select 100 % and record a take
- **THEN** the take is recorded and aligned exactly as at 100 % before any slowed practice

### Requirement: Position and loop kept across speeds
Changing the speed SHALL keep the playhead at the same song position, and a loop range SHALL keep repeating the same song span at the new speed.

#### Scenario: Switching speed while playing
- **GIVEN** playback at 100 % at 01:12.400
- **WHEN** the user selects 50 %
- **THEN** playback continues from 01:12.400 (within one audio buffer) at 50 %

#### Scenario: Slow loop
- **GIVEN** a loop on the phrase 00:09.000–00:14.500 at 60 %
- **WHEN** the playhead reaches 00:14.500
- **THEN** playback continues from 00:09.000 at 60 %
