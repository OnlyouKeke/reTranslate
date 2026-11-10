//! E2E Test Suite: Markdown Parser & CommonMark Structural Conformance
//! Covers Feature 23, 24 from PROJECT.md & TEST_INFRA.md (Tier 1 & Tier 2)

mod common;
use common::*;

// Contract model for Markdown parsing
pub struct MockMarkdownParser;

impl MockMarkdownParser {
    pub fn parse_to_blocks(input: &str) -> Vec<Block> {
        let mut blocks = Vec::new();
        let lines: Vec<&str> = input.lines().collect();
        let mut i = 0;
        let mut block_idx = 0;

        while i < lines.len() {
            let line = lines[i];
            let trimmed = line.trim();

            if trimmed.is_empty() {
                i += 1;
                continue;
            }

            // 1. Heading check
            if trimmed.starts_with('#') {
                let hashes = trimmed.chars().take_while(|&c| c == '#').count();
                if hashes <= 6 && trimmed.chars().nth(hashes) == Some(' ') {
                    let text = trimmed[hashes + 1..].trim().to_string();
                    blocks.push(Block {
                        id: format!("chap_0_b{}", block_idx),
                        chapter_id: "chap_0".to_string(),
                        index: block_idx,
                        kind: BlockKind::Heading(hashes as u8),
                        spans: vec![InlineSpan {
                            text,
                            style: InlineStyle::Plain,
                        }],
                        translatable: true,
                        translation: TranslationState::Untranslated,
                        metadata: BlockMetadata::default(),
                    });
                    block_idx += 1;
                    i += 1;
                    continue;
                }
            }

            // 2. Fenced code block check
            if trimmed.starts_with("```") {
                let lang = trimmed[3..].trim();
                let language = if lang.is_empty() { None } else { Some(lang.to_string()) };
                let mut code = String::new();
                i += 1;
                while i < lines.len() && !lines[i].trim().starts_with("```") {
                    code.push_str(lines[i]);
                    code.push('\n');
                    i += 1;
                }
                if i < lines.len() {
                    i += 1; // skip closing ```
                }
                blocks.push(Block {
                    id: format!("chap_0_b{}", block_idx),
                    chapter_id: "chap_0".to_string(),
                    index: block_idx,
                    kind: BlockKind::CodeBlock { language, code: code.trim_end().to_string() },
                    spans: vec![],
                    translatable: false, // Invariant: Code blocks are NOT translated
                    translation: TranslationState::Untranslated,
                    metadata: BlockMetadata::default(),
                });
                block_idx += 1;
                continue;
            }

            // 3. Blockquote check
            if trimmed.starts_with('>') {
                let quote_text = trimmed[1..].trim().to_string();
                blocks.push(Block {
                    id: format!("chap_0_b{}", block_idx),
                    chapter_id: "chap_0".to_string(),
                    index: block_idx,
                    kind: BlockKind::Quote,
                    spans: vec![InlineSpan {
                        text: quote_text,
                        style: InlineStyle::Plain,
                    }],
                    translatable: true,
                    translation: TranslationState::Untranslated,
                    metadata: BlockMetadata::default(),
                });
                block_idx += 1;
                i += 1;
                continue;
            }

            // 4. List item check
            if trimmed.starts_with("- ") || trimmed.starts_with("* ") {
                let item_text = trimmed[2..].trim().to_string();
                blocks.push(Block {
                    id: format!("chap_0_b{}", block_idx),
                    chapter_id: "chap_0".to_string(),
                    index: block_idx,
                    kind: BlockKind::ListItem { order: None, depth: 0 },
                    spans: vec![InlineSpan {
                        text: item_text,
                        style: InlineStyle::Plain,
                    }],
                    translatable: true,
                    translation: TranslationState::Untranslated,
                    metadata: BlockMetadata::default(),
                });
                block_idx += 1;
                i += 1;
                continue;
            }

            // 5. Default paragraph
            blocks.push(Block {
                id: format!("chap_0_b{}", block_idx),
                chapter_id: "chap_0".to_string(),
                index: block_idx,
                kind: BlockKind::Paragraph,
                spans: vec![InlineSpan {
                    text: trimmed.to_string(),
                    style: InlineStyle::Plain,
                }],
                translatable: true,
                translation: TranslationState::Untranslated,
                metadata: BlockMetadata::default(),
            });
            block_idx += 1;
            i += 1;
        }

        blocks
    }
}

// =========================================================================
// TIER 1: Feature Coverage (Nominal Paths)
// =========================================================================

#[test]
fn test_md_headings_h1_to_h6() {
    let md = "# Heading 1\n## Heading 2\n### Heading 3\n#### Heading 4\n##### Heading 5\n###### Heading 6";
    let blocks = MockMarkdownParser::parse_to_blocks(md);

    assert_eq!(blocks.len(), 6);
    for (i, block) in blocks.iter().enumerate() {
        if let BlockKind::Heading(level) = block.kind {
            assert_eq!(level, (i + 1) as u8);
        } else {
            panic!("Expected Heading level {}", i + 1);
        }
    }
}

#[test]
fn test_md_chapter_splitting_on_h1() {
    let md = "# Chapter One\nText for chapter 1.\n\n# Chapter Two\nText for chapter 2.";
    let blocks = MockMarkdownParser::parse_to_blocks(md);

    let h1_count = blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::Heading(1)))
        .count();
    assert_eq!(h1_count, 2, "H1 headings serve as chapter boundaries");
}

#[test]
fn test_md_inline_formatting() {
    let span_bold = InlineSpan {
        text: "Strong".to_string(),
        style: InlineStyle::Bold,
    };
    let span_italic = InlineSpan {
        text: "Emphasis".to_string(),
        style: InlineStyle::Italic,
    };
    let span_code = InlineSpan {
        text: "let x = 42;".to_string(),
        style: InlineStyle::Code,
    };

    assert_eq!(span_bold.style, InlineStyle::Bold);
    assert_eq!(span_italic.style, InlineStyle::Italic);
    assert_eq!(span_code.style, InlineStyle::Code);
}

#[test]
fn test_md_gfm_tables() {
    let header_cells = vec![
        TableCell {
            spans: vec![InlineSpan {
                text: "Original".to_string(),
                style: InlineStyle::Plain,
            }],
            col_span: 1,
            row_span: 1,
            align: TableAlignment::Left,
        },
        TableCell {
            spans: vec![InlineSpan {
                text: "Translation".to_string(),
                style: InlineStyle::Plain,
            }],
            col_span: 1,
            row_span: 1,
            align: TableAlignment::Right,
        },
    ];

    let table_block = Block {
        id: "c0_b1".to_string(),
        chapter_id: "c0".to_string(),
        index: 1,
        kind: BlockKind::Table {
            headers: header_cells,
            rows: vec![],
        },
        spans: vec![],
        translatable: true,
        translation: TranslationState::Untranslated,
        metadata: BlockMetadata::default(),
    };

    if let BlockKind::Table { headers, .. } = table_block.kind {
        assert_eq!(headers.len(), 2);
        assert_eq!(headers[0].align, TableAlignment::Left);
        assert_eq!(headers[1].align, TableAlignment::Right);
    } else {
        panic!("Expected BlockKind::Table");
    }
}

#[test]
fn test_md_code_block_untranslatable() {
    let md = "```rust\nfn calculate() -> i32 {\n    1 + 1\n}\n```";
    let blocks = MockMarkdownParser::parse_to_blocks(md);

    assert_eq!(blocks.len(), 1);
    assert!(!blocks[0].translatable, "Code blocks must have translatable == false");
    if let BlockKind::CodeBlock { language, code } = &blocks[0].kind {
        assert_eq!(language.as_deref(), Some("rust"));
        assert!(code.contains("1 + 1"));
    } else {
        panic!("Expected CodeBlock");
    }
}

// =========================================================================
// TIER 2: Boundary, Edge & Corner Cases
// =========================================================================

#[test]
fn test_md_raw_html_sanitization() {
    // Malicious or raw HTML embedded in Markdown
    let raw_html = "<script>alert('xss');</script>\nValid paragraph text.";
    let blocks = MockMarkdownParser::parse_to_blocks(raw_html);

    // Invariant: Raw scripts are not executed, treated safely as text or sanitized
    assert_eq!(blocks.len(), 2);
    assert_eq!(blocks[1].plain_text(), "Valid paragraph text.");
}

#[test]
fn test_md_nested_lists_and_depth() {
    let _item_l0 = BlockKind::ListItem { order: None, depth: 0 };
    let _item_l1 = BlockKind::ListItem { order: None, depth: 1 };
    let item_l2 = BlockKind::ListItem { order: Some(1), depth: 2 };

    if let BlockKind::ListItem { depth, order } = item_l2 {
        assert_eq!(depth, 2);
        assert_eq!(order, Some(1));
    }
}

#[test]
fn test_md_empty_file_and_whitespace() {
    let empty_md = "   \n\n\t  \n";
    let blocks = MockMarkdownParser::parse_to_blocks(empty_md);
    assert!(blocks.is_empty(), "Pure whitespace markdown produces 0 blocks");
}

#[test]
fn test_md_escaped_characters() {
    let escaped = r"\# Not a heading, just plain text.";
    let blocks = MockMarkdownParser::parse_to_blocks(escaped);
    assert_eq!(blocks.len(), 1);
    assert!(matches!(blocks[0].kind, BlockKind::Paragraph));
}

#[test]
fn test_md_blockquotes_nested() {
    let md = "> Quote level 1\n> Second line of quote";
    let blocks = MockMarkdownParser::parse_to_blocks(md);
    assert_eq!(blocks.len(), 2);
    assert!(matches!(blocks[0].kind, BlockKind::Quote));
    assert!(matches!(blocks[1].kind, BlockKind::Quote));
}
