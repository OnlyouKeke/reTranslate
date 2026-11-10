//! Streaming & Static Think-Tag Filter for Reasoning Models (e.g., DeepSeek-R1, QwQ)
//!
//! Strips `<think>...</think>` reasoning contents from streamed tokens or completed responses.

#[derive(Debug, Clone, PartialEq, Eq)]
enum ThinkState {
    Passthrough,
    TagOpening(String), // Accumulating '<', '<t', '<th', '<thi', '<thin', '<think>'
    InsideThink,
    TagClosing(String), // Accumulating '</', '</t', '</th', '</thi', '</thin', '</think>'
}

#[derive(Debug, Clone)]
pub struct ThinkFilter {
    state: ThinkState,
}

impl Default for ThinkFilter {
    fn default() -> Self {
        Self::new()
    }
}

impl ThinkFilter {
    pub fn new() -> Self {
        Self {
            state: ThinkState::Passthrough,
        }
    }

    /// Feeds an incoming token chunk and returns the sanitized text with `<think>` blocks stripped.
    pub fn process_token(&mut self, token: &str) -> String {
        let mut output = String::new();
        for ch in token.chars() {
            match &mut self.state {
                ThinkState::Passthrough => {
                    if ch == '<' {
                        self.state = ThinkState::TagOpening("<".to_string());
                    } else {
                        output.push(ch);
                    }
                }
                ThinkState::TagOpening(buf) => {
                    buf.push(ch);
                    let target = "<think>";
                    if target.starts_with(buf.as_str()) {
                        if buf.as_str() == target {
                            self.state = ThinkState::InsideThink;
                        }
                    } else {
                        // Deviated from <think>, e.g. <table> or <stdio.h>
                        output.push_str(buf);
                        self.state = ThinkState::Passthrough;
                    }
                }
                ThinkState::InsideThink => {
                    if ch == '<' {
                        self.state = ThinkState::TagClosing("<".to_string());
                    }
                    // Characters inside think are discarded
                }
                ThinkState::TagClosing(buf) => {
                    buf.push(ch);
                    let target = "</think>";
                    if target.starts_with(buf.as_str()) {
                        if buf.as_str() == target {
                            self.state = ThinkState::Passthrough;
                        }
                    } else {
                        // Deviated from </think>, still inside think
                        self.state = ThinkState::InsideThink;
                    }
                }
            }
        }
        output
    }

    /// Flushes any pending buffer at stream termination.
    pub fn flush(&mut self) -> String {
        let mut output = String::new();
        if let ThinkState::TagOpening(ref buf) = self.state {
            output.push_str(buf);
            self.state = ThinkState::Passthrough;
        }
        output
    }

    /// Utility to strip `<think>...</think>` from a complete static string.
    pub fn strip_static(text: &str) -> String {
        let mut filter = Self::new();
        let mut result = filter.process_token(text);
        result.push_str(&filter.flush());
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_static() {
        let raw = "<think>Let me reason about this.</think>Hello world!";
        assert_eq!(ThinkFilter::strip_static(raw), "Hello world!");
    }

    #[test]
    fn test_filter_split_chunks() {
        let mut filter = ThinkFilter::new();
        assert_eq!(filter.process_token("<th"), "");
        assert_eq!(filter.process_token("ink>Hidden reasoning</th"), "");
        assert_eq!(filter.process_token("ink>Revealed translation"), "Revealed translation");
    }

    #[test]
    fn test_filter_preserve_regular_tags() {
        let mut filter = ThinkFilter::new();
        assert_eq!(filter.process_token("<div>Table content</div>"), "<div>Table content</div>");
    }
}
