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

/// Payload of the `clip-preview-updated` event and the value returned by
/// `get_active_clip_preview`. Carries everything the preview page needs to
/// render one entry without ever crossing raw image bytes: Image entries get
/// a bounded, display-only JPEG data URL; Text/FilePaths carry their stored
/// text. Serialize-only, like Clip.
#[derive(Debug, Clone, Serialize)]
pub struct PreviewPayload {
    pub id: String,
    pub kind: ClipKind,
    pub text_content: Option<String>,
    pub image_preview_base64: Option<String>,
    pub truncated: bool,
    pub byte_size: u64,
    pub captured_at: u64,
    pub source_exe: String,
    pub source_title: String,
}

/// Highest tutorial version shipped with this build. The tutorial window
/// auto-opens once when a config's `tutorial_version` is below this value.
pub const CURRENT_TUTORIAL_VERSION: u32 = 1;

/// A durable favorite snapshot of one clipboard item. Mirrors `Clip` minus the
/// history-only `pinned` flag. The `id` is the item's `content_hash` (a stable
/// key across runs), so the same content shares one snapshot no matter how many
/// collections reference it. Raw image bytes are stored — never serialized over
/// IPC — so a favorite stays pasteable/copyable/previewable after its history
/// entry is deleted or evicted.
#[derive(Debug, Clone, Serialize)]
pub struct FavoriteItem {
    pub id: String,
    pub kind: ClipKind,
    pub text_content: Option<String>,
    #[serde(skip_serializing)]
    pub image_data: Option<Vec<u8>>,
    pub thumbnail_base64: Option<String>,
    pub content_hash: String,
    pub preview: String,
    pub truncated: bool,
    pub source_exe: String,
    pub source_title: String,
    pub source_icon: Option<String>,
    pub captured_at: u64,
    pub byte_size: u64,
    /// Membership timestamp (ms) when listed inside a collection; `None` when
    /// fetched outside one (see `list_favorite_items` vs `get_item`).
    pub added_at: Option<u64>,
}

impl FavoriteItem {
    /// Rebuild the `Clip` this snapshot came from, for reuse by the existing
    /// preview/paste/copy paths (a favorite is pasted exactly like a Clip).
    pub fn into_clip(self) -> Clip {
        Clip {
            id: self.id,
            kind: self.kind,
            text_content: self.text_content,
            image_data: self.image_data,
            thumbnail_base64: self.thumbnail_base64,
            content_hash: self.content_hash,
            preview: self.preview,
            truncated: self.truncated,
            source_exe: self.source_exe,
            source_title: self.source_title,
            source_icon: self.source_icon,
            captured_at: self.captured_at,
            pinned: false,
            byte_size: self.byte_size,
        }
    }
}

impl From<Clip> for FavoriteItem {
    fn from(clip: Clip) -> Self {
        FavoriteItem {
            // A favorite is keyed by content hash (stable across runs), so the
            // item id IS the content hash — never the session-scoped Clip id.
            id: clip.content_hash.clone(),
            kind: clip.kind,
            text_content: clip.text_content,
            image_data: clip.image_data,
            thumbnail_base64: clip.thumbnail_base64,
            content_hash: clip.content_hash,
            preview: clip.preview,
            truncated: clip.truncated,
            source_exe: clip.source_exe,
            source_title: clip.source_title,
            source_icon: clip.source_icon,
            captured_at: clip.captured_at,
            byte_size: clip.byte_size,
            added_at: None,
        }
    }
}

/// A favorites collection as shown in the sidebar.
#[derive(Debug, Clone, Serialize)]
pub struct CollectionSummary {
    pub id: String,
    pub name: String,
    pub sort_order: i64,
    pub created_at: u64,
    pub item_count: u64,
}

/// Identifies a clip by scope for cross-referencing ("favorite this history
/// item" / "copy this favorite into another collection").
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClipLocator {
    pub scope: ClipScope,
    pub id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ClipScope {
    History,
    Favorite,
}

/// Keyboard chord (key codes) that opens the favorites sidebar. Stored in
/// config; the frontend matches `KeyboardEvent.code` values, so the backend
/// only validates it — no global OS shortcut is registered for it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct PanelShortcut {
    pub codes: Vec<String>,
}

impl Default for PanelShortcut {
    fn default() -> Self {
        PanelShortcut {
            codes: vec!["AltLeft".to_string()],
        }
    }
}

/// True for the sided modifier key codes (the only modifier codes accepted).
fn is_modifier(code: &str) -> bool {
    matches!(
        code,
        "ControlLeft"
            | "ControlRight"
            | "AltLeft"
            | "AltRight"
            | "ShiftLeft"
            | "ShiftRight"
            | "MetaLeft"
            | "MetaRight"
    )
}

/// True for function-key codes F1..F12.
fn is_function(code: &str) -> bool {
    matches!(
        code,
        "F1" | "F2" | "F3" | "F4" | "F5" | "F6" | "F7" | "F8" | "F9" | "F10" | "F11" | "F12"
    )
}

/// True for the printable physical codes KeyA..KeyZ and Digit0..Digit9.
fn is_printable(code: &str) -> bool {
    if let Some(key) = code.strip_prefix("Key") {
        return key.len() == 1 && key.as_bytes()[0].is_ascii_uppercase();
    }
    if let Some(digit) = code.strip_prefix("Digit") {
        return digit.len() == 1 && digit.as_bytes()[0].is_ascii_digit();
    }
    false
}

/// Physical codes reserved by the panel/sidebar interaction surface.
fn is_reserved(code: &str) -> bool {
    matches!(
        code,
        "Escape"
            | "Enter"
            | "Space"
            | "Tab"
            | "ArrowUp"
            | "ArrowDown"
            | "ArrowLeft"
            | "ArrowRight"
            | "Slash"
    )
}

/// Map a physical code to its global-hotkey form (KeyA→A, Digit0→0, F5→F5).
fn global_key(code: &str) -> &str {
    if let Some(k) = code.strip_prefix("Key") {
        return k;
    }
    if let Some(d) = code.strip_prefix("Digit") {
        return d;
    }
    code
}

/// Canonical, order-insensitive parts of a chord: sided modifiers folded to
/// their global names, then the (non-modifier) keys, each list sorted.
fn chord_parts(codes: &[String]) -> Vec<String> {
    let mut mods: Vec<&str> = Vec::new();
    let mut keys: Vec<String> = Vec::new();
    for code in codes {
        match code.as_str() {
            "ControlLeft" | "ControlRight" => mods.push("Ctrl"),
            "AltLeft" | "AltRight" => mods.push("Alt"),
            "ShiftLeft" | "ShiftRight" => mods.push("Shift"),
            "MetaLeft" | "MetaRight" => mods.push("Super"),
            other => keys.push(global_key(other).to_string()),
        }
    }
    mods.sort_unstable();
    keys.sort();
    let mut parts: Vec<String> = mods.into_iter().map(String::from).collect();
    parts.extend(keys);
    parts
}

/// Canonical parts of a global hotkey string (e.g. "Ctrl+Shift+V").
fn hotkey_parts(hotkey: &str) -> Vec<String> {
    let mut mods: Vec<String> = Vec::new();
    let mut keys: Vec<String> = Vec::new();
    for part in hotkey.split('+') {
        match part {
            "Ctrl" | "Shift" | "Alt" | "Super" => mods.push(part.to_string()),
            other => keys.push(other.to_string()),
        }
    }
    mods.sort();
    keys.sort();
    let mut parts = mods;
    parts.extend(keys);
    parts
}

impl PanelShortcut {
    /// Reject malformed chords and keys reserved by the panel/sidebar
    /// interaction surface. Operates on physical `KeyboardEvent.code` values:
    /// unique recognized codes; a bare (modifier-less) chord is only a single
    /// modifier or a single F-key; printable letters/digits need a modifier.
    pub fn validate(&self) -> Result<(), String> {
        if self.codes.is_empty() {
            return Err("Drawer shortcut must include at least one key".to_string());
        }
        if self.codes.len() > 8 {
            return Err("Drawer shortcut has too many keys".to_string());
        }
        let mut seen = std::collections::HashSet::new();
        for code in &self.codes {
            if !seen.insert(code.as_str()) {
                return Err(format!("Drawer shortcut repeats key '{}'", code));
            }
        }
        for code in &self.codes {
            if is_reserved(code) {
                return Err(format!("Drawer shortcut key '{}' is reserved", code));
            }
        }
        for code in &self.codes {
            if !(is_modifier(code) || is_function(code) || is_printable(code)) {
                return Err(format!("Drawer shortcut key '{}' is not recognized", code));
            }
        }
        // No modifier present: only a single function key is a valid bare chord.
        if !self.codes.iter().any(|c| is_modifier(c))
            && !(self.codes.len() == 1 && is_function(&self.codes[0]))
        {
            return Err(
                "Drawer shortcut needs a modifier (Ctrl/Alt/Shift/Meta) or a function key (F1-F12)"
                    .to_string(),
            );
        }
        Ok(())
    }

    /// True when this chord is semantically the same gesture as the configured
    /// global panel hotkey (sided modifiers folded to their global names).
    pub fn equivalent_to_hotkey(&self, hotkey: &str) -> bool {
        chord_parts(&self.codes) == hotkey_parts(hotkey)
    }
}

/// Session-only runtime state of the favorites sidebar: whether it is open and
/// which collection (if any) is selected. Never persisted.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FavoritesUiState {
    pub open: bool,
    pub selected_collection: Option<String>,
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

/// User-configurable settings stored in mnemark.config.json
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
    /// Main panel opacity as a percentage (50-100, 100 = fully opaque).
    pub ui_opacity_percent: u8,
    /// UI language: "zh-TW" (default) or "en"
    pub language: String,
    /// When true, pasting a FilePaths entry writes a real CF_HDROP (the
    /// target app receives the actual files, which must still exist at their
    /// original paths). When false, the path text is pasted instead.
    pub paste_files_as_files: bool,
    /// When true, check for updates automatically (installed builds update
    /// in the background; portable builds check when the About page opens).
    pub auto_update: bool,
    /// When true, the Panel remembers the last-selected history filter across
    /// hide/show. When false (default), it resets to "All" each time the
    /// Panel opens. This does NOT persist the selected filter itself.
    pub remember_history_filter: bool,
    /// Key chord that opens the favorites sidebar (key codes). Backend-validated
    /// only — the frontend matches `KeyboardEvent.code` values.
    pub favorites_toggle_shortcut: PanelShortcut,
    /// Last tutorial version the user has seen. The backend auto-opens the
    /// tutorial once when this is below CURRENT_TUTORIAL_VERSION.
    pub tutorial_version: u32,
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
            ui_opacity_percent: 96,
            language: "zh-TW".to_string(),
            paste_files_as_files: true,
            auto_update: true,
            remember_history_filter: false,
            favorites_toggle_shortcut: PanelShortcut::default(),
            tutorial_version: 0,
        }
    }
}

impl AppConfig {
    /// Load config from the executable directory, or create default.
    pub fn load() -> Self {
        load_from(&config_path())
    }

    /// Save config to disk.
    pub fn save(&self) -> Result<(), String> {
        save_to(&config_path(), self)
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
        self.ui_opacity_percent = self.ui_opacity_percent.clamp(50, 100);
        self
    }
}

/// Load config from a specific path (split from `load` so tests use a temp
/// path). Missing file → default (written back). Corrupt file → the corrupt
/// bytes are preserved as a `.bak` and defaults are returned, so a bad config
/// never silently destroys the user's data.
fn load_from(path: &std::path::Path) -> AppConfig {
    match std::fs::read_to_string(path) {
        Ok(s) => match serde_json::from_str::<AppConfig>(&s) {
            Ok(cfg) => cfg,
            Err(e) => {
                crate::log(&format!(
                    "[Mnemark] corrupt config; backing up and using defaults: {e}"
                ));
                preserve_corrupt_config(path);
                AppConfig::default()
            }
        },
        Err(_) => {
            let config = AppConfig::default();
            if let Ok(json) = serde_json::to_string_pretty(&config) {
                let _ = std::fs::write(path, json);
            }
            config
        }
    }
}

/// Atomic config save: write a same-directory temp file, then rename it over
/// the target. `std::fs::rename` maps to `MoveFileExW(..., MOVEFILE_REPLACE_EXISTING)`
/// on Windows, so partially written JSON is never exposed at the target path.
fn save_to(path: &std::path::Path, config: &AppConfig) -> Result<(), String> {
    let json = serde_json::to_string_pretty(config).map_err(|e| e.to_string())?;
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    std::fs::write(&tmp, json).map_err(|e| format!("Failed to write {}: {}", tmp.display(), e))?;
    std::fs::rename(&tmp, path)
        .map_err(|e| format!("Failed to replace {}: {}", path.display(), e))?;
    Ok(())
}

/// Move a corrupt config aside as `mnemark.config.json.bak` so it can be
/// recovered rather than overwritten by the next save.
fn preserve_corrupt_config(path: &std::path::Path) {
    let mut backup = path.as_os_str().to_owned();
    backup.push(".bak");
    let backup = std::path::PathBuf::from(backup);
    if let Err(e) = std::fs::rename(path, &backup) {
        crate::log(&format!("[Mnemark] failed to back up corrupt config: {e}"));
    }
}

/// Where config and data files live. Portable builds keep everything next
/// to the exe; installed builds can't (the install dir may be Program
/// Files, which is not user-writable) so they use %APPDATA%\Mnemark.
pub fn data_dir() -> std::path::PathBuf {
    if crate::update::is_installed_build() {
        let dir = std::env::var_os("APPDATA")
            .map(std::path::PathBuf::from)
            .unwrap_or_else(|| std::path::PathBuf::from("."))
            .join("Mnemark");
        let _ = std::fs::create_dir_all(&dir);
        return dir;
    }
    std::env::current_exe()
        .unwrap_or_else(|_| std::path::PathBuf::from("mnemark.exe"))
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .to_path_buf()
}

fn config_path() -> std::path::PathBuf {
    data_dir().join("mnemark.config.json")
}

#[cfg(test)]
mod backward_compat_tests {
    use super::{AppConfig, PanelShortcut};

    #[test]
    fn old_json_without_remember_history_filter_defaults_false() {
        let json = r#"{
            "text_size_limit_kb": 100,
            "text_count_limit": 100,
            "image_count_limit": 10,
            "image_memory_budget_mb": 50,
            "image_size_limit_mb": 10,
            "hotkey": "Ctrl+Shift+V",
            "startup": false,
            "persist": false,
            "exclusion_list": ["1Password.exe"],
            "vim_mode": false,
            "debounce_ms": 200,
            "theme": "system",
            "language": "zh-TW",
            "paste_files_as_files": true,
            "auto_update": true
        }"#;
        let cfg: AppConfig = serde_json::from_str(json).expect("deserialize old config");
        assert!(!cfg.remember_history_filter);
        assert_eq!(cfg.ui_opacity_percent, 96);
    }

    #[test]
    fn explicit_true_round_trips() {
        let cfg = AppConfig {
            remember_history_filter: true,
            ..AppConfig::default()
        };
        let json = serde_json::to_string(&cfg).expect("serialize");
        let round: AppConfig = serde_json::from_str(&json).expect("deserialize");
        assert!(round.remember_history_filter);
    }

    #[test]
    fn explicit_false_round_trips() {
        let cfg = AppConfig {
            remember_history_filter: false,
            ..AppConfig::default()
        };
        let json = serde_json::to_string(&cfg).expect("serialize");
        let round: AppConfig = serde_json::from_str(&json).expect("deserialize");
        assert!(!round.remember_history_filter);
    }

    #[test]
    fn old_json_without_favorites_fields_defaults_safely() {
        // Pre-favorites config files have neither favorites_toggle_shortcut nor
        // tutorial_version — both must fall back to sane defaults via serde(default).
        let json = r#"{
            "hotkey": "Ctrl+Shift+V",
            "language": "en"
        }"#;
        let cfg: AppConfig = serde_json::from_str(json).expect("deserialize old config");
        assert_eq!(
            cfg.favorites_toggle_shortcut.codes,
            vec!["AltLeft".to_string()]
        );
        assert_eq!(cfg.tutorial_version, 0);
    }

    #[test]
    fn favorites_shortcut_and_tutorial_version_round_trip() {
        let cfg = AppConfig {
            favorites_toggle_shortcut: PanelShortcut {
                codes: vec!["ControlLeft".to_string(), "KeyB".to_string()],
            },
            tutorial_version: 1,
            ..AppConfig::default()
        };
        let json = serde_json::to_string(&cfg).expect("serialize");
        let round: AppConfig = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(
            round.favorites_toggle_shortcut.codes,
            vec!["ControlLeft".to_string(), "KeyB".to_string()]
        );
        assert_eq!(round.tutorial_version, 1);
    }
}

#[cfg(test)]
mod favorites_shortcut_tests {
    use super::PanelShortcut;

    fn chord(codes: &[&str]) -> PanelShortcut {
        PanelShortcut {
            codes: codes.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn default_chord_is_alt_left() {
        assert_eq!(PanelShortcut::default().codes, vec!["AltLeft".to_string()]);
        assert!(PanelShortcut::default().validate().is_ok());
    }

    #[test]
    fn empty_codes_are_rejected() {
        assert!(chord(&[]).validate().is_err());
    }

    #[test]
    fn reserved_physical_codes_are_rejected() {
        for key in [
            "Escape",
            "Enter",
            "Space",
            "Tab",
            "ArrowUp",
            "ArrowDown",
            "ArrowLeft",
            "ArrowRight",
            "Slash",
        ] {
            assert!(
                chord(&[key]).validate().is_err(),
                "{key} should be reserved"
            );
        }
    }

    #[test]
    fn duplicate_codes_are_rejected() {
        assert!(chord(&["ControlLeft", "ControlLeft"]).validate().is_err());
    }

    #[test]
    fn unrecognized_codes_are_rejected() {
        for key in [
            "j",
            "/",
            "F",
            "Key",
            "Digit",
            "Control",
            "Backspace",
            "KeyA ",
        ] {
            assert!(
                chord(&[key]).validate().is_err(),
                "{key} should be unrecognized"
            );
        }
    }

    #[test]
    fn bare_modifier_and_function_keys_are_allowed() {
        assert!(chord(&["ControlLeft"]).validate().is_ok());
        assert!(chord(&["ShiftRight"]).validate().is_ok());
        assert!(chord(&["F5"]).validate().is_ok());
        assert!(chord(&["F12"]).validate().is_ok());
    }

    #[test]
    fn bare_printable_codes_need_a_modifier() {
        assert!(chord(&["KeyA"]).validate().is_err());
        assert!(chord(&["Digit3"]).validate().is_err());
    }

    #[test]
    fn printable_with_modifier_is_allowed() {
        assert!(chord(&["ControlLeft", "KeyA"]).validate().is_ok());
        assert!(chord(&["ShiftLeft", "Digit0"]).validate().is_ok());
        assert!(chord(&["ControlLeft", "ShiftLeft", "KeyV"])
            .validate()
            .is_ok());
    }

    #[test]
    fn empty_code_string_is_rejected() {
        assert!(chord(&["  "]).validate().is_err());
    }

    #[test]
    fn too_many_codes_are_rejected() {
        let codes: Vec<String> = "ABCDEFGHI".chars().map(|c| format!("Key{c}")).collect();
        let refs: Vec<&str> = codes.iter().map(|s| s.as_str()).collect();
        assert!(chord(&refs).validate().is_err());
    }

    #[test]
    fn chord_equivalent_to_global_hotkey_is_detected() {
        assert!(chord(&["ControlLeft", "ShiftLeft", "KeyV"]).equivalent_to_hotkey("Ctrl+Shift+V"));
        assert!(chord(&["ControlLeft", "KeyV"]).equivalent_to_hotkey("Ctrl+V"));
        // Sided modifiers fold to their global name.
        assert!(chord(&["ControlRight", "MetaLeft", "KeyB"]).equivalent_to_hotkey("Super+Ctrl+B"));
    }

    #[test]
    fn chord_not_equivalent_to_hotkey() {
        assert!(!chord(&["ControlLeft"]).equivalent_to_hotkey("Ctrl+Shift+V"));
        assert!(!chord(&["ControlLeft", "KeyV"]).equivalent_to_hotkey("Ctrl+Shift+V"));
        assert!(!chord(&["ControlLeft", "KeyC"]).equivalent_to_hotkey("Ctrl+V"));
        assert!(!chord(&["F5"]).equivalent_to_hotkey("Ctrl+Shift+V"));
    }
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
            ui_opacity_percent: u8::MAX,
            ..AppConfig::default()
        }
        .sanitized();
        assert_eq!(cfg.text_size_limit_kb, 100_000);
        assert_eq!(cfg.text_count_limit, 10_000);
        assert_eq!(cfg.image_count_limit, 1_000);
        assert_eq!(cfg.image_memory_budget_mb, 2_048);
        assert_eq!(cfg.image_size_limit_mb, 256);
        assert_eq!(cfg.debounce_ms, 10_000);
        assert_eq!(cfg.ui_opacity_percent, 100);
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
        assert_eq!(cfg.ui_opacity_percent, d.ui_opacity_percent);
    }

    #[test]
    fn opacity_defaults_to_96() {
        assert_eq!(AppConfig::default().ui_opacity_percent, 96);
    }

    #[test]
    fn opacity_below_minimum_is_raised_to_50() {
        let cfg = AppConfig {
            ui_opacity_percent: 0,
            ..AppConfig::default()
        }
        .sanitized();
        assert_eq!(cfg.ui_opacity_percent, 50);
    }

    #[test]
    fn opacity_boundaries_pass_through_unchanged() {
        let fifty = AppConfig {
            ui_opacity_percent: 50,
            ..AppConfig::default()
        }
        .sanitized();
        assert_eq!(fifty.ui_opacity_percent, 50);

        let hundred = AppConfig {
            ui_opacity_percent: 100,
            ..AppConfig::default()
        }
        .sanitized();
        assert_eq!(hundred.ui_opacity_percent, 100);
    }
}

#[cfg(test)]
mod atomic_config_tests {
    use super::*;
    use std::path::PathBuf;

    fn temp_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("mnemark-{name}-{}.json", std::process::id()))
    }

    fn cleanup(path: &PathBuf) {
        let _ = std::fs::remove_file(path);
        let mut bak = path.as_os_str().to_owned();
        bak.push(".bak");
        let _ = std::fs::remove_file(PathBuf::from(bak));
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        let _ = std::fs::remove_file(PathBuf::from(tmp));
    }

    #[test]
    fn save_then_load_round_trips() {
        let path = temp_path("roundtrip");
        cleanup(&path);
        let cfg = AppConfig {
            persist: true,
            text_count_limit: 42,
            ..AppConfig::default()
        };
        save_to(&path, &cfg).unwrap();
        let loaded = load_from(&path);
        assert!(loaded.persist);
        assert_eq!(loaded.text_count_limit, 42);
        // The temp file is gone after the atomic rename.
        let mut tmp = path.as_os_str().to_owned();
        tmp.push(".tmp");
        assert!(!PathBuf::from(tmp).exists());
        cleanup(&path);
    }

    #[test]
    fn save_overwrites_existing_config() {
        let path = temp_path("overwrite");
        cleanup(&path);
        save_to(&path, &AppConfig::default()).unwrap();
        let next = AppConfig {
            persist: true,
            ..AppConfig::default()
        };
        save_to(&path, &next).unwrap();
        assert!(load_from(&path).persist);
        cleanup(&path);
    }

    #[test]
    fn corrupt_file_is_backed_up_and_defaults_returned() {
        let path = temp_path("corrupt");
        cleanup(&path);
        std::fs::write(&path, "{ not valid json").unwrap();
        let loaded = load_from(&path);
        assert!(!loaded.persist);
        assert_eq!(loaded.hotkey, AppConfig::default().hotkey);
        // The corrupt bytes are preserved for recovery...
        let mut bak = path.as_os_str().to_owned();
        bak.push(".bak");
        let bak = PathBuf::from(bak);
        assert_eq!(std::fs::read_to_string(&bak).unwrap(), "{ not valid json");
        // ...and the original path was moved aside, not silently overwritten.
        assert!(!path.exists());
        cleanup(&path);
    }
}
