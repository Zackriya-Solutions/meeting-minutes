//! Markdown to structured block model.
//!
//! The DOCX renderer needs structured input; PDF goes straight through
//! `pulldown_cmark`'s HTML writer instead. This module is therefore the parser
//! for the DOCX path only, and is kept deliberately small: the block set covers
//! exactly what meeting summaries and transcripts produce.

use pulldown_cmark::{CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd};

#[derive(Debug, Clone, PartialEq)]
pub enum Inline {
    Text(String),
    Bold(Vec<Inline>),
    Italic(Vec<Inline>),
    Code(String),
    Link { text: Vec<Inline>, href: String },
}

#[derive(Debug, Clone, PartialEq)]
pub enum Block {
    Heading { level: u8, inlines: Vec<Inline> },
    Paragraph(Vec<Inline>),
    List { ordered: bool, items: Vec<Vec<Block>> },
    Table { header: Vec<Vec<Inline>>, rows: Vec<Vec<Vec<Inline>>> },
    Code(String),
    Quote(Vec<Block>),
    Rule,
}

/// Markdown options used everywhere in the export pipeline.
///
/// Kept in one place so the PDF (HTML) and DOCX (block) paths cannot disagree
/// about which extensions are active.
pub fn markdown_options() -> Options {
    Options::ENABLE_TABLES | Options::ENABLE_STRIKETHROUGH | Options::ENABLE_TASKLISTS
}

pub fn parse_markdown(markdown: &str) -> Vec<Block> {
    let mut builder = Builder::new();
    for event in Parser::new_ext(markdown, markdown_options()) {
        builder.push(event);
    }
    builder.finish()
}

struct Builder {
    /// Stack of block containers. Index 0 is the document root; deeper frames
    /// are list-item or blockquote bodies.
    blocks: Vec<Vec<Block>>,
    /// Stack of inline containers. Empty means "not currently collecting inlines".
    inlines: Vec<Vec<Inline>>,
    /// Open lists: (ordered, items completed so far).
    lists: Vec<(bool, Vec<Vec<Block>>)>,
    /// Open link destinations, parallel to the inline frames they opened.
    links: Vec<String>,
    /// Depth of `inlines` at which each open list item's own text collects.
    ///
    /// A *tight* list emits item text directly inside `Item` with no wrapping
    /// paragraph, so every item opens an inline frame of its own; without this
    /// the text would have nowhere to go and be dropped.
    item_frames: Vec<usize>,
    heading_levels: Vec<u8>,
    code_buf: Option<String>,
    table_header: Vec<Vec<Inline>>,
    table_rows: Vec<Vec<Vec<Inline>>>,
    table_row: Vec<Vec<Inline>>,
    in_table_head: bool,
}

impl Builder {
    fn new() -> Self {
        Self {
            blocks: vec![Vec::new()],
            inlines: Vec::new(),
            lists: Vec::new(),
            links: Vec::new(),
            item_frames: Vec::new(),
            heading_levels: Vec::new(),
            code_buf: None,
            table_header: Vec::new(),
            table_rows: Vec::new(),
            table_row: Vec::new(),
            in_table_head: false,
        }
    }

    fn finish(mut self) -> Vec<Block> {
        // Unbalanced input would leave extra frames; flatten rather than panic.
        while self.blocks.len() > 1 {
            let frame = self.blocks.pop().expect("len > 1 checked above");
            self.blocks
                .last_mut()
                .expect("root frame always present")
                .extend(frame);
        }
        self.blocks.pop().unwrap_or_default()
    }

    fn emit(&mut self, block: Block) {
        self.blocks
            .last_mut()
            .expect("root frame always present")
            .push(block);
    }

    fn add_inline(&mut self, inline: Inline) {
        if let Some(frame) = self.inlines.last_mut() {
            frame.push(inline);
        }
    }

    fn pop_inlines(&mut self) -> Vec<Inline> {
        self.inlines.pop().unwrap_or_default()
    }

    /// Emits any text a tight list item has accumulated so far as a paragraph.
    ///
    /// Called before every block-level child so that, in `- outer\n  - inner`,
    /// "outer" lands ahead of the nested list rather than after it.
    fn flush_item_text(&mut self) {
        let Some(&frame_depth) = self.item_frames.last() else {
            return;
        };
        // Only flush when the item's own frame is the one on top; a nested
        // paragraph or emphasis frame owns the text instead.
        if self.inlines.len() != frame_depth {
            return;
        }
        let Some(frame) = self.inlines.last_mut() else {
            return;
        };
        if frame.is_empty() {
            return;
        }

        let inlines = std::mem::take(frame);
        self.emit(Block::Paragraph(inlines));
    }

    fn push(&mut self, event: Event<'_>) {
        match event {
            Event::Start(tag) => self.start(tag),
            Event::End(tag) => self.end(tag),
            Event::Text(text) => {
                if let Some(buf) = self.code_buf.as_mut() {
                    buf.push_str(&text);
                } else {
                    self.add_inline(Inline::Text(text.to_string()));
                }
            }
            Event::Code(code) => self.add_inline(Inline::Code(code.to_string())),
            Event::SoftBreak => self.add_inline(Inline::Text(" ".to_string())),
            Event::HardBreak => self.add_inline(Inline::Text("\n".to_string())),
            Event::Rule => self.emit(Block::Rule),
            // Raw HTML, footnotes and math have no meaning in an exported
            // meeting document; dropping them is intentional.
            _ => {}
        }
    }

    fn start(&mut self, tag: Tag<'_>) {
        // Any block-level child ends the run of loose text a tight list item
        // may have started.
        if matches!(
            tag,
            Tag::Paragraph
                | Tag::Heading { .. }
                | Tag::List(_)
                | Tag::BlockQuote(_)
                | Tag::CodeBlock(_)
                | Tag::Table(_)
        ) {
            self.flush_item_text();
        }

        match tag {
            Tag::Paragraph => self.inlines.push(Vec::new()),
            Tag::Heading { level, .. } => {
                self.heading_levels.push(heading_level(level));
                self.inlines.push(Vec::new());
            }
            Tag::Strong | Tag::Emphasis => self.inlines.push(Vec::new()),
            Tag::Link { dest_url, .. } => {
                self.links.push(dest_url.to_string());
                self.inlines.push(Vec::new());
            }
            Tag::List(start) => self.lists.push((start.is_some(), Vec::new())),
            Tag::Item => {
                self.blocks.push(Vec::new());
                self.inlines.push(Vec::new());
                self.item_frames.push(self.inlines.len());
            }
            Tag::BlockQuote(_) => self.blocks.push(Vec::new()),
            Tag::CodeBlock(CodeBlockKind::Fenced(_) | CodeBlockKind::Indented) => {
                self.code_buf = Some(String::new());
            }
            Tag::Table(_) => {
                self.table_header.clear();
                self.table_rows.clear();
                self.table_row.clear();
            }
            Tag::TableHead => {
                self.in_table_head = true;
                self.table_row.clear();
            }
            Tag::TableRow => self.table_row.clear(),
            Tag::TableCell => self.inlines.push(Vec::new()),
            _ => {}
        }
    }

    fn end(&mut self, tag: TagEnd) {
        match tag {
            TagEnd::Paragraph => {
                let inlines = self.pop_inlines();
                if !inlines.is_empty() {
                    self.emit(Block::Paragraph(inlines));
                }
            }
            TagEnd::Heading(_) => {
                let inlines = self.pop_inlines();
                let level = self.heading_levels.pop().unwrap_or(1);
                self.emit(Block::Heading { level, inlines });
            }
            TagEnd::Strong => {
                let inner = self.pop_inlines();
                self.add_inline(Inline::Bold(inner));
            }
            TagEnd::Emphasis => {
                let inner = self.pop_inlines();
                self.add_inline(Inline::Italic(inner));
            }
            TagEnd::Link => {
                let inner = self.pop_inlines();
                let href = self.links.pop().unwrap_or_default();
                self.add_inline(Inline::Link { text: inner, href });
            }
            TagEnd::Item => {
                self.flush_item_text();
                self.inlines.pop();
                self.item_frames.pop();

                let item = self.blocks.pop().unwrap_or_default();
                if let Some((_, items)) = self.lists.last_mut() {
                    items.push(item);
                } else {
                    // Item outside a list: keep the content rather than drop it.
                    for block in item {
                        self.emit(block);
                    }
                }
            }
            TagEnd::List(_) => {
                if let Some((ordered, items)) = self.lists.pop() {
                    self.emit(Block::List { ordered, items });
                }
            }
            TagEnd::BlockQuote(_) => {
                let inner = self.blocks.pop().unwrap_or_default();
                self.emit(Block::Quote(inner));
            }
            TagEnd::CodeBlock => {
                if let Some(code) = self.code_buf.take() {
                    self.emit(Block::Code(code.trim_end_matches('\n').to_string()));
                }
            }
            TagEnd::TableCell => {
                let cell = self.pop_inlines();
                self.table_row.push(cell);
            }
            TagEnd::TableHead => {
                self.table_header = std::mem::take(&mut self.table_row);
                self.in_table_head = false;
            }
            TagEnd::TableRow => {
                if !self.in_table_head {
                    let row = std::mem::take(&mut self.table_row);
                    self.table_rows.push(row);
                }
            }
            TagEnd::Table => {
                let header = std::mem::take(&mut self.table_header);
                let rows = std::mem::take(&mut self.table_rows);
                self.emit(Block::Table { header, rows });
            }
            _ => {}
        }
    }
}

fn heading_level(level: HeadingLevel) -> u8 {
    match level {
        HeadingLevel::H1 => 1,
        HeadingLevel::H2 => 2,
        HeadingLevel::H3 => 3,
        HeadingLevel::H4 => 4,
        HeadingLevel::H5 => 5,
        HeadingLevel::H6 => 6,
    }
}

/// Flattens inlines to plain text. Used by the DOCX table sizing heuristic and
/// by tests.
pub fn inline_text(inlines: &[Inline]) -> String {
    let mut out = String::new();
    for inline in inlines {
        match inline {
            Inline::Text(t) => out.push_str(t),
            Inline::Code(t) => out.push_str(t),
            Inline::Bold(children) | Inline::Italic(children) => {
                out.push_str(&inline_text(children))
            }
            Inline::Link { text, .. } => out.push_str(&inline_text(text)),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(s: &str) -> Inline {
        Inline::Text(s.to_string())
    }

    #[test]
    fn parses_headings_at_each_level() {
        let blocks = parse_markdown("# One\n\n## Two\n\n### Three");
        assert_eq!(
            blocks,
            vec![
                Block::Heading { level: 1, inlines: vec![text("One")] },
                Block::Heading { level: 2, inlines: vec![text("Two")] },
                Block::Heading { level: 3, inlines: vec![text("Three")] },
            ]
        );
    }

    #[test]
    fn parses_emphasis_and_inline_code() {
        let blocks = parse_markdown("Plain **bold** and *italic* and `code`.");
        let Block::Paragraph(inlines) = &blocks[0] else {
            panic!("expected paragraph, got {:?}", blocks[0]);
        };
        assert_eq!(inlines[0], text("Plain "));
        assert_eq!(inlines[1], Inline::Bold(vec![text("bold")]));
        assert_eq!(inlines[3], Inline::Italic(vec![text("italic")]));
        assert_eq!(inlines[5], Inline::Code("code".to_string()));
    }

    #[test]
    fn parses_nested_bullet_lists() {
        let blocks = parse_markdown("- outer\n  - inner\n- second");
        let Block::List { ordered, items } = &blocks[0] else {
            panic!("expected list, got {:?}", blocks[0]);
        };
        assert!(!ordered);
        assert_eq!(items.len(), 2);
        // First item holds its own text plus the nested list.
        assert_eq!(items[0][0], Block::Paragraph(vec![text("outer")]));
        assert!(matches!(items[0][1], Block::List { ordered: false, .. }));
        assert_eq!(items[1][0], Block::Paragraph(vec![text("second")]));
    }

    /// Regression: tight list items carry their text directly, with no
    /// wrapping paragraph. An earlier version dropped it entirely.
    #[test]
    fn tight_list_items_keep_their_text() {
        let blocks = parse_markdown("- first item\n- second item\n");
        let Block::List { items, .. } = &blocks[0] else {
            panic!("expected list, got {:?}", blocks[0]);
        };
        assert_eq!(items.len(), 2);
        assert_eq!(items[0], vec![Block::Paragraph(vec![text("first item")])]);
        assert_eq!(items[1], vec![Block::Paragraph(vec![text("second item")])]);
    }

    #[test]
    fn loose_list_items_keep_their_paragraph() {
        let blocks = parse_markdown("- first\n\n- second\n");
        let Block::List { items, .. } = &blocks[0] else {
            panic!("expected list");
        };
        assert_eq!(items[0], vec![Block::Paragraph(vec![text("first")])]);
        assert_eq!(items[1], vec![Block::Paragraph(vec![text("second")])]);
    }

    #[test]
    fn item_text_precedes_its_nested_list() {
        let blocks = parse_markdown("- outer\n  - inner\n");
        let Block::List { items, .. } = &blocks[0] else {
            panic!("expected list");
        };
        assert_eq!(items[0][0], Block::Paragraph(vec![text("outer")]));
        assert!(
            matches!(items[0][1], Block::List { .. }),
            "the nested list must come after the item's own text"
        );
    }

    #[test]
    fn formatted_text_inside_tight_items_survives() {
        let blocks = parse_markdown("- **Ayşe** owns `metrics`\n");
        let Block::List { items, .. } = &blocks[0] else {
            panic!("expected list");
        };
        let Block::Paragraph(inlines) = &items[0][0] else {
            panic!("expected paragraph");
        };
        assert_eq!(inlines[0], Inline::Bold(vec![text("Ayşe")]));
        assert_eq!(inline_text(inlines), "Ayşe owns metrics");
    }

    #[test]
    fn parses_ordered_lists() {
        let blocks = parse_markdown("1. first\n2. second");
        let Block::List { ordered, items } = &blocks[0] else {
            panic!("expected list, got {:?}", blocks[0]);
        };
        assert!(ordered);
        assert_eq!(items.len(), 2);
    }

    #[test]
    fn parses_gfm_tables() {
        let md = "| Owner | Task |\n| --- | --- |\n| Ayse | Ship it |\n| Mehmet | Review |";
        let blocks = parse_markdown(md);
        let Block::Table { header, rows } = &blocks[0] else {
            panic!("expected table, got {:?}", blocks[0]);
        };
        assert_eq!(inline_text(&header[0]), "Owner");
        assert_eq!(inline_text(&header[1]), "Task");
        assert_eq!(rows.len(), 2);
        assert_eq!(inline_text(&rows[0][0]), "Ayse");
        assert_eq!(inline_text(&rows[1][1]), "Review");
    }

    #[test]
    fn parses_bold_inside_table_cells() {
        let md = "| A | B |\n| --- | --- |\n| **Ayse** | plain |";
        let blocks = parse_markdown(md);
        let Block::Table { rows, .. } = &blocks[0] else {
            panic!("expected table");
        };
        assert_eq!(rows[0][0], vec![Inline::Bold(vec![text("Ayse")])]);
    }

    #[test]
    fn parses_code_blocks_and_rules() {
        let blocks = parse_markdown("```\nlet x = 1;\n```\n\n---\n");
        assert_eq!(blocks[0], Block::Code("let x = 1;".to_string()));
        assert_eq!(blocks[1], Block::Rule);
    }

    #[test]
    fn parses_links() {
        let blocks = parse_markdown("See [docs](https://example.com).");
        let Block::Paragraph(inlines) = &blocks[0] else {
            panic!("expected paragraph");
        };
        assert_eq!(
            inlines[1],
            Inline::Link {
                text: vec![text("docs")],
                href: "https://example.com".to_string()
            }
        );
    }

    #[test]
    fn preserves_non_ascii_text() {
        let blocks = parse_markdown("## Özet\n\nAyşe Çığır ölçüm İstanbul şğüöç.");
        assert_eq!(
            blocks[0],
            Block::Heading { level: 2, inlines: vec![text("Özet")] }
        );
        let Block::Paragraph(inlines) = &blocks[1] else {
            panic!("expected paragraph");
        };
        assert_eq!(inline_text(inlines), "Ayşe Çığır ölçüm İstanbul şğüöç.");
    }

    #[test]
    fn soft_breaks_become_spaces() {
        let blocks = parse_markdown("line one\nline two");
        let Block::Paragraph(inlines) = &blocks[0] else {
            panic!("expected paragraph");
        };
        assert_eq!(inline_text(inlines), "line one line two");
    }

    #[test]
    fn empty_input_yields_no_blocks() {
        assert!(parse_markdown("").is_empty());
        assert!(parse_markdown("   \n\n  ").is_empty());
    }
}
