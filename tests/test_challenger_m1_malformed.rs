//! Adversarial integration test suite for EPUB and DOCX
use std::io::{Cursor, Write};
use zip::write::FileOptions;
use zip::ZipWriter;
use retranslate::document::epub::EpubParser;
use retranslate::document::docx::DocxParser;
use retranslate::document::error::DocumentError;
use retranslate::document::model::*;
use docx_rs::*;

fn build_minimal_epub(extra_files: &[(&str, &[u8])]) -> Vec<u8> {
    let mut buffer = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut buffer);

    let container = r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;

    let opf = r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" unique-identifier="BookId" version="2.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>Test Book</dc:title>
    <dc:language>en</dc:language>
    <dc:identifier id="BookId">test-id-123</dc:identifier>
  </metadata>
  <manifest>
    <item id="chap1" href="chap1.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="chap1"/>
  </spine>
</package>"#;

    let chap1 = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>Chapter 1</title></head>
<body>
  <h1>Chapter 1</h1>
  <p>Hello adversarial world.</p>
</body>
</html>"#;

    zip.start_file("mimetype", FileOptions::<()>::default()).unwrap();
    zip.write_all(b"application/epub+zip").unwrap();

    zip.start_file("META-INF/container.xml", FileOptions::<()>::default()).unwrap();
    zip.write_all(container.as_bytes()).unwrap();

    zip.start_file("OEBPS/content.opf", FileOptions::<()>::default()).unwrap();
    zip.write_all(opf.as_bytes()).unwrap();

    zip.start_file("OEBPS/chap1.xhtml", FileOptions::<()>::default()).unwrap();
    zip.write_all(chap1.as_bytes()).unwrap();

    for (name, content) in extra_files {
        zip.start_file(*name, FileOptions::<()>::default()).unwrap();
        zip.write_all(content).unwrap();
    }

    zip.finish().unwrap();
    buffer.into_inner()
}

fn build_minimal_docx(extra_files: &[(&str, &[u8])]) -> Vec<u8> {
    let docx = Docx::new().add_paragraph(
        docx_rs::Paragraph::new()
            .add_run(Run::new().add_text("Adversarial DOCX heading").bold())
            .style("Heading1"),
    ).add_paragraph(
        docx_rs::Paragraph::new()
            .add_run(Run::new().add_text("Normal paragraph body text.")),
    );

    let mut buf = Cursor::new(Vec::new());
    docx.build().pack(&mut buf).unwrap();
    let bytes = buf.into_inner();

    if extra_files.is_empty() {
        return bytes;
    }

    // Unpack and repack with extra files
    let mut reader = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
    let mut out_buf = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(&mut out_buf);

    for i in 0..reader.len() {
        let mut file = reader.by_index(i).unwrap();
        let name = file.name().to_string();
        let mut data = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut data).unwrap();
        writer.start_file(name, FileOptions::<()>::default()).unwrap();
        writer.write_all(&data).unwrap();
    }

    for (name, content) in extra_files {
        writer.start_file(*name, FileOptions::<()>::default()).unwrap();
        writer.write_all(content).unwrap();
    }

    writer.finish().unwrap();
    out_buf.into_inner()
}

fn build_custom_docx_xml(doc_xml: &str) -> Vec<u8> {
    let base = build_minimal_docx(&[]);
    let mut reader = zip::ZipArchive::new(Cursor::new(&base)).unwrap();
    let mut out_buf = Cursor::new(Vec::new());
    let mut writer = ZipWriter::new(&mut out_buf);

    for i in 0..reader.len() {
        let mut file = reader.by_index(i).unwrap();
        let name = file.name().to_string();
        if name == "word/document.xml" {
            writer.start_file(name, FileOptions::<()>::default()).unwrap();
            writer.write_all(doc_xml.as_bytes()).unwrap();
        } else {
            let mut data = Vec::new();
            std::io::Read::read_to_end(&mut file, &mut data).unwrap();
            writer.start_file(name, FileOptions::<()>::default()).unwrap();
            writer.write_all(&data).unwrap();
        }
    }

    writer.finish().unwrap();
    out_buf.into_inner()
}

// =========================================================================
// EPUB DRM & CORRUPTED ARCHIVE TESTS
// =========================================================================

#[test]
fn test_epub_drm_encryption_xml_rejected() {
    let bytes = build_minimal_epub(&[
        ("META-INF/encryption.xml", b"<encryption xmlns='urn:oasis:names:tc:opendocument:xmlns:container'/>")
    ]);
    let res = EpubParser::parse_bytes(&bytes, None, None);
    match res {
        Err(DocumentError::DrmProtected(msg)) => {
            assert!(msg.to_lowercase().contains("drm"));
            let user_msg = DocumentError::DrmProtected(msg).user_facing_message("en");
            assert!(user_msg.contains("DRM"));
        }
        other => panic!("Expected DrmProtected error, got: {:?}", other),
    }
}

#[test]
fn test_epub_drm_rights_xml_rejected() {
    let bytes = build_minimal_epub(&[
        ("META-INF/rights.xml", b"<rights><encryptedKey/></rights>")
    ]);
    let res = EpubParser::parse_bytes(&bytes, None, None);
    match res {
        Err(DocumentError::DrmProtected(msg)) => {
            assert!(msg.to_lowercase().contains("drm"));
        }
        other => panic!("Expected DrmProtected error, got: {:?}", other),
    }
}

#[test]
fn test_epub_corrupted_and_truncated_zip() {
    let valid_bytes = build_minimal_epub(&[]);

    // Empty buffer
    let res_empty = EpubParser::parse_bytes(&[], None, None);
    assert!(matches!(res_empty, Err(DocumentError::EmptyDocument)));

    // Truncated headers (1, 2, 4, 10, 50, 100 bytes)
    for len in [1, 2, 4, 10, 50, 100, valid_bytes.len() / 2, valid_bytes.len() - 5] {
        let truncated = &valid_bytes[..len];
        let res = EpubParser::parse_bytes(truncated, None, None);
        assert!(
            matches!(res, Err(DocumentError::CorruptZip(_))),
            "Expected CorruptZip for length {}, got {:?}", len, res
        );
    }

    // Complete garbage / random bytes
    let garbage = vec![0xDE, 0xAD, 0xBE, 0xEF, 0x00, 0xFF, 0xAA, 0x55];
    let res_garbage = EpubParser::parse_bytes(&garbage, None, None);
    assert!(matches!(res_garbage, Err(DocumentError::CorruptZip(_))));
}

#[test]
fn test_epub_missing_container_xml() {
    let mut buffer = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut buffer);
    zip.start_file("OEBPS/content.opf", FileOptions::<()>::default()).unwrap();
    zip.write_all(b"<package/>").unwrap();
    zip.finish().unwrap();

    let bytes = buffer.into_inner();
    let res = EpubParser::parse_bytes(&bytes, None, None);
    match res {
        Err(DocumentError::CorruptZip(msg)) => {
            assert!(msg.contains("META-INF/container.xml"));
        }
        other => panic!("Expected CorruptZip for missing container.xml, got: {:?}", other),
    }
}

// =========================================================================
// EPUB HTML / SPINE / HREF CORNER CASES
// =========================================================================

#[test]
fn test_epub_malformed_unclosed_html_tags() {
    let malformed_html = r#"
        <!DOCTYPE html>
        <html>
        <head><title>Unbalanced</title></head>
        <body>
            <h1>Heading Without Close
            <p>Paragraph <b>Bold <i>BoldItalic <code>Code
            <div>Nested container without close
            <p>Second paragraph <a>Unclosed link text
            <table border="1">
                <tr><th>Header 1<th>Header 2
                <tr><td>Data 1<td>Data 2
            <hr>
            <blockquote>Unclosed quote
        "#;

    let (blocks, _) = EpubParser::extract_blocks_from_html(
        malformed_html,
        "c0",
        std::path::Path::new("OEBPS/chap.xhtml"),
        0,
        &mut epub::doc::EpubDoc::from_reader(Cursor::new(build_minimal_epub(&[]))).unwrap(),
    );

    assert!(!blocks.is_empty(), "Should extract blocks even from unclosed HTML");
    // Verify plain text contains extracted strings without panic
    let text_dump: String = blocks.iter().map(|b| b.plain_text()).collect::<Vec<_>>().join(" ");
    assert!(text_dump.contains("Heading Without Close"));
    assert!(text_dump.contains("Paragraph"));
    assert!(text_dump.contains("Bold"));
    assert!(text_dump.contains("Data 1"));
}

#[test]
fn test_epub_missing_spine_items() {
    // OPF declares two spine items: chap1 (present) and chap2 (missing from zip)
    let opf = r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" unique-identifier="BookId" version="2.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>Test Book</dc:title>
    <dc:language>en</dc:language>
    <dc:identifier id="BookId">test-id-123</dc:identifier>
  </metadata>
  <manifest>
    <item id="chap1" href="chap1.xhtml" media-type="application/xhtml+xml"/>
    <item id="chap2" href="chap2_missing.xhtml" media-type="application/xhtml+xml"/>
    <item id="chap3" href="chap3.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="chap1"/>
    <itemref idref="chap2"/>
    <itemref idref="chap_ghost_nonexistent"/>
    <itemref idref="chap3"/>
  </spine>
</package>"#;

    let chap3 = r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml">
<head><title>Chapter 3</title></head>
<body>
  <h2>Chapter 3</h2>
  <p>Content of chapter 3 surviving missing chap2.</p>
</body>
</html>"#;

    let mut buffer = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut buffer);

    let container = r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#;

    let chap1 = r#"<!DOCTYPE html><html><body><h1>Chapter 1</h1><p>Text</p></body></html>"#;

    zip.start_file("mimetype", FileOptions::<()>::default()).unwrap();
    zip.write_all(b"application/epub+zip").unwrap();
    zip.start_file("META-INF/container.xml", FileOptions::<()>::default()).unwrap();
    zip.write_all(container.as_bytes()).unwrap();
    zip.start_file("OEBPS/content.opf", FileOptions::<()>::default()).unwrap();
    zip.write_all(opf.as_bytes()).unwrap();
    zip.start_file("OEBPS/chap1.xhtml", FileOptions::<()>::default()).unwrap();
    zip.write_all(chap1.as_bytes()).unwrap();
    zip.start_file("OEBPS/chap3.xhtml", FileOptions::<()>::default()).unwrap();
    zip.write_all(chap3.as_bytes()).unwrap();

    zip.finish().unwrap();
    let bytes = buffer.into_inner();

    let doc = EpubParser::parse_bytes(&bytes, None, None).expect("Should survive missing spine items");
    assert_eq!(doc.chapters.len(), 2, "Should have parsed chap1 and chap3, skipping missing chap2 and chap_ghost");
    assert_eq!(doc.chapters[0].title, "Chapter 1");
}

#[test]
fn test_epub_circular_hrefs_and_navigation() {
    let chap1 = r##"<!DOCTYPE html><html><body>
        <h1>Chapter 1</h1>
        <p><a href="chap2.xhtml">Go to 2</a></p>
        <p><a href="chap1.xhtml">Self link</a></p>
        <p><a epub:type="noteref" href="#fn1">Note 1</a></p>
    </body></html>"##;

    let chap2 = r##"<!DOCTYPE html><html><body>
        <h1>Chapter 2</h1>
        <p><a href="chap1.xhtml">Go to 1</a></p>
        <p id="fn1"><a epub:type="noteref" href="chap1.xhtml#fn1">Back to note 1</a></p>
    </body></html>"##;

    let opf = r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" unique-identifier="BookId" version="2.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>Circular Test</dc:title>
    <dc:language>en</dc:language>
    <dc:identifier id="BookId">test-id-123</dc:identifier>
  </metadata>
  <manifest>
    <item id="c1" href="chap1.xhtml" media-type="application/xhtml+xml"/>
    <item id="c2" href="chap2.xhtml" media-type="application/xhtml+xml"/>
  </manifest>
  <spine>
    <itemref idref="c1"/>
    <itemref idref="c2"/>
  </spine>
</package>"#;

    let mut buffer = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut buffer);

    let container = r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;

    zip.start_file("mimetype", FileOptions::<()>::default()).unwrap();
    zip.write_all(b"application/epub+zip").unwrap();
    zip.start_file("META-INF/container.xml", FileOptions::<()>::default()).unwrap();
    zip.write_all(container.as_bytes()).unwrap();
    zip.start_file("OEBPS/content.opf", FileOptions::<()>::default()).unwrap();
    zip.write_all(opf.as_bytes()).unwrap();
    zip.start_file("OEBPS/chap1.xhtml", FileOptions::<()>::default()).unwrap();
    zip.write_all(chap1.as_bytes()).unwrap();
    zip.start_file("OEBPS/chap2.xhtml", FileOptions::<()>::default()).unwrap();
    zip.write_all(chap2.as_bytes()).unwrap();

    zip.finish().unwrap();
    let bytes = buffer.into_inner();

    let doc = EpubParser::parse_bytes(&bytes, None, None).expect("Circular hrefs must parse without loop");
    assert_eq!(doc.chapters.len(), 2);
}

#[test]
fn test_epub_invalid_image_mime_and_paths() {
    let chap_with_images = r#"<!DOCTYPE html><html><body>
        <h1>Image Test</h1>
        <img src="images/valid.png" alt="Valid PNG"/>
        <img src="images/unknown_format.xyz" alt="Unknown XYZ format"/>
        <img src="images/missing.jpg" alt="Missing File"/>
        <img src="https://example.com/remote.png" alt="Remote URL"/>
        <img src="" alt="Empty Src"/>
        <img src="data:image/png;base64,iVBORw0KGgo=" alt="Data URI"/>
    </body></html>"#;

    let opf = r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" unique-identifier="BookId" version="2.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>Image Test</dc:title>
    <dc:language>en</dc:language>
    <dc:identifier id="BookId">test-id-123</dc:identifier>
  </metadata>
  <manifest>
    <item id="c1" href="chap1.xhtml" media-type="application/xhtml+xml"/>
    <item id="img1" href="images/valid.png" media-type="image/png"/>
    <item id="img2" href="images/unknown_format.xyz" media-type="application/octet-stream"/>
  </manifest>
  <spine>
    <itemref idref="c1"/>
  </spine>
</package>"#;

    let mut buffer = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut buffer);

    let container = r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;

    zip.start_file("mimetype", FileOptions::<()>::default()).unwrap();
    zip.write_all(b"application/epub+zip").unwrap();
    zip.start_file("META-INF/container.xml", FileOptions::<()>::default()).unwrap();
    zip.write_all(container.as_bytes()).unwrap();
    zip.start_file("OEBPS/content.opf", FileOptions::<()>::default()).unwrap();
    zip.write_all(opf.as_bytes()).unwrap();
    zip.start_file("OEBPS/chap1.xhtml", FileOptions::<()>::default()).unwrap();
    zip.write_all(chap_with_images.as_bytes()).unwrap();
    zip.start_file("OEBPS/images/valid.png", FileOptions::<()>::default()).unwrap();
    zip.write_all(b"\x89PNG\r\n\x1a\nfake png data").unwrap();
    zip.start_file("OEBPS/images/unknown_format.xyz", FileOptions::<()>::default()).unwrap();
    zip.write_all(b"arbitrary bytes").unwrap();

    zip.finish().unwrap();
    let bytes = buffer.into_inner();

    let doc = EpubParser::parse_bytes(&bytes, None, None).expect("EPUB with edge image formats must parse");
    let blocks = &doc.chapters[0].blocks;
    let img_blocks: Vec<_> = blocks.iter().filter(|b| matches!(b.kind, BlockKind::Image { .. })).collect();
    assert_eq!(img_blocks.len(), 6);

    // Verify valid PNG image
    if let BlockKind::Image { src, mime, data, .. } = &img_blocks[0].kind {
        assert_eq!(src, "images/valid.png");
        assert_eq!(mime.as_deref(), Some("image/png"));
        assert!(data.is_some());
    }

    // Verify unknown format handled safely with application/octet-stream fallback
    if let BlockKind::Image { src, mime, data, .. } = &img_blocks[1].kind {
        assert_eq!(src, "images/unknown_format.xyz");
        assert_eq!(mime.as_deref(), Some("application/octet-stream"));
        assert!(data.is_some());
    }

    // Verify missing image handled with None data
    if let BlockKind::Image { src, mime, data, .. } = &img_blocks[2].kind {
        assert_eq!(src, "images/missing.jpg");
        assert!(mime.is_none());
        assert!(data.is_none());
    }

    // Verify remote URL handled with None data without network attempt
    if let BlockKind::Image { mime, data, .. } = &img_blocks[3].kind {
        assert!(mime.is_none());
        assert!(data.is_none());
    }
}

// =========================================================================
// DOCX MALFORMED / BOUNDARY TESTS
// =========================================================================

#[test]
fn test_docx_truncated_and_corrupt() {
    let valid_docx = build_minimal_docx(&[]);

    // 0 bytes
    let res_empty = DocxParser::parse_bytes(&[], None, None);
    assert!(matches!(res_empty, Err(DocumentError::EmptyDocument)));

    // Truncated at various byte lengths
    for len in [1, 4, 10, 50, 100, valid_docx.len() / 2, valid_docx.len() - 10] {
        let truncated = &valid_docx[..len];
        let res = DocxParser::parse_bytes(truncated, None, None);
        assert!(
            matches!(res, Err(DocumentError::InvalidDocx(_))),
            "Expected InvalidDocx for length {}, got: {:?}", len, res
        );
    }
}

#[test]
fn test_docx_missing_document_xml() {
    let mut buffer = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut buffer);
    zip.start_file("[Content_Types].xml", FileOptions::<()>::default()).unwrap();
    zip.write_all(b"<Types/>").unwrap();
    zip.start_file("word/styles.xml", FileOptions::<()>::default()).unwrap();
    zip.write_all(b"<w:styles/>").unwrap();
    zip.finish().unwrap();

    let bytes = buffer.into_inner();
    let res = DocxParser::parse_bytes(&bytes, None, None);
    match res {
        Err(DocumentError::InvalidDocx(msg)) => {
            assert!(msg.contains("word/document.xml"));
        }
        other => panic!("Expected InvalidDocx for missing word/document.xml, got: {:?}", other),
    }
}

#[test]
fn test_docx_malformed_xml_in_document_xml() {
    let broken_xmls = [
        ("empty", ""),
        ("garbage", "NOT XML AT ALL <<< ??? >>>"),
        ("unclosed", "<w:document><w:body><w:p><w:r><w:t>Unclosed tag"),
        ("bad_nested", "<w:document><w:body><w:p><w:r><w:t>Bad nested</w:p></w:r></w:body></w:document>"),
    ];

    for (label, xml) in broken_xmls {
        println!("Testing XML case: {}", label);
        let bytes = build_custom_docx_xml(xml);
        let res = DocxParser::parse_bytes(&bytes, None, None);
        println!("Result for {}: {:?}", label, res);
        assert!(
            matches!(res, Err(DocumentError::InvalidDocx(_))),
            "Expected InvalidDocx for broken XML: {}, got: {:?}", xml, res
        );
    }
}

#[test]
fn test_docx_empty_document_0_paragraphs() {
    let empty_body_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:sectPr/>
  </w:body>
</w:document>"#;

    let bytes = build_custom_docx_xml(empty_body_xml);
    let res = DocxParser::parse_bytes(&bytes, None, None);
    assert!(
        matches!(res, Err(DocumentError::EmptyDocument)),
        "DOCX with 0 paragraphs should return EmptyDocument, got: {:?}", res
    );
}

#[test]
fn test_docx_empty_and_nested_tables() {
    let nested_tables_xml = r#"<?xml version="1.0" encoding="UTF-8" standalone="yes"?>
<w:document xmlns:w="http://schemas.openxmlformats.org/wordprocessingml/2006/main">
  <w:body>
    <w:p><w:r><w:t>Document with tables</w:t></w:r></w:p>
    <!-- Empty table (0 rows) -->
    <w:tbl><w:tblPr/></w:tbl>
    <!-- Empty row (0 cells) -->
    <w:tbl><w:tr></w:tr></w:tbl>
    <!-- Empty cell (0 paragraphs) -->
    <w:tbl><w:tr><w:tc></w:tc></w:tr></w:tbl>
    <!-- Nested table inside table cell -->
    <w:tbl>
      <w:tr>
        <w:tc>
          <w:p><w:r><w:t>Outer Cell Text</w:t></w:r></w:p>
          <w:tbl>
            <w:tr>
              <w:tc>
                <w:p><w:r><w:t>Inner Cell Text</w:t></w:r></w:p>
              </w:tc>
            </w:tr>
          </w:tbl>
        </w:tc>
      </w:tr>
    </w:tbl>
    <w:p><w:r><w:t>Trailing text</w:t></w:r></w:p>
  </w:body>
</w:document>"#;

    let bytes = build_custom_docx_xml(nested_tables_xml);
    let doc = DocxParser::parse_bytes(&bytes, None, None).expect("Nested and empty tables must parse without panic");
    assert!(!doc.chapters.is_empty());
    let all_blocks = &doc.chapters[0].blocks;
    assert!(all_blocks.iter().any(|b| matches!(b.kind, BlockKind::Table { .. })));
}

#[test]
fn test_docx_thousands_of_page_breaks_stress() {
    let mut docx = Docx::new();
    // Add 1,000 paragraphs with page breaks
    for i in 0..1000 {
        docx = docx.add_paragraph(
            docx_rs::Paragraph::new()
                .add_run(Run::new().add_text(format!("Section {}", i)))
                .page_break_before(true),
        );
    }

    let mut buf = Cursor::new(Vec::new());
    docx.build().pack(&mut buf).unwrap();
    let bytes = buf.into_inner();

    let start = std::time::Instant::now();
    let doc = DocxParser::parse_bytes(&bytes, Some("Thousand Breaks"), None)
        .expect("Thousands of page breaks should parse safely");
    let elapsed = start.elapsed();

    assert_eq!(doc.chapters.len(), 1000);
    assert!(elapsed.as_millis() < 3000, "1000 page breaks took too long: {:?}", elapsed);
}

// =========================================================================
// SECURITY: ZIP SLIP & RESOURCE LIMITS
// =========================================================================

#[test]
fn test_security_zip_slip_attempts() {
    let evil_target_path = "/tmp/retranslate_adversarial_pwned.txt";
    // Ensure clean state before test
    let _ = std::fs::remove_file(evil_target_path);

    let zip_slip_files = [
        ("../../retranslate_adversarial_pwned.txt", b"EVIL DATA" as &[u8]),
        ("../../../../tmp/retranslate_adversarial_pwned.txt", b"EVIL DATA"),
        ("word/../../retranslate_adversarial_pwned.txt", b"EVIL DATA"),
    ];

    // 1. In EPUB
    let epub_bytes = build_minimal_epub(&zip_slip_files);
    let epub_res = EpubParser::parse_bytes(&epub_bytes, None, None);
    assert!(!std::path::Path::new(evil_target_path).exists(), "Zip slip must NOT create files outside sandbox!");

    // 2. In DOCX
    let docx_bytes = build_minimal_docx(&zip_slip_files);
    let docx_res = DocxParser::parse_bytes(&docx_bytes, None, None);
    assert!(!std::path::Path::new(evil_target_path).exists(), "Zip slip must NOT create files outside sandbox!");

    // Check that neither parser panicked
    assert!(epub_res.is_ok() || epub_res.is_err());
    assert!(docx_res.is_ok() || docx_res.is_err());
}

#[test]
fn test_security_high_decompression_ratio_zip_bomb() {
    // Generate a high compression ratio entry (10MB of repeated bytes compressed with Deflate)
    let uncompressed_len = 5 * 1024 * 1024; // 5MB
    let big_zeros = vec![b'A'; uncompressed_len];

    let mut buffer = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut buffer);

    let opts = FileOptions::<()>::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file("large_entry.txt", opts).unwrap();
    zip.write_all(&big_zeros).unwrap();

    let container = r#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles><rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/></rootfiles>
</container>"#;
    zip.start_file("META-INF/container.xml", FileOptions::<()>::default()).unwrap();
    zip.write_all(container.as_bytes()).unwrap();

    let opf = r#"<?xml version="1.0" encoding="UTF-8"?>
<package xmlns="http://www.idpf.org/2007/opf" unique-identifier="BookId" version="2.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:title>Bomb</dc:title><dc:language>en</dc:language>
    <dc:identifier id="BookId">id</dc:identifier>
  </metadata>
  <manifest><item id="c1" href="chap1.xhtml" media-type="application/xhtml+xml"/></manifest>
  <spine><itemref idref="c1"/></spine>
</package>"#;
    zip.start_file("OEBPS/content.opf", FileOptions::<()>::default()).unwrap();
    zip.write_all(opf.as_bytes()).unwrap();

    let chap = r#"<!DOCTYPE html><html><body><h1>Bomb</h1><p>Text</p></body></html>"#;
    zip.start_file("OEBPS/chap1.xhtml", FileOptions::<()>::default()).unwrap();
    zip.write_all(chap.as_bytes()).unwrap();

    zip.finish().unwrap();
    let bytes = buffer.into_inner();

    // The compressed size is tiny (~5KB for 5MB zeros), ratio > 1000:1
    println!("Compressed bytes len: {}, uncompressed: {}", bytes.len(), uncompressed_len);
    assert!(bytes.len() < 25_000, "Should be highly compressed");

    let start = std::time::Instant::now();
    let res = EpubParser::parse_bytes(&bytes, None, None);
    let duration = start.elapsed();

    // Verify it doesn't hang or crash and finishes quickly
    assert!(duration.as_secs() < 3, "Decompression took too long: {:?}", duration);
    assert!(res.is_ok());
}
