//! Numbered JSON Batch Translation Protocol
//!
//! Enforces strict ID alignment to prevent LLMs from merging, skipping, or swapping paragraphs.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BatchSegmentInput {
    pub id: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BatchSegmentOutput {
    pub id: usize,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchTranslationResponse {
    pub segments: Vec<BatchSegmentOutput>,
}

#[derive(Debug, PartialEq, Eq)]
pub enum BatchValidationResult {
    Success(Vec<(usize, String)>),
    Mismatch {
        expected_ids: Vec<usize>,
        received_ids: Vec<usize>,
        reason: String,
    },
    ParseError(String),
}

/// Strips markdown code block fences (```json ... ```) commonly output by LLMs
pub fn strip_markdown_fences(input: &str) -> String {
    let trimmed = input.trim();
    if trimmed.starts_with("```") {
        let lines: Vec<&str> = trimmed.lines().collect();
        if lines.len() >= 2 && lines.last().map_or(false, |l| l.trim().starts_with("```")) {
            return lines[1..lines.len() - 1].join("\n").trim().to_string();
        }
    }
    trimmed.to_string()
}

/// Validates raw JSON output from the LLM against the list of expected segment IDs
pub fn validate_batch_response(
    expected_ids: &[usize],
    raw_response: &str,
) -> BatchValidationResult {
    let clean_json = strip_markdown_fences(raw_response);
    let parsed: BatchTranslationResponse = match serde_json::from_str(&clean_json) {
        Ok(res) => res,
        Err(e) => return BatchValidationResult::ParseError(e.to_string()),
    };

    let received_ids: Vec<usize> = parsed.segments.iter().map(|s| s.id).collect();
    if parsed.segments.len() != expected_ids.len() {
        return BatchValidationResult::Mismatch {
            expected_ids: expected_ids.to_vec(),
            received_ids,
            reason: format!(
                "Count mismatch: expected {} segments, received {}",
                expected_ids.len(),
                parsed.segments.len()
            ),
        };
    }

    for (exp, seg) in expected_ids.iter().zip(&parsed.segments) {
        if *exp != seg.id {
            return BatchValidationResult::Mismatch {
                expected_ids: expected_ids.to_vec(),
                received_ids,
                reason: format!("ID sequence mismatch: expected {}, got {}", exp, seg.id),
            };
        }
    }

    BatchValidationResult::Success(
        parsed
            .segments
            .into_iter()
            .map(|s| (s.id, s.text))
            .collect(),
    )
}

/// Generates a structured system prompt instructing the LLM to output numbered JSON
pub fn build_batch_system_prompt(target_lang: &str) -> String {
    format!(
        "You are an expert literary and academic translator translating to {target_lang}. \
Translate the following array of numbered segments accurately and naturally. \
Maintain the exact formatting, tone, and casing. \
You MUST return ONLY a valid JSON object matching this schema: \
{{\"segments\": [ {{\"id\": <number>, \"text\": \"<translated text>\"}} ]}}. \
Do NOT merge or skip any IDs. Every input ID must be present in the output array in the exact same order."
    )
}

/// Serializes input segments into the user prompt payload
pub fn build_batch_user_prompt(segments: &[BatchSegmentInput]) -> String {
    serde_json::to_string(segments).unwrap_or_default()
}
