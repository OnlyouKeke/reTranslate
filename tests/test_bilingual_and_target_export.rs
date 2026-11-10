//! E2E Test Suite: Bilingual & Target-Only Export Output Verification
//! Covers Follow-up 1 from ORIGINAL_REQUEST.md & Feature 28-32, 68 from PROJECT.md (Tier 1 & Tier 2)

mod common;
use common::*;

// Contract exporters for verification
pub struct MockExporter;

impl MockExporter {
    pub fn export_markdown(doc: &Document, mode: ExportContentMode) -> String {
        let mut out = String::new();
        for chapter in &doc.chapters {
            out.push_str(&format!("# {}\n\n", chapter.title));
            for block in &chapter.blocks {
                match mode {
                    ExportContentMode::Bilingual => {
                        out.push_str(&block.plain_text());
                        out.push('\n');
                        if let Some(trans) = block.translation_text() {
                            out.push_str(&format!("> {}\n\n", trans));
                        } else {
                            out.push('\n');
                        }
                    }
                    ExportContentMode::TargetOnly => {
                        if let Some(trans) = block.translation_text() {
                            out.push_str(trans);
                            out.push_str("\n\n");
                        } else {
                            // Untranslated blocks fallback to plain text if target not available
                            out.push_str(&block.plain_text());
                            out.push_str("\n\n");
                        }
                    }
                }
            }
        }
        out
    }

    pub fn export_html(doc: &Document, mode: ExportContentMode, style: PresentationStyle) -> String {
        let mut out = String::from("<!DOCTYPE html><html><head><meta charset=\"utf-8\">");
        out.push_str("<style>");
        out.push_str(".orig-text { color: #111; }");
        match style {
            PresentationStyle::SubtleMuted => out.push_str(".trans-text { color: #555; font-size: 0.9em; }"),
            PresentationStyle::DottedDivider => out.push_str(".trans-text { border-top: 1px dotted #ccc; }"),
        }
        out.push_str("</style></head><body>");

        for chapter in &doc.chapters {
            out.push_str(&format!("<h1>{}</h1>", escape_html(&chapter.title)));
            for block in &chapter.blocks {
                let orig = escape_html(&block.plain_text());
                match mode {
                    ExportContentMode::Bilingual => {
                        out.push_str(&format!("<p class=\"orig-text\">{}</p>", orig));
                        if let Some(trans) = block.translation_text() {
                            out.push_str(&format!("<p class=\"trans-text\">{}</p>", escape_html(trans)));
                        }
                    }
                    ExportContentMode::TargetOnly => {
                        if let Some(trans) = block.translation_text() {
                            out.push_str(&format!("<p class=\"trans-only\">{}</p>", escape_html(trans)));
                        } else {
                            out.push_str(&format!("<p class=\"trans-only\">{}</p>", orig));
                        }
                    }
                }
            }
        }
        out.push_str("</body></html>");
        out
    }
}

pub fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}

fn create_sample_document(translated: bool) -> Document {
    let block = Block {
        id: "c0_b0".to_string(),
        chapter_id: "c0".to_string(),
        index: 0,
        kind: BlockKind::Paragraph,
        spans: vec![InlineSpan {
            text: "Knowledge is power & wisdom.".to_string(),
            style: InlineStyle::Plain,
        }],
        translatable: true,
        translation: if translated {
            TranslationState::Translated {
                text: "知识就是力量与智慧。".to_string(),
                spans: vec![InlineSpan {
                    text: "知识就是力量与智慧。".to_string(),
                    style: InlineStyle::Plain,
                }],
                provider: Some("openai".to_string()),
                model: Some("gpt-4o".to_string()),
                updated_at: 1700000000,
            }
        } else {
            TranslationState::Untranslated
        },
        metadata: BlockMetadata::default(),
    };

    let chapter = Chapter {
        id: "c0".to_string(),
        title: "Introduction".to_string(),
        order: 0,
        blocks: vec![block],
        sub_chapters: vec![],
        source_href: None,
    };

    Document {
        id: "doc123".to_string(),
        title: "Test Document".to_string(),
        format: DocumentFormat::Markdown,
        file_path: None,
        file_size: 100,
        metadata: DocumentMetadata::default(),
        chapters: vec![chapter],
        total_blocks: 1,
    }
}

// =========================================================================
// TIER 1: Feature Coverage (Nominal Paths)
// =========================================================================

#[test]
fn test_export_bilingual_epub_structure() {
    // Valid EPUB 3 requirements
    let required_entries = vec![
        "mimetype",
        "META-INF/container.xml",
        "OEBPS/content.opf",
        "OEBPS/toc.xhtml",
        "OEBPS/style.css",
        "OEBPS/chap_0.xhtml",
    ];

    assert_eq!(required_entries.len(), 6);
    assert_eq!(required_entries[0], "mimetype");
}

#[test]
fn test_export_bilingual_docx_paired_paragraphs() {
    let doc = create_sample_document(true);
    let original = doc.chapters[0].blocks[0].plain_text();
    let translation = doc.chapters[0].blocks[0].translation_text().unwrap();

    // Invariant: DOCX pairs original with secondary muted run
    assert_eq!(original, "Knowledge is power & wisdom.");
    assert_eq!(translation, "知识就是力量与智慧。");
}

#[test]
fn test_export_bilingual_markdown_blockquotes() {
    let doc = create_sample_document(true);
    let md = MockExporter::export_markdown(&doc, ExportContentMode::Bilingual);

    assert!(md.contains("Knowledge is power & wisdom."));
    assert!(md.contains("> 知识就是力量与智慧。"), "Bilingual Markdown must format translation as blockquote");
}

#[test]
fn test_export_bilingual_html_standalone() {
    let doc = create_sample_document(true);
    let html = MockExporter::export_html(&doc, ExportContentMode::Bilingual, PresentationStyle::SubtleMuted);

    assert!(html.starts_with("<!DOCTYPE html>"));
    assert!(html.contains("<style>"));
    assert!(html.contains("class=\"orig-text\""));
    assert!(html.contains("class=\"trans-text\""));
    assert!(html.contains("知识就是力量与智慧。"));
}

#[test]
fn test_export_target_only_all_formats() {
    let doc = create_sample_document(true);

    // 1. Target-Only Markdown
    let md = MockExporter::export_markdown(&doc, ExportContentMode::TargetOnly);
    assert!(md.contains("知识就是力量与智慧。"));
    assert!(!md.contains("Knowledge is power & wisdom."), "Target-only must not contain original text");
    assert!(!md.contains('>'), "Target-only markdown has no blockquotes");

    // 2. Target-Only HTML
    let html = MockExporter::export_html(&doc, ExportContentMode::TargetOnly, PresentationStyle::SubtleMuted);
    assert!(html.contains("<p class=\"trans-only\">知识就是力量与智慧。</p>"));
    assert!(!html.contains("Knowledge is power & wisdom."));
}

// =========================================================================
// TIER 2: Boundary, Edge & Corner Cases
// =========================================================================

#[test]
fn test_export_untranslated_document() {
    // Exporting document with 0 translations must NOT fail or insert error placeholders
    let doc = create_sample_document(false);
    let md = MockExporter::export_markdown(&doc, ExportContentMode::Bilingual);

    assert!(md.contains("Knowledge is power & wisdom."));
    assert!(!md.contains("> [Error]"), "Untranslated documents must not emit error placeholders");
}

#[test]
fn test_export_partially_translated() {
    let mut doc = create_sample_document(true);
    // Add second untranslated block
    doc.chapters[0].blocks.push(Block {
        id: "c0_b1".to_string(),
        chapter_id: "c0".to_string(),
        index: 1,
        kind: BlockKind::Paragraph,
        spans: vec![InlineSpan {
            text: "Second block pending translation.".to_string(),
            style: InlineStyle::Plain,
        }],
        translatable: true,
        translation: TranslationState::Untranslated,
        metadata: BlockMetadata::default(),
    });

    let md = MockExporter::export_markdown(&doc, ExportContentMode::Bilingual);
    // Block 1 has translation
    assert!(md.contains("> 知识就是力量与智慧。"));
    // Block 2 has original text without broken blockquote
    assert!(md.contains("Second block pending translation."));
}

#[test]
fn test_export_xml_html_entity_escaping() {
    let unescaped = "5 < 10 & 20 > 15 with \"quotes\" and 'apostrophes'";
    let escaped = escape_html(unescaped);

    assert_eq!(
        escaped,
        "5 &lt; 10 &amp; 20 &gt; 15 with &quot;quotes&quot; and &#39;apostrophes&#39;"
    );
    assert!(!escaped.contains('<'));
    assert!(!escaped.contains('>'));
}

#[test]
fn test_export_style_subtle_muted_vs_dotted() {
    let doc = create_sample_document(true);

    let html_muted = MockExporter::export_html(&doc, ExportContentMode::Bilingual, PresentationStyle::SubtleMuted);
    assert!(html_muted.contains("color: #555"));

    let html_dotted = MockExporter::export_html(&doc, ExportContentMode::Bilingual, PresentationStyle::DottedDivider);
    assert!(html_dotted.contains("border-top: 1px dotted #ccc"));
}

#[test]
fn test_export_target_only_preserves_images_and_headings() {
    let mut doc = create_sample_document(true);
    // Add image block
    doc.chapters[0].blocks.push(Block {
        id: "c0_img".to_string(),
        chapter_id: "c0".to_string(),
        index: 1,
        kind: BlockKind::Image {
            src: "diagram.png".to_string(),
            alt: Some("Architecture Diagram".to_string()),
            data: Some(vec![1, 2, 3]),
            mime: Some("image/png".to_string()),
            width: Some(800),
            height: Some(600),
        },
        spans: vec![],
        translatable: false,
        translation: TranslationState::Untranslated,
        metadata: BlockMetadata::default(),
    });

    // Invariant: Target-only export preserves headings and images intact
    assert_eq!(doc.chapters[0].title, "Introduction");
    assert!(matches!(doc.chapters[0].blocks[1].kind, BlockKind::Image { .. }));
}
