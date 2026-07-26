//! Block model -> DOCX.
//!
//! `docx-rs` needs structured input, so this renders [`super::blocks::Block`]
//! rather than HTML.

use docx_rs::*;

use super::blocks::{parse_markdown, Block, Inline};

/// Numbering definition ids. Both a bullet and a decimal definition are declared
/// up front so list rendering never has to mutate the document header mid-walk.
const BULLET_ID: usize = 1;
const ORDERED_ID: usize = 2;
/// Deepest list nesting that gets its own indent level; deeper lists reuse it.
const MAX_LIST_DEPTH: usize = 3;
/// Usable text width for A4 with the default one-inch margins, in twips.
const CONTENT_WIDTH_TWIPS: usize = 9026;

pub fn markdown_to_docx(markdown: &str) -> Result<Vec<u8>, String> {
    let blocks = parse_markdown(markdown);

    let mut docx = Docx::new()
        .add_abstract_numbering(numbering_definition(BULLET_ID, true))
        .add_numbering(Numbering::new(BULLET_ID, BULLET_ID))
        .add_abstract_numbering(numbering_definition(ORDERED_ID, false))
        .add_numbering(Numbering::new(ORDERED_ID, ORDERED_ID));

    for block in &blocks {
        docx = push_block(docx, block, 0);
    }

    let mut buffer: Vec<u8> = Vec::new();
    docx.build()
        .pack(std::io::Cursor::new(&mut buffer))
        .map_err(|e| format!("Failed to write DOCX: {e}"))?;

    if buffer.is_empty() {
        return Err("DOCX writer produced no output".to_string());
    }

    Ok(buffer)
}

fn numbering_definition(id: usize, bullet: bool) -> AbstractNumbering {
    let mut numbering = AbstractNumbering::new(id);
    for depth in 0..=MAX_LIST_DEPTH {
        let (format, text) = if bullet {
            ("bullet", "•".to_string())
        } else {
            ("decimal", format!("%{}.", depth + 1))
        };
        let indent = 720 * (depth as i32 + 1);
        numbering = numbering.add_level(
            Level::new(
                depth,
                Start::new(1),
                NumberFormat::new(format),
                LevelText::new(text),
                LevelJc::new("left"),
            )
            .indent(
                Some(indent),
                Some(SpecialIndentType::Hanging(360)),
                None,
                None,
            ),
        );
    }
    numbering
}

fn push_block(docx: Docx, block: &Block, depth: usize) -> Docx {
    match block {
        Block::Heading { level, inlines } => docx.add_paragraph(heading_paragraph(*level, inlines)),
        Block::Paragraph(inlines) => {
            docx.add_paragraph(with_runs(Paragraph::new(), inlines, Style::default()))
        }
        Block::List { ordered, items } => push_list(docx, *ordered, items, depth),
        Block::Table { header, rows } => docx.add_table(build_table(header, rows)),
        Block::Code(code) => {
            let mut docx = docx;
            // Each source line becomes its own paragraph; Word has no <pre>.
            for line in code.lines() {
                docx = docx.add_paragraph(Paragraph::new().add_run(monospace_run(line)));
            }
            docx
        }
        Block::Quote(inner) => {
            let mut docx = docx;
            for child in inner {
                docx = push_block(docx, child, depth);
            }
            docx
        }
        // A horizontal rule has no direct DOCX equivalent worth the complexity;
        // an empty paragraph keeps the visual separation.
        Block::Rule => docx.add_paragraph(Paragraph::new()),
    }
}

fn push_list(docx: Docx, ordered: bool, items: &[Vec<Block>], depth: usize) -> Docx {
    let numbering_id = if ordered { ORDERED_ID } else { BULLET_ID };
    let level = depth.min(MAX_LIST_DEPTH);
    let mut docx = docx;

    for item in items {
        let mut first = true;
        for child in item {
            match child {
                // The item's own text: render as a numbered paragraph.
                Block::Paragraph(inlines) if first => {
                    first = false;
                    let paragraph = with_runs(Paragraph::new(), inlines, Style::default())
                        .numbering(NumberingId::new(numbering_id), IndentLevel::new(level));
                    docx = docx.add_paragraph(paragraph);
                }
                // A nested list continues at the next indent level.
                Block::List { ordered: nested_ordered, items: nested_items } => {
                    docx = push_list(docx, *nested_ordered, nested_items, depth + 1);
                }
                other => {
                    first = false;
                    docx = push_block(docx, other, depth + 1);
                }
            }
        }
    }

    docx
}

fn heading_paragraph(level: u8, inlines: &[Inline]) -> Paragraph {
    // Half-points: 36 = 18pt, 28 = 14pt, 24 = 12pt.
    let size = match level {
        1 => 36,
        2 => 28,
        3 => 24,
        _ => 22,
    };
    let style = Style { bold: true, size: Some(size), ..Style::default() };
    with_runs(Paragraph::new().style(&format!("Heading{level}")), inlines, style)
}

fn build_table(header: &[Vec<Inline>], rows: &[Vec<Vec<Inline>>]) -> Table {
    let columns = header
        .len()
        .max(rows.iter().map(|row| row.len()).max().unwrap_or(0))
        .max(1);
    let column_width = CONTENT_WIDTH_TWIPS / columns;

    let mut table_rows = Vec::new();

    if !header.is_empty() {
        table_rows.push(build_row(header, columns, true));
    }
    for row in rows {
        table_rows.push(build_row(row, columns, false));
    }

    // A table with no rows at all is invalid; emit a single empty row instead.
    if table_rows.is_empty() {
        table_rows.push(build_row(&[], columns, false));
    }

    Table::new(table_rows).set_grid(vec![column_width; columns])
}

fn build_row(cells: &[Vec<Inline>], columns: usize, header: bool) -> TableRow {
    let mut table_cells = Vec::new();
    for index in 0..columns {
        let empty = Vec::new();
        let inlines = cells.get(index).unwrap_or(&empty);
        let style = Style { bold: header, ..Style::default() };
        table_cells
            .push(TableCell::new().add_paragraph(with_runs(Paragraph::new(), inlines, style)));
    }
    TableRow::new(table_cells)
}

/// Formatting inherited down the inline tree.
#[derive(Clone, Copy, Default)]
struct Style {
    bold: bool,
    italic: bool,
    code: bool,
    link: bool,
    size: Option<usize>,
}

fn with_runs(paragraph: Paragraph, inlines: &[Inline], style: Style) -> Paragraph {
    let mut runs = Vec::new();
    collect_runs(inlines, style, &mut runs);

    let mut paragraph = paragraph;
    for run in runs {
        paragraph = paragraph.add_run(run);
    }
    paragraph
}

fn collect_runs(inlines: &[Inline], style: Style, out: &mut Vec<Run>) {
    for inline in inlines {
        match inline {
            Inline::Text(text) => push_text_runs(text, style, out),
            Inline::Code(text) => push_text_runs(text, Style { code: true, ..style }, out),
            Inline::Bold(children) => collect_runs(children, Style { bold: true, ..style }, out),
            Inline::Italic(children) => {
                collect_runs(children, Style { italic: true, ..style }, out)
            }
            Inline::Link { text, .. } => {
                // The URL itself is dropped: docx-rs hyperlink support needs
                // relationship bookkeeping that buys little for a meeting note.
                collect_runs(text, Style { link: true, ..style }, out)
            }
        }
    }
}

fn push_text_runs(text: &str, style: Style, out: &mut Vec<Run>) {
    // Hard breaks arrive as newlines inside a single text node.
    for (index, segment) in text.split('\n').enumerate() {
        if index > 0 {
            out.push(Run::new().add_break(BreakType::TextWrapping));
        }
        if segment.is_empty() {
            continue;
        }
        out.push(styled_run(segment, style));
    }
}

fn styled_run(text: &str, style: Style) -> Run {
    let mut run = Run::new().add_text(text);
    if style.bold {
        run = run.bold();
    }
    if style.italic {
        run = run.italic();
    }
    if let Some(size) = style.size {
        run = run.size(size);
    }
    if style.code {
        run = run.fonts(RunFonts::new().ascii("Consolas").hi_ansi("Consolas"));
    }
    if style.link {
        run = run.color("0969DA").underline("single");
    }
    run
}

fn monospace_run(text: &str) -> Run {
    styled_run(text, Style { code: true, ..Style::default() })
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "# Toplantı Özeti\n\n\
        ## Özet\n\nAyşe Çığır **ölçüm** altyapısını gözden geçirdi.\n\n\
        ## Aksiyon Maddeleri\n\n\
        | Sorumlu | Görev |\n| --- | --- |\n| Ayşe | Panoyu hazırla |\n\n\
        ## Kararlar\n\n- Ölçüm önce.\n  - Alt madde.\n- İşe alım ertelendi.\n\n\
        1. Birinci\n2. İkinci\n";

    /// Zip entry names are stored uncompressed in the local file headers, so a
    /// byte search is enough to confirm the package layout.
    fn contains(haystack: &[u8], needle: &str) -> bool {
        haystack
            .windows(needle.len())
            .any(|window| window == needle.as_bytes())
    }

    #[test]
    fn produces_a_valid_docx_package() {
        let bytes = markdown_to_docx(SAMPLE).expect("render should succeed");
        assert!(bytes.starts_with(b"PK\x03\x04"), "not a zip archive");
        assert!(contains(&bytes, "word/document.xml"), "missing document part");
        assert!(contains(&bytes, "[Content_Types].xml"), "missing content types");
        assert!(contains(&bytes, "word/numbering.xml"), "missing numbering part");
        assert!(bytes.len() > 1000, "suspiciously small package: {}", bytes.len());
    }

    #[test]
    fn renders_empty_markdown_without_failing() {
        let bytes = markdown_to_docx("").expect("empty input should still produce a package");
        assert!(bytes.starts_with(b"PK\x03\x04"));
    }

    #[test]
    fn table_columns_are_sized_from_the_widest_row() {
        let header = vec![vec![Inline::Text("A".into())]];
        let rows = vec![vec![
            vec![Inline::Text("1".into())],
            vec![Inline::Text("2".into())],
            vec![Inline::Text("3".into())],
        ]];
        let table = build_table(&header, &rows);
        // Three columns detected from the widest row, not the one-cell header.
        assert_eq!(table.grid.len(), 3);
        assert_eq!(table.grid[0], CONTENT_WIDTH_TWIPS / 3);
    }

    #[test]
    fn short_cells_are_padded_to_the_column_count() {
        let row = build_row(&[vec![Inline::Text("only".into())]], 3, false);
        assert_eq!(row.cells.len(), 3);
    }

    #[test]
    fn hard_breaks_split_into_separate_runs() {
        let mut runs = Vec::new();
        push_text_runs("first\nsecond", Style::default(), &mut runs);
        assert_eq!(runs.len(), 3, "expected text, break, text");
    }

    #[test]
    fn nested_lists_do_not_lose_content() {
        let bytes = markdown_to_docx("- outer\n  - inner\n").expect("render");
        let blocks = parse_markdown("- outer\n  - inner\n");
        let Block::List { items, .. } = &blocks[0] else {
            panic!("expected list");
        };
        match items[0].first() {
            Some(Block::Paragraph(inlines)) => {
                assert_eq!(super::super::blocks::inline_text(inlines), "outer")
            }
            other => panic!("expected paragraph, got {other:?}"),
        }
        assert!(bytes.len() > 1000);
    }
}
