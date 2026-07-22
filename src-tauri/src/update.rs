//! Update support. One binary serves both channels:
//! - installed (NSIS): full auto update via tauri-plugin-updater (check →
//!   download → verify signature → install → confirm restart). The updater
//!   must NEVER run on portable — it would run the NSIS installer over a
//!   portable exe.
//! - portable (raw exe anywhere else): the About page checks GitHub for a
//!   newer release, then Rust downloads the new exe (the webview's fetch
//!   dies on CORS when GitHub redirects to the CDN) and the user overwrites
//!   the old exe manually after quitting.
//!
//! Channel detection reads the NSIS uninstall registry key — the only
//! reliable marker, since the install location is user-selectable
//! (per-user %LOCALAPPDATA%\Programs or per-machine Program Files).

use crate::models::AppConfig;
use serde::Serialize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use tauri::Manager;

fn to_wide(s: &str) -> Vec<u16> {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    OsStr::new(s).encode_wide().chain(std::iter::once(0)).collect()
}

/// InstallLocation from the NSIS uninstall key, if this machine has one.
/// The value is written quoted ("C:\...\ClipFlow") — quotes are trimmed.
fn install_location_from_registry() -> Option<PathBuf> {
    use windows::core::PCWSTR;
    use windows::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY, HKEY_CURRENT_USER,
        HKEY_LOCAL_MACHINE, KEY_READ,
    };

    let subkey = to_wide("Software\\Microsoft\\Windows\\CurrentVersion\\Uninstall\\ClipFlow");
    let value = to_wide("InstallLocation");

    for root in [HKEY_LOCAL_MACHINE, HKEY_CURRENT_USER] {
        unsafe {
            let mut hkey = HKEY::default();
            if RegOpenKeyExW(root, PCWSTR(subkey.as_ptr()), 0, KEY_READ, &mut hkey).is_err() {
                continue;
            }
            let mut buf = [0u16; 512];
            let mut len = (buf.len() * 2) as u32;
            let ok = RegQueryValueExW(
                hkey,
                PCWSTR(value.as_ptr()),
                None,
                None,
                Some(buf.as_mut_ptr() as *mut u8),
                Some(&mut len),
            );
            let _ = RegCloseKey(hkey);
            if ok.is_ok() && len >= 2 {
                let raw = String::from_utf16_lossy(&buf[..len as usize / 2 - 1]);
                let trimmed = raw.trim_matches('"').trim_end_matches(['\\', '/']);
                if !trimmed.is_empty() {
                    return Some(PathBuf::from(trimmed));
                }
            }
        }
    }
    None
}

/// True when this exe lives in the directory the NSIS installer registered.
/// Case-insensitive, trailing-separator-insensitive.
pub fn is_installed_build() -> bool {
    let exe = std::env::current_exe().unwrap_or_default();
    let Some(dir) = install_location_from_registry() else {
        return false;
    };
    let norm = |p: &std::path::Path| {
        p.to_string_lossy()
            .trim_end_matches(['\\', '/'])
            .to_lowercase()
    };
    exe.parent().map(norm) == Some(norm(&dir))
}

#[tauri::command]
pub fn update_channel() -> &'static str {
    if is_installed_build() { "installed" } else { "portable" }
}

/// Download the newer portable exe next to the running one. Rust-side
/// because GitHub's asset CDN omits CORS headers, so webview fetch fails.
/// Returns the destination path.
#[tauri::command]
pub async fn download_portable_update(url: String) -> Result<String, String> {
    if is_installed_build() {
        return Err("Portable download is only for portable builds".to_string());
    }
    let dest = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or_else(|| "No exe dir".to_string())?
        .join("clipflow-update.exe");

    tauri::async_runtime::spawn_blocking(move || {
        let resp = ureq::get(&url).call().map_err(|e| e.to_string())?;
        let mut reader = resp.into_reader();
        let mut file = std::fs::File::create(&dest).map_err(|e| e.to_string())?;
        std::io::copy(&mut reader, &mut file).map_err(|e| e.to_string())?;
        Ok(dest.to_string_lossy().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
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
