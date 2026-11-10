//! Bilingual & Target-Only Multi-Format Document Export Engine
//!
//! Supports exporting translated documents to:
//! - Markdown (.md)
//! - Standalone HTML (.html)
//! - Microsoft Word (.docx)
//! - EPUB Ebook (.epub)
//!
//! With full support for both Bilingual comparison and Target-Only (pure translated) modes.

pub mod docx;
pub mod epub;
pub mod html;
pub mod markdown;

use crate::document::model::Document;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    Epub,
    Docx,
    Markdown,
    Html,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportContentMode {
    Bilingual,
    TargetOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresentationStyle {
    SubtleMuted,
    DottedDivider,
}

#[derive(Debug, Clone)]
pub struct ExportConfig {
    pub format: ExportFormat,
    pub content_mode: ExportContentMode,
    pub style: PresentationStyle,
    pub include_untranslated: bool,
}

impl Default for ExportConfig {
    fn default() -> Self {
        Self {
            format: ExportFormat::Markdown,
            content_mode: ExportContentMode::Bilingual,
            style: PresentationStyle::SubtleMuted,
            include_untranslated: true,
        }
    }
}

pub struct Exporter;

impl Exporter {
    pub fn export_markdown(doc: &Document, mode: ExportContentMode) -> String {
        markdown::export_markdown(doc, mode)
    }

    pub fn export_html(
        doc: &Document,
        mode: ExportContentMode,
        style: PresentationStyle,
    ) -> String {
        html::export_html(doc, mode, style)
    }

    pub fn export_docx(
        doc: &Document,
        mode: ExportContentMode,
        style: PresentationStyle,
    ) -> Result<Vec<u8>, String> {
        docx::export_docx(doc, mode, style)
    }

    pub fn export_epub(
        doc: &Document,
        mode: ExportContentMode,
        style: PresentationStyle,
    ) -> Result<Vec<u8>, String> {
        epub::export_epub(doc, mode, style)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::model::*;

    fn sample_doc() -> Document {
        let block1 = Block {
            id: "c0_b0".to_string(),
            chapter_id: "c0".to_string(),
            index: 0,
            kind: BlockKind::Heading(1),
            spans: vec![InlineSpan {
                text: "Introduction".to_string(),
                style: InlineStyle::Plain,
            }],
            translatable: true,
            translation: TranslationState::Translated {
                text: "引言".to_string(),
                spans: vec![InlineSpan {
                    text: "引言".to_string(),
                    style: InlineStyle::Plain,
                }],
                provider: Some("openai".to_string()),
                model: Some("gpt-4o".to_string()),
                updated_at: 1000,
            },
            metadata: BlockMetadata::default(),
        };

        let block2 = Block {
            id: "c0_b1".to_string(),
            chapter_id: "c0".to_string(),
            index: 1,
            kind: BlockKind::Paragraph,
            spans: vec![InlineSpan {
                text: "Artificial intelligence is changing the world.".to_string(),
                style: InlineStyle::Plain,
            }],
            translatable: true,
            translation: TranslationState::Translated {
                text: "人工智能正在改变世界。".to_string(),
                spans: vec![InlineSpan {
                    text: "人工智能正在改变世界。".to_string(),
                    style: InlineStyle::Plain,
                }],
                provider: Some("openai".to_string()),
                model: Some("gpt-4o".to_string()),
                updated_at: 1000,
            },
            metadata: BlockMetadata::default(),
        };

        Document {
            id: "test_doc".to_string(),
            title: "AI Book".to_string(),
            format: DocumentFormat::Markdown,
            file_path: None,
            file_size: 100,
            metadata: DocumentMetadata::default(),
            chapters: vec![Chapter {
                id: "c0".to_string(),
                title: "Chapter 1".to_string(),
                order: 0,
                blocks: vec![block1, block2],
                sub_chapters: Vec::new(),
                source_href: None,
            }],
            total_blocks: 2,
        }
    }

    #[test]
    fn test_export_markdown_bilingual_and_target_only() {
        let doc = sample_doc();
        let md_bi = Exporter::export_markdown(&doc, ExportContentMode::Bilingual);
        assert!(md_bi.contains("Introduction"));
        assert!(md_bi.contains("引言"));
        assert!(md_bi.contains("Artificial intelligence"));
        assert!(md_bi.contains("人工智能正在改变世界。"));

        let md_target = Exporter::export_markdown(&doc, ExportContentMode::TargetOnly);
        assert!(!md_target.contains("Artificial intelligence"));
        assert!(md_target.contains("人工智能正在改变世界。"));
        assert!(md_target.contains("引言"));
    }

    #[test]
    fn test_export_html_bilingual_and_target_only() {
        let doc = sample_doc();
        let html_bi = Exporter::export_html(&doc, ExportContentMode::Bilingual, PresentationStyle::SubtleMuted);
        assert!(html_bi.contains("<!DOCTYPE html>"));
        assert!(html_bi.contains("trans-text"));
        assert!(html_bi.contains("人工智能正在改变世界。"));

        let html_dotted = Exporter::export_html(&doc, ExportContentMode::Bilingual, PresentationStyle::DottedDivider);
        assert!(html_dotted.contains("border-top: 1px dotted"));

        let html_target = Exporter::export_html(&doc, ExportContentMode::TargetOnly, PresentationStyle::SubtleMuted);
        assert!(html_target.contains("trans-only"));
        assert!(!html_target.contains("Artificial intelligence"));
    }

    #[test]
    fn test_export_docx_valid_zip_bytes() {
        let doc = sample_doc();
        let docx_bytes = Exporter::export_docx(&doc, ExportContentMode::Bilingual, PresentationStyle::SubtleMuted)
            .expect("DOCX export should succeed");
        assert!(docx_bytes.starts_with(b"PK\x03\x04"), "DOCX must be valid zip archive");
    }

    #[test]
    fn test_export_epub_valid_epub_structure() {
        let doc = sample_doc();
        let epub_bytes = Exporter::export_epub(&doc, ExportContentMode::Bilingual, PresentationStyle::SubtleMuted)
            .expect("EPUB export should succeed");
        assert!(epub_bytes.starts_with(b"PK\x03\x04"), "EPUB must be valid zip archive");

        // Verify mimetype is first and uncompressed
        let mut zip = zip::ZipArchive::new(std::io::Cursor::new(&epub_bytes)).unwrap();
        let mimetype = zip.by_name("mimetype").unwrap();
        assert_eq!(mimetype.compression(), zip::CompressionMethod::Stored);
    }
}
