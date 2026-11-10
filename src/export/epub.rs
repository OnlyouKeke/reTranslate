use std::io::{Cursor, Write};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

use crate::document::model::*;
use crate::export::html::escape_html;
use crate::export::{ExportContentMode, PresentationStyle};

pub fn export_epub(
    doc: &Document,
    mode: ExportContentMode,
    style: PresentationStyle,
) -> Result<Vec<u8>, String> {
    let mut buf = Cursor::new(Vec::new());
    let mut zip = ZipWriter::new(&mut buf);

    // 1. mimetype MUST be first and stored uncompressed
    let stored = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    zip.start_file("mimetype", stored)
        .map_err(|e| format!("Failed to write mimetype: {e}"))?;
    zip.write_all(b"application/epub+zip")
        .map_err(|e| format!("Failed to write mimetype: {e}"))?;

    let def_opts = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);

    // 2. META-INF/container.xml
    zip.start_file("META-INF/container.xml", def_opts)
        .map_err(|e| format!("Failed to write container.xml: {e}"))?;
    zip.write_all(
        br#"<?xml version="1.0" encoding="UTF-8"?>
<container version="1.0" xmlns="urn:oasis:names:tc:opendocument:xmlns:container">
  <rootfiles>
    <rootfile full-path="OEBPS/content.opf" media-type="application/oebps-package+xml"/>
  </rootfiles>
</container>"#,
    )
    .map_err(|e| format!("Failed to write container.xml: {e}"))?;

    // 3. Write chapters
    let mut manifest_items = Vec::new();
    let mut spine_items = Vec::new();
    let mut ncx_points = Vec::new();

    for (idx, chapter) in doc.chapters.iter().enumerate() {
        let filename = format!("chapter_{idx}.xhtml");
        let item_id = format!("chap_{idx}");

        manifest_items.push(format!(
            r#"<item id="{item_id}" href="{filename}" media-type="application/xhtml+xml"/>"#
        ));
        spine_items.push(format!(r#"<itemref idref="{item_id}"/>"#));

        let chap_title = if chapter.title.is_empty() {
            format!("Chapter {}", idx + 1)
        } else {
            chapter.title.clone()
        };

        ncx_points.push(format!(
            r#"<navPoint id="nav_{idx}" playOrder="{play}">
  <navLabel><text>{title}</text></navLabel>
  <content src="{filename}"/>
</navPoint>"#,
            idx = idx,
            play = idx + 1,
            title = escape_html(&chap_title),
            filename = filename
        ));

        // Generate chapter XHTML
        let mut chapter_xhtml = format!(
            r#"<?xml version="1.0" encoding="utf-8"?>
<!DOCTYPE html>
<html xmlns="http://www.w3.org/1999/xhtml" xmlns:epub="http://www.idpf.org/2007/ops">
<head>
  <title>{}</title>
  <style>
    body {{ font-family: sans-serif; line-height: 1.6; margin: 1em; }}
    h1, h2 {{ color: #111; }}
    .orig-text {{ color: #222; margin: 0.8em 0; }}
"#,
            escape_html(&chap_title)
        );

        match style {
            PresentationStyle::SubtleMuted => {
                chapter_xhtml.push_str("    .trans-text { color: #666; font-size: 0.9em; margin-top: -0.4em; margin-bottom: 1.2em; }\n");
            }
            PresentationStyle::DottedDivider => {
                chapter_xhtml.push_str("    .trans-text { color: #444; font-size: 0.9em; border-top: 1px dotted #ccc; padding-top: 0.4em; margin-bottom: 1.2em; }\n");
            }
        }
        chapter_xhtml.push_str("    .trans-only { color: #222; margin: 0.8em 0; }\n  </style>\n</head>\n<body>\n");
        chapter_xhtml.push_str(&format!("  <h2>{}</h2>\n", escape_html(&chap_title)));

        for block in &chapter.blocks {
            let orig = escape_html(&block.plain_text());
            let trans = block.translation_text().map(escape_html);

            match mode {
                ExportContentMode::Bilingual => {
                    chapter_xhtml.push_str(&format!("  <p class=\"orig-text\">{}</p>\n", orig));
                    if let Some(t) = trans {
                        chapter_xhtml.push_str(&format!("  <p class=\"trans-text\">{}</p>\n", t));
                    }
                }
                ExportContentMode::TargetOnly => {
                    if let Some(t) = trans {
                        chapter_xhtml.push_str(&format!("  <p class=\"trans-only\">{}</p>\n", t));
                    } else {
                        chapter_xhtml.push_str(&format!("  <p class=\"trans-only\">{}</p>\n", orig));
                    }
                }
            }
        }

        chapter_xhtml.push_str("</body>\n</html>\n");

        zip.start_file(format!("OEBPS/{filename}"), def_opts)
            .map_err(|e| format!("Failed to write {filename}: {e}"))?;
        zip.write_all(chapter_xhtml.as_bytes())
            .map_err(|e| format!("Failed to write {filename}: {e}"))?;
    }

    // 4. OEBPS/toc.ncx
    let toc_ncx = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<ncx xmlns="http://www.daisy.org/z3986/2005/ncx/" version="2005-1">
  <head>
    <meta name="dtb:uid" content="{uid}"/>
  </head>
  <docTitle><text>{title}</text></docTitle>
  <navMap>
    {points}
  </navMap>
</ncx>"#,
        uid = doc.id,
        title = escape_html(&doc.title),
        points = ncx_points.join("\n")
    );

    zip.start_file("OEBPS/toc.ncx", def_opts)
        .map_err(|e| format!("Failed to write toc.ncx: {e}"))?;
    zip.write_all(toc_ncx.as_bytes())
        .map_err(|e| format!("Failed to write toc.ncx: {e}"))?;

    // 5. OEBPS/content.opf
    let content_opf = format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<package xmlns="http://www.idpf.org/2007/opf" unique-identifier="BookId" version="3.0">
  <metadata xmlns:dc="http://purl.org/dc/elements/1.1/">
    <dc:identifier id="BookId">{uid}</dc:identifier>
    <dc:title>{title}</dc:title>
    <dc:language>zh-CN</dc:language>
  </metadata>
  <manifest>
    <item id="ncx" href="toc.ncx" media-type="application/x-dtbncx+xml"/>
    {manifest}
  </manifest>
  <spine toc="ncx">
    {spine}
  </spine>
</package>"#,
        uid = doc.id,
        title = escape_html(&doc.title),
        manifest = manifest_items.join("\n    "),
        spine = spine_items.join("\n    ")
    );

    zip.start_file("OEBPS/content.opf", def_opts)
        .map_err(|e| format!("Failed to write content.opf: {e}"))?;
    zip.write_all(content_opf.as_bytes())
        .map_err(|e| format!("Failed to write content.opf: {e}"))?;

    zip.finish()
        .map_err(|e| format!("Failed to finalize EPUB zip: {e}"))?;

    Ok(buf.into_inner())
}
