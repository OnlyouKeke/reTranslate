//! Empirical Stress & Boundary Challenge Test Suite for Milestone M0 Iteration 2
//!
//! Scope & Mandate:
//! 1. Multi-key redaction: Test log strings containing 5+ `sk-` keys and 5+ `Bearer ` tokens.
//! 2. Delimiters: Test URL query parameters, escaped JSON, XML attributes, and trailing semicolons.
//! 3. Channel concurrency: Test Tokio-GPUI channel events under high volume (50k+ events).

use retranslate::{redact_sensitive_str, BackgroundEvent};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::unbounded_channel;

// =========================================================================
// Suite 1: Multi-Key Redaction (5+ sk- keys and 5+ Bearer tokens)
// =========================================================================

#[test]
fn test_redact_5_plus_sk_keys_in_single_string() {
    let keys = [
        "sk-1111111111111111",
        "sk-2222222222222222",
        "sk-3333333333333333",
        "sk-4444444444444444",
        "sk-5555555555555555",
        "sk-6666666666666666",
        "sk-7777777777777777",
    ];

    let log_line = format!(
        "Batch failure key pool: k0={}, k1={}, k2={}, k3={}, k4={}, k5={}, k6={}",
        keys[0], keys[1], keys[2], keys[3], keys[4], keys[5], keys[6]
    );

    let redacted = redact_sensitive_str(&log_line);
    println!("Redacted 7 sk- keys: {}", redacted);

    // Verify NONE of the full keys are leaked
    for (i, key) in keys.iter().enumerate() {
        assert!(
            !redacted.contains(key),
            "Key {} ({}) leaked in plain text!",
            i,
            key
        );
        // Verify private middle segment is masked
        let secret_body = &key[3..key.len() - 4];
        assert!(
            !redacted.contains(secret_body),
            "Secret body of key {} ({}) leaked!",
            i,
            secret_body
        );
    }

    // Verify all 7 keys were replaced with expected masked patterns
    assert!(redacted.contains("sk-***1111"));
    assert!(redacted.contains("sk-***2222"));
    assert!(redacted.contains("sk-***3333"));
    assert!(redacted.contains("sk-***4444"));
    assert!(redacted.contains("sk-***5555"));
    assert!(redacted.contains("sk-***6666"));
    assert!(redacted.contains("sk-***7777"));
}

#[test]
fn test_redact_5_plus_bearer_tokens_in_single_string() {
    let tokens = [
        "secret-alpha-11111111",
        "secret-bravo-22222222",
        "secret-charlie-333333",
        "secret-delta-44444444",
        "secret-echo-555555555",
        "secret-foxtrot-666666",
        "secret-golf-777777777",
    ];

    let log_line = format!(
        "Multi-hop proxy headers: h1=Bearer {}, h2=Bearer {}, h3=Bearer {}, h4=Bearer {}, h5=Bearer {}, h6=Bearer {}, h7=Bearer {}",
        tokens[0], tokens[1], tokens[2], tokens[3], tokens[4], tokens[5], tokens[6]
    );

    let redacted = redact_sensitive_str(&log_line);
    println!("Redacted 7 Bearer tokens: {}", redacted);

    // Verify NONE of the full tokens are leaked
    for (i, tok) in tokens.iter().enumerate() {
        assert!(
            !redacted.contains(tok),
            "Bearer token {} ({}) leaked in plain text!",
            i,
            tok
        );
        let secret_body = &tok[3..tok.len() - 4];
        assert!(
            !redacted.contains(secret_body),
            "Secret body of token {} ({}) leaked!",
            i,
            secret_body
        );
    }

    // Verify all 7 tokens were replaced with masked versions
    assert!(redacted.contains("Bearer sec***1111"));
    assert!(redacted.contains("Bearer sec***2222"));
    assert!(redacted.contains("Bearer sec***3333"));
    assert!(redacted.contains("Bearer sec***4444"));
    assert!(redacted.contains("Bearer sec***5555"));
    assert!(redacted.contains("Bearer sec***6666"));
    assert!(redacted.contains("Bearer sec***7777"));
}

#[test]
fn test_redact_interleaved_5_plus_sk_and_5_plus_bearer() {
    let sk_keys = [
        "sk-proj-001-aaaa11112222",
        "sk-proj-002-bbbb22223333",
        "sk-proj-003-cccc33334444",
        "sk-proj-004-dddd44445555",
        "sk-proj-005-eeee55556666",
        "sk-proj-006-ffff66667777",
    ];
    let bearer_tokens = [
        "jwt-alpha-999988887777",
        "jwt-bravo-888877776666",
        "jwt-charlie-777766665555",
        "jwt-delta-666655554444",
        "jwt-echo-555544443333",
        "jwt-foxtrot-444433332222",
    ];

    let mut log_line = String::from("Comprehensive gateway trace: ");
    for i in 0..6 {
        log_line.push_str(&format!(
            "[req_{i}: auth=Bearer {}, key={}; fallback={}] ",
            bearer_tokens[i], sk_keys[i], sk_keys[(i + 1) % 6]
        ));
    }

    let redacted = redact_sensitive_str(&log_line);
    println!("Redacted interleaved log line:\n{}", redacted);

    // Verify all 6 sk_keys and all 6 bearer_tokens are completely masked
    for (i, key) in sk_keys.iter().enumerate() {
        assert!(
            !redacted.contains(key),
            "Interleaved sk- key {} ({}) leaked!",
            i,
            key
        );
    }
    for (i, tok) in bearer_tokens.iter().enumerate() {
        assert!(
            !redacted.contains(tok),
            "Interleaved Bearer token {} ({}) leaked!",
            i,
            tok
        );
    }

    // Verify prefix and suffix integrity
    assert!(redacted.contains("Bearer jwt***7777"));
    assert!(redacted.contains("sk-***2222"));
}

#[test]
fn test_redact_large_scale_100_keys_stress() {
    const COUNT: usize = 50;
    let mut keys = Vec::with_capacity(COUNT);
    let mut tokens = Vec::with_capacity(COUNT);
    let mut log_line = String::with_capacity(10_000);

    for i in 0..COUNT {
        let k = format!("sk-stress-key-{:04}-abcdefghijklmnop", i);
        let t = format!("bearer-token-{:04}-zyxwvutsrqponmlk", i);
        log_line.push_str(&format!("Entry {i}: Bearer {t} | key={k}\n"));
        keys.push(k);
        tokens.push(t);
    }

    let start = std::time::Instant::now();
    let redacted = redact_sensitive_str(&log_line);
    let elapsed = start.elapsed();

    println!(
        "Redacted 100 sensitive tokens in {:?} (string len: {})",
        elapsed,
        log_line.len()
    );

    // Ensure redaction is fast (under 100ms for 100 keys)
    assert!(
        elapsed < Duration::from_millis(100),
        "Redaction took too long: {:?}",
        elapsed
    );

    // Verify all 50 keys and 50 tokens are masked
    for k in &keys {
        assert!(!redacted.contains(k), "Stress key leaked: {}", k);
    }
    for t in &tokens {
        assert!(!redacted.contains(t), "Stress token leaked: {}", t);
    }
}

#[test]
fn test_redact_bearer_token_starting_with_sk() {
    let log_line = "Auth: Bearer sk-proj-1234567890abcdef1234, Backup: Bearer sk-backup-9876543210fedcba";
    let redacted = redact_sensitive_str(log_line);
    println!("Bearer with sk- result: {}", redacted);

    assert!(!redacted.contains("1234567890abcdef1234"));
    assert!(!redacted.contains("9876543210fedcba"));
    assert_eq!(
        redacted,
        "Auth: Bearer sk-***1234, Backup: Bearer sk-***dcba"
    );
}

// =========================================================================
// Suite 2: Delimiters (URL params, escaped JSON, XML, trailing semicolons)
// =========================================================================

#[test]
fn test_delimiter_url_query_parameters_multi_param() {
    let url = "GET /v1/chat?api_key=sk-1234567890abcdef&model=gpt-4o&stream=true&backup_key=sk-0987654321fedcba&format=json HTTP/1.1";
    let redacted = redact_sensitive_str(url);
    println!("Redacted multi-param URL: {}", redacted);

    assert!(!redacted.contains("1234567890abcdef"));
    assert!(!redacted.contains("0987654321fedcba"));
    assert_eq!(
        redacted,
        "GET /v1/chat?api_key=sk-***cdef&model=gpt-4o&stream=true&backup_key=sk-***dcba&format=json HTTP/1.1"
    );
}

#[test]
fn test_delimiter_url_query_parameter_trailing_and_semicolon() {
    // Semicolon delimiter in query string
    let url_semi = "https://api.openai.com/v1/models?api_key=sk-1234567890abcdef;backup=sk-9999888877776666;v=1";
    let redacted_semi = redact_sensitive_str(url_semi);
    assert_eq!(
        redacted_semi,
        "https://api.openai.com/v1/models?api_key=sk-***cdef;backup=sk-***6666;v=1"
    );

    // Key at exact end of URL (no trailing delimiters)
    let url_end = "https://api.openai.com/v1/models?api_key=sk-1234567890abcdef";
    let redacted_end = redact_sensitive_str(url_end);
    assert_eq!(
        redacted_end,
        "https://api.openai.com/v1/models?api_key=sk-***cdef"
    );
}

#[test]
fn test_delimiter_escaped_json_strings() {
    // Escaped quotes in JSON payload: \"sk-...\"
    let raw_escaped = r#"{\"request\": {\"api_key\": \"sk-abcdef1234567890\", \"auth_header\": \"Bearer my-jwt-token-xyz12345\", \"backup\": \"sk-zyxw9876543210\"}}"#;
    let redacted = redact_sensitive_str(raw_escaped);
    println!("Redacted escaped JSON:\n{}", redacted);

    assert!(!redacted.contains("abcdef1234567890"));
    assert!(!redacted.contains("my-jwt-token-xyz12345"));
    assert!(!redacted.contains("zyxw9876543210"));

    assert_eq!(
        redacted,
        r#"{\"request\": {\"api_key\": \"sk-***7890\", \"auth_header\": \"Bearer my-***2345\", \"backup\": \"sk-***3210\"}}"#
    );

    // Compact escaped JSON with no spacing after colon
    let compact_escaped = r#"{\"api_key\":\"sk-1111222233334444\",\"bearer\":\"Bearer tok-5555666677778888\"}"#;
    let red_compact = redact_sensitive_str(compact_escaped);
    assert_eq!(
        red_compact,
        r#"{\"api_key\":\"sk-***4444\",\"bearer\":\"Bearer tok***8888\"}"#
    );
}

#[test]
fn test_delimiter_xml_attributes_and_tags() {
    // XML with double quotes
    let xml_double = r#"<service config="sk-1234567890abcdef" token="Bearer secret-token-12345"/>"#;
    let red_double = redact_sensitive_str(xml_double);
    assert_eq!(
        red_double,
        r#"<service config="sk-***cdef" token="Bearer sec***2345"/>"#
    );

    // XML with single quotes
    let xml_single = "<service config='sk-1234567890abcdef' token='Bearer secret-token-12345'/>";
    let red_single = redact_sensitive_str(xml_single);
    assert_eq!(
        red_single,
        "<service config='sk-***cdef' token='Bearer sec***2345'/>"
    );

    // XML body elements: <Key>sk-...</Key> and <Token>Bearer ...</Token>
    let xml_body = "<Credentials><Key>sk-1234567890abcdef</Key><Token>Bearer secret-token-12345</Token></Credentials>";
    let red_body = redact_sensitive_str(xml_body);
    assert_eq!(
        red_body,
        "<Credentials><Key>sk-***cdef</Key><Token>Bearer sec***2345</Token></Credentials>"
    );
}

#[test]
fn test_delimiter_trailing_semicolons_and_sql() {
    // Trailing semicolon directly attached to secret
    let raw_semi = "Active session secret: sk-1234567890abcdef; next session: Bearer secret-tok-123456;";
    let red_semi = redact_sensitive_str(raw_semi);
    assert_eq!(
        red_semi,
        "Active session secret: sk-***cdef; next session: Bearer sec***3456;"
    );

    // Semicolon in SQL statement
    let sql = "INSERT INTO credentials (key, auth) VALUES ('sk-1234567890abcdef', 'Bearer my-secret-token-123');";
    let red_sql = redact_sensitive_str(sql);
    assert_eq!(
        red_sql,
        "INSERT INTO credentials (key, auth) VALUES ('sk-***cdef', 'Bearer my-***-123');"
    );

    // HTTP Cookie header format
    let cookie = "Cookie: session_id=abc; apikey=sk-1234567890abcdef; auth=Bearer token-xyz-123456; theme=dark";
    let red_cookie = redact_sensitive_str(cookie);
    assert_eq!(
        red_cookie,
        "Cookie: session_id=abc; apikey=sk-***cdef; auth=Bearer tok***3456; theme=dark"
    );
}

#[test]
fn test_delimiter_brackets_parentheses_and_punctuation() {
    let raw = "Tokens: (sk-1111222233334444), [Bearer tok-555566667777], {sk-9999888877776666}, <Bearer tok-121212121212>";
    let redacted = redact_sensitive_str(raw);
    assert_eq!(
        redacted,
        "Tokens: (sk-***4444), [Bearer tok***7777], {sk-***6666}, <Bearer tok***1212>"
    );
}

#[test]
fn test_redaction_boundary_edge_cases() {
    // 1. Secret of length <= 8
    assert_eq!(redact_sensitive_str("sk-1234"), "***");
    assert_eq!(redact_sensitive_str("sk-12345"), "***");
    assert_eq!(redact_sensitive_str("Bearer 12345"), "Bearer ***");

    // 2. Secret exactly at boundaries
    assert_eq!(redact_sensitive_str("sk-123456"), "sk-***3456");

    // 3. Isolated / incomplete prefixes
    assert_eq!(redact_sensitive_str("Just sk- without body"), "Just sk- without body");
    assert_eq!(redact_sensitive_str("Just Bearer"), "Just Bearer");
    assert_eq!(redact_sensitive_str("Bearer "), "Bearer ");

    // 4. Consecutive commas
    let raw_commas = "sk-11111111111111,sk-22222222222222,sk-33333333333333";
    let red_commas = redact_sensitive_str(raw_commas);
    assert_eq!(red_commas, "sk-***1111,sk-***2222,sk-***3333");

    // 5. Mixed whitespace (tabs, newlines)
    let raw_ws = "Key:\tsk-11111111111111\nBearer:\tBearer tok-222222222222\r\nEnd";
    let red_ws = redact_sensitive_str(raw_ws);
    assert_eq!(red_ws, "Key:\tsk-***1111\nBearer:\tBearer tok***2222\r\nEnd");
}

// =========================================================================
// Suite 3: Channel Concurrency (Tokio-GPUI Events Under High Volume)
// =========================================================================

#[tokio::test]
async fn test_channel_high_volume_50k_events_multi_producer() {
    let (tx, mut rx) = unbounded_channel::<BackgroundEvent>();
    const NUM_PRODUCERS: usize = 50;
    const MSGS_PER_PRODUCER: usize = 1_000;
    const TOTAL_MESSAGES: usize = NUM_PRODUCERS * MSGS_PER_PRODUCER;

    let mut handles = Vec::with_capacity(NUM_PRODUCERS);

    for p in 0..NUM_PRODUCERS {
        let tx_clone = tx.clone();
        handles.push(tokio::spawn(async move {
            for m in 0..MSGS_PER_PRODUCER {
                let event = match m % 5 {
                    0 => BackgroundEvent::TranslationStreamingDelta {
                        block_id: format!("p{}_m{}", p, m),
                        delta: format!("chunk_{}", m),
                    },
                    1 => BackgroundEvent::TranslationCompleted {
                        block_id: format!("p{}_m{}", p, m),
                        translated_text: format!("translated text {p}-{m}"),
                        provider: "deepseek".to_string(),
                        model: "deepseek-chat".to_string(),
                    },
                    2 => BackgroundEvent::TranslationFailed {
                        block_id: format!("p{}_m{}", p, m),
                        friendly_message: "翻译失败，请稍后重试".to_string(),
                        retryable: true,
                    },
                    3 => BackgroundEvent::ReadingProgressRestored {
                        chapter_idx: p,
                        block_idx: m,
                        scroll_offset: 120.5,
                    },
                    _ => BackgroundEvent::DocumentLoadFailed {
                        error: format!("Simulated I/O failure on worker {p} step {m}"),
                    },
                };
                tx_clone.send(event).expect("Send must succeed under high volume");
            }
        }));
    }

    // Drop original tx so receiver knows all producers are done once handles finish
    drop(tx);

    let mut total_received = 0usize;
    let mut variant_counts = [0usize; 5]; // delta, completed, failed, progress, doc_load_fail

    while let Some(event) = rx.recv().await {
        total_received += 1;
        match event {
            BackgroundEvent::TranslationStreamingDelta { .. } => variant_counts[0] += 1,
            BackgroundEvent::TranslationCompleted { .. } => variant_counts[1] += 1,
            BackgroundEvent::TranslationFailed { .. } => variant_counts[2] += 1,
            BackgroundEvent::ReadingProgressRestored { .. } => variant_counts[3] += 1,
            BackgroundEvent::DocumentLoadFailed { .. } => variant_counts[4] += 1,
            BackgroundEvent::DocumentLoaded { .. } => panic!("Unexpected DocumentLoaded in test"),
        }
    }

    for h in handles {
        h.await.expect("Producer task failed");
    }

    assert_eq!(
        total_received, TOTAL_MESSAGES,
        "All 50,000 events must be received without any message drops"
    );
    assert_eq!(variant_counts[0], 10_000);
    assert_eq!(variant_counts[1], 10_000);
    assert_eq!(variant_counts[2], 10_000);
    assert_eq!(variant_counts[3], 10_000);
    assert_eq!(variant_counts[4], 10_000);
}

#[tokio::test]
async fn test_channel_burst_saturation_before_drain() {
    let (tx, mut rx) = unbounded_channel::<BackgroundEvent>();
    const BURST_COUNT: usize = 20_000;

    // Burst 20,000 events into the channel BEFORE reading any
    for i in 0..BURST_COUNT {
        tx.send(BackgroundEvent::TranslationStreamingDelta {
            block_id: format!("burst_{}", i),
            delta: format!("d_{}", i),
        })
        .expect("Burst send must not fail on unbounded channel");
    }

    drop(tx);

    let mut drain_count = 0;
    while let Some(event) = rx.recv().await {
        if let BackgroundEvent::TranslationStreamingDelta { block_id, delta } = event {
            assert_eq!(block_id, format!("burst_{}", drain_count));
            assert_eq!(delta, format!("d_{}", drain_count));
            drain_count += 1;
        } else {
            panic!("Unexpected event type");
        }
    }

    assert_eq!(
        drain_count, BURST_COUNT,
        "All 20,000 pre-buffered burst events must be drained in exact FIFO order"
    );
}

#[tokio::test]
async fn test_channel_document_load_failed_event_diagnostics() {
    let (tx, mut rx) = unbounded_channel::<BackgroundEvent>();

    let errors = [
        "Failed to open EPUB archive: invalid zip header in /path/to/doc.epub",
        "Permission denied (os error 13): cannot read /etc/restricted.pdf",
        "Malformed XML in chapter_01.xhtml: unclosed <p> at byte 4096",
        "Corrupted DOCX document: word/document.xml missing",
    ];

    for err in &errors {
        tx.send(BackgroundEvent::DocumentLoadFailed {
            error: err.to_string(),
        })
        .expect("Send failed");
    }
    drop(tx);

    let mut received_errors = Vec::new();
    while let Some(event) = rx.recv().await {
        match event {
            BackgroundEvent::DocumentLoadFailed { error } => {
                // Verify detailed diagnostic information is fully preserved
                received_errors.push(error);
            }
            other => panic!("Unexpected event: {:?}", other),
        }
    }

    assert_eq!(received_errors.len(), errors.len());
    for (received, expected) in received_errors.iter().zip(errors.iter()) {
        assert_eq!(received, expected);
    }
}

#[tokio::test]
async fn test_channel_producer_cancellation_and_clean_termination() {
    let (tx, mut rx) = unbounded_channel::<BackgroundEvent>();
    let cancelled_flag = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();
    for p in 0..10 {
        let tx_clone = tx.clone();
        let cancel_cnt = cancelled_flag.clone();
        handles.push(tokio::spawn(async move {
            let mut i = 0;
            loop {
                let res = tx_clone.send(BackgroundEvent::TranslationStreamingDelta {
                    block_id: format!("p{}_{}", p, i),
                    delta: "chunk".to_string(),
                });
                if res.is_err() {
                    // Receiver was dropped, sender cleanly detects channel closed
                    cancel_cnt.fetch_add(1, Ordering::Relaxed);
                    break;
                }
                i += 1;
                tokio::task::yield_now().await;
            }
        }));
    }
    drop(tx);

    // Read only 500 events then drop receiver
    let mut count = 0;
    while let Some(_event) = rx.recv().await {
        count += 1;
        if count >= 500 {
            break;
        }
    }
    drop(rx); // Drop receiver while producers are still sending

    for h in handles {
        h.await.expect("Producer task failed");
    }

    // All 10 producers must detect the closed channel and terminate gracefully
    assert_eq!(
        cancelled_flag.load(Ordering::Relaxed),
        10,
        "All 10 producers must exit cleanly upon receiver drop without panicking"
    );
}
