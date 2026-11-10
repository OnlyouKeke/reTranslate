//! E2E Test Suite: Document Models, Fingerprinting & AST Integrity
//! Covers Feature 1-6 from PROJECT.md & TEST_INFRA.md (Tier 1 & Tier 2)

mod common;
use common::*;

// =========================================================================
// TIER 1: Feature Coverage (Nominal Paths)
// =========================================================================

#[test]
fn test_sha256_identical_content_different_names() {
    let content = b"reTranslate: High-performance bilingual reader in Rust.";
    let hash1 = compute_sha256_hex(content);
    let hash2 = compute_sha256_hex(content);

    assert_eq!(hash1, hash2, "Identical content must yield identical SHA-256");
    assert_eq!(hash1.len(), 64, "SHA-256 hex string must be exactly 64 characters");
}

#[test]
fn test_sha256_different_content_distinct_ids() {
    let content_a = b"The quick brown fox jumps over the lazy dog";
    let content_b = b"The quick brown fox jumps over the lazy cog"; // 1-bit alteration

    let hash_a = compute_sha256_hex(content_a);
    let hash_b = compute_sha256_hex(content_b);

    assert_ne!(hash_a, hash_b, "Different content must produce distinct SHA-256 hashes");
}

#[test]
fn test_ast_hierarchy_navigation() {
    let doc_bytes = b"Sample Document Body";
    let doc_id = compute_sha256_hex(doc_bytes);

    let span1 = InlineSpan {
        text: "Chapter Title".to_string(),
        style: InlineStyle::Plain,
    };
    let block1 = Block {
        id: "chap_0_b0".to_string(),
        chapter_id: "chap_0".to_string(),
        index: 0,
        kind: BlockKind::Heading(1),
        spans: vec![span1],
        translatable: true,
        translation: TranslationState::Untranslated,
        metadata: BlockMetadata::default(),
    };

    let span2 = InlineSpan {
        text: "Paragraph text with some words.".to_string(),
        style: InlineStyle::Plain,
    };
    let block2 = Block {
        id: "chap_0_b1".to_string(),
        chapter_id: "chap_0".to_string(),
        index: 1,
        kind: BlockKind::Paragraph,
        spans: vec![span2],
        translatable: true,
        translation: TranslationState::Untranslated,
        metadata: BlockMetadata::default(),
    };

    let chapter = Chapter {
        id: "chap_0".to_string(),
        title: "Introduction".to_string(),
        order: 0,
        blocks: vec![block1, block2],
        sub_chapters: vec![],
        source_href: Some("intro.xhtml".to_string()),
    };

    let doc = Document {
        id: doc_id,
        title: "Test Book".to_string(),
        format: DocumentFormat::Epub,
        file_path: None,
        file_size: doc_bytes.len() as u64,
        metadata: DocumentMetadata {
            author: Some("Author Name".to_string()),
            language: Some("en".to_string()),
            ..Default::default()
        },
        chapters: vec![chapter],
        total_blocks: 2,
    };

    assert_eq!(doc.chapters.len(), 1);
    assert_eq!(doc.chapters[0].blocks.len(), 2);
    assert_eq!(doc.total_blocks, 2);
    assert_eq!(doc.chapters[0].blocks[0].plain_text(), "Chapter Title");
    assert_eq!(doc.chapters[0].blocks[1].plain_text(), "Paragraph text with some words.");
}

#[test]
fn test_block_plain_text_extraction() {
    let spans = vec![
        InlineSpan {
            text: "Hello ".to_string(),
            style: InlineStyle::Plain,
        },
        InlineSpan {
            text: "World".to_string(),
            style: InlineStyle::Bold,
        },
        InlineSpan {
            text: "!".to_string(),
            style: InlineStyle::Italic,
        },
    ];

    let block = Block {
        id: "c0_b0".to_string(),
        chapter_id: "c0".to_string(),
        index: 0,
        kind: BlockKind::Paragraph,
        spans,
        translatable: true,
        translation: TranslationState::Untranslated,
        metadata: BlockMetadata::default(),
    };

    assert_eq!(block.plain_text(), "Hello World!");
}

#[test]
fn test_translation_state_transitions() {
    let mut block = Block {
        id: "c0_b0".to_string(),
        chapter_id: "c0".to_string(),
        index: 0,
        kind: BlockKind::Paragraph,
        spans: vec![InlineSpan {
            text: "Original Text".to_string(),
            style: InlineStyle::Plain,
        }],
        translatable: true,
        translation: TranslationState::Untranslated,
        metadata: BlockMetadata::default(),
    };

    assert_eq!(block.translation_text(), None);

    // 1. Mark Translating
    block.translation = TranslationState::Translating;
    assert_eq!(block.translation_text(), None);

    // 2. Mark Translated
    block.translation = TranslationState::Translated {
        text: "译文文本".to_string(),
        spans: vec![InlineSpan {
            text: "译文文本".to_string(),
            style: InlineStyle::Plain,
        }],
        provider: Some("deepseek".to_string()),
        model: Some("deepseek-chat".to_string()),
        updated_at: 1700000000,
    };
    assert_eq!(block.translation_text(), Some("译文文本"));
}

// =========================================================================
// TIER 2: Boundary, Edge & Corner Cases
// =========================================================================

#[test]
fn test_sha256_empty_file() {
    let empty_content = b"";
    let hash = compute_sha256_hex(empty_content);

    // The standard authoritative SHA-256 for an empty byte sequence:
    // e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855
    assert_eq!(
        hash,
        "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "Empty byte slice must produce standard SHA-256 digest"
    );
}

#[test]
fn test_ast_empty_document() {
    let doc = Document {
        id: compute_sha256_hex(b""),
        title: "Empty Document".to_string(),
        format: DocumentFormat::Txt,
        file_path: None,
        file_size: 0,
        metadata: DocumentMetadata::default(),
        chapters: vec![],
        total_blocks: 0,
    };

    assert_eq!(doc.total_blocks, 0);
    assert!(doc.chapters.is_empty());
}

#[test]
fn test_block_kind_code_block_translatable_false() {
    let code_block = Block {
        id: "c0_b3".to_string(),
        chapter_id: "c0".to_string(),
        index: 3,
        kind: BlockKind::CodeBlock {
            language: Some("rust".to_string()),
            code: "fn main() { println!(\"Hello\"); }".to_string(),
        },
        spans: vec![InlineSpan {
            text: "fn main() { println!(\"Hello\"); }".to_string(),
            style: InlineStyle::Code,
        }],
        translatable: false, // Invariant: code blocks are not sent to translation engine
        translation: TranslationState::Untranslated,
        metadata: BlockMetadata::default(),
    };

    assert!(!code_block.translatable, "Code blocks must have translatable == false");
    if let BlockKind::CodeBlock { language, code } = code_block.kind {
        assert_eq!(language, Some("rust".to_string()));
        assert!(code.contains("println!"));
    } else {
        panic!("Expected BlockKind::CodeBlock");
    }
}

#[test]
fn test_inline_span_nested_formatting() {
    let spans = vec![
        InlineSpan {
            text: "Check this ".to_string(),
            style: InlineStyle::Plain,
        },
        InlineSpan {
            text: "link".to_string(),
            style: InlineStyle::Link {
                url: "https://example.com".to_string(),
            },
        },
        InlineSpan {
            text: " with footnote".to_string(),
            style: InlineStyle::BoldItalic,
        },
        InlineSpan {
            text: "[1]".to_string(),
            style: InlineStyle::FootnoteRef {
                id: "fn1".to_string(),
                note: Some("Citation note".to_string()),
            },
        },
    ];

    assert_eq!(spans.len(), 4);
    assert_eq!(spans[1].style, InlineStyle::Link { url: "https://example.com".to_string() });
    assert_eq!(spans[2].style, InlineStyle::BoldItalic);
    if let InlineStyle::FootnoteRef { id, note } = &spans[3].style {
        assert_eq!(id, "fn1");
        assert_eq!(note.as_deref(), Some("Citation note"));
    } else {
        panic!("Expected FootnoteRef");
    }
}

#[test]
fn test_block_translation_failure_state() {
    let block = Block {
        id: "c0_b5".to_string(),
        chapter_id: "c0".to_string(),
        index: 5,
        kind: BlockKind::Paragraph,
        spans: vec![InlineSpan {
            text: "Difficult sentence.".to_string(),
            style: InlineStyle::Plain,
        }],
        translatable: true,
        translation: TranslationState::Failed {
            error: "Connection timeout".to_string(),
            retryable: true,
        },
        metadata: BlockMetadata::default(),
    };

    match &block.translation {
        TranslationState::Failed { error, retryable } => {
            assert_eq!(error, "Connection timeout");
            assert!(*retryable, "Timeout errors must be marked retryable");
        }
        _ => panic!("Expected TranslationState::Failed"),
    }
}
