use serde::{Deserialize, Serialize};

/// A unique clipboard entry.
/// Serialize-only: the frontend receives Clips but never sends them back
/// (commands take ids or plain text), so no Deserialize derive.
#[derive(Debug, Clone, Serialize)]
pub struct Clip {
    pub id: String,
    pub kind: ClipKind,
    /// Raw text content (text Clips) or semicolon-separated paths (FilePaths Clips)
    pub text_content: Option<String>,
    /// Compressed image data (DIB format) for Image Clips.
    /// Never serialized: raw images must not cross the IPC bridge as JSON
    /// number arrays (10MB → ~30MB JSON). Paste fetches the bytes by id.
    #[serde(skip_serializing)]
    pub image_data: Option<Vec<u8>>,
    /// Base64-encoded JPEG thumbnail (200px wide) for Image Clips
    pub thumbnail_base64: Option<String>,
    /// SHA-256 hex digest of the original content (pre-truncation for text)
    pub content_hash: String,
    /// First 200 chars of text for preview
    pub preview: String,
    /// Whether this Clip was truncated because it exceeded the size limit
    pub truncated: bool,
    /// Executable name of the foreground application
    pub source_exe: String,
    /// Window title at capture time
    pub source_title: String,
    /// Base64-encoded icon of the source application (cached)
    pub source_icon: Option<String>,
    /// Unix timestamp in milliseconds
    pub captured_at: u64,
    /// Whether this Clip is pinned
    pub pinned: bool,
    /// Byte size of the original content (pre-truncation for text)
    pub byte_size: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub enum ClipKind {
    Text,
    Image,
    FilePaths,
}

/// Payload of the `clipboard-update` event: the freshly captured Clip plus
/// the ids of any Clips evicted by capacity limits, so the frontend can drop
/// them and stay in sync with the backend History.
#[derive(Debug, Clone, Serialize)]
pub struct ClipboardUpdate {
    pub clip: Clip,
    pub evicted: Vec<String>,
}

impl Clip {
    /// Generate a new unique ID based on content hash and timestamp.
    pub fn new_id(content_hash: &str, captured_at: u64) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(content_hash.as_bytes());
        hasher.update(captured_at.to_be_bytes());
        hex::encode(hasher.finalize())[..16].to_string()
    }

    /// Clone everything except the raw image bytes (built field-by-field —
    /// `..self.clone()` would deep-copy image_data first). For IPC responses,
    /// where image_data is skip_serializing anyway and cloning up to 10 MB
    /// per image per call is pure waste.
    pub fn meta_clone(&self) -> Clip {
        Clip {
            id: self.id.clone(),
            kind: self.kind.clone(),
            text_content: self.text_content.clone(),
            image_data: None,
            thumbnail_base64: self.thumbnail_base64.clone(),
            content_hash: self.content_hash.clone(),
            preview: self.preview.clone(),
            truncated: self.truncated,
            source_exe: self.source_exe.clone(),
            source_title: self.source_title.clone(),
            source_icon: self.source_icon.clone(),
            captured_at: self.captured_at,
            pinned: self.pinned,
            byte_size: self.byte_size,
        }
    }
}

/// User-configurable settings stored in clipflow.config.json
/// Missing fields fall back to defaults so older config files keep working.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppConfig {
    pub text_size_limit_kb: u64,
    pub text_count_limit: usize,
    pub image_count_limit: usize,
    pub image_memory_budget_mb: u64,
    pub image_size_limit_mb: u64,
    pub hotkey: String,
    pub startup: bool,
    pub persist: bool,
    pub exclusion_list: Vec<String>,
    pub vim_mode: bool,
    pub debounce_ms: u64,
    pub theme: String,
    /// UI language: "zh-TW" (default) or "en"
    pub language: String,
    /// When true, pasting a FilePaths entry writes a real CF_HDROP (the
    /// target app receives the actual files, which must still exist at their
    /// original paths). When false, the path text is pasted instead.
    pub paste_files_as_files: bool,
    /// When true, check for updates automatically (installed builds update
    /// in the background; portable builds check when the About page opens).
    pub auto_update: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        Self {
            text_size_limit_kb: 100,
            text_count_limit: 100,
            image_count_limit: 10,
            image_memory_budget_mb: 50,
            image_size_limit_mb: 10,
            hotkey: "Ctrl+Shift+V".to_string(),
            // Off by default: autostart is opt-in via Settings, which creates
            // the shell:startup shortcut at toggle time.
            startup: false,
            persist: false,
            exclusion_list: vec![
                "1Password.exe".to_string(),
                "Bitwarden.exe".to_string(),
                "KeePass.exe".to_string(),
            ],
            vim_mode: false,
            debounce_ms: 200,
            theme: "system".to_string(),
            language: "zh-TW".to_string(),
            paste_files_as_files: true,
            auto_update: true,
        }
    }
}

impl AppConfig {
    /// Load config from the executable directory, or create default.
    pub fn load() -> Self {
        let path = config_path();
        if path.exists() {
            std::fs::read_to_string(&path)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        } else {
            let config = Self::default();
            if let Ok(json) = serde_json::to_string_pretty(&config) {
                let _ = std::fs::write(&path, json);
            }
            config
        }
    }

    /// Save config to disk.
    pub fn save(&self) -> Result<(), String> {
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        std::fs::write(config_path(), json).map_err(|e| e.to_string())
    }

    /// Clamp values that break behavior at extremes. The settings UI
    /// enforces ranges, but the config file is user-editable JSON, and
    /// commands receive whatever the frontend sends. Upper bounds sit far
    /// above the UI maxima: they only stop a hand-edited config from
    /// allowing unbounded memory growth.
    pub fn sanitized(mut self) -> Self {
        self.text_size_limit_kb = self.text_size_limit_kb.clamp(1, 100_000);
        self.text_count_limit = self.text_count_limit.clamp(1, 10_000);
        self.image_count_limit = self.image_count_limit.clamp(1, 1_000);
        self.image_memory_budget_mb = self.image_memory_budget_mb.clamp(1, 2_048);
        self.image_size_limit_mb = self.image_size_limit_mb.clamp(1, 256);
        self.debounce_ms = self.debounce_ms.min(10_000);
        self
    }
}

/// Where config and data files live. Portable builds keep everything next
/// to the exe; installed builds can't (the install dir may be Program
/// Files, which is not user-writable) so they use %APPDATA%\ClipFlow.
pub fn data_dir() -> std::path::PathBuf {
    if crate::update::is_installed_build() {
        let dir = std::env::var_os("APPDATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("ClipFlow");
        let _ = std::fs::create_dir_all(&dir);
        return dir;
    }
    std::env::current_exe()
        .unwrap_or_else(|_| std::path::PathBuf::from("ClipFlow.exe"))
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .to_path_buf()
}

fn config_path() -> std::path::PathBuf {
    data_dir().join("clipflow.config.json")
}

#[cfg(test)]
mod sanitize_tests {
    use super::AppConfig;

    #[test]
    fn zeros_are_raised_to_the_minimum() {
        let cfg = AppConfig {
            text_size_limit_kb: 0,
            text_count_limit: 0,
            image_count_limit: 0,
            image_memory_budget_mb: 0,
            image_size_limit_mb: 0,
            ..AppConfig::default()
        }
        .sanitized();
        assert_eq!(cfg.text_size_limit_kb, 1);
        assert_eq!(cfg.text_count_limit, 1);
        assert_eq!(cfg.image_count_limit, 1);
        assert_eq!(cfg.image_memory_budget_mb, 1);
        assert_eq!(cfg.image_size_limit_mb, 1);
    }

    #[test]
    fn absurd_values_are_capped() {
        // A hand-edited config must not allow unbounded memory growth.
        let cfg = AppConfig {
            text_size_limit_kb: u64::MAX,
            text_count_limit: usize::MAX,
            image_count_limit: usize::MAX,
            image_memory_budget_mb: u64::MAX,
            image_size_limit_mb: u64::MAX,
            debounce_ms: u64::MAX,
            ..AppConfig::default()
        }
        .sanitized();
        assert_eq!(cfg.text_size_limit_kb, 100_000);
        assert_eq!(cfg.text_count_limit, 10_000);
        assert_eq!(cfg.image_count_limit, 1_000);
        assert_eq!(cfg.image_memory_budget_mb, 2_048);
        assert_eq!(cfg.image_size_limit_mb, 256);
        assert_eq!(cfg.debounce_ms, 10_000);
    }

    #[test]
    fn normal_values_pass_through_unchanged() {
        let cfg = AppConfig::default().sanitized();
        let d = AppConfig::default();
        assert_eq!(cfg.text_size_limit_kb, d.text_size_limit_kb);
        assert_eq!(cfg.text_count_limit, d.text_count_limit);
        assert_eq!(cfg.image_count_limit, d.image_count_limit);
        assert_eq!(cfg.image_memory_budget_mb, d.image_memory_budget_mb);
        assert_eq!(cfg.image_size_limit_mb, d.image_size_limit_mb);
        assert_eq!(cfg.debounce_ms, d.debounce_ms);
    }
}
