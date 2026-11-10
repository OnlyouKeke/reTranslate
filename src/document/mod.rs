pub mod docx;
pub mod epub;
pub mod error;
pub mod markdown;
pub mod model;
pub mod pdf;
pub mod txt;

use std::path::Path;

pub use docx::{parse_docx, parse_docx_from_bytes, DocxParser};
pub use epub::{parse_epub, parse_epub_from_bytes, EpubParser};
pub use error::DocumentError;
pub use markdown::{
    parse_markdown, parse_markdown_from_str, MarkdownParser, MarkdownParserOptions,
};
pub use model::*;
pub use pdf::{parse_pdf, parse_pdf_from_bytes, PdfParser};
pub use txt::{
    detect_and_decode, normalize_newlines, parse_txt, parse_txt_from_bytes,
    segment_txt_paragraphs, TxtParser,
};

/// Unified synchronous document parser contract.
pub trait DocumentParser: Send + Sync {
    /// Parse document directly from file on disk.
    fn parse(&self, path: &Path) -> Result<Document, DocumentError>;

    /// Parse document from raw in-memory byte buffer.
    fn parse_bytes(&self, bytes: &[u8], title: Option<&str>) -> Result<Document, DocumentError>;

    /// Returns list of formats supported by this parser instance.
    fn supported_formats(&self) -> &[DocumentFormat];
}

impl DocumentParser for TxtParser {
    fn parse(&self, path: &Path) -> Result<Document, DocumentError> {
        Self::parse_file(path)
    }

    fn parse_bytes(&self, bytes: &[u8], title: Option<&str>) -> Result<Document, DocumentError> {
        Self::parse_bytes(bytes, title, None)
    }

    fn supported_formats(&self) -> &[DocumentFormat] {
        &[DocumentFormat::Txt]
    }
}

impl DocumentParser for MarkdownParser {
    fn parse(&self, path: &Path) -> Result<Document, DocumentError> {
        Self::parse_file(path)
    }

    fn parse_bytes(&self, bytes: &[u8], title: Option<&str>) -> Result<Document, DocumentError> {
        let content = std::str::from_utf8(bytes)
            .map_err(|e| DocumentError::EncodingError(e.to_string()))?;
        Self::parse_str(content, title, None, None)
    }

    fn supported_formats(&self) -> &[DocumentFormat] {
        &[DocumentFormat::Markdown]
    }
}

impl DocumentParser for EpubParser {
    fn parse(&self, path: &Path) -> Result<Document, DocumentError> {
        Self::parse_file(path)
    }

    fn parse_bytes(&self, bytes: &[u8], title: Option<&str>) -> Result<Document, DocumentError> {
        Self::parse_bytes(bytes, title, None)
    }

    fn supported_formats(&self) -> &[DocumentFormat] {
        &[DocumentFormat::Epub]
    }
}

impl DocumentParser for DocxParser {
    fn parse(&self, path: &Path) -> Result<Document, DocumentError> {
        Self::parse_file(path)
    }

    fn parse_bytes(&self, bytes: &[u8], title: Option<&str>) -> Result<Document, DocumentError> {
        Self::parse_bytes(bytes, title, None)
    }

    fn supported_formats(&self) -> &[DocumentFormat] {
        &[DocumentFormat::Docx]
    }
}

impl DocumentParser for PdfParser {
    fn parse(&self, path: &Path) -> Result<Document, DocumentError> {
        Self::parse_file(path)
    }

    fn parse_bytes(&self, bytes: &[u8], title: Option<&str>) -> Result<Document, DocumentError> {
        Self::parse_bytes(bytes, title, None)
    }

    fn supported_formats(&self) -> &[DocumentFormat] {
        &[DocumentFormat::Pdf]
    }
}

/// Unified document loader that sniffs the file extension, checks existence,
/// and delegates to the appropriate format parser.
pub fn load_document<P: AsRef<Path>>(path: P) -> Result<Document, DocumentError> {
    let path = path.as_ref();
    if !path.exists() {
        return Err(DocumentError::FileNotFound(path.to_path_buf()));
    }

    let format = DocumentFormat::from_path(path).ok_or_else(|| {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("")
            .to_string();
        DocumentError::UnsupportedFormat(ext)
    })?;

    match format {
        DocumentFormat::Epub => epub::parse_epub(path),
        DocumentFormat::Docx => docx::parse_docx(path),
        DocumentFormat::Markdown => markdown::parse_markdown(path),
        DocumentFormat::Txt => txt::parse_txt(path),
        DocumentFormat::Pdf => pdf::parse_pdf(path),
    }
}

/// Unified document loader from memory bytes with specified format.
pub fn load_document_from_bytes(
    bytes: &[u8],
    format: DocumentFormat,
    title: Option<&str>,
) -> Result<Document, DocumentError> {
    if bytes.is_empty() {
        return Err(DocumentError::EmptyDocument);
    }
    match format {
        DocumentFormat::Epub => epub::parse_epub_from_bytes(bytes, title),
        DocumentFormat::Docx => docx::parse_docx_from_bytes(bytes, title),
        DocumentFormat::Markdown => {
            let text = std::str::from_utf8(bytes)
                .map_err(|e| DocumentError::EncodingError(e.to_string()))?;
            markdown::parse_markdown_from_str(text, title)
        }
        DocumentFormat::Txt => txt::parse_txt_from_bytes(bytes, title),
        DocumentFormat::Pdf => pdf::parse_pdf_from_bytes(bytes, title),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_document_from_bytes_markdown_and_txt() {
        let md = "# Doc Title\n\nParagraph content.";
        let doc_md = load_document_from_bytes(md.as_bytes(), DocumentFormat::Markdown, Some("Test MD")).unwrap();
        assert_eq!(doc_md.format, DocumentFormat::Markdown);
        assert_eq!(doc_md.title, "Test MD");
        assert_eq!(doc_md.chapters[0].title, "Doc Title");

        let txt = "Paragraph 1\n\nParagraph 2";
        let doc_txt = load_document_from_bytes(txt.as_bytes(), DocumentFormat::Txt, Some("Test TXT")).unwrap();
        assert_eq!(doc_txt.format, DocumentFormat::Txt);
        assert_eq!(doc_txt.total_blocks, 2);
    }

    #[test]
    fn test_parser_trait_implementations() {
        let txt_parser = TxtParser;
        assert_eq!(txt_parser.supported_formats(), &[DocumentFormat::Txt]);

        let md_parser = MarkdownParser;
        assert_eq!(md_parser.supported_formats(), &[DocumentFormat::Markdown]);

        let epub_parser = EpubParser;
        assert_eq!(epub_parser.supported_formats(), &[DocumentFormat::Epub]);

        let docx_parser = DocxParser;
        assert_eq!(docx_parser.supported_formats(), &[DocumentFormat::Docx]);
    }
}

