//! Short DeepSeek classification used by animated transcript avatars.

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap};
use tauri::State;

use crate::llm::providers::resolve_deepseek;
use crate::llm::{ensure_outbound_allowed, Purpose};
use crate::state::AppState;

const REACTIONS: [&str; 17] = [
    "neutral",
    "excited",
    "bored",
    "suspicious",
    "angry",
    "drowsy",
    "happy",
    "curious",
    "confused",
    "surprised",
    "proud",
    "shy",
    "sad",
    "laughing",
    "scared",
    "playful",
    "celebrate",
];
const MAX_BATCH_SIZE: usize = 24;
const MAX_TEXT_CHARS: usize = 600;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReactionInput {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct ReactionResult {
    pub id: String,
    pub reaction: String,
}

#[derive(Debug, Deserialize)]
struct DeepSeekReactionEnvelope {
    reactions: Vec<ReactionResult>,
}

fn normalize_reaction(value: &str) -> &'static str {
    REACTIONS
        .into_iter()
        .find(|reaction| reaction.eq_ignore_ascii_case(value.trim()))
        .unwrap_or("neutral")
}

fn parse_reactions(raw: &str, inputs: &[ReactionInput]) -> Vec<ReactionResult> {
    let by_id = serde_json::from_str::<DeepSeekReactionEnvelope>(raw)
        .map(|envelope| {
            envelope
                .reactions
                .into_iter()
                .map(|result| (result.id, normalize_reaction(&result.reaction).to_string()))
                .collect::<HashMap<_, _>>()
        })
        .unwrap_or_default();

    inputs
        .iter()
        .map(|input| ReactionResult {
            id: input.id.clone(),
            reaction: by_id
                .get(&input.id)
                .cloned()
                .unwrap_or_else(|| "neutral".to_string()),
        })
        .collect()
}

#[tauri::command]
pub async fn classify_transcript_reactions(
    state: State<'_, AppState>,
    messages: Vec<ReactionInput>,
) -> Result<Vec<ReactionResult>, String> {
    let inputs = messages
        .into_iter()
        .filter(|message| !message.id.trim().is_empty() && !message.text.trim().is_empty())
        .take(MAX_BATCH_SIZE)
        .map(|message| ReactionInput {
            id: message.id,
            text: message.text.chars().take(MAX_TEXT_CHARS).collect(),
        })
        .collect::<Vec<_>>();

    if inputs.is_empty() {
        return Ok(Vec::new());
    }

    let pool = state.db_manager.pool();
    ensure_outbound_allowed(pool, Purpose::Extract)
        .await
        .map_err(|error| error.to_string())?;
    let client = resolve_deepseek(pool).await?;

    let system = format!(
        "Ты выбираешь визуальную реакцию персонажа на смысл каждой реплики стенограммы. \
Верни только JSON-объект вида {{\"reactions\":[{{\"id\":\"...\",\"reaction\":\"happy\"}}]}}. \
Для каждого id выбери ровно одно значение из списка: {}. \
Оценивай не только прямые слова об эмоциях, но и тон: вопрос и интерес — curious, \
непонимание — confused, шутка или ирония — playful/laughing, согласие и хороший результат — happy/proud, \
раздражение — angry, тревога и недоверие — scared/suspicious, усталость — bored/drowsy, \
неожиданность — surprised. neutral используй только для сухой фактической реплики без отношения. \
Оценивай каждую реплику независимо и не ставь всем одинаковую реакцию по умолчанию. \
Сохрани все id без изменений.",
        REACTIONS.join(", ")
    );
    let user = serde_json::to_string(&serde_json::json!({ "messages": &inputs }))
        .map_err(|error| format!("Не удалось подготовить реплики: {error}"))?;
    let raw = client.complete_json(&system, &user, 0.0).await?;

    let results = parse_reactions(&raw, &inputs);
    let distribution = results.iter().fold(BTreeMap::new(), |mut counts, result| {
        *counts.entry(result.reaction.as_str()).or_insert(0usize) += 1;
        counts
    });
    log::info!("DeepSeek transcript reactions classified: {distribution:?}");
    Ok(results)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn input(id: &str) -> ReactionInput {
        ReactionInput {
            id: id.to_string(),
            text: "тест".to_string(),
        }
    }

    #[test]
    fn invalid_or_missing_reactions_fall_back_to_neutral() {
        let inputs = vec![input("a"), input("b")];
        let parsed = parse_reactions(
            r#"{"reactions":[{"id":"a","reaction":"not-real"}]}"#,
            &inputs,
        );
        assert_eq!(
            parsed,
            vec![
                ReactionResult {
                    id: "a".into(),
                    reaction: "neutral".into(),
                },
                ReactionResult {
                    id: "b".into(),
                    reaction: "neutral".into(),
                },
            ]
        );
    }

    #[test]
    fn known_reactions_are_normalized_case_insensitively() {
        let parsed = parse_reactions(
            r#"{"reactions":[{"id":"a","reaction":" HAPPY "}]}"#,
            &[input("a")],
        );
        assert_eq!(parsed[0].reaction, "happy");
    }
}
