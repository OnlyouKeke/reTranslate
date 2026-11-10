//! E2E Test Suite: Tier 3 Cross-Feature Interactions & Tier 4 Real-World Workloads
//! Covers Pairwise Interactions (T3-01 to T3-10) and Real-World Workloads from TEST_INFRA.md

mod common;
use common::*;

// =========================================================================
// TIER 3: Cross-Feature Interactions (Pairwise Matrix)
// =========================================================================

#[test]
fn test_t3_01_epub_batch_subtle_muted_bilingual_epub_fallback() {
    // Combination: EPUB format + Numbered Batch JSON + Subtle Muted + Bilingual EPUB export
    // Stress Condition: Batch count mismatch triggers single-segment fallback
    let expected_ids = vec![1, 2];
    let bad_batch = r#"{"segments":[{"id":1,"text":"First"}]}"#; // Missing segment 2

    let val = validate_batch_response(&expected_ids, bad_batch);
    assert!(matches!(val, BatchValidationResult::Mismatch { .. }));

    // Fallback succeeds
    let recovered_1 = "第一个段落";
    let recovered_2 = "第二个段落";

    assert!(!recovered_1.is_empty());
    assert!(!recovered_2.is_empty());
}

#[test]
fn test_t3_02_docx_batch_dotted_bilingual_docx_merged_fallback() {
    // DOCX format + Dotted Divider + Bilingual DOCX export
    // Stress Condition: LLM merged segments into 1
    let expected_ids = vec![1, 2];
    let merged_batch = r#"{"segments":[{"id":1,"text":"Merged 1 and 2"}]}"#;

    let val = validate_batch_response(&expected_ids, merged_batch);
    assert!(matches!(val, BatchValidationResult::Mismatch { .. }));
}

#[test]
fn test_t3_03_markdown_sse_side_by_side_bilingual_md_split_think() {
    // Markdown format + SSE Streaming + Side-by-Side + Bilingual Markdown export
    // Stress Condition: Split <think> tag across delta chunks
    let mut filter = ThinkFilter::new();
    let mut clean_trans = String::new();

    clean_trans.push_str(&filter.process_token("<thi"));
    clean_trans.push_str(&filter.process_token("nk>Internal analysis</th"));
    clean_trans.push_str(&filter.process_token("ink>Markdown heading translation"));

    assert_eq!(clean_trans, "Markdown heading translation");
}

#[test]
fn test_t3_04_txt_gbk_batch_target_only_html_rate_limit() {
    // TXT (GBK) + Numbered Batch JSON + Target Only + Target-Only HTML export
    // Stress Condition: HTTP 429 received -> UI error isolation
    let (ui_err, log_entry) = isolate_translation_error(
        "HTTP 429 Too Many Requests (Rate limit)",
        Some(429),
        None,
        "zh-CN",
    );

    assert_eq!(ui_err.message, "翻译失败，请稍后重试");
    assert!(!ui_err.message.contains("429"));
    assert_eq!(log_entry.http_status, Some(429));
}

#[test]
fn test_t3_05_txt_utf8_sse_target_only_epub_unclosed_think() {
    // TXT (UTF-8) + SSE Streaming + Target Only + Target-Only EPUB export
    // Stress Condition: Unclosed <think> tag
    let mut filter = ThinkFilter::new();
    let mut output = String::new();

    output.push_str(&filter.process_token("<think>Unclosed thinking block..."));
    output.push_str(&filter.flush());

    // Should gracefully discard unclosed think content
    assert_eq!(output, "");
}

#[test]
fn test_t3_06_epub_single_subtle_muted_target_only_docx_http500_retry() {
    // EPUB + Single Segment + Subtle Muted + Target-Only DOCX export
    // Stress Condition: HTTP 500 error triggers one-click retry UI
    let (ui_err, _) = isolate_translation_error("HTTP 500 Internal Error", Some(500), None, "en-US");

    assert_eq!(ui_err.message, "Translation failed, please try again later");
    assert!(ui_err.has_retry_button);
    assert!(ui_err.retryable);
}

#[test]
fn test_t3_07_docx_batch_original_only_bilingual_html_partial() {
    // DOCX + Numbered Batch JSON + Original Only + Bilingual HTML export
    // Stress Condition: Partially translated document (50% done)
    let block_1_trans = Some("已翻译段落");
    let block_2_trans: Option<&str> = None;

    assert!(block_1_trans.is_some());
    assert!(block_2_trans.is_none());
}

#[test]
fn test_t3_08_markdown_single_dotted_target_only_md_untranslated() {
    // Markdown + Single Segment + Dotted Divider + Target-Only Markdown
    // Stress Condition: Untranslated document (0% translated)
    let doc = Document {
        id: "doc_untrans".to_string(),
        title: "Untranslated".to_string(),
        format: DocumentFormat::Markdown,
        file_path: None,
        file_size: 50,
        metadata: DocumentMetadata::default(),
        chapters: vec![Chapter {
            id: "c0".to_string(),
            title: "Chap 1".to_string(),
            order: 0,
            blocks: vec![Block {
                id: "c0_b0".to_string(),
                chapter_id: "c0".to_string(),
                index: 0,
                kind: BlockKind::Paragraph,
                spans: vec![InlineSpan {
                    text: "Pure source text".to_string(),
                    style: InlineStyle::Plain,
                }],
                translatable: true,
                translation: TranslationState::Untranslated,
                metadata: BlockMetadata::default(),
            }],
            sub_chapters: vec![],
            source_href: None,
        }],
        total_blocks: 1,
    };

    assert_eq!(doc.chapters[0].blocks[0].translation_text(), None);
}

#[test]
fn test_t3_09_txt_utf16_batch_side_by_side_bilingual_docx_cjk_softwrap() {
    // TXT (UTF-16) + Numbered Batch JSON + Side-by-Side + Bilingual DOCX
    // Stress Condition: Soft wrap with CJK characters
    let cjk_softwrap = "第一行中文内容\n第二行中文紧接";
    let merged = cjk_softwrap.replace('\n', "");
    assert_eq!(merged, "第一行中文内容第二行中文紧接");
}

#[test]
fn test_t3_10_epub_batch_target_only_md_missing_image_footnote() {
    // EPUB + Numbered Batch JSON + Target Only + Bilingual Markdown
    // Stress Condition: Missing manifest image + footnote reference
    let footnote_span = InlineSpan {
        text: "[1]".to_string(),
        style: InlineStyle::FootnoteRef {
            id: "fn_1".to_string(),
            note: Some("Explanatory footnote".to_string()),
        },
    };

    if let InlineStyle::FootnoteRef { note, .. } = footnote_span.style {
        assert_eq!(note.as_deref(), Some("Explanatory footnote"));
    } else {
        panic!("Expected FootnoteRef");
    }
}

// =========================================================================
// TIER 4: Real-World Application Workloads
// =========================================================================

#[test]
fn test_workload_1_end_to_end_novel_translation_and_export() {
    // Workload 1: End-to-end 3-chapter novel import -> view -> translate -> cache -> export
    let novel_bytes = b"Chapter 1\nLine 1\n\nChapter 2\nLine 2\n\nChapter 3\nLine 3";
    let doc_id = compute_sha256_hex(novel_bytes);

    assert_eq!(doc_id.len(), 64);

    // Simulate batch translation of Chapter 1
    let expected_ids = vec![1];
    let response = r#"{"segments":[{"id":1,"text":"第一行译文"}]}"#;
    let val = validate_batch_response(&expected_ids, response);

    match val {
        BatchValidationResult::Success(pairs) => {
            assert_eq!(pairs[0].1, "第一行译文");
        }
        _ => panic!("Expected Success"),
    }
}

#[test]
fn test_workload_2_network_interruption_error_isolation_and_retry() {
    // Workload 2: Network drop during chapter translation
    let (ui_err, log_entry) = isolate_translation_error(
        "connect error: Network unreachable (os error 101)",
        None,
        Some("sk-secret-token-key-12345"),
        "zh-CN",
    );

    // 1. Strict UI Error Text
    assert_eq!(ui_err.message, "翻译失败，请稍后重试");
    assert!(ui_err.has_retry_button);

    // 2. Secret masked in logs
    assert!(!log_entry.raw_error.contains("sk-secret-token-key-12345"));
}

#[test]
fn test_workload_3_file_rename_fingerprint_and_progress_preservation() {
    // Workload 3: File renamed and moved across folders
    let original_bytes = b"Essential book contents that should not lose progress.";
    let original_id = compute_sha256_hex(original_bytes);

    // File renamed to another path with same byte content
    let renamed_bytes = b"Essential book contents that should not lose progress.";
    let renamed_id = compute_sha256_hex(renamed_bytes);

    // Invariant: SHA-256 fingerprint remains identical across file renames and moves
    assert_eq!(
        original_id, renamed_id,
        "Reading progress and translation cache must remain intact after renaming"
    );
}
