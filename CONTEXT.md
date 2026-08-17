# Mnemark — CONTEXT

## Glossary

### Clip
A unique clipboard entry. Deduplicated by content hash — the same content copied twice is the same Clip with an updated timestamp, never two Clips. Deduplication happens at the monitor layer (Rust side).

**Kinds:** Text | Image | FilePaths

**Properties:**
- `id` — unique identifier
- `kind` — Text, Image, or FilePaths
- `content` — raw content (text string, raw pixel data, or list of file paths)
- `content_hash` — SHA-256 of content (computed on the *pre-truncation* original for Text Clips), used for deduplication
- `preview` — first 200 chars for Text; 200px-wide JPEG thumbnail for Image; file names for FilePaths
- `truncated` — true if this Text Clip exceeded the size limit and was cut. The preview suffix shows `[Truncated, original X KB]`
- `source` — the Source application that owned the foreground window at capture time
- `captured_at` — timestamp of the most recent copy
- `pinned` — whether this Clip is pinned to the top of history
- `byte_size` — content size in bytes (original size before truncation)

**Invariants:**
- Text Clips: stored content is capped at `text_size_limit` (default 100 KB, user-configurable). Content exceeding the limit is truncated at a UTF-8 character boundary; `byte_size` keeps the original pre-truncation size. A truncated Clip is rendered with a warning accent color in the Panel to distinguish it from complete Clips.
- Image Clips: stored as compressed bitmap. A 200px-wide thumbnail is generated on capture. A per-image size limit (`image_size_limit`, default 10 MB, configurable) applies — images exceeding it are compressed or downscaled to fit.
- FilePaths Clips: only the paths are stored, not the file contents.
- Deduplication: no two Clips may share the same `content_hash`. For Text and FilePaths, the hash is computed on the original content. For Image, the hash is a pixel-level SHA-256 of the raw bitmap data — byte-for-byte, not perceptual. Different encodings of the "same" image produce different Clips. A new copy of existing content updates `captured_at` and `source`, then moves the Clip to the top of history.
- Text capacity: max 100 Clips (configurable). When exceeded, the oldest unpinned Clip is evicted.
- Image capacity: dual limit — `image_count_limit` (default 10, configurable) and `image_memory_budget` (default 50 MB, configurable). Whichever limit is hit first triggers eviction of the oldest unpinned Image Clip. Eviction continues until both limits are satisfied.
- Pinned Clips of any kind are never evicted by capacity limits.

### Source
The foreground application window that was active when a Clip was captured.

**Properties:**
- `executable_name` — e.g. `Code.exe`, `chrome.exe`
- `window_title` — the title bar text at capture time
- `icon` — extracted application icon (cached per executable)

### ClipboardMonitor
The background Rust thread that watches the Windows clipboard by polling `GetClipboardSequenceNumber` every 200ms. Runs for the lifetime of the app.

**Behavior:**
- On clipboard change: reads available formats, determines Clip kind by priority (Image > FilePaths > Text), computes content hash, deduplicates, applies exclusion list, applies debounce (200ms), stores valid Clips in History.
- Exclusion list: a set of executable names (e.g. `1Password.exe`, `Bitwarden.exe`, `KeePass.exe`). Clips captured while any of these is the foreground window are discarded.
- Debounce: a clipboard change observed within 200ms of the previous capture is deferred, then captures the latest content once the window passes; if that yields the same content hash first observed inside the window, it is silently dropped (handles double Ctrl+C). Re-copying the same content AFTER the window counts as a new copy — `captured_at` and `source` refresh and the Clip moves to the top.
- Pause: when monitoring is paused (via Tray menu), all clipboard changes are ignored. On resume, the current clipboard content is NOT automatically captured — only new changes are recorded. Paused copies are permanently lost.

### History
The ordered, in-memory collection of all Clips. Managed by the Rust backend, exposed to the frontend via Tauri commands.

**Ordering:** newest `captured_at` first, except pinned Clips which always sort to the top. Within pinned Clips, newest first. A divider separates pinned from unpinned.

**Capacity:** dual limit for images — count (`image_count_limit`, default 10) and memory (`image_memory_budget`, default 50 MB). Text: max 100 Clips (configurable). FilePaths Clips count toward text. Eviction is oldest-unpinned-first on whichever image limit is breached first.

**Persistence:** in-memory by default. Optional SQLite write-through persistence via the `persist` config option (Settings checkbox). The database (`mnemark.db`) lives in the data dir — next to the exe for portable builds, `%APPDATA%\Mnemark` for installed builds (see Portable). When enabled, every capture/delete/pin/eviction is mirrored to SQLite and the History is reloaded on startup. Disabling persistence deletes `mnemark.db`.

### Pin
A marker on a Clip that keeps it at the top of the History, above a visual divider.

**Constraints:**
- Maximum 10 pinned Clips at any time.
- Pinning an 11th Clip fails — the oldest pinned Clip must be unpinned first.
- Pinned Clips are never evicted by capacity limits. They must be explicitly unpinned before eviction is possible.

### Hotkey
The global keyboard shortcut `Ctrl+Shift+V` (default, configurable) that opens the History panel. Registered via Windows `RegisterHotKey` API through Tauri's global shortcut plugin.

**Conflict detection:** on startup and on every hotkey change in Settings, registration is attempted immediately. If `RegisterHotKey` fails (another application owns the combination), the Settings window opens automatically with an inline error: "This combination is already in use." The user must choose a different combination before the panel can be invoked.

### Panel
The floating WebView window that displays the History. Created on first invocation, then reused via hide/show — dismissal hides the window rather than destroying it, so re-opening is instant.

**Open:** triggered by the Hotkey (`Ctrl+Shift+V`).

**Close (dismiss):**
- `Esc` key
- Hotkey pressed again (toggle behavior — if Panel is open, close it)
- Click outside the Panel (blur / focus-loss)
- Click a Clip row → Paste + close
- `Enter` on a selected Clip → Paste + close

**Clip row interaction:**
- Click the main row body → Paste the Clip into the previous application, then close the Panel.
- Click a side action button (📌 Pin, 📋 Copy-only, ⋯ More) → perform that single action, leave the Panel open. These do not dismiss. The Delete action lives inside the More popover menu.
- More menu opens near the row button, closes on outside click / Escape / delete action. Only one menu at a time; closing the panel or changing the clip list dismisses it.

**Every open starts from the top:** the search box is cleared, the first Clip is selected, and the list is scrolled to `scrollTop: 0`. The previous session's scroll position, selection, and search query are discarded on close. The active type filter resets to "All" when `remember_history_filter` is off (default); when on, the in-memory filter survives hide/show but is never persisted to disk and resets when Mnemark exits.

While the Panel is open, new clipboard captures from the ClipboardMonitor still arrive in real time. The list updates without scrolling to the top, preserving the user's current scroll position. Delete, pin, and other in-session actions likewise preserve scroll position — only a close/reopen resets to the top.

### Filter
A five-button segmented control below the search box categorizes Clips by type: **All / Text / Image / Files / Links.** The classification is mutually exclusive — each Clip belongs to exactly one filter category.

**Classification rules:**
- Image: `kind === "Image"`
- Files: `kind === "FilePaths"`
- Links: `kind === "Text"` AND `text_content` trims to a single valid URL with protocol exactly `http:` or `https:`. Embedded URLs, invalid URLs, empty strings, and other schemes (ftp, mailto, etc.) remain Text.
- Text: all remaining `kind === "Text"` Clips

**Behavior:**
- The filter intersects with search: both conditions must pass for a Clip to appear.
- Changing search input or filter resets the keyboard selection to the first visible result.
- Filter buttons support Tab navigation and ArrowLeft / ArrowRight movement with immediate activation.
- When the visible list is empty, the empty state distinguishes "no history at all," "no search matches," and "no Clips in this category yet."
- The `remember_history_filter` config preference (default off) controls whether the selected filter survives Panel hide/show. The filter itself is never persisted.

### Search
Case-insensitive substring matching against Clip preview text, source app name, and source window title. Input in the search box filters the Clip list in real time. No fuzzy matching. No typo tolerance. Zero additional dependencies. Search and the type filter combine: both must match for a Clip to appear.

### Paste
The action of selecting a Clip and inserting it into the previously focused application.

**Two-phase:**
1. Write the Clip's content to the clipboard.
2. Simulate `Ctrl+V` to the previously focused window.

If phase 2 fails (target window vanished, etc.), the content remains on the clipboard for manual `Ctrl+V`.

For FilePaths Clips, phase 1 depends on the `paste_files_as_files` setting (default on): on writes a real CF_HDROP plus a CF_UNICODETEXT companion (non-file targets still get path text); off writes the `;`-joined path text. See `docs/adr/0001-cfhdrop-file-paste.md`.

### Tray
The system tray icon that indicates Mnemark is running. Right-click opens a native context menu: Settings, About, Pause Monitoring, Quit.

### Language
All user-facing UI (Panel, Settings, About, tray menu) is localized via the `language` config option: `zh-TW` (Traditional Chinese, default) or `en`. Frontend pages share one dictionary (`src/i18n.ts`); the Panel re-applies the language whenever it regains focus, and the tray menu labels update immediately when the setting changes.

### Typography
IBM Plex Sans TC, bundled locally under `src/assets/fonts/`. Three weights included as complete hinted WOFF2 files:

| Weight | File | Usage |
|---|---|---|
| Regular 400 | `IBMPlexSansTC-Regular.woff2` | Metadata, hints, secondary text |
| Medium 500 | `IBMPlexSansTC-Medium.woff2` | Primary UI text, body |
| SemiBold 600 | `IBMPlexSansTC-SemiBold.woff2` | Headings, active states, emphasis |

**Provenance:** Official IBM Plex release `@ibm/plex-sans-tc@1.1.1` from https://github.com/IBM/plex. Licensed under SIL Open Font License 1.1 (see `src/assets/fonts/LICENSE.txt`). Covers Traditional Chinese + Latin character sets. No runtime network requests — all font data is embedded in the application bundle at build time. Font stack: `"IBM Plex Sans TC", "Segoe UI", system-ui, -apple-system, sans-serif`.

### Portable
Mnemark runs without installation or registry writes. Startup is achieved via a `.lnk` shortcut in `shell:startup` with `--hidden` flag — no registry Run key.

One exe also ships inside the NSIS installer. Config and data live next to the exe for portable builds; installed builds use `%APPDATA%\Mnemark` because the install dir (e.g. Program Files) is not user-writable. The channel is detected at runtime via the NSIS uninstall registry key; installed builds auto-update via tauri-plugin-updater, portable builds download the new exe from GitHub Releases for manual overwrite (minisign-verified against the embedded updater pubkey before anything is written to disk). See `docs/adr/0002-update-strategy.md`.

---

## Decisions

See `docs/adr/` for architectural decisions that meet the ADR threshold.
