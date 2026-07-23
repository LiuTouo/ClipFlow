# 0002 — Update strategy: updater plugin for installed, manual download for portable

Date: 2026-07-22 · Status: accepted

## Context

One cargo build produces a single exe that ships two ways: bundled into the
NSIS installer and uploaded raw as the portable asset. Both channels should
self-update, but the mechanisms differ fundamentally: tauri-plugin-updater
runs the NSIS installer, which would be wrong (and destructive in intent)
against a portable exe.

## Decision

- **Runtime channel detection** (`update::is_installed_build`): reads the
  NSIS uninstall registry key (`HKLM`/`HKCU ...\Uninstall\ClipFlow` →
  `InstallLocation`) and compares it with the exe's directory. Chosen over
  path heuristics after 0.4.0 shipped a `%LOCALAPPDATA%\Programs` check
  that broke on per-machine installs (`C:\Program Files\ClipFlow`) — the
  install dir is user-selectable, the registry key is not.
- **Installed**: tauri-plugin-updater, minisign-signed artifacts, endpoint
  `releases/latest/download/latest.json`. With `auto_update` on, a background
  task checks ~5s after startup, downloads, verifies, installs, then asks
  once (MessageBoxW, localized) before `tauri::process::restart`. The About
  page offers the same flow manually.
- **Portable**: the About page checks the GitHub releases API (CORS-enabled),
  then Rust downloads the new exe via `update::download_portable_update`
  (ureq) to `clipflow-update.exe` next to the running exe — the webview's
  fetch can't follow GitHub's asset CDN redirect (no CORS headers). The
  user quits and overwrites manually; Windows cannot replace a running exe.
- **Portable verification**: CI signs the portable exe with the same minisign
  key as the NSIS updater artifacts (`tauri signer sign`, uploading
  `<asset>.sig`). `download_portable_update` accepts only https URLs on
  `github.com` / `*.githubusercontent.com`, re-validating every redirect
  hop, and writes `clipflow-update.exe` ONLY after the downloaded bytes
  verify against the pubkey embedded in the binary — the webview picks the
  release asset but cannot turn that into an arbitrary-URL file drop.
- **Config/data location**: portable keeps everything next to the exe;
  installed builds use `%APPDATA%\ClipFlow` (the install dir may be
  Program Files, which is not user-writable). See `models::data_dir`.
- CI (GitHub Actions, tag-triggered) builds both artifacts + `latest.json`;
  private signing key lives in repo secrets.

## Consequences

- First-run migration: builds installed before this change wrote config next
  to the (unwritable) exe, so their settings never actually persisted; they
  start fresh in `%APPDATA%\ClipFlow`.
- Builds ≤ 0.4.0 misdetect their channel (path heuristic), so their
  automatic update path is dead — they must be replaced manually once.
- Before the first CI release, `latest.json` 404s; checks fail silently
  (logged), the About page shows "no release yet".
- Releases cut before portable signing existed have no `.sig` asset; the
  About page refuses them with "no signature file". Since updates only move
  forward, this only blocks "updating" toward those older releases.
- Losing the signing private key breaks updates permanently; it must be
  backed up alongside the repo secrets.
