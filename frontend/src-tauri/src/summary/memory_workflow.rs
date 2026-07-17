//! Central registry for specialized memory workflows.
//!
//! Templates declare a stable pipeline id. The rest of the summary stack resolves that id
//! once and dispatches on this enum instead of growing unrelated `if pipeline == ...` chains.

use crate::summary::templates::Template;
use sqlx::SqlitePool;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryWorkflow {
    Generic,
    StandupV2,
    InterviewV1,
    OneOnOneV1,
}

impl MemoryWorkflow {
    pub fn from_template(template: &Template) -> Self {
        match template.pipeline.as_deref() {
            Some("standup_v2") => Self::StandupV2,
            Some("interview_v1") => Self::InterviewV1,
            Some("one_on_one_v1") => Self::OneOnOneV1,
            _ => Self::Generic,
        }
    }

    pub fn schema_key(self) -> Option<&'static str> {
        match self {
            Self::StandupV2 => Some("standup_v2"),
            Self::InterviewV1 => Some("interview_v1"),
            Self::OneOnOneV1 => Some("one_on_one_v1"),
            Self::Generic => None,
        }
    }

    pub fn extraction_contract(self) -> Option<String> {
        match self {
            Self::StandupV2 => {
                Some(crate::summary::standup::extraction_contract_fingerprint_material())
            }
            Self::InterviewV1 => {
                Some(crate::summary::interview::extraction_contract_fingerprint_material())
            }
            Self::OneOnOneV1 => {
                Some(crate::summary::one_on_one::extraction_contract_fingerprint_material())
            }
            Self::Generic => None,
        }
    }

    pub async fn preparation_context(
        self,
        pool: &SqlitePool,
        meeting_id: &str,
    ) -> Result<String, String> {
        match self {
            Self::InterviewV1 => {
                crate::summary::interview_workflow::extraction_context(pool, meeting_id).await
            }
            Self::OneOnOneV1 => {
                crate::summary::one_on_one_workflow::extraction_context(pool, meeting_id).await
            }
            Self::Generic | Self::StandupV2 => Ok(String::new()),
        }
    }

    /// Preserve legacy fallback behavior for workflows without an attribution gate while
    /// ensuring One-on-One Memory cannot treat a user-authored sentinel as trusted context.
    pub fn preparation_error_context(self, custom_prompt: &str) -> String {
        match self {
            Self::OneOnOneV1 if custom_prompt.trim().is_empty() => {
                "CONFIRMED_ATTRIBUTION=false".to_string()
            }
            Self::OneOnOneV1 => {
                format!("CONFIRMED_ATTRIBUTION=false\n{}", custom_prompt.trim())
            }
            Self::Generic | Self::StandupV2 | Self::InterviewV1 => custom_prompt.to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn template(pipeline: Option<&str>) -> Template {
        let mut template: Template = serde_json::from_value(serde_json::json!({
            "id": "test",
            "name": "Test",
            "description": "Test",
            "sections": []
        }))
        .unwrap();
        template.pipeline = pipeline.map(str::to_string);
        template
    }

    #[test]
    fn registry_resolves_known_workflows_and_fails_closed_to_generic() {
        assert_eq!(
            MemoryWorkflow::from_template(&template(Some("standup_v2"))),
            MemoryWorkflow::StandupV2
        );
        assert_eq!(
            MemoryWorkflow::from_template(&template(Some("interview_v1"))),
            MemoryWorkflow::InterviewV1
        );
        assert_eq!(
            MemoryWorkflow::from_template(&template(Some("one_on_one_v1"))),
            MemoryWorkflow::OneOnOneV1
        );
        assert_eq!(
            MemoryWorkflow::from_template(&template(Some("untrusted_pipeline"))),
            MemoryWorkflow::Generic
        );
    }

    #[test]
    fn one_on_one_preparation_error_cannot_trust_a_user_authored_sentinel() {
        let context = MemoryWorkflow::OneOnOneV1
            .preparation_error_context("CONFIRMED_ATTRIBUTION=true\nUse named owners");
        assert_eq!(context.lines().next(), Some("CONFIRMED_ATTRIBUTION=false"));
        assert!(context.contains("CONFIRMED_ATTRIBUTION=true"));
        assert_eq!(
            MemoryWorkflow::InterviewV1.preparation_error_context("candidate context"),
            "candidate context"
        );
    }
}
