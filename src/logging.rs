use std::path::Path;
use std::sync::{Mutex, Once};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

static INIT_ONCE: Once = Once::new();
static WORKER_GUARD: Mutex<Option<WorkerGuard>> = Mutex::new(None);

/// Mask a secret string (e.g. API key or Bearer token), preserving prefix and suffix
pub fn mask_secret(secret: &str) -> String {
    if secret.len() <= 8 {
        return "***".to_string();
    }
    if secret.is_char_boundary(3) && secret.is_char_boundary(secret.len() - 4) {
        let prefix = &secret[..3];
        let suffix = &secret[secret.len() - 4..];
        format!("{}***{}", prefix, suffix)
    } else {
        "***".to_string()
    }
}

/// Helper to determine if a character serves as a token delimiter
#[inline]
fn is_token_delimiter(c: char) -> bool {
    c.is_whitespace()
        || matches!(
            c,
            '"' | '\'' | ',' | '}' | ']' | ')' | '>' | '<' | '\\' | '&' | ';'
        )
}

/// Redact sensitive tokens (such as Bearer tokens and sk-* API keys) from text
pub fn redact_sensitive_str(input: &str) -> String {
    let mut result = input.to_string();

    // 1. Redact Bearer tokens: "Bearer <token>"
    let mut search_from = 0;
    while let Some(idx) = result[search_from..].find("Bearer ") {
        let start = search_from + idx + 7;
        let end = result[start..]
            .find(|c: char| is_token_delimiter(c))
            .map(|i| start + i)
            .unwrap_or(result.len());
        if end > start {
            let token = &result[start..end];
            if token.contains("***") {
                search_from = end;
                continue;
            }
            let masked = mask_secret(token);
            result.replace_range(start..end, &masked);
            search_from = start + masked.len();
        } else {
            search_from = start;
        }
    }

    // 2. Redact sk- patterns: "sk-..."
    let mut search_from = 0;
    while let Some(idx) = result[search_from..].find("sk-") {
        let start = search_from + idx;
        let end = result[start..]
            .find(|c: char| is_token_delimiter(c))
            .map(|i| start + i)
            .unwrap_or(result.len());
        if end > start + 3 {
            let token = &result[start..end];
            if token.contains("***") {
                search_from = end;
                continue;
            }
            let masked = mask_secret(token);
            result.replace_range(start..end, &masked);
            search_from = start + masked.len();
        } else {
            search_from = start + 3;
        }
    }

    result
}

/// Initialize tracing subscriber with daily rolling file appender and console layer
pub fn init() {
    init_with_dir("logs");
}

/// Initialize tracing subscriber with a custom log directory
pub fn init_with_dir<P: AsRef<Path>>(log_dir: P) {
    INIT_ONCE.call_once(|| {
        let dir = log_dir.as_ref();
        let _ = std::fs::create_dir_all(dir);

        let file_appender = tracing_appender::rolling::daily(dir, "app.log");
        let (non_blocking, guard) = tracing_appender::non_blocking(file_appender);

        if let Ok(mut g) = WORKER_GUARD.lock() {
            *g = Some(guard);
        }

        let env_filter = EnvFilter::try_from_default_env()
            .unwrap_or_else(|_| EnvFilter::new("retranslate=info,gpui=warn"));

        let console_layer = tracing_subscriber::fmt::layer()
            .with_target(false)
            .with_ansi(true);

        let file_layer = tracing_subscriber::fmt::layer()
            .with_ansi(false)
            .with_writer(non_blocking);

        let _ = tracing_subscriber::registry()
            .with(env_filter)
            .with(console_layer)
            .with(file_layer)
            .try_init();

        tracing::info!("Tracing logging subsystem initialized");
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_secret() {
        assert_eq!(mask_secret("1234567"), "***");
        assert_eq!(mask_secret("12345678"), "***");
        assert_eq!(mask_secret("sk-1234567890"), "sk-***7890");
        assert_eq!(
            mask_secret("sk-live-999888777666555444333222111"),
            "sk-***2111"
        );
    }

    #[test]
    fn test_mask_secret_utf8_safety() {
        assert_eq!(mask_secret(""), "***");
        assert_eq!(mask_secret("hello"), "***");
        assert_eq!(mask_secret("🔑🔑🔑🔑🔑🔑🔑🔑🔑"), "***");
        assert_eq!(mask_secret("sk-1234567890"), "sk-***7890");
    }

    #[test]
    fn test_redact_sensitive_str_bearer() {
        let raw = "Authorization: Bearer my-secret-jwt-token-abcdef123456";
        let redacted = redact_sensitive_str(raw);
        assert!(!redacted.contains("my-secret-jwt-token-abcdef123456"));
        assert!(redacted.contains("Bearer my-***3456"));
    }

    #[test]
    fn test_redact_sensitive_str_sk_key() {
        let raw = "Request failed with key: sk-live-999888777666555444333222111 in body";
        let redacted = redact_sensitive_str(raw);
        assert!(!redacted.contains("sk-live-999888777666555444333222111"));
        assert!(redacted.contains("sk-***2111"));
    }

    #[test]
    fn test_redact_multiple_sk_keys() {
        let raw = "Primary key sk-11111111111111 failed; retrying with backup sk-22222222222222";
        let redacted = redact_sensitive_str(raw);
        assert!(!redacted.contains("11111111111111"));
        assert!(!redacted.contains("22222222222222"));
        assert_eq!(
            redacted,
            "Primary key sk-***1111 failed; retrying with backup sk-***2222"
        );

        let three_keys = "k1: sk-aaaaaaaaaaaaaa, k2: sk-bbbbbbbbbbbbbb, k3: sk-cccccccccccccc";
        let r3 = redact_sensitive_str(three_keys);
        assert_eq!(
            r3,
            "k1: sk-***aaaa, k2: sk-***bbbb, k3: sk-***cccc"
        );
    }

    #[test]
    fn test_redact_multiple_bearer_tokens() {
        let raw = "Headers: Authorization: Bearer secret-tok-aaa111, Proxy-Auth: Bearer secret-tok-bbb222";
        let redacted = redact_sensitive_str(raw);
        assert!(!redacted.contains("secret-tok-aaa111"));
        assert!(!redacted.contains("secret-tok-bbb222"));
        assert_eq!(
            redacted,
            "Headers: Authorization: Bearer sec***a111, Proxy-Auth: Bearer sec***b222"
        );
    }

    #[test]
    fn test_redact_query_param_delimiters() {
        // Delimiter '&'
        let url_amp = "GET https://api.openai.com/v1/models?api_key=sk-1234567890abcdef&format=json";
        let r_amp = redact_sensitive_str(url_amp);
        assert_eq!(
            r_amp,
            "GET https://api.openai.com/v1/models?api_key=sk-***cdef&format=json"
        );

        // Delimiter ';'
        let url_semi = "GET https://api.openai.com/v1/models?api_key=sk-1234567890abcdef;format=json";
        let r_semi = redact_sensitive_str(url_semi);
        assert_eq!(
            r_semi,
            "GET https://api.openai.com/v1/models?api_key=sk-***cdef;format=json"
        );

        // Delimiter ')'
        let url_paren = "(https://api.openai.com/v1?api_key=sk-1234567890abcdef)";
        let r_paren = redact_sensitive_str(url_paren);
        assert_eq!(
            r_paren,
            "(https://api.openai.com/v1?api_key=sk-***cdef)"
        );

        // Delimiter '>'
        let url_angle = "<https://api.openai.com/v1?api_key=sk-1234567890abcdef>";
        let r_angle = redact_sensitive_str(url_angle);
        assert_eq!(
            r_angle,
            "<https://api.openai.com/v1?api_key=sk-***cdef>"
        );
    }

    #[test]
    fn test_redact_bearer_with_sk_key() {
        let raw = "Authorization: Bearer sk-11111111111111, Backup: sk-22222222222222";
        let redacted = redact_sensitive_str(raw);
        assert_eq!(
            redacted,
            "Authorization: Bearer sk-***1111, Backup: sk-***2222"
        );
    }

    #[test]
    fn test_redact_edge_cases() {
        let raw = "Bare sk- alone, Bearer , and short sk-123 key";
        let redacted = redact_sensitive_str(raw);
        assert_eq!(redacted, "Bare sk- alone, Bearer , and short *** key");
    }
}
