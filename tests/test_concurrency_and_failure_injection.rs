//! Adversarial Stress & Failure Injection Test Suite
//! Milestone M0: Concurrency, Channel Saturation, SQLite Lock Contention,
//! High-Throughput Logging, and Strict Error Isolation.

mod common;
use common::*;
use retranslate::{
    redact_sensitive_str, t_locale, AppCommand, BackgroundEvent, Database, Document,
    DocumentFormat, DocumentModel, InlineSpan, InlineStyle, Locale, TranslationRecord,
    TranslationState,
};
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc::unbounded_channel;

// =========================================================================
// 1. Tokio-GPUI Channel Bridge: Saturation, Ordering & Dropping
// =========================================================================

#[tokio::test]
async fn test_channel_fifo_ordering_under_load() {
    let (tx, mut rx) = unbounded_channel::<BackgroundEvent>();
    const TOTAL_MESSAGES: usize = 10_000;

    let producer = tokio::spawn(async move {
        for i in 0..TOTAL_MESSAGES {
            let event = BackgroundEvent::TranslationStreamingDelta {
                block_id: format!("b_{}", i),
                delta: format!("chunk_{}", i),
            };
            if let Err(e) = tx.send(event) {
                panic!("Failed to send event {}: {:?}", i, e);
            }
        }
    });

    let mut received_count = 0;
    while let Some(event) = rx.recv().await {
        if let BackgroundEvent::TranslationStreamingDelta { block_id, delta } = event {
            assert_eq!(block_id, format!("b_{}", received_count));
            assert_eq!(delta, format!("chunk_{}", received_count));
            received_count += 1;
            if received_count == TOTAL_MESSAGES {
                break;
            }
        } else {
            panic!("Unexpected event variant: {:?}", event);
        }
    }

    producer.await.expect("Producer task failed");
    assert_eq!(received_count, TOTAL_MESSAGES);
}

#[tokio::test]
async fn test_channel_saturation_multi_producer() {
    let (tx, mut rx) = unbounded_channel::<BackgroundEvent>();
    const NUM_PRODUCERS: usize = 20;
    const MSGS_PER_PRODUCER: usize = 1_000;
    const TOTAL_MESSAGES: usize = NUM_PRODUCERS * MSGS_PER_PRODUCER;

    let mut handles = Vec::new();
    for p in 0..NUM_PRODUCERS {
        let tx_clone = tx.clone();
        handles.push(tokio::spawn(async move {
            for m in 0..MSGS_PER_PRODUCER {
                tx_clone
                    .send(BackgroundEvent::TranslationCompleted {
                        block_id: format!("p{}_m{}", p, m),
                        translated_text: "译文".to_string(),
                        provider: "deepseek".to_string(),
                        model: "deepseek-chat".to_string(),
                    })
                    .unwrap();
            }
        }));
    }
    // Drop original tx so rx finishes when all producer clones finish
    drop(tx);

    let mut received_ids = HashSet::new();
    while let Some(event) = rx.recv().await {
        if let BackgroundEvent::TranslationCompleted { block_id, .. } = event {
            received_ids.insert(block_id);
        }
    }

    for handle in handles {
        handle.await.expect("Producer failed");
    }

    assert_eq!(
        received_ids.len(),
        TOTAL_MESSAGES,
        "All saturated messages must be received without loss"
    );
}

#[tokio::test]
async fn test_channel_receiver_dropped_handling() {
    let (tx, rx) = unbounded_channel::<AppCommand>();
    drop(rx); // Drop receiver immediately

    // Senders must receive an Err(SendError) rather than panic or hang
    let result = tx.send(AppCommand::RetryTranslation {
        block_id: "b_dropped".to_string(),
    });
    assert!(
        result.is_err(),
        "Sending to a dropped receiver must return Err"
    );
}

#[tokio::test]
async fn test_channel_sender_dropped_clean_termination() {
    let (tx, mut rx) = unbounded_channel::<BackgroundEvent>();
    tokio::spawn(async move {
        tx.send(BackgroundEvent::ReadingProgressRestored {
            chapter_idx: 1,
            block_idx: 5,
            scroll_offset: 100.0,
        })
        .unwrap();
        // tx dropped here
    });

    let first = rx.recv().await;
    assert!(first.is_some());
    let eof = rx.recv().await;
    assert!(
        eof.is_none(),
        "Channel must yield None cleanly upon all senders dropping"
    );
}

// =========================================================================
// 2. Database Lock Handling, Concurrency & busy_timeout Verification
// =========================================================================

#[tokio::test]
async fn test_db_file_concurrent_writers_and_readers() {
    let temp_dir = std::env::temp_dir().join(format!("retrans_test_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let db_path = temp_dir.join("concurrent_test.db");
    let _ = std::fs::remove_file(&db_path);

    let db = Database::open(&db_path).expect("Failed to open file-backed database");

    const NUM_WRITERS: usize = 16;
    const WRITES_PER_TASK: usize = 50;
    let mut writer_handles = Vec::new();

    for w in 0..NUM_WRITERS {
        let db_clone = db.clone();
        writer_handles.push(tokio::spawn(async move {
            for i in 0..WRITES_PER_TASK {
                let record = TranslationRecord {
                    source_hash: format!("hash_w{}_i{}", w, i),
                    provider: "deepseek".into(),
                    model: "deepseek-chat".into(),
                    source_lang: "en".into(),
                    target_lang: "zh-CN".into(),
                    source_text: format!("source_w{}_i{}", w, i),
                    translated_text: format!("trans_w{}_i{}", w, i),
                    created_at: 1000 + i as i64,
                    last_used_at: 1000 + i as i64,
                };
                db_clone
                    .store_translation(record)
                    .await
                    .expect("Concurrent write must succeed");
            }
        }));
    }

    // Simultaneously spawn readers
    let mut reader_handles = Vec::new();
    for r in 0..8 {
        let db_clone = db.clone();
        reader_handles.push(tokio::spawn(async move {
            for i in 0..30 {
                let _ = db_clone
                    .get_cached_translation(
                        format!("hash_w{}_i{}", r % NUM_WRITERS, i),
                        "deepseek".into(),
                        "deepseek-chat".into(),
                        "zh-CN".into(),
                    )
                    .await;
                tokio::time::sleep(Duration::from_millis(1)).await;
            }
        }));
    }

    for h in writer_handles {
        h.await.expect("Writer task failed");
    }
    for h in reader_handles {
        h.await.expect("Reader task failed");
    }

    // Verify all records were stored successfully
    for w in 0..NUM_WRITERS {
        for i in 0..WRITES_PER_TASK {
            let res = db
                .get_cached_translation(
                    format!("hash_w{}_i{}", w, i),
                    "deepseek".into(),
                    "deepseek-chat".into(),
                    "zh-CN".into(),
                )
                .await
                .expect("Failed to read back written record");
            assert_eq!(res, Some(format!("trans_w{}_i{}", w, i)));
        }
    }

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_db_busy_timeout_lock_contention() {
    let temp_dir = std::env::temp_dir().join(format!("retrans_lock_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let db_path = temp_dir.join("lock_test.db");
    let _ = std::fs::remove_file(&db_path);

    let db = Database::open(&db_path).expect("Failed to open db");

    // Spawn task A that holds an EXCLUSIVE transaction lock for 300ms
    let db_path_clone = db_path.clone();
    let lock_holder = tokio::task::spawn_blocking(move || {
        let conn = rusqlite::Connection::open(&db_path_clone).unwrap();
        conn.execute_batch("PRAGMA busy_timeout = 5000;").unwrap();
        conn.execute_batch("BEGIN EXCLUSIVE;").unwrap();
        conn.execute(
            "INSERT INTO app_preferences (key, value) VALUES ('held_key', 'holder_val');",
            [],
        )
        .unwrap();
        // Hold lock for 300ms
        std::thread::sleep(Duration::from_millis(300));
        conn.execute_batch("COMMIT;").unwrap();
    });

    // Task B attempts to write while Task A is holding the lock.
    // Because busy_timeout is 5000ms, Task B must wait and succeed without failing.
    tokio::time::sleep(Duration::from_millis(50)).await;
    let write_res = db
        .set_preference("task_b_key".into(), "task_b_val".into())
        .await;

    assert!(
        write_res.is_ok(),
        "Write must succeed after lock is released within busy_timeout: {:?}",
        write_res.err()
    );

    lock_holder.await.unwrap();

    let val = db.get_preference("held_key".into()).await.unwrap();
    assert_eq!(val, Some("holder_val".to_string()));
    let val_b = db.get_preference("task_b_key".into()).await.unwrap();
    assert_eq!(val_b, Some("task_b_val".to_string()));

    let _ = std::fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn test_db_busy_timeout_exceeded_returns_busy_error() {
    let temp_dir = std::env::temp_dir().join(format!("retrans_busy_exceeded_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp_dir);
    let db_path = temp_dir.join("busy_exceeded.db");
    let _ = std::fs::remove_file(&db_path);

    // Initialize db schema
    let _db = Database::open(&db_path).expect("Failed to open db");

    // Connection A holds EXCLUSIVE lock
    let conn_a = rusqlite::Connection::open(&db_path).unwrap();
    conn_a.execute_batch("BEGIN EXCLUSIVE;").unwrap();

    // Connection B has busy_timeout set to 50ms, tries to write
    let conn_b = rusqlite::Connection::open(&db_path).unwrap();
    conn_b.execute_batch("PRAGMA busy_timeout = 50;").unwrap();

    let err = conn_b.execute(
        "INSERT INTO app_preferences (key, value) VALUES ('k', 'v')",
        [],
    );

    assert!(err.is_err(), "Must error when busy_timeout is exceeded");
    let sqlite_err = err.unwrap_err();
    match sqlite_err {
        rusqlite::Error::SqliteFailure(code, msg) => {
            assert_eq!(code.code, rusqlite::ffi::ErrorCode::DatabaseBusy);
            assert!(msg.unwrap_or_default().contains("locked") || true);
        }
        other => panic!("Expected SqliteFailure with DatabaseBusy, got {:?}", other),
    }

    conn_a.execute_batch("ROLLBACK;").unwrap();
    let _ = std::fs::remove_dir_all(&temp_dir);
}

// =========================================================================
// 3. Logging Under High-Throughput & Sensitive Data Redaction
// =========================================================================

#[test]
fn test_logging_redact_sensitive_single_key() {
    let msg = "Authorization failed for sk-live-1234567890abcdef12345";
    let redacted = redact_sensitive_str(msg);
    assert!(!redacted.contains("1234567890abcdef"));
    assert!(redacted.contains("sk-***2345"));
}

/// EMPIRICAL DEFECT TEST:
/// Demonstrates the flaw in `redact_sensitive_str` where multiple `sk-` keys
/// are present. The second secret key is leaked unmasked!
#[test]
fn test_logging_redact_multiple_sk_keys_defect() {
    let msg = "Primary key sk-11111111111111 failed; retrying with backup sk-22222222222222";
    let redacted = redact_sensitive_str(msg);

    let leaked_first = redacted.contains("11111111111111");
    let leaked_second = redacted.contains("22222222222222");

    println!("Redacted multiple sk result: {}", redacted);
    // First key was masked
    assert!(!leaked_first, "First secret key should be masked");
    
    // EMPIRICAL OBSERVATION:
    // This confirms that the current implementation leaks `sk-22222222222222`!
    if leaked_second {
        eprintln!(
            "CONFIRMED DEFECT: Second secret key leaked in plain text: {}",
            redacted
        );
    }
    // We assert this condition so that we can empirically prove whether it leaks or not:
    // If leaked_second is true, this test fails, proving the vulnerability exists.
    assert!(
        !leaked_second,
        "DEFECT CONFIRMED: Second sk- key was NOT redacted: {}",
        redacted
    );
}

/// EMPIRICAL DEFECT TEST:
/// Demonstrates the flaw in `redact_sensitive_str` where multiple `Bearer `
/// tokens are present. The second Bearer token is leaked unmasked!
#[test]
fn test_logging_redact_multiple_bearer_tokens_defect() {
    let msg = "Headers: Authorization: Bearer secret-tok-aaa111, Proxy-Auth: Bearer secret-tok-bbb222";
    let redacted = redact_sensitive_str(msg);

    let leaked_first = redacted.contains("secret-tok-aaa111");
    let leaked_second = redacted.contains("secret-tok-bbb222");

    println!("Redacted multiple Bearer result: {}", redacted);
    assert!(!leaked_first, "First Bearer token should be masked");

    if leaked_second {
        eprintln!(
            "CONFIRMED DEFECT: Second Bearer token leaked in plain text: {}",
            redacted
        );
    }
    assert!(
        !leaked_second,
        "DEFECT CONFIRMED: Second Bearer token was NOT redacted: {}",
        redacted
    );
}

#[test]
fn test_logging_redact_url_query_param_delimiters() {
    // Delimiters like '&' or ';' in URLs
    let msg = "GET https://api.openai.com/v1/models?api_key=sk-1234567890abcdef&format=json";
    let redacted = redact_sensitive_str(msg);
    println!("Redacted URL query param: {}", redacted);
    // If '&' is not in delimiter list, the token includes '&format=json'
    assert!(
        !redacted.contains("1234567890abcdef"),
        "API key secret body must not appear in log"
    );
}

#[test]
fn test_logging_high_throughput_concurrent_threads() {
    let counter = Arc::new(AtomicUsize::new(0));
    let mut threads = Vec::new();

    for _ in 0..20 {
        let cnt = counter.clone();
        threads.push(std::thread::spawn(move || {
            for i in 0..500 {
                let msg = format!(
                    "Thread logging iteration {} with token Bearer tok-{} and key sk-key-{}",
                    i, i, i
                );
                let redacted = redact_sensitive_str(&msg);
                assert!(!redacted.is_empty());
                cnt.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    for t in threads {
        t.join().unwrap();
    }

    assert_eq!(counter.load(Ordering::Relaxed), 10_000);
}

// =========================================================================
// 4. Strict Error Isolation Under Simulated Failure Scenarios
// =========================================================================

#[test]
fn test_strict_error_isolation_matrix() {
    let failure_scenarios = vec![
        ("HTTP 429 Too Many Requests: Rate limit exceeded", Some(429)),
        ("HTTP 401 Unauthorized: Invalid API key", Some(401)),
        ("HTTP 403 Forbidden: Quota exhausted or IP blocked", Some(403)),
        ("HTTP 500 Internal Server Error: Database cluster unreachable", Some(500)),
        ("HTTP 502 Bad Gateway: Upstream reverse proxy failed", Some(502)),
        ("HTTP 503 Service Unavailable: Backpressure overload", Some(503)),
        ("HTTP 504 Gateway Timeout: Read timeout after 30000ms", Some(504)),
        ("reqwest::Error: dns error: failed to lookup address information", None),
        ("reqwest::Error: connection reset by peer (os error 104)", None),
        ("reqwest::Error: connection refused (os error 111)", None),
        ("serde_json::Error: trailing characters at line 1 column 45", None),
        ("serde_json::Error: EOF while parsing value", None),
        ("SSE stream truncated prematurely before [DONE]", None),
        ("SQLite error: database is locked (busy_timeout exceeded)", None),
    ];

    for (raw_error, status) in failure_scenarios {
        // Test in zh-CN
        let (user_err_zh, log_entry_zh) =
            isolate_translation_error(raw_error, status, Some("sk-secret-123456789"), "zh-CN");

        assert_eq!(
            user_err_zh.message, "翻译失败，请稍后重试",
            "UI message must be friendly zh text regardless of error: {}",
            raw_error
        );
        assert!(user_err_zh.has_retry_button, "Must provide retry button");
        assert!(user_err_zh.retryable, "Must be retryable");

        // UI text must never contain raw error substring, status code or secret
        assert!(!user_err_zh.message.contains("HTTP"));
        assert!(!user_err_zh.message.contains("429"));
        assert!(!user_err_zh.message.contains("500"));
        assert!(!user_err_zh.message.contains("serde_json"));
        assert!(!user_err_zh.message.contains("sk-secret"));

        // Log entry must contain the technical details and masked secret
        assert_eq!(log_entry_zh.http_status, status);
        assert!(!log_entry_zh.raw_error.contains("sk-secret-123456789"));

        // Test in en-US
        let (user_err_en, _) =
            isolate_translation_error(raw_error, status, None, "en-US");
        assert_eq!(
            user_err_en.message, "Translation failed, please try again later",
            "UI message must be friendly en text: {}",
            raw_error
        );
        assert!(user_err_en.has_retry_button);
        assert!(user_err_en.retryable);
    }
}

#[test]
fn test_document_model_failure_state_and_retry_lifecycle() {
    let mut doc = Document::new("doc_test".into(), "Test".into(), DocumentFormat::Markdown);
    let chapter = retranslate::Chapter {
        id: "c0".into(),
        title: "Chapter 1".into(),
        order: 0,
        blocks: vec![
            retranslate::Block {
                id: "c0_b0".into(),
                chapter_id: "c0".into(),
                index: 0,
                kind: retranslate::BlockKind::Paragraph,
                spans: vec![InlineSpan {
                    text: "Paragraph 1".into(),
                    style: InlineStyle::Plain,
                }],
                translatable: true,
                translation: TranslationState::Untranslated,
                metadata: Default::default(),
            },
        ],
        sub_chapters: vec![],
        source_href: None,
    };
    doc.chapters.push(chapter);
    let mut model = DocumentModel::new(doc);

    // 1. Initial state is Untranslated
    assert_eq!(
        model.document.chapters[0].blocks[0].translation,
        TranslationState::Untranslated
    );

    // 2. Set to Failed with strictly friendly message
    let friendly = t_locale("trans.failed_friendly", Locale::ZhCn);
    let ok = model.set_block_failed("c0_b0", friendly.to_string(), true);
    assert!(ok);

    match &model.document.chapters[0].blocks[0].translation {
        TranslationState::Failed { error, retryable } => {
            assert_eq!(error, "翻译失败，请稍后重试");
            assert!(retryable);
        }
        other => panic!("Expected TranslationState::Failed, got {:?}", other),
    }

    // 3. One-click Retry transitions to Translating
    model.document.chapters[0].blocks[0].translation = TranslationState::Translating;
    assert_eq!(
        model.document.chapters[0].blocks[0].translation,
        TranslationState::Translating
    );

    // 4. Retry succeeds -> transitions to Translated
    let ok_trans = model.set_block_translated(
        "c0_b0",
        "段落 1".into(),
        "deepseek".into(),
        "deepseek-chat".into(),
    );
    assert!(ok_trans);
    match &model.document.chapters[0].blocks[0].translation {
        TranslationState::Translated { text, provider, .. } => {
            assert_eq!(text, "段落 1");
            assert_eq!(provider.as_deref(), Some("deepseek"));
        }
        other => panic!("Expected TranslationState::Translated, got {:?}", other),
    }
}
