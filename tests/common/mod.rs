//! Common test fixtures, contract types, mock generators, and verification helpers
//! for reTranslate opaque-box E2E testing track.

#![allow(dead_code)]

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::PathBuf;

// =========================================================================
// 1. Unified Document AST Contract Types (PROJECT.md § Code Layout & M1 Spec)
// =========================================================================

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DocumentFormat {
    Epub,
    Pdf,
    Docx,
    Markdown,
    Txt,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Chapter {
    pub id: String,
    pub title: String,
    pub order: usize,
    pub blocks: Vec<Block>,
    pub sub_chapters: Vec<Chapter>,
    pub source_href: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BlockMetadata {
    pub page_number: Option<usize>,
    pub bbox: Option<BoundingBox>,
    pub anchor_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
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

impl Block {
    pub fn plain_text(&self) -> String {
        self.spans.iter().map(|s| s.text.as_str()).collect()
    }

    pub fn translation_text(&self) -> Option<&str> {
        match &self.translation {
            TranslationState::Translated { text, .. } => Some(text.as_str()),
            _ => None,
        }
    }
}

// =========================================================================
// 2. Cryptographic Fingerprinting Verification Helper
// =========================================================================

pub fn compute_sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    let result = hasher.finalize();
    hex_encode(&result)
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{:02x}", b)).collect()
}

// =========================================================================
// 3. Numbered JSON Batch Translation Protocol Types & Contract Validator
// =========================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchSegmentInput {
    pub id: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchSegmentOutput {
    pub id: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchTranslationResponse {
    pub segments: Vec<BatchSegmentOutput>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BatchValidationResult {
    Success(Vec<(usize, String)>),
    Mismatch {
        expected_ids: Vec<usize>,
        received_ids: Vec<usize>,
        reason: String,
    },
    ParseError(String),
}

pub fn strip_markdown_fences(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.starts_with("```") {
        let lines: Vec<&str> = trimmed.lines().collect();
        if lines.len() >= 2 && lines.last().map_or(false, |l| l.trim().starts_with("```")) {
            return lines[1..lines.len() - 1].join("\n").trim().to_string();
        }
    }
    trimmed.to_string()
}

pub fn validate_batch_response(
    expected_ids: &[usize],
    raw_response: &str,
) -> BatchValidationResult {
    let clean_json = strip_markdown_fences(raw_response);
    let parsed: BatchTranslationResponse = match serde_json::from_str(&clean_json) {
        Ok(res) => res,
        Err(e) => return BatchValidationResult::ParseError(e.to_string()),
    };

    let received_ids: Vec<usize> = parsed.segments.iter().map(|s| s.id).collect();
    if parsed.segments.len() != expected_ids.len() {
        return BatchValidationResult::Mismatch {
            expected_ids: expected_ids.to_vec(),
            received_ids,
            reason: format!(
                "Count mismatch: expected {} segments, received {}",
                expected_ids.len(),
                parsed.segments.len()
            ),
        };
    }

    for (exp, seg) in expected_ids.iter().zip(&parsed.segments) {
        if *exp != seg.id {
            return BatchValidationResult::Mismatch {
                expected_ids: expected_ids.to_vec(),
                received_ids,
                reason: format!("ID sequence mismatch: expected {}, got {}", exp, seg.id),
            };
        }
    }

    BatchValidationResult::Success(
        parsed
            .segments
            .into_iter()
            .map(|s| (s.id, s.text))
            .collect(),
    )
}

// =========================================================================
// 4. Think-Tag Stripping State Machine (PROJECT.md § Feature 36)
// =========================================================================

#[derive(Debug, Clone, PartialEq, Eq)]
enum ThinkState {
    Passthrough,
    TagOpening(String), // Accumulating '<', '<t', '<th', '<thi', '<thin', '<think>'
    InsideThink,
    TagClosing(String), // Accumulating '</', '</t', '</th', '</thi', '</thin', '</think>'
}

#[derive(Debug, Clone)]
pub struct ThinkFilter {
    state: ThinkState,
}

impl Default for ThinkFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl ThinkFilter {
    pub fn new() -> Self {
        Self {
            state: ThinkState::Passthrough,
        }
    }

    pub fn process_token(&mut self, token: &str) -> String {
        let mut output = String::new();
        for ch in token.chars() {
            match &mut self.state {
                ThinkState::Passthrough => {
                    if ch == '<' {
                        self.state = ThinkState::TagOpening("<".to_string());
                    } else {
                        output.push(ch);
                    }
                }
                ThinkState::TagOpening(buf) => {
                    buf.push(ch);
                    let target = "<think>";
                    if target.starts_with(buf.as_str()) {
                        if buf.as_str() == target {
                            self.state = ThinkState::InsideThink;
                        }
                    } else {
                        // Deviated from <think>, e.g. <table or <stdio.h>
                        output.push_str(buf);
                        self.state = ThinkState::Passthrough;
                    }
                }
                ThinkState::InsideThink => {
                    if ch == '<' {
                        self.state = ThinkState::TagClosing("<".to_string());
                    }
                    // Characters inside think are discarded
                }
                ThinkState::TagClosing(buf) => {
                    buf.push(ch);
                    let target = "</think>";
                    if target.starts_with(buf.as_str()) {
                        if buf.as_str() == target {
                            self.state = ThinkState::Passthrough;
                        }
                    } else {
                        // Deviated from </think>, still inside think
                        self.state = ThinkState::InsideThink;
                    }
                }
            }
        }
        output
    }

    pub fn flush(&mut self) -> String {
        let mut output = String::new();
        match &self.state {
            ThinkState::TagOpening(buf) => {
                output.push_str(buf);
                self.state = ThinkState::Passthrough;
            }
            _ => {}
        }
        output
    }
}

// =========================================================================
// 5. Strict Error Isolation Contract (ORIGINAL_REQUEST.md Follow-up 2)
// =========================================================================

pub const UI_ERROR_MSG_ZH: &str = "翻译失败，请稍后重试";
pub const UI_ERROR_MSG_EN: &str = "Translation failed, please try again later";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserFacingError {
    pub message: String,
    pub has_retry_button: bool,
    pub retryable: bool,
}

#[derive(Debug, Clone)]
pub struct InternalLogEntry {
    pub level: String,
    pub raw_error: String,
    pub http_status: Option<u16>,
    pub duration_ms: u64,
    pub sanitized_url: String,
    pub sanitized_headers: HashMap<String, String>,
}

pub fn isolate_translation_error(
    raw_error: &str,
    http_status: Option<u16>,
    api_key: Option<&str>,
    locale: &str,
) -> (UserFacingError, InternalLogEntry) {
    let ui_message = match locale {
        "en" | "en-US" => UI_ERROR_MSG_EN.to_string(),
        _ => UI_ERROR_MSG_ZH.to_string(),
    };

    let user_error = UserFacingError {
        message: ui_message,
        has_retry_button: true,
        retryable: true,
    };

    // Sanitize API key from logs
    let mut sanitized_error = raw_error.to_string();
    if let Some(key) = api_key {
        if !key.is_empty() {
            let masked = mask_secret(key);
            sanitized_error = sanitized_error.replace(key, &masked);
        }
    }

    let log_entry = InternalLogEntry {
        level: "ERROR".to_string(),
        raw_error: sanitized_error,
        http_status,
        duration_ms: 450,
        sanitized_url: "https://api.openai.com/v1/chat/completions".to_string(),
        sanitized_headers: HashMap::new(),
    };

    (user_error, log_entry)
}

pub fn mask_secret(secret: &str) -> String {
    if secret.len() <= 8 {
        return "***".to_string();
    }
    let prefix = &secret[..3];
    let suffix = &secret[secret.len() - 4..];
    format!("{}***{}", prefix, suffix)
}

// =========================================================================
// 6. Export Mode & Presentation Types (ORIGINAL_REQUEST.md Follow-up 1)
// =========================================================================

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
