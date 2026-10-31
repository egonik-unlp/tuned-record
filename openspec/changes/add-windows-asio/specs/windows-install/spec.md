# Delta Spec: add-windows-asio

**Capability:** windows-install
**Change:** add-windows-asio

## Purpose

How tuned-record gets onto a Windows PC: a setup program built on the owner's Linux machine that installs the app for a non-technical user, and how the installed app presents itself on Windows.

## ADDED Requirements

### Requirement: Windows build from Linux
The project SHALL provide one command, run on the owner's Linux machine, that cross-compiles the application for 64-bit Windows and packages it into a single setup program, without CI and without a Windows machine. The installed application MUST NOT need any runtime library that the setup program does not install.

#### Scenario: Build the setup program
- **GIVEN** the documented Linux build prerequisites are installed
- **WHEN** the owner runs the Windows build command
- **THEN** a single setup `.exe` is produced in the build output folder

#### Scenario: Smoke test under Wine
- **GIVEN** the Windows executable has been built
- **WHEN** it is started under Wine on the Linux machine
- **THEN** its window opens, and the absence of an ASIO driver is reported as an audio error rather than a crash

### Requirement: Windows installation
The setup program SHALL install the application for the current user, add a Start-menu entry, and register an uninstaller in Windows' installed apps list. The installed application SHALL start without a console window and SHALL keep its configuration under the user's application-data folder, so it survives reinstalling or upgrading.

#### Scenario: Install and start
- **GIVEN** a Windows PC without tuned-record
- **WHEN** the user runs the setup program and accepts its defaults
- **THEN** tuned-record appears in the Start menu
- **AND** starting it opens the app window with no console window

#### Scenario: Upgrade keeps settings
- **GIVEN** tuned-record installed with a chosen mic channel and a stored calibration
- **WHEN** the setup program of a newer build is run over it
- **THEN** the app starts with the same mic channel and calibration

#### Scenario: Uninstall
- **GIVEN** tuned-record installed
- **WHEN** the user uninstalls it from Windows' installed apps list
- **THEN** the program files and the Start-menu entry are removed
- **AND** the user's recordings are left untouched
