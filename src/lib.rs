pub mod app_state;
pub mod document;
pub mod export;
pub mod logging;
pub mod storage;
pub mod translation;
pub mod ui;

pub use app_state::{AppCommand, AppState, BackgroundEvent, GlobalAppState};
pub use document::*;
pub use export::{ExportConfig, ExportContentMode, ExportFormat, Exporter, PresentationStyle};
pub use logging::{init as init_logging, mask_secret, redact_sensitive_str};
pub use storage::db::{
    Bookmark, Database, DbError, GlossaryTerm, ReadingProgress, TranslationRecord,
};
pub use storage::keyring::{CredentialStore, KeyringError};
pub use translation::{
    build_batch_system_prompt, build_batch_user_prompt, isolate_translation_error,
    strip_markdown_fences, validate_batch_response, BatchSegmentInput, BatchSegmentOutput,
    BatchTranslationResponse, BatchValidationResult, InternalLogEntry, OpenAiClient, ThinkFilter,
    TranslationConfig, TranslationService, UserFacingError, UI_ERROR_MSG_EN, UI_ERROR_MSG_ZH,
};
pub use ui::i18n::{current_locale, set_locale, t, t_locale, Locale};
pub use ui::main_window::MainWindow;
pub use ui::theme::{LayoutSettings, ReadingMode, ReadingWidth};
