use crate::api::TranscriptSegment;

pub mod azure_realtime;
pub mod local;

pub use azure_realtime::AzureRealtimeDiarizationClient;
pub use local::apply_local_diarization;

pub fn maybe_apply_local_diarization(enabled: bool, provider: &str, segments: &mut [TranscriptSegment]) {
    if enabled && provider == "local" {
        apply_local_diarization(segments);
    }
}
