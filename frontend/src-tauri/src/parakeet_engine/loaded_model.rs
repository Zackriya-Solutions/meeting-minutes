use super::model::{ParakeetError, ParakeetModel, TimestampedResult};
use std::path::Path;

pub enum LoadedParakeetModel {
    Tdt(ParakeetModel),
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    CoreMlCtc(super::coreml_ctc::CoreMlCtcModel),
}

impl LoadedParakeetModel {
    pub fn load_tdt(model_dir: &Path, quantized: bool) -> Result<Self, ParakeetError> {
        ParakeetModel::new(model_dir, quantized).map(Self::Tdt)
    }

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    pub fn load_coreml_ctc(model_dir: &Path) -> Result<Self, ParakeetError> {
        super::coreml_ctc::CoreMlCtcModel::new(model_dir)
            .map(Self::CoreMlCtc)
            .map_err(|error| ParakeetError::Other(error.to_string()))
    }

    pub fn transcribe_samples(
        &mut self,
        samples: Vec<f32>,
    ) -> Result<TimestampedResult, ParakeetError> {
        match self {
            Self::Tdt(model) => model.transcribe_samples(samples),
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            Self::CoreMlCtc(model) => model
                .transcribe_samples(samples)
                .map_err(|error| ParakeetError::Other(error.to_string())),
        }
    }

    pub fn max_segment_samples(&self) -> usize {
        match self {
            Self::Tdt(_) => 25 * 16_000,
            #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
            Self::CoreMlCtc(_) => super::ctc::PARAKEET_CTC_ZH_CN_MAX_SAMPLES,
        }
    }
}
