use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateOpenSpecInput {
    pub meeting_id: String,
}

pub mod commands;
pub mod generator;
pub mod service;
pub mod setup;

pub use commands::{
    api_generate_openspec_bundle, api_save_openspec_bundle_as, check_openspec_setup_status,
    cancel_openspec_generation, check_node_runtime_status, install_node_runtime, install_openspec_cli, install_openspec_setup,
    skip_openspec_setup, SaveOpenSpecBundleAsResult,
};
pub use service::{
    GenerateOpenSpecSuccess, OpenSpecErrorCode, OpenSpecErrorPayload, OpenSpecGenerationResult,
};
pub use setup::{NodeRuntimeStatusPayload, OpenSpecSetupDecision, OpenSpecSetupStatusPayload};
