//! AI Translation Subsystem
//!
//! Provides OpenAI-compatible client, streaming SSE with ThinkFilter,
//! numbered JSON batch translation, SQLite-backed cache, and strict error isolation.

pub mod batch;
pub mod client;
pub mod error;
pub mod think_filter;

use std::sync::Arc;
use sha2::{Digest, Sha256};

pub use batch::{
    build_batch_system_prompt, build_batch_user_prompt, strip_markdown_fences,
    validate_batch_response, BatchSegmentInput, BatchSegmentOutput, BatchTranslationResponse,
    BatchValidationResult,
};
pub use client::{OpenAiClient, TranslationConfig};
pub use error::{
    isolate_translation_error, InternalLogEntry, UserFacingError, UI_ERROR_MSG_EN, UI_ERROR_MSG_ZH,
};
pub use think_filter::ThinkFilter;

use crate::storage::db::{Database, TranslationRecord};

pub struct TranslationService {
    client: Arc<OpenAiClient>,
    db: Arc<Database>,
}

impl TranslationService {
    pub fn new(client: Arc<OpenAiClient>, db: Arc<Database>) -> Self {
        Self { client, db }
    }

    pub fn client(&self) -> &Arc<OpenAiClient> {
        &self.client
    }

    /// Computes the unique deterministic cache key for a given block and configuration
    pub fn compute_cache_hash(text: &str, target_lang: &str, model: &str, base_url: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(text.trim().as_bytes());
        hasher.update(b"::");
        hasher.update(target_lang.as_bytes());
        hasher.update(b"::");
        hasher.update(model.as_bytes());
        hasher.update(b"::");
        hasher.update(base_url.as_bytes());
        let result = hasher.finalize();
        result.iter().map(|b| format!("{:02x}", b)).collect()
    }

    /// Translates text, checking the persistent SQLite cache first.
    pub async fn translate_with_cache(
        &self,
        _doc_id: &str,
        _chapter_id: &str,
        _block_id: &str,
        source_text: &str,
    ) -> Result<String, UserFacingError> {
        let config = self.client.config();
        let hash = Self::compute_cache_hash(
            source_text,
            &config.target_lang,
            &config.model,
            &config.api_base,
        );

        // 1. Check SQLite Cache
        let provider = "openai".to_string();
        if let Ok(Some(cached_text)) = self
            .db
            .get_cached_translation(
                hash.clone(),
                provider.clone(),
                config.model.clone(),
                config.target_lang.clone(),
            )
            .await
        {
            return Ok(cached_text);
        }

        // 2. Query LLM
        let translated = self.client.translate_single(source_text).await?;

        // 3. Save to SQLite Cache
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64;

        let record = TranslationRecord {
            source_hash: hash,
            provider,
            model: config.model.clone(),
            source_lang: config.source_lang.clone(),
            target_lang: config.target_lang.clone(),
            source_text: source_text.to_string(),
            translated_text: translated.clone(),
            created_at: now,
            last_used_at: now,
        };

        let _ = self.db.store_translation(record).await;

        Ok(translated)
    }
}
