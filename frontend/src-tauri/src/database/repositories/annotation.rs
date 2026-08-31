use crate::api::{TranscriptAnnotation, TranscriptAnnotationInput};
use crate::database::models::TranscriptAnnotation as TranscriptAnnotationModel;
use chrono::Utc;
use sqlx::{Error as SqlxError, SqlitePool};
use std::path::{Path, PathBuf};
use tokio::fs;
use uuid::Uuid;

pub struct AnnotationsRepository;

impl AnnotationsRepository {
    pub async fn list(pool: &SqlitePool, meeting_id: &str) -> Result<Vec<TranscriptAnnotation>, SqlxError> {
        let rows = sqlx::query_as::<_, TranscriptAnnotationModel>(
            "SELECT id, meeting_id, annotation_type, anchor_time, created_at, text, image_file
             FROM transcript_annotations WHERE meeting_id = ?
             ORDER BY anchor_time ASC, created_at ASC",
        )
        .bind(meeting_id)
        .fetch_all(pool)
        .await?;

        Ok(rows.into_iter().map(Into::into).collect())
    }

    pub async fn add(
        pool: &SqlitePool,
        meeting_id: &str,
        input: &TranscriptAnnotationInput,
        image_dir: &Path,
    ) -> Result<TranscriptAnnotation, SqlxError> {
        let mut annotations = Self::save(pool, meeting_id, std::slice::from_ref(input), image_dir).await?;
        Ok(annotations.remove(0))
    }

    pub async fn save(
        pool: &SqlitePool,
        meeting_id: &str,
        inputs: &[TranscriptAnnotationInput],
        image_dir: &Path,
    ) -> Result<Vec<TranscriptAnnotation>, SqlxError> {
        let mut transaction = pool.begin().await?;
        let mut saved = Vec::with_capacity(inputs.len());

        for input in inputs {
            validate_input(input)?;
            let id = input.id.clone().unwrap_or_else(|| format!("annotation-{}", Uuid::new_v4()));
            let created_at = input.created_at.clone().unwrap_or_else(|| Utc::now().to_rfc3339());
            let image_file = if let Some(data) = &input.image_data {
                fs::create_dir_all(image_dir).await.map_err(SqlxError::Io)?;
                let extension = extension_for_mime(input.image_mime.as_deref());
                let file_name = format!("{}.{}", id, extension);
                let path = safe_image_path(image_dir, &file_name)?;
                fs::write(path, data).await.map_err(SqlxError::Io)?;
                Some(file_name)
            } else {
                input.image_file.clone()
            };

            sqlx::query(
                "INSERT INTO transcript_annotations
                 (id, meeting_id, annotation_type, anchor_time, created_at, text, image_file)
                 VALUES (?, ?, ?, ?, ?, ?, ?)",
            )
            .bind(&id)
            .bind(meeting_id)
            .bind(&input.annotation_type)
            .bind(input.anchor_time)
            .bind(&created_at)
            .bind(&input.text)
            .bind(&image_file)
            .execute(&mut *transaction)
            .await?;

            saved.push(TranscriptAnnotation {
                id,
                annotation_type: input.annotation_type.clone(),
                anchor_time: input.anchor_time,
                created_at,
                text: input.text.clone(),
                image_file,
            });
        }

        transaction.commit().await?;
        Ok(saved)
    }

    pub async fn delete(pool: &SqlitePool, annotation_id: &str) -> Result<bool, SqlxError> {
        let result = sqlx::query("DELETE FROM transcript_annotations WHERE id = ?")
            .bind(annotation_id)
            .execute(pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

fn validate_input(input: &TranscriptAnnotationInput) -> Result<(), SqlxError> {
    if !matches!(input.annotation_type.as_str(), "bookmark" | "note" | "image") {
        return Err(SqlxError::Protocol("Invalid annotation type".to_string()));
    }
    if !input.anchor_time.is_finite() || input.anchor_time < 0.0 {
        return Err(SqlxError::Protocol("Invalid annotation anchor time".to_string()));
    }
    if input.annotation_type == "note" && input.text.as_deref().unwrap_or("").trim().is_empty() {
        return Err(SqlxError::Protocol("Note text cannot be empty".to_string()));
    }
    if input.annotation_type == "image" && input.image_data.is_none() && input.image_file.is_none() {
        return Err(SqlxError::Protocol("Image data is required".to_string()));
    }
    Ok(())
}

fn extension_for_mime(mime: Option<&str>) -> &'static str {
    match mime {
        Some("image/jpeg") => "jpg",
        Some("image/webp") => "webp",
        Some("image/gif") => "gif",
        _ => "png",
    }
}

fn safe_image_path(dir: &Path, file_name: &str) -> Result<PathBuf, SqlxError> {
    if file_name.contains('/') || file_name.contains('\\') || file_name == "." || file_name == ".." {
        return Err(SqlxError::Protocol("Invalid annotation image filename".to_string()));
    }
    Ok(dir.join(file_name))
}

impl From<TranscriptAnnotationModel> for TranscriptAnnotation {
    fn from(model: TranscriptAnnotationModel) -> Self {
        Self {
            id: model.id,
            annotation_type: model.annotation_type,
            anchor_time: model.anchor_time,
            created_at: model.created_at,
            text: model.text,
            image_file: model.image_file,
        }
    }
}
