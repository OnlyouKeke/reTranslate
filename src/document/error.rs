//! Document parsing and error definitions
use std::path::PathBuf;
use thiserror::Error;

/// Comprehensive error type for document parsing, decoding, and extraction operations.
#[derive(Debug, Error)]
pub enum DocumentError {
    #[error("File not found at path: {0}")]
    FileNotFound(PathBuf),

    #[error("Unsupported document format: {0}")]
    UnsupportedFormat(String),

    #[error("Document package or zip archive is corrupted: {0}")]
    CorruptZip(String),

    #[error("Document file is corrupted: {0}")]
    CorruptFile(String),

    #[error("Book is protected by DRM encryption and cannot be read: {0}")]
    DrmProtected(String),

    #[error("Malformed package structure: {0}")]
    MalformedPackage(String),

    #[error("Invalid DOCX package: {0}")]
    InvalidDocx(String),

    #[error("Text encoding detection or decoding failed: {0}")]
    EncodingError(String),

    #[error("Failed to parse XHTML content: {0}")]
    HtmlParseError(String),

    #[error("Document parse failed: {0}")]
    ParseError(String),

    #[error("Document contains no readable content or blocks")]
    EmptyDocument,

    #[error("I/O error during document access: {0}")]
    Io(#[from] std::io::Error),
}

impl From<zip::result::ZipError> for DocumentError {
    fn from(err: zip::result::ZipError) -> Self {
        DocumentError::CorruptZip(err.to_string())
    }
}

impl DocumentError {
    /// Return user-friendly localized message suitable for presentation in UI dialogs.
    /// In accordance with strict error isolation rules, this hides technical stack traces.
    pub fn user_facing_message(&self, locale: &str) -> String {
        let is_en = locale.starts_with("en");
        match self {
            DocumentError::FileNotFound(path) => {
                if is_en {
                    format!("File not found: {}", path.display())
                } else {
                    format!("文件不存在或已被移动: {}", path.display())
                }
            }
            DocumentError::UnsupportedFormat(ext) => {
                if is_en {
                    format!("Unsupported file format: .{}", ext)
                } else {
                    format!("不支持的文件格式: .{}", ext)
                }
            }
            DocumentError::CorruptZip(_) | DocumentError::CorruptFile(_) => {
                if is_en {
                    "The document file is corrupted and cannot be opened".to_string()
                } else {
                    "文档文件损坏或格式无效，无法正常打开".to_string()
                }
            }
            DocumentError::DrmProtected(_) => {
                if is_en {
                    "This book is protected by DRM encryption and cannot be read".to_string()
                } else {
                    "该电子书受 DRM 版权加密保护，无法解密阅读".to_string()
                }
            }
            DocumentError::MalformedPackage(_) | DocumentError::InvalidDocx(_) => {
                if is_en {
                    "Invalid document structure or package format".to_string()
                } else {
                    "文档结构损坏或文件格式无效，无法解析".to_string()
                }
            }
            DocumentError::EncodingError(_) => {
                if is_en {
                    "Failed to decode text encoding; please check file encoding".to_string()
                } else {
                    "文本编码识别失败，无法解码文件内容".to_string()
                }
            }
            DocumentError::HtmlParseError(_) | DocumentError::ParseError(_) => {
                if is_en {
                    "Failed to parse document content".to_string()
                } else {
                    "文档内容解析失败，请检查文件格式是否规范".to_string()
                }
            }
            DocumentError::EmptyDocument => {
                if is_en {
                    "The document contains no readable text".to_string()
                } else {
                    "文档内容为空，未提取到任何有效段落".to_string()
                }
            }
            DocumentError::Io(e) => {
                if is_en {
                    format!("File I/O error: {}", e)
                } else {
                    format!("文件读取错误: {}", e)
                }
            }
        }
    }

    /// Check if the error is retryable (e.g., temporary I/O locks).
    pub fn is_retryable(&self) -> bool {
        matches!(self, DocumentError::Io(_))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_user_facing_messages_en_and_zh() {
        let err_io = DocumentError::Io(std::io::Error::new(std::io::ErrorKind::NotFound, "disk read"));
        assert!(err_io.user_facing_message("en").contains("File I/O error"));
        assert!(err_io.user_facing_message("zh-CN").contains("文件读取错误"));
        assert!(err_io.is_retryable());

        let err_drm = DocumentError::DrmProtected("Adobe DRM".into());
        assert!(err_drm.user_facing_message("en").contains("DRM encryption"));
        assert!(err_drm.user_facing_message("zh-CN").contains("DRM 版权加密保护"));
        assert!(!err_drm.is_retryable());

        let err_empty = DocumentError::EmptyDocument;
        assert!(err_empty.user_facing_message("en").contains("no readable text"));
        assert!(err_empty.user_facing_message("zh").contains("文档内容为空"));

        let err_corrupt = DocumentError::CorruptFile("crc mismatch".into());
        assert!(err_corrupt.user_facing_message("en").contains("corrupted"));
        assert!(err_corrupt.user_facing_message("zh").contains("文档文件损坏"));

        let err_unsupported = DocumentError::UnsupportedFormat("xyz".into());
        assert!(err_unsupported.user_facing_message("en").contains(".xyz"));
        assert!(err_unsupported.user_facing_message("zh").contains(".xyz"));
    }
}

