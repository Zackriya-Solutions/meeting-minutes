# Spec Delta: openspec-generation

## ADDED Requirements

### Requirement: Generate OpenSpec Button
The meeting details page SHALL display a "Generate OpenSpec" action next to
the existing Summary action, available whenever a transcript exists for the
meeting.

#### Scenario: Button visible with transcript present
- **GIVEN** a meeting with a completed transcript
- **WHEN** the user views the meeting details page
- **THEN** a "Generate OpenSpec" button is shown next to the Summary button group

#### Scenario: Button disabled without transcript
- **GIVEN** a meeting with no transcript yet (still recording/transcribing)
- **WHEN** the user views the meeting details page
- **THEN** the "Generate OpenSpec" button is disabled or hidden, consistent
  with how the Summary button behaves without a transcript

### Requirement: Button State Machine
The Generate OpenSpec button SHALL expose four visual/interactive states:
idle, generating, error, and done.

#### Scenario: Idle to generating
- **GIVEN** the button is idle
- **WHEN** the user clicks it
- **THEN** the button enters `generating` state, shows a progress indicator,
  and is disabled to prevent duplicate invocations

#### Scenario: Generating to done
- **GIVEN** the button is in `generating` state
- **WHEN** the backend returns a successful OpenSpec artifact bundle
- **THEN** the button enters `done` state and a native "Save As" dialog opens
  automatically for the produced zip file

#### Scenario: Generating to error
- **GIVEN** the button is in `generating` state
- **WHEN** the backend returns a failure (CLI missing, CLI failure, timeout)
- **THEN** the button enters `error` state, displays an actionable message
  describing the failure, and returns to `idle` on next click

#### Scenario: Done to idle for regeneration
- **GIVEN** the button is in `done` state from a previous generation
- **WHEN** the user clicks "Regenerate"
- **THEN** the button returns to `generating` state and a new generation run
  starts, overwriting the previous output for that meeting

### Requirement: Node.js and OpenSpec CLI Detection
The system SHALL detect whether Node.js (and therefore the ability to run
the `openspec` CLI via `npx`) is available before attempting generation, and
SHALL guide the user to install it if missing, mirroring the existing
"Ollama not installed" detection/guidance pattern.

#### Scenario: Node.js missing
- **GIVEN** Node.js is not found on the user's machine
- **WHEN** the user clicks "Generate OpenSpec"
- **THEN** the system shows a blocking, actionable message explaining Node.js
  is required, with a link to install instructions, using the same UX
  pattern/component as the existing "Ollama not installed" message
- **AND** no CLI invocation is attempted

#### Scenario: Node.js present
- **GIVEN** Node.js is found (via version check, e.g. `node --version`)
- **WHEN** the user clicks "Generate OpenSpec"
- **THEN** the system proceeds to invoke the OpenSpec CLI

### Requirement: OpenSpec CLI Invocation
The system SHALL invoke the real `openspec` CLI (https://github.com/Fission-AI/OpenSpec)
from the Tauri/Rust backend, using the meeting transcript (and summary, if
available) as the source context, and SHALL produce the standard OpenSpec
artifact set for a single change.

#### Scenario: Successful CLI run
- **GIVEN** Node.js is available and a transcript exists
- **WHEN** the backend runs `npx openspec@latest` (or a detected global
  install) against a working directory seeded with transcript-derived context
- **THEN** the CLI produces `openspec/changes/<slug>/proposal.md`,
  `specs/**`, `design.md`, and `tasks.md` in that working directory

#### Scenario: CLI process failure
- **GIVEN** Node.js is available
- **WHEN** the `openspec` CLI process exits with a non-zero code or writes to
  stderr indicating failure
- **THEN** the backend command returns a structured error, and the frontend
  transitions the button to `error` state with the CLI's failure summary

#### Scenario: CLI network/timeout failure
- **GIVEN** Node.js is available
- **WHEN** `npx openspec@latest` cannot resolve/download the package (no
  network, registry unreachable) or the process runs past a bounded timeout
- **THEN** the backend aborts the process, returns a structured
  network/timeout error, and the frontend shows an actionable error message
  distinct from a generic CLI failure

### Requirement: Packaging and Download
The system SHALL package the generated OpenSpec artifacts as a single `.zip`
file and SHALL trigger the operating system's native "Save As" dialog so the
user can choose where to store it.

#### Scenario: Zip creation
- **GIVEN** the OpenSpec CLI run completed successfully
- **WHEN** the backend command finishes
- **THEN** it zips the entire `openspec/changes/<slug>/` folder into a single
  `.zip` file

#### Scenario: Save dialog triggered
- **GIVEN** the zip file was created successfully
- **WHEN** the frontend receives the success result
- **THEN** it invokes the native "Save As" file dialog defaulting to a
  filename derived from the meeting title/slug, letting the user pick the
  destination path

### Requirement: Regeneration Overwrite Semantics
Regenerating OpenSpec artifacts for the same meeting SHALL overwrite the
previously generated working-directory output; the system SHALL NOT keep
multiple versions.

#### Scenario: Regenerate overwrites prior output
- **GIVEN** a meeting already has a generated OpenSpec working directory
  from a previous run
- **WHEN** the user clicks "Regenerate"
- **THEN** the backend clears/overwrites the prior working directory content
  for that meeting before running the CLI again, matching the overwrite
  behavior already used for Summary regeneration
