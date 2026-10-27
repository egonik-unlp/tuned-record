# Delta Spec: add-practice-aids

**Capability:** pitch-feedback
**Change:** add-practice-aids

## MODIFIED Requirements

### Requirement: Live cents deviation
When both the live pitch and a target note (segmented from the transposed target: the reference, or the take chosen as target) at the playhead are voiced, the system SHALL show the deviation in cents between the live pitch and that note's pitch with a direction cue ("sing higher" / "sing lower" outside the in-tune band) and color the live line by deviation band (in tune, near, off); otherwise it SHALL show no deviation and draw the live line in a neutral color. The plot SHALL shade a corridor of ±20 ¢ and ±50 ¢ around the target notes' bars, ahead of and behind the playhead, and mark the note to sing at the playhead with its name.

#### Scenario: Singing on target
- **GIVEN** the reference note at the playhead is A3 with an offset of +4 st (target C#4)
- **WHEN** the user sings C#4 within ±20 cents
- **THEN** the deviation readout shows that value and the live line is drawn in the in-tune color

#### Scenario: Flat singing
- **GIVEN** the target note at the playhead is E4
- **WHEN** the user sings 60 cents below E4
- **THEN** the readout shows −60 ¢, "off" and "sing higher", and the live line is outside the shaded corridor

#### Scenario: Reference unvoiced
- **GIVEN** the reference is unvoiced at the playhead
- **WHEN** the user sings
- **THEN** no deviation is shown and the live line is drawn in the neutral color

#### Scenario: Steady voice over a wobbling guide
- **GIVEN** a target note at A3 whose raw guide contour wobbles ±30 cents
- **WHEN** the user holds A3 steadily within ±10 cents
- **THEN** the live line stays in the in-tune color for the whole note
