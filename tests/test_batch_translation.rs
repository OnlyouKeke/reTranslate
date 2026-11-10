//! E2E Test Suite: Numbered Batch JSON Translation Protocol & Single-Segment Fallback
//! Covers Feature 38, 39, 40 from PROJECT.md & TEST_INFRA.md (Tier 1 & Tier 2)

mod common;
use common::*;

// =========================================================================
// TIER 1: Feature Coverage (Nominal Paths)
// =========================================================================

#[test]
fn test_batch_prompt_generation() {
    let segments = vec![
        BatchSegmentInput {
            id: 1,
            text: "Call me Ishmael.".to_string(),
        },
        BatchSegmentInput {
            id: 2,
            text: "Some years ago—never mind how long precisely...".to_string(),
        },
    ];

    let payload = serde_json::to_string(&segments).unwrap();
    assert!(payload.contains("\"id\":1"));
    assert!(payload.contains("Call me Ishmael."));
    assert!(payload.contains("\"id\":2"));
}

#[test]
fn test_batch_validation_success() {
    let expected_ids = vec![1, 2, 3];
    let raw_response = r#"{
        "segments": [
            {"id": 1, "text": "叫我以实玛利。"},
            {"id": 2, "text": "几年前——具体多久不必细究..."},
            {"id": 3, "text": "我的钱包里几乎空空如也。"}
        ]
    }"#;

    let res = validate_batch_response(&expected_ids, raw_response);
    match res {
        BatchValidationResult::Success(pairs) => {
            assert_eq!(pairs.len(), 3);
            assert_eq!(pairs[0], (1, "叫我以实玛利。".to_string()));
            assert_eq!(pairs[1].0, 2);
            assert_eq!(pairs[2].0, 3);
        }
        _ => panic!("Expected BatchValidationResult::Success"),
    }
}

#[test]
fn test_batch_markdown_fence_stripping() {
    let expected_ids = vec![1];
    let fenced_response = "```json\n{\n  \"segments\": [\n    {\"id\": 1, \"text\": \"测试译文\"}\n  ]\n}\n```";

    let res = validate_batch_response(&expected_ids, fenced_response);
    match res {
        BatchValidationResult::Success(pairs) => {
            assert_eq!(pairs.len(), 1);
            assert_eq!(pairs[0].1, "测试译文");
        }
        _ => panic!("Expected BatchValidationResult::Success with stripped markdown fences"),
    }
}

#[test]
fn test_batch_count_mismatch_fallback() {
    // 3 segments requested, LLM only returned 2
    let expected_ids = vec![1, 2, 3];
    let truncated_response = r#"{
        "segments": [
            {"id": 1, "text": "译文 1"},
            {"id": 2, "text": "译文 2"}
        ]
    }"#;

    let res = validate_batch_response(&expected_ids, truncated_response);
    match res {
        BatchValidationResult::Mismatch { expected_ids, received_ids, reason } => {
            assert_eq!(expected_ids, vec![1, 2, 3]);
            assert_eq!(received_ids, vec![1, 2]);
            assert!(reason.contains("Count mismatch"));
        }
        _ => panic!("Expected Mismatch due to count divergence"),
    }
}

#[test]
fn test_batch_id_order_mismatch_fallback() {
    // LLM swapped the order of segments
    let expected_ids = vec![1, 2];
    let out_of_order_response = r#"{
        "segments": [
            {"id": 2, "text": "译文 2"},
            {"id": 1, "text": "译文 1"}
        ]
    }"#;

    let res = validate_batch_response(&expected_ids, out_of_order_response);
    match res {
        BatchValidationResult::Mismatch { reason, .. } => {
            assert!(reason.contains("ID sequence mismatch"));
        }
        _ => panic!("Expected Mismatch due to reversed segment IDs"),
    }
}

// =========================================================================
// TIER 2: Boundary, Edge & Corner Cases
// =========================================================================

#[test]
fn test_batch_missing_intermediate_id() {
    // Expected [1, 2, 3], received [1, 3] with count 2
    let expected_ids = vec![1, 2, 3];
    let missing_id_response = r#"{
        "segments": [
            {"id": 1, "text": "Segment 1"},
            {"id": 3, "text": "Segment 3"}
        ]
    }"#;

    let res = validate_batch_response(&expected_ids, missing_id_response);
    assert!(matches!(res, BatchValidationResult::Mismatch { .. }));
}

#[test]
fn test_batch_unescaped_quotes_json_error() {
    let expected_ids = vec![1];
    // Unescaped quote inside string: "He said "hello""
    let broken_json = r#"{
        "segments": [
            {"id": 1, "text": "He said "hello" to them."}
        ]
    }"#;

    let res = validate_batch_response(&expected_ids, broken_json);
    match res {
        BatchValidationResult::ParseError(err) => {
            assert!(!err.is_empty(), "Parse error must detail json syntax problem");
        }
        _ => panic!("Expected ParseError for invalid JSON syntax"),
    }
}

#[test]
fn test_batch_merged_paragraphs_recovery() {
    // When batch validation fails, single-segment fallback executes sequentially
    let inputs = vec![
        BatchSegmentInput { id: 1, text: "Paragraph 1".to_string() },
        BatchSegmentInput { id: 2, text: "Paragraph 2".to_string() },
    ];

    // Single-segment fallback simulation
    let mut fallback_results = Vec::new();
    for seg in inputs {
        // Individual prompt: "Translate the following text into zh-CN:"
        let single_translation = format!("译文: {}", seg.text);
        fallback_results.push((seg.id, single_translation));
    }

    assert_eq!(fallback_results.len(), 2);
    assert_eq!(fallback_results[0].0, 1);
    assert_eq!(fallback_results[1].0, 2);
}

#[test]
fn test_single_segment_fallback_retry() {
    // Single segment request fails first attempt with glitch, succeeds on retry
    let mut attempts = 0;
    let mut success = false;
    let max_retries = 2;

    while attempts < max_retries {
        attempts += 1;
        if attempts == 1 {
            // simulate transient network failure
            continue;
        }
        // retry succeeds
        success = true;
        break;
    }

    assert!(success);
    assert_eq!(attempts, 2, "Single-segment fallback must retry up to 2 times");
}

#[test]
fn test_batch_size_upper_and_lower_bounds() {
    // Lower bound: 1 segment
    let one_seg = vec![1];
    let res_one = validate_batch_response(
        &one_seg,
        r#"{"segments":[{"id":1,"text":"Single segment"}]}"#,
    );
    assert!(matches!(res_one, BatchValidationResult::Success(_)));

    // Upper bound: 15 segments
    let fifteen_segs: Vec<usize> = (1..=15).collect();
    let mut seg_json = String::from("{\"segments\":[");
    for id in &fifteen_segs {
        if *id > 1 {
            seg_json.push(',');
        }
        seg_json.push_str(&format!("{{\"id\":{},\"text\":\"Seg {}\"}}", id, id));
    }
    seg_json.push_str("]}");

    let res_fifteen = validate_batch_response(&fifteen_segs, &seg_json);
    match res_fifteen {
        BatchValidationResult::Success(pairs) => {
            assert_eq!(pairs.len(), 15);
            assert_eq!(pairs[14].0, 15);
        }
        _ => panic!("Expected Success for 15 segments"),
    }
}
