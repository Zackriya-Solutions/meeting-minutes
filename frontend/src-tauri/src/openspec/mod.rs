use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GenerateOpenSpecInput {
    pub meeting_id: String,
}

pub mod commands;
pub mod service;

pub use commands::{api_generate_openspec_bundle, api_save_openspec_bundle_as, SaveOpenSpecBundleAsResult};
pub use service::{
    GenerateOpenSpecSuccess, OpenSpecErrorCode, OpenSpecErrorPayload, OpenSpecGenerationResult,
};
