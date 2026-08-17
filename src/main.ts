import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { setLanguage, applyI18n, t, localizeBackendError } from "./i18n";
import { applyTheme } from "./theme";
import { PreviewController } from "./preview-state";

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
// Press-Space preview toggle state machine. Owns the visibility flag, the
// intent token that resolves show/hide/resync races, and the held-Space
// repeat-suppression flag. Backend get_active_clip_preview stays the authority;
// see preview-state.ts.
const previewState = new PreviewController();
// One-time onboarding hint: stays visible until the first successful preview.
let previewHintSeen = false;
// Fade-in timer for the per-row preview hint (single hover at a time).
let previewHintTimer: ReturnType<typeof setTimeout> | null = null;

const PREVIEW_HINT_SEEN_KEY = "mnemark.previewHintSeen.v1";
const LEGACY_PREVIEW_HINT_SEEN_KEY = "clipflow.previewHintSeen.v1";
const PREVIEW_HINT_DELAY = 400;

const searchInput = document.getElementById("search-input") as HTMLInputElement;
const filterBar = document.getElementById("filter-bar")!;
const clipList = document.getElementById("clip-list")!;
const emptyState = document.getElementById("empty-state")!;
const emptyTitle = document.getElementById("empty-title")!;
const emptyHint = document.getElementById("empty-hint")!;
const toast = document.getElementById("toast")!;
const actionMenu = document.getElementById("clip-action-menu")!;
const previewHintStrip = document.getElementById("preview-hint-strip")!;

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
  previewHintSeen = readPreviewHintSeen();

  clips = await invoke("get_clips");
  selectedIndex = 0;
  render();

  // The Panel is reused via hide/show — re-apply the config every time it
  // regains focus so changes made in Settings take effect on next open.
  await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
    if (!focused) return; // composite external-focus-loss hiding is backend-owned
    // Main regained focus. An open preview survives a focus bounce to the
    // preview window and back; resync adopts backend truth, so a suspended
    // preview (panel hidden by paste/toggle/focus-loss, payload preserved) is
    // restored, while an explicit close stays closed.
    resyncPreviewState();
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

  // The preview window closes itself on Space/Escape when it owns focus. It
  // emits this only after its own hide_clip_preview resolves, but we still do
  // not treat it as proof of backend state — resync from the backend so a
  // concurrent show in this panel (hover-then-Space while the preview window
  // was closing) still wins.
  await listen("clip-preview-closed", () => {
    resyncPreviewState();
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

// === SVG icon helpers ===
const SVG_NS = "http://www.w3.org/2000/svg";

/** Create an SVG-namespaced element with the given attributes. */
function svgEl(name: string, attrs: Record<string, string>): SVGElement {
  const el = document.createElementNS(SVG_NS, name);
  for (const key in attrs) {
    el.setAttribute(key, attrs[key]);
  }
  return el;
}

/** Decorative icon root: shared 24x24 viewBox, aria-hidden, focusable=false. */
function iconRoot(size: number, fill: string, stroke: string): SVGElement {
  const attrs: Record<string, string> = {
    width: String(size),
    height: String(size),
    viewBox: "0 0 24 24",
    fill,
    stroke,
    "aria-hidden": "true",
    focusable: "false",
  };
  if (stroke !== "none") attrs["stroke-width"] = "2";
  return svgEl("svg", attrs);
}

/** Generic copy glyph (rect + arrow) — reused by the text-clip icon and the Copy button. */
function copyIcon(size: number): SVGElement {
  const svg = iconRoot(size, "none", "currentColor");
  svg.append(
    svgEl("rect", { x: "9", y: "9", width: "13", height: "13", rx: "2" }),
    svgEl("path", { d: "M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" }),
  );
  return svg;
}

/** FilePaths clip icon. */
function fileIcon(): SVGElement {
  const svg = iconRoot(16, "none", "currentColor");
  svg.append(
    svgEl("path", { d: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" }),
    svgEl("polyline", { points: "14 2 14 8 20 8" }),
  );
  return svg;
}

/** Link clip icon. */
function linkIcon(): SVGElement {
  const svg = iconRoot(16, "none", "currentColor");
  svg.append(
    svgEl("path", { d: "M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" }),
    svgEl("path", { d: "M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" }),
  );
  return svg;
}

/** Pin button icon; filled only while pinned. */
function pinIcon(pinned: boolean): SVGElement {
  const svg = iconRoot(15, pinned ? "currentColor" : "none", "currentColor");
  svg.append(
    svgEl("path", { d: "M12 2v8M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" }),
    svgEl("path", { d: "M4 6h16" }),
    svgEl("path", { d: "M10 10v8a2 2 0 0 0 2 2 2 2 0 0 0 2-2v-8" }),
  );
  return svg;
}

/** More button icon (three dots). */
function moreIcon(): SVGElement {
  const svg = iconRoot(15, "currentColor", "none");
  svg.append(
    svgEl("circle", { cx: "5", cy: "12", r: "2" }),
    svgEl("circle", { cx: "12", cy: "12", r: "2" }),
    svgEl("circle", { cx: "19", cy: "12", r: "2" }),
  );
  return svg;
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

  // If the previewed clip is no longer visible (deleted, evicted, or filtered
  // out by search/filter), close the preview so it never shows a ghost.
  if (previewState.isOpen && !visibleClips.some(c => c.id === previewState.currentId)) {
    hidePreview();
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
  // A pending row-hint fade-in targets a row about to be destroyed.
  if (previewHintTimer) {
    clearTimeout(previewHintTimer);
    previewHintTimer = null;
  }
  clipList.replaceChildren();

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
  updatePreviewHintStrip();

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

    // Right-side preview hint (Space keycap + label), collapsed until the row
    // is hovered. Non-interactive and never overlaps actions.
    const hint = document.createElement("div");
    hint.className = "clip-preview-hint";
    const hintKeycap = document.createElement("kbd");
    hintKeycap.className = "keycap";
    hintKeycap.setAttribute("aria-hidden", "true");
    hintKeycap.textContent = "Space";
    const hintLabel = document.createElement("span");
    hintLabel.textContent = t("pressToPreview");
    hint.append(hintKeycap, hintLabel);

    // Click row body = paste. Action buttons stop propagation.
    el.addEventListener("click", () => {
      pasteClip(clip);
    });

    // Hover tracking for the press-Space preview (does not alter paste).
    el.addEventListener("pointerenter", () => {
      if (previewState.isOpen) showPreview(clip.id);
      scheduleRowHint(hint);
    });
    el.addEventListener("pointerleave", () => {
      clearRowHint(hint);
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
      iconDiv.appendChild(fileIcon());
    } else if (isLink(clip.text_content)) {
      iconDiv.className = "clip-icon text-icon";
      iconDiv.appendChild(linkIcon());
    } else {
      iconDiv.className = "clip-icon text-icon";
      iconDiv.appendChild(copyIcon(16));
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

    // Hint sits between content and time so it never crowds the actions.
    el.appendChild(hint);

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
    pinBtn.appendChild(pinIcon(clip.pinned));
    pinBtn.title = clip.pinned ? t("unpinTitle") : t("pinTitle");
    pinBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      togglePin(clip);
    });
    actions.appendChild(pinBtn);

    // Copy
    const copyBtn = document.createElement("button");
    copyBtn.className = "clip-action-btn";
    copyBtn.appendChild(copyIcon(15));
    copyBtn.title = t("copyOnlyTitle");
    copyBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      copyOnly(clip);
    });
    actions.appendChild(copyBtn);

    // More (opens menu with Delete)
    const moreBtn = document.createElement("button");
    moreBtn.className = "clip-action-btn more-btn";
    moreBtn.appendChild(moreIcon());
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
  // Route through the backend suspension primitive so main + preview hide
  // atomically (no 150 ms focus-loss gap), while the saved preview payload —
  // and thus the still-enabled toggle — is preserved for reopen. Only an
  // explicit close (Space/Escape) clears the toggle state.
  await invoke("hide_panel_command");
}

// === Press-Space Preview ===
// Point at a row and press Space to toggle the side preview window. Pressing
// again closes it; Escape also closes it. Hover alone never opens it; Space
// with no hovered row stays normal search input. Closing the panel instead
// suspends it — the preview toggle stays enabled and is restored on reopen.
function showPreview(id: string) {
  hideAllRowHints();
  // Optimistically mark open: the toggle must reflect the press immediately so
  // key-repeat suppression and hover-to-update behave. The returned token
  // guards this show's completion against any newer show/hide mutation.
  const token = previewState.beginShow(id);
  invoke("show_clip_preview", { id })
    .then(() => {
      // The backend committed this show. Re-adopt it even if an earlier
      // resync read (which ran before the commit) saw the backend still null;
      // a newer show/hide mutation still wins via the token.
      previewState.resolveShow(token, id);
      markPreviewHintSeen();
    })
    .catch((err) => {
      console.error("Failed to show preview:", err);
      // Backend truth is unknown only while this is still the newest mutation;
      // a newer intent's completion owns the state otherwise.
      if (previewState.isCurrent(token)) resyncPreviewState();
    });
}

// === Preview discoverability hints ===
function readPreviewHintSeen(): boolean {
  try {
    // One-time migration from the legacy ClipFlow key: prefer the new key,
    // else adopt the legacy value, then drop the legacy key.
    const current = localStorage.getItem(PREVIEW_HINT_SEEN_KEY);
    if (current !== null) return current === "1";
    const legacy = localStorage.getItem(LEGACY_PREVIEW_HINT_SEEN_KEY);
    if (legacy !== null) {
      localStorage.setItem(PREVIEW_HINT_SEEN_KEY, legacy);
      localStorage.removeItem(LEGACY_PREVIEW_HINT_SEEN_KEY);
      return legacy === "1";
    }
    return false;
  } catch (err) {
    console.error("Failed to read preview hint state:", err);
    return false;
  }
}

/** Toggle the one-time onboarding strip: visible only when a history row is
 * shown and the hint has not yet been marked seen. */
function updatePreviewHintStrip() {
  const show = !previewHintSeen && visibleClips.length > 0;
  previewHintStrip.classList.toggle("hidden", !show);
}

/** Persist the onboarding hint as seen after a successful preview show.
 * localStorage failure must not break preview — it only leaves the hint
 * visible for the next successful use. */
function markPreviewHintSeen() {
  if (previewHintSeen) return;
  previewHintSeen = true;
  try {
    localStorage.setItem(PREVIEW_HINT_SEEN_KEY, "1");
  } catch (err) {
    console.error("Failed to persist preview hint state:", err);
  }
  updatePreviewHintStrip();
}

/** Fade in the row hint after the hover delay, unless a preview is already
 * open. */
function scheduleRowHint(hint: HTMLElement) {
  clearRowHint(hint);
  if (previewState.isOpen) return;
  previewHintTimer = setTimeout(() => {
    previewHintTimer = null;
    hint.classList.add("visible");
  }, PREVIEW_HINT_DELAY);
}

/** Cancel the pending fade-in and hide the hint immediately. */
function clearRowHint(hint: HTMLElement) {
  if (previewHintTimer) {
    clearTimeout(previewHintTimer);
    previewHintTimer = null;
  }
  hint.classList.remove("visible");
}

/** Hide every row hint at once (on preview open). */
function hideAllRowHints() {
  if (previewHintTimer) {
    clearTimeout(previewHintTimer);
    previewHintTimer = null;
  }
  clipList.querySelectorAll(".clip-preview-hint.visible").forEach((el) => {
    el.classList.remove("visible");
  });
}

function hidePreview() {
  // Do NOT clear the toggle flag here: a hide is confirmed only once
  // hide_clip_preview resolves. Keeping it "open" until then lets a concurrent
  // newer show/hide win via the intent token instead of a stale completion.
  const token = previewState.beginHide();
  invoke("hide_clip_preview")
    .then(() => previewState.resolveHide(token))
    .catch((err) => {
      console.error("Failed to hide preview:", err);
      if (previewState.isCurrent(token)) resyncPreviewState();
    });
}

/** Adopt backend truth: get_active_clip_preview is the authority for whether a
 * preview is actually shown. Used on panel focus regain, on preview-window
 * close, and on show/hide failure. Guarded by its own token so a slow query
 * can never overwrite a newer intent. */
function resyncPreviewState() {
  const token = previewState.beginResync();
  invoke<{ id: string } | null>("get_active_clip_preview")
    .then((active) => previewState.resolveResync(token, active ? active.id : null))
    .catch(() => {});
}

/** True when the event is the Spacebar, by physical code first (stable across
 * layouts and modifiers) with the standard key value as a fallback. */
function isSpaceKey(e: KeyboardEvent): boolean {
  return e.code === "Space" || e.key === " ";
}

/** Clip id of the live row currently under the pointer, or null. Reads the
 * live :hover state directly, so a re-render that rebuilt the rows under a
 * stationary cursor can't leave a stale value in either direction (a row under
 * the pointer with no pointerenter, or a row that's gone with a lingering
 * pointerleave). */
function hoveredRowId(): string | null {
  const row = clipList.querySelector<HTMLElement>(".clip-item:hover");
  return row?.dataset.clipId ?? null;
}

// === Toast ===
function showToast(message: string, onUndo?: () => void) {
  if (toastTimer) clearTimeout(toastTimer);

  toast.replaceChildren();
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
      // A preview open closes only the preview — Escape must not fall
      // through to closePanel and hide the history panel.
      if (previewState.isOpen) {
        e.stopPropagation();
        hidePreview();
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

// Press-Space toggle: capture Space before the search input or the vim/char
// handler below sees it. Registered on window (earliest capture phase) so no
// listener registration order can slip ahead.
window.addEventListener("keydown", (e) => {
  if (!isSpaceKey(e)) return;
  const action = previewState.decideSpaceKeydown(e.repeat, hoveredRowId());
  switch (action.type) {
    case "swallow":
      // Auto-repeat of a held opening/closing press: swallow until keyup so it
      // never inserts whitespace or re-toggles.
      e.preventDefault();
      e.stopImmediatePropagation();
      return;
    case "close":
      // Preview open: Space closes it, even with no row hovered or the pointer
      // over the preview window.
      e.preventDefault();
      e.stopImmediatePropagation();
      previewState.consumeSpace();
      hidePreview();
      return;
    case "open":
      e.preventDefault();
      e.stopImmediatePropagation();
      previewState.consumeSpace();
      showPreview(action.id);
      return;
    case "ignore":
      return; // no live row — Space stays normal search input
  }
}, true);

// A consumed Space press ends at keyup; releasing must not close the preview,
// only re-arm ordinary Space input.
window.addEventListener("keyup", (e) => {
  if (isSpaceKey(e)) previewState.releaseSpace();
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
