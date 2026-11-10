use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

// =========================================================================
// Cryptographic Fingerprinting Helpers
// =========================================================================

/// Compute 64-character lowercase hexadecimal SHA-256 hash of a byte slice.
pub fn compute_sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let result = hasher.finalize();
    result.iter().map(|b| format!("{:02x}", b)).collect()
}

/// Compute 64-character lowercase hexadecimal SHA-256 hash of a file on disk.
pub fn compute_sha256_file<P: AsRef<Path>>(path: P) -> io::Result<String> {
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    let result = hasher.finalize();
    Ok(result.iter().map(|b| format!("{:02x}", b)).collect())
}

// =========================================================================
// Unified Document AST Contract Types
// =========================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DocumentFormat {
    Epub,
    Pdf,
    Docx,
    Markdown,
    Txt,
}

impl DocumentFormat {
    /// Detect document format from file extension.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext.to_ascii_lowercase().trim_start_matches('.').as_ref() {
            "epub" => Some(Self::Epub),
            "docx" => Some(Self::Docx),
            "md" | "markdown" => Some(Self::Markdown),
            "txt" | "text" => Some(Self::Txt),
            "pdf" => Some(Self::Pdf),
            _ => None,
        }
    }

    /// Detect document format from file path.
    pub fn from_path<P: AsRef<Path>>(path: P) -> Option<Self> {
        path.as_ref()
            .extension()
            .and_then(|ext| ext.to_str())
            .and_then(Self::from_extension)
    }

    /// Standard file extension without dot.
    pub fn extension(&self) -> &'static str {
        match self {
            Self::Epub => "epub",
            Self::Docx => "docx",
            Self::Markdown => "md",
            Self::Txt => "txt",
            Self::Pdf => "pdf",
        }
    }

    /// Standard MIME content type.
    pub fn mime_type(&self) -> &'static str {
        match self {
            Self::Epub => "application/epub+zip",
            Self::Docx => "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
            Self::Markdown => "text/markdown",
            Self::Txt => "text/plain",
            Self::Pdf => "application/pdf",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct DocumentMetadata {
    pub author: Option<String>,
    pub language: Option<String>,
    pub publisher: Option<String>,
    pub description: Option<String>,
    pub created_at: Option<String>,
    pub page_count: Option<usize>,
    pub cover_image: Option<Vec<u8>>,
    pub cover_mime: Option<String>,
    pub extra: HashMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub id: String, // 64-char lowercase hex SHA-256
    pub title: String,
    pub format: DocumentFormat,
    pub file_path: Option<PathBuf>,
    pub file_size: u64,
    pub metadata: DocumentMetadata,
    pub chapters: Vec<Chapter>,
    pub total_blocks: usize,
}

impl Document {
    pub fn new(id: String, title: String, format: DocumentFormat) -> Self {
        Self {
            id,
            title,
            format,
            file_path: None,
            file_size: 0,
            metadata: DocumentMetadata::default(),
            chapters: Vec::new(),
            total_blocks: 0,
        }
    }

    pub fn recalculate_total_blocks(&mut self) {
        self.total_blocks = self.chapters.iter().map(|c| c.total_blocks()).sum();
    }

    /// Recursively search for a block by ID across all chapters and sub-chapters.
    pub fn find_block(&self, block_id: &str) -> Option<&Block> {
        self.chapters.iter().find_map(|c| c.find_block(block_id))
    }

    /// Recursively search for a mutable reference to a block by ID across all chapters and sub-chapters.
    pub fn find_block_mut(&mut self, block_id: &str) -> Option<&mut Block> {
        self.chapters.iter_mut().find_map(|c| c.find_block_mut(block_id))
    }

    /// Iterator over all blocks in the document across chapters and nested sub-chapters.
    pub fn iter_blocks(&self) -> Box<dyn Iterator<Item = &Block> + '_> {
        Box::new(self.chapters.iter().flat_map(|c| c.iter_blocks()))
    }

    /// Mutable iterator over all blocks in the document across chapters and nested sub-chapters.
    pub fn iter_blocks_mut(&mut self) -> Box<dyn Iterator<Item = &mut Block> + '_> {
        Box::new(self.chapters.iter_mut().flat_map(|c| c.iter_blocks_mut()))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Chapter {
    pub id: String,
    pub title: String,
    pub order: usize,
    pub blocks: Vec<Block>,
    pub sub_chapters: Vec<Chapter>,
    pub source_href: Option<String>,
}

impl Chapter {
    pub fn total_blocks(&self) -> usize {
        self.blocks.len() + self.sub_chapters.iter().map(|s| s.total_blocks()).sum::<usize>()
    }

    /// Recursively search for a block by ID within this chapter and its sub-chapters.
    pub fn find_block(&self, block_id: &str) -> Option<&Block> {
        if let Some(b) = self.blocks.iter().find(|b| b.id == block_id) {
            return Some(b);
        }
        for sub in &self.sub_chapters {
            if let Some(b) = sub.find_block(block_id) {
                return Some(b);
            }
        }
        None
    }

    /// Recursively search for a mutable reference to a block by ID within this chapter and its sub-chapters.
    pub fn find_block_mut(&mut self, block_id: &str) -> Option<&mut Block> {
        if let Some(b) = self.blocks.iter_mut().find(|b| b.id == block_id) {
            return Some(b);
        }
        for sub in &mut self.sub_chapters {
            if let Some(b) = sub.find_block_mut(block_id) {
                return Some(b);
            }
        }
        None
    }

    /// Iterator over all blocks in this chapter and all nested sub-chapters.
    pub fn iter_blocks(&self) -> Box<dyn Iterator<Item = &Block> + '_> {
        let direct = self.blocks.iter();
        let nested = self.sub_chapters.iter().flat_map(|sub| sub.iter_blocks());
        Box::new(direct.chain(nested))
    }

    /// Mutable iterator over all blocks in this chapter and all nested sub-chapters.
    pub fn iter_blocks_mut(&mut self) -> Box<dyn Iterator<Item = &mut Block> + '_> {
        let direct = self.blocks.iter_mut();
        let nested = self.sub_chapters.iter_mut().flat_map(|sub| sub.iter_blocks_mut());
        Box::new(direct.chain(nested))
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Block {
    pub id: String, // format: "{chapter_id}_b{index}"
    pub chapter_id: String,
    pub index: usize,
    pub kind: BlockKind,
    pub spans: Vec<InlineSpan>,
    pub translatable: bool,
    pub translation: TranslationState,
    pub metadata: BlockMetadata,
}

impl Block {
    pub fn new_paragraph(id: String, chapter_id: String, index: usize, text: String) -> Self {
        Self {
            id,
            chapter_id,
            index,
            kind: BlockKind::Paragraph,
            spans: vec![InlineSpan {
                text,
                style: InlineStyle::Plain,
            }],
            translatable: true,
            translation: TranslationState::Untranslated,
            metadata: BlockMetadata::default(),
        }
    }

    pub fn new_heading(id: String, chapter_id: String, index: usize, level: u8, text: String) -> Self {
        Self {
            id,
            chapter_id,
            index,
            kind: BlockKind::Heading(level),
            spans: vec![InlineSpan {
                text,
                style: InlineStyle::Plain,
            }],
            translatable: true,
            translation: TranslationState::Untranslated,
            metadata: BlockMetadata::default(),
        }
    }

    pub fn new_code_block(
        id: String,
        chapter_id: String,
        index: usize,
        language: Option<String>,
        code: String,
    ) -> Self {
        Self {
            id,
            chapter_id,
            index,
            kind: BlockKind::CodeBlock { language, code },
            spans: vec![],
            translatable: false, // Invariant: code blocks are strictly not translatable
            translation: TranslationState::Untranslated,
            metadata: BlockMetadata::default(),
        }
    }

    pub fn new_quote(id: String, chapter_id: String, index: usize, text: String) -> Self {
        Self {
            id,
            chapter_id,
            index,
            kind: BlockKind::Quote,
            spans: vec![InlineSpan {
                text,
                style: InlineStyle::Plain,
            }],
            translatable: true,
            translation: TranslationState::Untranslated,
            metadata: BlockMetadata::default(),
        }
    }

    pub fn new_image(
        id: String,
        chapter_id: String,
        index: usize,
        src: String,
        alt: Option<String>,
        data: Option<Vec<u8>>,
        mime: Option<String>,
    ) -> Self {
        Self {
            id,
            chapter_id,
            index,
            kind: BlockKind::Image {
                src,
                alt,
                data,
                mime,
                width: None,
                height: None,
            },
            spans: vec![],
            translatable: false,
            translation: TranslationState::Untranslated,
            metadata: BlockMetadata::default(),
        }
    }

    pub fn plain_text(&self) -> String {
        if self.spans.is_empty() {
            match &self.kind {
                BlockKind::CodeBlock { code, .. } => code.clone(),
                _ => String::new(),
            }
        } else {
            self.spans.iter().map(|s| s.text.as_str()).collect()
        }
    }

    pub fn translation_text(&self) -> Option<&str> {
        match &self.translation {
            TranslationState::Translated { text, .. } => Some(text.as_str()),
            _ => None,
        }
    }

    pub fn is_translated(&self) -> bool {
        matches!(self.translation, TranslationState::Translated { .. })
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct BlockMetadata {
    pub page_number: Option<usize>,
    pub bbox: Option<BoundingBox>,
    pub anchor_id: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BoundingBox {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum BlockKind {
    Paragraph,
    Heading(u8),
    CodeBlock {
        language: Option<String>,
        code: String,
    },
    Quote,
    ListItem {
        order: Option<u64>,
        depth: u8,
    },
    Table {
        headers: Vec<TableCell>,
        rows: Vec<Vec<TableCell>>,
    },
    Image {
        src: String,
        alt: Option<String>,
        data: Option<Vec<u8>>,
        mime: Option<String>,
        width: Option<u32>,
        height: Option<u32>,
    },
    ThematicBreak,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TableCell {
    pub spans: Vec<InlineSpan>,
    pub col_span: usize,
    pub row_span: usize,
    pub align: TableAlignment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TableAlignment {
    Left,
    Center,
    Right,
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InlineSpan {
    pub text: String,
    pub style: InlineStyle,
}

impl InlineSpan {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: InlineStyle::Plain,
        }
    }

    pub fn bold(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: InlineStyle::Bold,
        }
    }

    pub fn italic(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: InlineStyle::Italic,
        }
    }

    pub fn code(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: InlineStyle::Code,
        }
    }

    pub fn link(text: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            style: InlineStyle::Link { url: url.into() },
        }
    }

    pub fn footnote(text: impl Into<String>, id: impl Into<String>, note: Option<String>) -> Self {
        Self {
            text: text.into(),
            style: InlineStyle::FootnoteRef {
                id: id.into(),
                note,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum InlineStyle {
    Plain,
    Bold,
    Italic,
    BoldItalic,
    Code,
    Strikethrough,
    Underline,
    Link { url: String },
    FootnoteRef { id: String, note: Option<String> },
    Subscript,
    Superscript,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TranslationState {
    Untranslated,
    Translating,
    Translated {
        text: String,
        spans: Vec<InlineSpan>,
        provider: Option<String>,
        model: Option<String>,
        updated_at: u64,
    },
    Failed {
        error: String,
        retryable: bool,
    },
}

/// In-memory model wrapper for reactive UI operations
#[derive(Debug, Clone)]
pub struct DocumentModel {
    pub document: Document,
}

impl DocumentModel {
    pub fn new(document: Document) -> Self {
        Self { document }
    }

    pub fn set_block_translated(
        &mut self,
        block_id: &str,
        text: String,
        provider: String,
        model: String,
    ) -> bool {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if let Some(block) = self.document.find_block_mut(block_id) {
            block.translation = TranslationState::Translated {
                text: text.clone(),
                spans: vec![InlineSpan {
                    text,
                    style: InlineStyle::Plain,
                }],
                provider: Some(provider),
                model: Some(model),
                updated_at: now,
            };
            true
        } else {
            false
        }
    }

    pub fn set_block_failed(&mut self, block_id: &str, error: String, retryable: bool) -> bool {
        if let Some(block) = self.document.find_block_mut(block_id) {
            block.translation = TranslationState::Failed { error, retryable };
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compute_sha256_hex() {
        assert_eq!(
            compute_sha256_hex(b"hello world"),
            "b94d27b9934d3e08a52e52d7da7dabfac484efe37a5380ee9088f7ace2efcde9"
        );
        assert_eq!(
            compute_sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn test_sub_chapter_find_and_traverse() {
        let mut doc = Document::new("doc1".into(), "Title".into(), DocumentFormat::Epub);
        let sub = Chapter {
            id: "c1_sub1".into(),
            title: "Sub Section".into(),
            order: 0,
            blocks: vec![Block::new_paragraph(
                "c1_sub1_b0".into(),
                "c1_sub1".into(),
                0,
                "Nested content".into(),
            )],
            sub_chapters: vec![],
            source_href: None,
        };
        let chap = Chapter {
            id: "c1".into(),
            title: "Chap 1".into(),
            order: 0,
            blocks: vec![Block::new_heading(
                "c1_b0".into(),
                "c1".into(),
                0,
                1,
                "Heading 1".into(),
            )],
            sub_chapters: vec![sub],
            source_href: None,
        };
        doc.chapters.push(chap);
        doc.recalculate_total_blocks();

        assert_eq!(doc.total_blocks, 2);
        assert!(doc.find_block("c1_sub1_b0").is_some());
        assert_eq!(doc.find_block("c1_sub1_b0").unwrap().plain_text(), "Nested content");

        let mut model = DocumentModel::new(doc);
        assert!(model.set_block_translated("c1_sub1_b0", "嵌套内容".into(), "deepseek".into(), "chat".into()));
        assert_eq!(
            model.document.find_block("c1_sub1_b0").unwrap().translation_text(),
            Some("嵌套内容")
        );
    }

    #[test]
    fn test_document_format_helpers() {
        assert_eq!(DocumentFormat::from_extension("epub"), Some(DocumentFormat::Epub));
        assert_eq!(DocumentFormat::from_extension(".DOCX"), Some(DocumentFormat::Docx));
        assert_eq!(DocumentFormat::from_extension("md"), Some(DocumentFormat::Markdown));
        assert_eq!(DocumentFormat::from_extension("txt"), Some(DocumentFormat::Txt));
        assert_eq!(DocumentFormat::from_extension("pdf"), Some(DocumentFormat::Pdf));
        assert_eq!(DocumentFormat::from_extension("unknown"), None);

        assert_eq!(DocumentFormat::from_path("book.epub"), Some(DocumentFormat::Epub));
        assert_eq!(DocumentFormat::Markdown.extension(), "md");
        assert_eq!(DocumentFormat::Txt.mime_type(), "text/plain");
    }
}
