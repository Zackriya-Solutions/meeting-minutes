//! Markdown -> HTML -> PDF.
//!
//! `pulldown_cmark` already emits correct nested lists and GFM tables, and
//! `printpdf`'s html feature handles wrapping, pagination, tables and page
//! numbers. Going through HTML therefore avoids hand-writing a layout engine.

use std::collections::BTreeMap;
use std::sync::OnceLock;

use printpdf::html::{build_font_pool, SharedFontPool};
use printpdf::{Base64OrRaw, GeneratePdfOptions, PdfDocument, PdfSaveOptions};
use pulldown_cmark::{html, Event, Parser};
use tracing::{debug, warn};

use super::blocks::markdown_options;

/// Conservative stylesheet: `printpdf` supports a subset of CSS, so this sticks
/// to properties verified to render.
///
/// No font is embedded in the binary; the family stack falls back to whatever
/// the host provides. Turkish glyphs were verified to render through this path.
const STYLESHEET: &str = r#"
body { font-family: "Segoe UI", "Helvetica Neue", Helvetica, Arial, sans-serif; font-size: 10.5pt; color: #1f2328; }
h1 { font-size: 19pt; margin: 0 0 6px 0; }
h2 { font-size: 13.5pt; margin: 16px 0 4px 0; }
h3 { font-size: 11.5pt; margin: 12px 0 4px 0; }
h4, h5, h6 { font-size: 10.5pt; margin: 10px 0 4px 0; }
p { margin: 0 0 8px 0; }
ul, ol { margin: 0 0 8px 0; padding-left: 18px; }
li { margin: 0 0 3px 0; }
table { width: 100%; border-collapse: collapse; margin: 8px 0 12px 0; }
th { text-align: left; background-color: #f3f4f6; border: 1px solid #d0d7de; padding: 5px 6px; font-size: 9.5pt; }
td { border: 1px solid #d0d7de; padding: 5px 6px; font-size: 9.5pt; }
code { font-family: Consolas, monospace; font-size: 9.5pt; }
pre { background-color: #f6f8fa; padding: 8px; }
blockquote { border-left: 3px solid #d0d7de; padding-left: 10px; color: #57606a; }
hr { border: none; border-top: 1px solid #d0d7de; margin: 12px 0; }
"#;

/// Building the pool scans system fonts, which measured ~7 s on Windows.
/// Cached for the process lifetime and reused by every render.
static FONT_POOL: OnceLock<SharedFontPool> = OnceLock::new();

fn font_pool() -> SharedFontPool {
    FONT_POOL
        .get_or_init(|| {
            debug!("Building PDF font pool (scans system fonts, one-off)");
            build_font_pool(&BTreeMap::new(), None)
        })
        .clone()
}

/// Builds the font pool ahead of the first export so the user does not wait for
/// the system font scan while a save dialog is open.
pub fn warm_font_pool() {
    let _ = font_pool();
}

/// Renders markdown to the HTML document handed to `printpdf`.
///
/// Raw HTML embedded in the markdown is dropped: the summary text comes from an
/// LLM, and unbalanced markup would fail `printpdf`'s strict XML parser and take
/// the whole export down with it.
pub fn markdown_to_html(markdown: &str) -> String {
    let events = Parser::new_ext(markdown, markdown_options())
        .filter(|event| !matches!(event, Event::Html(_) | Event::InlineHtml(_)));

    let mut body = String::new();
    html::push_html(&mut body, events);

    format!("<html><head><style>{STYLESHEET}</style></head><body>{body}</body></html>")
}

fn pdf_options() -> GeneratePdfOptions {
    GeneratePdfOptions {
        page_width: Some(210.0),
        page_height: Some(297.0),
        margin_top: Some(18.0),
        margin_right: Some(16.0),
        margin_bottom: Some(18.0),
        margin_left: Some(16.0),
        show_page_numbers: Some(true),
        ..Default::default()
    }
}

pub fn markdown_to_pdf(markdown: &str) -> Result<Vec<u8>, String> {
    let html_doc = markdown_to_html(markdown);
    let images: BTreeMap<String, Base64OrRaw> = BTreeMap::new();
    let fonts: BTreeMap<String, Base64OrRaw> = BTreeMap::new();

    let mut warnings = Vec::new();
    let document = PdfDocument::from_html_with_cache(
        &html_doc,
        &images,
        &fonts,
        &pdf_options(),
        &mut warnings,
        Some(font_pool()),
    )
    .map_err(|e| format!("Failed to lay out PDF: {e}"))?;

    for warning in warnings.iter().take(5) {
        warn!("PDF layout warning: {:?}", warning);
    }

    let page_count = document.pages.len();
    let mut save_warnings = Vec::new();
    let bytes = document.save(&PdfSaveOptions::default(), &mut save_warnings);

    for warning in save_warnings.iter().take(5) {
        warn!("PDF save warning: {:?}", warning);
    }

    if bytes.is_empty() {
        return Err("PDF renderer produced no output".to_string());
    }

    debug!("Rendered PDF: {} pages, {} bytes", page_count, bytes.len());
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# Toplantı Özeti\n\n\
        **Tarih:** 26 Temmuz 2026\n\n\
        ## Özet\n\nAyşe Çığır ölçüm altyapısını İstanbul ekibiyle gözden geçirdi.\n\n\
        ## Aksiyon Maddeleri\n\n\
        | Sorumlu | Görev | Termin |\n| --- | --- | --- |\n| Ayşe | Panoyu hazırla | 2 Ağustos |\n\n\
        ## Kararlar\n\n- Ölçüm altyapısı önce gelecek.\n- Yeni işe alım ertelendi.\n";

    #[test]
    fn html_contains_structure_from_markdown() {
        let html_doc = markdown_to_html(SAMPLE);
        assert!(html_doc.contains("<h1>"), "missing h1: {html_doc}");
        assert!(html_doc.contains("<table>"), "missing table");
        assert!(html_doc.contains("<th>"), "missing table header cell");
        assert!(html_doc.contains("<ul>"), "missing list");
        assert!(html_doc.contains("<strong>"), "missing bold");
        assert!(html_doc.contains("Ayşe Çığır"), "non-ascii text lost");
    }

    #[test]
    fn html_drops_raw_markup() {
        let html_doc = markdown_to_html("before\n\n<div class=\"x\">raw</div>\n\nafter");
        assert!(!html_doc.contains("<div"), "raw block html leaked: {html_doc}");
        assert!(html_doc.contains("before") && html_doc.contains("after"));

        let inline = markdown_to_html("text with <span>inline</span> markup");
        assert!(!inline.contains("<span"), "raw inline html leaked: {inline}");
    }

    #[test]
    fn html_escapes_special_characters() {
        let html_doc = markdown_to_html("A & B are < C");
        assert!(html_doc.contains("&amp;"), "ampersand not escaped: {html_doc}");
        assert!(html_doc.contains("&lt;"), "angle bracket not escaped");
    }

    #[test]
    fn renders_a_valid_pdf() {
        let bytes = markdown_to_pdf(SAMPLE).expect("render should succeed");
        assert!(bytes.starts_with(b"%PDF-"), "not a PDF");
        assert!(bytes.len() > 1000, "suspiciously small PDF: {} bytes", bytes.len());
    }

    #[test]
    fn renders_empty_markdown_without_failing() {
        let bytes = markdown_to_pdf("").expect("empty input should still produce a document");
        assert!(bytes.starts_with(b"%PDF-"));
    }

    #[test]
    fn longer_documents_produce_more_output() {
        let mut long_source = String::from("# Uzun Belge\n\n");
        for i in 0..400 {
            long_source.push_str(&format!("Satır {i}: ölçüm altyapısı gözden geçirildi.\n\n"));
        }
        let short = markdown_to_pdf("# Kısa\n\nTek satır.").expect("short render");
        let long = markdown_to_pdf(&long_source).expect("long render");
        assert!(
            long.len() > short.len(),
            "expected the long document to be larger ({} vs {})",
            long.len(),
            short.len()
        );
    }
}
