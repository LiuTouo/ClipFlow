//! Update support. One binary serves both channels:
//! - installed (NSIS, per-user under %LOCALAPPDATA%\Programs): full auto
//!   update via tauri-plugin-updater (check → download → verify signature →
//!   install → confirm restart).
//! - portable (raw exe anywhere else): the updater must NEVER run here — it
//!   would run the NSIS installer over a portable exe. Portable updates are
//!   checked/downloaded from the About page in TS (GitHub API + plugin-fs);
//!   Rust only tells the frontend which channel this is.
//!
//! Known limitation: a portable exe hand-placed under %LOCALAPPDATA%\Programs
//! is misdetected as installed.

use crate::models::AppConfig;
use serde::Serialize;
use std::sync::{Arc, Mutex};
use tauri::Manager;

/// Tauri v2 NSIS defaults to a per-user install under %LOCALAPPDATA%\Programs.
pub fn is_installed_build() -> bool {
    let exe = std::env::current_exe().unwrap_or_default();
    let programs = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_default()
        .join("Programs");
    exe.starts_with(programs)
}

#[tauri::command]
pub fn update_channel() -> &'static str {
    if is_installed_build() { "installed" } else { "portable" }
}

#[derive(Serialize)]
pub struct UpdateCheck {
    /// "up_to_date" | "available"
    pub status: String,
    pub version: Option<String>,
}

#[tauri::command]
pub async fn check_for_updates(app: tauri::AppHandle) -> Result<UpdateCheck, String> {
    use tauri_plugin_updater::UpdaterExt;

    if !is_installed_build() {
        return Err("Updater is only available in installed builds".to_string());
    }
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?;
    Ok(match update {
        Some(u) => UpdateCheck { status: "available".to_string(), version: Some(u.version) },
        None => UpdateCheck { status: "up_to_date".to_string(), version: None },
    })
}

/// Download and install the pending update. Returns the new version.
#[tauri::command]
pub async fn install_update(app: tauri::AppHandle) -> Result<String, String> {
    use tauri_plugin_updater::UpdaterExt;

    if !is_installed_build() {
        return Err("Updater is only available in installed builds".to_string());
    }
    let update = app
        .updater()
        .map_err(|e| e.to_string())?
        .check()
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "No update available".to_string())?;
    let version = update.version.clone();
    update
        .download_and_install(|_chunk, _total| {}, || {})
        .await
        .map_err(|e| e.to_string())?;
    Ok(version)
}

#[tauri::command]
pub fn restart_app(app: tauri::AppHandle) {
    tauri::process::restart(&app.env());
}

struct UpdateLabels {
    title: &'static str,
    restart_body: &'static str,
}

/// Localized MessageBox strings, mirroring tray_labels() in lib.rs.
fn update_labels(lang: &str) -> UpdateLabels {
    if lang == "en" {
        UpdateLabels {
            title: "ClipFlow Update",
            restart_body: "A new version has been installed. Restart ClipFlow now?",
        }
    } else {
        UpdateLabels {
            title: "ClipFlow 更新",
            restart_body: "已安裝新版本。現在重新啟動 ClipFlow 嗎？",
        }
    }
}

/// Yes/No system dialog. Returns true on Yes.
fn msg_box_yes_no(title: &str, body: &str) -> bool {
    use windows::core::PCWSTR;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        MessageBoxW, IDYES, MB_DEFBUTTON2, MB_ICONINFORMATION, MB_YESNO,
    };

    let to_wide = |s: &str| -> Vec<u16> {
        use std::ffi::OsStr;
        use std::os::windows::ffi::OsStrExt;
        OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
    };
    let title_w = to_wide(title);
    let body_w = to_wide(body);
    unsafe {
        MessageBoxW(
            HWND(std::ptr::null_mut()),
            PCWSTR(body_w.as_ptr()),
            PCWSTR(title_w.as_ptr()),
            MB_YESNO | MB_ICONINFORMATION | MB_DEFBUTTON2,
        ) == IDYES
    }
}

/// Called once from setup(). No-op unless auto_update is on AND this is an
/// installed build. Everything is automatic except one confirmation click
/// before the restart; errors (including a 404 latest.json before the first
/// CI release exists) are logged, never shown at startup.
pub fn spawn_auto_update_check(app: tauri::AppHandle, config: Arc<Mutex<AppConfig>>) {
    let (enabled, lang) = {
        let cfg = config.lock().unwrap();
        (cfg.auto_update, cfg.language.clone())
    };
    if !enabled || !is_installed_build() {
        return;
    }

    tauri::async_runtime::spawn(async move {
        use tauri_plugin_updater::UpdaterExt;

        // Let the app settle before hitting the network.
        tokio::time::sleep(std::time::Duration::from_secs(5)).await;

        let update = match app.updater().map_err(|e| e.to_string()) {
            Ok(u) => match u.check().await {
                Ok(Some(update)) => update,
                Ok(None) => return, // up to date
                Err(e) => {
                    crate::log(&format!("[ClipFlow] auto-update check failed: {e}"));
                    return;
                }
            },
            Err(e) => {
                crate::log(&format!("[ClipFlow] auto-update unavailable: {e}"));
                return;
            }
        };

        crate::log(&format!("[ClipFlow] auto-update: installing v{}", update.version));
        if let Err(e) = update.download_and_install(|_chunk, _total| {}, || {}).await {
            crate::log(&format!("[ClipFlow] auto-update install failed: {e}"));
            return;
        }

        // On Windows the NSIS updater may have already replaced/relaunched
        // the process; if we're still running, offer the restart.
        let labels = update_labels(&lang);
        if msg_box_yes_no(labels.title, labels.restart_body) {
            tauri::process::restart(&app.env());
        }
    });
}
