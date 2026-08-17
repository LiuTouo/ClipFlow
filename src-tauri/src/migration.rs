//! One-time migration from the legacy ClipFlow identity to Mnemark.
//!
//! Runs once at startup, before `AppConfig::load` and `Persistence::open`, so
//! the first Mnemark launch reads the migrated files. The migration is a
//! *copy*, never a move: legacy files are retained as a recoverable backup and
//! are only superseded by the new Mnemark files. This keeps it idempotent (the
//! second run sees the destination and skips it) and safe (a failure leaves the
//! legacy data untouched).
//!
//! Installed builds migrate `%APPDATA%\ClipFlow` to `%APPDATA%\Mnemark`; portable
//! builds migrate the legacy sibling files next to the exe. The autostart
//! shortcut is migrated separately in `startup::migrate_legacy_startup_shortcut`
//! so the old `ClipFlow.lnk` is only removed after `Mnemark.lnk` is created.

use std::path::{Path, PathBuf};

/// Legacy → new filename pairs for the data files that carry user state.
const FILE_PAIRS: [(&str, &str); 4] = [
    ("clipflow.config.json", "mnemark.config.json"),
    ("clipflow.config.json.bak", "mnemark.config.json.bak"),
    ("clipflow.config.json.tmp", "mnemark.config.json.tmp"),
    ("clipflow.db", "mnemark.db"),
];

/// Migrate legacy ClipFlow data into the Mnemark identity, if any is present.
/// Returns `Ok(())` after a best-effort migration, or `Err` on a failure that
/// would otherwise risk data loss. Progress and conflicts are logged.
pub fn migrate_legacy_data() -> Result<(), String> {
    let installed = crate::update::is_installed_build();
    let dest_dir = crate::models::data_dir();
    let src_dir = if installed {
        legacy_appdata_dir()
    } else {
        current_exe_dir()?
    };
    migrate_files(&src_dir, &dest_dir)?;
    crate::startup::migrate_legacy_startup_shortcut()?;
    Ok(())
}

/// The legacy installed-build data directory (`%APPDATA%\ClipFlow`).
fn legacy_appdata_dir() -> PathBuf {
    std::env::var_os("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("."))
        .join("ClipFlow")
}

fn current_exe_dir() -> Result<PathBuf, String> {
    std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| "no executable directory".to_string())
}

/// Copy every legacy file present in `src_dir` to its new name in `dest_dir`.
/// A destination that already exists wins (new beats old); the legacy source is
/// retained and the conflict is reported. Any copy failure aborts the remaining
/// files and returns `Err`, leaving all legacy data intact.
fn migrate_files(src_dir: &Path, dest_dir: &Path) -> Result<(), String> {
    for (legacy, new) in FILE_PAIRS {
        migrate_one(src_dir, legacy, dest_dir, new)?;
    }
    Ok(())
}

fn migrate_one(src_dir: &Path, legacy: &str, dest_dir: &Path, new: &str) -> Result<(), String> {
    let src = src_dir.join(legacy);
    let dest = dest_dir.join(new);
    if !src.exists() {
        return Ok(());
    }
    if dest.exists() {
        // New destination wins; keep the legacy file as a recoverable backup.
        crate::log(&format!(
            "[Mnemark] migration: retained legacy {legacy} ({new} already exists)"
        ));
        return Ok(());
    }
    if let Some(parent) = dest.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let copied = std::fs::copy(&src, &dest).map_err(|e| {
        format!(
            "Migration failed to copy {}: {} — legacy data preserved",
            src.display(),
            e
        )
    })?;
    // Confirm the destination before considering the file migrated.
    let src_len = std::fs::metadata(&src).map(|m| m.len()).unwrap_or(0);
    let dest_len = std::fs::metadata(&dest).map(|m| m.len()).unwrap_or(0);
    if dest_len != src_len {
        let _ = std::fs::remove_file(&dest);
        return Err(format!(
            "Migration of {legacy} failed verification — legacy data preserved"
        ));
    }
    crate::log(&format!(
        "[Mnemark] migration: {legacy} -> {new} ({copied} bytes)"
    ));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Unique temp dir per test so tests don't collide across parallel runs.
    fn temp_dir(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!("mnemark-migrate-{name}-{}", std::process::id()))
    }

    fn reset(dir: &Path) -> PathBuf {
        let _ = std::fs::remove_dir_all(dir);
        std::fs::create_dir_all(dir).unwrap();
        dir.to_path_buf()
    }

    #[test]
    fn no_legacy_data_is_a_noop() {
        let base = reset(&temp_dir("none"));
        let src = base.join("src");
        let dest = base.join("dest");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dest).unwrap();
        migrate_files(&src, &dest).unwrap();
        assert!(!dest.join("mnemark.config.json").exists());
        assert!(!dest.join("mnemark.db").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn migrates_config_backup_tmp_and_db() {
        let base = reset(&temp_dir("files"));
        let src = base.join("src");
        let dest = base.join("dest");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(src.join("clipflow.config.json"), r#"{"persist":true}"#).unwrap();
        std::fs::write(src.join("clipflow.config.json.bak"), "backup").unwrap();
        std::fs::write(src.join("clipflow.config.json.tmp"), "tmp").unwrap();
        std::fs::write(src.join("clipflow.db"), "db-bytes").unwrap();

        migrate_files(&src, &dest).unwrap();

        assert_eq!(
            std::fs::read_to_string(dest.join("mnemark.config.json")).unwrap(),
            r#"{"persist":true}"#
        );
        assert_eq!(
            std::fs::read_to_string(dest.join("mnemark.config.json.bak")).unwrap(),
            "backup"
        );
        assert_eq!(
            std::fs::read_to_string(dest.join("mnemark.config.json.tmp")).unwrap(),
            "tmp"
        );
        assert_eq!(
            std::fs::read_to_string(dest.join("mnemark.db")).unwrap(),
            "db-bytes"
        );
        // Legacy sources are retained as a backup.
        assert!(src.join("clipflow.config.json").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn destination_wins_and_legacy_is_retained() {
        let base = reset(&temp_dir("collision"));
        let src = base.join("src");
        let dest = base.join("dest");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(src.join("clipflow.config.json"), "legacy").unwrap();
        std::fs::write(dest.join("mnemark.config.json"), "new").unwrap();

        migrate_files(&src, &dest).unwrap();

        // New destination wins; legacy source stays put.
        assert_eq!(
            std::fs::read_to_string(dest.join("mnemark.config.json")).unwrap(),
            "new"
        );
        assert_eq!(
            std::fs::read_to_string(src.join("clipflow.config.json")).unwrap(),
            "legacy"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn migration_is_idempotent() {
        let base = reset(&temp_dir("idempotent"));
        let src = base.join("src");
        let dest = base.join("dest");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(src.join("clipflow.db"), "db").unwrap();

        migrate_files(&src, &dest).unwrap();
        migrate_files(&src, &dest).unwrap();

        assert_eq!(
            std::fs::read_to_string(dest.join("mnemark.db")).unwrap(),
            "db"
        );
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn copy_failure_preserves_legacy_data() {
        let base = reset(&temp_dir("failure"));
        let src = base.join("src");
        let dest = base.join("dest");
        std::fs::create_dir_all(&src).unwrap();
        std::fs::create_dir_all(&dest).unwrap();
        // A directory named like a legacy file makes fs::copy fail deterministically.
        std::fs::create_dir_all(src.join("clipflow.db")).unwrap();

        let err = migrate_files(&src, &dest).unwrap_err();

        assert!(
            err.contains("preserved"),
            "error should report preservation: {err}"
        );
        assert!(!dest.join("mnemark.db").exists());
        let _ = std::fs::remove_dir_all(&base);
    }
}
