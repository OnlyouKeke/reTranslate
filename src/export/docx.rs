use docx_rs::{Docx, Paragraph, Run};
use std::io::Cursor;

use crate::document::model::Document;
use crate::export::{ExportContentMode, PresentationStyle};

pub fn export_docx(
    doc: &Document,
    mode: ExportContentMode,
    style: PresentationStyle,
) -> Result<Vec<u8>, String> {
    let mut docx = Docx::new();

    // Document title
    let title_p = Paragraph::new().add_run(Run::new().add_text(&doc.title).size(36).bold());
    docx = docx.add_paragraph(title_p);

    for chapter in &doc.chapters {
        if !chapter.title.is_empty() && chapter.title != doc.title {
            let chap_p = Paragraph::new().add_run(Run::new().add_text(&chapter.title).size(28).bold());
            docx = docx.add_paragraph(chap_p);
        }

        for block in &chapter.blocks {
            let orig = block.plain_text();
            let trans = block.translation_text();

            match mode {
                ExportContentMode::Bilingual => {
                    // Original paragraph
                    let orig_p = Paragraph::new().add_run(Run::new().add_text(&orig).size(24));
                    docx = docx.add_paragraph(orig_p);

                    // Translated paragraph
                    if let Some(t) = trans {
                        let mut run = Run::new().add_text(t).size(22).color("64748B");
                        if matches!(style, PresentationStyle::SubtleMuted) {
                            run = run.italic();
                        }
                        let trans_p = Paragraph::new().add_run(run);
                        docx = docx.add_paragraph(trans_p);
                    }
                }
                ExportContentMode::TargetOnly => {
                    let text = trans.unwrap_or(&orig);
                    let p = Paragraph::new().add_run(Run::new().add_text(text).size(24));
                    docx = docx.add_paragraph(p);
                }
            }
        }
    }

    let mut buf = Cursor::new(Vec::new());
    docx.build()
        .pack(&mut buf)
        .map_err(|e| format!("Failed to build DOCX package: {e:?}"))?;

    Ok(buf.into_inner())
}
