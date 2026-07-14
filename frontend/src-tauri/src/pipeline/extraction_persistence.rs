//! Atomic, idempotent persistence for entity/action extraction results.

use super::extraction::{normalize_name, resolve_entity, Extraction, KnownEntity, Resolution};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PersistenceStats {
    pub entities_created: usize,
    pub mentions_created: usize,
    pub pending_merges_created: usize,
    pub action_items_created: usize,
}

pub async fn persist_extraction(
    pool: &sqlx::SqlitePool,
    meeting_id: &str,
    extraction: &Extraction,
) -> anyhow::Result<PersistenceStats> {
    let mut tx = pool.begin().await?;
    let rows: Vec<(i64, String, String, String)> =
        sqlx::query_as("SELECT id, type, canonical_name, aliases FROM entities ORDER BY id")
            .fetch_all(&mut *tx)
            .await?;
    let mut known: Vec<KnownEntity> = rows
        .into_iter()
        .map(|(id, entity_type, canonical_name, aliases_json)| {
            let aliases: Vec<String> = serde_json::from_str(&aliases_json).unwrap_or_default();
            let mut normalized_names = vec![normalize_name(&canonical_name)];
            normalized_names.extend(aliases.iter().map(|alias| normalize_name(alias)));
            normalized_names.sort();
            normalized_names.dedup();
            KnownEntity {
                id,
                entity_type,
                normalized_names,
            }
        })
        .collect();
    let mut stats = PersistenceStats::default();

    for entity in &extraction.entities {
        let entity_id = match resolve_entity(&entity.name, &entity.entity_type, &known) {
            Resolution::Exact(id) | Resolution::Merge(id) => Some(id),
            Resolution::Review { entity_id, score } => {
                let result = sqlx::query(
                    "INSERT INTO pending_merges \
                     (entity_id, incoming_name, incoming_type, incoming_aliases, score, meeting_id) \
                     SELECT ?, ?, ?, ?, ?, ? WHERE NOT EXISTS (SELECT 1 FROM pending_merges \
                       WHERE entity_id = ? AND meeting_id = ? AND status = 'pending' \
                         AND lower(trim(incoming_name)) = lower(trim(?)))",
                )
                .bind(entity_id)
                .bind(entity.name.trim())
                .bind(&entity.entity_type)
                .bind(serde_json::to_string(&entity.aliases)?)
                .bind(score)
                .bind(meeting_id)
                .bind(entity_id)
                .bind(meeting_id)
                .bind(entity.name.trim())
                .execute(&mut *tx).await?;
                stats.pending_merges_created += result.rows_affected() as usize;
                None
            }
            Resolution::New => {
                let id: i64 = sqlx::query_scalar(
                    "INSERT INTO entities(type, canonical_name, aliases) VALUES(?, ?, ?) RETURNING id",
                )
                .bind(&entity.entity_type)
                .bind(entity.name.trim())
                .bind(serde_json::to_string(&entity.aliases)?)
                .fetch_one(&mut *tx).await?;
                let mut normalized_names = vec![normalize_name(&entity.name)];
                normalized_names.extend(entity.aliases.iter().map(|alias| normalize_name(alias)));
                normalized_names.sort();
                normalized_names.dedup();
                known.push(KnownEntity {
                    id,
                    entity_type: entity.entity_type.clone(),
                    normalized_names,
                });
                stats.entities_created += 1;
                Some(id)
            }
        };

        if let Some(entity_id) = entity_id {
            let chunk_id: Option<i64> =
                match entity.quote.as_deref().filter(|q| !q.trim().is_empty()) {
                    Some(quote) => {
                        sqlx::query_scalar(
                            "SELECT id FROM chunks WHERE meeting_id = ? \
                     AND instr(lower(text), lower(?)) > 0 ORDER BY start_ms LIMIT 1",
                        )
                        .bind(meeting_id)
                        .bind(quote.trim())
                        .fetch_optional(&mut *tx)
                        .await?
                    }
                    None => None,
                };
            let result = sqlx::query(
                "INSERT INTO entity_mentions(entity_id, meeting_id, chunk_id, quote) \
                 SELECT ?, ?, ?, ? WHERE NOT EXISTS (SELECT 1 FROM entity_mentions \
                   WHERE entity_id = ? AND meeting_id = ? \
                     AND COALESCE(quote, '') = COALESCE(?, ''))",
            )
            .bind(entity_id)
            .bind(meeting_id)
            .bind(chunk_id)
            .bind(entity.quote.as_deref())
            .bind(entity_id)
            .bind(meeting_id)
            .bind(entity.quote.as_deref())
            .execute(&mut *tx)
            .await?;
            stats.mentions_created += result.rows_affected() as usize;
        }
    }

    for item in &extraction.action_items {
        let owner_speaker_id: Option<i64> = match item
            .owner
            .as_deref()
            .filter(|s| !s.trim().is_empty())
        {
            Some(owner) => sqlx::query_scalar(
                "SELECT id FROM speakers WHERE lower(trim(display_name)) = lower(trim(?)) LIMIT 1",
            )
            .bind(owner)
            .fetch_optional(&mut *tx)
            .await?,
            None => None,
        };
        let source_start_ms: Option<i64> =
            match item.quote.as_deref().filter(|q| !q.trim().is_empty()) {
                Some(quote) => {
                    sqlx::query_scalar(
                        "SELECT start_ms FROM chunks WHERE meeting_id = ? \
                 AND instr(lower(text), lower(?)) > 0 ORDER BY start_ms LIMIT 1",
                    )
                    .bind(meeting_id)
                    .bind(quote.trim())
                    .fetch_optional(&mut *tx)
                    .await?
                }
                None => None,
            };
        let result = sqlx::query(
            "INSERT INTO action_items \
             (meeting_id, text, owner_speaker_id, owner_name_raw, due_date, source_quote, source_start_ms) \
             SELECT ?, ?, ?, ?, ?, ?, ? WHERE NOT EXISTS (SELECT 1 FROM action_items \
               WHERE meeting_id = ? AND lower(trim(text)) = lower(trim(?)) \
                 AND status != 'superseded')",
        )
        .bind(meeting_id).bind(item.text.trim()).bind(owner_speaker_id)
        .bind(item.owner.as_deref()).bind(item.due_date.as_deref())
        .bind(item.quote.as_deref()).bind(source_start_ms)
        .bind(meeting_id).bind(item.text.trim())
        .execute(&mut *tx).await?;
        stats.action_items_created += result.rows_affected() as usize;
    }

    tx.commit().await?;
    Ok(stats)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pipeline::extraction::{ExtractedAction, ExtractedEntity};

    #[tokio::test]
    async fn persistence_is_idempotent_and_keeps_provenance() {
        let pool = sqlx::sqlite::SqlitePoolOptions::new()
            .max_connections(1)
            .connect("sqlite::memory:")
            .await
            .unwrap();
        for ddl in [
            "CREATE TABLE chunks(id INTEGER PRIMARY KEY, meeting_id TEXT, start_ms INTEGER, text TEXT)",
            "CREATE TABLE speakers(id INTEGER PRIMARY KEY, display_name TEXT)",
            "CREATE TABLE entities(id INTEGER PRIMARY KEY, type TEXT, canonical_name TEXT, aliases TEXT DEFAULT '[]', UNIQUE(type, canonical_name))",
            "CREATE TABLE entity_mentions(id INTEGER PRIMARY KEY, entity_id INTEGER, meeting_id TEXT, chunk_id INTEGER, quote TEXT)",
            "CREATE TABLE pending_merges(id INTEGER PRIMARY KEY, entity_id INTEGER, incoming_name TEXT, incoming_type TEXT, incoming_aliases TEXT, score REAL, meeting_id TEXT, status TEXT DEFAULT 'pending')",
            "CREATE TABLE action_items(id INTEGER PRIMARY KEY, meeting_id TEXT, text TEXT, owner_speaker_id INTEGER, owner_name_raw TEXT, due_date TEXT, status TEXT DEFAULT 'open', source_quote TEXT, source_start_ms INTEGER)",
        ] {
            sqlx::query(ddl).execute(&pool).await.unwrap();
        }
        sqlx::query("INSERT INTO chunks VALUES(1,'m1',1200,'Иван подготовит отчёт')")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO speakers VALUES(7,'Иван')")
            .execute(&pool)
            .await
            .unwrap();
        let extraction = Extraction {
            entities: vec![ExtractedEntity {
                entity_type: "person".into(),
                name: "Иван".into(),
                aliases: vec![],
                quote: Some("Иван подготовит отчёт".into()),
            }],
            action_items: vec![ExtractedAction {
                text: "Подготовить отчёт".into(),
                owner: Some("Иван".into()),
                due_date: None,
                quote: Some("Иван подготовит отчёт".into()),
                approx_position: None,
            }],
        };
        let first = persist_extraction(&pool, "m1", &extraction).await.unwrap();
        assert_eq!(
            (
                first.entities_created,
                first.mentions_created,
                first.action_items_created
            ),
            (1, 1, 1)
        );
        assert_eq!(
            persist_extraction(&pool, "m1", &extraction).await.unwrap(),
            PersistenceStats::default()
        );
        let start_ms: i64 = sqlx::query_scalar("SELECT source_start_ms FROM action_items")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(start_ms, 1200);
    }
}
