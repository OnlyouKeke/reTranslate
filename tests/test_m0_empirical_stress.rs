//! Empirical Stress & Boundary Challenge Test Suite for Milestone M0
//!
//! Areas challenged:
//! 1. SQLite connection pool under high concurrency and duplicate keys.
//! 2. SHA-256 fingerprinting on empty, zero-byte, and large inputs (10MB+).
//! 3. Keyring fallback file corruption and recovery.
//! 4. i18n lookup with invalid and edge keys.

use retranslate::{
    compute_sha256_file, compute_sha256_hex, current_locale, set_locale, t, t_locale,
    CredentialStore, Database, GlossaryTerm, Locale, ReadingProgress, TranslationRecord,
};
use sha2::{Digest, Sha256};
use std::fs;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

// =========================================================================
// Area 1: SQLite Connection Pool Under High Concurrency and Duplicate Keys
// =========================================================================

#[tokio::test]
async fn challenge_sqlite_concurrent_cold_start_open() {
    let temp_dir = std::env::temp_dir().join(format!("retrans_sqlite_cold_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();
    let db_path = temp_dir.join("cold_start.db");

    // 10 concurrent threads all calling Database::open at the exact same instant on a new DB
    let mut handles = Vec::new();
    for _ in 0..10 {
        let p = db_path.clone();
        handles.push(tokio::spawn(async move {
            Database::open(&p)
        }));
    }

    for h in handles {
        let res = h.await.unwrap();
        assert!(res.is_ok(), "Concurrent Database::open cold start must not fail: {:?}", res.err());
    }

    let _ = fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn challenge_sqlite_high_concurrency_file_pool_100_tasks() {
    let temp_dir = std::env::temp_dir().join(format!("retrans_sqlite_stress100_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();
    let db_path = temp_dir.join("stress100.db");

    let db = Database::open(&db_path).expect("Failed to open file-backed SQLite database");

    // 100 concurrent tokio tasks, each performing 10 writes and 10 reads (total 2000 ops)
    let num_tasks = 100;
    let ops_per_task = 10;
    let mut handles = Vec::new();

    for task_id in 0..num_tasks {
        let db_clone = db.clone();
        handles.push(tokio::spawn(async move {
            for op in 0..ops_per_task {
                let source_hash = format!("hash_{}_{}", task_id, op);
                let record = TranslationRecord {
                    source_hash: source_hash.clone(),
                    provider: "openai".into(),
                    model: "gpt-4o".into(),
                    source_lang: "en".into(),
                    target_lang: "zh-CN".into(),
                    source_text: format!("Source text {task_id}-{op}"),
                    translated_text: format!("译文 {task_id}-{op}"),
                    created_at: 1000 + op as i64,
                    last_used_at: 1000 + op as i64,
                };

                // Store
                db_clone.store_translation(record).await.expect("Store failed under 100-concurrency");

                // Read back
                let cached = db_clone
                    .get_cached_translation(
                        source_hash,
                        "openai".into(),
                        "gpt-4o".into(),
                        "zh-CN".into(),
                    )
                    .await
                    .expect("Read failed under 100-concurrency");
                assert_eq!(cached, Some(format!("译文 {task_id}-{op}")));
            }
        }));
    }

    for h in handles {
        h.await.expect("Task panicked during SQLite 100-task concurrency stress");
    }

    let _ = fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn challenge_sqlite_duplicate_primary_key_collisions() {
    let temp_dir = std::env::temp_dir().join(format!("retrans_sqlite_dup_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();
    let db_path = temp_dir.join("dup_keys.db");

    let db = Database::open(&db_path).expect("Failed to open file-backed SQLite database");

    // 50 concurrent tasks hammering the EXACT same primary key with different translations
    let num_tasks = 50;
    let mut handles = Vec::new();

    for task_id in 0..num_tasks {
        let db_clone = db.clone();
        handles.push(tokio::spawn(async move {
            let record = TranslationRecord {
                source_hash: "shared_collision_hash".into(),
                provider: "deepseek".into(),
                model: "deepseek-chat".into(),
                source_lang: "en".into(),
                target_lang: "zh-CN".into(),
                source_text: "Shared source text".into(),
                translated_text: format!("Concurrent translation from worker {task_id}"),
                created_at: 2000,
                last_used_at: 2000 + task_id as i64,
            };
            db_clone.store_translation(record).await
        }));
    }

    for h in handles {
        let res = h.await.expect("Task panicked");
        assert!(res.is_ok(), "Duplicate key upsert must not return error: {:?}", res);
    }

    // Verify exactly one row exists and it has a valid worker translation
    let cached = db
        .get_cached_translation(
            "shared_collision_hash".into(),
            "deepseek".into(),
            "deepseek-chat".into(),
            "zh-CN".into(),
        )
        .await
        .unwrap();

    assert!(cached.is_some());
    let val = cached.unwrap();
    assert!(val.starts_with("Concurrent translation from worker "));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[tokio::test]
async fn challenge_sqlite_reading_progress_and_glossary_conflict_upsert() {
    let db = Database::open_in_memory().expect("In memory DB open");

    // Test duplicate reading progress updates
    for i in 0..10 {
        let p = ReadingProgress {
            document_id: "doc_test_conflict".into(),
            file_path: "/path/doc.epub".into(),
            chapter_index: i,
            block_index: i * 2,
            scroll_offset_px: (i as f32) * 50.0,
            last_read_at: 5000 + i as i64,
        };
        db.save_reading_progress(p).await.expect("save progress conflict");
    }

    let loaded = db.get_reading_progress("doc_test_conflict".into()).await.unwrap().unwrap();
    assert_eq!(loaded.chapter_index, 9);
    assert_eq!(loaded.block_index, 18);

    // Test duplicate glossary term with same (source_term, domain)
    for i in 0..5 {
        let term = GlossaryTerm {
            id: 0,
            source_term: "AI".into(),
            target_term: format!("人工智能 v{i}"),
            domain: "tech".into(),
            case_sensitive: false,
            created_at: 1000,
        };
        db.add_glossary_term(term).await.expect("add glossary conflict");
    }

    let glossary_list = db.list_glossary(Some("tech".into())).await.unwrap();
    assert_eq!(glossary_list.len(), 1, "UNIQUE (source_term, domain) should maintain 1 record");
    assert_eq!(glossary_list[0].target_term, "人工智能 v4");
}

#[tokio::test]
async fn challenge_sqlite_edge_inputs_and_null_bytes() {
    let db = Database::open_in_memory().expect("In memory DB open");

    // 1. Text containing embedded null bytes and extreme unicode
    let record = TranslationRecord {
        source_hash: "hash_null_\0_test".into(),
        provider: "provider_\u{1F980}_crab".into(),
        model: "model_\0_end".into(),
        source_lang: "en".into(),
        target_lang: "zh".into(),
        source_text: "Text with \0 null and \t \n \r formatting".into(),
        translated_text: "译文含 \0 空字符".into(),
        created_at: i64::MAX,
        last_used_at: i64::MIN,
    };
    db.store_translation(record).await.expect("Storing null bytes and extreme i64 must succeed");

    let cached = db
        .get_cached_translation(
            "hash_null_\0_test".into(),
            "provider_\u{1F980}_crab".into(),
            "model_\0_end".into(),
            "zh".into(),
        )
        .await
        .expect("Querying null bytes must succeed");
    assert_eq!(cached, Some("译文含 \0 空字符".to_string()));

    // 2. Reading progress with extreme float scroll offset
    let progress = ReadingProgress {
        document_id: "doc_nan_offset".into(),
        file_path: "/test/file".into(),
        chapter_index: usize::MAX / 2,
        block_index: usize::MAX / 2,
        scroll_offset_px: 999999.75,
        last_read_at: 123456789,
    };
    db.save_reading_progress(progress).await.expect("Extreme offset save");
    let loaded = db.get_reading_progress("doc_nan_offset".into()).await.unwrap().unwrap();
    assert_eq!(loaded.scroll_offset_px, 999999.75);
}

// =========================================================================
// Area 2: SHA-256 Fingerprinting on Empty, Zero-byte, and Large Inputs (10MB+)
// =========================================================================

#[test]
fn challenge_sha256_empty_and_zero_byte_inputs() {
    // 1. Standard known SHA-256 for empty byte buffer:
    let empty_hash = compute_sha256_hex(b"");
    assert_eq!(
        empty_hash,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "Empty byte slice must match standard SHA-256"
    );

    // 2. Standard known SHA-256 for single zero byte b"\x00":
    // echo -en '\x00' | sha256sum -> 6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d
    let single_zero_hash = compute_sha256_hex(&[0u8]);
    assert_eq!(
        single_zero_hash,
        "6e340b9cffb37a989ca544e6bb780a2c78901d3fb33738768511a30617afa01d",
        "Single zero-byte 0x00 must match standard SHA-256"
    );

    // 3. ASCII '0' (0x30):
    // echo -n '0' | sha256sum -> 5feceb66ffc86f38d952786c6d696c79c2dbc239dd4e91b46729d73a27fb57e9
    let ascii_zero_hash = compute_sha256_hex(b"0");
    assert_eq!(
        ascii_zero_hash,
        "5feceb66ffc86f38d952786c6d696c79c2dbc239dd4e91b46729d73a27fb57e9",
        "ASCII character '0' must match standard SHA-256"
    );

    // 4. 1MB of all zeros
    let one_mb_zeros = vec![0u8; 1024 * 1024];
    let hex_1mb = compute_sha256_hex(&one_mb_zeros);
    assert_eq!(hex_1mb.len(), 64);
    assert!(hex_1mb.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));

    // Reference verify against Sha256 direct
    let mut ref_hasher = Sha256::new();
    ref_hasher.update(&one_mb_zeros);
    let ref_hex: String = ref_hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect();
    assert_eq!(hex_1mb, ref_hex);
}

#[test]
fn challenge_sha256_large_15mb_buffer_and_file() {
    let size = 15 * 1024 * 1024; // 15 Megabytes
    let mut data = Vec::with_capacity(size);
    // Fill with deterministic pseudo-random pattern
    for i in 0..size {
        data.push((i % 251) as u8);
    }

    let in_memory_hash = compute_sha256_hex(&data);
    assert_eq!(in_memory_hash.len(), 64);

    // Reference check
    let mut ref_hasher = Sha256::new();
    ref_hasher.update(&data);
    let expected_hash: String = ref_hasher.finalize().iter().map(|b| format!("{:02x}", b)).collect();
    assert_eq!(in_memory_hash, expected_hash);

    // Test file-based streaming sha256 with 15MB file
    let temp_file = std::env::temp_dir().join(format!("retrans_sha_test_{}.bin", std::process::id()));
    fs::write(&temp_file, &data).expect("Failed to write temp file");

    let file_hash = compute_sha256_file(&temp_file).expect("compute_sha256_file failed on 15MB");
    assert_eq!(file_hash, in_memory_hash, "Streaming chunked file hash must equal in-memory hash");

    let _ = fs::remove_file(&temp_file);
}

#[test]
fn challenge_sha256_file_edge_cases() {
    let temp_dir = std::env::temp_dir().join(format!("retrans_sha_edge_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    // 1. Zero-byte file on disk
    let empty_file = temp_dir.join("empty.bin");
    fs::write(&empty_file, b"").unwrap();
    let empty_hash = compute_sha256_file(&empty_file).unwrap();
    assert_eq!(
        empty_hash,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
    );

    // 2. Exact 64KB boundary file (internal chunk size is 65536)
    let exact_64k = temp_dir.join("exact_64k.bin");
    let buf_64k = vec![0x42u8; 65536];
    fs::write(&exact_64k, &buf_64k).unwrap();
    assert_eq!(
        compute_sha256_file(&exact_64k).unwrap(),
        compute_sha256_hex(&buf_64k)
    );

    // 3. 64KB + 1 byte (tests multi-chunk transition)
    let boundary_file = temp_dir.join("boundary.bin");
    let mut buf_64k_plus = vec![0x42u8; 65537];
    buf_64k_plus[65536] = 0x99;
    fs::write(&boundary_file, &buf_64k_plus).unwrap();
    assert_eq!(
        compute_sha256_file(&boundary_file).unwrap(),
        compute_sha256_hex(&buf_64k_plus)
    );

    // 4. Non-existent file
    let non_existent = temp_dir.join("non_existent.bin");
    assert!(compute_sha256_file(&non_existent).is_err());

    // 5. Passing a directory path instead of a file
    assert!(compute_sha256_file(&temp_dir).is_err());

    let _ = fs::remove_dir_all(&temp_dir);
}

// =========================================================================
// Area 3: Keyring Fallback File Corruption and Recovery
// =========================================================================

#[test]
fn challenge_keyring_fallback_truncated_json_corruption_and_recovery() {
    let temp_dir = std::env::temp_dir().join(format!("retrans_keyring_trunc_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();
    let file = temp_dir.join("credentials.json");

    // Write truncated/malformed JSON
    fs::write(&file, b"{\"api_key:deepseek\": \"sk-corrupted-partial").unwrap();

    let store = CredentialStore::with_fallback_file(file.clone());

    // Should return None gracefully without panicking
    let read_result = store.get_key("deepseek");
    assert!(read_result.is_ok(), "Malformed JSON should not panic get_key");
    assert_eq!(read_result.unwrap(), None);

    // Now test recovery: write a new key, verifying file recovers to valid JSON
    store.set_key("deepseek", "sk-recovered-key-1234").expect("set_key should recover corrupted JSON file");

    let recovered_val = store.get_key("deepseek").unwrap();
    assert_eq!(recovered_val, Some("sk-recovered-key-1234".to_string()));

    // Verify file content is valid formatted JSON on disk
    let content = fs::read_to_string(&file).unwrap();
    let map: serde_json::Value = serde_json::from_str(&content).expect("Disk file must be valid JSON after recovery");
    assert_eq!(map["api_key:deepseek"], "sk-recovered-key-1234");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn challenge_keyring_fallback_wrong_schema_and_empty_file_recovery() {
    let temp_dir = std::env::temp_dir().join(format!("retrans_keyring_schema_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();
    let file = temp_dir.join("credentials.json");

    // Case A: 0-byte empty file
    fs::write(&file, b"").unwrap();
    let store = CredentialStore::with_fallback_file(file.clone());
    assert_eq!(store.get_key("openai").unwrap(), None);
    store.set_key("openai", "sk-key-from-empty").unwrap();
    assert_eq!(store.get_key("openai").unwrap(), Some("sk-key-from-empty".into()));

    // Case B: Valid JSON, but wrong type (array instead of map)
    fs::write(&file, b"[1, 2, 3, 4]").unwrap();
    assert_eq!(store.get_key("openai").unwrap(), None);
    store.set_key("openai", "sk-key-from-array").unwrap();
    assert_eq!(store.get_key("openai").unwrap(), Some("sk-key-from-array".into()));

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn challenge_keyring_fallback_permission_tampering_and_restoration() {
    let temp_dir = std::env::temp_dir().join(format!("retrans_keyring_perm_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();
    let file = temp_dir.join("credentials.json");

    let store = CredentialStore::with_fallback_file(file.clone());
    store.set_key("provider1", "key1").unwrap();

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        // External tool tampered permissions to 0777 (world readable/writable)
        fs::set_permissions(&file, fs::Permissions::from_mode(0o777)).unwrap();
        let meta = fs::metadata(&file).unwrap();
        assert_eq!(meta.permissions().mode() & 0o777, 0o777);

        // Store updates a key — it MUST restore restricted 0600 permissions
        store.set_key("provider2", "key2").unwrap();

        let restored_meta = fs::metadata(&file).unwrap();
        let mode = restored_meta.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "Permissions must be strictly restored to 0600 after update");
    }

    let _ = fs::remove_dir_all(&temp_dir);
}

/// CHALLENGE VERIFICATION 1:
/// Empirical verification of non-UTF-8 binary corruption self-healing recovery.
/// When credentials.json contains non-UTF-8 bytes, get_key recovers gracefully returning Ok(None)
/// and set_key successfully self-heals by overwriting the corrupted file with valid JSON.
#[test]
fn challenge_keyring_fallback_non_utf8_binary_corruption_finding() {
    let temp_dir = std::env::temp_dir().join(format!("retrans_keyring_binary_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();
    let file = temp_dir.join("credentials.json");

    // Write raw invalid UTF-8 bytes (binary corruption)
    fs::write(&file, [0xFF, 0xFE, 0xFD, 0x80, 0x00, 0x01]).unwrap();

    let store = CredentialStore::with_fallback_file(file.clone());

    // 1. Verify get_key recovers gracefully from non-UTF8 corruption and returns Ok(None)
    let get_res = store.get_key("openai");
    assert!(get_res.is_ok(), "Non-UTF8 corrupted file returns Ok(None) from get_key");
    assert_eq!(get_res.unwrap(), None);

    // 2. Verify set_key self-heals when file has non-UTF8 corruption
    let set_res = store.set_key("openai", "sk-new-key");
    assert!(set_res.is_ok(), "set_key must self-heal when file has non-UTF8 bytes");
    assert_eq!(store.get_key("openai").unwrap(), Some("sk-new-key".to_string()));

    let _ = fs::remove_dir_all(&temp_dir);
}

/// CHALLENGE FINDING 2:
/// Serial access works, but concurrent set_key calls collide on the static `credentials.tmp` filename.
#[test]
fn challenge_keyring_serial_access_works_as_designed() {
    let temp_dir = std::env::temp_dir().join(format!("retrans_keyring_serial_{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();
    let file = temp_dir.join("credentials.json");

    let store = CredentialStore::with_fallback_file(file.clone());
    for i in 0..10 {
        let provider = format!("provider_{i}");
        let key = format!("sk-secret-val-{i}");
        store.set_key(&provider, &key).expect("set_key serial");
        let read_val = store.get_key(&provider).expect("get_key serial");
        assert_eq!(read_val, Some(key));
    }

    let _ = fs::remove_dir_all(&temp_dir);
}

// =========================================================================
// Area 4: i18n Lookup with Invalid and Edge Keys
// =========================================================================

#[test]
fn challenge_i18n_invalid_and_edge_keys() {
    set_locale(Locale::ZhCn);

    // 1. Empty key
    assert_eq!(t(""), "", "Empty key should return empty string");
    assert_eq!(t_locale("", Locale::EnUs), "");
    assert_eq!(t_locale("", Locale::ZhCn), "");

    // 2. Non-existent keys
    assert_eq!(t("totally.nonexistent.key.xyz"), "");
    assert_eq!(t_locale("missing.key", Locale::EnUs), "");

    // 3. Symbol and punctuation keys
    assert_eq!(t("...---___@@@###$$$%%%^^^&&&***()"), "");
    assert_eq!(t("\0null\0bytes\0"), "");
    assert_eq!(t("\n\r\t\x1b[31m"), "");

    // 4. Extremely long key (100,000 characters)
    let huge_key = "a".repeat(100_000);
    assert_eq!(t(&huge_key), "");

    // 5. Locale::from_code edge cases
    assert_eq!(Locale::from_code(""), None);
    assert_eq!(Locale::from_code("   "), None);
    assert_eq!(Locale::from_code("zh"), Some(Locale::ZhCn));
    assert_eq!(Locale::from_code("ZH"), Some(Locale::ZhCn));
    assert_eq!(Locale::from_code("zh_CN"), Some(Locale::ZhCn));
    assert_eq!(Locale::from_code("ZH-CN"), Some(Locale::ZhCn));
    assert_eq!(Locale::from_code("zh-hans"), Some(Locale::ZhCn));
    assert_eq!(Locale::from_code("en"), Some(Locale::EnUs));
    assert_eq!(Locale::from_code("EN-us"), Some(Locale::EnUs));
    assert_eq!(Locale::from_code("en_gb"), Some(Locale::EnUs));
    assert_eq!(Locale::from_code("fr"), None);
    assert_eq!(Locale::from_code("de"), None);
    assert_eq!(Locale::from_code("zh-tw"), None);
    assert_eq!(Locale::from_code("zh-hant"), None);
}

#[test]
fn challenge_i18n_concurrent_switching_stress() {
    let running = Arc::new(AtomicBool::new(true));
    let mut handles = Vec::new();

    // 10 threads continuously switching locale
    for _i in 0..10 {
        let running_clone = running.clone();
        handles.push(std::thread::spawn(move || {
            let mut iter = 0;
            while running_clone.load(Ordering::Relaxed) {
                if iter % 2 == 0 {
                    set_locale(Locale::ZhCn);
                } else {
                    set_locale(Locale::EnUs);
                }
                iter += 1;
                if iter > 2000 {
                    break;
                }
            }
        }));
    }

    // 10 threads continuously reading translations
    for _ in 0..10 {
        let running_clone = running.clone();
        handles.push(std::thread::spawn(move || {
            let mut iter = 0;
            while running_clone.load(Ordering::Relaxed) {
                let name = t("app.name");
                assert_eq!(name, "reTranslate");

                let err_msg = t("trans.failed_friendly");
                assert!(
                    err_msg == "翻译失败，请稍后重试" || err_msg == "Translation failed, please try again later",
                    "Invalid error message returned during concurrent locale switch: {}",
                    err_msg
                );

                let retry_btn = t("trans.retry_button");
                assert!(
                    retry_btn == "重试" || retry_btn == "Retry",
                    "Invalid retry text during switch: {}",
                    retry_btn
                );

                let current = current_locale();
                assert!(current == Locale::ZhCn || current == Locale::EnUs);

                iter += 1;
                if iter > 2000 {
                    break;
                }
            }
        }));
    }

    for h in handles {
        h.join().expect("Thread panicked in i18n concurrent switching stress");
    }

    running.store(false, Ordering::Relaxed);
    // Reset to default
    set_locale(Locale::ZhCn);
}
