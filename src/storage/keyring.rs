use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};
use thiserror::Error;

pub const APP_SERVICE_NAME: &str = "retranslate";

static TEMP_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

#[derive(Error, Debug)]
pub enum KeyringError {
    #[error("Keyring error: {0}")]
    Keyring(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Path resolution error: {0}")]
    Path(String),
}

#[derive(Clone)]
pub struct CredentialStore {
    service: String,
    fallback_file: PathBuf,
    lock: Arc<Mutex<()>>,
}

impl CredentialStore {
    /// Create credential store using default OS config path
    pub fn new() -> Result<Self, KeyringError> {
        let base = dirs::config_dir()
            .or_else(dirs::home_dir)
            .ok_or_else(|| KeyringError::Path("Failed to resolve user config directory".into()))?;
        let base_dir = base.join("reTranslate");
        fs::create_dir_all(&base_dir)?;
        let fallback_file = base_dir.join("credentials.json");

        Ok(Self {
            service: APP_SERVICE_NAME.to_string(),
            fallback_file,
            lock: Arc::new(Mutex::new(())),
        })
    }

    /// Create credential store with custom fallback file (for tests or custom configuration)
    pub fn with_fallback_file(fallback_file: PathBuf) -> Self {
        let dir_name = fallback_file
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| "test".to_string());
        let service = format!("retrans_{}_{}", dir_name, std::process::id());
        Self::with_service_and_fallback(service, fallback_file)
    }

    /// Create credential store with custom service name and fallback file (for isolated tests)
    pub fn with_service_and_fallback(service: impl Into<String>, fallback_file: PathBuf) -> Self {
        Self {
            service: service.into(),
            fallback_file,
            lock: Arc::new(Mutex::new(())),
        }
    }

    /// Retrieve API key for a provider
    pub fn get_key(&self, provider: &str) -> Result<Option<String>, KeyringError> {
        let key_name = format!("api_key:{}", provider);

        #[cfg(feature = "keyring")]
        {
            if let Ok(entry) = keyring::Entry::new(&self.service, &key_name) {
                if let Ok(pwd) = entry.get_password() {
                    return Ok(Some(pwd));
                }
            }
        }

        self.get_from_fallback(&key_name)
    }

    /// Store API key for a provider
    pub fn set_key(&self, provider: &str, secret: &str) -> Result<(), KeyringError> {
        let key_name = format!("api_key:{}", provider);

        #[cfg(feature = "keyring")]
        {
            if let Ok(entry) = keyring::Entry::new(&self.service, &key_name) {
                let _ = entry.set_password(secret);
            }
        }

        // Always update fallback file to ensure durability across headless CI & restarts
        self.save_to_fallback(&key_name, secret)
    }

    /// Delete API key for a provider
    pub fn delete_key(&self, provider: &str) -> Result<(), KeyringError> {
        let key_name = format!("api_key:{}", provider);

        #[cfg(feature = "keyring")]
        {
            if let Ok(entry) = keyring::Entry::new(&self.service, &key_name) {
                let _ = entry.delete_credential();
            }
        }

        self.delete_from_fallback(&key_name)
    }

    // =========================================================================
    // Restricted Permissions Fallback Helpers
    // =========================================================================

    /// Generate unique temporary file name in the same parent directory to avoid collisions
    fn unique_temp_file(&self) -> PathBuf {
        let pid = std::process::id();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_nanos();
        let counter = TEMP_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
        let tmp_name = format!("credentials_{}_{}_{}.tmp", pid, nanos, counter);

        if let Some(parent) = self.fallback_file.parent() {
            if !parent.as_os_str().is_empty() {
                return parent.join(tmp_name);
            }
        }
        PathBuf::from(tmp_name)
    }

    /// Read credentials map from fallback file, self-healing from non-UTF8 corruption or missing file
    fn read_fallback_map(&self) -> HashMap<String, String> {
        if !self.fallback_file.exists() {
            return HashMap::new();
        }

        match fs::read_to_string(&self.fallback_file) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => {
                // Non-UTF8 binary corruption or unreadable file: fall back gracefully to empty map
                HashMap::new()
            }
        }
    }

    /// Atomically write credentials map to disk with 0600 permissions
    fn write_fallback_atomic(&self, map: &HashMap<String, String>) -> Result<(), KeyringError> {
        if let Some(parent) = self.fallback_file.parent() {
            if !parent.as_os_str().is_empty() {
                fs::create_dir_all(parent)?;
            }
        }

        let json_str = serde_json::to_string_pretty(map)?;
        let tmp_file = self.unique_temp_file();

        let write_op = (|| -> Result<(), KeyringError> {
            let mut options = OpenOptions::new();
            options.create(true).write(true).truncate(true);

            #[cfg(unix)]
            {
                use std::os::unix::fs::OpenOptionsExt;
                options.mode(0o600);
            }

            let mut file = options.open(&tmp_file)?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&tmp_file, fs::Permissions::from_mode(0o600));
            }

            file.write_all(json_str.as_bytes())?;
            file.flush()?;
            let _ = file.sync_all();
            drop(file);

            fs::rename(&tmp_file, &self.fallback_file)?;

            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = fs::set_permissions(&self.fallback_file, fs::Permissions::from_mode(0o600));
            }

            Ok(())
        })();

        if write_op.is_err() {
            let _ = fs::remove_file(&tmp_file);
        }

        write_op
    }

    fn get_from_fallback(&self, key: &str) -> Result<Option<String>, KeyringError> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let map = self.read_fallback_map();
        Ok(map.get(key).cloned())
    }

    fn save_to_fallback(&self, key: &str, secret: &str) -> Result<(), KeyringError> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        let mut map = self.read_fallback_map();
        map.insert(key.to_string(), secret.to_string());
        self.write_fallback_atomic(&map)
    }

    fn delete_from_fallback(&self, key: &str) -> Result<(), KeyringError> {
        let _guard = self.lock.lock().unwrap_or_else(|e| e.into_inner());
        if !self.fallback_file.exists() {
            return Ok(());
        }
        let mut map = self.read_fallback_map();
        if map.remove(key).is_some() {
            self.write_fallback_atomic(&map)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_credential_store_fallback_file() {
        let temp_dir = std::env::temp_dir().join(format!("retranslate_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        let fallback_file = temp_dir.join("credentials.json");

        let service = format!("retrans_test_fallback_{}", std::process::id());
        let store = CredentialStore::with_service_and_fallback(service, fallback_file.clone());
        let _ = store.delete_key("openai");

        // 1. Initially missing
        assert_eq!(store.get_key("openai").unwrap(), None);

        // 2. Set key
        store.set_key("openai", "sk-test-secret-123").unwrap();
        assert_eq!(store.get_key("openai").unwrap(), Some("sk-test-secret-123".into()));

        // Check file exists and permissions on Unix
        assert!(fallback_file.exists());
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = fs::metadata(&fallback_file).unwrap();
            let mode = meta.permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "Fallback file must have 0600 permissions");
        }

        // 3. Update key
        store.set_key("openai", "sk-test-updated-456").unwrap();
        assert_eq!(store.get_key("openai").unwrap(), Some("sk-test-updated-456".into()));

        // 4. Delete key
        store.delete_key("openai").unwrap();
        assert_eq!(store.get_key("openai").unwrap(), None);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_credential_store_non_utf8_binary_corruption_recovery() {
        let temp_dir = std::env::temp_dir().join(format!("retranslate_nonutf8_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::create_dir_all(&temp_dir);
        let fallback_file = temp_dir.join("credentials.json");

        // Write raw binary invalid UTF-8 bytes
        fs::write(&fallback_file, [0xFF, 0xFE, 0xFD, 0x80, 0x00, 0x01]).unwrap();

        let service = format!("retrans_test_nonutf8_{}", std::process::id());
        let store = CredentialStore::with_service_and_fallback(service, fallback_file.clone());
        let _ = store.delete_key("openai");

        // 1. get_key should recover gracefully and return Ok(None)
        assert_eq!(store.get_key("openai").unwrap(), None);

        // 2. set_key should self-heal and overwrite corrupted file with valid JSON
        store.set_key("openai", "sk-recovered-key").unwrap();
        assert_eq!(store.get_key("openai").unwrap(), Some("sk-recovered-key".into()));

        // 3. Verify file on disk is valid JSON
        let content = fs::read_to_string(&fallback_file).unwrap();
        let val: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(val["api_key:openai"], "sk-recovered-key");

        let _ = store.delete_key("openai");
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_credential_store_concurrent_writes() {
        let temp_dir = std::env::temp_dir().join(format!("retranslate_concurrent_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::create_dir_all(&temp_dir);
        let fallback_file = temp_dir.join("credentials.json");

        let service = format!("retrans_test_concurrent_{}", std::process::id());
        let store = CredentialStore::with_service_and_fallback(service, fallback_file.clone());
        let mut handles = Vec::new();

        for i in 0..20 {
            let store_clone = store.clone();
            handles.push(std::thread::spawn(move || {
                let provider = format!("provider_{}", i);
                let secret = format!("sk-secret-{}", i);
                store_clone.set_key(&provider, &secret).unwrap();
            }));
        }

        for h in handles {
            h.join().unwrap();
        }

        // All 20 keys should be present in the store
        for i in 0..20 {
            let provider = format!("provider_{}", i);
            let secret = format!("sk-secret-{}", i);
            assert_eq!(store.get_key(&provider).unwrap(), Some(secret));
        }

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = fs::metadata(&fallback_file).unwrap();
            let mode = meta.permissions().mode() & 0o777;
            assert_eq!(mode, 0o600, "Fallback file must have 0600 permissions");
        }

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_credential_store_atomic_delete() {
        let temp_dir = std::env::temp_dir().join(format!("retranslate_atomic_del_test_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        let _ = fs::create_dir_all(&temp_dir);
        let fallback_file = temp_dir.join("credentials.json");

        let service = format!("retrans_test_atomic_{}", std::process::id());
        let store = CredentialStore::with_service_and_fallback(service, fallback_file.clone());
        store.set_key("key1", "val1").unwrap();
        store.set_key("key2", "val2").unwrap();

        store.delete_key("key1").unwrap();
        assert_eq!(store.get_key("key1").unwrap(), None);
        assert_eq!(store.get_key("key2").unwrap(), Some("val2".into()));

        // Verify file is valid JSON and has 0600 permissions
        let content = fs::read_to_string(&fallback_file).unwrap();
        let val: serde_json::Value = serde_json::from_str(&content).unwrap();
        assert_eq!(val["api_key:key2"], "val2");
        assert!(val.get("api_key:key1").is_none());

        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let meta = fs::metadata(&fallback_file).unwrap();
            let mode = meta.permissions().mode() & 0o777;
            assert_eq!(mode, 0o600);
        }

        let _ = store.delete_key("key2");
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
