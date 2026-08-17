# 0003 — Rebrand to Mnemark and migrate legacy ClipFlow data

Date: 2026-08-17 · Status: accepted

## Context

The product was renamed from ClipFlow to Mnemark (GitHub `LiuTouo/Mnemark`). The
new identity changes the executable, config, database, updater, and autostart
artifact names, which means existing ClipFlow installs and portable copies would
silently lose their settings, history, and autostart shortcut unless the first
Mnemark launch migrates them.

## Decision

**New identity.** Name **Mnemark** (`/ˈniː.mɑːrk/` — "NEE-mark"), tagline
"Find anything you've copied.", meaning *mneme* (Greek for memory) + *mark*.
Artifact contracts: `mnemark.exe`, `mnemark.config.json`, `mnemark.db`,
`mnemark-update.exe`, and `Mnemark.lnk`. App identifier `com.mnemark.app`;
GitHub repository `LiuTouo/Mnemark`.

**Data migration (Rust, one-time, startup).** `migration::migrate_legacy_data`
runs once before `AppConfig::load` and `Persistence::open`. It copies — never
moves — each legacy file to its new name:

| Legacy | New |
| --- | --- |
| `clipflow.config.json` | `mnemark.config.json` |
| `clipflow.config.json.bak` | `mnemark.config.json.bak` |
| `clipflow.config.json.tmp` | `mnemark.config.json.tmp` |
| `clipflow.db` | `mnemark.db` |

Installed builds migrate `%APPDATA%\ClipFlow` → `%APPDATA%\Mnemark`; portable
builds migrate sibling files next to the exe. Autostart migration
(`startup::migrate_legacy_startup_shortcut`) renames `ClipFlow.lnk` →
`Mnemark.lnk`, removing the legacy shortcut only after the new one exists.

**Conflict rules.** Migration is copy-based and idempotent: the legacy source is
retained as a recoverable backup; a pre-existing destination wins (new beats
old); any copy failure aborts the remaining files and returns an error so legacy
data is never lost. The copied size is verified before a file counts as migrated.

**Installer cleanup.** The NSIS installer hooks (`installer-hooks.nsh`) detect
the legacy ClipFlow uninstall record (HKCU then HKLM) and run its uninstaller in
PREINSTALL; they abort the Mnemark install if the legacy install cannot be
removed, preventing a silent side-by-side install.

**Frontend key migration.** The preview-hint localStorage key moves from
`clipflow.previewHintSeen.v1` to `mnemark.previewHintSeen.v1`, with a one-time
read-new → read-legacy → write-new → remove-legacy migration.

## Consequences

- Legacy `ClipFlow`/`clipflow` strings remain only in the migration and installer
  cleanup code, the historical CHANGELOG release prose and pre-0.6.0 compare
  links, and the historical ADRs (0002). New code and active docs use Mnemark.
- The migration is a copy, not a move, so a downgrade or re-run never destroys
  legacy data; the legacy files are simply never read again.
- The old release artifact convention (`ClipFlow_<tag>_x64-portable.exe`) is
  superseded by `Mnemark_<tag>_x64-portable.exe`; the About-page portable
  updater matches the new asset name pattern.
