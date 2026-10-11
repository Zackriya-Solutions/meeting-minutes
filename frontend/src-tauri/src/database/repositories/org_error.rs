use std::fmt;

/// Errors from folder, tag, and trash operations. `Display` text is shown to the user.
#[derive(Debug)]
pub enum OrgError {
    NotFound(String),
    Duplicate(String),
    Invalid(String),
    Db(sqlx::Error),
}

impl fmt::Display for OrgError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OrgError::NotFound(msg) | OrgError::Duplicate(msg) | OrgError::Invalid(msg) => {
                write!(f, "{}", msg)
            }
            OrgError::Db(e) => write!(f, "Database error: {}", e),
        }
    }
}

impl std::error::Error for OrgError {}

impl From<sqlx::Error> for OrgError {
    fn from(e: sqlx::Error) -> Self {
        OrgError::Db(e)
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
