# Delta Spec: add-pitch-follow-recorder

**Capability:** project-session
**Change:** add-pitch-follow-recorder

## Purpose

Persisting a recording session as a self-contained project folder, so layering can continue across sessions.

## ADDED Requirements

### Requirement: Project folder
The system SHALL let the user create a new project in an empty folder or open an existing project folder, and SHALL store inside it copies of the imported sources, the cached reference analysis, the takes' audio and contours, and all take and guide settings.

#### Scenario: Create a project
- **GIVEN** an empty folder
- **WHEN** the user creates a new project there
- **THEN** the folder contains a project manifest and the app shows an empty project

#### Scenario: Project survives moving the original files
- **GIVEN** a project whose backing and stem were imported from another folder
- **WHEN** the original files are deleted and the project is reopened
- **THEN** the backing, stem and reference contour are all available

### Requirement: Continuous saving
The system SHALL write take audio to the project folder while it is being recorded and SHALL save setting changes without an explicit save action.

#### Scenario: Unexpected exit during recording
- **GIVEN** the user is recording a take
- **WHEN** the application exits unexpectedly
- **THEN** on reopening, the audio recorded up to shortly before the exit is available as a take

#### Scenario: Settings persist
- **GIVEN** the user changes a take's volume and the guide offset
- **WHEN** the project is closed and reopened
- **THEN** the take's volume and the guide offset are as they were left

### Requirement: Faithful reopen
Reopening a project SHALL restore the sources, reference contour, takes with their positions, contours and controls, and the guide settings, with takes still aligned to the backing.

#### Scenario: Layering in a later session
- **GIVEN** a project with a take recorded in an earlier session
- **WHEN** the user reopens it and plays it back
- **THEN** the earlier take plays aligned with the backing, as it did when recorded
