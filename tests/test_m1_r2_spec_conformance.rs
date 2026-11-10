//! Reviewer 2 Verification Suite for M1 Gate: Format Specifications & Edge Cases
use std::io::Cursor;
use docx_rs::{Docx, Paragraph, Run, Table, TableCell as DocxTableCell, TableRow, VMergeType};
use retranslate::document::docx::DocxParser;
use retranslate::document::epub::EpubParser;
use retranslate::document::error::DocumentError;
use retranslate::document::markdown::MarkdownParser;
use retranslate::document::model::*;
use retranslate::document::txt::{detect_and_decode, is_chapter_heading, segment_txt_paragraphs};

use zip::write::FileOptions;
use zip::ZipWriter;

fn create_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buffer = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut buffer);
    for (name, content) in files {
        zip.start_file(*name, FileOptions::<()>::default()).unwrap();
        std::io::Write::write_all(&mut zip, content).unwrap();
    }
    zip.finish().unwrap();
    buffer.into_inner()
}

// =========================================================================
// EPUB Verification Tests
// =========================================================================

#[test]
fn test_epub_drm_preflight_blocks_and_friendly_error() {
    // 1. META-INF/encryption.xml
    let zip_enc = create_zip(&[
        ("META-INF/container.xml", b"<container/>"),
        ("META-INF/encryption.xml", b"<encryption/>"),
    ]);
    let res_enc = EpubParser::parse_bytes(&zip_enc, None, None);
    match res_enc {
        Err(DocumentError::DrmProtected(msg)) => {
            let user_msg_en = DocumentError::DrmProtected(msg.clone()).user_facing_message("en-US");
            let user_msg_zh = DocumentError::DrmProtected(msg).user_facing_message("zh-CN");
            assert!(user_msg_en.contains("DRM"));
            assert!(user_msg_zh.contains("DRM"));
        }
        other => panic!("Expected DrmProtected, got {:?}", other),
    }

    // 2. META-INF/rights.xml
    let zip_rights = create_zip(&[
        ("META-INF/container.xml", b"<container/>"),
        ("META-INF/rights.xml", b"<rights/>"),
    ]);
    let res_rights = EpubParser::parse_bytes(&zip_rights, None, None);
    assert!(matches!(res_rights, Err(DocumentError::DrmProtected(_))));
}

#[test]
fn test_epub_corrupt_zip_missing_container() {
    let zip_bad = create_zip(&[("OEBPS/content.opf", b"<package/>")]);
    let res = EpubParser::parse_bytes(&zip_bad, None, None);
    match res {
        Err(DocumentError::CorruptZip(msg)) => {
            assert!(msg.contains("META-INF/container.xml"));
        }
        other => panic!("Expected CorruptZip, got {:?}", other),
    }
}

#[test]
fn test_epub_code_block_translatable_false_and_empty_spans() {
    let container = r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;

    let opf = r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" unique-identifier="BookId" version="2.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>Code Test</dc:title>
    <dc:language>en</dc:language>
    <dc:identifier id="BookId">test-code-123</dc:identifier>
  </metadata>
  <manifest>
    <item id="chap1" href="chap1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine><itemref idref="chap1"/></spine>
</package>"#;

    let chap1 = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>Code</title></head>
<body>
  <h1>Heading</h1>
  <pre>fn main() { println!("Hello"); }</pre>
</body>
</html>"#;

    let epub_bytes = create_zip(&[
        ("mimetype", b"application/epub+zip"),
        ("META-INF/container.xml", container.as_bytes()),
        ("OEBPS/content.opf", opf.as_bytes()),
        ("OEBPS/chap1.xhtml", chap1.as_bytes()),
    ]);

    let doc = EpubParser::parse_bytes(&epub_bytes, None, None).expect("Valid EPUB");
    assert_eq!(doc.chapters.len(), 1);
    let code_block = doc.chapters[0]
        .blocks
        .iter()
        .find(|b| matches!(b.kind, BlockKind::CodeBlock { .. }))
        .expect("CodeBlock must be found");

    assert!(!code_block.translatable, "EPUB CodeBlock translatable must be false");
    assert!(code_block.spans.is_empty(), "EPUB CodeBlock spans must be empty");
    if let BlockKind::CodeBlock { code, .. } = &code_block.kind {
        assert!(code.contains("fn main()"));
    }
}

// =========================================================================
// DOCX Verification Tests (Table Merged Cells & Structure)
// =========================================================================

#[test]
fn test_docx_real_merged_table_cells() {
    let docx = Docx::new()
        .add_paragraph(Paragraph::new().add_run(Run::new().add_text("Chapter 1")).style("Heading1"))
        .add_table(Table::new(vec![
            TableRow::new(vec![
                DocxTableCell::new()
                    .add_paragraph(Paragraph::new().add_run(Run::new().add_text("Cell 1x3")))
                    .grid_span(3),
            ]),
            TableRow::new(vec![
                DocxTableCell::new()
                    .add_paragraph(Paragraph::new().add_run(Run::new().add_text("Merged Row Start")))
                    .vertical_merge(VMergeType::Restart),
                DocxTableCell::new()
                    .add_paragraph(Paragraph::new().add_run(Run::new().add_text("Regular Cell"))),
            ]),
        ]));

    let mut buf = Cursor::new(Vec::new());
    docx.build().pack(&mut buf).unwrap();
    let bytes = buf.into_inner();

    let doc = DocxParser::parse_bytes(&bytes, Some("Docx Table Test"), None).expect("DOCX should parse");
    assert_eq!(doc.chapters.len(), 1);
    let table_block = doc.chapters[0]
        .blocks
        .iter()
        .find(|b| matches!(b.kind, BlockKind::Table { .. }))
        .expect("Table block should exist");

    if let BlockKind::Table { rows, .. } = &table_block.kind {
        assert_eq!(rows.len(), 2, "Table should have 2 rows");
        // Row 0, Cell 0: col_span must be 3
        assert_eq!(rows[0][0].col_span, 3, "gridSpan 3 must extract col_span = 3");
        assert_eq!(rows[0][0].row_span, 1);

        // Row 1, Cell 0: row_span must be 2 (restart)
        assert_eq!(rows[1][0].col_span, 1);
        assert_eq!(rows[1][0].row_span, 2, "vMerge restart must extract row_span = 2");
    } else {
        panic!("Expected Table kind");
    }
}

#[test]
fn test_docx_chapter_break_page_break_and_headings() {
    let docx = Docx::new()
        .add_paragraph(Paragraph::new().add_run(Run::new().add_text("Chapter 1 Intro")).style("Heading1"))
        .add_paragraph(Paragraph::new().add_run(Run::new().add_text("Content in chapter 1.")))
        .add_paragraph(Paragraph::new().add_run(Run::new().add_text("Chapter 2 Content")).style("Heading1"))
        .add_paragraph(Paragraph::new().add_run(Run::new().add_text("Content in chapter 2.")));

    let mut buf = Cursor::new(Vec::new());
    docx.build().pack(&mut buf).unwrap();
    let bytes = buf.into_inner();

    let doc = DocxParser::parse_bytes(&bytes, Some("Docx Chapters"), None).unwrap();
    assert_eq!(doc.chapters.len(), 2, "Heading1 should partition into 2 chapters");
    assert_eq!(doc.chapters[0].title, "Chapter 1 Intro");
    assert_eq!(doc.chapters[1].title, "Chapter 2 Content");
}

// =========================================================================
// Markdown Verification Tests
// =========================================================================

#[test]
fn test_markdown_code_block_invariants_fenced_and_indented() {
    let md = r#"# Section

```python
def add(a, b):
    return a + b
```

    indented_code_line_1
    indented_code_line_2

Paragraph after code.
"#;

    let doc = MarkdownParser::parse_str(md, Some("MD Code"), None, None).unwrap();
    let code_blocks: Vec<&Block> = doc
        .chapters[0]
        .blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::CodeBlock { .. }))
        .collect();

    assert_eq!(code_blocks.len(), 2, "Both fenced and indented code blocks must be captured");

    for cb in code_blocks {
        assert!(!cb.translatable, "Critical Invariant: Code block must strictly have translatable=false");
        assert!(cb.spans.is_empty(), "Critical Invariant: Code block must strictly have empty spans");
    }
}

#[test]
fn test_markdown_nested_lists_depth_and_counters() {
    let md = r#"
- Level 0 Item 1
- Level 0 Item 2
  - Level 1 Subitem A
  - Level 1 Subitem B
    1. Level 2 Ordered 1
    2. Level 2 Ordered 2
"#;

    let doc = MarkdownParser::parse_str(md, Some("Lists"), None, None).unwrap();
    let list_blocks: Vec<&Block> = doc
        .chapters[0]
        .blocks
        .iter()
        .filter(|b| matches!(b.kind, BlockKind::ListItem { .. }))
        .collect();

    assert_eq!(list_blocks.len(), 6);
    if let BlockKind::ListItem { depth, order } = list_blocks[0].kind {
        assert_eq!(depth, 0);
        assert_eq!(order, None);
    }
    if let BlockKind::ListItem { depth, order } = list_blocks[2].kind {
        assert_eq!(depth, 1);
        assert_eq!(order, None);
    }
    if let BlockKind::ListItem { depth, order } = list_blocks[4].kind {
        assert_eq!(depth, 2);
        assert_eq!(order, Some(1));
    }
}

#[test]
fn test_markdown_gfm_tables_alignments() {
    let md = r#"
| Col 1 Left | Col 2 Center | Col 3 Right | Col 4 None |
| :--- | :---: | ---: | --- |
| L1 | C1 | R1 | N1 |
| L2 | C2 | R2 | N2 |
"#;

    let doc = MarkdownParser::parse_str(md, Some("Table Test"), None, None).unwrap();
    let table = doc
        .chapters[0]
        .blocks
        .iter()
        .find(|b| matches!(b.kind, BlockKind::Table { .. }))
        .expect("Table should exist");

    if let BlockKind::Table { headers, rows } = &table.kind {
        assert_eq!(headers.len(), 4);
        assert_eq!(headers[0].align, TableAlignment::Left);
        assert_eq!(headers[1].align, TableAlignment::Center);
        assert_eq!(headers[2].align, TableAlignment::Right);
        assert_eq!(headers[3].align, TableAlignment::None);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].len(), 4);
    } else {
        panic!("Expected Table");
    }
}

// =========================================================================
// TXT Verification Tests (Encoding Cascade, Soft-Wrap & Chapter Heuristics)
// =========================================================================

#[test]
fn test_txt_bom_sniffing_all_variants() {
    // UTF-8 BOM
    let utf8 = "Hello 世界";
    let mut utf8_bom = vec![0xEF, 0xBB, 0xBF];
    utf8_bom.extend_from_slice(utf8.as_bytes());
    assert_eq!(detect_and_decode(&utf8_bom).unwrap(), utf8);

    // UTF-16LE BOM
    // "AB" -> 0xFF, 0xFE, 0x41, 0x00, 0x42, 0x00
    let u16le = vec![0xFF, 0xFE, 0x41, 0x00, 0x42, 0x00];
    assert_eq!(detect_and_decode(&u16le).unwrap(), "AB");

    // UTF-16BE BOM
    // "AB" -> 0xFE, 0xFF, 0x00, 0x41, 0x00, 0x42
    let u16be = vec![0xFE, 0xFF, 0x00, 0x41, 0x00, 0x42];
    assert_eq!(detect_and_decode(&u16be).unwrap(), "AB");
}

#[test]
fn test_txt_gb18030_cascade() {
    // GBK bytes for "中国古代文学经典"
    // 中 (0xD6, 0xD0), 国 (0xB9, 0xFA), 古 (0xB9, 0xc5), 代 (0xb4, 0xfa)
    let gbk = vec![0xD6, 0xD0, 0xB9, 0xFA, 0xB9, 0xC5, 0xB4, 0xFA];
    let decoded = detect_and_decode(&gbk).unwrap();
    assert_eq!(decoded, "中国古代");
}

#[test]
fn test_txt_soft_wrap_cjk_vs_latin_whitespace() {
    // 1. Pure Latin: MUST insert space
    let latin = "First line of text\nsecond line continues\nthird line ends.";
    let res_latin = segment_txt_paragraphs(latin);
    assert_eq!(res_latin.len(), 1);
    assert_eq!(res_latin[0], "First line of text second line continues third line ends.");

    // 2. Pure CJK: MUST NOT insert space
    let cjk = "这是第一行内容，\n接着是第二行内容，\n最后是第三行。";
    let res_cjk = segment_txt_paragraphs(cjk);
    assert_eq!(res_cjk.len(), 1);
    assert_eq!(res_cjk[0], "这是第一行内容，接着是第二行内容，最后是第三行。");
    assert!(!res_cjk[0].contains(' '));

    // 3. Mixed Latin and CJK boundary:
    // "Hello\n世界" -> boundary has CJK on one side -> NO space
    let mixed = "Hello\n世界";
    let res_mixed = segment_txt_paragraphs(mixed);
    assert_eq!(res_mixed.len(), 1);
    assert_eq!(res_mixed[0], "Hello世界");
}

#[test]
fn test_txt_monolithic_line_chunking() {
    // A single line of 2000 Chinese characters with punctuation
    let mut monolithic = String::new();
    for i in 0..10 {
        monolithic.push_str(&format!("这是第{}段非常长且详细的描述内容，用于测试超过阈值时的切分逻辑是否正确执行。", i));
    }
    // Repeat to exceed 1500 chars
    while monolithic.chars().count() < 1600 {
        monolithic.push_str("持续扩展的内容句子。");
    }

    let chunks = segment_txt_paragraphs(&monolithic);
    assert!(chunks.len() > 1, "Monolithic text > 1500 chars must be chunked into multiple segments");
    for chunk in &chunks {
        assert!(!chunk.is_empty());
    }
}

#[test]
fn test_txt_chapter_heuristics() {
    // Positive matches (Chinese)
    assert!(is_chapter_heading("第一章 初始"));
    assert!(is_chapter_heading("第十二卷 风云突变"));
    assert!(is_chapter_heading("第一百二十五节 巅峰"));
    assert!(is_chapter_heading("第99章 完结篇"));

    // Positive matches (Western)
    assert!(is_chapter_heading("Chapter 1: The Beginning"));
    assert!(is_chapter_heading("CHAPTER IV - The Encounter"));
    assert!(is_chapter_heading("Act 2 Scene 1"));
    assert!(is_chapter_heading("Book III"));

    // Negative matches
    assert!(!is_chapter_heading("第一本书")); // '书' is not a chapter heading unit
    assert!(!is_chapter_heading("This chapter was good."));
    assert!(!is_chapter_heading("Chaptering your files"));
    assert!(!is_chapter_heading(""));
}
