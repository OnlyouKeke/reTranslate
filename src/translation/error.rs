//! Strict Error Isolation Gateway
//!
//! Transforms technical exceptions into friendly user-facing messages
//! while keeping sanitized details for internal logging.

use std::collections::HashMap;
use crate::logging::mask_secret;

pub const UI_ERROR_MSG_ZH: &str = "翻译失败，请稍后重试";
pub const UI_ERROR_MSG_EN: &str = "Translation failed, please try again later";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UserFacingError {
    pub message: String,
    pub has_retry_button: bool,
    pub retryable: bool,
}

impl std::fmt::Display for UserFacingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

impl std::error::Error for UserFacingError {}

#[derive(Debug, Clone)]
pub struct InternalLogEntry {
    pub level: String,
    pub raw_error: String,
    pub http_status: Option<u16>,
    pub duration_ms: u64,
    pub sanitized_url: String,
    pub sanitized_headers: HashMap<String, String>,
}

/// Isolates errors: always gives friendly user message + retry button,
/// while logging sanitized technical details internally.
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
        duration_ms: 0,
        sanitized_url: "https://api.openai.com/v1/chat/completions".to_string(),
        sanitized_headers: HashMap::new(),
    };

    // Log internally
    tracing::error!(
        status = ?http_status,
        error = %log_entry.raw_error,
        "Translation request failed (isolated from user)"
    );

    (user_error, log_entry)
}
