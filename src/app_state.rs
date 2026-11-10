use gpui::{AppContext, Context, Entity};
use std::ops::Range;
use std::path::PathBuf;
use tokio::sync::mpsc::{unbounded_channel, UnboundedSender};

use crate::document::model::{Document, DocumentModel};
use crate::ui::{LayoutSettings, ReadingMode};

/// Commands sent from GPUI Main Thread to Tokio Background Workers
#[derive(Debug)]
pub enum AppCommand {
    LoadDocument {
        path: PathBuf,
    },
    ViewportChanged {
        chapter_id: String,
        visible_range: Range<usize>,
    },
    TranslateBlock {
        block_id: String,
        source_text: String,
    },
    RetryTranslation {
        block_id: String,
    },
    CancelPendingTranslations {
        chapter_id: String,
    },
    SaveReadingProgress {
        doc_id: String,
        chapter_idx: usize,
        block_idx: usize,
        scroll_offset: f32,
    },
}

/// Events sent from Tokio Background Workers to GPUI Main Thread
#[derive(Debug)]
pub enum BackgroundEvent {
    DocumentLoaded {
        doc: Box<Document>,
    },
    DocumentLoadFailed {
        error: String,
    },
    TranslationStreamingDelta {
        block_id: String,
        delta: String,
    },
    TranslationCompleted {
        block_id: String,
        translated_text: String,
        provider: String,
        model: String,
    },
    TranslationFailed {
        block_id: String,
        friendly_message: String,
        retryable: bool,
    },
    ReadingProgressRestored {
        chapter_idx: usize,
        block_idx: usize,
        scroll_offset: f32,
    },
}

pub struct AppState {
    pub tokio_handle: tokio::runtime::Handle,
    pub command_tx: UnboundedSender<AppCommand>,
    pub active_document: Option<Entity<DocumentModel>>,
    pub active_chapter_idx: usize,
    pub reading_mode: ReadingMode,
    pub layout_settings: LayoutSettings,
    pub current_locale: String,
    pub last_error: Option<String>,
    pub status_message: Option<String>,
}

#[derive(Clone)]
pub struct GlobalAppState(pub Entity<AppState>);

impl gpui::Global for GlobalAppState {}

impl std::ops::Deref for GlobalAppState {
    type Target = Entity<AppState>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl AppState {
    pub fn new(tokio_handle: tokio::runtime::Handle, cx: &mut Context<Self>) -> Self {
        let (cmd_tx, mut cmd_rx) = unbounded_channel::<AppCommand>();
        let (event_tx, mut event_rx) = unbounded_channel::<BackgroundEvent>();

        // 1. Spawn background command consumer in Tokio runtime
        let event_tx_clone = event_tx.clone();
        tokio_handle.spawn(async move {
            while let Some(cmd) = cmd_rx.recv().await {
                Self::process_background_command(cmd, &event_tx_clone).await;
            }
        });

        // 2. Spawn GPUI foreground listener task to receive background events
        cx.spawn(async move |this, cx| {
            while let Some(event) = event_rx.recv().await {
                if let Some(this) = this.upgrade() {
                    this.update(cx, |state, cx| {
                        state.handle_background_event(event, cx);
                    });
                } else {
                    break;
                }
            }
        })
        .detach(); // Detach to prevent task cancellation when Task is dropped

        Self {
            tokio_handle,
            command_tx: cmd_tx,
            active_document: None,
            active_chapter_idx: 0,
            reading_mode: ReadingMode::InlineMuted,
            layout_settings: LayoutSettings::default(),
            current_locale: "zh-CN".to_string(),
            last_error: None,
            status_message: None,
        }
    }

    pub fn handle_background_event(&mut self, event: BackgroundEvent, cx: &mut Context<Self>) {
        match event {
            BackgroundEvent::TranslationCompleted {
                block_id,
                translated_text,
                provider,
                model,
            } => {
                if let Some(doc_model) = &self.active_document {
                    doc_model.update(cx, |doc, cx| {
                        doc.set_block_translated(&block_id, translated_text, provider, model);
                        cx.notify();
                    });
                }
            }
            BackgroundEvent::TranslationFailed {
                block_id,
                friendly_message,
                retryable,
            } => {
                if let Some(doc_model) = &self.active_document {
                    doc_model.update(cx, |doc, cx| {
                        doc.set_block_failed(&block_id, friendly_message, retryable);
                        cx.notify();
                    });
                }
            }
            BackgroundEvent::DocumentLoaded { doc } => {
                self.last_error = None;
                self.status_message = Some(format!("Loaded: {}", doc.title));
                let doc_entity = cx.new(|_| DocumentModel::new(*doc));
                self.active_document = Some(doc_entity);
                cx.notify();
            }
            BackgroundEvent::DocumentLoadFailed { error } => {
                tracing::error!("Document load failed: {}", error);
                self.last_error = Some(error.clone());
                self.status_message = Some(format!("Document load failed: {}", error));
                self.active_document = None;
                cx.notify();
            }
            BackgroundEvent::ReadingProgressRestored {
                chapter_idx,
                block_idx: _,
                scroll_offset: _,
            } => {
                self.active_chapter_idx = chapter_idx;
                cx.notify();
            }
            BackgroundEvent::TranslationStreamingDelta { .. } => {
                tracing::trace!("Translation streaming delta received");
            }
        }
    }

    /// Clear current error status
    pub fn clear_error(&mut self, cx: &mut Context<Self>) {
        self.last_error = None;
        self.status_message = None;
        cx.notify();
    }

    /// Set an explicit error message and notify UI observers
    pub fn set_error(&mut self, error: impl Into<String>, cx: &mut Context<Self>) {
        let err_str = error.into();
        tracing::error!("AppState error set: {}", err_str);
        self.last_error = Some(err_str.clone());
        self.status_message = Some(err_str);
        cx.notify();
    }

    async fn process_background_command(
        cmd: AppCommand,
        _event_tx: &UnboundedSender<BackgroundEvent>,
    ) {
        tracing::debug!("Received background command: {:?}", cmd);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_background_event_debug_and_variants() {
        let load_fail = BackgroundEvent::DocumentLoadFailed {
            error: "Failed to open EPUB archive: invalid zip header".to_string(),
        };
        match &load_fail {
            BackgroundEvent::DocumentLoadFailed { error } => {
                assert!(error.contains("invalid zip header"));
            }
            _ => panic!("Expected DocumentLoadFailed variant"),
        }
        let debug_str = format!("{:?}", load_fail);
        assert!(debug_str.contains("DocumentLoadFailed"));
    }

    #[tokio::test]
    async fn test_background_event_channel_roundtrip() {
        let (tx, mut rx) = unbounded_channel::<BackgroundEvent>();
        tx.send(BackgroundEvent::DocumentLoadFailed {
            error: "IO error: File not found".to_string(),
        })
        .expect("Send must succeed");

        let event = rx.recv().await.expect("Receive must succeed");
        match event {
            BackgroundEvent::DocumentLoadFailed { error } => {
                assert_eq!(error, "IO error: File not found");
            }
            _ => panic!("Expected DocumentLoadFailed variant"),
        }
    }
}
