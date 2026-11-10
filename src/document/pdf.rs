//! PDF Document Parser with Page-by-Page Extraction & Paragraph Reconstruction
//!
//! Provides reflowable bilingual reading blocks and page-level metadata.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use crate::document::error::DocumentError;
use crate::document::model::*;

pub struct PdfParser;

impl PdfParser {
    pub fn parse_file(path: &Path) -> Result<Document, DocumentError> {
        if !path.exists() {
            return Err(DocumentError::FileNotFound(path.to_path_buf()));
        }
        let bytes = fs::read(path).map_err(|e| DocumentError::CorruptFile(e.to_string()))?;
        let filename = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled PDF");
        Self::parse_bytes(&bytes, Some(filename), Some(path.to_path_buf()))
    }

    pub fn parse_bytes(
        bytes: &[u8],
        title: Option<&str>,
        file_path: Option<PathBuf>,
    ) -> Result<Document, DocumentError> {
        if bytes.is_empty() {
            return Err(DocumentError::EmptyDocument);
        }

        // Validate PDF magic bytes
        if !bytes.starts_with(b"%PDF-") && !bytes[..bytes.len().min(1024)].windows(5).any(|w| w == b"%PDF-") {
            return Err(DocumentError::UnsupportedFormat(
                "Not a valid PDF file: missing %PDF- header".to_string(),
            ));
        }

        let doc = lopdf::Document::load_mem(bytes).map_err(|e| {
            DocumentError::InvalidDocx(format!("Failed to parse PDF: {e:?}"))
        })?;

        let doc_id = compute_sha256_hex(bytes);
        let doc_title = title.unwrap_or("Untitled PDF").to_string();
        let page_numbers: Vec<u32> = doc.get_pages().keys().copied().collect();

        if page_numbers.is_empty() {
            return Err(DocumentError::EmptyDocument);
        }

        let metadata = DocumentMetadata {
            author: None,
            language: None,
            publisher: None,
            description: None,
            created_at: None,
            page_count: Some(page_numbers.len()),
            cover_image: None,
            cover_mime: None,
            extra: HashMap::new(),
        };

        let mut chapters = Vec::new();
        let mut total_blocks = 0;

        for (chapter_idx, &page_num) in page_numbers.iter().enumerate() {
            let page_text = doc.extract_text(&[page_num]).unwrap_or_default();
            let paragraphs = Self::extract_paragraphs_from_page(&page_text);

            let mut blocks = Vec::new();
            for (block_idx, para) in paragraphs.into_iter().enumerate() {
                let is_heading = Self::detect_heading(&para);
                let kind = if is_heading {
                    BlockKind::Heading(2)
                } else {
                    BlockKind::Paragraph
                };

                let block = Block {
                    id: format!("c{chapter_idx}_b{block_idx}"),
                    chapter_id: format!("c{chapter_idx}"),
                    index: block_idx,
                    kind,
                    spans: vec![InlineSpan {
                        text: para,
                        style: InlineStyle::Plain,
                    }],
                    translatable: true,
                    translation: TranslationState::Untranslated,
                    metadata: BlockMetadata {
                        page_number: Some(page_num as usize),
                        bbox: None,
                        anchor_id: None,
                    },
                };
                blocks.push(block);
            }

            if blocks.is_empty() {
                // If page text is empty (e.g. image-only scanned page), add a placeholder block
                blocks.push(Block {
                    id: format!("c{chapter_idx}_b0"),
                    chapter_id: format!("c{chapter_idx}"),
                    index: 0,
                    kind: BlockKind::Paragraph,
                    spans: vec![InlineSpan {
                        text: format!("[Page {} - Image or Empty Page]", page_num),
                        style: InlineStyle::Italic,
                    }],
                    translatable: false,
                    translation: TranslationState::Untranslated,
                    metadata: BlockMetadata {
                        page_number: Some(page_num as usize),
                        bbox: None,
                        anchor_id: None,
                    },
                });
            }

            total_blocks += blocks.len();
            chapters.push(Chapter {
                id: format!("c{chapter_idx}"),
                title: format!("Page {}", page_num),
                order: chapter_idx,
                blocks,
                sub_chapters: Vec::new(),
                source_href: None,
            });
        }

        let document = Document {
            id: doc_id,
            title: doc_title,
            format: DocumentFormat::Pdf,
            file_path,
            file_size: bytes.len() as u64,
            metadata,
            chapters,
            total_blocks,
        };

        Ok(document)
    }

    /// Segments raw extracted page text into coherent paragraphs
    fn extract_paragraphs_from_page(text: &str) -> Vec<String> {
        let normalized = text.replace("\r\n", "\n").replace('\r', "\n");
        let raw_lines: Vec<&str> = normalized.lines().map(|l| l.trim()).collect();

        let mut paragraphs = Vec::new();
        let mut current_para = Vec::new();

        for line in raw_lines {
            if line.is_empty() {
                if !current_para.is_empty() {
                    let para_text = Self::join_lines(&current_para);
                    if !para_text.is_empty() {
                        paragraphs.push(para_text);
                    }
                    current_para.clear();
                }
            } else {
                current_para.push(line);
            }
        }

        if !current_para.is_empty() {
            let para_text = Self::join_lines(&current_para);
            if !para_text.is_empty() {
                paragraphs.push(para_text);
            }
        }

        paragraphs
    }

    /// Joins lines in a paragraph, handling hyphenated line wraps (e.g., "trans-\nlation" -> "translation")
    fn join_lines(lines: &[&str]) -> String {
        let mut result = String::new();
        for (i, line) in lines.iter().enumerate() {
            if i > 0 {
                if result.ends_with('-') && !result.ends_with("--") {
                    result.pop(); // Remove trailing hyphen
                } else {
                    result.push(' ');
                }
            }
            result.push_str(line);
        }
        result.trim().to_string()
    }

    /// Heuristic to detect potential headings
    fn detect_heading(text: &str) -> bool {
        let trimmed = text.trim();
        if trimmed.is_empty() || trimmed.len() > 80 {
            return false;
        }

        // Starts with Chapter, Section, or digits + dot
        if trimmed.starts_with("Chapter ")
            || trimmed.starts_with("Section ")
            || trimmed.starts_with("Part ")
        {
            return true;
        }

        // Digits and dots at start like "1. ", "1.1 ", "2.0 "
        if let Some(first_space) = trimmed.find(' ') {
            let prefix = &trimmed[..first_space];
            if prefix.chars().all(|c| c.is_ascii_digit() || c == '.') && prefix.chars().any(|c| c.is_ascii_digit()) {
                return true;
            }
        }

        // Short uppercase title
        if trimmed.len() < 50 && trimmed.chars().all(|c| !c.is_alphabetic() || c.is_uppercase()) {
            return true;
        }

        false
    }
}

pub fn parse_pdf(path: &Path) -> Result<Document, DocumentError> {
    PdfParser::parse_file(path)
}

pub fn parse_pdf_from_bytes(bytes: &[u8], title: Option<&str>) -> Result<Document, DocumentError> {
    PdfParser::parse_bytes(bytes, title, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pdf_invalid_bytes() {
        let bad_bytes = b"NOT A PDF";
        let res = PdfParser::parse_bytes(bad_bytes, None, None);
        assert!(matches!(res, Err(DocumentError::UnsupportedFormat(_))));
    }

    #[test]
    fn test_pdf_empty_bytes() {
        let empty_bytes = b"";
        let res = PdfParser::parse_bytes(empty_bytes, None, None);
        assert!(matches!(res, Err(DocumentError::EmptyDocument)));
    }

    #[test]
    fn test_pdf_join_lines_hyphenation() {
        let lines = ["trans-", "lation", "of", "documents"];
        let joined = PdfParser::join_lines(&lines);
        assert_eq!(joined, "translation of documents");
    }

    #[test]
    fn test_pdf_detect_heading() {
        assert!(PdfParser::detect_heading("Chapter 1: The Beginning"));
        assert!(PdfParser::detect_heading("1.1 Introduction"));
        assert!(PdfParser::detect_heading("ABSTRACT"));
        assert!(!PdfParser::detect_heading("This is a long sentence that should not be classified as a heading because it is too descriptive."));
    }
}
