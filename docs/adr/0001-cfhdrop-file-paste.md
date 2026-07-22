# 0001 — Paste file entries as real files via CF_HDROP path reference

Date: 2026-07-22 · Status: accepted

## Context

FilePaths Clips historically stored only the `;`-joined path text, and pasting
wrote that text as CF_UNICODETEXT. Users wanted "paste the actual file(s)"
from history.

Two candidate semantics:

1. **Byte snapshot** — read file contents into the history at copy time.
   Survives source deletion, but large files inflate memory and the SQLite
   blob, needs a size budget and schema migration, and the snapshot silently
   goes stale when the source changes.
2. **Path reference (CF_HDROP)** — store paths only (as today); at paste time
   write a real CF_HDROP (DROPFILES + double-NUL-terminated UTF-16 list).
   The target app reads the files from their original locations — exactly
   what Windows itself does for an Explorer copy (Explorer never puts file
   bytes on the clipboard either).

## Decision

Path reference. `clipboard::write_files_to_clipboard` sets CF_HDROP plus a
CF_UNICODETEXT companion (paths joined with `\r\n`) so non-file targets
(Notepad) still receive something. Paths that vanished are filtered; when
ALL are gone the paste falls back to the path text (toast on copy-only).
Gated by the `paste_files_as_files` config toggle (default on); off restores
the old paste-path-text behavior.

## Consequences

- Zero memory/storage growth — the originally requested "uses more memory"
  warning was dropped because memory does not increase.
- Paste fails for sources deleted/moved after the copy — identical to
  Explorer's native copy/paste behavior, so acceptable.
- `;`-joined storage means filenames containing `;` split incorrectly
  (pre-existing format limitation, unchanged).
