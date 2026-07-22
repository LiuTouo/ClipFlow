# 0002 — Update strategy: updater plugin for installed, manual download for portable

Date: 2026-07-22 · Status: accepted

## Context

One cargo build produces a single exe that ships two ways: bundled into the
NSIS installer and uploaded raw as the portable asset. Both channels should
self-update, but the mechanisms differ fundamentally: tauri-plugin-updater
runs the NSIS installer, which would be wrong (and destructive in intent)
against a portable exe.

## Decision

- **Runtime channel detection** (`update::is_installed_build`): exe path
  under `%LOCALAPPDATA%\Programs` → installed (Tauri v2 NSIS per-user
  default); anything else → portable. A build-time flag is impossible — the
  two artifacts are byte-identical outputs of one build.
- **Installed**: tauri-plugin-updater, minisign-signed artifacts, endpoint
  `releases/latest/download/latest.json`. With `auto_update` on, a background
  task checks ~5s after startup, downloads, verifies, installs, then asks
  once (MessageBoxW, localized) before `tauri::process::restart`. The About
  page offers the same flow manually.
- **Portable**: pure TypeScript on the About page — GitHub releases API →
  semver compare → download the `*-portable.exe` asset (fetch + plugin-fs)
  to `clipflow-update.exe` next to the running exe → user quits and
  overwrites manually. Windows cannot replace a running exe, and no helper
  process is worth the maintenance. No reqwest dependency.
- Config/data stay next to the exe in both channels (the per-user NSIS dir
  is user-writable), preserving the portable config model.
- CI (GitHub Actions, tag-triggered) builds both artifacts + `latest.json`;
  private signing key lives in repo secrets.

## Consequences

- Known limitation: a portable exe hand-placed under
  `%LOCALAPPDATA%\Programs` is misdetected as installed; the updater then
  effectively installs it — messy but self-healing.
- Before the first CI release, `latest.json` 404s; checks fail silently
  (logged), the About page shows "no release yet".
- Releases ≤ 0.2.1 have no update mechanism and must be replaced manually
  one last time.
- Losing the signing private key breaks updates permanently; it must be
  backed up alongside the repo secrets.
