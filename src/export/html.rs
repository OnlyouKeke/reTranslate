use crate::document::model::*;
use crate::export::{ExportContentMode, PresentationStyle};

pub fn export_html(
    doc: &Document,
    mode: ExportContentMode,
    style: PresentationStyle,
) -> String {
    let mut out = String::from("<!DOCTYPE html>\n<html lang=\"zh-CN\">\n<head>\n");
    out.push_str("<meta charset=\"utf-8\">\n");
    out.push_str("<meta name=\"viewport\" content=\"width=device-width, initial-scale=1.0\">\n");
    out.push_str(&format!("<title>{}</title>\n", escape_html(&doc.title)));
    out.push_str("<style>\n");
    out.push_str("body { font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, 'Noto Sans', sans-serif; line-height: 1.7; max-width: 800px; margin: 40px auto; padding: 0 20px; color: #1e293b; background: #fff; }\n");
    out.push_str("h1, h2, h3 { color: #0f172a; margin-top: 1.8em; margin-bottom: 0.6em; }\n");
    out.push_str("p { margin: 1em 0; }\n");
    out.push_str(".orig-text { color: #1e293b; font-size: 1.05em; }\n");

    match style {
        PresentationStyle::SubtleMuted => {
            out.push_str(".trans-text { color: #64748b; font-size: 0.95em; margin-top: -0.5em; margin-bottom: 1.5em; }\n");
        }
        PresentationStyle::DottedDivider => {
            out.push_str(".trans-text { color: #475569; font-size: 0.95em; border-top: 1px dotted #cbd5e1; padding-top: 0.5em; margin-top: -0.3em; margin-bottom: 1.5em; }\n");
        }
    }

    out.push_str(".trans-only { color: #1e293b; font-size: 1.05em; margin: 1em 0; }\n");
    out.push_str("pre { background: #f8fafc; border: 1px solid #e2e8f0; border-radius: 6px; padding: 12px; overflow-x: auto; font-family: monospace; }\n");
    out.push_str("</style>\n</head>\n<body>\n");

    out.push_str(&format!("<h1>{}</h1>\n", escape_html(&doc.title)));

    for chapter in &doc.chapters {
        if !chapter.title.is_empty() && chapter.title != doc.title {
            out.push_str(&format!("<h2>{}</h2>\n", escape_html(&chapter.title)));
        }

        for block in &chapter.blocks {
            let orig = escape_html(&block.plain_text());
            let trans = block.translation_text().map(escape_html);

            match mode {
                ExportContentMode::Bilingual => {
                    match block.kind {
                        BlockKind::Heading(level) => {
                            let tag = format!("h{}", (level as usize).clamp(1, 6));
                            out.push_str(&format!("<{} class=\"orig-text\">{}</{}>\n", tag, orig, tag));
                            if let Some(t) = trans {
                                out.push_str(&format!("<{} class=\"trans-text\">{}</{}>\n", tag, t, tag));
                            }
                        }
                        BlockKind::CodeBlock { ref code, .. } => {
                            out.push_str(&format!("<pre><code>{}</code></pre>\n", escape_html(code)));
                        }
                        _ => {
                            out.push_str(&format!("<p class=\"orig-text\">{}</p>\n", orig));
                            if let Some(t) = trans {
                                out.push_str(&format!("<p class=\"trans-text\">{}</p>\n", t));
                            }
                        }
                    }
                }
                ExportContentMode::TargetOnly => {
                    match block.kind {
                        BlockKind::Heading(level) => {
                            let tag = format!("h{}", (level as usize).clamp(1, 6));
                            if let Some(t) = trans {
                                out.push_str(&format!("<{} class=\"trans-only\">{}</{}>\n", tag, t, tag));
                            } else {
                                out.push_str(&format!("<{} class=\"trans-only\">{}</{}>\n", tag, orig, tag));
                            }
                        }
                        BlockKind::CodeBlock { ref code, .. } => {
                            out.push_str(&format!("<pre><code>{}</code></pre>\n", escape_html(code)));
                        }
                        _ => {
                            if let Some(t) = trans {
                                out.push_str(&format!("<p class=\"trans-only\">{}</p>\n", t));
                            } else {
                                out.push_str(&format!("<p class=\"trans-only\">{}</p>\n", orig));
                            }
                        }
                    }
                }
            }
        }
    }

    out.push_str("</body>\n</html>\n");
    out
}

pub fn escape_html(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#39;")
}
