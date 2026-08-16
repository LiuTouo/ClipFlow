import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { setLanguage, applyI18n, t, localizeBackendError } from "./i18n";
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

type FilterKind = "all" | "text" | "image" | "files" | "links";

let clips: Clip[] = [];
// The search-filtered view of clips, in display order. Keyboard selection
// indexes into this — never into `clips` directly, or search + Enter pastes
// the wrong item.
let visibleClips: Clip[] = [];
let selectedIndex = -1;
let vimMode = false;
let pasteFilesAsFiles = true;
let rememberHistoryFilter = false;
let activeFilter: FilterKind = "all";
let openMenuClipId: string | null = null;
let toastTimer: ReturnType<typeof setTimeout> | null = null;
let hoveredClipId: string | null = null;
let spacePressed = false;
let pointerOverPreview = false;
let previewHideTimer: ReturnType<typeof setTimeout> | null = null;
// Whether the search box had focus when the Hover+Space preview latched.
// Used to restore focus on release only when it was focused before preview.
let searchFocusedBeforePreview = false;

const searchInput = document.getElementById("search-input") as HTMLInputElement;
const filterBar = document.getElementById("filter-bar")!;
const clipList = document.getElementById("clip-list")!;
const emptyState = document.getElementById("empty-state")!;
const emptyTitle = document.getElementById("empty-title")!;
const emptyHint = document.getElementById("empty-hint")!;
const toast = document.getElementById("toast")!;
const actionMenu = document.getElementById("clip-action-menu")!;

// === Link classification ===
/** True when text_content is trimmed to a single valid http/https URL. */
function isLink(text: string | null): boolean {
  if (!text) return false;
  const trimmed = text.trim();
  try {
    const url = new URL(trimmed);
    return url.protocol === "http:" || url.protocol === "https:";
  } catch {
    return false;
  }
}

/** Classify a Clip for filter matching. */
function classifyClip(clip: Clip): FilterKind {
  if (clip.kind === "Image") return "image";
  if (clip.kind === "FilePaths") return "files";
  if (isLink(clip.text_content)) return "links";
  return "text";
}

/** Does this Clip pass the active filter? */
function matchesFilter(clip: Clip, filter: FilterKind): boolean {
  if (filter === "all") return true;
  return classifyClip(clip) === filter;
}

// === Init ===
/** Pull the live config into the page: language, vim mode, theme, filter pref. */
async function refreshConfig() {
  try {
    const config = await invoke<{
      language?: string;
      vim_mode?: boolean;
      theme?: string;
      ui_opacity_percent?: number;
      paste_files_as_files?: boolean;
      remember_history_filter?: boolean;
    }>("get_config");
    setLanguage(config.language || "zh-TW");
    vimMode = !!config.vim_mode;
    pasteFilesAsFiles = config.paste_files_as_files !== false;
    rememberHistoryFilter = !!config.remember_history_filter;
    applyTheme(config.theme || "system");
    const opacity = Math.min(100, Math.max(50, config.ui_opacity_percent ?? 96));
    document.documentElement.style.setProperty("--panel-opacity", String(opacity / 100));
  } catch (err) {
    console.error("Failed to load config:", err);
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
  selectedIndex = 0;
  render();

  // The Panel is reused via hide/show — re-apply the config every time it
  // regains focus so changes made in Settings take effect on next open.
  await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
    if (!focused) return; // composite external-focus-loss hiding is backend-owned
    // Main regained focus: hide any preview left active while focus was in the
    // preview window and reset hover/pointer/timer state. The hide invoke is
    // issued before any later Space keydown can request a new show, so a fresh
    // show stays authoritative. The physical Space latch is intentionally left
    // intact so a held Space keeps suppressing repeat input.
    resetPreviewOnFocus();
    refreshConfig().then(() => {
      applyI18n();
      searchInput.value = "";
      selectedIndex = 0;
      openMenuClipId = null;
      hideActionMenu();
      if (!rememberHistoryFilter) {
        activeFilter = "all";
      }
      render();
      clipList.scrollTop = 0;
    });
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

  // Preview window ↔ main panel sync: the preview reports its pointer
  // enter/leave and its own Space-release so a hovered preview never gets
  // stuck while the cursor crosses between windows.
  await listen<boolean>("clip-preview-pointer", (event) => {
    pointerOverPreview = event.payload;
    if (!pointerOverPreview && spacePressed && hoveredClipId === null) {
      schedulePreviewHide();
    }
  });
  await listen("clip-preview-space-released", () => {
    releasePreview();
  });
}

// === Filter bar ===
function setFilter(filter: FilterKind) {
  if (filter === activeFilter) return;
  activeFilter = filter;
  selectedIndex = 0;
  render();
}

function updateFilterBar() {
  filterBar.querySelectorAll(".filter-btn").forEach((btn) => {
    const el = btn as HTMLButtonElement;
    const filter = el.dataset.filter;
    const isActive = filter === activeFilter;
    el.classList.toggle("active", isActive);
    el.setAttribute("aria-pressed", String(isActive));
  });
  // Keep aria-label in sync with current language.
  filterBar.setAttribute("aria-label", t("filterBarLabel"));
}

// === Render ===
function render() {
  const query = searchInput.value.toLowerCase();

  // Combine search + filter: search narrows, filter categorizes
  const filtered = clips.filter(c => {
    if (!matchesFilter(c, activeFilter)) return false;
    if (!query) return true;
    return c.preview.toLowerCase().includes(query)
      || c.source_exe.toLowerCase().includes(query)
      || c.source_title.toLowerCase().includes(query);
  });
  visibleClips = filtered;

  // If the hovered row was removed (search/filter change, delete, or
  // eviction), drop its hover state and any preview it was showing.
  if (hoveredClipId && !visibleClips.some(c => c.id === hoveredClipId)) {
    hoveredClipId = null;
    if (spacePressed) hidePreview();
  }

  // Selection indexes into visibleClips — keep it in range after any
  // filter or list change (delete, eviction, new search).
  // -1 means no selection (empty result set); any non-negative value
  // is clamped into [0, visibleClips.length).
  if (visibleClips.length === 0) {
    selectedIndex = -1;
  } else {
    if (selectedIndex < 0) selectedIndex = 0;
    if (selectedIndex >= visibleClips.length) selectedIndex = visibleClips.length - 1;
  }

  // Close stale menus (DOM is rebuilt, old More button is gone).
  openMenuClipId = null;
  hideActionMenu();

  // Preserve the scroll position across the rebuild.
  const scrollTop = clipList.scrollTop;
  clipList.innerHTML = "";

  const searching = query.length > 0;
  const filtering = activeFilter !== "all";
  const showEmpty = visibleClips.length === 0;
  const totalEmpty = clips.length === 0;

  emptyState.classList.toggle("hidden", !showEmpty);
  if (showEmpty) {
    if (totalEmpty) {
      emptyTitle.textContent = t("emptyTitle");
      emptyHint.classList.remove("hidden");
    } else if (searching || filtering) {
      emptyTitle.textContent = searching && filtering
        ? t("noResults")
        : filtering && !searching
          ? t("categoryEmpty")
          : t("noResults");
      emptyHint.classList.add("hidden");
    }
  }

  updateFilterBar();

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
    el.dataset.clipId = clip.id;

    // Click row body = paste. Action buttons stop propagation.
    el.addEventListener("click", () => {
      pasteClip(clip);
    });

    // Hover tracking for the Hover+Space preview (does not alter paste).
    el.addEventListener("pointerenter", () => {
      hoveredClipId = clip.id;
      if (spacePressed) showPreview(clip.id);
    });
    el.addEventListener("pointerleave", () => {
      if (hoveredClipId === clip.id) hoveredClipId = null;
      if (spacePressed) schedulePreviewHide();
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
    } else if (isLink(clip.text_content)) {
      iconDiv.className = "clip-icon text-icon";
      iconDiv.innerHTML = `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71"/><path d="M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71"/></svg>`;
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

    // Actions — always-visible SVG buttons
    const actions = document.createElement("div");
    actions.className = "clip-actions";

    // Pin
    const pinBtn = document.createElement("button");
    pinBtn.className = `clip-action-btn pin-btn${clip.pinned ? " pinned" : ""}`;
    pinBtn.innerHTML = `<svg width="15" height="15" viewBox="0 0 24 24" fill="${clip.pinned ? "currentColor" : "none"}" stroke="currentColor" stroke-width="2"><path d="M12 2v8M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2"/><path d="M4 6h16"/><path d="M10 10v8a2 2 0 0 0 2 2 2 2 0 0 0 2-2v-8"/></svg>`;
    pinBtn.title = clip.pinned ? t("unpinTitle") : t("pinTitle");
    pinBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      togglePin(clip);
    });
    actions.appendChild(pinBtn);

    // Copy
    const copyBtn = document.createElement("button");
    copyBtn.className = "clip-action-btn";
    copyBtn.innerHTML = `<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>`;
    copyBtn.title = t("copyOnlyTitle");
    copyBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      copyOnly(clip);
    });
    actions.appendChild(copyBtn);

    // More (opens menu with Delete)
    const moreBtn = document.createElement("button");
    moreBtn.className = "clip-action-btn more-btn";
    moreBtn.innerHTML = `<svg width="15" height="15" viewBox="0 0 24 24" fill="currentColor" stroke="none"><circle cx="5" cy="12" r="2"/><circle cx="12" cy="12" r="2"/><circle cx="19" cy="12" r="2"/></svg>`;
    moreBtn.title = t("moreTitle");
    moreBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      toggleActionMenu(clip.id, moreBtn);
    });
    actions.appendChild(moreBtn);

    el.appendChild(actions);
    clipList.appendChild(el);
  });

  // Restore scroll position; keyboard navigation (selected item) wins
  // and scrolls the selection into view instead.
  clipList.scrollTop = scrollTop;
  if (selectedIndex >= 0) {
    const selected = clipList.querySelector(".clip-item.selected");
    selected?.scrollIntoView({ block: "nearest" });
  }
}

// === Action Menu ===
function toggleActionMenu(clipId: string, anchor: HTMLElement) {
  if (openMenuClipId === clipId) {
    hideActionMenu();
    return;
  }
  openMenuClipId = clipId;
  const rect = anchor.getBoundingClientRect();
  const panelRect = document.getElementById("panel")!.getBoundingClientRect();
  actionMenu.classList.remove("hidden");
  actionMenu.style.top = `${rect.top - panelRect.top + rect.height + 4}px`;
  actionMenu.style.right = `${panelRect.right - rect.right}px`;
}

function hideActionMenu() {
  openMenuClipId = null;
  actionMenu.classList.add("hidden");
}

actionMenu.addEventListener("click", (e) => {
  e.stopPropagation();
  const btn = (e.target as HTMLElement).closest(".menu-item-delete");
  if (btn && openMenuClipId) {
    const clip = clips.find(c => c.id === openMenuClipId);
    if (clip) deleteClip(clip);
    hideActionMenu();
  }
});

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
    showToast(t("pasteFailed"));
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
    showToast(t("copyFailed"));
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
        showToast(localizeBackendError(String(err)));
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
    showToast(localizeBackendError(String(err)));
  }
}

async function closePanel() {
  cancelPreview();
  await getCurrentWindow().hide();
}

// === Hover+Space Preview ===
// Point at a row and hold Space to open a side preview window. Hover alone
// never opens it; Space with no hovered row stays normal search input.
function cancelPreviewHide() {
  if (previewHideTimer) {
    clearTimeout(previewHideTimer);
    previewHideTimer = null;
  }
}

function showPreview(id: string) {
  cancelPreviewHide();
  invoke("show_clip_preview", { id }).catch((err) => {
    console.error("Failed to show preview:", err);
  });
}

function hidePreview() {
  cancelPreviewHide();
  pointerOverPreview = false;
  invoke("hide_clip_preview").catch((err) => {
    console.error("Failed to hide preview:", err);
  });
}

// Bridge the physical gap between a row and the separate preview window:
// leaving a row while Space is still held waits a short grace for the cursor
// to land in the preview (which emits clip-preview-pointer=true) before hiding.
function schedulePreviewHide() {
  cancelPreviewHide();
  previewHideTimer = setTimeout(() => {
    previewHideTimer = null;
    if (spacePressed && hoveredClipId === null && !pointerOverPreview) {
      hidePreview();
    }
  }, 150);
}

/** Reset preview visibility + local hover/pointer/timer state. Does NOT touch
 * the physical Space latch (spacePressed) or search focus — those clear only on
 * an authoritative release, so a held Space keeps suppressing repeat input
 * across focus gain, backend preview hide, and pointer leave. */
function resetLocalPreviewState() {
  hoveredClipId = null;
  pointerOverPreview = false;
  cancelPreviewHide();
}

/** Restore search focus after the preview latch clears, only when the search
 * box was focused before preview and no other control/window took focus.
 * Deferred to a microtask so the released Space keyup is fully processed first
 * and cannot insert into the freshly-focused input. */
function restoreSearchFocus() {
  queueMicrotask(() => {
    const active = document.activeElement;
    if (active && active !== searchInput && active !== document.body && active !== document.documentElement) {
      return; // another control intentionally holds focus
    }
    if (!document.hasFocus()) return; // panel no longer the active window
    searchInput.focus();
  });
}

/** Cancel the latch without restoring focus — panel close/destroy. */
function cancelPreview() {
  spacePressed = false;
  resetLocalPreviewState();
  searchFocusedBeforePreview = false;
  hidePreview();
}

/** Authoritative release (physical Space keyup, clip-preview-space-released,
 * or Escape). Cancels the latch and restores search focus only when it was
 * focused before the preview latched. */
function releasePreview() {
  const restore = searchFocusedBeforePreview;
  cancelPreview();
  if (restore) restoreSearchFocus();
}

/** Main window regained focus. When the physical Space latch is active, do
 * nothing to preview/hover/pointer state and leave search unfocused — the held
 * Space keeps its preview and keeps suppressing repeat input across the focus
 * transition the unfocused preview show triggers. Only with no latch does a
 * stale preview/local state get cleaned up. */
function resetPreviewOnFocus() {
  if (spacePressed) return;
  resetLocalPreviewState();
  hidePreview();
}

/** True when the event is the Spacebar, by physical code first (stable across
 * layouts and modifiers) with the standard key value as a fallback. */
function isSpaceKey(e: KeyboardEvent): boolean {
  return e.code === "Space" || e.key === " ";
}

/** Clip id of the live row currently under the pointer, or null. Reads the
 * live :hover state rather than the hoveredClipId variable, so a re-render
 * that rebuilt the rows under a stationary cursor can't leave a stale value in
 * either direction (a row under the pointer with no pointerenter, or a row
 * that's gone with a lingering pointerleave). */
function hoveredRowId(): string | null {
  const row = clipList.querySelector<HTMLElement>(".clip-item:hover");
  return row?.dataset.clipId ?? null;
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
  if (visibleClips.length === 0) return;
  if (selectedIndex < 0) {
    selectedIndex = 0;
  } else {
    selectedIndex = Math.min(Math.max(selectedIndex + delta, 0), visibleClips.length - 1);
  }
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
  const inFilter = document.activeElement instanceof HTMLElement
    && document.activeElement.closest("#filter-bar");

  // '/' focuses the search box from anywhere outside an editable control
  // (only search itself here), where '/' types normally. preventDefault stops
  // the '/' from being delivered into the newly-focused input.
  if (e.key === "/" && !inSearch) {
    e.preventDefault();
    searchInput.focus();
    return;
  }

  // Filter bar: ArrowLeft / ArrowRight move between filter buttons
  if (inFilter) {
    if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
      e.preventDefault();
      const buttons = Array.from(filterBar.querySelectorAll(".filter-btn")) as HTMLButtonElement[];
      const currentIdx = buttons.indexOf(document.activeElement as HTMLButtonElement);
      const nextIdx = e.key === "ArrowLeft"
        ? (currentIdx - 1 + buttons.length) % buttons.length
        : (currentIdx + 1) % buttons.length;
      buttons[nextIdx].focus();
      // Activate the filter directly; setFilter() moves selection to the new
      // filter's first item.
      const filter = buttons[nextIdx].dataset.filter as FilterKind;
      setFilter(filter);
      return;
    }
  }

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
      // When a filter button has focus, activate it directly and keep
      // focus there. preventDefault already cancelled the native click,
      // so we must handle activation ourselves.
      if (inFilter) {
        const btn = document.activeElement as HTMLButtonElement;
        const filter = btn.dataset.filter as FilterKind;
        if (filter) {
          setFilter(filter);
          btn.focus();
        }
        return;
      }
      pasteSelected();
      return;
    case "Escape":
      e.preventDefault();
      // A preview open (Space held) closes only the preview — Escape must
      // not fall through to closePanel and hide the history panel.
      if (spacePressed) {
        e.stopPropagation();
        releasePreview();
        return;
      }
      // If action menu is open, first Escape closes only the menu
      if (openMenuClipId) {
        hideActionMenu();
        return;
      }
      // Vim mode: first Escape blurs the search box into navigation mode,
      // the next Escape closes the Panel.
      if (inSearch && vimMode) {
        searchInput.blur();
      } else {
        closePanel();
      }
      return;
  }

  if (!inSearch && !inFilter) {
    if (vimMode && (e.key === "j" || e.key === "k")) {
      e.preventDefault();
      moveSelection(e.key === "j" ? 1 : -1);
      return;
    }
  }
});

// Hover+Space: capture Space before the search input or the vim/char handler
// below sees it. Intercepts only when a live row is under the pointer —
// otherwise Space falls through and types normally. Registered on window
// (earliest capture phase) so no listener registration order can slip ahead.
window.addEventListener("keydown", (e) => {
  if (!isSpaceKey(e)) return;
  // Latched: once the initial Space was captured, every Space keydown until
  // keyup is swallowed unconditionally — held-Space auto-repeat must never
  // type into search even after the preview window steals focus or the row's
  // :hover state drops. A newly hovered row while held re-shows via the row's
  // pointerenter handler, not here.
  if (spacePressed) {
    e.preventDefault();
    e.stopImmediatePropagation();
    return;
  }
  if (e.repeat) return; // repeat never starts a fresh preview
  const id = hoveredRowId();
  if (!id) return; // no live row — Space stays normal input
  e.preventDefault();
  e.stopImmediatePropagation();
  spacePressed = true;
  // The preview window is shown unfocused (backend `.focused(false)`), so the
  // search box keeps window focus. Record whether it was focused and blur it
  // so held-Space auto-repeat cannot type into it while the latch is active;
  // focus is restored on release by releasePreview().
  searchFocusedBeforePreview = document.activeElement === searchInput;
  if (searchFocusedBeforePreview) searchInput.blur();
  showPreview(id);
}, true);

// Space keyup closes the preview regardless of where focus has moved.
window.addEventListener("keyup", (e) => {
  if (isSpaceKey(e) && spacePressed) {
    releasePreview();
  }
}, true);

// Input-layer suppression: while the preview latch is active, block any
// whitespace insertion into the (normally focused) search box. This catches
// paths the keydown suppression cannot — auto-repeat, or a Space delivered
// while focus is momentarily elsewhere — without erasing existing search text.
window.addEventListener("beforeinput", (e) => {
  if (!spacePressed) return;
  if (e.inputType === "insertText" && (e.data == null || /^\s+$/.test(e.data))) {
    e.preventDefault();
  }
}, true);

// Reset selected on new search input
searchInput.addEventListener("input", () => {
  selectedIndex = 0;
  render();
});

// Filter bar mouse click: activate the filter. Focus stays on the clicked
// button; only a direct search click or the '/' shortcut moves it to search.
filterBar.addEventListener("click", (e) => {
  const btn = (e.target as HTMLElement).closest(".filter-btn") as HTMLButtonElement | null;
  if (!btn) return;
  const filter = btn.dataset.filter as FilterKind;
  if (filter && filter !== activeFilter) {
    setFilter(filter);
  }
});

// Close menu on outside click (panel body clicks that aren't on a More button
// or the menu itself). Row clicks paste — close the menu but let paste happen.
document.addEventListener("click", (e) => {
  if (!openMenuClipId) return;
  const target = e.target as HTMLElement;
  if (!target.closest(".more-btn") && !target.closest("#clip-action-menu")) {
    hideActionMenu();
  }
}, true); // capture phase — runs before row click handler so menu closes first

// Clicks on the transparent margin around the panel dismiss it.
document.body.addEventListener("click", (e) => {
  if (e.target === document.body || e.target === document.documentElement) {
    closePanel();
  }
});

// Focus-loss dismissal is handled on the Rust side (WindowEvent::Focused).

// === Initialize ===
window.addEventListener("DOMContentLoaded", init);
