use crate::api::{TranscriptAnnotation, TranscriptAnnotationInput};
use crate::database::models::TranscriptAnnotation as TranscriptAnnotationModel;
use chrono::Utc;
use sqlx::{Error as SqlxError, Sqlite, SqlitePool, Transaction};
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
        let (saved, files) = match Self::save_in_transaction(&mut transaction, meeting_id, inputs, image_dir).await {
            Ok(result) => result,
            Err(error) => {
                let _ = transaction.rollback().await;
                return Err(error);
            }
        };
        if let Err(error) = transaction.commit().await {
            files.rollback().await;
            return Err(error);
        }
        files.commit().await;
        Ok(saved)
    }

    pub async fn save_in_transaction(
        transaction: &mut Transaction<'_, Sqlite>,
        meeting_id: &str,
        inputs: &[TranscriptAnnotationInput],
        image_dir: &Path,
    ) -> Result<(Vec<TranscriptAnnotation>, AnnotationFiles), SqlxError> {
        let mut saved = Vec::with_capacity(inputs.len());
        let staging_dir = image_dir.join(".staging").join(Uuid::new_v4().to_string());
        let mut files = AnnotationFiles::new(image_dir, &staging_dir);

        for input in inputs {
            if let Err(error) = validate_input(input) {
                files.rollback().await;
                return Err(error);
            }

            let id = input.id.clone().unwrap_or_else(|| format!("annotation-{}", Uuid::new_v4()));
            let created_at = input.created_at.clone().unwrap_or_else(|| Utc::now().to_rfc3339());
            let image_file = if let Some(data) = &input.image_data {
                let extension = extension_for_mime(input.image_mime.as_deref());
                let file_name = format!("{}.{}", id, extension);
                let path = match safe_image_path(image_dir, &file_name) {
                    Ok(path) => path,
                    Err(error) => {
                        files.rollback().await;
                        return Err(error);
                    }
                };
                match fs::try_exists(&path).await {
                    Ok(true) => {
                        files.rollback().await;
                        return Err(SqlxError::Protocol("Annotation image already exists".to_string()));
                    }
                    Ok(false) => {}
                    Err(error) => {
                        files.rollback().await;
                        return Err(SqlxError::Io(error));
                    }
                }
                if let Err(error) = fs::create_dir_all(&staging_dir).await {
                    files.rollback().await;
                    return Err(SqlxError::Io(error));
                }
                let staged_path = staging_dir.join(&file_name);
                if let Err(error) = fs::write(&staged_path, data).await {
                    files.rollback().await;
                    return Err(SqlxError::Io(error));
                }
                files.add(staged_path, path.clone());
                Some(file_name)
            } else {
                input.image_file.clone()
            };

            if let Err(error) = sqlx::query(
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
            .execute(&mut **transaction)
            .await
            {
                files.rollback().await;
                return Err(error);
            }

            saved.push(TranscriptAnnotation {
                id,
                annotation_type: input.annotation_type.clone(),
                anchor_time: input.anchor_time,
                created_at,
                text: input.text.clone(),
                image_file,
            });
        }

        if let Err(error) = files.install().await {
            files.rollback().await;
            return Err(error);
        }
        Ok((saved, files))
    }

    pub async fn remove_directory(image_dir: &Path) -> Result<(), SqlxError> {
        match fs::remove_dir_all(image_dir).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(SqlxError::Io(error)),
        }
    }

    pub async fn delete(
        pool: &SqlitePool,
        annotation_id: &str,
        image_dir: &Path,
    ) -> Result<bool, SqlxError> {
        let image_file: Option<String> = sqlx::query_scalar(
            "SELECT image_file FROM transcript_annotations WHERE id = ?",
        )
        .bind(annotation_id)
        .fetch_optional(pool)
        .await?;
        let image_path = image_file
            .as_deref()
            .map(|file| safe_image_path(image_dir, file))
            .transpose()?;
        let result = sqlx::query("DELETE FROM transcript_annotations WHERE id = ?")
            .bind(annotation_id)
            .execute(pool)
            .await?;
        if result.rows_affected() > 0 {
            if let Some(path) = image_path {
                match fs::remove_file(path).await {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(SqlxError::Io(error)),
                }
            }
        }
        Ok(result.rows_affected() > 0)
    }
}

pub struct AnnotationFiles {
    image_dir: PathBuf,
    staging_dir: PathBuf,
    installed_files: Vec<PathBuf>,
    staged_files: Vec<PathBuf>,
}

impl AnnotationFiles {
    fn new(image_dir: &Path, staging_dir: &Path) -> Self {
        Self {
            image_dir: image_dir.to_path_buf(),
            staging_dir: staging_dir.to_path_buf(),
            installed_files: Vec::new(),
            staged_files: Vec::new(),
        }
    }

    fn add(&mut self, staged_path: PathBuf, final_path: PathBuf) {
        self.staged_files.push(staged_path);
        self.installed_files.push(final_path);
    }

    async fn install(&mut self) -> Result<(), SqlxError> {
        if self.staged_files.is_empty() {
            return Ok(());
        }
        let image_dir = self.installed_files.first().and_then(|path| path.parent())
            .ok_or_else(|| SqlxError::Protocol("Invalid annotation image directory".to_string()))?;
        fs::create_dir_all(image_dir).await.map_err(SqlxError::Io)?;
        for (staged, final_path) in self.staged_files.iter().zip(&self.installed_files) {
            fs::rename(staged, final_path).await.map_err(SqlxError::Io)?;
        }
        Ok(())
    }

    pub(crate) async fn commit(self) {
        let staging_parent = self.staging_dir.parent().map(PathBuf::from);
        let _ = fs::remove_dir_all(self.staging_dir).await;
        if let Some(staging_parent) = staging_parent {
            let _ = fs::remove_dir(staging_parent).await;
        }
        let _ = fs::remove_dir(self.image_dir).await;
    }

    pub(crate) async fn rollback(&self) {
        for path in &self.installed_files {
            let _ = fs::remove_file(path).await;
        }
        for path in &self.staged_files {
            let _ = fs::remove_file(path).await;
        }
        let staging_parent = self.staging_dir.parent().map(PathBuf::from);
        let _ = fs::remove_dir_all(&self.staging_dir).await;
        if let Some(staging_parent) = staging_parent {
            let _ = fs::remove_dir(staging_parent).await;
        }
        let _ = fs::remove_dir(&self.image_dir).await;
    }
}

pub fn annotation_directory(
    app_data_dir: &Path,
    meeting_id: &str,
    folder_path: Option<&str>,
) -> Result<PathBuf, SqlxError> {
    if let Some(folder_path) = folder_path {
        return Ok(PathBuf::from(folder_path).join("annotations"));
    }
    validate_meeting_id(meeting_id)?;
    Ok(app_data_dir.join("meetings").join(meeting_id).join("annotations"))
}

fn validate_meeting_id(meeting_id: &str) -> Result<(), SqlxError> {
    let path = Path::new(meeting_id);
    if meeting_id.trim().is_empty()
        || meeting_id.contains('/')
        || meeting_id.contains('\\')
        || meeting_id == "."
        || meeting_id == ".."
        || path.components().count() != 1
        || !matches!(path.components().next(), Some(std::path::Component::Normal(_)))
    {
        return Err(SqlxError::Protocol("Invalid meeting ID for annotation path".to_string()));
    }
    Ok(())
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
    if let Some(image_file) = &input.image_file {
        safe_image_path(Path::new("."), image_file)?;
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

#[cfg(test)]
mod tests {
    use super::*;
    use sqlx::sqlite::SqlitePoolOptions;
    use tempfile::tempdir;

    async fn pool_with_schema() -> SqlitePool {
        let pool = SqlitePoolOptions::new().max_connections(1).connect("sqlite::memory:").await.unwrap();
        sqlx::migrate!("./migrations").run(&pool).await.unwrap();
        sqlx::query("INSERT INTO meetings (id, title, created_at, updated_at) VALUES (?, ?, ?, ?)")
            .bind("meeting-test")
            .bind("Test meeting")
            .bind("2026-08-31T10:00:00Z")
            .bind("2026-08-31T10:00:00Z")
            .execute(&pool)
            .await
            .unwrap();
        pool
    }

    fn image_input(id: &str) -> TranscriptAnnotationInput {
        TranscriptAnnotationInput {
            id: Some(id.to_string()),
            annotation_type: "image".to_string(),
            anchor_time: 2.0,
            created_at: Some("2026-08-31T10:00:01Z".to_string()),
            text: None,
            image_file: None,
            image_data: Some(vec![1, 2, 3]),
            image_mime: Some("image/png".to_string()),
        }
    }

    #[tokio::test]
    async fn save_and_list_persist_image_in_the_resolved_directory() {
        let pool = pool_with_schema().await;
        let root = tempdir().unwrap();
        let directory = annotation_directory(root.path(), "meeting-test", None).unwrap();
        let saved = AnnotationsRepository::save(&pool, "meeting-test", &[image_input("annotation-image")], &directory).await.unwrap();
        assert_eq!(saved[0].image_file.as_deref(), Some("annotation-image.png"));
        assert_eq!(fs::read(directory.join("annotation-image.png")).await.unwrap(), vec![1, 2, 3]);
        assert_eq!(AnnotationsRepository::list(&pool, "meeting-test").await.unwrap().len(), 1);
        assert!(AnnotationsRepository::delete(&pool, "annotation-image", &directory).await.unwrap());
        assert!(!directory.join("annotation-image.png").exists());
    }

    #[tokio::test]
    async fn duplicate_annotation_id_leaves_existing_row_and_file_untouched() {
        let pool = pool_with_schema().await;
        let root = tempdir().unwrap();
        let directory = annotation_directory(root.path(), "meeting-test", None).unwrap();
        AnnotationsRepository::save(&pool, "meeting-test", &[image_input("annotation-image")], &directory).await.unwrap();
        assert!(AnnotationsRepository::save(&pool, "meeting-test", &[image_input("annotation-image")], &directory).await.is_err());
        assert_eq!(AnnotationsRepository::list(&pool, "meeting-test").await.unwrap().len(), 1);
        assert_eq!(fs::read(directory.join("annotation-image.png")).await.unwrap(), vec![1, 2, 3]);
    }

    #[test]
    fn directory_helper_is_symmetric_and_rejects_traversal() {
        let app_data = tempdir().unwrap();
        let custom_folder = tempdir().unwrap();
        assert_eq!(annotation_directory(app_data.path(), "meeting-test", None).unwrap(), app_data.path().join("meetings/meeting-test/annotations"));
        assert_eq!(annotation_directory(app_data.path(), "meeting-test", custom_folder.path().to_str()).unwrap(), custom_folder.path().join("annotations"));
        assert!(annotation_directory(app_data.path(), "../../outside", None).is_err());
    }

    #[tokio::test]
    async fn removing_annotation_directory_removes_images_without_removing_root() {
        let root = tempdir().unwrap();
        let directory = annotation_directory(root.path(), "meeting-test", None).unwrap();
        fs::create_dir_all(&directory).await.unwrap();
        fs::write(directory.join("image.png"), [1, 2, 3]).await.unwrap();
        AnnotationsRepository::remove_directory(&directory).await.unwrap();
        assert!(!directory.exists());
        assert!(root.path().exists());
    }
}
