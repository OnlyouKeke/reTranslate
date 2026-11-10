//! Challenger 1 Stress & Adversarial Test Suite for Milestone M1
//! Probing Targets:
//! 1. TXT Encodings & Paragraph Segmentation:
//!    - Massive files (10MB+ and 100,000 lines)
//!    - Thousands of consecutive blank lines & extreme whitespace
//!    - Mixed CRLF / LF / CR newline chaos
//!    - Exotic encodings: GBK with odd byte splits, UTF-16LE/BE with surrogate pairs,
//!      invalid byte sequences recovering via replacement characters, BOM variants
//!    - CJK mixed with ASCII soft-wraps without extra spaces
//!    - Extreme monolithic lines (sentence chunking boundary)
//! 2. Markdown Parsing & Structural Conformance:
//!    - Deeply nested lists (depth 10+ to 15)
//!    - Unclosed tags & malformed CommonMark / HTML
//!    - Thousands of table rows with alignments
//!    - Massive code blocks with Unicode art, box drawings, and astral symbols
//!    - Empty input & whitespace boundaries
//!    - Documents with no headings vs documents with hundreds of H1 headings
//! 3. Concurrency & Thread Safety:
//!    - High concurrency multi-threaded document parsing (16 threads, 320 operations)
//!    - Deterministic SHA-256 fingerprint validation under parallel load
//!    - Send + Sync trait object invocation across threads

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

use retranslate::document::error::DocumentError;
use retranslate::document::markdown::{
    parse_markdown_from_str, MarkdownParser, MarkdownParserOptions,
};
use retranslate::document::model::*;
use retranslate::document::txt::{
    detect_and_decode, normalize_newlines, parse_txt_from_bytes, segment_txt_paragraphs,
    TxtParser,
};
use retranslate::document::{load_document_from_bytes, DocumentParser};

// =========================================================================
// SECTION 1: TXT Adversarial & Boundary Stress Tests
// =========================================================================

#[test]
fn test_txt_massive_10mb_file_stress() {
    // Generate a structured TXT buffer exceeding 10 MB (approx 10.5 MB)
    let paragraph_template = "This is a recurring test paragraph in the massive 10MB document workload. \
It contains multiple sentences to simulate real literary text. \
这是一段包含中文的混合文本，用于验证大文件解析与编码检测的吞吐量与内存边界。\n\n";

    let template_bytes = paragraph_template.as_bytes();
    let target_size = 10 * 1024 * 1024 + 512 * 1024; // ~10.5 MB
    let repetitions = target_size / template_bytes.len() + 1;

    let mut massive_buffer = Vec::with_capacity(repetitions * template_bytes.len() + 1024);
    massive_buffer.extend_from_slice(b"Chapter 1: The Massive Volume Beginning\n\n");

    for i in 0..repetitions {
        if i > 0 && i % 5000 == 0 {
            let chap_marker = format!("Chapter {}: Intermediate Checkpoint\n\n", (i / 5000) + 1);
            massive_buffer.extend_from_slice(chap_marker.as_bytes());
        }
        massive_buffer.extend_from_slice(template_bytes);
    }

    assert!(
        massive_buffer.len() > 10 * 1024 * 1024,
        "Buffer must exceed 10MB, actual: {} bytes",
        massive_buffer.len()
    );

    let start = Instant::now();
    let doc = parse_txt_from_bytes(&massive_buffer, Some("Massive 10MB Novel")).expect("10MB parse must succeed");
    let duration = start.elapsed();

    assert_eq!(doc.format, DocumentFormat::Txt);
    assert_eq!(doc.title, "Massive 10MB Novel");
    assert!(doc.file_size >= 10 * 1024 * 1024);
    assert!(doc.chapters.len() >= 2, "Should detect multiple chapters, found: {}", doc.chapters.len());
    assert!(doc.total_blocks > 5000, "Should extract thousands of blocks, found: {}", doc.total_blocks);
    assert_eq!(doc.id.len(), 64, "SHA-256 fingerprint must be valid 64-char hex");

    // Ensure throughput is reasonable (< 5 seconds for 10MB in debug mode)
    assert!(
        duration.as_secs() < 5,
        "10MB TXT parsing took too long: {:?}",
        duration
    );
}

#[test]
fn test_txt_100_000_lines_stress() {
    // Generate 100,000 lines of text
    let mut buffer = String::with_capacity(5_000_000);
    const LINE_COUNT: usize = 100_000;

    for i in 0..LINE_COUNT {
        if i % 2 == 0 {
            buffer.push_str(&format!("Line {} of the one-hundred-thousand lines dataset.\n", i));
        } else {
            // Empty line every other line to form distinct paragraphs
            buffer.push('\n');
        }
    }

    let start = Instant::now();
    let paragraphs = segment_txt_paragraphs(&buffer);
    let duration = start.elapsed();

    // 50,000 non-empty lines separated by double newlines = 50,000 paragraphs
    assert_eq!(paragraphs.len(), LINE_COUNT / 2);
    assert!(
        duration.as_secs() < 3,
        "100,000 lines segmentation took too long: {:?}",
        duration
    );

    // Verify document creation from this buffer
    let doc = parse_txt_from_bytes(buffer.as_bytes(), Some("100K Lines")).expect("100K lines TXT must parse");
    assert_eq!(doc.total_blocks, LINE_COUNT / 2);
    assert_eq!(doc.chapters[0].blocks[0].id, "chap_0_b0");
    assert_eq!(
        doc.chapters[0].blocks[paragraphs.len() - 1].id,
        format!("chap_0_b{}", paragraphs.len() - 1)
    );
}

#[test]
fn test_txt_thousands_of_consecutive_blank_lines() {
    // 10,000 consecutive newlines between two paragraphs
    let mut excessive = String::with_capacity(20_000);
    excessive.push_str("Leading Valid Paragraph\n");
    for _ in 0..10_000 {
        excessive.push('\n');
    }
    excessive.push_str("Trailing Valid Paragraph\n");

    let paragraphs = segment_txt_paragraphs(&excessive);
    assert_eq!(
        paragraphs.len(),
        2,
        "10,000 consecutive blank lines must collapse into single separation"
    );
    assert_eq!(paragraphs[0], "Leading Valid Paragraph");
    assert_eq!(paragraphs[1], "Trailing Valid Paragraph");

    // Thousands of CRLF blank lines
    let mut crlf_excessive = String::with_capacity(30_000);
    crlf_excessive.push_str("First CRLF Block\r\n");
    for _ in 0..5_000 {
        crlf_excessive.push_str("\r\n");
    }
    crlf_excessive.push_str("Second CRLF Block\r\n");

    let crlf_paragraphs = segment_txt_paragraphs(&crlf_excessive);
    assert_eq!(crlf_paragraphs.len(), 2);
    assert_eq!(crlf_paragraphs[0], "First CRLF Block");
    assert_eq!(crlf_paragraphs[1], "Second CRLF Block");

    // Pure blank lines file with 20,000 newlines must fail with EmptyDocument
    let mut pure_blanks = String::with_capacity(25_000);
    for _ in 0..20_000 {
        pure_blanks.push_str("   \t  \n");
    }
    let res = parse_txt_from_bytes(pure_blanks.as_bytes(), Some("Only Blanks"));
    assert!(matches!(res, Err(DocumentError::EmptyDocument)));
}

#[test]
fn test_txt_mixed_crlf_lf_cr_newline_chaos() {
    // Chaotic mixture of Windows (\r\n), Unix (\n), Classic Mac (\r), double CR-LF (\r\r\n)
    let chaos = "Line 1\r\n\r\nLine 2\r\rLine 3\n\nLine 4\r\r\nLine 5\n\r\nLine 6\r\n\rLine 7";
    let normalized = normalize_newlines(chaos);

    // Invariant: Normalized output must not contain ANY carriage return '\r'
    assert!(!normalized.contains('\r'), "Normalized text must not contain '\\r'");

    let paragraphs = segment_txt_paragraphs(chaos);
    assert!(paragraphs.len() >= 5, "Must segment into distinct paragraphs");
    assert_eq!(paragraphs[0], "Line 1");
    assert_eq!(paragraphs[1], "Line 2");
    assert_eq!(paragraphs[2], "Line 3");
    assert_eq!(paragraphs[3], "Line 4");
}

#[test]
fn test_txt_gbk_odd_byte_splits_and_truncations() {
    // GBK Chinese character bytes:
    // "你" -> 0xC4, 0xE3
    // "好" -> 0xBA, 0xC3
    // "世" -> 0xCA, 0xC0
    // "界" -> 0xBD, 0xE7
    let valid_gbk = vec![0xC4, 0xE3, 0xBA, 0xC3, 0xCA, 0xC0, 0xBD, 0xE7];
    let decoded = detect_and_decode(&valid_gbk).expect("Valid GBK must decode cleanly");
    assert_eq!(decoded, "你好世界");

    // Test longer GBK with odd split:
    let mut long_gbk = vec![
        0xb5, 0xda, 0xd2, 0xbb, 0xd5, 0xc2, 0x20, 0xcf, 0xc9, 0xb5, 0xc0, 0xc7, 0xf3, 0xcb, 0xf7,
        0x20, 0xd5, 0xe2, 0xca, 0xc7, 0xd6, 0xd0, 0xce, 0xc4, 0xd0, 0xa1, 0xcb, 0xb5, 0xb5, 0xc4,
        0xd5, 0xfd, 0xce, 0xc4, 0xc4, 0xda, 0xc8, 0xdd, 0xa3, 0xac, 0xc3, 0xe8, 0xca, 0xf6, 0xc1,
        0x53, 0xd2, 0xbb, 0xb8, 0xf6, 0xbe, 0xab, 0xb2, 0xca, 0xb5, 0xc4, 0xb9, 0xca, 0xca, 0xc2,
        0xa1, 0xa3,
    ];
    let decoded_long = detect_and_decode(&long_gbk).expect("Long GBK must decode cleanly");
    assert!(decoded_long.contains("第一章"), "Valid long GBK must contain '第一章'");

    // Truncate long GBK with an odd byte split (appending half of a 2-byte Chinese character)
    long_gbk.push(0xC4);
    let decoded_long_odd = detect_and_decode(&long_gbk).expect("Long GBK with odd split must decode without error");
    assert!(
        decoded_long_odd.contains("第一章"),
        "GBK with trailing odd byte split must recover Chinese text via replacement char, got mojibake: {}",
        decoded_long_odd
    );

    // Adversarial: Short GBK with odd byte split at EOF ("你好" + 0xC4)
    let odd_split_eof = vec![0xC4, 0xE3, 0xBA, 0xC3, 0xC4];
    let decoded_odd = detect_and_decode(&odd_split_eof).expect("Odd split at EOF must not panic");
    assert!(
        decoded_odd.contains("你好"),
        "Short GBK with odd split must preserve decodable Chinese characters via replacement char, got mojibake: {}",
        decoded_odd
    );

    // Adversarial: Odd split in the middle followed by valid ASCII
    let mut odd_split_mid = vec![0xC4, 0xE3]; // "你"
    odd_split_mid.push(0xBA); // First half of "好"
    odd_split_mid.extend_from_slice(b" - English text follows\n\nSecond line");
    let decoded_mid = detect_and_decode(&odd_split_mid).expect("Mid-stream odd split must recover");
    assert!(decoded_mid.contains("English text follows"));

    // Verify document parser accepts odd-split GBK buffer
    let doc = parse_txt_from_bytes(&odd_split_eof, Some("Odd GBK")).expect("Document parse must succeed");
    assert!(!doc.chapters.is_empty());
}

#[test]
fn test_txt_utf16_surrogate_pairs_le_and_be() {
    // Astral plane characters requiring UTF-16 surrogate pairs:
    // 😀 U+1F600: High surrogate = 0xD83D, Low surrogate = 0xDE00
    // 🦀 U+1F980: High surrogate = 0xD83E, Low surrogate = 0xDD80
    // 𠀀 U+20000 (CJK Ext B): High surrogate = 0xD840, Low surrogate = 0xDC00

    // UTF-16LE with BOM
    let mut utf16le = vec![0xFF, 0xFE]; // LE BOM
    // "Hello " in LE:
    for &b in b"Hello " {
        utf16le.push(b);
        utf16le.push(0x00);
    }
    // 😀 (0xD83D, 0xDE00) in LE:
    utf16le.extend_from_slice(&[0x3D, 0xD8, 0x00, 0xDE]);
    // " " in LE:
    utf16le.extend_from_slice(&[0x20, 0x00]);
    // 🦀 (0xD83E, 0xDD80) in LE:
    utf16le.extend_from_slice(&[0x3E, 0xD8, 0x80, 0xDD]);

    let decoded_le = detect_and_decode(&utf16le).expect("UTF-16LE with surrogate pairs must decode");
    assert_eq!(decoded_le, "Hello 😀 🦀");

    // UTF-16BE with BOM
    let mut utf16be = vec![0xFE, 0xFF]; // BE BOM
    for &b in b"Hello " {
        utf16be.push(0x00);
        utf16be.push(b);
    }
    // 😀 (0xD83D, 0xDE00) in BE:
    utf16be.extend_from_slice(&[0xD8, 0x3D, 0xDE, 0x00]);
    // " " in BE:
    utf16be.extend_from_slice(&[0x00, 0x20]);
    // 🦀 (0xD83E, 0xDD80) in BE:
    utf16be.extend_from_slice(&[0xD8, 0x3E, 0xDD, 0x80]);

    let decoded_be = detect_and_decode(&utf16be).expect("UTF-16BE with surrogate pairs must decode");
    assert_eq!(decoded_be, "Hello 😀 🦀");

    // Adversarial: Unpaired UTF-16LE surrogate (High surrogate without low surrogate)
    let unpaired_le = vec![0xFF, 0xFE, 0x3D, 0xD8, 0x41, 0x00];
    let res_unpaired = detect_and_decode(&unpaired_le);
    assert!(
        matches!(res_unpaired, Err(DocumentError::EncodingError(_))),
        "Unpaired UTF-16 surrogate must fail cleanly with EncodingError"
    );

    // Adversarial: Truncated odd byte count in UTF-16LE
    let odd_len_le = vec![0xFF, 0xFE, 0x41];
    let res_odd_len = detect_and_decode(&odd_len_le);
    assert!(
        matches!(res_odd_len, Err(DocumentError::EncodingError(_))),
        "Odd-length UTF-16 stream must return EncodingError"
    );
}

#[test]
fn test_txt_cjk_ascii_mixed_soft_wraps_spacing() {
    // Rule:
    // 1. Latin to Latin soft wrap -> inserts space (' ')
    // 2. CJK to CJK soft wrap -> NO space
    // 3. CJK to Latin soft wrap -> NO space
    // 4. Latin to CJK soft wrap -> NO space
    // 5. CJK punctuation to text -> NO space

    let input = "Case 1: Latin to Latin\nwords with continuation.\n\n\
Case 2: 中文到中文换行，\n第二行继续叙述。\n\n\
Case 3: CJK to Latin 混合\nAlgorithm is evaluated.\n\n\
Case 4: Latin to CJK continuation\n算法评估结果优异。\n\n\
Case 5: 标点符号在行末！\n下一句立刻跟上。";

    let paragraphs = segment_txt_paragraphs(input);
    assert_eq!(paragraphs.len(), 5);

    // Case 1: Latin continuation must insert space
    assert!(
        paragraphs[0].contains("Latin words with continuation"),
        "Latin to Latin must insert space: {}",
        paragraphs[0]
    );

    // Case 2: CJK to CJK must NOT insert space
    assert!(
        paragraphs[1].contains("中文到中文换行，第二行继续叙述。"),
        "CJK to CJK must NOT insert space: {}",
        paragraphs[1]
    );

    // Case 3: CJK to Latin must NOT insert space
    assert!(
        paragraphs[2].contains("混合Algorithm"),
        "CJK to Latin must NOT insert space: {}",
        paragraphs[2]
    );

    // Case 4: Latin to CJK must NOT insert space
    assert!(
        paragraphs[3].contains("continuation算法"),
        "Latin to CJK must NOT insert space: {}",
        paragraphs[3]
    );

    // Case 5: CJK punctuation to text must NOT insert space
    assert!(
        paragraphs[4].contains("行末！下一句"),
        "CJK punctuation must NOT insert space: {}",
        paragraphs[4]
    );
}

#[test]
fn test_txt_extreme_monolithic_line_chunking() {
    // Single monolithic line of 10,000 characters with Chinese full stop punctuation '。'
    let mut long_cjk = String::with_capacity(12_000);
    for i in 0..30 {
        long_cjk.push_str(&format!(
            "这是第{}段非常漫长的单行无换行文学文本，讲述了一个关于时空穿越与星际文明的宏大故事。",
            i
        ));
        for _ in 0..5 {
            long_cjk.push_str("持续扩展的内容填充句子以确保单句长度达到拆分阈值。");
        }
        long_cjk.push('。');
    }

    assert!(long_cjk.chars().count() > 3000);
    let paragraphs = segment_txt_paragraphs(&long_cjk);

    // Since text > 1500 chars and has '。' delimiters every ~300+ chars, it must be chunked
    assert!(
        paragraphs.len() > 1,
        "Monolithic long line must be chunked into multiple blocks, got {}",
        paragraphs.len()
    );

    for p in &paragraphs {
        assert!(!p.is_empty());
        assert!(!p.starts_with(' '));
        assert!(!p.ends_with(' '));
    }

    // Monolithic line with NO sentence punctuation at all (10,000 'A's)
    let unpunctuated = "A".repeat(10_000);
    let p_unpunctuated = segment_txt_paragraphs(&unpunctuated);
    assert_eq!(p_unpunctuated.len(), 1, "Unpunctuated monolithic line remains single chunk");
    assert_eq!(p_unpunctuated[0].len(), 10_000);
}

// =========================================================================
// SECTION 2: Markdown Adversarial & Boundary Stress Tests
// =========================================================================

#[test]
fn test_md_deeply_nested_lists_depth_15() {
    // Generate Markdown list nested 15 levels deep
    let mut md = String::new();
    md.push_str("# Deeply Nested Hierarchy\n\n");

    for depth in 0..15 {
        let indent = "  ".repeat(depth);
        md.push_str(&format!("{}- Level {} item\n", indent, depth));
    }

    let doc = parse_markdown_from_str(&md, Some("Deep Lists")).expect("Deep nested list must parse");
    assert_eq!(doc.chapters.len(), 1);

    let list_blocks: Vec<&Block> = doc.chapters[0]
        .blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::ListItem { .. }))
        .collect();

    assert_eq!(list_blocks.len(), 15, "Should extract exactly 15 list item blocks");

    for (expected_depth, block) in list_blocks.iter().enumerate() {
        if let BlockKind::ListItem { depth, order } = block.kind {
            assert_eq!(depth, expected_depth as u8, "Depth mismatch for item {}", expected_depth);
            assert_eq!(order, None, "Unordered list item order must be None");
            assert_eq!(block.plain_text(), format!("Level {} item", expected_depth));
        } else {
            panic!("Expected ListItem block");
        }
    }
}

#[test]
fn test_md_unclosed_tags_and_malformed_syntax() {
    // 1. Unclosed fenced code block at end of file
    let unclosed_code = "# Code Doc\n\n```rust\nfn calculate() -> i32 {\n    42\n// Missing closing fence at EOF";
    let doc_code = parse_markdown_from_str(unclosed_code, Some("Unclosed Code")).expect("Unclosed code block must parse");
    assert_eq!(doc_code.chapters[0].blocks.len(), 2);
    let code_block = &doc_code.chapters[0].blocks[1];
    assert!(!code_block.translatable);
    if let BlockKind::CodeBlock { language, code } = &code_block.kind {
        assert_eq!(language.as_deref(), Some("rust"));
        assert!(code.contains("calculate()"));
    } else {
        panic!("Expected CodeBlock");
    }

    // 2. Unclosed formatting tags (bold, italic, strikethrough) across paragraphs
    let unclosed_inline = "# Formatting\n\n**Unclosed bold paragraph\n\n*Unclosed italic text\n\n~~Unclosed strike";
    let doc_inline = parse_markdown_from_str(unclosed_inline, Some("Unclosed Inline")).expect("Unclosed inline styles must parse");
    assert_eq!(doc_inline.chapters[0].blocks.len(), 4);

    // 3. Unclosed blockquote
    let unclosed_quote = "> This quote never has a closing marker\n> It just spans multiple lines\n\nFollowing paragraph.";
    let doc_quote = parse_markdown_from_str(unclosed_quote, Some("Quote")).expect("Unclosed quote must parse");
    assert_eq!(doc_quote.chapters[0].blocks.len(), 2);
    assert!(matches!(doc_quote.chapters[0].blocks[0].kind, BlockKind::Quote));
    assert!(matches!(doc_quote.chapters[0].blocks[1].kind, BlockKind::Paragraph));

    // 4. Malformed table (missing closing pipes, missing rows)
    let malformed_table = "| Col 1 | Col 2\n| --- | ---\n| Val 1 | Val 2";
    let doc_table = parse_markdown_from_str(malformed_table, Some("Malformed Table")).expect("Malformed table must not panic");
    assert!(!doc_table.chapters[0].blocks.is_empty());
}

#[test]
fn test_md_thousands_of_table_rows_stress() {
    // Construct a Markdown table with 3,000 data rows and 4 columns with different alignments
    let mut table_md = String::with_capacity(500_000);
    table_md.push_str("# High-Volume Table Document\n\n");
    table_md.push_str("| ID (Left) | Metric (Center) | Score (Right) | Notes (None) |\n");
    table_md.push_str("| :--- | :---: | ---: | --- |\n");

    const ROW_COUNT: usize = 3_000;
    for i in 0..ROW_COUNT {
        table_md.push_str(&format!(
            "| Item_{} | Benchmark_{} | {:.2} | Validated record |\n",
            i,
            i % 10,
            (i as f64) * 1.5
        ));
    }

    let start = Instant::now();
    let doc = parse_markdown_from_str(&table_md, Some("Large Table")).expect("3000-row table must parse");
    let duration = start.elapsed();

    assert_eq!(doc.chapters[0].blocks.len(), 2); // 1 Heading + 1 Table
    let table_block = &doc.chapters[0].blocks[1];

    if let BlockKind::Table { headers, rows } = &table_block.kind {
        assert_eq!(headers.len(), 4);
        assert_eq!(headers[0].align, TableAlignment::Left);
        assert_eq!(headers[1].align, TableAlignment::Center);
        assert_eq!(headers[2].align, TableAlignment::Right);
        assert_eq!(headers[3].align, TableAlignment::None);

        assert_eq!(rows.len(), ROW_COUNT);
        assert_eq!(rows[0].len(), 4);
        assert_eq!(rows[ROW_COUNT - 1].len(), 4);
    } else {
        panic!("Expected BlockKind::Table");
    }

    assert!(
        duration.as_secs() < 3,
        "3,000-row table parse took too long: {:?}",
        duration
    );
}

#[test]
fn test_md_massive_code_block_with_unicode_art() {
    // Code block with 3,000 lines containing box drawing characters, braille, astral math, and ANSI escapes
    let mut code_content = String::with_capacity(300_000);
    for i in 0..3_000 {
        code_content.push_str(&format!(
            "┌─[{:04}]─┬───┬───┐ │ ⠋⠙⠹ 🦀 👨‍👩‍👧‍👦 │ ∑(x_i) = ∫ f(t)dt │ \\x1b[32mOK\\x1b[0m └─┴─┴─┘\n",
            i
        ));
    }

    let mut md = String::new();
    md.push_str("# Complex Code Art\n\n```ascii-art\n");
    md.push_str(&code_content);
    md.push_str("```\n\nClosing paragraph.");

    let doc = parse_markdown_from_str(&md, Some("Unicode Art Doc")).expect("Unicode art code block must parse");
    assert_eq!(doc.chapters[0].blocks.len(), 3);

    let code_block = &doc.chapters[0].blocks[1];
    assert!(!code_block.translatable, "Code block must be strictly untranslatable");
    assert!(code_block.spans.is_empty(), "Code block must have empty spans");

    if let BlockKind::CodeBlock { language, code } = &code_block.kind {
        assert_eq!(language.as_deref(), Some("ascii-art"));
        assert!(code.contains("┌─[0000]─"));
        assert!(code.contains("┌─[2999]─"));
        assert!(code.contains("👨‍👩‍👧‍👦"));
        assert!(code.contains("∑(x_i)"));
    } else {
        panic!("Expected CodeBlock");
    }
}

#[test]
fn test_md_empty_input_and_whitespace_boundaries() {
    // Pure empty
    assert!(matches!(
        parse_markdown_from_str("", None),
        Err(DocumentError::EmptyDocument)
    ));

    // Whitespace only
    assert!(matches!(
        parse_markdown_from_str("     \t\t   ", None),
        Err(DocumentError::EmptyDocument)
    ));

    // Multi-line whitespace
    assert!(matches!(
        parse_markdown_from_str("\n\n\r\n\t  \r  \n", None),
        Err(DocumentError::EmptyDocument)
    ));

    // Empty document via load_document_from_bytes
    assert!(matches!(
        load_document_from_bytes(b"", DocumentFormat::Markdown, None),
        Err(DocumentError::EmptyDocument)
    ));
    assert!(matches!(
        load_document_from_bytes(b"", DocumentFormat::Txt, None),
        Err(DocumentError::EmptyDocument)
    ));
}

#[test]
fn test_md_headings_extremes_zero_vs_hundreds_of_h1() {
    // Case A: 0 headings, 100 paragraphs
    let mut no_headings = String::new();
    for i in 0..100 {
        no_headings.push_str(&format!("This is regular paragraph number {}.\n\n", i));
    }
    let doc_no_headings = parse_markdown_from_str(&no_headings, Some("No Headings")).expect("Doc with no headings must parse");
    assert_eq!(doc_no_headings.chapters.len(), 1);
    assert_eq!(doc_no_headings.chapters[0].title, "No Headings");
    assert_eq!(doc_no_headings.total_blocks, 100);

    // Case B: 250 H1 Headings with split_on_h1 = true
    let mut many_h1 = String::new();
    for i in 0..250 {
        many_h1.push_str(&format!("# Section {}\n\nContent for section {}.\n\n", i + 1, i + 1));
    }

    let doc_split_h1 = parse_markdown_from_str(&many_h1, Some("250 Chapters")).expect("250 H1 doc must parse");
    assert_eq!(doc_split_h1.chapters.len(), 250, "Every H1 must form a new chapter");
    assert_eq!(doc_split_h1.chapters[0].title, "Section 1");
    assert_eq!(doc_split_h1.chapters[249].title, "Section 250");
    assert_eq!(doc_split_h1.total_blocks, 500); // 250 headings + 250 paragraphs

    // Test block finding in chapter 249
    let last_block_id = "chap_249_b1";
    let found = doc_split_h1.find_block(last_block_id);
    assert!(found.is_some());
    assert_eq!(found.unwrap().plain_text(), "Content for section 250.");

    // Case C: 250 H1 Headings with split_on_h1 = false
    let opts = MarkdownParserOptions {
        split_on_h1: false,
        split_on_horizontal_rule: false,
    };
    let doc_no_split = MarkdownParser::parse_str(&many_h1, Some("Single Chapter"), None, Some(opts))
        .expect("Non-splitting markdown must parse");
    assert_eq!(doc_no_split.chapters.len(), 1, "Must remain single chapter when split_on_h1 is false");
    assert_eq!(doc_no_split.total_blocks, 500);

    // Case D: Full heading depth H1 through H6
    let full_depth_md = "# H1 Level\n## H2 Level\n### H3 Level\n#### H4 Level\n##### H5 Level\n###### H6 Level\nParagraph";
    let doc_depth = parse_markdown_from_str(full_depth_md, Some("Heading Depths")).expect("H1-H6 doc must parse");
    assert_eq!(doc_depth.chapters[0].blocks[0].kind, BlockKind::Heading(1));
    assert_eq!(doc_depth.chapters[0].blocks[1].kind, BlockKind::Heading(2));
    assert_eq!(doc_depth.chapters[0].blocks[2].kind, BlockKind::Heading(3));
    assert_eq!(doc_depth.chapters[0].blocks[3].kind, BlockKind::Heading(4));
    assert_eq!(doc_depth.chapters[0].blocks[4].kind, BlockKind::Heading(5));
    assert_eq!(doc_depth.chapters[0].blocks[5].kind, BlockKind::Heading(6));
}

// =========================================================================
// SECTION 3: Concurrency & Thread-Safety Stress Tests
// =========================================================================

#[test]
fn test_concurrency_parallel_document_parsing_16_threads() {
    const THREAD_COUNT: usize = 16;
    const ITERATIONS_PER_THREAD: usize = 20;

    let barrier = Arc::new(std::sync::Barrier::new(THREAD_COUNT));
    let total_parsed = Arc::new(AtomicUsize::new(0));

    // Prepare 4 distinct document payloads
    let txt_payload_1 = Arc::new(
        "Chapter 1: The Beginning\n\nFirst paragraph of text.\n\nChapter 2: The Middle\n\nSecond paragraph."
            .to_string(),
    );
    let txt_payload_gbk = Arc::new(vec![
        0xC4, 0xE3, 0xBA, 0xC3, 0xCA, 0xC0, 0xBD, 0xE7, b'\n', b'\n', 0xB5, 0xDA, 0xD2, 0xBB,
        0xD5, 0xC2, b' ', b'O', b'K',
    ]);
    let md_payload_table = Arc::new(
        "# Table Document\n\n| A | B |\n|---|---|\n| 1 | 2 |\n| 3 | 4 |\n\n```rust\nlet x = 1;\n```"
            .to_string(),
    );
    let md_payload_lists = Arc::new(
        "# Lists\n\n- Level 0\n  - Level 1\n    - Level 2\n      - Level 3\n\n> Blockquote here."
            .to_string(),
    );

    let mut handles = Vec::with_capacity(THREAD_COUNT);

    for thread_idx in 0..THREAD_COUNT {
        let barrier_clone = barrier.clone();
        let total_parsed_clone = total_parsed.clone();
        let t1 = txt_payload_1.clone();
        let t_gbk = txt_payload_gbk.clone();
        let m_tbl = md_payload_table.clone();
        let m_lst = md_payload_lists.clone();

        handles.push(thread::spawn(move || {
            // Synchronize starting line
            barrier_clone.wait();

            for iter in 0..ITERATIONS_PER_THREAD {
                match (thread_idx + iter) % 4 {
                    0 => {
                        let doc = parse_txt_from_bytes(t1.as_bytes(), Some("Parallel TXT"))
                            .expect("Parallel TXT parse failed");
                        assert_eq!(doc.chapters.len(), 2);
                        assert_eq!(doc.total_blocks, 4);
                    }
                    1 => {
                        let doc = parse_txt_from_bytes(&t_gbk, Some("Parallel GBK"))
                            .expect("Parallel GBK parse failed");
                        assert!(!doc.chapters.is_empty());
                    }
                    2 => {
                        let doc = parse_markdown_from_str(&m_tbl, Some("Parallel Table"))
                            .expect("Parallel MD Table parse failed");
                        assert_eq!(doc.total_blocks, 3);
                    }
                    3 => {
                        let doc = parse_markdown_from_str(&m_lst, Some("Parallel Lists"))
                            .expect("Parallel MD Lists parse failed");
                        assert_eq!(doc.chapters.len(), 1);
                        assert!(doc.total_blocks >= 5);
                    }
                    _ => unreachable!(),
                }
                total_parsed_clone.fetch_add(1, Ordering::Relaxed);
            }
        }));
    }

    for h in handles {
        h.join().expect("Worker thread panicked during concurrency test");
    }

    assert_eq!(
        total_parsed.load(Ordering::SeqCst),
        THREAD_COUNT * ITERATIONS_PER_THREAD,
        "All 320 parallel document parses must complete successfully"
    );
}

#[test]
fn test_concurrency_trait_object_and_sha256_determinism() {
    // Test DocumentParser trait object across threads
    let txt_parser: Arc<dyn DocumentParser> = Arc::new(TxtParser);
    let md_parser: Arc<dyn DocumentParser> = Arc::new(MarkdownParser);

    const WORKERS: usize = 8;
    let mut handles = Vec::with_capacity(WORKERS);

    let doc_content = "Common document content used for SHA-256 fingerprint validation across threads.";
    let expected_sha256 = compute_sha256_hex(doc_content.as_bytes());

    for _ in 0..WORKERS {
        let p_txt = txt_parser.clone();
        let p_md = md_parser.clone();
        let expected_hash = expected_sha256.clone();

        handles.push(thread::spawn(move || {
            let doc_t = p_txt
                .parse_bytes(doc_content.as_bytes(), Some("Test"))
                .expect("Trait parse_bytes TXT must succeed");
            assert_eq!(doc_t.id, expected_hash, "TXT SHA-256 ID must be deterministic");

            let doc_m = p_md
                .parse_bytes(doc_content.as_bytes(), Some("Test"))
                .expect("Trait parse_bytes MD must succeed");
            assert_eq!(doc_m.id, expected_hash, "MD SHA-256 ID must be deterministic");
        }));
    }

    for h in handles {
        h.join().expect("Trait object worker thread panicked");
    }
}
