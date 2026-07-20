# ClipFlow — CONTEXT

## Glossary

### Clip
A unique clipboard entry. Deduplicated by content hash — the same content copied twice is the same Clip with an updated timestamp, never two Clips. Deduplication happens at the monitor layer (Rust side).

**Kinds:** Text | Image | FilePaths

**Properties:**
- `id` — unique identifier
- `kind` — Text, Image, or FilePaths
- `content` — raw content (text string, raw pixel data, or list of file paths)
- `content_hash` — SHA-256 of content (computed on the *pre-truncation* original for Text Clips), used for deduplication
- `preview` — first 200 chars for Text; 48×48 JPEG thumbnail for Image; file names for FilePaths
- `truncated` — true if this Text Clip exceeded the size limit and was cut. The preview suffix shows `[Truncated, original X KB]`
- `source` — the Source application that owned the foreground window at capture time
- `captured_at` — timestamp of the most recent copy
- `pinned` — whether this Clip is pinned to the top of history
- `byte_size` — content size in bytes (original size before truncation)

**Invariants:**
- Text Clips: `byte_size` ≤ `text_size_limit` (default 100 KB, user-configurable). Content exceeding the limit is truncated; the original size is preserved in `byte_size`. A truncated Clip is rendered with a warning accent color in the Panel to distinguish it from complete Clips.
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
The background Rust service that watches the Windows clipboard via `AddClipboardFormatListener`. Runs for the lifetime of the app.

**Behavior:**
- On clipboard change: reads available formats, determines Clip kind by priority (Image > Text > FilePaths), computes content hash, deduplicates, applies exclusion list, applies debounce (200ms), stores valid Clips in History.
- Exclusion list: a set of executable names (e.g. `1Password.exe`, `Bitwarden.exe`, `KeePass.exe`). Clips captured while any of these is the foreground window are discarded.
- Debounce: if the same content hash appears within 200ms of the previous capture, the second event is silently dropped (handles double Ctrl+C).
- Pause: when monitoring is paused (via Tray menu), all clipboard changes are ignored. On resume, the current clipboard content is NOT automatically captured — only new changes are recorded. Paused copies are permanently lost.

### History
The ordered, in-memory collection of all Clips. Managed by the Rust backend, exposed to the frontend via Tauri commands.

**Ordering:** newest `captured_at` first, except pinned Clips which always sort to the top. Within pinned Clips, newest first. A divider separates pinned from unpinned.

**Capacity:** dual limit for images — count (`image_count_limit`, default 10) and memory (`image_memory_budget`, default 50 MB). Text: max 100 Clips (configurable). FilePaths Clips count toward text. Eviction is oldest-unpinned-first on whichever image limit is breached first.

**Persistence:** in-memory by default. Optional SQLite persistence via `--persist` flag.

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
The floating WebView window that displays the History. Only exists while invoked — destroyed on close so WebView memory is fully released.

**Open:** triggered by the Hotkey (`Ctrl+Shift+V`).

**Close (dismiss):**
- `Esc` key
- Hotkey pressed again (toggle behavior — if Panel is open, close it)
- Click outside the Panel (blur / focus-loss)
- Click a Clip row → Paste + close
- `Enter` on a selected Clip → Paste + close

**Clip row interaction:**
- Click the main row body → Paste the Clip into the previous application, then close the Panel.
- Click a side action button (📌 Pin, 🗑 Delete, 📋 Copy-only) → perform that single action, leave the Panel open. These do not dismiss.

While the Panel is open, new clipboard captures from the ClipboardMonitor still arrive in real time. The list updates without scrolling to the top, preserving the user's current scroll position.

### Search
Case-insensitive substring matching against Clip preview text. Input in the search box filters the Clip list in real time. No fuzzy matching. No typo tolerance. Zero additional dependencies.

### Paste
The action of selecting a Clip and inserting it into the previously focused application.

**Two-phase:**
1. Write the Clip's content to the clipboard.
2. Simulate `Ctrl+V` to the previously focused window.

If phase 2 fails (target window vanished, etc.), the content remains on the clipboard for manual `Ctrl+V`.

### Tray
The system tray icon that indicates ClipFlow is running. Right-click opens a native context menu: Settings, About, Pause Monitoring, Quit.

### Portable
ClipFlow runs without installation or registry writes. All configuration and data live alongside the executable. Startup is achieved via a `.lnk` shortcut in `shell:startup` with `--hidden` flag — no registry Run key.

---

## Decisions

See `docs/adr/` for architectural decisions that meet the ADR threshold.
