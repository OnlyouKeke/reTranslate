//! DOCX Document Parser using docx-rs
//! Conforms to Office Open XML (DOCX) and reTranslate Unified AST

use std::collections::HashMap;
use std::io::Cursor;
use std::path::{Path, PathBuf};

use docx_rs::{
    DocumentChild, Paragraph, ParagraphChild, RunChild, Table, TableCellContent, TableChild,
    TableRowChild,
};
use zip::ZipArchive;

use crate::document::error::DocumentError;
use crate::document::model::{
    compute_sha256_hex, Block, BlockKind, BlockMetadata, Chapter, Document, DocumentFormat,
    DocumentMetadata, InlineSpan, InlineStyle, TableAlignment, TableCell, TranslationState,
};

pub struct DocxParser;

impl DocxParser {
    pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<Document, DocumentError> {
        let p = path.as_ref();
        if !p.exists() {
            return Err(DocumentError::FileNotFound(p.to_path_buf()));
        }
        let bytes = std::fs::read(p)?;
        let title = p
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled DOCX");
        Self::parse_bytes(&bytes, Some(title), Some(p.to_path_buf()))
    }

    pub fn parse_bytes(
        bytes: &[u8],
        title: Option<&str>,
        file_path: Option<PathBuf>,
    ) -> Result<Document, DocumentError> {
        if bytes.is_empty() {
            return Err(DocumentError::EmptyDocument);
        }

        // 1. Pre-flight check: archive must be a valid zip and contain word/document.xml
        let cursor = Cursor::new(bytes);
        let mut zip = ZipArchive::new(cursor).map_err(|e| {
            DocumentError::InvalidDocx(format!("Not a valid DOCX package: corrupt zip: {e}"))
        })?;

        let mut has_doc_xml = false;
        let mut doc_xml_empty = false;
        for i in 0..zip.len() {
            if let Ok(file) = zip.by_index(i) {
                if file.name() == "word/document.xml" {
                    has_doc_xml = true;
                    if file.size() == 0 {
                        doc_xml_empty = true;
                    }
                    break;
                }
            }
        }

        if !has_doc_xml {
            return Err(DocumentError::InvalidDocx(
                "Not a valid DOCX package: missing word/document.xml".to_string(),
            ));
        }

        if doc_xml_empty {
            return Err(DocumentError::InvalidDocx(
                "Not a valid DOCX package: word/document.xml is empty".to_string(),
            ));
        }

        // Validate XML well-formedness to protect against hangs in docx-rs on unclosed/malformed XML
        if let Ok(mut doc_file) = zip.by_name("word/document.xml") {
            let mut doc_xml_bytes = Vec::new();
            use std::io::Read;
            if doc_file.read_to_end(&mut doc_xml_bytes).is_ok() {
                let mut reader = quick_xml::Reader::from_reader(doc_xml_bytes.as_slice());
                let mut buf = Vec::new();
                let mut tag_stack: Vec<Vec<u8>> = Vec::new();
                let mut has_root = false;
                loop {
                    match reader.read_event_into(&mut buf) {
                        Ok(quick_xml::events::Event::Start(e)) => {
                            has_root = true;
                            tag_stack.push(e.name().as_ref().to_vec());
                        }
                        Ok(quick_xml::events::Event::End(e)) => {
                            if let Some(expected) = tag_stack.pop() {
                                if expected != e.name().as_ref() {
                                    return Err(DocumentError::InvalidDocx(
                                        "Mismatched closing tag in word/document.xml".to_string(),
                                    ));
                                }
                            } else {
                                return Err(DocumentError::InvalidDocx(
                                    "Unexpected closing tag in word/document.xml".to_string(),
                                ));
                            }
                        }
                        Ok(quick_xml::events::Event::Eof) => {
                            if !has_root || !tag_stack.is_empty() {
                                return Err(DocumentError::InvalidDocx(
                                    "Unclosed XML tags in word/document.xml".to_string(),
                                ));
                            }
                            break;
                        }
                        Err(e) => {
                            return Err(DocumentError::InvalidDocx(format!(
                                "Malformed XML in word/document.xml: {e:?}"
                            )));
                        }
                        _ => {}
                    }
                    buf.clear();
                }
            }
        }

        // 2. Parse using docx-rs
        let docx = docx_rs::read_docx(bytes).map_err(|e| {
            DocumentError::InvalidDocx(format!("Failed to parse DOCX contents: {e:?}"))
        })?;

        let doc_id = compute_sha256_hex(bytes);
        let doc_title = title.unwrap_or("Untitled DOCX").to_string();

        let metadata = DocumentMetadata {
            author: None,
            language: None,
            publisher: None,
            description: None,
            created_at: None,
            page_count: None,
            cover_image: None,
            cover_mime: None,
            extra: HashMap::new(),
        };

        // 3. Extract chapters and blocks
        let mut chapters = Vec::new();
        let mut current_chapter_blocks = Vec::new();
        let mut chapter_idx = 0;
        let mut current_chapter_title = doc_title.clone();
        let mut block_idx = 0;

        for child in docx.document.children {
            match child {
                DocumentChild::Paragraph(p) => {
                    let is_page_break_before = p.property.page_break_before == Some(true);
                    let (kind, heading_level) = Self::detect_paragraph_kind(&p);
                    let contains_break = Self::paragraph_has_page_break(&p);

                    // Chapter partition condition
                    let should_split = (heading_level == Some(1)
                        && !current_chapter_blocks.is_empty())
                        || (is_page_break_before && !current_chapter_blocks.is_empty())
                        || (contains_break && !current_chapter_blocks.is_empty());

                    if should_split {
                        chapters.push(Chapter {
                            id: format!("c{chapter_idx}"),
                            title: current_chapter_title.clone(),
                            order: chapter_idx,
                            blocks: std::mem::take(&mut current_chapter_blocks),
                            sub_chapters: Vec::new(),
                            source_href: None,
                        });
                        chapter_idx += 1;
                        block_idx = 0;
                        current_chapter_title = format!("Chapter {}", chapter_idx + 1);
                    }

                    let spans = Self::extract_paragraph_spans(&p);
                    if heading_level == Some(1) {
                        let text = spans.iter().map(|s| s.text.as_str()).collect::<String>();
                        if !text.trim().is_empty() {
                            current_chapter_title = text.trim().to_string();
                        }
                    }

                    if !spans.is_empty() {
                        let block = Block {
                            id: format!("c{chapter_idx}_b{block_idx}"),
                            chapter_id: format!("c{chapter_idx}"),
                            index: block_idx,
                            kind,
                            spans,
                            translatable: true,
                            translation: TranslationState::Untranslated,
                            metadata: BlockMetadata::default(),
                        };
                        current_chapter_blocks.push(block);
                        block_idx += 1;
                    }
                }
                DocumentChild::Table(tbl) => {
                    let table_block = Self::extract_table_block(
                        &tbl,
                        &format!("c{chapter_idx}"),
                        block_idx,
                    );
                    current_chapter_blocks.push(table_block);
                    block_idx += 1;
                }
                _ => {}
            }
        }

        // Flush trailing chapter
        if !current_chapter_blocks.is_empty() {
            chapters.push(Chapter {
                id: format!("c{chapter_idx}"),
                title: current_chapter_title,
                order: chapter_idx,
                blocks: current_chapter_blocks,
                sub_chapters: Vec::new(),
                source_href: None,
            });
        }

        if chapters.is_empty() {
            return Err(DocumentError::EmptyDocument);
        }

        let mut document = Document {
            id: doc_id,
            title: doc_title,
            format: DocumentFormat::Docx,
            file_path,
            file_size: bytes.len() as u64,
            metadata,
            chapters,
            total_blocks: 0,
        };
        document.recalculate_total_blocks();
        Ok(document)
    }

    /// Detect paragraph kind (Heading 1-6, ListItem, or Paragraph)
    fn detect_paragraph_kind(p: &Paragraph) -> (BlockKind, Option<u8>) {
        if let Some(style) = &p.property.style {
            let s = style.val.to_ascii_lowercase();
            if s == "heading1" || s == "heading 1" || s == "1" {
                return (BlockKind::Heading(1), Some(1));
            } else if s == "heading2" || s == "heading 2" || s == "2" {
                return (BlockKind::Heading(2), Some(2));
            } else if s == "heading3" || s == "heading 3" || s == "3" {
                return (BlockKind::Heading(3), Some(3));
            } else if s == "heading4" || s == "heading 4" || s == "4" {
                return (BlockKind::Heading(4), Some(4));
            } else if s == "heading5" || s == "heading 5" || s == "5" {
                return (BlockKind::Heading(5), Some(5));
            } else if s == "heading6" || s == "heading 6" || s == "6" {
                return (BlockKind::Heading(6), Some(6));
            }
        }

        if let Some(num_pr) = &p.property.numbering_property {
            let depth = num_pr.level.as_ref().map_or(0, |l| l.val as u8);
            return (
                BlockKind::ListItem {
                    order: None,
                    depth,
                },
                None,
            );
        }

        (BlockKind::Paragraph, None)
    }

    fn paragraph_has_page_break(p: &Paragraph) -> bool {
        for child in &p.children {
            if let ParagraphChild::Run(run) = child {
                for r_child in &run.children {
                    if let RunChild::Break(b) = r_child {
                        let val = serde_json::to_value(b).unwrap_or_default();
                        let break_type = val.get("breakType").and_then(|v| v.as_str());
                        if break_type == Some("page") {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    /// Extract inline spans and formatting from Paragraph runs
    fn extract_paragraph_spans(p: &Paragraph) -> Vec<InlineSpan> {
        let mut spans = Vec::new();
        for child in &p.children {
            match child {
                ParagraphChild::Run(run) => {
                    let prop = &run.run_property;
                    let is_bold = prop.bold.is_some();
                    let is_italic = prop.italic.is_some();
                    let is_strike = prop.strike.is_some();
                    let is_underline = prop.underline.is_some();

                    let style = if is_bold && is_italic {
                        InlineStyle::BoldItalic
                    } else if is_bold {
                        InlineStyle::Bold
                    } else if is_italic {
                        InlineStyle::Italic
                    } else if is_strike {
                        InlineStyle::Strikethrough
                    } else if is_underline {
                        InlineStyle::Underline
                    } else {
                        InlineStyle::Plain
                    };

                    for r_child in &run.children {
                        match r_child {
                            RunChild::Text(t) => {
                                if !t.text.is_empty() {
                                    spans.push(InlineSpan {
                                        text: t.text.clone(),
                                        style: style.clone(),
                                    });
                                }
                            }
                            RunChild::Tab(_) => {
                                spans.push(InlineSpan {
                                    text: "\t".to_string(),
                                    style: style.clone(),
                                });
                            }
                            _ => {}
                        }
                    }
                }
                ParagraphChild::Hyperlink(link) => {
                    let url = match &link.link {
                        docx_rs::HyperlinkData::External { rid, path } => {
                            if !path.is_empty() {
                                path.clone()
                            } else {
                                rid.clone()
                            }
                        }
                        docx_rs::HyperlinkData::Anchor { anchor } => anchor.clone(),
                    };
                    for r in &link.children {
                        if let ParagraphChild::Run(run) = r {
                            for r_child in &run.children {
                                if let RunChild::Text(t) = r_child {
                                    spans.push(InlineSpan {
                                        text: t.text.clone(),
                                        style: InlineStyle::Link {
                                            url: url.clone(),
                                        },
                                    });
                                }
                            }
                        }
                    }
                }
                _ => {}
            }
        }
        spans
    }

    /// Extract Table block with merged cells and alignment
    fn extract_table_block(tbl: &Table, chap_id: &str, block_idx: usize) -> Block {
        let mut rows: Vec<Vec<TableCell>> = Vec::new();

        for child in &tbl.rows {
            let TableChild::TableRow(row) = child;
            let mut row_cells = Vec::new();

            for cell_child in &row.cells {
                let TableRowChild::TableCell(cell) = cell_child;

                // Extract cell text from child paragraphs
                let mut cell_spans = Vec::new();
                for content in &cell.children {
                    if let TableCellContent::Paragraph(p) = content {
                        cell_spans.extend(Self::extract_paragraph_spans(p));
                    }
                }

                // Query gridSpan & verticalMerge via serde_json
                let prop_val = serde_json::to_value(&cell.property).unwrap_or_default();
                let col_span = prop_val
                    .get("gridSpan")
                    .and_then(|v| {
                        v.as_u64()
                            .or_else(|| v.get("val").and_then(|x| x.as_u64()))
                    })
                    .map(|v| v as usize)
                    .unwrap_or(1);

                let row_span = match prop_val.get("verticalMerge").and_then(|v| {
                    v.as_str()
                        .or_else(|| v.get("val").and_then(|x| x.as_str()))
                }) {
                    Some("restart") => 2, // Marks start of vertical merge
                    _ => 1,
                };

                row_cells.push(TableCell {
                    spans: cell_spans,
                    col_span,
                    row_span,
                    align: TableAlignment::Left,
                });
            }
            if !row_cells.is_empty() {
                rows.push(row_cells);
            }
        }

        Block {
            id: format!("{chap_id}_b{block_idx}"),
            chapter_id: chap_id.to_string(),
            index: block_idx,
            kind: BlockKind::Table {
                headers: Vec::new(),
                rows,
            },
            spans: Vec::new(),
            translatable: true,
            translation: TranslationState::Untranslated,
            metadata: BlockMetadata::default(),
        }
    }
}

pub fn parse_docx<P: AsRef<Path>>(path: P) -> Result<Document, DocumentError> {
    DocxParser::parse_file(path)
}

pub fn parse_docx_from_bytes(
    bytes: &[u8],
    title: Option<&str>,
) -> Result<Document, DocumentError> {
    DocxParser::parse_bytes(bytes, title, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use docx_rs::{Docx, Run};
    use std::io::Write;
    use zip::write::FileOptions;
    use zip::ZipWriter;

    #[test]
    fn test_docx_corrupted_zip_archive() {
        let corrupt = b"not a zip file";
        let res = DocxParser::parse_bytes(corrupt, None, None);
        assert!(matches!(res, Err(DocumentError::InvalidDocx(_))));
    }

    #[test]
    fn test_docx_missing_document_xml() {
        let mut buffer = Cursor::new(Vec::new());
        let mut zip = ZipWriter::new(&mut buffer);
        zip.start_file("other.txt", FileOptions::<()>::default()).unwrap();
        zip.write_all(b"hello").unwrap();
        zip.finish().unwrap();

        let bytes = buffer.into_inner();
        let res = DocxParser::parse_bytes(&bytes, None, None);
        match res {
            Err(DocumentError::InvalidDocx(msg)) => {
                assert!(msg.contains("word/document.xml"));
            }
            other => panic!("Expected InvalidDocx, got {:?}", other),
        }
    }

    #[test]
    fn test_docx_build_and_parse_valid() {
        let docx = Docx::new()
            .add_paragraph(
                docx_rs::Paragraph::new()
                    .add_run(Run::new().add_text("Heading One").bold())
                    .style("Heading1"),
            )
            .add_paragraph(
                docx_rs::Paragraph::new()
                    .add_run(Run::new().add_text("Normal paragraph with "))
                    .add_run(Run::new().add_text("italic words.").italic()),
            );

        let mut buf = Cursor::new(Vec::new());
        docx.build().pack(&mut buf).unwrap();
        let bytes = buf.into_inner();

        let parsed = DocxParser::parse_bytes(&bytes, Some("My Doc"), None).unwrap();
        assert_eq!(parsed.title, "My Doc");
        assert_eq!(parsed.chapters.len(), 1);
        assert_eq!(parsed.chapters[0].blocks.len(), 2);
        assert_eq!(parsed.chapters[0].blocks[0].kind, BlockKind::Heading(1));
        assert_eq!(parsed.chapters[0].blocks[0].plain_text(), "Heading One");
        assert_eq!(parsed.chapters[0].blocks[1].kind, BlockKind::Paragraph);
        assert!(parsed.chapters[0].blocks[1].plain_text().contains("Normal paragraph"));
    }
}

