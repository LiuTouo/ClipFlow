//! Manage the portable autostart shortcut (`Mnemark.lnk`) in the current
//! user's `shell:startup` folder. No registry writes — per CONTEXT spec the
//! shortcut points at the exe with the `--hidden` flag.

use std::path::{Path, PathBuf};

use windows::core::{Interface, HSTRING};
use windows::Win32::Foundation::S_OK;
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, IPersistFile,
    CLSCTX_INPROC_SERVER, COINIT_APARTMENTTHREADED,
};
use windows::Win32::UI::Shell::{
    FOLDERID_Startup, IShellLinkW, SHGetKnownFolderPath, ShellLink, KF_FLAG_DEFAULT,
};

/// Resolve the per-user startup folder (what `shell:startup` expands to).
fn startup_dir() -> Result<PathBuf, String> {
    let pwstr = unsafe { SHGetKnownFolderPath(&FOLDERID_Startup, KF_FLAG_DEFAULT, None) }
        .map_err(|e| format!("Failed to locate shell:startup folder: {}", e))?;
    let path =
        unsafe { pwstr.to_string() }.map_err(|e| format!("Invalid shell:startup path: {}", e))?;
    unsafe { CoTaskMemFree(Some(pwstr.as_ptr() as *const core::ffi::c_void)) };
    Ok(PathBuf::from(path))
}

fn shortcut_path() -> Result<PathBuf, String> {
    Ok(startup_dir()?.join("Mnemark.lnk"))
}

/// What the autostart migration must do for a given legacy/current pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ShortcutAction {
    /// No legacy shortcut present — nothing to migrate.
    Noop,
    /// Legacy and Mnemark shortcuts both present — drop the legacy one.
    RemoveLegacy,
    /// Legacy present, Mnemark absent — create Mnemark, then drop legacy only
    /// once the new shortcut is confirmed to exist.
    CreateThenRemoveLegacy,
}

/// Decide the migration action from the two shortcuts' on-disk presence.
fn plan_shortcut_migration(legacy_exists: bool, current_exists: bool) -> ShortcutAction {
    match (legacy_exists, current_exists) {
        (false, _) => ShortcutAction::Noop,
        (true, true) => ShortcutAction::RemoveLegacy,
        (true, false) => ShortcutAction::CreateThenRemoveLegacy,
    }
}

fn remove_file_if_exists(path: &Path) -> Result<(), String> {
    if path.exists() {
        std::fs::remove_file(path)
            .map_err(|e| format!("Failed to remove {}: {}", path.display(), e))?;
    }
    Ok(())
}

/// Core of the shortcut migration, split from the COM-dependent `set_startup`
/// so the file logic is unit-testable. `create_current` creates the Mnemark
/// shortcut (real: `set_startup(true)`; tests: a plain file write). The legacy
/// shortcut is only removed after the destination exists — a failed creation
/// returns `Err` and leaves the only working (legacy) shortcut in place.
fn migrate_shortcut_files(
    legacy: &Path,
    current: &Path,
    create_current: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    match plan_shortcut_migration(legacy.exists(), current.exists()) {
        ShortcutAction::Noop => Ok(()),
        ShortcutAction::RemoveLegacy => remove_file_if_exists(legacy),
        ShortcutAction::CreateThenRemoveLegacy => {
            create_current()?;
            if current.exists() {
                remove_file_if_exists(legacy)
            } else {
                Err("failed to create Mnemark autostart shortcut; legacy retained".to_string())
            }
        }
    }
}

/// One-time migration of the legacy autostart shortcut: if `ClipFlow.lnk`
/// exists and `Mnemark.lnk` is absent, create the latter (pointing at the
/// current exe) and only then remove the former; if both exist, remove the
/// legacy shortcut. A failure never removes the only working legacy shortcut.
pub fn migrate_legacy_startup_shortcut() -> Result<(), String> {
    let startup = startup_dir()?;
    let legacy = startup.join("ClipFlow.lnk");
    let current = startup.join("Mnemark.lnk");
    migrate_shortcut_files(&legacy, &current, || set_startup(true))
}

/// Create or remove the autostart shortcut.
pub fn set_startup(enabled: bool) -> Result<(), String> {
    let lnk = shortcut_path()?;

    if !enabled {
        if lnk.exists() {
            std::fs::remove_file(&lnk)
                .map_err(|e| format!("Failed to remove {}: {}", lnk.display(), e))?;
        }
        return Ok(());
    }

    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe_dir = exe.parent().map(|p| p.to_path_buf()).unwrap_or_default();

    unsafe {
        let hr = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        if hr.is_err() {
            return Err(format!("CoInitializeEx failed: {:?}", hr));
        }
        // Balance only the initialization we actually performed.
        let should_uninit = hr == S_OK;

        let result = (|| -> Result<(), String> {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER)
                .map_err(|e| format!("Failed to create ShellLink object: {}", e))?;
            link.SetPath(&HSTRING::from(exe.to_string_lossy().as_ref()))
                .map_err(|e| format!("IShellLinkW::SetPath failed: {}", e))?;
            link.SetArguments(&HSTRING::from("--hidden"))
                .map_err(|e| format!("IShellLinkW::SetArguments failed: {}", e))?;
            link.SetWorkingDirectory(&HSTRING::from(exe_dir.to_string_lossy().as_ref()))
                .map_err(|e| format!("IShellLinkW::SetWorkingDirectory failed: {}", e))?;
            let file: IPersistFile = link
                .cast()
                .map_err(|e| format!("ShellLink does not expose IPersistFile: {}", e))?;
            file.Save(&HSTRING::from(lnk.to_string_lossy().as_ref()), true)
                .map_err(|e| format!("Failed to save {}: {}", lnk.display(), e))?;
            Ok(())
        })();

        if should_uninit {
            CoUninitialize();
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::{migrate_shortcut_files, plan_shortcut_migration, ShortcutAction};
    use std::path::PathBuf;

    fn temp(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("mnemark-startup-{}-{}", name, std::process::id()))
    }

    fn reset(dir: &std::path::Path) {
        let _ = std::fs::remove_dir_all(dir);
        std::fs::create_dir_all(dir).unwrap();
    }

    /// A `create_current` closure that writes a real file, simulating success.
    fn creator(path: PathBuf) -> impl FnOnce() -> Result<(), String> {
        move || std::fs::write(&path, "shortcut").map_err(|e| e.to_string())
    }

    #[test]
    fn plans_each_state() {
        assert_eq!(plan_shortcut_migration(false, false), ShortcutAction::Noop);
        assert_eq!(plan_shortcut_migration(false, true), ShortcutAction::Noop);
        assert_eq!(
            plan_shortcut_migration(true, true),
            ShortcutAction::RemoveLegacy
        );
        assert_eq!(
            plan_shortcut_migration(true, false),
            ShortcutAction::CreateThenRemoveLegacy
        );
    }

    #[test]
    fn no_legacy_is_a_noop() {
        let dir = temp("noop");
        reset(&dir);
        let legacy = dir.join("ClipFlow.lnk");
        let current = dir.join("Mnemark.lnk");
        migrate_shortcut_files(&legacy, &current, creator(current.clone())).unwrap();
        assert!(!legacy.exists());
        assert!(!current.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn legacy_only_creates_then_removes_legacy() {
        let dir = temp("create");
        reset(&dir);
        let legacy = dir.join("ClipFlow.lnk");
        let current = dir.join("Mnemark.lnk");
        std::fs::write(&legacy, "legacy").unwrap();
        migrate_shortcut_files(&legacy, &current, creator(current.clone())).unwrap();
        assert!(current.exists());
        assert!(!legacy.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn both_existing_removes_legacy() {
        let dir = temp("both");
        reset(&dir);
        let legacy = dir.join("ClipFlow.lnk");
        let current = dir.join("Mnemark.lnk");
        std::fs::write(&legacy, "legacy").unwrap();
        std::fs::write(&current, "current").unwrap();
        migrate_shortcut_files(&legacy, &current, creator(current.clone())).unwrap();
        assert!(current.exists());
        assert!(!legacy.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn failed_creation_retains_legacy() {
        let dir = temp("fail");
        reset(&dir);
        let legacy = dir.join("ClipFlow.lnk");
        let current = dir.join("Mnemark.lnk");
        std::fs::write(&legacy, "legacy").unwrap();
        let err =
            migrate_shortcut_files(&legacy, &current, || Err("boom".to_string())).unwrap_err();
        assert!(err.contains("boom"));
        // The only working legacy shortcut survives a failed creation.
        assert!(legacy.exists());
        assert!(!current.exists());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
