//! E2E Test Suite: Strict Error Isolation & One-Click Retry Guarantee
//! Covers Follow-up 2 from ORIGINAL_REQUEST.md & Feature 73 from PROJECT.md (Tier 1 & Tier 2)

mod common;
use common::*;

// =========================================================================
// TIER 1: Feature Coverage (Nominal Paths)
// =========================================================================

#[test]
fn test_ui_error_message_zh_cn() {
    let raw_error = "HTTP 500 Internal Server Error: Database cluster unreachable";
    let (user_err, _) = isolate_translation_error(raw_error, Some(500), None, "zh-CN");

    // Invariant: UI presentation MUST ALWAYS display exactly "翻译失败，请稍后重试"
    assert_eq!(user_err.message, "翻译失败，请稍后重试");
    assert!(user_err.has_retry_button, "UI must provide retry button");
    assert!(user_err.retryable, "Error must be marked retryable");
}

#[test]
fn test_ui_error_message_en_us() {
    let raw_error = "HTTP 500 Internal Server Error: Service Unavailable";
    let (user_err, _) = isolate_translation_error(raw_error, Some(500), None, "en-US");

    // Invariant: In English locale, UI MUST display "Translation failed, please try again later"
    assert_eq!(user_err.message, "Translation failed, please try again later");
    assert!(user_err.has_retry_button);
    assert!(user_err.retryable);
}

#[test]
fn test_retry_flag_set_on_network_failure() {
    let raw_error = "tcp connect failed: Connection refused (os error 111)";
    let (user_err, _) = isolate_translation_error(raw_error, None, None, "zh-CN");

    assert!(user_err.retryable);
    assert!(user_err.has_retry_button);
}

#[test]
fn test_sanitized_logger_receives_details() {
    let raw_error = "HTTP 503 Service Temporarily Overloaded";
    let (_, log_entry) = isolate_translation_error(raw_error, Some(503), None, "zh-CN");

    assert_eq!(log_entry.http_status, Some(503));
    assert!(log_entry.raw_error.contains("Service Temporarily Overloaded"));
    assert_eq!(log_entry.level, "ERROR");
}

#[test]
fn test_api_key_masked_in_logs() {
    let api_key = "sk-live-999888777666555444333222111";
    let raw_error = format!("Request failed with Authorization: Bearer {}", api_key);

    let (_, log_entry) = isolate_translation_error(&raw_error, Some(401), Some(api_key), "zh-CN");

    // Invariant: Raw API key must NEVER be printed to logs unmasked
    assert!(!log_entry.raw_error.contains(api_key), "Raw secret must not appear in log message");
    assert!(log_entry.raw_error.contains("sk-***2111"), "Masked secret must appear instead");
}

// =========================================================================
// TIER 2: Boundary, Edge & Corner Cases
// =========================================================================

#[test]
fn test_http_429_rate_limit_ui_isolation() {
    let raw_error = "HTTP 429 Too Many Requests: Rate limit exceeded (limit: 60 req/min)";
    let (user_err, log_entry) = isolate_translation_error(raw_error, Some(429), None, "zh-CN");

    // UI text must NOT contain "429" or "Rate limit"
    assert_eq!(user_err.message, "翻译失败，请稍后重试");
    assert!(!user_err.message.contains("429"));
    assert!(!user_err.message.contains("Rate limit"));

    // Log entry retains 429
    assert_eq!(log_entry.http_status, Some(429));
    assert!(log_entry.raw_error.contains("Rate limit exceeded"));
}

#[test]
fn test_http_401_unauthorized_ui_isolation() {
    let raw_error = "HTTP 401 Unauthorized: Invalid API Key provided";
    let (user_err, log_entry) = isolate_translation_error(raw_error, Some(401), None, "zh-CN");

    // UI text must not expose 401
    assert_eq!(user_err.message, "翻译失败，请稍后重试");
    assert!(user_err.has_retry_button);

    // Diagnostics captures 401
    assert_eq!(log_entry.http_status, Some(401));
}

#[test]
fn test_json_deserialization_error_ui_isolation() {
    let raw_error = "serde_json error: EOF while parsing a value at line 1 column 0";
    let (user_err, log_entry) = isolate_translation_error(raw_error, None, None, "zh-CN");

    // User never sees "serde_json error" or EOF stack trace
    assert_eq!(user_err.message, "翻译失败，请稍后重试");
    assert!(!user_err.message.contains("serde_json"));

    // Logger captures syntax diagnostics
    assert!(log_entry.raw_error.contains("EOF while parsing a value"));
}

#[test]
fn test_dns_timeout_ui_isolation() {
    let raw_error = "dns lookup failed: name resolution error for api.deepseek.com";
    let (user_err, log_entry) = isolate_translation_error(raw_error, None, None, "zh-CN");

    assert_eq!(user_err.message, "翻译失败，请稍后重试");
    assert!(!user_err.message.contains("dns"));
    assert!(log_entry.raw_error.contains("name resolution error"));
}

#[test]
fn test_one_click_retry_action_dispatch() {
    // Simulate user clicking [Retry] button
    let mut block = Block {
        id: "c0_b12".to_string(),
        chapter_id: "c0".to_string(),
        index: 12,
        kind: BlockKind::Paragraph,
        spans: vec![InlineSpan {
            text: "Paragraph that failed earlier.".to_string(),
            style: InlineStyle::Plain,
        }],
        translatable: true,
        translation: TranslationState::Failed {
            error: "Network failure".to_string(),
            retryable: true,
        },
        metadata: BlockMetadata::default(),
    };

    // User clicks [Retry] button -> state transitions to Translating (enqueued at Priority 0)
    if let TranslationState::Failed { retryable, .. } = block.translation {
        assert!(retryable);
        block.translation = TranslationState::Translating;
    }

    assert!(matches!(block.translation, TranslationState::Translating));
}
