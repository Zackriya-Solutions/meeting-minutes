//! Tauri commands for document export.

use tauri::{AppHandle, Runtime};
use tauri_plugin_dialog::DialogExt;
use tracing::{info, warn};

use super::{docx, pdf};

/// Characters Windows forbids in a filename, plus the path separators.
const ILLEGAL_FILENAME_CHARS: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
const MAX_FILENAME_LEN: usize = 120;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Markdown,
    Pdf,
    Docx,
}

impl ExportFormat {
    fn parse(value: &str) -> Result<Self, String> {
        match value.to_ascii_lowercase().as_str() {
            "md" | "markdown" => Ok(Self::Markdown),
            "pdf" => Ok(Self::Pdf),
            "docx" | "word" => Ok(Self::Docx),
            other => Err(format!(
                "Unsupported export format '{other}'. Expected one of: md, pdf, docx"
            )),
        }
    }

    fn extension(self) -> &'static str {
        match self {
            Self::Markdown => "md",
            Self::Pdf => "pdf",
            Self::Docx => "docx",
        }
    }

    fn filter_name(self) -> &'static str {
        match self {
            Self::Markdown => "Markdown",
            Self::Pdf => "PDF Document",
            Self::Docx => "Word Document",
        }
    }

    fn render(self, markdown: &str) -> Result<Vec<u8>, String> {
        match self {
            Self::Markdown => Ok(markdown.as_bytes().to_vec()),
            Self::Pdf => pdf::markdown_to_pdf(markdown),
            Self::Docx => docx::markdown_to_docx(markdown),
        }
    }
}

/// Turns a meeting title into something every platform will accept as a
/// filename. Never returns an empty string.
pub fn sanitize_file_name(raw: &str) -> String {
    let mut cleaned: String = raw
        .chars()
        .map(|c| {
            if ILLEGAL_FILENAME_CHARS.contains(&c) || c.is_control() {
                '-'
            } else {
                c
            }
        })
        .collect();

    cleaned = cleaned.trim().trim_matches('.').trim().to_string();

    // Collapse runs of separators introduced by the replacement above.
    while cleaned.contains("--") {
        cleaned = cleaned.replace("--", "-");
    }

    // A name made only of replaced characters would otherwise become "-".
    cleaned = cleaned.trim_matches('-').trim().to_string();

    if cleaned.chars().count() > MAX_FILENAME_LEN {
        cleaned = cleaned.chars().take(MAX_FILENAME_LEN).collect();
        cleaned = cleaned.trim().trim_matches('-').to_string();
    }

    if cleaned.is_empty() {
        "meeting".to_string()
    } else {
        cleaned
    }
}

/// Exports a prepared markdown document.
///
/// The frontend assembles the markdown so that all three formats share one
/// source. Returns the written path, or `None` when the user cancels the dialog.
#[tauri::command]
pub async fn api_export_document<R: Runtime>(
    app: AppHandle<R>,
    markdown: String,
    format: String,
    suggested_name: String,
) -> Result<Option<String>, String> {
    let format = ExportFormat::parse(&format)?;

    if markdown.trim().is_empty() {
        return Err("Nothing to export: the document is empty".to_string());
    }

    let file_name = format!(
        "{}.{}",
        sanitize_file_name(&suggested_name),
        format.extension()
    );
    info!("Export requested: format={:?}, file={}", format, file_name);

    let chosen = app
        .dialog()
        .file()
        .set_file_name(&file_name)
        .add_filter(format.filter_name(), &[format.extension()])
        .blocking_save_file();

    let Some(chosen) = chosen else {
        info!("Export cancelled by user");
        return Ok(None);
    };

    let path = chosen
        .into_path()
        .map_err(|e| format!("Could not resolve the selected path: {e}"))?;

    let bytes = format.render(&markdown)?;

    std::fs::write(&path, &bytes).map_err(|e| {
        warn!("Failed to write export to {:?}: {}", path, e);
        format!("Failed to write {}: {}", path.display(), e)
    })?;

    info!("Exported {} bytes to {:?}", bytes.len(), path);
    Ok(Some(path.to_string_lossy().to_string()))
}

/// Builds the PDF font pool ahead of time.
///
/// The first PDF render scans system fonts (~7 s). The frontend calls this when
/// the export menu mounts so that cost is paid before the user asks for a file.
#[tauri::command]
pub async fn api_warm_export_engine() -> Result<(), String> {
    std::thread::spawn(pdf::warm_font_pool);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_known_formats() {
        assert_eq!(ExportFormat::parse("md").unwrap(), ExportFormat::Markdown);
        assert_eq!(
            ExportFormat::parse("markdown").unwrap(),
            ExportFormat::Markdown
        );
        assert_eq!(ExportFormat::parse("PDF").unwrap(), ExportFormat::Pdf);
        assert_eq!(ExportFormat::parse("docx").unwrap(), ExportFormat::Docx);
        assert_eq!(ExportFormat::parse("Word").unwrap(), ExportFormat::Docx);
    }

    #[test]
    fn rejects_unknown_formats() {
        let error = ExportFormat::parse("rtf").unwrap_err();
        assert!(
            error.contains("rtf"),
            "error should name the bad value: {error}"
        );
    }

    #[test]
    fn markdown_format_writes_the_source_verbatim() {
        let source = "# Başlık\n\nGövde.\n";
        let bytes = ExportFormat::Markdown.render(source).unwrap();
        assert_eq!(String::from_utf8(bytes).unwrap(), source);
    }

    #[test]
    fn strips_characters_windows_rejects() {
        assert_eq!(sanitize_file_name("Q3 / Q4: plan?"), "Q3 - Q4- plan");
        assert_eq!(sanitize_file_name("a\\b|c*d"), "a-b-c-d");
    }

    #[test]
    fn keeps_non_ascii_titles() {
        assert_eq!(
            sanitize_file_name("Sprint Planlama Toplantısı"),
            "Sprint Planlama Toplantısı"
        );
    }

    #[test]
    fn never_returns_an_empty_name() {
        assert_eq!(sanitize_file_name(""), "meeting");
        assert_eq!(sanitize_file_name("   "), "meeting");
        assert_eq!(sanitize_file_name("..."), "meeting");
        assert_eq!(sanitize_file_name("///"), "meeting");
    }

    #[test]
    fn truncates_very_long_names() {
        let name = sanitize_file_name(&"ö".repeat(400));
        assert_eq!(name.chars().count(), MAX_FILENAME_LEN);
    }
}
