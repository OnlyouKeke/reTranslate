use crate::document::model::*;
use crate::export::ExportContentMode;

pub fn export_markdown(doc: &Document, mode: ExportContentMode) -> String {
    let mut out = String::new();
    out.push_str(&format!("# {}\n\n", doc.title));

    for chapter in &doc.chapters {
        if !chapter.title.is_empty() && chapter.title != doc.title {
            out.push_str(&format!("## {}\n\n", chapter.title));
        }

        for block in &chapter.blocks {
            let orig = block.plain_text();
            let trans = block.translation_text();

            match mode {
                ExportContentMode::Bilingual => {
                    match block.kind {
                        BlockKind::Heading(level) => {
                            let hashes = "#".repeat((level as usize).clamp(1, 6));
                            out.push_str(&format!("{} {}\n", hashes, orig));
                            if let Some(t) = trans {
                                out.push_str(&format!("> {} {}\n\n", hashes, t));
                            } else {
                                out.push('\n');
                            }
                        }
                        BlockKind::CodeBlock { ref code, ref language } => {
                            let lang = language.as_deref().unwrap_or("");
                            out.push_str(&format!("```{}\n{}\n```\n\n", lang, code));
                        }
                        _ => {
                            out.push_str(&orig);
                            out.push('\n');
                            if let Some(t) = trans {
                                out.push_str(&format!("> {}\n\n", t));
                            } else {
                                out.push('\n');
                            }
                        }
                    }
                }
                ExportContentMode::TargetOnly => {
                    match block.kind {
                        BlockKind::Heading(level) => {
                            let hashes = "#".repeat((level as usize).clamp(1, 6));
                            if let Some(t) = trans {
                                out.push_str(&format!("{} {}\n\n", hashes, t));
                            } else {
                                out.push_str(&format!("{} {}\n\n", hashes, orig));
                            }
                        }
                        BlockKind::CodeBlock { ref code, ref language } => {
                            let lang = language.as_deref().unwrap_or("");
                            out.push_str(&format!("```{}\n{}\n```\n\n", lang, code));
                        }
                        _ => {
                            if let Some(t) = trans {
                                out.push_str(t);
                                out.push_str("\n\n");
                            } else {
                                out.push_str(&orig);
                                out.push_str("\n\n");
                            }
                        }
                    }
                }
            }
        }
    }

    out
}
