// Built-in AI summary engine module
// Provides local LLM inference via llama-helper sidecar

pub mod client;
pub mod commands;
pub mod model_manager;
pub mod models;
pub mod sidecar;

// Re-export commonly used types
pub use client::{generate_with_builtin, is_sidecar_healthy, shutdown_sidecar_gracefully, force_shutdown_sidecar};
pub use commands::{
    __cmd__builtin_ai_cancel_download, __cmd__builtin_ai_delete_model,
    __cmd__builtin_ai_download_model, __cmd__builtin_ai_get_available_summary_model,
    __cmd__builtin_ai_get_model_info, __cmd__builtin_ai_get_recommended_model, __cmd__builtin_ai_is_model_ready,
    __cmd__builtin_ai_list_models, builtin_ai_cancel_download, builtin_ai_delete_model, builtin_ai_download_model,
    builtin_ai_get_available_summary_model, builtin_ai_get_model_info, builtin_ai_get_recommended_model, builtin_ai_is_model_ready,
    builtin_ai_list_models, init_model_manager, ModelManagerState,
};
pub use model_manager::{ModelInfo, ModelStatus};
pub use models::{get_available_models, get_default_model, get_model_by_name, ModelDef};
