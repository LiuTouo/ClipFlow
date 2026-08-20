# Mnemark

**[繁體中文](README.md) | [English](README.en.md) (current page)**

<p align="center">
  <img src="mnemark_icon.svg" alt="Mnemark" width="128" />
</p>

<p align="center">
  <strong>Find anything you've copied.</strong><br />
  <sub>Pronounced /ˈniː.mɑːrk/ — “NEE-mark”. The name joins <em>mneme</em> (memory) and <em>mark</em>.</sub>
</p>

Everything you copy leaves a mark — Mnemark keeps those marks searchable, so you can always find your way back to something you've copied before.

Mnemark is a clipboard manager for Windows. Press a global shortcut to open a compact floating panel, search your recent clips, and paste one back into the app you were using.

## Requirements

- 64-bit Windows 10 or Windows 11

Mnemark needs the Microsoft Edge WebView2 Runtime. It is already included with Windows 11 and most up-to-date Windows 10 systems; on older or stripped-down installations you can install it for free from the [Microsoft website](https://developer.microsoft.com/microsoft-edge/webview2/).

## Download

Download the latest version from [GitHub Releases](https://github.com/LiuTouo/Mnemark/releases/latest):

| Edition | Choose this if | Updates | Data location |
| --- | --- | --- | --- |
| Installer (`*-setup.exe`) | You want a conventional installation | Downloads and installs signed updates in the background when automatic updates are enabled | `%APPDATA%\Mnemark` |
| Portable (`*-portable.exe`) | You want a standalone executable | Checks and downloads a new executable from the About window; you replace the old file manually | Same folder as the executable |

The portable edition requires no installation and does not write to the registry. Both editions offer an optional launch-at-startup setting.

## Quick start

1. Download and run the installer or the portable executable.
2. Mnemark sits in the system tray — there is no main window.
3. Copy text, images, or files as usual.
4. Press `Ctrl+Shift+V` to open your clipboard history.
5. Select a clip to paste it into the application you were using.

The global shortcut can be changed in Settings. If another application already owns the shortcut you picked, Mnemark opens Settings and asks you to choose a different combination.

## Features

- Capture text, images, and copied files
- Instant search across clip content and source application
- Full keyboard control, with optional `j` / `k` navigation
- Pin important clips (up to 10) so capacity limits never evict them
- Paste back into the previous app, or copy without closing the panel
- Drawers: drag clips into separate drawers to keep them organized
- Optional persistent history
- Exclude clips copied from selected applications such as password managers
- Pause monitoring from the system tray
- Traditional Chinese and English interface
- Adjustable theme and transparency

## Everyday controls

| Action | Result |
| --- | --- |
| `Ctrl+Shift+V` | Open or close the history panel |
| Arrow keys or `j` / `k` | Move through clips (`j` / `k` must be enabled in Settings) |
| `Enter` or click a clip | Paste the selected clip and close the panel |
| `Space` | Toggle the preview of the selected clip |
| `Esc` or click outside | Close the panel |
| Pin | Keep a clip at the top, protected from automatic eviction |
| Copy | Put a clip on the clipboard without closing the panel |
| Delete | Remove a clip, with a 3-second undo |
| Tray menu | Pause monitoring, open Settings or About, or quit |

Search matches clip previews, source application names, and source window titles, case-insensitively.

File clips are pasted as actual files by default, just like copying files in File Explorer (the original files must still exist); Settings can change this to paste path text instead.

## Drawers

Press the left `Alt` key (the default; changeable in Settings) or click the star in the top-right of the history panel to open or close the drawer interface.

- Create multiple drawers, and rename, delete, or reorder them by dragging
- Drag clips from history into a drawer to keep them; duplicate entries are blocked
- Browse, search, preview, copy, and paste inside a drawer, exactly like in the history panel
- Switch back to the full history at any time

## Data and privacy

- Clipboard history is kept in memory by default and disappears when Mnemark exits.
- If you enable persistence in Settings, history is written to a `mnemark.db` file; disabling persistence deletes the stored history.
- The installed edition stores its settings and data in `%APPDATA%\Mnemark`; the portable edition stores them next to the executable.
- 1Password, Bitwarden, and KeePass are excluded by default: content copied while one of them is in the foreground is never recorded. You can edit the exclusion list yourself.
- Copies made while monitoring is paused are discarded and are not captured after monitoring resumes.
- Text and image history limits are configurable. When a limit is exceeded, the oldest unpinned clips are removed first.

## Known limitations

- Windows restricts lower-privilege programs from typing into windows that run as administrator (such as some Task Manager or terminal windows). Mnemark cannot auto-paste into those; the content stays on the clipboard, so you can still press `Ctrl+V` there manually.
- Application exclusion is based on the foreground application at copy time. If a password manager is not in the foreground (for example a browser extension autofilling), its input cannot be identified.
- File history stores path references, not copies of file contents. If the source files were moved or deleted, they cannot be pasted as files.

## More

- [Changelog](CHANGELOG.md)
- [License](LICENSE) (GNU General Public License v3.0)
