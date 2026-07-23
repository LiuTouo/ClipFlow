import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { setLanguage, applyI18n, t } from "./i18n";
import { applyTheme } from "./theme";

interface Clip {
  id: string;
  kind: "Text" | "Image" | "FilePaths";
  text_content: string | null;
  // Raw image bytes never cross IPC — paste/copy fetch them by id.
  thumbnail_base64: string | null;
  content_hash: string;
  preview: string;
  truncated: boolean;
  source_exe: string;
  source_title: string;
  source_icon: string | null;
  captured_at: number;
  pinned: boolean;
  byte_size: number;
}

interface ClipboardUpdate {
  clip: Clip;
  evicted: string[];
}

let clips: Clip[] = [];
// The search-filtered view of clips, in display order. Keyboard selection
// indexes into this — never into `clips` directly, or search + Enter pastes
// the wrong item.
let visibleClips: Clip[] = [];
let selectedIndex = -1;
let vimMode = false;
let pasteFilesAsFiles = true;
let toastTimer: ReturnType<typeof setTimeout> | null = null;

const searchInput = document.getElementById("search-input") as HTMLInputElement;
const clipList = document.getElementById("clip-list")!;
const emptyState = document.getElementById("empty-state")!;
const emptyTitle = document.getElementById("empty-title")!;
const emptyHint = document.getElementById("empty-hint")!;
const toast = document.getElementById("toast")!;

// === Init ===
/** Pull the live config into the page: language, vim mode, theme. */
async function refreshConfig() {
  try {
    const config = await invoke<{ language?: string; vim_mode?: boolean; theme?: string; paste_files_as_files?: boolean }>("get_config");
    setLanguage(config.language || "zh-TW");
    vimMode = !!config.vim_mode;
    pasteFilesAsFiles = config.paste_files_as_files !== false;
    applyTheme(config.theme || "system");
  } catch (_) {
    setLanguage("zh-TW");
  }
}

/** Pinned first, then newest — matches backend HistoryStore::get_all. */
function sortClips() {
  clips.sort((a, b) => Number(b.pinned) - Number(a.pinned) || b.captured_at - a.captured_at);
}

async function init() {
  await refreshConfig();
  applyI18n();

  clips = await invoke("get_clips");
  render();

  // The Panel is reused via hide/show — re-apply the config every time it
  // regains focus so changes made in Settings take effect on next open.
  await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
    if (focused) {
      refreshConfig().then(() => {
        applyI18n();
        render();
      });
    }
  });

  // Listen for clipboard updates
  await listen<ClipboardUpdate>("clipboard-update", (event) => {
    const { clip, evicted } = event.payload;
    // Dedup locally
    const existingIndex = clips.findIndex(c => c.content_hash === clip.content_hash);
    if (existingIndex >= 0) {
      clips[existingIndex] = clip;
    } else {
      clips.unshift(clip);
    }
    // Drop clips the backend evicted by capacity limits (possibly the new
    // clip itself), so the panel never shows ghosts.
    if (evicted.length > 0) {
      clips = clips.filter(c => !evicted.includes(c.id));
    }
    sortClips();
    render();
  });
}

// === Render ===
function render() {
  const query = searchInput.value.toLowerCase();
  const filtered = clips.filter(c => {
    if (!query) return true;
    return c.preview.toLowerCase().includes(query)
      || c.source_exe.toLowerCase().includes(query)
      || c.source_title.toLowerCase().includes(query);
  });
  visibleClips = filtered;

  // Selection indexes into visibleClips — keep it in range after any
  // filter or list change (delete, eviction, new search).
  if (selectedIndex >= visibleClips.length) {
    selectedIndex = visibleClips.length - 1;
  }

  clipList.innerHTML = "";
  const searching = query.length > 0;
  const showEmpty = visibleClips.length === 0;
  emptyState.classList.toggle("hidden", !showEmpty);
  if (showEmpty) {
    // "No history yet" and "no search matches" are different states —
    // show the honest one.
    emptyTitle.textContent = searching ? t("noResults") : t("emptyTitle");
    emptyHint.classList.toggle("hidden", searching);
  }

  let hasPinned = false;
  let hasUnpinned = false;

  filtered.forEach((clip, index) => {
    if (clip.pinned && !hasPinned) {
      hasPinned = true;
    }
    if (!clip.pinned && !hasUnpinned && hasPinned) {
      // Insert divider
      const divider = document.createElement("div");
      divider.className = "pinned-divider";
      divider.textContent = t("pinnedDivider");
      clipList.appendChild(divider);
      hasUnpinned = true;
    }

    const el = document.createElement("div");
    el.className = `clip-item${clip.truncated ? " truncated" : ""}${index === selectedIndex ? " selected" : ""}`;
    el.dataset.index = String(index);
    el.addEventListener("click", (e) => {
      // Don't paste if clicking action buttons
      const target = e.target as HTMLElement;
      if (target.closest(".clip-action-btn")) return;
      pasteClip(clip);
    });

    // Icon / Thumbnail
    const iconDiv = document.createElement("div");

    if (clip.kind === "Image" && clip.thumbnail_base64) {
      iconDiv.className = "thumbnail-container";
      const img = document.createElement("img");
      img.src = clip.thumbnail_base64;
      img.alt = "Image";
      iconDiv.appendChild(img);
    } else if (clip.kind === "FilePaths") {
      iconDiv.className = "clip-icon text-icon";
      iconDiv.innerHTML = `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/></svg>`;
    } else {
      iconDiv.className = "clip-icon text-icon";
      iconDiv.innerHTML = `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>`;
    }

    el.appendChild(iconDiv);

    // Content
    const contentDiv = document.createElement("div");
    contentDiv.className = "clip-content";

    const title = document.createElement("div");
    title.className = "clip-title";
    let titleText = clip.preview || "(empty)";
    if (clip.kind === "Image") {
      titleText = t("imageClip");
    } else if (clip.kind === "Text") {
      titleText = titleText.replace(/\n/g, " ");
    }
    title.textContent = titleText;
    contentDiv.appendChild(title);

    const meta = document.createElement("div");
    meta.className = "clip-meta";
    const source = document.createElement("span");
    source.className = "source";
    source.textContent = !clip.source_exe || clip.source_exe === "Unknown"
      ? t("unknownSource")
      : clip.source_exe;
    meta.appendChild(source);

    const size = document.createElement("span");
    size.textContent = clip.kind === "Image"
      ? `${(clip.byte_size / 1024 / 1024).toFixed(1)}MB`
      : `${clip.byte_size} B`;
    meta.appendChild(size);

    contentDiv.appendChild(meta);
    el.appendChild(contentDiv);

    // Time
    const time = document.createElement("span");
    time.className = "clip-time";
    time.textContent = formatTime(clip.captured_at);
    el.appendChild(time);

    // Actions
    const actions = document.createElement("div");
    actions.className = "clip-actions";

    const pinBtn = document.createElement("button");
    pinBtn.className = `clip-action-btn pin-btn${clip.pinned ? " pinned" : ""}`;
    pinBtn.innerHTML = "📌";
    pinBtn.title = clip.pinned ? t("unpinTitle") : t("pinTitle");
    pinBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      togglePin(clip);
    });
    actions.appendChild(pinBtn);

    const copyBtn = document.createElement("button");
    copyBtn.className = "clip-action-btn";
    copyBtn.innerHTML = "📋";
    copyBtn.title = t("copyOnlyTitle");
    copyBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      copyOnly(clip);
    });
    actions.appendChild(copyBtn);

    const deleteBtn = document.createElement("button");
    deleteBtn.className = "clip-action-btn delete-btn";
    deleteBtn.innerHTML = "🗑";
    deleteBtn.title = t("deleteTitle");
    deleteBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      deleteClip(clip);
    });
    actions.appendChild(deleteBtn);

    el.appendChild(actions);
    clipList.appendChild(el);
  });

  // Scroll selected into view
  if (selectedIndex >= 0) {
    const selected = clipList.querySelector(".clip-item.selected");
    selected?.scrollIntoView({ block: "nearest" });
  }
}

// === Actions ===
async function pasteClip(clip: Clip) {
  // The backend writes to the clipboard, hides the Panel (returning focus to
  // the previous app), then simulates Ctrl+V.
  try {
    switch (clip.kind) {
      case "Text":
        await invoke("paste_text", { text: clip.text_content || "" });
        break;
      case "FilePaths":
        if (pasteFilesAsFiles) {
          // Panel hides during paste — a fallback toast would be invisible.
          await invoke<string>("paste_files", { text: clip.text_content || "" });
        } else {
          await invoke("paste_text", { text: clip.text_content || "" });
        }
        break;
      case "Image":
        await invoke("paste_image", { id: clip.id });
        break;
    }
  } catch (err) {
    console.error("Paste failed:", err);
  }
}

async function copyOnly(clip: Clip) {
  try {
    let toastKey = "copied";
    switch (clip.kind) {
      case "Text":
        await invoke("copy_only_text", { text: clip.text_content || "" });
        break;
      case "FilePaths":
        if (pasteFilesAsFiles) {
          const outcome = await invoke<string>("copy_only_files", { text: clip.text_content || "" });
          if (outcome === "text") toastKey = "filesMissingFallback";
        } else {
          await invoke("copy_only_text", { text: clip.text_content || "" });
        }
        break;
      case "Image":
        await invoke("copy_only_image", { id: clip.id });
        break;
    }
    showToast(t(toastKey));
  } catch (err) {
    console.error("Copy failed:", err);
  }
}

async function deleteClip(clip: Clip) {
  const removeLocal = () => {
    clips = clips.filter(c => c.id !== clip.id);
    render(); // render() clamps selectedIndex against visibleClips
  };
  try {
    await invoke("delete_clip", { id: clip.id });
    removeLocal();

    // Show undo toast
    showToast(t("deleted"), async () => {
      try {
        await invoke("undo_delete", { id: clip.id });
        clips = await invoke("get_clips");
        render();
      } catch (err) {
        // Stale undo — a newer delete already superseded this one.
        showToast(String(err));
      }
    });
  } catch (err) {
    // Already gone in the backend (e.g. evicted) — sync the local list
    // instead of leaving a ghost entry.
    if (String(err).includes("Clip not found")) {
      removeLocal();
    } else {
      console.error("Delete failed:", err);
    }
  }
}

async function togglePin(clip: Clip) {
  try {
    await invoke("set_pinned", { id: clip.id, pinned: !clip.pinned });
    clip.pinned = !clip.pinned;
    sortClips();
    render();
  } catch (err) {
    showToast(String(err));
  }
}

async function closePanel() {
  await getCurrentWindow().hide();
}

// === Toast ===
function showToast(message: string, onUndo?: () => void) {
  if (toastTimer) clearTimeout(toastTimer);

  toast.innerHTML = "";
  const span = document.createElement("span");
  span.textContent = message;
  toast.appendChild(span);

  if (onUndo) {
    const undoBtn = document.createElement("button");
    undoBtn.className = "undo-btn";
    undoBtn.textContent = t("undo");
    undoBtn.addEventListener("click", () => {
      onUndo();
      hideToast();
    });
    toast.appendChild(undoBtn);
  }

  toast.classList.remove("hidden");

  toastTimer = setTimeout(() => {
    hideToast();
  }, 4000);
}

function hideToast() {
  toast.classList.add("hidden");
  if (toastTimer) clearTimeout(toastTimer);
}

// === Formatting ===
function formatTime(ts: number): string {
  const now = Date.now();
  const diff = now - ts;
  const sec = Math.floor(diff / 1000);
  const min = Math.floor(sec / 60);
  const hr = Math.floor(min / 60);

  if (sec < 60) return t("justNow");
  if (min < 60) return t("minutesAgo", { n: min });
  if (hr < 24) return t("hoursAgo", { n: hr });
  const days = Math.floor(hr / 24);
  return t("daysAgo", { n: days });
}

// === Keyboard Navigation ===
function moveSelection(delta: number) {
  selectedIndex = Math.min(Math.max(selectedIndex + delta, 0), visibleClips.length - 1);
  render();
}

function pasteSelected() {
  if (selectedIndex >= 0 && selectedIndex < visibleClips.length) {
    pasteClip(visibleClips[selectedIndex]);
  }
}

// Bound on document so vim navigation keeps working after the search box is
// blurred. j/k only navigate when the search box is NOT focused — otherwise
// vim mode would make the letters j/k untypeable in search.
document.addEventListener("keydown", (e) => {
  const inSearch = document.activeElement === searchInput;

  switch (e.key) {
    case "ArrowDown":
      e.preventDefault();
      moveSelection(1);
      return;
    case "ArrowUp":
      e.preventDefault();
      moveSelection(-1);
      return;
    case "Enter":
      e.preventDefault();
      pasteSelected();
      return;
    case "Escape":
      e.preventDefault();
      // Vim mode: first Escape blurs the search box into navigation mode,
      // the next Escape closes the Panel.
      if (inSearch && vimMode) {
        searchInput.blur();
      } else {
        closePanel();
      }
      return;
  }

  if (!inSearch) {
    if (vimMode && (e.key === "j" || e.key === "k")) {
      e.preventDefault();
      moveSelection(e.key === "j" ? 1 : -1);
      return;
    }
    // Any printable character refocuses the search box; focusing during
    // keydown lets Chromium deliver the char into the input.
    if (e.key.length === 1 && !e.ctrlKey && !e.altKey && !e.metaKey) {
      searchInput.focus();
    }
  }
});

// Reset selected on new search input
searchInput.addEventListener("input", () => {
  selectedIndex = 0;
  render();
});

// Clicks on the transparent margin around the panel dismiss it.
document.body.addEventListener("click", (e) => {
  if (e.target === document.body || e.target === document.documentElement) {
    closePanel();
  }
});

// Focus-loss dismissal is handled on the Rust side (WindowEvent::Focused).

// === Initialize ===
window.addEventListener("DOMContentLoaded", init);
