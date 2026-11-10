//! EPUB Document Parser using epub-rs and scraper
//! Conforms to EPUB 2/3 specification and reTranslate Unified AST

use std::collections::HashMap;
use std::io::{Cursor, Read, Seek};
use std::path::{Component, Path, PathBuf};

use epub::doc::EpubDoc;
use scraper::{ElementRef, Html, Node, Selector};
use zip::ZipArchive;

use crate::document::error::DocumentError;
use crate::document::model::*;

pub struct EpubParser;

impl EpubParser {
    pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<Document, DocumentError> {
        let p = path.as_ref();
        if !p.exists() {
            return Err(DocumentError::FileNotFound(p.to_path_buf()));
        }
        let file = std::fs::File::open(p)?;
        let file_size = file.metadata().map(|m| m.len()).unwrap_or(0);
        Self::parse_reader(file, file_size, Some(p.to_path_buf()))
    }

    pub fn parse_bytes(
        bytes: &[u8],
        title: Option<&str>,
        file_path: Option<PathBuf>,
    ) -> Result<Document, DocumentError> {
        if bytes.is_empty() {
            return Err(DocumentError::EmptyDocument);
        }
        let cursor = Cursor::new(bytes);
        let mut doc = Self::parse_reader(cursor, bytes.len() as u64, file_path)?;
        if let Some(t) = title {
            if doc.title == "Untitled EPUB" || doc.title.is_empty() {
                doc.title = t.to_string();
            }
        }
        Ok(doc)
    }

    pub fn parse_reader<R: Read + Seek>(
        mut reader: R,
        file_size: u64,
        file_path: Option<PathBuf>,
    ) -> Result<Document, DocumentError> {
        // 1. Pre-flight check for DRM & archive integrity
        let mut zip = ZipArchive::new(&mut reader)
            .map_err(|e| DocumentError::CorruptZip(format!("Failed to read zip archive: {e}")))?;

        let mut has_container = false;
        for i in 0..zip.len() {
            if let Ok(file) = zip.by_index(i) {
                let name = file.name().to_string();
                if name == "META-INF/encryption.xml" || name == "META-INF/rights.xml" {
                    return Err(DocumentError::DrmProtected(
                        "Book is protected by DRM encryption and cannot be read".to_string(),
                    ));
                }
                if name == "META-INF/container.xml" {
                    has_container = true;
                }
            }
        }

        if !has_container {
            return Err(DocumentError::CorruptZip(
                "Missing META-INF/container.xml".to_string(),
            ));
        }

        // Reset reader cursor to start
        reader.seek(std::io::SeekFrom::Start(0))?;

        // 2. Load EpubDoc
        let mut doc = EpubDoc::from_reader(reader).map_err(|e| {
            DocumentError::MalformedPackage(format!("EPUB package parse error: {e}"))
        })?;

        let doc_id = compute_sha256_hex(
            doc.unique_identifier
                .as_deref()
                .unwrap_or("retranslate_epub")
                .as_bytes(),
        );

        let title = doc
            .get_title()
            .unwrap_or_else(|| "Untitled EPUB".to_string());
        let author = doc.mdata("creator").map(|m| m.value.clone());
        let language = doc.mdata("language").map(|m| m.value.clone());
        let publisher = doc.mdata("publisher").map(|m| m.value.clone());
        let description = doc.mdata("description").map(|m| m.value.clone());

        let (cover_image, cover_mime) = doc.get_cover().map_or((None, None), |(data, mime)| {
            (Some(data), Some(mime))
        });

        let metadata = DocumentMetadata {
            author,
            language,
            publisher,
            description,
            created_at: None,
            page_count: None,
            cover_image,
            cover_mime,
            extra: HashMap::new(),
        };

        // 3. TOC Map (Href -> Title)
        let mut toc_map = HashMap::new();
        for nav in &doc.toc {
            let path_str = nav.content.to_string_lossy().to_string();
            toc_map.insert(path_str, nav.label.clone());
        }

        // 4. Traverse spine
        let mut chapters = Vec::new();
        let spine_items = doc.spine.clone();
        let mut global_block_idx = 0;

        for (spine_idx, item) in spine_items.iter().enumerate() {
            let resource = match doc.resources.get(&item.idref) {
                Some(r) => r.clone(),
                None => continue,
            };

            let chap_content_opt = doc.get_resource(&item.idref);
            let (raw_bytes, mime) = match chap_content_opt {
                Some(pair) => pair,
                None => continue,
            };

            if !mime.contains("xhtml") && !mime.contains("html") && !mime.contains("xml") {
                continue;
            }

            let xhtml_str = String::from_utf8_lossy(&raw_bytes).to_string();
            let chap_id = format!("c{spine_idx}");
            let chap_rel_path = resource.path.to_string_lossy().to_string();

            let chap_title = toc_map
                .get(&chap_rel_path)
                .cloned()
                .unwrap_or_else(|| format!("Chapter {}", spine_idx + 1));

            let (blocks, next_idx) = Self::extract_blocks_from_html(
                &xhtml_str,
                &chap_id,
                &resource.path,
                global_block_idx,
                &mut doc,
            );
            global_block_idx = next_idx;

            if !blocks.is_empty() {
                chapters.push(Chapter {
                    id: chap_id,
                    title: chap_title,
                    order: spine_idx,
                    blocks,
                    sub_chapters: Vec::new(),
                    source_href: Some(chap_rel_path),
                });
            }
        }

        if chapters.is_empty() {
            return Err(DocumentError::EmptyDocument);
        }

        let mut document = Document {
            id: doc_id,
            title,
            format: DocumentFormat::Epub,
            file_path,
            file_size,
            metadata,
            chapters,
            total_blocks: 0,
        };
        document.recalculate_total_blocks();
        Ok(document)
    }

    /// Extract blocks from HTML string using scraper
    pub fn extract_blocks_from_html<R: Read + Seek>(
        html_str: &str,
        chap_id: &str,
        chap_path: &Path,
        mut start_block_idx: usize,
        doc: &mut EpubDoc<R>,
    ) -> (Vec<Block>, usize) {
        let mut blocks = Vec::new();
        let document = Html::parse_document(html_str);

        let body_selector = Selector::parse("body").unwrap();
        let root_elements: Vec<ElementRef> =
            if let Some(body) = document.select(&body_selector).next() {
                body.children().filter_map(ElementRef::wrap).collect()
            } else {
                document
                    .root_element()
                    .children()
                    .filter_map(ElementRef::wrap)
                    .collect()
            };

        for elem in root_elements {
            Self::traverse_block_element(
                elem,
                chap_id,
                chap_path,
                &mut start_block_idx,
                &mut blocks,
                doc,
            );
        }

        (blocks, start_block_idx)
    }

    fn traverse_block_element<R: Read + Seek>(
        elem: ElementRef,
        chap_id: &str,
        chap_path: &Path,
        block_idx: &mut usize,
        blocks: &mut Vec<Block>,
        doc: &mut EpubDoc<R>,
    ) {
        let tag = elem.value().name();
        match tag {
            "h1" | "h2" | "h3" | "h4" | "h5" | "h6" => {
                let lvl = tag.chars().nth(1).unwrap().to_digit(10).unwrap() as u8;
                let spans = Self::extract_inlines(elem);
                if !spans.is_empty() {
                    blocks.push(Block {
                        id: format!("{chap_id}_b{block_idx}"),
                        chapter_id: chap_id.to_string(),
                        index: *block_idx,
                        kind: BlockKind::Heading(lvl),
                        spans,
                        translatable: true,
                        translation: TranslationState::Untranslated,
                        metadata: BlockMetadata::default(),
                    });
                    *block_idx += 1;
                }
            }
            "p" => {
                let spans = Self::extract_inlines(elem);
                if !spans.is_empty() {
                    blocks.push(Block {
                        id: format!("{chap_id}_b{block_idx}"),
                        chapter_id: chap_id.to_string(),
                        index: *block_idx,
                        kind: BlockKind::Paragraph,
                        spans,
                        translatable: true,
                        translation: TranslationState::Untranslated,
                        metadata: BlockMetadata::default(),
                    });
                    *block_idx += 1;
                }
            }
            "blockquote" => {
                let spans = Self::extract_inlines(elem);
                if !spans.is_empty() {
                    blocks.push(Block {
                        id: format!("{chap_id}_b{block_idx}"),
                        chapter_id: chap_id.to_string(),
                        index: *block_idx,
                        kind: BlockKind::Quote,
                        spans,
                        translatable: true,
                        translation: TranslationState::Untranslated,
                        metadata: BlockMetadata::default(),
                    });
                    *block_idx += 1;
                }
            }
            "pre" => {
                let code_text: String = elem.text().collect();
                blocks.push(Block {
                    id: format!("{chap_id}_b{block_idx}"),
                    chapter_id: chap_id.to_string(),
                    index: *block_idx,
                    kind: BlockKind::CodeBlock {
                        language: None,
                        code: code_text.trim().to_string(),
                    },
                    spans: vec![],
                    translatable: false,
                    translation: TranslationState::Untranslated,
                    metadata: BlockMetadata::default(),
                });
                *block_idx += 1;
            }
            "ul" | "ol" => {
                let is_ordered = tag == "ol";
                let mut order_counter = 1;
                for child in elem.children().filter_map(ElementRef::wrap) {
                    if child.value().name() == "li" {
                        let spans = Self::extract_inlines(child);
                        if !spans.is_empty() {
                            blocks.push(Block {
                                id: format!("{chap_id}_b{block_idx}"),
                                chapter_id: chap_id.to_string(),
                                index: *block_idx,
                                kind: BlockKind::ListItem {
                                    order: if is_ordered {
                                        Some(order_counter)
                                    } else {
                                        None
                                    },
                                    depth: 0,
                                },
                                spans,
                                translatable: true,
                                translation: TranslationState::Untranslated,
                                metadata: BlockMetadata::default(),
                            });
                            *block_idx += 1;
                            order_counter += 1;
                        }
                    }
                }
            }
            "table" => {
                let (headers, rows) = Self::extract_table(elem);
                blocks.push(Block {
                    id: format!("{chap_id}_b{block_idx}"),
                    chapter_id: chap_id.to_string(),
                    index: *block_idx,
                    kind: BlockKind::Table { headers, rows },
                    spans: vec![],
                    translatable: true,
                    translation: TranslationState::Untranslated,
                    metadata: BlockMetadata::default(),
                });
                *block_idx += 1;
            }
            "img" => {
                let src = elem.value().attr("src").unwrap_or("").to_string();
                let alt = elem.value().attr("alt").map(str::to_string);
                let (img_data, mime) = Self::resolve_image(&src, chap_path, doc);
                blocks.push(Block {
                    id: format!("{chap_id}_b{block_idx}"),
                    chapter_id: chap_id.to_string(),
                    index: *block_idx,
                    kind: BlockKind::Image {
                        src,
                        alt,
                        data: img_data,
                        mime,
                        width: None,
                        height: None,
                    },
                    spans: vec![],
                    translatable: false,
                    translation: TranslationState::Untranslated,
                    metadata: BlockMetadata::default(),
                });
                *block_idx += 1;
            }
            "hr" => {
                blocks.push(Block {
                    id: format!("{chap_id}_b{block_idx}"),
                    chapter_id: chap_id.to_string(),
                    index: *block_idx,
                    kind: BlockKind::ThematicBreak,
                    spans: vec![],
                    translatable: false,
                    translation: TranslationState::Untranslated,
                    metadata: BlockMetadata::default(),
                });
                *block_idx += 1;
            }
            _ => {
                // For generic container divs, sections, articles, recurse into children
                for child in elem.children().filter_map(ElementRef::wrap) {
                    Self::traverse_block_element(
                        child,
                        chap_id,
                        chap_path,
                        block_idx,
                        blocks,
                        doc,
                    );
                }
            }
        }
    }

    /// Extract inline spans and typography styles recursively
    pub fn extract_inlines(elem: ElementRef) -> Vec<InlineSpan> {
        let mut spans = Vec::new();
        Self::collect_inlines_recursive(elem, InlineStyle::Plain, &mut spans);
        spans
    }

    fn collect_inlines_recursive(
        elem: ElementRef,
        parent_style: InlineStyle,
        spans: &mut Vec<InlineSpan>,
    ) {
        for node in elem.children() {
            match node.value() {
                Node::Text(text_node) => {
                    let text = text_node.text.to_string();
                    if !text.is_empty() {
                        spans.push(InlineSpan {
                            text,
                            style: parent_style.clone(),
                        });
                    }
                }
                Node::Element(el) => {
                    let child_elem = ElementRef::wrap(node).unwrap();
                    let tag = el.name();
                    let current_style = match tag {
                        "b" | "strong" => match parent_style {
                            InlineStyle::Italic => InlineStyle::BoldItalic,
                            _ => InlineStyle::Bold,
                        },
                        "i" | "em" => match parent_style {
                            InlineStyle::Bold => InlineStyle::BoldItalic,
                            _ => InlineStyle::Italic,
                        },
                        "code" => InlineStyle::Code,
                        "s" | "strike" | "del" => InlineStyle::Strikethrough,
                        "u" | "ins" => InlineStyle::Underline,
                        "sub" => InlineStyle::Subscript,
                        "sup" => InlineStyle::Superscript,
                        "a" => {
                            let href = el.attr("href").unwrap_or("").to_string();
                            let is_noteref = el.attr("epub:type") == Some("noteref")
                                || el.attr("class").map_or(false, |c| c.contains("noteref"));
                            if is_noteref {
                                let id = href.trim_start_matches('#').to_string();
                                InlineStyle::FootnoteRef { id, note: None }
                            } else {
                                InlineStyle::Link { url: href }
                            }
                        }
                        _ => parent_style.clone(),
                    };
                    Self::collect_inlines_recursive(child_elem, current_style, spans);
                }
                _ => {}
            }
        }
    }

    /// Parse table elements (table/tr/th/td) into TableCell matrices
    fn extract_table(elem: ElementRef) -> (Vec<TableCell>, Vec<Vec<TableCell>>) {
        let tr_sel = Selector::parse("tr").unwrap();
        let th_sel = Selector::parse("th").unwrap();
        let td_sel = Selector::parse("td").unwrap();

        let mut headers = Vec::new();
        let mut rows = Vec::new();

        for (row_idx, tr) in elem.select(&tr_sel).enumerate() {
            let th_cells: Vec<_> = tr.select(&th_sel).collect();
            if !th_cells.is_empty() && row_idx == 0 {
                for th in th_cells {
                    headers.push(TableCell {
                        spans: Self::extract_inlines(th),
                        col_span: th
                            .value()
                            .attr("colspan")
                            .and_then(|c| c.parse().ok())
                            .unwrap_or(1),
                        row_span: th
                            .value()
                            .attr("rowspan")
                            .and_then(|r| r.parse().ok())
                            .unwrap_or(1),
                        align: TableAlignment::Left,
                    });
                }
            } else {
                let mut row_cells = Vec::new();
                for td in tr.select(&td_sel) {
                    row_cells.push(TableCell {
                        spans: Self::extract_inlines(td),
                        col_span: td
                            .value()
                            .attr("colspan")
                            .and_then(|c| c.parse().ok())
                            .unwrap_or(1),
                        row_span: td
                            .value()
                            .attr("rowspan")
                            .and_then(|r| r.parse().ok())
                            .unwrap_or(1),
                        align: TableAlignment::Left,
                    });
                }
                if !row_cells.is_empty() {
                    rows.push(row_cells);
                }
            }
        }
        (headers, rows)
    }

    /// Resolve relative path inside zip archive
    fn resolve_image<R: Read + Seek>(
        src: &str,
        chap_path: &Path,
        doc: &mut EpubDoc<R>,
    ) -> (Option<Vec<u8>>, Option<String>) {
        if src.is_empty() || src.starts_with("http://") || src.starts_with("https://") {
            return (None, None);
        }

        let base_dir = chap_path.parent().unwrap_or(Path::new(""));
        let mut resolved = base_dir.to_path_buf();
        for component in Path::new(src).components() {
            match component {
                Component::Normal(c) => resolved.push(c),
                Component::ParentDir => {
                    resolved.pop();
                }
                Component::CurDir => {}
                _ => {}
            }
        }

        let resolved_str = resolved.to_string_lossy().replace('\\', "/");
        let mime_query = resolved_str.to_lowercase();
        if let Some(bytes) = doc
            .get_resource_by_path(&resolved_str)
            .or_else(|| doc.get_resource_by_path(resolved_str.trim_start_matches('/')))
        {
            let mime = if mime_query.ends_with(".png") {
                "image/png".to_string()
            } else if mime_query.ends_with(".jpg") || mime_query.ends_with(".jpeg") {
                "image/jpeg".to_string()
            } else if mime_query.ends_with(".webp") {
                "image/webp".to_string()
            } else if mime_query.ends_with(".gif") {
                "image/gif".to_string()
            } else {
                "application/octet-stream".to_string()
            };
            (Some(bytes), Some(mime))
        } else {
            (None, None)
        }
    }
}

pub fn parse_epub<P: AsRef<Path>>(path: P) -> Result<Document, DocumentError> {
    EpubParser::parse_file(path)
}

pub fn parse_epub_from_bytes(
    bytes: &[u8],
    title: Option<&str>,
) -> Result<Document, DocumentError> {
    EpubParser::parse_bytes(bytes, title, None)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use zip::write::FileOptions;
    use zip::ZipWriter;

    fn create_test_zip(files: &[(&str, &[u8])]) -> Vec<u8> {
        let mut buffer = Cursor::new(Vec::new());
        let mut zip = ZipWriter::new(&mut buffer);
        for (name, content) in files {
            zip.start_file(*name, FileOptions::<()>::default()).unwrap();
            zip.write_all(content).unwrap();
        }
        zip.finish().unwrap();
        buffer.into_inner()
    }

    #[test]
    fn test_epub_drm_detection() {
        let files = [
            ("META-INF/container.xml", b"<container/>".as_slice()),
            ("META-INF/encryption.xml", b"<encryption>DRM</encryption>".as_slice()),
        ];
        let bytes = create_test_zip(&files);
        let res = EpubParser::parse_bytes(&bytes, None, None);
        match res {
            Err(DocumentError::DrmProtected(msg)) => {
                assert!(msg.contains("DRM"));
            }
            other => panic!("Expected DrmProtected, got {:?}", other),
        }
    }

    #[test]
    fn test_epub_missing_container() {
        let files = [
            ("OEBPS/content.opf", b"<package/>".as_slice()),
        ];
        let bytes = create_test_zip(&files);
        let res = EpubParser::parse_bytes(&bytes, None, None);
        assert!(matches!(res, Err(DocumentError::CorruptZip(_))));
    }

    #[test]
    fn test_epub_unclosed_html() {
        let malformed = "<p>Unclosed paragraph <b>bold text<p>Next paragraph";
        let doc = Html::parse_document(malformed);
        let spans = EpubParser::extract_inlines(doc.root_element());
        let text: String = spans.iter().map(|s| s.text.as_str()).collect();
        assert!(text.contains("Unclosed paragraph"));
        assert!(text.contains("bold text"));
        assert!(text.contains("Next paragraph"));
    }
}

