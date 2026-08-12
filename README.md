# ClipFlow

**[English](README.md) | [繁體中文](README.zh-TW.md)**

<p align="center">
  <img src="clipflow_icon.svg" alt="ClipFlow" width="128" />
</p>

<p align="center">
  <strong>A lightweight clipboard history tool for Windows.</strong><br />
  <sub>Fast access to text, images, and copied files without interrupting your workflow.</sub>
</p>

ClipFlow is a Windows 10/11 clipboard manager built with Tauri 2. Press a global shortcut to open a compact floating panel, search recent clips, and paste one back into the app you were using.

It is available as an NSIS installer with background updates or as a portable executable with no installation or registry writes.

## Features

- Capture text, images, and copied file paths
- Search clip content and source application instantly
- Navigate with the keyboard, with optional `j` / `k` Vim controls
- Pin up to 10 clips so capacity limits never evict them
- Paste into the previously focused app or copy without closing the panel
- Paste copied files as actual files (`CF_HDROP`) or as path text
- Undo deleted clips within 3 seconds
- Configure history limits, hotkey, theme, language, and capture debounce
- Exclude clipboard content copied from selected applications
- Pause monitoring from the system tray
- Optionally persist history in SQLite
- Use Traditional Chinese or English throughout the interface

## Requirements

- Windows 10 or Windows 11, 64-bit
- [Microsoft Edge WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/)

WebView2 is included with Windows 11 and most current Windows 10 installations. Stripped-down or LTSC systems may need the Evergreen installer.

## Download

Download the latest version from [GitHub Releases](https://github.com/LiuTouo/ClipFlow/releases/latest):

| Edition | Choose this if | Updates | Data location |
| --- | --- | --- | --- |
| NSIS installer (`*-setup.exe`) | You want a conventional installation | Downloads and installs signed updates in the background when automatic updates are enabled | `%APPDATA%\ClipFlow` |
| Portable (`*-portable.exe`) | You want a standalone executable | Checks and downloads a signed replacement from the About window; you replace the running executable manually | Next to the executable |

The portable edition does not require installation and does not use a registry `Run` entry. Optional startup uses a shortcut in `shell:startup`.

## Quick start

1. Download and run either edition.
2. ClipFlow starts in the system tray without opening a main window.
3. Copy text, an image, or files as usual.
4. Press `Ctrl+Shift+V` to open clipboard history.
5. Select a clip to paste it into the application you were using.

The global shortcut can be changed in Settings. If another application already owns the selected shortcut, ClipFlow opens Settings and asks you to choose a different combination.

## Usage

| Action | Result |
| --- | --- |
| `Ctrl+Shift+V` | Open or close the history panel |
| Arrow keys or `j` / `k` | Move through clips (`j` / `k` requires Vim mode) |
| `Enter` or click a clip | Paste the selected clip and close the panel |
| `Esc` or click outside | Close the panel |
| Pin | Keep a clip at the top and protect it from automatic eviction |
| Copy | Put a clip on the clipboard without closing the panel |
| Delete | Remove a clip and show a 3-second undo action |
| Tray menu | Pause monitoring, open Settings or About, or quit |

Search matches clip previews, source application names, and source window titles without case sensitivity.

For file entries, the default behavior writes an actual Windows file-drop clipboard format, so pasting works like copying files in File Explorer. The original files must still exist. This behavior can be changed to paste path text instead.

## Data and privacy

- Clipboard history is kept in memory by default and is lost when ClipFlow exits.
- Enabling persistence writes history to `clipflow.db`. Disabling it deletes that database.
- Portable configuration and data are stored next to the executable; installed builds use `%APPDATA%\ClipFlow`.
- The default exclusion list contains `1Password.exe`, `Bitwarden.exe`, and `KeePass.exe`. Clips copied while one of these applications is in the foreground are discarded.
- Copies made while monitoring is paused are discarded and are not captured after monitoring resumes.
- Text and image history limits are configurable. When a limit is exceeded, the oldest unpinned clips are removed first.

## Known limitations

- A non-elevated ClipFlow process cannot inject paste input into an application running as administrator because Windows UIPI blocks it. The selected content remains on the clipboard, so you can paste it manually with `Ctrl+V`.
- Application exclusion is based on the foreground application at copy time. Password-manager autofill cannot be identified when the password manager itself is not the foreground application.
- File history stores references to paths, not copies of file contents. Pasting files as files fails if the source files were moved or deleted.

## Build from source

Install [Node.js](https://nodejs.org/), [Rust](https://rustup.rs/), and the Windows prerequisites required by Tauri 2, then run:

```powershell
git clone https://github.com/LiuTouo/ClipFlow.git
cd ClipFlow
npm ci
npm run build:app
```

The release executable is written to `src-tauri/target/release/clipflow.exe`. Use `npm run build:app` for production builds because it enables Tauri's required `custom-protocol` feature.

For development with hot reload:

```powershell
npm run tauri dev
```

Useful validation commands:

```powershell
npm run build
cargo check --manifest-path src-tauri/Cargo.toml
cargo test --manifest-path src-tauri/Cargo.toml
```

## Documentation

- [Changelog](CHANGELOG.md)
- [Behavior and domain reference](CONTEXT.md)
- [Architecture decision records](docs/adr/README.md)

## License

ClipFlow is distributed under the [GNU General Public License v3.0](LICENSE).
