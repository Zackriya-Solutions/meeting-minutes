/// Errors from folder, tag, and trash operations. `Display` text is shown to the user.
#[derive(Debug, thiserror::Error)]
pub enum OrgError {
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Duplicate(String),
    #[error("{0}")]
    Invalid(String),
    #[error("Database error: {0}")]
    Db(#[from] sqlx::Error),
}

/// Commands return the display text, so the frontend sees a plain message string.
impl serde::Serialize for OrgError {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

const MAX_NAME_CHARS: usize = 100;

/// Trim a folder/tag name and reject empty or overlong names.
pub fn clean_name(name: &str) -> Result<String, OrgError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(OrgError::Invalid("Name cannot be empty".to_string()));
    }
    if name.chars().count() > MAX_NAME_CHARS {
        return Err(OrgError::Invalid(format!(
            "Name is too long (max {} characters)",
            MAX_NAME_CHARS
        )));
    }
    Ok(name.to_string())
}

pub fn is_unique_violation(e: &sqlx::Error) -> bool {
    matches!(e, sqlx::Error::Database(db) if db.is_unique_violation())
}

/// Map a unique-constraint violation to a user-facing duplicate-name error, e.g. noun "folder".
pub fn duplicate_or(e: sqlx::Error, noun: &str, name: &str) -> OrgError {
    if is_unique_violation(&e) {
        OrgError::Duplicate(format!("A {} named \"{}\" already exists", noun, name))
    } else {
        e.into()
    }
}
