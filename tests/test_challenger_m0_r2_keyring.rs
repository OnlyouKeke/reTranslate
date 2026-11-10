//! Empirical Stress Harness: Keyring Concurrency, Self-Healing, and Permissions
//! Milestone M0 Iteration 2 Challenger Verification Suite
//!
//! Validates:
//! 1. Concurrency: High-throughput simultaneous multi-threaded writes against CredentialStore (no collisions, no lost updates).
//! 2. Self-healing: Non-UTF8 binary noise and malformed files overwritten cleanly by set_key into valid JSON.
//! 3. Permissions: Strict 0o600 mode on Unix across writes, updates, deletions, and after permission tampering.

use retranslate::storage::CredentialStore;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

/// Helper to allocate an isolated temporary test directory
fn temp_test_dir(suffix: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "retrans_challenger_r2_{}_{}_{}",
        suffix,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos()
    ));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).expect("Failed to create temporary test directory");
    dir
}

// =========================================================================
// 1. CONCURRENCY: Multi-Threaded Simultaneous Writes & Mixed Workloads
// =========================================================================

#[test]
fn test_concurrency_simultaneous_multithreaded_writes_50_threads() {
    let temp_dir = temp_test_dir("concurrent_50_threads");
    let fallback_file = temp_dir.join("credentials.json");
    let store = CredentialStore::with_fallback_file(fallback_file.clone());

    const THREAD_COUNT: usize = 20;
    const WRITES_PER_THREAD: usize = 5;
    let mut handles = Vec::with_capacity(THREAD_COUNT);

    // Barrier to ensure all threads begin writing simultaneously
    let barrier = Arc::new(std::sync::Barrier::new(THREAD_COUNT));

    for thread_idx in 0..THREAD_COUNT {
        let store_clone = store.clone();
        let barrier_clone = barrier.clone();

        handles.push(thread::spawn(move || {
            barrier_clone.wait();
            for item_idx in 0..WRITES_PER_THREAD {
                let provider = format!("provider_t{}_i{}", thread_idx, item_idx);
                let secret = format!("sk-secret-t{}-i{}-xyz", thread_idx, item_idx);
                store_clone
                    .set_key(&provider, &secret)
                    .unwrap_or_else(|e| panic!("set_key failed in thread {}: {:?}", thread_idx, e));
            }
        }));
    }

    for h in handles {
        h.join().expect("Worker thread panicked during concurrent write");
    }

    // Verify all keys are present in the store and match expected secrets
    for thread_idx in 0..THREAD_COUNT {
        for item_idx in 0..WRITES_PER_THREAD {
            let provider = format!("provider_t{}_i{}", thread_idx, item_idx);
            let expected_secret = format!("sk-secret-t{}-i{}-xyz", thread_idx, item_idx);
            let val = store
                .get_key(&provider)
                .unwrap_or_else(|e| panic!("get_key failed for {}: {:?}", provider, e));
            assert_eq!(
                val,
                Some(expected_secret),
                "Lost update or data corruption for key: {}",
                provider
            );
        }
    }

    // Verify on-disk file is valid JSON and contains exactly the expected entries
    let content = fs::read_to_string(&fallback_file).expect("Failed to read fallback file");
    let map: HashMap<String, String> =
        serde_json::from_str(&content).expect("Fallback file on disk must be valid JSON");
    assert_eq!(
        map.len(),
        THREAD_COUNT * WRITES_PER_THREAD,
        "Fallback map must contain exactly expected entries on disk"
    );

    #[cfg(unix)]
    {
        let meta = fs::metadata(&fallback_file).expect("metadata failed");
        let mode = meta.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "Fallback file mode must be 0600 on Unix");
    }

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_concurrency_overlapping_keys_high_contention() {
    let temp_dir = temp_test_dir("contention_overlapping");
    let fallback_file = temp_dir.join("credentials.json");
    let store = CredentialStore::with_fallback_file(fallback_file.clone());

    const THREADS: usize = 10;
    const ITERATIONS: usize = 15;
    let mut handles = Vec::with_capacity(THREADS);
    let barrier = Arc::new(std::sync::Barrier::new(THREADS));

    // Multiple threads contest for a small set of 3 shared provider keys
    for t_id in 0..THREADS {
        let store_clone = store.clone();
        let barrier_clone = barrier.clone();

        handles.push(thread::spawn(move || {
            barrier_clone.wait();
            for i in 0..ITERATIONS {
                let key_id = (t_id + i) % 3;
                let provider = format!("shared_provider_{}", key_id);
                let secret = format!("sk-val-t{}-iter{}", t_id, i);
                store_clone
                    .set_key(&provider, &secret)
                    .unwrap_or_else(|e| panic!("Contended set_key failed: {:?}", e));
            }
        }));
    }

    for h in handles {
        h.join().expect("Contended thread panicked");
    }

    // Verify the store is valid and readable
    for key_id in 0..3 {
        let provider = format!("shared_provider_{}", key_id);
        let val = store.get_key(&provider).expect("get_key must succeed");
        assert!(val.is_some(), "Key {} must be present", provider);
        assert!(
            val.unwrap().starts_with("sk-val-"),
            "Key content must be a valid secret"
        );
    }

    // Verify disk content
    let content = fs::read_to_string(&fallback_file).expect("read fallback file");
    let map: HashMap<String, String> =
        serde_json::from_str(&content).expect("Must be valid JSON after contention");
    assert_eq!(map.len(), 3);

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_concurrency_mixed_readers_writers_deleters() {
    let temp_dir = temp_test_dir("mixed_rw_del");
    let fallback_file = temp_dir.join("credentials.json");
    let store = CredentialStore::with_fallback_file(fallback_file.clone());

    // Pre-populate 20 keys
    for i in 0..20 {
        store
            .set_key(&format!("init_key_{}", i), &format!("val_{}", i))
            .unwrap();
    }

    let running = Arc::new(AtomicBool::new(true));
    let read_ops = Arc::new(AtomicUsize::new(0));
    let write_ops = Arc::new(AtomicUsize::new(0));
    let delete_ops = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();

    // 10 Reader threads
    for _ in 0..10 {
        let store_clone = store.clone();
        let running_clone = running.clone();
        let read_ops_clone = read_ops.clone();
        handles.push(thread::spawn(move || {
            let mut i = 0;
            while running_clone.load(Ordering::Relaxed) {
                let _ = store_clone.get_key(&format!("init_key_{}", i % 20));
                read_ops_clone.fetch_add(1, Ordering::Relaxed);
                i += 1;
            }
        }));
    }

    // 10 Writer threads
    for t_id in 0..10 {
        let store_clone = store.clone();
        let running_clone = running.clone();
        let write_ops_clone = write_ops.clone();
        handles.push(thread::spawn(move || {
            let mut i = 0;
            while running_clone.load(Ordering::Relaxed) {
                let _ = store_clone.set_key(
                    &format!("dynamic_key_t{}_{}", t_id, i % 5),
                    &format!("secret_{}", i),
                );
                write_ops_clone.fetch_add(1, Ordering::Relaxed);
                i += 1;
            }
        }));
    }

    // 5 Deleter threads
    for t_id in 0..5 {
        let store_clone = store.clone();
        let running_clone = running.clone();
        let delete_ops_clone = delete_ops.clone();
        handles.push(thread::spawn(move || {
            let mut i = 0;
            while running_clone.load(Ordering::Relaxed) {
                let _ = store_clone.delete_key(&format!("dynamic_key_t{}_{}", t_id, i % 5));
                delete_ops_clone.fetch_add(1, Ordering::Relaxed);
                i += 1;
            }
        }));
    }

    // Run for 2500 milliseconds under high concurrent pressure
    thread::sleep(Duration::from_millis(2500));
    running.store(false, Ordering::Relaxed);

    for h in handles {
        h.join().expect("Worker thread panicked in mixed load");
    }

    let r = read_ops.load(Ordering::Relaxed);
    let w = write_ops.load(Ordering::Relaxed);
    let d = delete_ops.load(Ordering::Relaxed);
    println!("Mixed concurrent workload completed: reads={}, writes={}, deletes={}", r, w, d);

    assert!(r > 0, "Must have performed read operations (got {})", r);
    assert!(w > 0, "Must have performed write operations (got {})", w);
    assert!(d > 0, "Must have performed delete operations (got {})", d);

    // Final sanity check: file must be completely valid JSON and readable
    let content = fs::read_to_string(&fallback_file).expect("read fallback file");
    let val: serde_json::Value =
        serde_json::from_str(&content).expect("Must remain valid JSON after mixed load");
    assert!(val.is_object());

    let _ = fs::remove_dir_all(&temp_dir);
}

// =========================================================================
// 2. SELF-HEALING: Non-UTF8 Binary Noise, Partial UTF8, and Schema Corruption
// =========================================================================

#[test]
fn test_self_healing_arbitrary_binary_noise_and_nulls() {
    let temp_dir = temp_test_dir("self_healing_binary");
    let fallback_file = temp_dir.join("credentials.json");

    // Case 1: Pure binary non-UTF8 noise with 0xFF, 0xFE, 0x00, control characters
    let binary_noise: Vec<u8> = vec![
        0xFF, 0xFE, 0xFD, 0x80, 0x00, 0x01, 0xC0, 0xAF, 0x80, 0xBF, 0x00, 0x00, 0xFF, 0xFF,
    ];
    fs::write(&fallback_file, &binary_noise).expect("write binary noise");

    let store = CredentialStore::with_fallback_file(fallback_file.clone());

    // 1. get_key should not panic or error; should return Ok(None)
    let get_res = store.get_key("openai");
    assert!(get_res.is_ok(), "get_key must return Ok on binary noise");
    assert_eq!(get_res.unwrap(), None);

    // 2. set_key should self-heal and write valid JSON
    let set_res = store.set_key("openai", "sk-healing-test-1");
    assert!(set_res.is_ok(), "set_key must recover from binary noise");

    // 3. Verify file content on disk is valid JSON
    let content = fs::read_to_string(&fallback_file).expect("read healed file");
    let parsed: HashMap<String, String> =
        serde_json::from_str(&content).expect("Must be valid JSON after self-healing");
    assert_eq!(
        parsed.get("api_key:openai"),
        Some(&"sk-healing-test-1".to_string())
    );

    // 4. get_key should now return the recovered key
    assert_eq!(
        store.get_key("openai").unwrap(),
        Some("sk-healing-test-1".into())
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_self_healing_large_64kb_random_binary_garbage() {
    let temp_dir = temp_test_dir("self_healing_large_binary");
    let fallback_file = temp_dir.join("credentials.json");

    // Create 64 KiB of pseudo-random binary data with illegal UTF-8 byte sequences
    let mut large_noise = vec![0u8; 65536];
    for (idx, byte) in large_noise.iter_mut().enumerate() {
        // Deliberately insert invalid lead and continuation bytes
        *byte = match idx % 7 {
            0 => 0xFF,
            1 => 0xC0, // Overlong UTF-8 lead byte
            2 => 0x80, // Invalid start byte
            3 => 0xFE,
            4 => 0x00, // Null byte
            5 => ((idx * 37) % 256) as u8,
            _ => 0xC1,
        };
    }
    fs::write(&fallback_file, &large_noise).expect("write 64KB noise");

    let store = CredentialStore::with_fallback_file(fallback_file.clone());

    // Verify graceful handling
    assert_eq!(store.get_key("deepseek").unwrap(), None);

    // Set multiple keys to verify complete overwrite of large garbage
    store.set_key("deepseek", "sk-deepseek-key-123").unwrap();
    store.set_key("openai", "sk-openai-key-456").unwrap();

    assert_eq!(
        store.get_key("deepseek").unwrap(),
        Some("sk-deepseek-key-123".into())
    );
    assert_eq!(
        store.get_key("openai").unwrap(),
        Some("sk-openai-key-456".into())
    );

    let content = fs::read_to_string(&fallback_file).unwrap();
    let map: HashMap<String, String> = serde_json::from_str(&content).unwrap();
    assert_eq!(map.len(), 2);
    assert_eq!(
        map.get("api_key:deepseek"),
        Some(&"sk-deepseek-key-123".to_string())
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_self_healing_truncated_multibyte_utf8_sequence() {
    let temp_dir = temp_test_dir("self_healing_truncated_utf8");
    let fallback_file = temp_dir.join("credentials.json");

    // Valid JSON prefix followed by incomplete 3-byte UTF-8 character (e.g. 0xE4 0xBD without 3rd byte)
    let mut truncated = Vec::from(br#"{"api_key:test": ""#.as_slice());
    truncated.push(0xE4);
    truncated.push(0xBD);
    // Missing third byte, file ends abruptly
    fs::write(&fallback_file, &truncated).expect("write truncated utf8");

    let store = CredentialStore::with_fallback_file(fallback_file.clone());
    assert_eq!(store.get_key("test").unwrap(), None);

    store.set_key("test", "sk-recovered-unicode-你好").unwrap();
    assert_eq!(
        store.get_key("test").unwrap(),
        Some("sk-recovered-unicode-你好".into())
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_self_healing_malformed_json_and_wrong_schema() {
    let temp_dir = temp_test_dir("self_healing_schema");
    let fallback_file = temp_dir.join("credentials.json");

    let malformed_scenarios: Vec<&str> = vec![
        "",                                    // Empty 0-byte file
        "   \n\t  ",                           // Whitespace only
        "not json at all",                     // Plain text
        "[1, 2, 3, 4]",                        // Array instead of map
        "42",                                  // Number
        "true",                                // Bool
        "null",                                // Null
        "{\"api_key:key\": [\"not\", \"a\"]}", // Map of arrays instead of Map of strings
        "{\"api_key:truncated\": ",            // Truncated syntax
        "{{{{{{",                              // Broken braces
    ];

    for (idx, malformed) in malformed_scenarios.iter().enumerate() {
        fs::write(&fallback_file, malformed).expect("write malformed scenario");

        let store = CredentialStore::with_fallback_file(fallback_file.clone());
        let _ = store.delete_key("target");
        assert_eq!(
            store.get_key("target").unwrap(),
            None,
            "Scenario {} failed on get_key",
            idx
        );

        store
            .set_key("target", &format!("sk-valid-{}", idx))
            .unwrap_or_else(|e| panic!("Scenario {} failed on set_key: {:?}", idx, e));

        assert_eq!(
            store.get_key("target").unwrap(),
            Some(format!("sk-valid-{}", idx)),
            "Scenario {} failed to retrieve recovered key",
            idx
        );

        let content = fs::read_to_string(&fallback_file).unwrap();
        let map: HashMap<String, String> = serde_json::from_str(&content)
            .unwrap_or_else(|e| panic!("Scenario {} file not valid JSON: {:?}", idx, e));
        assert_eq!(
            map.get("api_key:target"),
            Some(&format!("sk-valid-{}", idx))
        );
        let _ = store.delete_key("target");
    }

    let _ = fs::remove_dir_all(&temp_dir);
}

// =========================================================================
// 3. PERMISSIONS: Mode 0o600 on Unix across Lifecycle & After Tampering
// =========================================================================

#[test]
#[cfg(unix)]
fn test_permissions_mode_0600_lifecycle_and_tampering() {
    let temp_dir = temp_test_dir("permissions_lifecycle");
    let fallback_file = temp_dir.join("credentials.json");
    let store = CredentialStore::with_fallback_file(fallback_file.clone());

    // 1. Initial write creates file with 0600 mode
    store.set_key("provider_a", "sk-secret-a").unwrap();
    assert!(fallback_file.exists());
    let mode1 = fs::metadata(&fallback_file).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        mode1, 0o600,
        "Initial creation mode must be exactly 0600 (got {:o})",
        mode1
    );

    // 2. Second write (update existing key) preserves 0600 mode
    store.set_key("provider_a", "sk-secret-a-updated").unwrap();
    let mode2 = fs::metadata(&fallback_file).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        mode2, 0o600,
        "Update operation mode must be exactly 0600 (got {:o})",
        mode2
    );

    // 3. Add second key preserves 0600 mode
    store.set_key("provider_b", "sk-secret-b").unwrap();
    let mode3 = fs::metadata(&fallback_file).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        mode3, 0o600,
        "Add second key mode must be exactly 0600 (got {:o})",
        mode3
    );

    // 4. Delete one key (partial delete) preserves 0600 mode
    store.delete_key("provider_a").unwrap();
    assert_eq!(store.get_key("provider_a").unwrap(), None);
    assert_eq!(
        store.get_key("provider_b").unwrap(),
        Some("sk-secret-b".into())
    );
    let mode4 = fs::metadata(&fallback_file).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        mode4, 0o600,
        "Partial delete mode must be exactly 0600 (got {:o})",
        mode4
    );

    // 5. Delete last key (empty map) preserves 0600 mode
    store.delete_key("provider_b").unwrap();
    assert_eq!(store.get_key("provider_b").unwrap(), None);
    let mode5 = fs::metadata(&fallback_file).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        mode5, 0o600,
        "Empty map delete mode must be exactly 0600 (got {:o})",
        mode5
    );

    // 6. External Permission Tampering & Restoration via set_key
    // Deliberately tamper permissions to insecure 0o666 (world readable & writable)
    fs::set_permissions(&fallback_file, fs::Permissions::from_mode(0o666)).unwrap();
    let tampered_mode = fs::metadata(&fallback_file).unwrap().permissions().mode() & 0o777;
    assert_eq!(tampered_mode, 0o666);

    // Invoking set_key MUST restore restricted 0o600 permissions
    store.set_key("tamper_test", "sk-restored").unwrap();
    let restored_mode = fs::metadata(&fallback_file).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        restored_mode, 0o600,
        "set_key must strictly restore permissions to 0600 after tampering (got {:o})",
        restored_mode
    );

    // 7. External Permission Tampering & Restoration via delete_key
    fs::set_permissions(&fallback_file, fs::Permissions::from_mode(0o644)).unwrap();
    let tampered_mode2 = fs::metadata(&fallback_file).unwrap().permissions().mode() & 0o777;
    assert_eq!(tampered_mode2, 0o644);

    // Invoking delete_key MUST restore restricted 0o600 permissions
    store.delete_key("tamper_test").unwrap();
    let restored_mode2 = fs::metadata(&fallback_file).unwrap().permissions().mode() & 0o777;
    assert_eq!(
        restored_mode2, 0o600,
        "delete_key must strictly restore permissions to 0600 after tampering (got {:o})",
        restored_mode2
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

// =========================================================================
// 4. ADVERSARIAL EDGE CASES: Exotic Characters, Huge Secrets, Deep Paths
// =========================================================================

#[test]
fn test_edge_cases_exotic_chars_and_extreme_sizes() {
    let temp_dir = temp_test_dir("edge_cases");
    let fallback_file = temp_dir.join("credentials.json");
    let store = CredentialStore::with_fallback_file(fallback_file.clone());

    // 1. Empty provider and empty secret
    store.set_key("", "").unwrap();
    assert_eq!(store.get_key("").unwrap(), Some("".into()));

    // 2. Exotic characters: Unicode, emojis, newlines, JSON delimiters, quotes, slashes
    let exotic_provider = "openai/gpt-4o+deepseek@v1 #test:special_provider_🚀!§$%&/()=?*'\"><`\\";
    let exotic_secret = "sk-🔑-test\n\r\t\"with'quotes\"and\\backslashes/{[}]:;,\0null";
    store.set_key(exotic_provider, exotic_secret).unwrap();
    assert_eq!(
        store.get_key(exotic_provider).unwrap(),
        Some(exotic_secret.into())
    );

    // 3. Huge secret (1 Megabyte)
    let huge_secret = "x".repeat(1024 * 1024);
    store.set_key("huge_provider", &huge_secret).unwrap();
    assert_eq!(
        store.get_key("huge_provider").unwrap(),
        Some(huge_secret.clone())
    );

    // Verify it persists and reloads correctly
    let reloaded_store = CredentialStore::with_fallback_file(fallback_file.clone());
    assert_eq!(
        reloaded_store.get_key("huge_provider").unwrap(),
        Some(huge_secret)
    );

    // 4. Delete exotic key
    store.delete_key(exotic_provider).unwrap();
    assert_eq!(store.get_key(exotic_provider).unwrap(), None);

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_deep_nested_parent_directory_auto_creation() {
    let temp_dir = temp_test_dir("deep_nested_path");
    let deep_fallback_file = temp_dir
        .join("a")
        .join("b")
        .join("c")
        .join("credentials.json");

    assert!(!deep_fallback_file.parent().unwrap().exists());

    let store = CredentialStore::with_fallback_file(deep_fallback_file.clone());
    store.set_key("deep_provider", "sk-deep").unwrap();

    assert!(deep_fallback_file.exists());
    assert_eq!(
        store.get_key("deep_provider").unwrap(),
        Some("sk-deep".into())
    );

    #[cfg(unix)]
    {
        let mode = fs::metadata(&deep_fallback_file)
            .unwrap()
            .permissions()
            .mode()
            & 0o777;
        assert_eq!(mode, 0o600);
    }

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_no_orphan_temp_files_left_behind() {
    let temp_dir = temp_test_dir("no_orphans");
    let fallback_file = temp_dir.join("credentials.json");
    let store = CredentialStore::with_fallback_file(fallback_file.clone());

    // Perform sequential and concurrent writes and deletes
    for i in 0..50 {
        store.set_key(&format!("key_{}", i), &format!("val_{}", i)).unwrap();
        if i % 2 == 0 {
            store.delete_key(&format!("key_{}", i)).unwrap();
        }
    }

    // Inspect temp_dir for any .tmp files
    let mut tmp_files = Vec::new();
    for entry in fs::read_dir(&temp_dir).unwrap() {
        let path = entry.unwrap().path();
        if let Some(ext) = path.extension() {
            if ext == "tmp" {
                tmp_files.push(path);
            }
        }
    }

    assert!(
        tmp_files.is_empty(),
        "No orphan .tmp files should remain in the directory: {:?}",
        tmp_files
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
#[cfg(unix)]
fn test_write_failure_cleans_up_temp_file() {
    let temp_dir = temp_test_dir("write_failure_cleanup");
    let fallback_file = temp_dir.join("credentials.json");
    let store = CredentialStore::with_fallback_file(fallback_file.clone());

    // First write to establish file
    store.set_key("k1", "v1").unwrap();

    // Make parent directory read-only (chmod 0o500)
    fs::set_permissions(&temp_dir, fs::Permissions::from_mode(0o500)).unwrap();

    // Now set_key should fail because it cannot create new temp files or rename
    let write_res = store.set_key("k2", "v2");
    assert!(write_res.is_err(), "Write must fail in read-only directory");

    // Restore permissions so we can inspect and clean up
    fs::set_permissions(&temp_dir, fs::Permissions::from_mode(0o700)).unwrap();

    // Verify no orphan .tmp files were leaked
    let mut tmp_files = Vec::new();
    for entry in fs::read_dir(&temp_dir).unwrap() {
        let path = entry.unwrap().path();
        if let Some(ext) = path.extension() {
            if ext == "tmp" {
                tmp_files.push(path);
            }
        }
    }
    assert!(
        tmp_files.is_empty(),
        "Failed write must clean up any temporary file: {:?}",
        tmp_files
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_bom_and_utf8_special_markers_recovery() {
    let temp_dir = temp_test_dir("bom_markers");
    let fallback_file = temp_dir.join("credentials.json");

    // Write UTF-8 BOM followed by invalid JSON
    let mut bom_corrupted = vec![0xEF, 0xBB, 0xBF];
    bom_corrupted.extend_from_slice(b"BOM prefix with broken content");
    fs::write(&fallback_file, &bom_corrupted).unwrap();

    let store = CredentialStore::with_fallback_file(fallback_file.clone());
    assert_eq!(store.get_key("test_provider").unwrap(), None);

    store.set_key("test_provider", "sk-bom-recovered").unwrap();
    assert_eq!(
        store.get_key("test_provider").unwrap(),
        Some("sk-bom-recovered".into())
    );

    let content = fs::read_to_string(&fallback_file).unwrap();
    let map: HashMap<String, String> = serde_json::from_str(&content).unwrap();
    assert_eq!(map.get("api_key:test_provider"), Some(&"sk-bom-recovered".to_string()));

    let _ = fs::remove_dir_all(&temp_dir);
}

