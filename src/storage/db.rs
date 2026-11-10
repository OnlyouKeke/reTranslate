use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum DbError {
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("Pool initialization error: {0}")]
    Pool(String),
    #[error("Connection acquisition error: {0}")]
    Connection(String),
    #[error("Task join error: {0}")]
    TaskJoin(String),
    #[error("Path resolution error: {0}")]
    Path(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TranslationRecord {
    pub source_hash: String,
    pub provider: String,
    pub model: String,
    pub source_lang: String,
    pub target_lang: String,
    pub source_text: String,
    pub translated_text: String,
    pub created_at: i64,
    pub last_used_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ReadingProgress {
    pub document_id: String,
    pub file_path: String,
    pub chapter_index: usize,
    pub block_index: usize,
    pub scroll_offset_px: f32,
    pub last_read_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Bookmark {
    pub id: i64,
    pub document_id: String,
    pub chapter_index: usize,
    pub block_index: usize,
    pub source_snippet: String,
    pub translated_snippet: Option<String>,
    pub note: Option<String>,
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct GlossaryTerm {
    pub id: i64,
    pub source_term: String,
    pub target_term: String,
    pub domain: String,
    pub case_sensitive: bool,
    pub created_at: i64,
}

#[derive(Clone)]
pub struct Database {
    pool: Pool<SqliteConnectionManager>,
}

impl Database {
    /// Open or create SQLite database with WAL mode and run migrations
    pub fn open<P: AsRef<Path>>(path: P) -> Result<Self, DbError> {
        let path_ref = path.as_ref();
        if let Some(parent) = path_ref.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let manager = SqliteConnectionManager::file(path_ref).with_init(|c| {
            c.execute_batch(
                "PRAGMA journal_mode = WAL;
                 PRAGMA synchronous = NORMAL;
                 PRAGMA busy_timeout = 5000;
                 PRAGMA foreign_keys = ON;
                 PRAGMA temp_store = MEMORY;",
            )
        });
        let pool = Pool::builder()
            .max_size(16)
            .build(manager)
            .map_err(|e| DbError::Pool(e.to_string()))?;

        let db = Self { pool };
        db.migrate()?;
        Ok(db)
    }

    /// Open an in-memory database instance (primarily for testing and memory fallback)
    pub fn open_in_memory() -> Result<Self, DbError> {
        // Use a shared memory connection string so all pool connections access the same memory DB
        let manager = SqliteConnectionManager::file("file:memdb_retranslate?mode=memory&cache=shared")
            .with_init(|c| {
                c.execute_batch(
                    "PRAGMA busy_timeout = 5000;
                     PRAGMA foreign_keys = ON;",
                )
            });
        let pool = Pool::builder()
            .max_size(4)
            .build(manager)
            .map_err(|e| DbError::Pool(e.to_string()))?;

        let db = Self { pool };
        db.migrate()?;
        Ok(db)
    }

    /// Default database file path in user data directory
    pub fn default_path() -> Result<PathBuf, DbError> {
        let base = dirs::data_dir()
            .or_else(dirs::home_dir)
            .ok_or_else(|| DbError::Path("Failed to resolve app data directory".into()))?;
        Ok(base.join("reTranslate").join("retranslate.db"))
    }

    /// Run idempotent migrations
    pub fn migrate(&self) -> Result<(), DbError> {
        let conn = self.pool.get().map_err(|e| DbError::Connection(e.to_string()))?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS schema_migrations (
                version INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                applied_at INTEGER NOT NULL
            );

            CREATE TABLE IF NOT EXISTS translations (
                source_hash TEXT NOT NULL,
                provider TEXT NOT NULL,
                model TEXT NOT NULL,
                source_lang TEXT NOT NULL,
                target_lang TEXT NOT NULL,
                source_text TEXT NOT NULL,
                translated_text TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                last_used_at INTEGER NOT NULL,
                PRIMARY KEY (source_hash, provider, model, target_lang)
            );
            CREATE INDEX IF NOT EXISTS idx_translations_last_used ON translations (last_used_at);

            CREATE TABLE IF NOT EXISTS reading_progress (
                document_id TEXT PRIMARY KEY,
                file_path TEXT NOT NULL,
                chapter_index INTEGER NOT NULL,
                block_index INTEGER NOT NULL,
                scroll_offset_px REAL NOT NULL,
                last_read_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_reading_progress_recent ON reading_progress (last_read_at DESC);

            CREATE TABLE IF NOT EXISTS bookmarks (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                document_id TEXT NOT NULL,
                chapter_index INTEGER NOT NULL,
                block_index INTEGER NOT NULL,
                source_snippet TEXT NOT NULL,
                translated_snippet TEXT,
                note TEXT,
                created_at INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_bookmarks_doc ON bookmarks (document_id, chapter_index);

            CREATE TABLE IF NOT EXISTS glossary (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                source_term TEXT NOT NULL,
                target_term TEXT NOT NULL,
                domain TEXT NOT NULL DEFAULT 'general',
                case_sensitive INTEGER NOT NULL DEFAULT 0,
                created_at INTEGER NOT NULL,
                UNIQUE (source_term, domain)
            );
            CREATE INDEX IF NOT EXISTS idx_glossary_lookup ON glossary (domain, source_term);

            CREATE TABLE IF NOT EXISTS app_preferences (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );
            "#,
        )?;
        Ok(())
    }

    // =========================================================================
    // Translation Cache CRUD
    // =========================================================================

    pub async fn get_cached_translation(
        &self,
        source_hash: String,
        provider: String,
        model: String,
        target_lang: String,
    ) -> Result<Option<String>, DbError> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| DbError::Connection(e.to_string()))?;
            let mut stmt = conn.prepare(
                "SELECT translated_text FROM translations WHERE source_hash = ?1 AND provider = ?2 AND model = ?3 AND target_lang = ?4",
            )?;
            let mut rows = stmt.query(params![source_hash, provider, model, target_lang])?;
            if let Some(row) = rows.next()? {
                let trans: String = row.get(0)?;
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;
                let _ = conn.execute(
                    "UPDATE translations SET last_used_at = ?1 WHERE source_hash = ?2 AND provider = ?3 AND model = ?4 AND target_lang = ?5",
                    params![now, source_hash, provider, model, target_lang],
                );
                Ok(Some(trans))
            } else {
                Ok(None)
            }
        })
        .await
        .map_err(|e| DbError::TaskJoin(e.to_string()))?
    }

    pub async fn store_translation(&self, record: TranslationRecord) -> Result<(), DbError> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| DbError::Connection(e.to_string()))?;
            conn.execute(
                r#"
                INSERT INTO translations 
                (source_hash, provider, model, source_lang, target_lang, source_text, translated_text, created_at, last_used_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
                ON CONFLICT(source_hash, provider, model, target_lang) DO UPDATE SET
                    translated_text = excluded.translated_text,
                    last_used_at = excluded.last_used_at
                "#,
                params![
                    record.source_hash,
                    record.provider,
                    record.model,
                    record.source_lang,
                    record.target_lang,
                    record.source_text,
                    record.translated_text,
                    record.created_at,
                    record.last_used_at,
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| DbError::TaskJoin(e.to_string()))?
    }

    // =========================================================================
    // Reading Progress CRUD
    // =========================================================================

    pub async fn get_reading_progress(&self, document_id: String) -> Result<Option<ReadingProgress>, DbError> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| DbError::Connection(e.to_string()))?;
            let mut stmt = conn.prepare(
                "SELECT document_id, file_path, chapter_index, block_index, scroll_offset_px, last_read_at FROM reading_progress WHERE document_id = ?1",
            )?;
            let mut rows = stmt.query(params![document_id])?;
            if let Some(row) = rows.next()? {
                Ok(Some(ReadingProgress {
                    document_id: row.get(0)?,
                    file_path: row.get(1)?,
                    chapter_index: row.get::<_, i64>(2)? as usize,
                    block_index: row.get::<_, i64>(3)? as usize,
                    scroll_offset_px: row.get::<_, f64>(4)? as f32,
                    last_read_at: row.get(5)?,
                }))
            } else {
                Ok(None)
            }
        })
        .await
        .map_err(|e| DbError::TaskJoin(e.to_string()))?
    }

    pub async fn save_reading_progress(&self, progress: ReadingProgress) -> Result<(), DbError> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| DbError::Connection(e.to_string()))?;
            conn.execute(
                r#"
                INSERT INTO reading_progress (document_id, file_path, chapter_index, block_index, scroll_offset_px, last_read_at)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ON CONFLICT(document_id) DO UPDATE SET
                    file_path = excluded.file_path,
                    chapter_index = excluded.chapter_index,
                    block_index = excluded.block_index,
                    scroll_offset_px = excluded.scroll_offset_px,
                    last_read_at = excluded.last_read_at
                "#,
                params![
                    progress.document_id,
                    progress.file_path,
                    progress.chapter_index as i64,
                    progress.block_index as i64,
                    progress.scroll_offset_px as f64,
                    progress.last_read_at,
                ],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| DbError::TaskJoin(e.to_string()))?
    }

    // =========================================================================
    // Bookmarks CRUD
    // =========================================================================

    pub async fn add_bookmark(
        &self,
        document_id: String,
        chapter_index: usize,
        block_index: usize,
        source_snippet: String,
        translated_snippet: Option<String>,
        note: Option<String>,
    ) -> Result<i64, DbError> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| DbError::Connection(e.to_string()))?;
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64;
            conn.execute(
                "INSERT INTO bookmarks (document_id, chapter_index, block_index, source_snippet, translated_snippet, note, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![
                    document_id,
                    chapter_index as i64,
                    block_index as i64,
                    source_snippet,
                    translated_snippet,
                    note,
                    now
                ],
            )?;
            Ok(conn.last_insert_rowid())
        })
        .await
        .map_err(|e| DbError::TaskJoin(e.to_string()))?
    }

    pub async fn list_bookmarks(&self, document_id: String) -> Result<Vec<Bookmark>, DbError> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| DbError::Connection(e.to_string()))?;
            let mut stmt = conn.prepare(
                "SELECT id, document_id, chapter_index, block_index, source_snippet, translated_snippet, note, created_at
                 FROM bookmarks WHERE document_id = ?1 ORDER BY chapter_index ASC, block_index ASC",
            )?;
            let rows = stmt.query_map(params![document_id], |row| {
                Ok(Bookmark {
                    id: row.get(0)?,
                    document_id: row.get(1)?,
                    chapter_index: row.get::<_, i64>(2)? as usize,
                    block_index: row.get::<_, i64>(3)? as usize,
                    source_snippet: row.get(4)?,
                    translated_snippet: row.get(5)?,
                    note: row.get(6)?,
                    created_at: row.get(7)?,
                })
            })?;
            let mut list = Vec::new();
            for b in rows {
                list.push(b?);
            }
            Ok(list)
        })
        .await
        .map_err(|e| DbError::TaskJoin(e.to_string()))?
    }

    pub async fn delete_bookmark(&self, id: i64) -> Result<(), DbError> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| DbError::Connection(e.to_string()))?;
            conn.execute("DELETE FROM bookmarks WHERE id = ?1", params![id])?;
            Ok(())
        })
        .await
        .map_err(|e| DbError::TaskJoin(e.to_string()))?
    }

    // =========================================================================
    // Glossary CRUD
    // =========================================================================

    pub async fn add_glossary_term(&self, term: GlossaryTerm) -> Result<i64, DbError> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| DbError::Connection(e.to_string()))?;
            conn.execute(
                "INSERT INTO glossary (source_term, target_term, domain, case_sensitive, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(source_term, domain) DO UPDATE SET
                    target_term = excluded.target_term,
                    case_sensitive = excluded.case_sensitive",
                params![
                    term.source_term,
                    term.target_term,
                    term.domain,
                    if term.case_sensitive { 1 } else { 0 },
                    term.created_at
                ],
            )?;
            Ok(conn.last_insert_rowid())
        })
        .await
        .map_err(|e| DbError::TaskJoin(e.to_string()))?
    }

    pub async fn list_glossary(&self, domain: Option<String>) -> Result<Vec<GlossaryTerm>, DbError> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| DbError::Connection(e.to_string()))?;
            let sql = if domain.is_some() {
                "SELECT id, source_term, target_term, domain, case_sensitive, created_at FROM glossary WHERE domain = ?1 ORDER BY source_term ASC"
            } else {
                "SELECT id, source_term, target_term, domain, case_sensitive, created_at FROM glossary ORDER BY domain ASC, source_term ASC"
            };
            let mut stmt = conn.prepare(sql)?;
            let rows = if let Some(ref d) = domain {
                stmt.query_map(params![d], Self::map_glossary_row)?
            } else {
                stmt.query_map([], Self::map_glossary_row)?
            };
            let mut list = Vec::new();
            for t in rows {
                list.push(t?);
            }
            Ok(list)
        })
        .await
        .map_err(|e| DbError::TaskJoin(e.to_string()))?
    }

    pub async fn lookup_glossary_term(&self, source: String, domain: String) -> Result<Option<String>, DbError> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| DbError::Connection(e.to_string()))?;
            let mut stmt = conn.prepare(
                "SELECT target_term FROM glossary WHERE source_term = ?1 AND domain = ?2",
            )?;
            let mut rows = stmt.query(params![source, domain])?;
            if let Some(row) = rows.next()? {
                Ok(Some(row.get(0)?))
            } else {
                Ok(None)
            }
        })
        .await
        .map_err(|e| DbError::TaskJoin(e.to_string()))?
    }

    fn map_glossary_row(row: &rusqlite::Row) -> Result<GlossaryTerm, rusqlite::Error> {
        Ok(GlossaryTerm {
            id: row.get(0)?,
            source_term: row.get(1)?,
            target_term: row.get(2)?,
            domain: row.get(3)?,
            case_sensitive: row.get::<_, i64>(4)? != 0,
            created_at: row.get(5)?,
        })
    }

    // =========================================================================
    // App Preferences CRUD
    // =========================================================================

    pub async fn get_preference(&self, key: String) -> Result<Option<String>, DbError> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| DbError::Connection(e.to_string()))?;
            let mut stmt = conn.prepare("SELECT value FROM app_preferences WHERE key = ?1")?;
            let mut rows = stmt.query(params![key])?;
            if let Some(row) = rows.next()? {
                Ok(Some(row.get(0)?))
            } else {
                Ok(None)
            }
        })
        .await
        .map_err(|e| DbError::TaskJoin(e.to_string()))?
    }

    pub async fn set_preference(&self, key: String, value: String) -> Result<(), DbError> {
        let pool = self.pool.clone();
        tokio::task::spawn_blocking(move || {
            let conn = pool.get().map_err(|e| DbError::Connection(e.to_string()))?;
            conn.execute(
                "INSERT INTO app_preferences (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )?;
            Ok(())
        })
        .await
        .map_err(|e| DbError::TaskJoin(e.to_string()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_db_in_memory_migrations_and_crud() {
        let db = Database::open_in_memory().expect("Failed to open in-memory db");

        // 1. Translations CRUD
        let record = TranslationRecord {
            source_hash: "hash_test_1".into(),
            provider: "deepseek".into(),
            model: "deepseek-chat".into(),
            source_lang: "en".into(),
            target_lang: "zh-CN".into(),
            source_text: "Hello".into(),
            translated_text: "你好".into(),
            created_at: 1000,
            last_used_at: 1000,
        };
        db.store_translation(record).await.unwrap();

        let cached = db
            .get_cached_translation(
                "hash_test_1".into(),
                "deepseek".into(),
                "deepseek-chat".into(),
                "zh-CN".into(),
            )
            .await
            .unwrap();
        assert_eq!(cached, Some("你好".to_string()));

        // Model isolation check
        let cached_diff_model = db
            .get_cached_translation(
                "hash_test_1".into(),
                "deepseek".into(),
                "deepseek-reasoner".into(),
                "zh-CN".into(),
            )
            .await
            .unwrap();
        assert_eq!(cached_diff_model, None);

        // 2. Reading Progress CRUD
        let progress = ReadingProgress {
            document_id: "doc_123".into(),
            file_path: "/path/to/doc.epub".into(),
            chapter_index: 2,
            block_index: 5,
            scroll_offset_px: 120.5,
            last_read_at: 2000,
        };
        db.save_reading_progress(progress.clone()).await.unwrap();

        let loaded_progress = db.get_reading_progress("doc_123".into()).await.unwrap().unwrap();
        assert_eq!(loaded_progress.document_id, "doc_123");
        assert_eq!(loaded_progress.chapter_index, 2);
        assert_eq!(loaded_progress.block_index, 5);
        assert!((loaded_progress.scroll_offset_px - 120.5).abs() < 0.01);

        // 3. Bookmarks CRUD
        let bookmark_id = db
            .add_bookmark(
                "doc_123".into(),
                2,
                5,
                "Snippet source".into(),
                Some("Snippet translation".into()),
                Some("Important note".into()),
            )
            .await
            .unwrap();
        assert!(bookmark_id > 0);

        let bookmarks = db.list_bookmarks("doc_123".into()).await.unwrap();
        assert_eq!(bookmarks.len(), 1);
        assert_eq!(bookmarks[0].source_snippet, "Snippet source");

        db.delete_bookmark(bookmark_id).await.unwrap();
        let bookmarks_after = db.list_bookmarks("doc_123".into()).await.unwrap();
        assert!(bookmarks_after.is_empty());

        // 4. Glossary CRUD
        let term = GlossaryTerm {
            id: 0,
            source_term: "LLM".into(),
            target_term: "大语言模型".into(),
            domain: "ai".into(),
            case_sensitive: true,
            created_at: 3000,
        };
        db.add_glossary_term(term).await.unwrap();

        let lookup = db.lookup_glossary_term("LLM".into(), "ai".into()).await.unwrap();
        assert_eq!(lookup, Some("大语言模型".to_string()));

        // 5. App Preferences
        db.set_preference("theme".into(), "dark".into()).await.unwrap();
        let theme_pref = db.get_preference("theme".into()).await.unwrap();
        assert_eq!(theme_pref, Some("dark".to_string()));
    }
}
