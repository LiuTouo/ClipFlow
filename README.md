# ClipFlow

**[English](README.md) | [繁體中文](README.zh-TW.md)**

<p align="center">
  <img src="clipflow_icon.svg" alt="ClipFlow" width="128" />
</p>

<p align="center">
  <strong>A clipboard that keeps your workflow uninterrupted.</strong><br />
  <sub>Lightweight · Instant · Focused</sub>
</p>

Modern lightweight Windows clipboard history tool. Tauri v2 + Vanilla TS/CSS, Raycast-style floating panel. Fully portable — no installation, no registry writes.

A tool purpose-built for the most frequent action in modern AI VibeCoding — copying.

See `CONTEXT.md` for the domain glossary and behavior spec.

## Features

- **Clipboard monitoring** — Text, Image, and File Paths clips with SHA-256 deduplication
- **Floating panel** — `Ctrl+Shift+V` toggles a transparent, rounded Raycast-style panel; real-time updates
- **Search** — instant case-insensitive substring filtering
- **Keyboard first** — arrows / `Enter` / `Esc`, optional Vim mode (`j`/`k`)
- **Pin** — up to 10 clips pinned above a divider, never evicted
- **Paste** — writes to the clipboard, returns focus to the previous app, simulates `Ctrl+V`; copy-only button per clip
- **File paste** — file entries can paste the actual files (CF_HDROP, like an Explorer copy; source files must still exist), or fall back to pasting path text
- **Auto-update** — installed builds download and install updates in the background (signature-verified); portable builds check and download from the About page for a manual overwrite
- **Delete with undo** — 3-second toast
- **Exclusion list** — clipboard content from password managers (1Password, Bitwarden, KeePass) is never recorded
- **Pause monitoring** — from the tray menu; paused copies are permanently discarded
- **Capacity limits** — configurable text count/size, image count/memory budget; oldest unpinned evicted first
- **Optional SQLite persistence** — write-through to `clipflow.db` next to the exe
- **Autostart** — optional `shell:startup` shortcut, no registry Run key
- **Themes & languages** — dark/light follows system; settings UI in 繁體中文 (default) or English

## Requirements

- Windows 10 / 11 (64-bit)
- [WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/) — preinstalled on Windows 11 and on most up-to-date Windows 10 machines. Only rare stripped/LTSC systems need the small evergreen installer.

## Quick start (portable)

Download from [Releases](https://github.com/LiuTouo/ClipFlow/releases/latest): the portable exe (`*-portable.exe`) or the NSIS installer (`*-setup.exe`, with background auto-update).

1. Copy `clipflow.exe` into its own folder (config and data live next to the exe; the NSIS-installed build uses `%APPDATA%\ClipFlow` instead).
2. Run it — no window appears; ClipFlow lives in the system tray.
3. Press `Ctrl+Shift+V` to open the history panel.

```
ClipFlow\
├── clipflow.exe
├── clipflow.config.json   (auto-generated)
└── clipflow.db            (only when persistence is enabled)
```

## Usage

- `Ctrl+Shift+V` — toggle the history panel (configurable)
- `Esc` / click outside / pick a clip — dismiss the panel
- Click a clip — paste it into the previously focused app
- 📌 pin · 📋 copy-only · 🗑 delete — per-clip side actions (panel stays open)
- Tray icon (right-click) — Pause Monitoring, Settings, About, Quit

## Known limitations

- Paste cannot be injected into apps running **as administrator** (Windows UIPI blocks simulated input from non-elevated processes). The clip stays on the clipboard — press `Ctrl+V` manually.
- The exclusion list matches the foreground app at copy time; password-manager autofill (where the manager is not in the foreground) cannot be excluded.

## Build from source

Prerequisites: [Node.js](https://nodejs.org/) and [Rust](https://rustup.rs/).

```bash
git clone https://github.com/LiuTouo/ClipFlow
cd ClipFlow
npm install
npm run build:app
```

Output: `src-tauri/target/release/clipflow.exe` (~15 MB, frontend assets embedded).

`npm run build:app` runs `npm run build` (tsc + vite → `dist/`) then `cargo build --release --features custom-protocol`. The `custom-protocol` feature is **required for production**: without it Tauri compiles in dev mode and every window tries to load `http://localhost:1420` instead of the embedded assets. Keep it off for `npm run tauri dev` (hot-reload via the vite dev server).

## Dev

```bash
npm run tauri dev
```

## Release

1. `npm run bump -- x.y.z` — propagates the single source of truth (`src-tauri/Cargo.toml`) to package.json and package-lock.json, and inserts a CHANGELOG skeleton.
2. Fill in the `CHANGELOG.md` entry.
3. Commit, `git tag vx.y.z`, `git push --tags` — GitHub Actions builds the NSIS installer, portable exe, and updater `latest.json`, and uploads them to the Release (requires `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` repo secrets).

## Project structure

```
index.html / settings.html / about.html   # pages (vite multi-page)
src/                                      # frontend TS + styles
src-tauri/
  src/
    main.rs          # entry: --hidden flag
    lib.rs           # Tauri core: tray, hotkey, commands, panel lifecycle
    clipboard.rs     # Win32 clipboard capture/write, DIB decode, Ctrl+V
    history.rs       # in-memory history: dedup, limits, eviction, pinning
    models.rs        # Clip + AppConfig (portable JSON config)
    persistence.rs   # optional SQLite write-through store
    startup.rs       # shell:startup .lnk via COM IShellLinkW
    update.rs        # update channel detection, updater commands, background auto-update
```

## Tech stack

Tauri v2 (Rust, `windows` crate for Win32) · Vanilla TypeScript/CSS · vite · rusqlite (bundled SQLite) · image/sha2/base64
