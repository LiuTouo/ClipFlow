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

/// The minisign public key, mirrored from tauri.conf.json
/// (`plugins.updater.pubkey`). The same key verifies NSIS updater artifacts
/// and portable-update downloads — keep the two in sync when rotating keys.
const UPDATE_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IEQ5QjAzOEE4RUJCOTM4MjAKUldRZ09MbnJxRGl3Mlp3NTlQaVNtTmxJS1B1NjRtQ3JjdFlPaklzR2V6d0ZQQ1hoak1NSDdyWDIK";

/// A portable exe is ~15 MB; anything wildly bigger is not an update.
const MAX_DOWNLOAD_BYTES: u64 = 128 * 1024 * 1024;

/// Hosts a portable update may be fetched from. GitHub redirects release
/// assets to its CDN, so both are allowed — and every redirect hop is
/// re-validated: the webview supplies these URLs and must not pivot elsewhere.
fn is_allowed_host(host: &str) -> bool {
    host.eq_ignore_ascii_case("github.com")
        || host.to_ascii_lowercase().ends_with(".githubusercontent.com")
}

/// Enforce https + the host allowlist on an absolute URL. Applied to the
/// initial URL and to every redirect target.
fn validate_download_url(url: &str) -> Result<(), String> {
    let rest = url
        .strip_prefix("https://")
        .ok_or_else(|| "Download URL must be https".to_string())?;
    let authority = rest.split('/').next().unwrap_or_default();
    // Strip any userinfo ("user@host") before comparing hosts.
    let host_port = authority.rsplit('@').next().unwrap_or_default();
    let host = host_port.split(':').next().unwrap_or_default();
    if host.is_empty() || !is_allowed_host(host) {
        return Err(format!("Download host '{host}' is not allowed"));
    }
    Ok(())
}

/// GET `start_url`, following at most 5 redirects with each hop re-validated
/// (https + host allowlist). The body is capped at MAX_DOWNLOAD_BYTES.
fn download_validated(start_url: &str) -> Result<Vec<u8>, String> {
    use std::io::Read;

    let agent = ureq::AgentBuilder::new()
        .redirects(0)
        .timeout_connect(std::time::Duration::from_secs(10))
        .timeout_read(std::time::Duration::from_secs(120))
        .build();

    let mut url = start_url.to_string();
    for _ in 0..5 {
        validate_download_url(&url)?;
        let resp = match agent.get(&url).call() {
            Ok(resp) => resp,
            Err(ureq::Error::Status(_, resp)) => resp, // 3xx arrives here with redirects(0)
            Err(e) => return Err(format!("Download failed: {e}")),
        };
        let status = resp.status();
        if (300..400).contains(&status) {
            let location = resp
                .header("Location")
                .ok_or_else(|| "Redirect without Location".to_string())?;
            url = location.to_string();
            continue;
        }
        if status != 200 {
            return Err(format!("Download failed with status {status}"));
        }
        let mut buf = Vec::new();
        resp.into_reader()
            .take(MAX_DOWNLOAD_BYTES + 1)
            .read_to_end(&mut buf)
            .map_err(|e| e.to_string())?;
        if buf.len() as u64 > MAX_DOWNLOAD_BYTES {
            return Err("Download exceeds the size limit".to_string());
        }
        return Ok(buf);
    }
    Err("Too many redirects".to_string())
}

/// Verify `data` against a `tauri signer sign` .sig file, using a pubkey in
/// the tauri.conf.json format (both are base64-wrapped minisign text).
fn verify_with_pubkey(data: &[u8], sig_file: &str, pubkey_b64: &str) -> Result<(), String> {
    use base64::Engine;
    let b64 = base64::engine::general_purpose::STANDARD;

    let sig_text = b64
        .decode(sig_file.trim())
        .map_err(|e| format!("Bad signature encoding: {e}"))?;
    let sig_text = String::from_utf8(sig_text).map_err(|e| e.to_string())?;
    let signature = minisign_verify::Signature::decode(&sig_text)
        .map_err(|e| format!("Bad signature: {e}"))?;

    // The pubkey base64-decodes to the minisign.pub text format
    // ("untrusted comment: ...\n<key base64>"), which PublicKey::decode reads.
    let key_text = b64
        .decode(pubkey_b64.trim())
        .map_err(|e| format!("Bad pubkey encoding: {e}"))?;
    let key_text = String::from_utf8(key_text).map_err(|e| e.to_string())?;
    let pubkey =
        minisign_verify::PublicKey::decode(&key_text).map_err(|e| format!("Bad pubkey: {e}"))?;

    pubkey
        .verify(data, &signature, false)
        .map_err(|e| format!("Signature verification failed: {e}"))
}

/// Download the newer portable exe next to the running one. Rust-side
/// because GitHub's asset CDN omits CORS headers, so webview fetch fails.
/// The exe is written ONLY after its minisign signature verifies — an
/// unverified byte never becomes clipflow-update.exe. Returns the dest path.
#[tauri::command]
pub async fn download_portable_update(url: String, sig_url: String) -> Result<String, String> {
    if is_installed_build() {
        return Err("Portable download is only for portable builds".to_string());
    }
    validate_download_url(&url)?;
    validate_download_url(&sig_url)?;
    let dest = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or_else(|| "No exe dir".to_string())?
        .join("clipflow-update.exe");

    tauri::async_runtime::spawn_blocking(move || {
        let exe_bytes = download_validated(&url)?;
        let sig_bytes = download_validated(&sig_url)?;
        let sig_text = String::from_utf8(sig_bytes).map_err(|e| e.to_string())?;
        verify_with_pubkey(&exe_bytes, &sig_text, UPDATE_PUBKEY)?;
        std::fs::write(&dest, &exe_bytes).map_err(|e| e.to_string())?;
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


#[cfg(test)]
mod tests {
    use super::*;

    // Fixture generated with `npx tauri signer generate` + `npx tauri signer
    // sign` — the exact formats the CI release pipeline uploads and that
    // tauri.conf.json's updater pubkey uses (base64-wrapped minisign text).
    const TEST_PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDlDRDFFNTQ5OTdDNDExQTUKUldTbEVjU1hTZVhSbkwrR3FaVWNMejFiTDROak9PL1RNeGliak81WW9ENTltQXFxTEI4MlJ3Q1QK";
    const TEST_SIG: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IHNpZ25hdHVyZSBmcm9tIHRhdXJpIHNlY3JldCBrZXkKUlVTbEVjU1hTZVhSbk50c0Y4dmYzNVlzZmh2Z3FYaTk2V0RQNW43VUxicjMvMEsraXhBSzFOcDhKcURKUXVmREFMYzVvaG1RUU13K3ArTjVhVkVqMklVUnNiMWVLRUdsVmc0PQp0cnVzdGVkIGNvbW1lbnQ6IHRpbWVzdGFtcDoxNzg0NzgyMTAyCWZpbGU6ZHVtbXkuZXhlClRibDZJK20rRGFoekUwS25TZ01pSGdYZnA0Si9jQkNKampHTFphYmdOSnUyVXVpMzlTb0MzRDVFVVYxT082MjIxbFhoOUVESk14RTJjM1ZvZ1JzRERBPT0K";
    const TEST_DATA: &[u8] = b"dummy-portable-exe-bytes-for-signature-test";

    #[test]
    fn verifies_a_real_tauri_cli_signature() {
        verify_with_pubkey(TEST_DATA, TEST_SIG, TEST_PUBKEY).unwrap();
    }

    #[test]
    fn rejects_tampered_data() {
        let mut bad = TEST_DATA.to_vec();
        bad[0] ^= 0xFF;
        assert!(verify_with_pubkey(&bad, TEST_SIG, TEST_PUBKEY).is_err());
    }

    #[test]
    fn rejects_a_signature_from_another_key() {
        // The project's real update keypair did not sign this fixture.
        assert!(verify_with_pubkey(TEST_DATA, TEST_SIG, UPDATE_PUBKEY).is_err());
    }

    #[test]
    fn url_validation_accepts_only_https_github_hosts() {
        for good in [
            "https://github.com/LiuTouo/ClipFlow/releases/download/v1.0.0/ClipFlow_v1.0.0_x64-portable.exe",
            "https://objects.githubusercontent.com/github-production-release-asset/abc",
            "https://release-assets.githubusercontent.com/github-production/abc",
            "https://github.com:443/x",
        ] {
            assert!(validate_download_url(good).is_ok(), "should accept: {good}");
        }
        for bad in [
            "http://github.com/x",           // plain http
            "https://evil.com/x",
            "https://github.com.evil.com/x", // lookalike suffix
            "https://github.com@evil.com/x", // userinfo hides the real host
            "https://evilgithubusercontent.com/x",
            "ftp://github.com/x",
            "github.com/x", // no scheme
            "https:///x",   // empty host
            "/relative/path", // redirects must stay absolute
        ] {
            assert!(validate_download_url(bad).is_err(), "should reject: {bad}");
        }
    }
}
