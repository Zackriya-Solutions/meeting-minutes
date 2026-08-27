use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct DiarizationTurn {
    pub start: f64,
    pub end: f64,
    pub speaker: String,
}

#[derive(Debug, Clone)]
pub struct SpeakerAudioSegment {
    pub samples: Vec<f32>,
    pub start_seconds: f64,
    pub end_seconds: f64,
    pub speaker: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpeakerLabelUpdate {
    pub sequence_id: u64,
    pub speaker: Option<String>,
}
