import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { setLanguage, applyI18n, t, localizeBackendError } from "./i18n";
import { applyTheme } from "./theme";
import { PreviewController } from "./preview-state";
import { ShortcutMatcher, FAVORITES_DEFAULT_CODES } from "./shortcut";
import { ChooserGate, computeMenuPlacement } from "./menu";
import { DragController, itemDragStartPayload, physicalScreenPoint, isFavoriteItem, clipLocator } from "./drag";
import type { ItemDragPoint } from "./drag";
import { filterItems, isLink } from "./dataset";
import type { FilterKind } from "./dataset";
import type { Clip, ClipboardUpdate, ClipLocator, CollectionSummary, FavoriteItem, FavoritesUiState } from "./types";

type DisplayItem = Clip | FavoriteItem;

let clips: Clip[] = [];
let favoriteItems: FavoriteItem[] = [];
let collections: CollectionSummary[] = [];
let selectedCollection: string | null = null;
let sidebarOpen = false;
// The search-filtered view of the active dataset, in display order. Keyboard
// selection indexes into this — never into the raw arrays directly.
let visibleClips: DisplayItem[] = [];
let selectedIndex = -1;
let vimMode = false;
let pasteFilesAsFiles = true;
let rememberHistoryFilter = false;
let activeFilter: FilterKind = "all";
let openMenuClipId: string | null = null;
let toastTimer: ReturnType<typeof setTimeout> | null = null;
const previewState = new PreviewController();
let previewHintSeen = false;
let previewHintTimer: ReturnType<typeof setTimeout> | null = null;
let shortcutMatcher = new ShortcutMatcher(FAVORITES_DEFAULT_CODES);
// The history row currently being dragged toward the drawer, so a mid-drag
// re-render can clear its source feedback and broadcast a cancel.
let activeDragSource: HTMLElement | null = null;
// Monotonic session id + the active one, carried on every move/end/cancel so
// the sidebar can reject stale or cancelled drags.
let dragSessionSeq = 0;
let activeDragSessionId: number | null = null;
// Cached main-window geometry for converting client CSS points to physical
// screen coordinates (screenX/Y lie at non-100% DPI).
let windowOrigin = { x: 0, y: 0 };
let scaleFactor = 1;
const chooserGate = new ChooserGate();
let lastMenuPos = { anchorTop: 0, anchorBottom: 0, right: 0 };

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
const addMenu = document.getElementById("add-to-collection-menu")!;
const previewHintStrip = document.getElementById("preview-hint-strip")!;
const favoritesToggle = document.getElementById("favorites-toggle") as HTMLButtonElement;

// === Init ===
async function refreshConfig() {
  try {
    const config = await invoke<{
      language?: string;
      vim_mode?: boolean;
      theme?: string;
      ui_opacity_percent?: number;
      paste_files_as_files?: boolean;
      remember_history_filter?: boolean;
      favorites_toggle_shortcut?: { codes: string[] };
    }>("get_config");
    setLanguage(config.language || "zh-TW");
    vimMode = !!config.vim_mode;
    pasteFilesAsFiles = config.paste_files_as_files !== false;
    rememberHistoryFilter = !!config.remember_history_filter;
    applyTheme(config.theme || "system");
    const opacity = Math.min(100, Math.max(50, config.ui_opacity_percent ?? 99));
    document.documentElement.style.setProperty("--panel-opacity", String(opacity / 100));
    shortcutMatcher = new ShortcutMatcher(config.favorites_toggle_shortcut?.codes ?? FAVORITES_DEFAULT_CODES);
  } catch (err) {
    console.error("Failed to load config:", err);
    setLanguage("zh-TW");
  }
}

function sortClips() {
  clips.sort((a, b) => Number(b.pinned) - Number(a.pinned) || b.captured_at - a.captured_at);
}

function activeDataset(): DisplayItem[] {
  return selectedCollection === null ? clips : favoriteItems;
}

async function loadFavoritesContext() {
  try {
    const [cols, state] = await Promise.all([
      invoke<CollectionSummary[]>("list_collections"),
      invoke<FavoritesUiState>("get_favorites_ui_state"),
    ]);
    collections = cols;
    selectedCollection = state.selected_collection;
    sidebarOpen = state.open;
    if (selectedCollection !== null) {
      favoriteItems = await invoke<FavoriteItem[]>("list_favorite_items", { collectionId: selectedCollection });
    } else {
      favoriteItems = [];
    }
    favoritesToggle.classList.toggle("active", sidebarOpen);
    favoritesToggle.setAttribute("aria-pressed", String(sidebarOpen));
    favoritesToggle.title = sidebarOpen ? t("sidebarClose") : t("sidebarOpen");
    render();
  } catch (err) {
    console.error("Failed to load favorites context:", err);
  }
}

function updateFavoritesToggleA11y() {
  favoritesToggle.classList.toggle("active", sidebarOpen);
  favoritesToggle.setAttribute("aria-pressed", String(sidebarOpen));
  favoritesToggle.title = sidebarOpen ? t("sidebarClose") : t("sidebarOpen");
}

async function cacheWindowGeometry(): Promise<void> {
  try {
    const win = getCurrentWindow();
    const pos = await win.outerPosition();
    const scale = await win.scaleFactor();
    windowOrigin = { x: pos.x, y: pos.y };
    scaleFactor = scale || 1;
  } catch {
    windowOrigin = { x: 0, y: 0 };
    scaleFactor = 1;
  }
}

async function init() {
  await refreshConfig();
  applyI18n();
  previewHintSeen = readPreviewHintSeen();
  await cacheWindowGeometry();

  clips = await invoke("get_clips");
  selectedIndex = 0;
  await loadFavoritesContext();
  render();

  await getCurrentWindow().onFocusChanged(({ payload: focused }) => {
    if (!focused) return;
    resyncPreviewState();
    cacheWindowGeometry();
    refreshConfig().then(() => {
      applyI18n();
      updateFavoritesToggleA11y();
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

  await listen<ClipboardUpdate>("clipboard-update", (event) => {
    const { clip, evicted } = event.payload;
    const existingIndex = clips.findIndex((c) => c.content_hash === clip.content_hash);
    if (existingIndex >= 0) {
      clips[existingIndex] = clip;
    } else {
      clips.unshift(clip);
    }
    if (evicted.length > 0) {
      clips = clips.filter((c) => !evicted.includes(c.id));
    }
    sortClips();
    render();
  });

  await listen("clip-preview-closed", () => {
    resyncPreviewState();
  });

  // Favorites data + selection sync across both windows.
  await listen<void>("favorites-updated", () => {
    void loadFavoritesContext();
  });
  await listen<FavoritesUiState>("favorites-ui-state-changed", () => {
    void loadFavoritesContext();
  });

  // Favorites toggle button.
  favoritesToggle.addEventListener("click", () => {
    void toggleSidebar();
  });

  document.addEventListener("keydown", onFavoritesShortcutKeydown, true);
  document.addEventListener("keyup", onFavoritesShortcutKeyup, true);
}

async function toggleSidebar() {
  sidebarOpen = !sidebarOpen;
  updateFavoritesToggleA11y();
  await invoke("set_favorites_open", { open: sidebarOpen }).catch(() => {});
}

function onFavoritesShortcutKeydown(e: KeyboardEvent) {
  if (shortcutMatcher.keydown(e.code, e.repeat)) {
    e.preventDefault();
    e.stopPropagation();
    void toggleSidebar();
  }
}

function onFavoritesShortcutKeyup(e: KeyboardEvent) {
  shortcutMatcher.keyup(e.code);
}

// === Link classification ===
// (isLink/classifyClip/matchesFilter live in dataset.ts; isLink is re-exported
//  for icon selection below.)

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
  filterBar.setAttribute("aria-label", t("filterBarLabel"));
}

// === SVG icon helpers ===
const SVG_NS = "http://www.w3.org/2000/svg";

function svgEl(name: string, attrs: Record<string, string>): SVGElement {
  const el = document.createElementNS(SVG_NS, name);
  for (const key in attrs) {
    el.setAttribute(key, attrs[key]);
  }
  return el;
}

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

function copyIcon(size: number): SVGElement {
  const svg = iconRoot(size, "none", "currentColor");
  svg.append(
    svgEl("rect", { x: "9", y: "9", width: "13", height: "13", rx: "2" }),
    svgEl("path", { d: "M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" }),
  );
  return svg;
}

function fileIcon(): SVGElement {
  const svg = iconRoot(16, "none", "currentColor");
  svg.append(
    svgEl("path", { d: "M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z" }),
    svgEl("polyline", { points: "14 2 14 8 20 8" }),
  );
  return svg;
}

function linkIcon(): SVGElement {
  const svg = iconRoot(16, "none", "currentColor");
  svg.append(
    svgEl("path", { d: "M10 13a5 5 0 0 0 7.54.54l3-3a5 5 0 0 0-7.07-7.07l-1.72 1.71" }),
    svgEl("path", { d: "M14 11a5 5 0 0 0-7.54-.54l-3 3a5 5 0 0 0 7.07 7.07l1.71-1.71" }),
  );
  return svg;
}

function pinIcon(pinned: boolean): SVGElement {
  const svg = iconRoot(15, pinned ? "currentColor" : "none", "currentColor");
  svg.append(
    svgEl("path", { d: "M12 2v8M8 6V4a2 2 0 0 1 2-2h4a2 2 0 0 1 2 2v2" }),
    svgEl("path", { d: "M4 6h16" }),
    svgEl("path", { d: "M10 10v8a2 2 0 0 0 2 2 2 2 0 0 0 2-2v-8" }),
  );
  return svg;
}

function moreIcon(): SVGElement {
  const svg = iconRoot(15, "currentColor", "none");
  svg.append(
    svgEl("circle", { cx: "5", cy: "12", r: "2" }),
    svgEl("circle", { cx: "12", cy: "12", r: "2" }),
    svgEl("circle", { cx: "19", cy: "12", r: "2" }),
  );
  return svg;
}

function dragGripIcon(size = 14): SVGElement {
  const svg = iconRoot(size, "currentColor", "none");
  for (const [cx, cy] of [[9, 6], [15, 6], [9, 12], [15, 12], [9, 18], [15, 18]] as const) {
    svg.append(svgEl("circle", { cx: String(cx), cy: String(cy), r: "1.5" }));
  }
  return svg;
}

// === Render ===
function render() {
  const query = searchInput.value.toLowerCase();
  const source = activeDataset();
  const filtered = filterItems(source, query, activeFilter);
  visibleClips = filtered;

  if (visibleClips.length === 0) {
    selectedIndex = -1;
  } else {
    if (selectedIndex < 0) selectedIndex = 0;
    if (selectedIndex >= visibleClips.length) selectedIndex = visibleClips.length - 1;
  }

  openMenuClipId = null;
  hideActionMenu();

  const scrollTop = clipList.scrollTop;
  if (previewHintTimer) {
    clearTimeout(previewHintTimer);
    previewHintTimer = null;
  }
  if (activeDragSource) {
    releaseDragSource(activeDragSource, activeDragSessionId !== null);
  }
  clipList.replaceChildren();

  const searching = query.length > 0;
  const filtering = activeFilter !== "all";
  const showEmpty = visibleClips.length === 0;
  const totalEmpty = source.length === 0;

  emptyState.classList.toggle("hidden", !showEmpty);
  if (showEmpty) {
    if (totalEmpty) {
      emptyTitle.textContent = selectedCollection === null ? t("emptyTitle") : t("favoritesEmptyTitle");
      emptyHint.classList.toggle("hidden", selectedCollection !== null);
      if (selectedCollection !== null) {
        emptyHint.textContent = t("favoritesEmptyHint");
        emptyHint.classList.remove("hidden");
      }
    } else if (searching || filtering) {
      emptyTitle.textContent =
        searching && filtering ? t("noResults") : filtering && !searching ? t("categoryEmpty") : t("noResults");
      emptyHint.classList.add("hidden");
    }
  }

  updateFilterBar();
  updatePreviewHintStrip();

  let hasPinned = false;
  let hasUnpinned = false;

  filtered.forEach((item, index) => {
    const isFav = isFavoriteItem(item);
    const pinned = !isFav && item.pinned;
    if (pinned && !hasPinned) {
      hasPinned = true;
    }
    if (!pinned && !hasUnpinned && hasPinned) {
      const divider = document.createElement("div");
      divider.className = "pinned-divider";
      divider.textContent = t("pinnedDivider");
      clipList.appendChild(divider);
      hasUnpinned = true;
    }

    const el = document.createElement("div");
    el.className = `clip-item${item.truncated ? " truncated" : ""}${index === selectedIndex ? " selected" : ""}`;
    el.dataset.index = String(index);
    el.dataset.clipId = item.id;
    el.dataset.isFavorite = isFav ? "1" : "0";

    const hint = document.createElement("div");
    hint.className = "clip-preview-hint";
    const hintKeycap = document.createElement("kbd");
    hintKeycap.className = "keycap";
    hintKeycap.setAttribute("aria-hidden", "true");
    hintKeycap.textContent = "Space";
    const hintLabel = document.createElement("span");
    hintLabel.textContent = t("pressToPreview");
    hint.append(hintKeycap, hintLabel);

    el.addEventListener("click", () => {
      pasteActive(item);
    });

    el.addEventListener("pointerenter", () => {
      if (previewState.isOpen) showPreviewFor(item);
      scheduleRowHint(hint);
    });
    el.addEventListener("pointerleave", () => {
      clearRowHint(hint);
    });

    const dragHandle = document.createElement("button");
    dragHandle.type = "button";
    dragHandle.className = "clip-drag-handle";
    dragHandle.setAttribute("aria-label", t("dragToDrawer"));
    dragHandle.title = t("dragToDrawer");
    dragHandle.appendChild(dragGripIcon());
    el.appendChild(dragHandle);
    attachRowDrag(el, dragHandle, item);

    const iconDiv = document.createElement("div");
    if (item.kind === "Image" && item.thumbnail_base64) {
      iconDiv.className = "thumbnail-container";
      const img = document.createElement("img");
      img.src = item.thumbnail_base64;
      img.alt = "Image";
      iconDiv.appendChild(img);
    } else if (item.kind === "FilePaths") {
      iconDiv.className = "clip-icon text-icon";
      iconDiv.appendChild(fileIcon());
    } else if (isLink(item.text_content)) {
      iconDiv.className = "clip-icon text-icon";
      iconDiv.appendChild(linkIcon());
    } else {
      iconDiv.className = "clip-icon text-icon";
      iconDiv.appendChild(copyIcon(16));
    }
    el.appendChild(iconDiv);

    const contentDiv = document.createElement("div");
    contentDiv.className = "clip-content";

    const title = document.createElement("div");
    title.className = "clip-title";
    let titleText = item.preview || "(empty)";
    if (item.kind === "Image") {
      titleText = t("imageClip");
    } else if (item.kind === "Text") {
      titleText = titleText.replace(/\n/g, " ");
    }
    title.textContent = titleText;
    contentDiv.appendChild(title);

    const meta = document.createElement("div");
    meta.className = "clip-meta";
    const source = document.createElement("span");
    source.className = "source";
    source.textContent = !item.source_exe || item.source_exe === "Unknown" ? t("unknownSource") : item.source_exe;
    meta.appendChild(source);

    const size = document.createElement("span");
    size.textContent = item.kind === "Image" ? `${(item.byte_size / 1024 / 1024).toFixed(1)}MB` : `${item.byte_size} B`;
    meta.appendChild(size);

    contentDiv.appendChild(meta);
    el.appendChild(contentDiv);

    el.appendChild(hint);

    const time = document.createElement("span");
    time.className = "clip-time";
    time.textContent = formatTime(isFav ? item.added_at ?? item.captured_at : item.captured_at);
    el.appendChild(time);

    const actions = document.createElement("div");
    actions.className = "clip-actions";

    if (!isFav) {
      const pinBtn = document.createElement("button");
      pinBtn.className = `clip-action-btn pin-btn${item.pinned ? " pinned" : ""}`;
      pinBtn.appendChild(pinIcon(item.pinned));
      pinBtn.title = item.pinned ? t("unpinTitle") : t("pinTitle");
      pinBtn.addEventListener("click", (e) => {
        e.stopPropagation();
        togglePin(item as Clip);
      });
      actions.appendChild(pinBtn);
    }

    const copyBtn = document.createElement("button");
    copyBtn.className = "clip-action-btn";
    copyBtn.appendChild(copyIcon(15));
    copyBtn.title = t("copyOnlyTitle");
    copyBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      copyActive(item);
    });
    actions.appendChild(copyBtn);

    const moreBtn = document.createElement("button");
    moreBtn.className = "clip-action-btn more-btn";
    moreBtn.appendChild(moreIcon());
    moreBtn.title = t("moreTitle");
    moreBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      toggleActionMenu(item.id, moreBtn);
    });
    actions.appendChild(moreBtn);

    el.appendChild(actions);
    clipList.appendChild(el);
  });

  clipList.scrollTop = scrollTop;
  if (selectedIndex >= 0) {
    const selected = clipList.querySelector(".clip-item.selected");
    selected?.scrollIntoView({ block: "nearest" });
  }
}

// === Action Menu ===
// Place an absolutely-positioned menu so it stays inside the panel: prefer
// below the anchor, flip above when that side has more room, and constrain the
// height (vertical scroll) when it is taller than either side. `anchor` is in
// viewport coordinates; `right` is the anchor-right distance from the panel's
// right edge.
function placeMenu(
  menu: HTMLElement,
  anchor: { top: number; bottom: number },
  panelRect: DOMRect,
  right: number,
): void {
  menu.classList.remove("hidden");
  // Reset any previous constraint so the measurement is the natural size.
  menu.style.maxHeight = "";
  menu.style.maxWidth = "";
  const p = computeMenuPlacement(
    { top: anchor.top - panelRect.top, bottom: anchor.bottom - panelRect.top },
    { width: panelRect.width, height: panelRect.height },
    menu.offsetWidth,
    menu.offsetHeight,
    right,
  );
  menu.style.top = `${p.top}px`;
  menu.style.right = `${p.right}px`;
  menu.style.maxHeight = p.maxHeight === null ? "" : `${p.maxHeight}px`;
  menu.style.maxWidth = p.maxWidth === null ? "" : `${p.maxWidth}px`;
}

function toggleActionMenu(clipId: string, anchor: HTMLElement) {
  if (openMenuClipId === clipId) {
    hideActionMenu();
    return;
  }
  openMenuClipId = clipId;
  // Opening a fresh More menu dismisses any lingering add-to-collection chooser
  // so the two never overlap.
  hideAddChooser();
  const item = activeDataset().find((c) => c.id === clipId);
  if (!item) return;
  renderActionMenu(item);
  const rect = anchor.getBoundingClientRect();
  const panelRect = document.getElementById("panel")!.getBoundingClientRect();
  const right = panelRect.right - rect.right;
  placeMenu(actionMenu, rect, panelRect, right);
  lastMenuPos = { anchorTop: rect.top, anchorBottom: rect.bottom, right };
}

function renderActionMenu(item: DisplayItem) {
  actionMenu.replaceChildren();
  const isFav = isFavoriteItem(item);

  if (isFav) {
    actionMenu.appendChild(menuItem(t("removeFromCollection"), () => {
      const fav = item as FavoriteItem;
      invoke("remove_favorite", { collectionId: selectedCollection, itemId: fav.id })
        .then(() => {
          showToast(t("removedFromFavorites"));
          void loadFavoritesContext();
        })
        .catch((err) => showToast(localizeBackendError(String(err))));
    }, true));
    actionMenu.appendChild(menuItem(t("addToOtherCollection"), () => {
      hideActionMenu();
      openAddChooser(item);
    }));
  } else {
    actionMenu.appendChild(menuItem(t("addToCollection"), () => {
      hideActionMenu();
      openAddChooser(item);
    }));
    actionMenu.appendChild(menuItem(t("deleteTitle"), () => {
      deleteClip(item as Clip);
    }, true));
  }
}

function menuItem(label: string, onClick: () => void, danger = false): HTMLButtonElement {
  const b = document.createElement("button");
  b.className = `menu-item${danger ? " menu-item-delete" : ""}`;
  b.setAttribute("role", "menuitem");
  b.textContent = label;
  b.addEventListener("click", (e) => {
    e.stopPropagation();
    onClick();
  });
  return b;
}

function hideActionMenu() {
  openMenuClipId = null;
  actionMenu.classList.add("hidden");
  hideAddChooser();
}

// === Add-to-collection chooser ===
function openAddChooser(item: DisplayItem) {
  const token = chooserGate.open(item.id);
  const locator: ClipLocator = clipLocator(item);

  invoke<string[]>("favorite_collection_ids", { locator }).then((existing) => {
    if (!chooserGate.isCurrent(item.id, token)) return;
    renderAddChooser(locator, existing);
  }).catch(() => {});
}

function renderAddChooser(locator: ClipLocator, existing: string[]) {
  addMenu.replaceChildren();

  if (collections.length === 0) {
    const create = menuItem(t("createCollection"), () => {
      hideAddChooser();
      void invoke("set_favorites_open", { open: true }).then(() => emit("favorites-create-request"));
    });
    addMenu.appendChild(create);
  } else {
    collections.forEach((c) => {
      const member = existing.includes(c.id);
      const b = menuItem(`${c.name}${member ? ` · ${t("addedToFavorites")}` : ""}`, () => {
        hideAddChooser();
        invoke("add_favorite", { collectionId: c.id, locator })
          .then(() => { showToast(t("addedToFavorites")); void loadFavoritesContext(); })
          .catch((err) => showToast(localizeBackendError(String(err))));
      });
      b.disabled = member;
      b.classList.toggle("menu-item-checked", member);
      addMenu.appendChild(b);
    });
  }

  // Position to the left of where the More menu opened (the menu is hidden now,
  // so reuse its last recorded anchor), flipping above when there is no room
  // below.
  const panelRect = document.getElementById("panel")!.getBoundingClientRect();
  placeMenu(
    addMenu,
    { top: lastMenuPos.anchorTop, bottom: lastMenuPos.anchorBottom },
    panelRect,
    lastMenuPos.right + 8,
  );
}

function hideAddChooser() {
  chooserGate.close();
  addMenu.classList.add("hidden");
}

actionMenu.addEventListener("click", (e) => {
  e.stopPropagation();
});
addMenu.addEventListener("click", (e) => {
  e.stopPropagation();
});

// === Actions ===
async function pasteActive(item: DisplayItem) {
  if (isFavoriteItem(item)) {
    try {
      await invoke<string>("paste_favorite", { id: item.id });
    } catch (err) {
      console.error("Paste failed:", err);
      showToast(t("pasteFailed"));
    }
    return;
  }
  await pasteClip(item as Clip);
}

async function pasteClip(clip: Clip) {
  try {
    switch (clip.kind) {
      case "Text":
        await invoke("paste_text", { text: clip.text_content || "" });
        break;
      case "FilePaths":
        if (pasteFilesAsFiles) {
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

async function copyActive(item: DisplayItem) {
  if (isFavoriteItem(item)) {
    try {
      await invoke<string>("copy_favorite", { id: item.id });
      showToast(t("copied"));
    } catch (err) {
      console.error("Copy failed:", err);
      showToast(t("copyFailed"));
    }
    return;
  }
  await copyOnly(item as Clip);
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
    clips = clips.filter((c) => c.id !== clip.id);
    render();
  };
  try {
    await invoke("delete_clip", { id: clip.id });
    removeLocal();
    showToast(t("deleted"), async () => {
      try {
        await invoke("undo_delete", { id: clip.id });
        clips = await invoke("get_clips");
        render();
      } catch (err) {
        showToast(localizeBackendError(String(err)));
      }
    });
  } catch (err) {
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
  await invoke("hide_panel_command");
}

// === Press-Space Preview ===
function showPreviewFor(item: DisplayItem) {
  hideAllRowHints();
  const token = previewState.beginShow(item.id);
  const cmd = isFavoriteItem(item) ? "show_favorite_preview" : "show_clip_preview";
  invoke(cmd, { id: item.id })
    .then(() => {
      previewState.resolveShow(token, item.id);
      markPreviewHintSeen();
    })
    .catch((err) => {
      console.error("Failed to show preview:", err);
      if (previewState.isCurrent(token)) resyncPreviewState();
    });
}

function readPreviewHintSeen(): boolean {
  try {
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

function updatePreviewHintStrip() {
  const show = !previewHintSeen && visibleClips.length > 0;
  previewHintStrip.classList.toggle("hidden", !show);
}

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

function scheduleRowHint(hint: HTMLElement) {
  clearRowHint(hint);
  if (previewState.isOpen) return;
  previewHintTimer = setTimeout(() => {
    previewHintTimer = null;
    hint.classList.add("visible");
  }, PREVIEW_HINT_DELAY);
}

function clearRowHint(hint: HTMLElement) {
  if (previewHintTimer) {
    clearTimeout(previewHintTimer);
    previewHintTimer = null;
  }
  hint.classList.remove("visible");
}

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
  const token = previewState.beginHide();
  invoke("hide_clip_preview")
    .then(() => previewState.resolveHide(token))
    .catch((err) => {
      console.error("Failed to hide preview:", err);
      if (previewState.isCurrent(token)) resyncPreviewState();
    });
}

function resyncPreviewState() {
  const token = previewState.beginResync();
  invoke<{ id: string } | null>("get_active_clip_preview")
    .then((active) => previewState.resolveResync(token, active ? active.id : null))
    .catch(() => {});
}

function isSpaceKey(e: KeyboardEvent): boolean {
  return e.code === "Space" || e.key === " ";
}

function hoveredRowId(): string | null {
  const row = clipList.querySelector<HTMLElement>(".clip-item:hover");
  return row?.dataset.clipId ?? null;
}

function selectedRowId(): string | null {
  return visibleClips[selectedIndex]?.id ?? null;
}

function isEditableActive(): boolean {
  const el = document.activeElement;
  if (!(el instanceof HTMLElement)) return false;
  return el.isContentEditable || el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.tagName === "SELECT";
}

// === Item drag source (to the sidebar) ===
// Both history clips and drawer favorite items drag into a drawer collection
// (a copy — the source row is never removed or moved). Source feedback is
// added once the 6px threshold is crossed and cleared on drop/cancel; a cancel
// is broadcast on abort so the sidebar drops any lingering drop-target state.
function releaseDragSource(el: HTMLElement, cancel: boolean): void {
  if (activeDragSource === el) {
    activeDragSource = null;
    if (cancel && activeDragSessionId !== null) {
      void emit("favorites-item-drag-cancel", activeDragSessionId);
    }
    activeDragSessionId = null;
  }
  el.classList.remove("dragging-source", "drag-held");
}

function attachRowDrag(row: HTMLElement, handle: HTMLElement, item: DisplayItem) {
  const drag = new DragController(6);
  const locator = clipLocator(item);
  handle.addEventListener("pointerdown", (e) => {
    e.preventDefault();
    e.stopPropagation();
    drag.beginImmediately(e.clientX, e.clientY);
    dragSessionSeq += 1;
    activeDragSessionId = dragSessionSeq;
    row.classList.add("dragging-source");
    activeDragSource = row;
    handle.setPointerCapture(e.pointerId);
    const point = physicalScreenPoint(windowOrigin, { x: e.clientX, y: e.clientY }, scaleFactor);
    const payload: ItemDragPoint = { sessionId: activeDragSessionId, locator, x: point.x, y: point.y };
    void emit("favorites-item-drag", itemDragStartPayload(activeDragSessionId, item, point));
    void emit("favorites-item-drag-move", payload);
  });
  handle.addEventListener("pointermove", (e) => {
    if (!drag.isDragging || activeDragSessionId === null) return;
    const p = physicalScreenPoint(windowOrigin, { x: e.clientX, y: e.clientY }, scaleFactor);
    const payload: ItemDragPoint = { sessionId: activeDragSessionId, locator, x: p.x, y: p.y };
    void emit("favorites-item-drag-move", payload);
  });
  handle.addEventListener("pointerup", (e) => {
    if (drag.didDrag && activeDragSessionId !== null) {
      const p = physicalScreenPoint(windowOrigin, { x: e.clientX, y: e.clientY }, scaleFactor);
      const payload: ItemDragPoint = { sessionId: activeDragSessionId, locator, x: p.x, y: p.y };
      void emit("favorites-item-drag-end", payload);
    }
    releaseDragSource(row, false);
    drag.pointerUp();
  });
  handle.addEventListener("pointercancel", () => {
    const didDrag = drag.didDrag;
    releaseDragSource(row, didDrag);
    drag.pointerUp();
  });
  handle.addEventListener("lostpointercapture", () => {
    const didDrag = drag.didDrag;
    releaseDragSource(row, didDrag);
    drag.pointerUp();
  });
  handle.addEventListener("click", (e) => {
    e.stopPropagation();
    e.preventDefault();
  });
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
    pasteActive(visibleClips[selectedIndex]);
  }
}

document.addEventListener("keydown", (e) => {
  const inSearch = document.activeElement === searchInput;
  const inFilter = document.activeElement instanceof HTMLElement && document.activeElement.closest("#filter-bar");

  if (e.key === "/" && !inSearch) {
    e.preventDefault();
    searchInput.focus();
    return;
  }

  if (inFilter) {
    if (e.key === "ArrowLeft" || e.key === "ArrowRight") {
      e.preventDefault();
      const buttons = Array.from(filterBar.querySelectorAll(".filter-btn")) as HTMLButtonElement[];
      const currentIdx = buttons.indexOf(document.activeElement as HTMLButtonElement);
      const nextIdx = e.key === "ArrowLeft" ? (currentIdx - 1 + buttons.length) % buttons.length : (currentIdx + 1) % buttons.length;
      buttons[nextIdx].focus();
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
      if (previewState.isOpen) {
        e.stopPropagation();
        hidePreview();
        return;
      }
      if (openMenuClipId) {
        hideActionMenu();
        return;
      }
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

window.addEventListener("keydown", (e) => {
  if (!isSpaceKey(e)) return;
  if (!previewState.isOpen && isEditableActive()) return;
  const action = previewState.decideSpaceKeydown(e.repeat, hoveredRowId(), selectedRowId());
  switch (action.type) {
    case "swallow":
      e.preventDefault();
      e.stopImmediatePropagation();
      return;
    case "close":
      e.preventDefault();
      e.stopImmediatePropagation();
      previewState.consumeSpace();
      hidePreview();
      return;
    case "open":
      e.preventDefault();
      e.stopImmediatePropagation();
      previewState.consumeSpace();
      showPreviewForById(action.id);
      return;
    case "ignore":
      return;
  }
}, true);

function showPreviewForById(id: string) {
  const item = activeDataset().find((c) => c.id === id);
  if (item) showPreviewFor(item);
}

window.addEventListener("keyup", (e) => {
  if (isSpaceKey(e)) previewState.releaseSpace();
}, true);

// Focus loss (alt-tab, minimize) aborts an in-flight handle drag so the row
// doesn't stay stuck in a held/dragging state.
window.addEventListener("blur", () => {
  if (activeDragSource) releaseDragSource(activeDragSource, true);
});

searchInput.addEventListener("input", () => {
  selectedIndex = 0;
  render();
});

filterBar.addEventListener("click", (e) => {
  const btn = (e.target as HTMLElement).closest(".filter-btn") as HTMLButtonElement | null;
  if (!btn) return;
  const filter = btn.dataset.filter as FilterKind;
  if (filter && filter !== activeFilter) {
    setFilter(filter);
  }
});

document.addEventListener("click", (e) => {
  if (!openMenuClipId && !chooserGate.isOpen) return;
  const target = e.target as HTMLElement;
  if (!target.closest(".more-btn") && !target.closest("#clip-action-menu") && !target.closest("#add-to-collection-menu")) {
    hideActionMenu();
  }
}, true);

document.body.addEventListener("click", (e) => {
  if (e.target === document.body || e.target === document.documentElement) {
    closePanel();
  }
});

// === Initialize ===
window.addEventListener("DOMContentLoaded", init);
