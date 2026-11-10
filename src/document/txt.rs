//! TXT Document Parser with Multi-Tier Auto-Encoding Detection & Paragraph Segmentation

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use chardetng::{EncodingDetector, Iso2022JpDetection, Utf8Detection};
use encoding_rs::{GB18030, UTF_16BE, UTF_16LE};
use regex::Regex;

use crate::document::error::DocumentError;
use crate::document::model::*;

static RE_CHINESE_CHAP: OnceLock<Regex> = OnceLock::new();
static RE_WESTERN_CHAP: OnceLock<Regex> = OnceLock::new();

fn get_re_chinese_chap() -> &'static Regex {
    RE_CHINESE_CHAP.get_or_init(|| {
        Regex::new(r"^第\s*[0-9一二三四五六七八九十百千万]+\s*[章回节卷集篇部]").unwrap()
    })
}

fn get_re_western_chap() -> &'static Regex {
    RE_WESTERN_CHAP.get_or_init(|| {
        Regex::new(r"(?i)^(Chapter|Act|Book)\s+([0-9]+|[IVXLCDM]+)([:\.\-\s]|$)").unwrap()
    })
}

/// Detects character encoding and returns a valid UTF-8 String
pub fn detect_and_decode(bytes: &[u8]) -> Result<String, DocumentError> {
    if bytes.is_empty() {
        return Ok(String::new());
    }

    // 1. BOM Sniffing
    if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        return std::str::from_utf8(&bytes[3..])
            .map(|s| s.to_string())
            .map_err(|e| DocumentError::EncodingError(format!("Invalid UTF-8 BOM content: {e}")));
    }
    if bytes.starts_with(&[0xFF, 0xFE]) {
        let (s, had_errors) = UTF_16LE.decode_without_bom_handling(&bytes[2..]);
        if had_errors {
            return Err(DocumentError::EncodingError("Invalid UTF-16LE sequence".to_string()));
        }
        return Ok(s.into_owned());
    }
    if bytes.starts_with(&[0xFE, 0xFF]) {
        let (s, had_errors) = UTF_16BE.decode_without_bom_handling(&bytes[2..]);
        if had_errors {
            return Err(DocumentError::EncodingError("Invalid UTF-16BE sequence".to_string()));
        }
        return Ok(s.into_owned());
    }

    // 2. Validate clean UTF-8 fast-path
    if let Ok(valid_utf8) = std::str::from_utf8(bytes) {
        return Ok(valid_utf8.to_string());
    }

    // 3. Fallback cascade: check GB18030 (which covers GBK and GB2312)
    // Legacy Chinese texts often fail pure UTF-8 validation
    let (gbk_decoded, gbk_errors) = GB18030.decode_without_bom_handling(bytes);
    let cjk_count = gbk_decoded.chars().filter(|c| is_cjk(*c)).count();
    if !gbk_errors && cjk_count > 0 {
        return Ok(gbk_decoded.into_owned());
    }

    // 4. Statistical detector via chardetng
    let mut detector = EncodingDetector::new(Iso2022JpDetection::Deny);
    detector.feed(bytes, true);
    let encoding = detector.guess(None, Utf8Detection::Allow);
    let (decoded, had_errors) = encoding.decode_without_bom_handling(bytes);

    // If chardetng guessed a single-byte encoding (like windows-1252) with 0 errors,
    // but GB18030 produced genuine CJK characters (e.g. truncated odd byte at EOF),
    // prioritize CJK content over European mojibake:
    if cjk_count > 0 && encoding != encoding_rs::GB18030 && encoding != encoding_rs::GBK {
        let decoded_cjk = decoded.chars().filter(|c| is_cjk(*c)).count();
        if cjk_count > decoded_cjk {
            return Ok(gbk_decoded.into_owned());
        }
    }

    if !had_errors {
        return Ok(decoded.into_owned());
    }

    // 5. If GB18030 succeeded without errors (even without detected CJK ideographs), use it
    if !gbk_errors {
        return Ok(gbk_decoded.into_owned());
    }

    // 6. Return best lossy decode with \u{FFFD} replacement characters
    Ok(decoded.into_owned())
}

/// Normalizes CRLF and CR to standard LF newlines
pub fn normalize_newlines(text: &str) -> String {
    text.replace("\r\n", "\n").replace('\r', "\n")
}

/// Check if a character is within CJK unicode ranges
pub fn is_cjk(ch: char) -> bool {
    matches!(
        ch,
        '\u{4E00}'..='\u{9FFF}'       // CJK Unified Ideographs
            | '\u{3400}'..='\u{4DBF}' // CJK Extension A
            | '\u{20000}'..='\u{2A6DF}' // CJK Extension B
            | '\u{3000}'..='\u{303F}' // CJK Symbols and Punctuation
            | '\u{FF00}'..='\u{FFEF}' // Halfwidth and Fullwidth Forms
            | '\u{3040}'..='\u{309F}' // Hiragana
            | '\u{30A0}'..='\u{30FF}' // Katakana
            | '\u{AC00}'..='\u{D7AF}' // Hangul Syllables
    )
}

/// Check if a text line represents a chapter heading
pub fn is_chapter_heading(line: &str) -> bool {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return false;
    }
    // Negative match check: "第一本书" is not a chapter heading
    if trimmed.starts_with("第一本书") {
        return false;
    }
    get_re_chinese_chap().is_match(trimmed) || get_re_western_chap().is_match(trimmed)
}

/// Splits text into paragraphs, normalizing newlines, collapsing excessive blank lines,
/// merging soft-wrapped lines (with CJK spacing awareness), and chunking monolithic lines.
pub fn segment_txt_paragraphs(text: &str) -> Vec<String> {
    let normalized = normalize_newlines(text);
    let raw_paragraphs = normalized.split("\n\n");
    let mut result = Vec::new();

    for p in raw_paragraphs {
        let trimmed = p.trim();
        if trimmed.is_empty() {
            continue;
        }

        let lines: Vec<&str> = trimmed
            .lines()
            .map(|l| l.trim())
            .filter(|l| !l.is_empty())
            .collect();

        if lines.len() <= 1 {
            // Check if monolithic long line (> 1500 chars) needs sentence splitting
            if trimmed.chars().count() > 1500 {
                result.extend(chunk_monolithic_line(trimmed));
            } else {
                result.push(trimmed.to_string());
            }
        } else {
            let mut merged = String::new();
            for (idx, line) in lines.iter().enumerate() {
                if idx > 0 {
                    let prev_last = lines[idx - 1].chars().last().unwrap_or(' ');
                    let curr_first = line.chars().next().unwrap_or(' ');
                    // CJK boundary: do NOT insert space
                    if is_cjk(prev_last) || is_cjk(curr_first) {
                        // no space
                    } else {
                        merged.push(' ');
                    }
                }
                merged.push_str(line);
            }
            if merged.chars().count() > 1500 {
                result.extend(chunk_monolithic_line(&merged));
            } else {
                result.push(merged);
            }
        }
    }
    result
}

/// Splits monolithic lines longer than 1500 characters on sentence boundaries
fn chunk_monolithic_line(text: &str) -> Vec<String> {
    let mut chunks = Vec::new();
    let mut current = String::new();

    for ch in text.chars() {
        current.push(ch);
        if (ch == '。' || ch == '！' || ch == '？' || ch == '.' || ch == '!' || ch == '?')
            && current.chars().count() >= 300
        {
            chunks.push(current.trim().to_string());
            current = String::new();
        }
    }
    if !current.trim().is_empty() {
        chunks.push(current.trim().to_string());
    }
    chunks
}

pub struct TxtParser;

impl TxtParser {
    pub fn parse_file<P: AsRef<Path>>(path: P) -> Result<Document, DocumentError> {
        let p = path.as_ref();
        if !p.exists() {
            return Err(DocumentError::FileNotFound(p.to_path_buf()));
        }
        let bytes = std::fs::read(p)?;
        let title = p
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("Untitled TXT");
        Self::parse_bytes(&bytes, Some(title), Some(p.to_path_buf()))
    }

    pub fn parse_bytes(
        bytes: &[u8],
        title: Option<&str>,
        file_path: Option<PathBuf>,
    ) -> Result<Document, DocumentError> {
        if bytes.is_empty() {
            return Err(DocumentError::EmptyDocument);
        }

        let doc_id = compute_sha256_hex(bytes);
        let doc_title = title.unwrap_or("Untitled TXT").to_string();
        let decoded_text = detect_and_decode(bytes)?;

        if decoded_text.trim().is_empty() {
            return Err(DocumentError::EmptyDocument);
        }

        let paragraphs = segment_txt_paragraphs(&decoded_text);
        if paragraphs.is_empty() {
            return Err(DocumentError::EmptyDocument);
        }

        let mut chapters = Vec::new();
        let mut chap_idx = 0;
        let mut current_chap = Chapter {
            id: format!("chap_{chap_idx}"),
            title: doc_title.clone(),
            order: chap_idx,
            blocks: Vec::new(),
            sub_chapters: Vec::new(),
            source_href: None,
        };

        for p in paragraphs {
            if is_chapter_heading(&p) {
                if !current_chap.blocks.is_empty() {
                    chapters.push(current_chap);
                    chap_idx += 1;
                    current_chap = Chapter {
                        id: format!("chap_{chap_idx}"),
                        title: p.clone(),
                        order: chap_idx,
                        blocks: Vec::new(),
                        sub_chapters: Vec::new(),
                        source_href: None,
                    };
                } else {
                    current_chap.title = p.clone();
                }

                let block_idx = current_chap.blocks.len();
                current_chap.blocks.push(Block::new_heading(
                    format!("{}_b{}", current_chap.id, block_idx),
                    current_chap.id.clone(),
                    block_idx,
                    1,
                    p,
                ));
            } else {
                let block_idx = current_chap.blocks.len();
                current_chap.blocks.push(Block::new_paragraph(
                    format!("{}_b{}", current_chap.id, block_idx),
                    current_chap.id.clone(),
                    block_idx,
                    p,
                ));
            }
        }

        if !current_chap.blocks.is_empty() {
            chapters.push(current_chap);
        }

        let mut doc = Document::new(doc_id, doc_title, DocumentFormat::Txt);
        doc.file_path = file_path;
        doc.file_size = bytes.len() as u64;
        doc.chapters = chapters;
        doc.recalculate_total_blocks();
        Ok(doc)
    }
}

pub fn parse_txt<P: AsRef<Path>>(path: P) -> Result<Document, DocumentError> {
    TxtParser::parse_file(path)
}

pub fn parse_txt_from_bytes(bytes: &[u8], title: Option<&str>) -> Result<Document, DocumentError> {
    TxtParser::parse_bytes(bytes, title, None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_txt_encoding_and_bom() {
        let text = "你好，reTranslate！";
        let mut utf8_bom = vec![0xEF, 0xBB, 0xBF];
        utf8_bom.extend_from_slice(text.as_bytes());

        let decoded = detect_and_decode(&utf8_bom).unwrap();
        assert_eq!(decoded, text);

        let doc = parse_txt_from_bytes(text.as_bytes(), Some("Test")).unwrap();
        assert_eq!(doc.total_blocks, 1);
        assert_eq!(doc.chapters[0].blocks[0].plain_text(), text);
    }

    #[test]
    fn test_txt_chapter_detection() {
        let content = "Chapter 1: Begin\n\nFirst paragraph here.\n\nChapter 2: Middle\n\nSecond paragraph.";
        let doc = parse_txt_from_bytes(content.as_bytes(), Some("Novel")).unwrap();
        assert_eq!(doc.chapters.len(), 2);
        assert_eq!(doc.chapters[0].title, "Chapter 1: Begin");
        assert_eq!(doc.chapters[1].title, "Chapter 2: Middle");
    }

    #[test]
    fn test_txt_utf16_and_gbk() {
        // UTF-16LE: "AB"
        let utf16le = [0xFF, 0xFE, 0x41, 0x00, 0x42, 0x00];
        let decoded = detect_and_decode(&utf16le).unwrap();
        assert_eq!(decoded, "AB");

        // UTF-16BE: "AB"
        let utf16be = [0xFE, 0xFF, 0x00, 0x41, 0x00, 0x42];
        let decoded_be = detect_and_decode(&utf16be).unwrap();
        assert_eq!(decoded_be, "AB");

        // GBK: "你好世界"
        let gbk_bytes = [0xC4, 0xE3, 0xBA, 0xC3, 0xCA, 0xC0, 0xBD, 0xE7];
        let decoded_gbk = detect_and_decode(&gbk_bytes).unwrap();
        assert_eq!(decoded_gbk, "你好世界");
    }

    #[test]
    fn test_txt_soft_wraps_and_blank_lines() {
        let latin = "The quick brown\nfox jumps over\nthe lazy dog.";
        let p_latin = segment_txt_paragraphs(latin);
        assert_eq!(p_latin.len(), 1);
        assert_eq!(p_latin[0], "The quick brown fox jumps over the lazy dog.");

        let cjk = "这是一段很长很长的中文段落，\n在行尾发生了自动换行，\n继续讲述故事。";
        let p_cjk = segment_txt_paragraphs(cjk);
        assert_eq!(p_cjk.len(), 1);
        assert_eq!(p_cjk[0], "这是一段很长很长的中文段落，在行尾发生了自动换行，继续讲述故事。");

        let mut blank_lines = String::from("First\n");
        for _ in 0..1000 {
            blank_lines.push('\n');
        }
        blank_lines.push_str("Second\n");
        let p_blanks = segment_txt_paragraphs(&blank_lines);
        assert_eq!(p_blanks.len(), 2);
    }
}

