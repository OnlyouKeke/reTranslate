//! Markdown Document Parser using pulldown-cmark
//! Conforms to CommonMark and reTranslate Unified AST

use std::path::{Path, PathBuf};

use pulldown_cmark::{
    Alignment, CodeBlockKind, Event, HeadingLevel, Options, Parser, Tag, TagEnd,
};

use crate::document::error::DocumentError;
use crate::document::model::*;

pub struct MarkdownParserOptions {
    pub split_on_h1: bool,
    pub split_on_horizontal_rule: bool,
}

impl Default for MarkdownParserOptions {
    fn default() -> Self {
        Self {
            split_on_h1: true,
            split_on_horizontal_rule: false,
        }
    }
}

pub struct MarkdownParser;

impl MarkdownParser {
    pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<Document, DocumentError> {
        let p = path.as_ref();
        if !p.exists() {
            return Err(DocumentError::FileNotFound(p.to_path_buf()));
        }
        let bytes = std::fs::read(p)?;
        let title = p
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled Markdown");
        let content = std::str::from_utf8(&bytes)
            .map_err(|e| DocumentError::EncodingError(e.to_string()))?;
        Self::parse_str(content, Some(title), Some(p.to_path_buf()), None)
    }

    pub fn parse_str(
        raw_content: &str,
        title: Option<&str>,
        file_path: Option<PathBuf>,
        options: Option<MarkdownParserOptions>,
    ) -> Result<Document, DocumentError> {
        if raw_content.trim().is_empty() {
            return Err(DocumentError::EmptyDocument);
        }

        let opts = options.unwrap_or_default();
        let doc_id = compute_sha256_hex(raw_content.as_bytes());
        let doc_title = title.unwrap_or("Untitled Markdown").to_string();

        let mut pulldown_opts = Options::empty();
        pulldown_opts.insert(Options::ENABLE_TABLES);
        pulldown_opts.insert(Options::ENABLE_FOOTNOTES);
        pulldown_opts.insert(Options::ENABLE_STRIKETHROUGH);
        pulldown_opts.insert(Options::ENABLE_TASKLISTS);
        pulldown_opts.insert(Options::ENABLE_HEADING_ATTRIBUTES);

        let parser = Parser::new_ext(raw_content, pulldown_opts);

        let mut chapters: Vec<Chapter> = Vec::new();
        let mut chap_idx = 0;
        let mut current_chap = Chapter {
            id: format!("chap_{chap_idx}"),
            title: doc_title.clone(),
            order: chap_idx,
            blocks: Vec::new(),
            sub_chapters: Vec::new(),
            source_href: None,
        };

        // Styling state
        let mut is_bold = false;
        let mut is_italic = false;
        let mut is_strike = false;
        let mut current_link: Option<String> = None;

        // Container state
        enum Container {
            None,
            Paragraph,
            Heading(u8),
            CodeBlock {
                language: Option<String>,
                buffer: String,
            },
            ListItem {
                order: Option<u64>,
                depth: u8,
            },
            Table {
                alignments: Vec<TableAlignment>,
                headers: Vec<TableCell>,
                rows: Vec<Vec<TableCell>>,
                current_row: Vec<TableCell>,
                is_head: bool,
            },
        }

        let mut container = Container::None;
        let mut current_spans: Vec<InlineSpan> = Vec::new();
        let mut list_counters: Vec<Option<u64>> = Vec::new();
        let mut list_depth: u8 = 0;
        let mut blockquote_depth: usize = 0;

        let push_block =
            |chap: &mut Chapter, kind: BlockKind, spans: Vec<InlineSpan>, translatable: bool| {
                let block_idx = chap.blocks.len();
                chap.blocks.push(Block {
                    id: format!("{}_b{}", chap.id, block_idx),
                    chapter_id: chap.id.clone(),
                    index: block_idx,
                    kind,
                    spans,
                    translatable,
                    translation: TranslationState::Untranslated,
                    metadata: BlockMetadata::default(),
                });
            };

        let get_active_style =
            |bold: bool, italic: bool, strike: bool, link: &Option<String>| -> InlineStyle {
                if let Some(url) = link {
                    InlineStyle::Link { url: url.clone() }
                } else if strike {
                    InlineStyle::Strikethrough
                } else if bold && italic {
                    InlineStyle::BoldItalic
                } else if bold {
                    InlineStyle::Bold
                } else if italic {
                    InlineStyle::Italic
                } else {
                    InlineStyle::Plain
                }
            };

        for event in parser {
            match event {
                // Heading
                Event::Start(Tag::Heading { level, .. }) => {
                    let lvl = match level {
                        HeadingLevel::H1 => 1,
                        HeadingLevel::H2 => 2,
                        HeadingLevel::H3 => 3,
                        HeadingLevel::H4 => 4,
                        HeadingLevel::H5 => 5,
                        HeadingLevel::H6 => 6,
                    };
                    container = Container::Heading(lvl);
                    current_spans.clear();
                }
                Event::End(TagEnd::Heading(_)) => {
                    if let Container::Heading(lvl) = container {
                        let heading_text = current_spans
                            .iter()
                            .map(|s| s.text.as_str())
                            .collect::<String>();

                        if lvl == 1 && opts.split_on_h1 {
                            if current_chap.blocks.is_empty() {
                                current_chap.title = heading_text.clone();
                            } else {
                                chapters.push(current_chap);
                                chap_idx += 1;
                                current_chap = Chapter {
                                    id: format!("chap_{chap_idx}"),
                                    title: heading_text.clone(),
                                    order: chap_idx,
                                    blocks: Vec::new(),
                                    sub_chapters: Vec::new(),
                                    source_href: None,
                                };
                            }
                        }
                        push_block(
                            &mut current_chap,
                            BlockKind::Heading(lvl),
                            current_spans.clone(),
                            true,
                        );
                        current_spans.clear();
                    }
                    container = Container::None;
                }

                // Paragraph
                Event::Start(Tag::Paragraph) => {
                    if matches!(container, Container::None) {
                        container = Container::Paragraph;
                        current_spans.clear();
                    }
                }
                Event::End(TagEnd::Paragraph) => {
                    if let Container::Paragraph = container {
                        if !current_spans.is_empty() {
                            let kind = if blockquote_depth > 0 {
                                BlockKind::Quote
                            } else {
                                BlockKind::Paragraph
                            };
                            push_block(
                                &mut current_chap,
                                kind,
                                current_spans.clone(),
                                true,
                            );
                            current_spans.clear();
                        }
                        container = Container::None;
                    }
                }

                // Code Block
                Event::Start(Tag::CodeBlock(kind)) => {
                    let language = match kind {
                        CodeBlockKind::Fenced(lang) => {
                            let l = lang.trim();
                            if l.is_empty() {
                                None
                            } else {
                                Some(l.to_string())
                            }
                        }
                        CodeBlockKind::Indented => None,
                    };
                    container = Container::CodeBlock {
                        language,
                        buffer: String::new(),
                    };
                }
                Event::End(TagEnd::CodeBlock) => {
                    if let Container::CodeBlock { language, buffer } = container {
                        push_block(
                            &mut current_chap,
                            BlockKind::CodeBlock {
                                language,
                                code: buffer.trim_end_matches('\n').to_string(),
                            },
                            vec![],
                            false, // Invariant: Code block strictly not translatable
                        );
                    }
                    container = Container::None;
                }

                // BlockQuote
                Event::Start(Tag::BlockQuote(_)) => {
                    blockquote_depth += 1;
                }
                Event::End(TagEnd::BlockQuote(_)) => {
                    if !current_spans.is_empty() {
                        push_block(
                            &mut current_chap,
                            BlockKind::Quote,
                            current_spans.clone(),
                            true,
                        );
                        current_spans.clear();
                    }
                    blockquote_depth = blockquote_depth.saturating_sub(1);
                }

                // Lists
                Event::Start(Tag::List(first_num)) => {
                    // If a list begins inside an open item, flush the item's prefix text first
                    if let Container::ListItem { order, depth } = container {
                        if !current_spans.is_empty() {
                            push_block(
                                &mut current_chap,
                                BlockKind::ListItem { order, depth },
                                current_spans.clone(),
                                true,
                            );
                            current_spans.clear();
                        }
                    }
                    list_counters.push(first_num);
                    if list_counters.len() > 1 {
                        list_depth += 1;
                    }
                }
                Event::End(TagEnd::List(_)) => {
                    list_counters.pop();
                    if list_depth > 0 {
                        list_depth -= 1;
                    }
                }
                Event::Start(Tag::Item) => {
                    let order = if let Some(last) = list_counters.last_mut() {
                        if let Some(counter) = last {
                            let val = *counter;
                            *counter += 1;
                            Some(val)
                        } else {
                            None
                        }
                    } else {
                        None
                    };
                    container = Container::ListItem {
                        order,
                        depth: list_depth,
                    };
                    current_spans.clear();
                }
                Event::End(TagEnd::Item) => {
                    if let Container::ListItem { order, depth } = container {
                        if !current_spans.is_empty() {
                            push_block(
                                &mut current_chap,
                                BlockKind::ListItem { order, depth },
                                current_spans.clone(),
                                true,
                            );
                            current_spans.clear();
                        }
                    }
                    container = Container::None;
                }

                // Tables
                Event::Start(Tag::Table(aligns)) => {
                    let alignments = aligns
                        .into_iter()
                        .map(|a| match a {
                            Alignment::Left => TableAlignment::Left,
                            Alignment::Center => TableAlignment::Center,
                            Alignment::Right => TableAlignment::Right,
                            Alignment::None => TableAlignment::None,
                        })
                        .collect();
                    container = Container::Table {
                        alignments,
                        headers: Vec::new(),
                        rows: Vec::new(),
                        current_row: Vec::new(),
                        is_head: false,
                    };
                }
                Event::End(TagEnd::Table) => {
                    if let Container::Table { headers, rows, .. } = container {
                        push_block(
                            &mut current_chap,
                            BlockKind::Table { headers, rows },
                            vec![],
                            true,
                        );
                    }
                    container = Container::None;
                }
                Event::Start(Tag::TableHead) => {
                    if let Container::Table {
                        ref mut is_head, ..
                    } = container
                    {
                        *is_head = true;
                    }
                }
                Event::End(TagEnd::TableHead) => {
                    if let Container::Table {
                        ref mut is_head, ..
                    } = container
                    {
                        *is_head = false;
                    }
                }
                Event::Start(Tag::TableRow) => {
                    if let Container::Table {
                        ref mut current_row,
                        ..
                    } = container
                    {
                        current_row.clear();
                    }
                }
                Event::End(TagEnd::TableRow) => {
                    if let Container::Table {
                        ref mut rows,
                        ref mut current_row,
                        ..
                    } = container
                    {
                        rows.push(current_row.clone());
                        current_row.clear();
                    }
                }
                Event::Start(Tag::TableCell) => {
                    current_spans.clear();
                }
                Event::End(TagEnd::TableCell) => {
                    if let Container::Table {
                        ref alignments,
                        ref mut headers,
                        ref mut current_row,
                        is_head,
                        ..
                    } = container
                    {
                        let col_idx = if is_head {
                            headers.len()
                        } else {
                            current_row.len()
                        };
                        let align = alignments
                            .get(col_idx)
                            .copied()
                            .unwrap_or(TableAlignment::None);
                        let cell = TableCell {
                            spans: current_spans.clone(),
                            col_span: 1,
                            row_span: 1,
                            align,
                        };
                        if is_head {
                            headers.push(cell);
                        } else {
                            current_row.push(cell);
                        }
                        current_spans.clear();
                    }
                }

                // Thematic Break
                Event::Rule => {
                    if opts.split_on_horizontal_rule && !current_chap.blocks.is_empty() {
                        chapters.push(current_chap);
                        chap_idx += 1;
                        current_chap = Chapter {
                            id: format!("chap_{chap_idx}"),
                            title: format!("Section {}", chap_idx + 1),
                            order: chap_idx,
                            blocks: Vec::new(),
                            sub_chapters: Vec::new(),
                            source_href: None,
                        };
                    } else {
                        push_block(&mut current_chap, BlockKind::ThematicBreak, vec![], false);
                    }
                }

                // Inline styles
                Event::Start(Tag::Strong) => is_bold = true,
                Event::End(TagEnd::Strong) => is_bold = false,
                Event::Start(Tag::Emphasis) => is_italic = true,
                Event::End(TagEnd::Emphasis) => is_italic = false,
                Event::Start(Tag::Strikethrough) => is_strike = true,
                Event::End(TagEnd::Strikethrough) => is_strike = false,
                Event::Start(Tag::Link { dest_url, .. }) => {
                    current_link = Some(dest_url.to_string())
                }
                Event::End(TagEnd::Link) => current_link = None,

                Event::Code(cow) => {
                    current_spans.push(InlineSpan {
                        text: cow.to_string(),
                        style: InlineStyle::Code,
                    });
                }
                Event::FootnoteReference(name) => {
                    current_spans.push(InlineSpan {
                        text: format!("[{name}]"),
                        style: InlineStyle::FootnoteRef {
                            id: name.to_string(),
                            note: None,
                        },
                    });
                }

                // Text
                Event::Text(cow) => {
                    if let Container::CodeBlock {
                        ref mut buffer, ..
                    } = container
                    {
                        buffer.push_str(&cow);
                    } else {
                        let style = get_active_style(is_bold, is_italic, is_strike, &current_link);
                        current_spans.push(InlineSpan {
                            text: cow.to_string(),
                            style,
                        });
                    }
                }
                Event::SoftBreak => {
                    if let Container::CodeBlock {
                        ref mut buffer, ..
                    } = container
                    {
                        buffer.push('\n');
                    } else {
                        let style = get_active_style(is_bold, is_italic, is_strike, &current_link);
                        current_spans.push(InlineSpan {
                            text: " ".to_string(),
                            style,
                        });
                    }
                }
                Event::HardBreak => {
                    let style = get_active_style(is_bold, is_italic, is_strike, &current_link);
                    current_spans.push(InlineSpan {
                        text: "\n".to_string(),
                        style,
                    });
                }

                // Raw HTML (sanitized / treated as safe plain text span)
                Event::Html(cow) | Event::InlineHtml(cow) => {
                    if let Container::CodeBlock {
                        ref mut buffer, ..
                    } = container
                    {
                        buffer.push_str(&cow);
                    } else {
                        let style = get_active_style(is_bold, is_italic, is_strike, &current_link);
                        current_spans.push(InlineSpan {
                            text: cow.to_string(),
                            style,
                        });
                    }
                }

                _ => {}
            }
        }

        if !current_chap.blocks.is_empty() {
            chapters.push(current_chap);
        }

        if chapters.is_empty() {
            return Err(DocumentError::EmptyDocument);
        }

        let mut doc = Document::new(doc_id, doc_title, DocumentFormat::Markdown);
        doc.file_path = file_path;
        doc.file_size = raw_content.len() as u64;
        doc.chapters = chapters;
        doc.recalculate_total_blocks();
        Ok(doc)
    }
}

pub fn parse_markdown<P: AsRef<Path>>(path: P) -> Result<Document, DocumentError> {
    MarkdownParser::parse_file(path)
}

pub fn parse_markdown_from_str(
    content: &str,
    title: Option<&str>,
) -> Result<Document, DocumentError> {
    MarkdownParser::parse_str(content, title, None, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markdown_heading_and_paragraphs() {
        let md = "# Title\n\nFirst paragraph with **bold** and *italic*.\n\n## Subheading\n\nSecond paragraph.";
        let doc = parse_markdown_from_str(md, Some("Test")).unwrap();
        assert_eq!(doc.chapters.len(), 1);
        assert_eq!(doc.chapters[0].blocks.len(), 4);
        assert_eq!(doc.chapters[0].blocks[0].kind, BlockKind::Heading(1));
        assert_eq!(doc.chapters[0].blocks[2].kind, BlockKind::Heading(2));
    }

    #[test]
    fn test_markdown_code_block_untranslatable() {
        let md = "```rust\nfn main() {}\n```";
        let doc = parse_markdown_from_str(md, Some("Code")).unwrap();
        assert_eq!(doc.total_blocks, 1);
        let block = &doc.chapters[0].blocks[0];
        assert!(!block.translatable);
        assert!(block.spans.is_empty());
    }

    #[test]
    fn test_markdown_chapter_splitting() {
        let md = "# Chapter 1\nIntro text.\n\n# Chapter 2\nMore text.";
        let doc = parse_markdown_from_str(md, Some("Book")).unwrap();
        assert_eq!(doc.chapters.len(), 2);
        assert_eq!(doc.chapters[0].title, "Chapter 1");
        assert_eq!(doc.chapters[1].title, "Chapter 2");
    }

    #[test]
    fn test_markdown_blockquote_and_lists() {
        let md = "> Quote line\n\n- Item 1\n- Item 2\n  1. Sub item 1\n  2. Sub item 2";
        let doc = parse_markdown_from_str(md, Some("Doc")).unwrap();
        assert!(doc.chapters[0].blocks.iter().any(|b| matches!(b.kind, BlockKind::Quote)));
        assert!(doc.chapters[0].blocks.iter().any(|b| matches!(b.kind, BlockKind::ListItem { .. })));
    }

    #[test]
    fn test_markdown_table() {
        let md = "| Left | Center | Right |\n| :--- | :---: | ---: |\n| 1 | 2 | 3 |";
        let doc = parse_markdown_from_str(md, Some("Table Doc")).unwrap();
        let tbl_block = doc.chapters[0].blocks.iter().find(|b| matches!(b.kind, BlockKind::Table { .. })).unwrap();
        if let BlockKind::Table { headers, rows } = &tbl_block.kind {
            assert_eq!(headers.len(), 3);
            assert_eq!(headers[0].align, TableAlignment::Left);
            assert_eq!(headers[1].align, TableAlignment::Center);
            assert_eq!(headers[2].align, TableAlignment::Right);
            assert_eq!(rows.len(), 1);
        } else {
            panic!("Expected table");
        }
    }

    #[test]
    fn test_markdown_raw_html_and_empty() {
        let md = "<script>alert('xss');</script>\nParagraph text.";
        let doc = parse_markdown_from_str(md, Some("HTML Doc")).unwrap();
        assert_eq!(doc.chapters[0].blocks.len(), 1);
        assert!(doc.chapters[0].blocks[0].plain_text().contains("Paragraph text."));

        assert!(matches!(parse_markdown_from_str("", None), Err(DocumentError::EmptyDocument)));
        assert!(matches!(parse_markdown_from_str("   \n\t  ", None), Err(DocumentError::EmptyDocument)));
    }
}

