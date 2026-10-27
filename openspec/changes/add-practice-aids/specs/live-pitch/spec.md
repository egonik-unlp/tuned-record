# Delta Spec: add-practice-aids

**Capability:** live-pitch
**Change:** add-practice-aids

## MODIFIED Requirements

### Requirement: Live pitch estimation
The system SHALL estimate the fundamental frequency of the mic signal at least 100 times per second over the vocal range 65–1100 Hz, and SHALL mark frames without a confident pitch as unvoiced. Once a note has started, the system SHALL keep it voiced through brief dips in pitch confidence (voicing hysteresis), as long as the signal stays pitched and above the silence level.

#### Scenario: Sustained sung note
- **GIVEN** the engine is running
- **WHEN** the user sings a steady A3 (220 Hz)
- **THEN** the estimated pitch stays within ±10 cents of 220 Hz while the note is held

#### Scenario: Silence and consonants
- **GIVEN** the engine is running
- **WHEN** the mic picks up silence or an unvoiced consonant
- **THEN** those frames are marked unvoiced and no pitch is drawn for them

#### Scenario: Brief confidence dip inside a note
- **GIVEN** the user holds a steady note whose pitch confidence drops briefly below the voicing threshold (e.g. breathiness or a vibrato peak) while the level stays above silence
- **WHEN** the note is tracked
- **THEN** those frames stay voiced at the held pitch

### Requirement: Live contour display
The system SHALL draw the live pitch as a continuous line on a vertical axis graduated in semitones with labelled note names, positioned by song time while the transport runs, ending in a dot at the most recent voiced pitch. Gaps of unvoiced frames shorter than 50 ms between voiced frames less than one semitone apart SHALL be bridged, so that a held note is drawn as one line. While the transport is stopped the plot stays frozen: no live line is drawn and the view does not follow the voice; the current note readout keeps updating.

#### Scenario: Monitoring with the transport stopped
- **GIVEN** no playback is running
- **WHEN** the user sings
- **THEN** the plot does not move and the current note readout shows the sung note

#### Scenario: Singing during playback
- **GIVEN** playback is running
- **WHEN** the user sings
- **THEN** the live line is drawn at the playhead, at the song time it was sung

#### Scenario: Held note with dropouts
- **GIVEN** playback is running and the detector reports a held E4 with unvoiced gaps of 10–40 ms
- **WHEN** the live line is drawn
- **THEN** the note is drawn as one unbroken line, without isolated dots

#### Scenario: Real pause
- **GIVEN** playback is running
- **WHEN** the user stops singing for 200 ms between two notes
- **THEN** the line is broken during the pause
