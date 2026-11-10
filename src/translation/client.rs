//! OpenAI-Compatible AI Translation Client
//!
//! Supports OpenAI, DeepSeek, Moonshot, SiliconFlow, Ollama, and custom endpoints.
//! Features SSE streaming, ThinkFilter reasoning strip, numbered batch validation,
//! and automatic single-segment fallback on mismatch.

use std::time::Instant;
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};

use crate::translation::batch::*;
use crate::translation::error::*;
use crate::translation::think_filter::ThinkFilter;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TranslationConfig {
    pub api_base: String,
    pub api_key: String,
    pub model: String,
    pub target_lang: String,
    pub source_lang: String,
    pub temperature: Option<f32>,
    pub custom_system_prompt: Option<String>,
}

impl Default for TranslationConfig {
    fn default() -> Self {
        Self {
            api_base: "https://api.openai.com/v1".to_string(),
            api_key: String::new(),
            model: "gpt-4o-mini".to_string(),
            target_lang: "zh-CN".to_string(),
            source_lang: "auto".to_string(),
            temperature: Some(0.3),
            custom_system_prompt: None,
        }
    }
}

pub struct OpenAiClient {
    client: reqwest::Client,
    config: TranslationConfig,
}

#[derive(Serialize)]
struct ChatMessage<'a> {
    role: &'a str,
    content: &'a str,
}

#[derive(Serialize)]
struct ChatCompletionRequest<'a> {
    model: &'a str,
    messages: Vec<ChatMessage<'a>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    temperature: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    stream: Option<bool>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: Option<ChatContent>,
    delta: Option<ChatContent>,
}

#[derive(Deserialize)]
struct ChatContent {
    content: Option<String>,
}

#[derive(Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

impl OpenAiClient {
    pub fn new(config: TranslationConfig) -> Self {
        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(45))
            .build()
            .unwrap_or_default();
        Self { client, config }
    }

    pub fn config(&self) -> &TranslationConfig {
        &self.config
    }

    /// Pings the endpoint with a test prompt, returning roundtrip latency in milliseconds.
    pub async fn test_connection(&self) -> Result<u64, UserFacingError> {
        let start = Instant::now();
        self.translate_single("Hello").await?;
        Ok(start.elapsed().as_millis() as u64)
    }

    /// Translates a single text segment.
    pub async fn translate_single(&self, text: &str) -> Result<String, UserFacingError> {
        let sys_prompt = self.config.custom_system_prompt.clone().unwrap_or_else(|| {
            format!(
                "You are an expert translator. Translate the text to {}. Return ONLY the direct translation, preserving all formatting and line breaks.",
                self.config.target_lang
            )
        });

        let endpoint = format!("{}/chat/completions", self.config.api_base.trim_end_matches('/'));
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        if !self.config.api_key.is_empty() {
            if let Ok(val) = HeaderValue::from_str(&format!("Bearer {}", self.config.api_key)) {
                headers.insert(AUTHORIZATION, val);
            }
        }

        let body = ChatCompletionRequest {
            model: &self.config.model,
            messages: vec![
                ChatMessage {
                    role: "system",
                    content: &sys_prompt,
                },
                ChatMessage {
                    role: "user",
                    content: text,
                },
            ],
            temperature: self.config.temperature,
            stream: Some(false),
        };

        let resp = self
            .client
            .post(&endpoint)
            .headers(headers)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                isolate_translation_error(&e.to_string(), e.status().map(|s| s.as_u16()), Some(&self.config.api_key), "zh").0
            })?;

        let status = resp.status();
        if !status.is_success() {
            let error_text = resp.text().await.unwrap_or_default();
            let (user_err, _) = isolate_translation_error(
                &format!("HTTP {status}: {error_text}"),
                Some(status.as_u16()),
                Some(&self.config.api_key),
                "zh",
            );
            return Err(user_err);
        }

        let chat_res: ChatCompletionResponse = resp.json().await.map_err(|e| {
            isolate_translation_error(&format!("JSON decode error: {e}"), Some(status.as_u16()), Some(&self.config.api_key), "zh").0
        })?;

        let raw_content = chat_res
            .choices
            .first()
            .and_then(|c| c.message.as_ref())
            .and_then(|m| m.content.as_deref())
            .unwrap_or_default();

        Ok(ThinkFilter::strip_static(raw_content).trim().to_string())
    }

    /// Translates an array of segments using the numbered JSON batch protocol,
    /// automatically falling back to individual segment requests if validation fails.
    pub async fn translate_batch(
        &self,
        segments: &[BatchSegmentInput],
    ) -> Result<Vec<(usize, String)>, UserFacingError> {
        if segments.is_empty() {
            return Ok(Vec::new());
        }

        let expected_ids: Vec<usize> = segments.iter().map(|s| s.id).collect();
        let sys_prompt = build_batch_system_prompt(&self.config.target_lang);
        let user_prompt = build_batch_user_prompt(segments);

        let endpoint = format!("{}/chat/completions", self.config.api_base.trim_end_matches('/'));
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        if !self.config.api_key.is_empty() {
            if let Ok(val) = HeaderValue::from_str(&format!("Bearer {}", self.config.api_key)) {
                headers.insert(AUTHORIZATION, val);
            }
        }

        let body = ChatCompletionRequest {
            model: &self.config.model,
            messages: vec![
                ChatMessage {
                    role: "system",
                    content: &sys_prompt,
                },
                ChatMessage {
                    role: "user",
                    content: &user_prompt,
                },
            ],
            temperature: self.config.temperature,
            stream: Some(false),
        };

        let resp_result = self.client.post(&endpoint).headers(headers).json(&body).send().await;

        let should_fallback = match resp_result {
            Ok(resp) => {
                if resp.status().is_success() {
                    if let Ok(chat_res) = resp.json::<ChatCompletionResponse>().await {
                        let raw_content = chat_res
                            .choices
                            .first()
                            .and_then(|c| c.message.as_ref())
                            .and_then(|m| m.content.as_deref())
                            .unwrap_or_default();

                        let clean_content = ThinkFilter::strip_static(raw_content);
                        match validate_batch_response(&expected_ids, &clean_content) {
                            BatchValidationResult::Success(pairs) => return Ok(pairs),
                            _ => true, // Fallback on mismatch or parse error
                        }
                    } else {
                        true
                    }
                } else {
                    false
                }
            }
            Err(_) => false,
        };

        if should_fallback {
            tracing::warn!("Batch validation failed, falling back to sequential single-segment translation");
            let mut results = Vec::new();
            for seg in segments {
                let trans = self.translate_single(&seg.text).await?;
                results.push((seg.id, trans));
            }
            return Ok(results);
        }

        let (user_err, _) = isolate_translation_error(
            "Batch translation service unreachable",
            None,
            Some(&self.config.api_key),
            "zh",
        );
        Err(user_err)
    }

    /// Translates text via SSE streaming, piping tokens through ThinkFilter.
    pub async fn translate_stream<F>(&self, text: &str, mut on_token: F) -> Result<(), UserFacingError>
    where
        F: FnMut(&str) + Send,
    {
        use futures::StreamExt;

        let sys_prompt = self.config.custom_system_prompt.clone().unwrap_or_else(|| {
            format!(
                "You are an expert translator. Translate the text to {}. Return ONLY the direct translation, preserving all formatting.",
                self.config.target_lang
            )
        });

        let endpoint = format!("{}/chat/completions", self.config.api_base.trim_end_matches('/'));
        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        if !self.config.api_key.is_empty() {
            if let Ok(val) = HeaderValue::from_str(&format!("Bearer {}", self.config.api_key)) {
                headers.insert(AUTHORIZATION, val);
            }
        }

        let body = ChatCompletionRequest {
            model: &self.config.model,
            messages: vec![
                ChatMessage {
                    role: "system",
                    content: &sys_prompt,
                },
                ChatMessage {
                    role: "user",
                    content: text,
                },
            ],
            temperature: self.config.temperature,
            stream: Some(true),
        };

        let resp = self
            .client
            .post(&endpoint)
            .headers(headers)
            .json(&body)
            .send()
            .await
            .map_err(|e| {
                isolate_translation_error(&e.to_string(), e.status().map(|s| s.as_u16()), Some(&self.config.api_key), "zh").0
            })?;

        let mut stream = resp.bytes_stream();
        let mut filter = ThinkFilter::new();
        let mut buffer = String::new();

        while let Some(chunk_res) = stream.next().await {
            let chunk = chunk_res.map_err(|e| {
                isolate_translation_error(&e.to_string(), None, Some(&self.config.api_key), "zh").0
            })?;

            if let Ok(text_chunk) = std::str::from_utf8(&chunk) {
                buffer.push_str(text_chunk);
                while let Some(line_end) = buffer.find('\n') {
                    let line = buffer[..line_end].trim().to_string();
                    buffer = buffer[line_end + 1..].to_string();

                    if line == "data: [DONE]" {
                        break;
                    }
                    if let Some(json_str) = line.strip_prefix("data: ") {
                        if let Ok(chat_res) = serde_json::from_str::<ChatCompletionResponse>(json_str) {
                            if let Some(delta) = chat_res.choices.first().and_then(|c| c.delta.as_ref()) {
                                if let Some(content) = &delta.content {
                                    let clean = filter.process_token(content);
                                    if !clean.is_empty() {
                                        on_token(&clean);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        let remaining = filter.flush();
        if !remaining.is_empty() {
            on_token(&remaining);
        }

        Ok(())
    }
}
