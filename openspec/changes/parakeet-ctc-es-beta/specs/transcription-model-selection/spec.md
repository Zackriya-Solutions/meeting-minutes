# Spec Delta: transcription-model-selection

## ADDED Requirements

### Requirement: Parakeet CTC ES Beta Option In Settings

The system SHALL display Parakeet CTC ES as a downloadable **Beta** transcription model option inside Transcript Settings, using the same model-selection and download interaction pattern as the existing local downloadable transcription models.

#### Scenario: Beta model visible in transcript settings

- **GIVEN** a user opens Transcript Settings
- **WHEN** local downloadable transcription models are listed
- **THEN** the system shows a Parakeet CTC ES option labeled as **Beta**
- **AND** the option includes descriptive usage copy aligned with the style used by the other transcription models

#### Scenario: Existing default remains visually primary

- **GIVEN** Transcript Settings are shown
- **WHEN** the user has not explicitly selected Parakeet CTC ES
- **THEN** the current Parakeet TDT option remains the default/recommended path
- **AND** Parakeet CTC ES is shown as an explicit beta alternative rather than a replacement

### Requirement: Selected Beta Model Persists Across Sessions

The system SHALL persist a user's explicit selection of Parakeet CTC ES through the existing transcript provider/model configuration flow and SHALL restore that selection in later sessions until the user changes it.

#### Scenario: Beta selection is remembered

- **GIVEN** a user selects Parakeet CTC ES in Transcript Settings
- **WHEN** the user restarts the application or returns in a later session
- **THEN** the system restores Parakeet CTC ES as the selected transcription model
- **AND** the selection remains active until the user chooses a different model

#### Scenario: Existing users are not silently migrated

- **GIVEN** a user previously used the current Parakeet TDT default path
- **WHEN** the beta model becomes available in the application
- **THEN** the system keeps the user's existing transcription selection unchanged
- **AND** no automatic migration to Parakeet CTC ES occurs

### Requirement: Backend Model Lifecycle Supports Parakeet CTC ES

The system SHALL allow the Parakeet transcription backend to report, download, validate, load, and run Parakeet CTC ES alongside the existing Parakeet TDT variants.

#### Scenario: Backend exposes beta model in downloadable model inventory

- **GIVEN** the frontend requests available Parakeet transcription models
- **WHEN** backend model inventory is returned
- **THEN** Parakeet CTC ES appears as a valid downloadable model entry
- **AND** its availability/download state is represented using the same lifecycle conventions as the existing downloadable Parakeet models

#### Scenario: Selected beta model is treated as valid runtime choice

- **GIVEN** a user has selected Parakeet CTC ES
- **WHEN** the application prepares local transcription for use
- **THEN** backend validation recognizes Parakeet CTC ES as a valid selectable Parakeet model
- **AND** model loading uses the selected CTC ES variant rather than falling back to a different Parakeet model silently

### Requirement: Live Recording Compatibility Gate

Parakeet CTC ES beta SHALL not be accepted as complete unless it is proven usable in the live-recording transcription path.

#### Scenario: Live recording can initialize with beta model selected

- **GIVEN** Parakeet CTC ES is downloaded and selected in Transcript Settings
- **WHEN** a user starts a live recording session
- **THEN** the application initializes local transcription successfully with Parakeet CTC ES selected
- **AND** the live-recording flow does not fail solely because the selected model differs from the current Parakeet TDT default

#### Scenario: Completion requires live-path evidence

- **GIVEN** the beta option appears in Settings and can be selected
- **WHEN** the change is evaluated for completion
- **THEN** acceptance requires explicit evidence that live recording works with Parakeet CTC ES selected
- **AND** the change is not considered complete based only on settings, import, or retranscription coverage

### Requirement: Onboarding And Default Path Stay Unchanged In First Slice

The first slice SHALL keep onboarding and default transcription guidance unchanged while introducing Parakeet CTC ES only through Settings.

#### Scenario: Onboarding does not promote beta model

- **GIVEN** a user is in onboarding or first-run setup
- **WHEN** the application presents local transcription guidance
- **THEN** the current onboarding behavior remains unchanged
- **AND** Parakeet CTC ES is not promoted as the default onboarding recommendation

#### Scenario: Start-recording checks preserve current default behavior

- **GIVEN** the user has not explicitly switched to Parakeet CTC ES
- **WHEN** the application runs start-recording readiness checks
- **THEN** the current Parakeet TDT default path behaves as before
- **AND** introducing the beta option does not regress the existing default recording path
