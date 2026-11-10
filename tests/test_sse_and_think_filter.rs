//! E2E Test Suite: SSE Streaming & Think-Tag Stripping State Machine
//! Covers Feature 35, 36, 37 from PROJECT.md & TEST_INFRA.md (Tier 1 & Tier 2)

mod common;
use common::*;

// Contract model for SSE parsing
pub fn parse_sse_line(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed == "data: [DONE]" {
        return Some("[DONE]".to_string());
    }
    if let Some(payload) = trimmed.strip_prefix("data: ") {
        // Parse JSON chunk: {"choices":[{"delta":{"content":"foo"}}]}
        if let Ok(val) = serde_json::from_str::<serde_json::Value>(payload) {
            // Discard reasoning_content strictly (DeepSeek-R1 API standard)
            if let Some(content) = val["choices"][0]["delta"]["content"].as_str() {
                return Some(content.to_string());
            }
        }
    }
    None
}

// =========================================================================
// TIER 1: Feature Coverage (Nominal Paths)
// =========================================================================

#[test]
fn test_sse_standard_stream_parsing() {
    let raw_stream = vec![
        "data: {\"choices\":[{\"delta\":{\"content\":\"Hello \"}}]}",
        "data: {\"choices\":[{\"delta\":{\"content\":\"World!\"}}]}",
        "data: [DONE]",
    ];

    let mut assembled = String::new();
    let mut terminated = false;

    for line in raw_stream {
        if let Some(chunk) = parse_sse_line(line) {
            if chunk == "[DONE]" {
                terminated = true;
                break;
            }
            assembled.push_str(&chunk);
        }
    }

    assert_eq!(assembled, "Hello World!");
    assert!(terminated, "Stream must terminate on [DONE]");
}

#[test]
fn test_sse_done_termination() {
    let done_line = "data: [DONE]";
    let parsed = parse_sse_line(done_line);
    assert_eq!(parsed, Some("[DONE]".to_string()));
}

#[test]
fn test_think_filter_nominal_stripping() {
    let mut filter = ThinkFilter::new();
    let token1 = filter.process_token("<think>I should translate this text carefully</think>");
    let token2 = filter.process_token("Actual translation text.");

    let final_text = format!("{}{}", token1, token2);
    assert_eq!(final_text, "Actual translation text.");
}

#[test]
fn test_reasoning_content_delta_discarded() {
    // Modern reasoning model emits reasoning_content alongside content: None
    let reasoning_line = "data: {\"choices\":[{\"delta\":{\"reasoning_content\":\"Internal chain of thought...\"}}]}";
    let content_line = "data: {\"choices\":[{\"delta\":{\"content\":\"Public translated text\"}}]}";

    let r_chunk = parse_sse_line(reasoning_line);
    let c_chunk = parse_sse_line(content_line);

    assert_eq!(r_chunk, None, "reasoning_content must be discarded");
    assert_eq!(c_chunk, Some("Public translated text".to_string()));
}

#[test]
fn test_multiline_reasoning_stripping() {
    let mut filter = ThinkFilter::new();
    let mut output = String::new();

    output.push_str(&filter.process_token("<think>\nLine 1\nLine 2\nLine 3\n</think>"));
    output.push_str(&filter.process_token("Clean Output"));

    assert_eq!(output, "Clean Output");
}

// =========================================================================
// TIER 2: Boundary, Edge & Corner Cases
// =========================================================================

#[test]
fn test_think_tag_split_opening_chunk() {
    // Tag split across chunk boundaries: "<thi" then "nk>thought</think>target"
    let mut filter = ThinkFilter::new();
    let mut output = String::new();

    output.push_str(&filter.process_token("<thi"));
    output.push_str(&filter.process_token("nk>Deep reasoning here</think>Final text"));

    assert_eq!(output, "Final text", "Split <think> opening must be correctly stripped");
}

#[test]
fn test_think_tag_split_closing_chunk() {
    // Closing tag split across chunk boundaries: "</th" then "ink>target"
    let mut filter = ThinkFilter::new();
    let mut output = String::new();

    output.push_str(&filter.process_token("<think>Thought content</th"));
    output.push_str(&filter.process_token("ink>Clean text"));

    assert_eq!(output, "Clean text", "Split </think> closing must be correctly stripped");
}

#[test]
fn test_unclosed_think_tag_recovery() {
    // Model ran out of tokens while inside <think> and never emitted </think>
    let mut filter = ThinkFilter::new();
    let mut output = String::new();

    output.push_str(&filter.process_token("<think>Unclosed thinking block..."));
    output.push_str(&filter.flush());

    // Should not panic, and output should safely discard reasoning
    assert_eq!(output, "");
}

#[test]
fn test_legitimate_xml_passthrough() {
    // Text contains legitimate HTML/XML that is NOT <think>
    let mut filter = ThinkFilter::new();
    let mut output = String::new();

    output.push_str(&filter.process_token("Include <stdio.h> and <iostream> in C++."));
    output.push_str(&filter.flush());

    assert_eq!(
        output,
        "Include <stdio.h> and <iostream> in C++.",
        "Non-think XML tags must be preserved verbatim without character loss"
    );
}

#[test]
fn test_nested_or_consecutive_think_tags() {
    let mut filter = ThinkFilter::new();
    let mut output = String::new();

    output.push_str(&filter.process_token("<think>Thought 1</think>Part 1 "));
    output.push_str(&filter.process_token("<think>Thought 2</think>Part 2"));

    assert_eq!(output, "Part 1 Part 2");
}
