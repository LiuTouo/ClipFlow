// Favorites sidebar page (favorites.html): collection list + History button,
// inline create/rename, reorder via drag or Move Up/Down, destructive remove
// modal, and the cross-window item-drop target. The backend is the authority
// for every mutation; this page only mirrors and re-reads it.

import { listen } from "@tauri-apps/api/event";
import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { applyI18n, setLanguage, t } from "./i18n";
import { applyTheme } from "./theme";
import { ShortcutMatcher, FAVORITES_DEFAULT_CODES } from "./shortcut";
import { computeMenuPlacement } from "./menu";
import { DragController, rectContains, acceptDropSession, isAvailableDropTarget } from "./drag";
import type { ItemDragPoint, ItemDragStart } from "./drag";
import { insertBefore, moveOne } from "./reorder";
import type { CollectionSummary, FavoritesUiState } from "./types";

interface AppConfig {
  language?: string;
  theme?: string;
  ui_opacity_percent?: number;
  favorites_toggle_shortcut?: { codes: string[] };
}

const SVG_NS = "http://www.w3.org/2000/svg";

let collections: CollectionSummary[] = [];
let selected: string | null = null;
let matcher = new ShortcutMatcher(FAVORITES_DEFAULT_CODES);
let moreMenuFor: string | null = null;
let editingCreate = false;
let renamingId: string | null = null;
// Cross-window item drop state. move/end payloads carry their own locator, so
// no start event is required; these track the newest session and the last
// cancelled one so stale or aborted drags are rejected.
let activeSessionId: number | null = null;
let cancelledSessionId: number | null = null;
let activeDragStart: ItemDragStart | null = null;
let activeDragPoint: ItemDragPoint | null = null;
let dragMembershipIds: string[] = [];
let dragMembershipReady = false;
let dragMembershipPromise: Promise<string[]> | null = null;
// Cached window geometry for screen-coordinate hit-testing.
let windowOffset = { x: 0, y: 0 };
let scaleFactor = 1;

const listEl = document.getElementById("favorites-list")!;
const emptyEl = document.getElementById("favorites-empty")!;
const createRow = document.getElementById("favorites-create-row")!;
const createInput = document.getElementById("favorites-create-input") as HTMLElement;
const addBtn = document.getElementById("favorites-add") as HTMLButtonElement;
const moreMenu = document.getElementById("favorites-more-menu")!;
const toast = document.getElementById("favorites-toast")!;
const removeModal = document.getElementById("remove-modal")!;
const removeModalBody = document.getElementById("remove-modal-body")!;
const removeCancel = document.getElementById("remove-modal-cancel") as HTMLButtonElement;
const removeConfirm = document.getElementById("remove-modal-confirm") as HTMLButtonElement;

// === small DOM helpers ===
function svgEl(name: string, attrs: Record<string, string>): SVGElement {
  const el = document.createElementNS(SVG_NS, name);
  for (const k in attrs) el.setAttribute(k, attrs[k]);
  return el;
}

function icon(d: string, size = 15): SVGElement {
  const s = svgEl("svg", {
    width: String(size),
    height: String(size),
    viewBox: "0 0 24 24",
    fill: "none",
    stroke: "currentColor",
    "stroke-width": "2",
  });
  const path = svgEl("path", { d });
  s.append(path);
  return s;
}

const DRAG_HANDLE_D =
  "M8 5a1 1 0 1 0 0 2 1 1 0 0 0 0-2zm0 6a1 1 0 1 0 0 2 1 1 0 0 0 0-2zm0 6a1 1 0 1 0 0 2 1 1 0 0 0 0-2zm8-12a1 1 0 1 0 0 2 1 1 0 0 0 0-2zm0 6a1 1 0 1 0 0 2 1 1 0 0 0 0-2zm0 6a1 1 0 1 0 0 2 1 1 0 0 0 0-2z";
const MORE_D = "M5 12a1.5 1.5 0 1 0 0 3 1.5 1.5 0 0 0 0-3zm7 0a1.5 1.5 0 1 0 0 3 1.5 1.5 0 0 0 0-3zm7 0a1.5 1.5 0 1 0 0 3 1.5 1.5 0 0 0 0-3z";

// === init ===
async function refreshConfig(): Promise<void> {
  const config = await invoke<AppConfig>("get_config");
  setLanguage(config.language || "zh-TW");
  applyTheme(config.theme || "system");
  const opacity = Math.min(100, Math.max(50, config.ui_opacity_percent ?? 99));
  document.documentElement.style.setProperty("--panel-opacity", String(opacity / 100));
  matcher = new ShortcutMatcher(config.favorites_toggle_shortcut?.codes ?? FAVORITES_DEFAULT_CODES);
}

async function cacheWindowGeometry(): Promise<void> {
  try {
    const win = getCurrentWindow();
    const pos = await win.outerPosition();
    const scale = await win.scaleFactor();
    windowOffset = { x: pos.x, y: pos.y };
    scaleFactor = scale || 1;
  } catch {
    windowOffset = { x: 0, y: 0 };
    scaleFactor = 1;
  }
}

async function loadCollections(): Promise<void> {
  collections = await invoke<CollectionSummary[]>("list_collections");
  render();
}

async function loadUiState(): Promise<void> {
  const s = await invoke<FavoritesUiState>("get_favorites_ui_state");
  selected = s.selected_collection;
  render();
}

// === render ===
function render(): void {
  listEl.replaceChildren();
  const hasCollections = collections.length > 0;
  emptyEl.classList.toggle("hidden", hasCollections);

  // History is navigation, not another drawer. Show a contextual return row
  // only while a drawer dataset is active, keeping it visually separate from
  // sortable/drop-target collection rows.
  if (selected !== null) {
    const activeCollection = collections.find((c) => c.id === selected);
    const historyBtn = document.createElement("button");
    historyBtn.className = "favorites-history-return";
    historyBtn.setAttribute("aria-label", t("returnToHistory"));
    historyBtn.append(icon("M15 18l-6-6 6-6"));
    const historyCopy = document.createElement("span");
    historyCopy.className = "favorites-history-copy";
    const historyLabel = document.createElement("span");
    historyLabel.className = "favorites-history-label";
    historyLabel.textContent = t("returnToHistory");
    const historyContext = document.createElement("span");
    historyContext.className = "favorites-history-context";
    historyContext.textContent = t("currentlyViewing", { name: activeCollection?.name ?? "" });
    historyCopy.append(historyLabel, historyContext);
    historyBtn.append(historyCopy);
    historyBtn.addEventListener("click", () => selectCollection(null));
    listEl.append(historyBtn);
  }

  collections.forEach((c) => {
    const row = document.createElement("div");
    row.className = `favorites-row${selected === c.id ? " selected" : ""}`;
    row.dataset.collectionId = c.id;
    row.setAttribute("role", "option");
    row.setAttribute("aria-selected", String(selected === c.id));

    const handle = document.createElement("button");
    handle.className = "favorites-drag-handle";
    handle.setAttribute("aria-label", t("dragHandleLabel"));
    handle.title = t("dragHandleLabel");
    handle.append(icon(DRAG_HANDLE_D, 14));
    attachReorderDrag(handle, c.id);

    const name = document.createElement("span");
    name.className = "favorites-row-name";
    name.textContent = c.name;
    name.title = c.name;
    if (renamingId === c.id) {
      const editor = makeNameEditor({
        value: c.name,
        className: "favorites-rename-input",
        label: t("collectionNamePlaceholder"),
        onCommit: (v) => commitRename(c.id, v),
        onCancel: () => { renamingId = null; render(); },
      });
      row.append(handle, editor);
      editor.focus();
      selectAllContents(editor);
    } else {
      name.addEventListener("dblclick", () => { renamingId = c.id; render(); });
      row.append(handle, name);
    }

    const count = document.createElement("span");
    count.className = "favorites-row-count";
    count.textContent = String(c.item_count);
    row.append(count);

    const dropLabel = document.createElement("span");
    dropLabel.className = "favorites-row-drop-label";
    dropLabel.setAttribute("aria-hidden", "true");
    row.append(dropLabel);

    const more = document.createElement("button");
    more.className = "clip-action-btn more-btn favorites-more-btn";
    more.title = t("moreTitle");
    more.setAttribute("aria-haspopup", "menu");
    more.setAttribute("aria-expanded", String(moreMenuFor === c.id));
    more.append(icon(MORE_D, 15));
    more.addEventListener("click", (e) => {
      e.stopPropagation();
      toggleMoreMenu(c.id, more);
    });
    row.append(more);

    row.addEventListener("click", () => {
      if (renamingId !== c.id) selectCollection(c.id);
    });

    listEl.append(row);
  });

  createRow.classList.toggle("hidden", !editingCreate);
  updateMoreMenu();
  applyItemDragState();
}

function selectCollection(id: string | null): void {
  invoke("set_favorites_selected", { collectionId: id }).catch(() => {});
}

// === name editor ===
// A single-line plaintext contenteditable, shared by create and rename so both
// avoid native input/form semantics (no autofill or saved-info UI in the
// WebView). Enter commits, Escape cancels, blur commits (guarded by caller
// state), and paste is flattened to plaintext with no newlines.
function wireNameEditor(
  el: HTMLElement,
  opts: { onCommit: (value: string) => void; onCancel: () => void },
): HTMLElement {
  el.setAttribute("role", "textbox");
  el.setAttribute("contenteditable", "plaintext-only");
  el.spellcheck = false;

  el.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      e.preventDefault();
      e.stopPropagation();
      opts.onCommit(el.textContent ?? "");
    } else if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      opts.onCancel();
    } else {
      e.stopPropagation();
    }
  });

  el.addEventListener("beforeinput", (e) => {
    const ie = e as InputEvent;
    if (ie.inputType === "insertParagraph" || ie.inputType === "insertLineBreak") {
      e.preventDefault();
      return;
    }
    if (ie.inputType === "insertFromPaste") {
      const text = ie.dataTransfer?.getData("text/plain");
      if (text != null) {
        e.preventDefault();
        document.execCommand("insertText", false, text.replace(/[\r\n]+/g, " "));
      }
    }
  });

  el.addEventListener("blur", () => opts.onCommit(el.textContent ?? ""));

  return el;
}

function makeNameEditor(opts: {
  value: string;
  className: string;
  label: string;
  onCommit: (value: string) => void;
  onCancel: () => void;
}): HTMLElement {
  const el = document.createElement("div");
  el.className = opts.className;
  el.setAttribute("aria-label", opts.label);
  el.textContent = opts.value;
  return wireNameEditor(el, opts);
}

function selectAllContents(el: HTMLElement): void {
  const range = document.createRange();
  range.selectNodeContents(el);
  const sel = window.getSelection();
  sel?.removeAllRanges();
  sel?.addRange(range);
}

// === create ===
function startCreate(): void {
  editingCreate = true;
  render();
  createInput.textContent = "";
  createInput.focus();
}

function commitCreate(value: string): void {
  if (!editingCreate) return;
  const name = value.trim();
  if (!name) { cancelCreate(); return; }
  invoke<CollectionSummary>("create_collection", { name })
    .then(() => {
      editingCreate = false;
      showToast(t("collectionAdded"));
      return loadCollections();
    })
    .catch((err) => showToast(String(err)));
}

function cancelCreate(): void {
  editingCreate = false;
  render();
}

// === rename ===
function commitRename(id: string, value: string): void {
  if (renamingId !== id) return;
  renamingId = null;
  const name = value.trim();
  if (!name) { render(); return; }
  invoke("rename_collection", { id, name })
    .then(() => loadCollections())
    .catch((err) => showToast(String(err)));
}

// === more menu ===
function toggleMoreMenu(id: string, anchor: HTMLElement): void {
  moreMenuFor = moreMenuFor === id ? null : id;
  updateMoreMenu();
  if (moreMenuFor) {
    const rect = anchor.getBoundingClientRect();
    const panelRect = document.getElementById("favorites-panel")!.getBoundingClientRect();
    placeMoreMenu(rect, panelRect);
  }
}

// Same boundary-safe placement as the history More menu: prefer below, flip
// above when that side has more room, constrain height (scroll) when neither
// side fits. `updateMoreMenu` has already populated the items, so the natural
// size is measurable here.
function placeMoreMenu(rect: DOMRect, panelRect: DOMRect): void {
  moreMenu.style.maxHeight = "";
  moreMenu.style.maxWidth = "";
  const p = computeMenuPlacement(
    { top: rect.top - panelRect.top, bottom: rect.bottom - panelRect.top },
    { width: panelRect.width, height: panelRect.height },
    moreMenu.offsetWidth,
    moreMenu.offsetHeight,
    panelRect.right - rect.right,
  );
  moreMenu.style.top = `${p.top}px`;
  moreMenu.style.right = `${p.right}px`;
  moreMenu.style.maxHeight = p.maxHeight === null ? "" : `${p.maxHeight}px`;
  moreMenu.style.maxWidth = p.maxWidth === null ? "" : `${p.maxWidth}px`;
}

function updateMoreMenu(): void {
  moreMenu.replaceChildren();
  const id = moreMenuFor;
  if (!id) { moreMenu.classList.add("hidden"); return; }
  moreMenu.classList.remove("hidden");

  const col = collections.find((c) => c.id === id);
  if (!col) { moreMenu.classList.add("hidden"); return; }
  const index = collections.findIndex((c) => c.id === id);

  const rename = menuItem(t("renameCollection"), () => { renamingId = id; moreMenuFor = null; render(); });
  const up = menuItem(t("moveUp"), () => moveCollection(id, -1), index === 0);
  const down = menuItem(t("moveDown"), () => moveCollection(id, 1), index === collections.length - 1);
  const remove = menuItem(t("remove"), () => openRemoveModal(col), false, true);

  moreMenu.append(rename, up, down, remove);
}

function menuItem(label: string, onClick: () => void, disabled = false, danger = false): HTMLButtonElement {
  const b = document.createElement("button");
  b.className = `menu-item${danger ? " menu-item-delete" : ""}`;
  b.setAttribute("role", "menuitem");
  b.textContent = label;
  b.disabled = disabled;
  b.addEventListener("click", (e) => {
    e.stopPropagation();
    moreMenuFor = null;
    updateMoreMenu();
    onClick();
  });
  return b;
}

async function moveCollection(id: string, delta: number): Promise<void> {
  const fromIndex = collections.findIndex((c) => c.id === id);
  const order = moveOne(collections.map((c) => c.id), fromIndex, delta);
  await invoke("reorder_collections", { ids: order }).then(() => loadCollections()).catch((err) => { showToast(String(err)); loadCollections(); });
}

// === remove modal ===
let removeTargetId: string | null = null;
let restoreFocus: HTMLElement | null = null;

function openRemoveModal(col: CollectionSummary): void {
  removeTargetId = col.id;
  removeModalBody.textContent = t("removeCollectionBody", { name: col.name, count: String(col.item_count) });
  restoreFocus = document.activeElement as HTMLElement | null;
  removeModal.classList.remove("hidden");
  removeCancel.focus();
}

function closeRemoveModal(): void {
  removeModal.classList.add("hidden");
  removeTargetId = null;
  restoreFocus?.focus();
  restoreFocus = null;
}

function confirmRemove(): void {
  if (!removeTargetId) return;
  const id = removeTargetId;
  closeRemoveModal();
  invoke("delete_collection", { id })
    .then(() => { showToast(t("collectionRemoved")); return loadCollections(); })
    .catch((err) => showToast(String(err)));
}

// === toast ===
let toastTimer: ReturnType<typeof setTimeout> | null = null;
function showToast(message: string): void {
  if (toastTimer) clearTimeout(toastTimer);
  toast.textContent = message;
  toast.classList.remove("hidden");
  toastTimer = setTimeout(() => toast.classList.add("hidden"), 2500);
}

// === collection reorder drag (local to this window) ===
function attachReorderDrag(handle: HTMLElement, id: string): void {
  const drag = new DragController(6);
  handle.addEventListener("pointerdown", (e) => {
    e.preventDefault();
    e.stopPropagation();
    drag.pointerDown(e.clientX, e.clientY);
    handle.setPointerCapture(e.pointerId);
  });
  handle.addEventListener("pointermove", (e) => {
    if (!drag.isDragging && drag.pointerMove(e.clientX, e.clientY)) {
      beginReorder();
    }
    if (drag.isDragging) updateReorderIndicator(id, e.clientY);
  });
  handle.addEventListener("pointerup", () => {
    if (drag.didDrag) commitReorder(id);
    else drag.pointerUp();
    clearIndicator();
  });
  handle.addEventListener("pointercancel", () => { drag.pointerUp(); clearIndicator(); });
}

let indicator: HTMLDivElement | null = null;

function beginReorder(): void {
  listEl.classList.add("reordering");
}

function clearIndicator(): void {
  listEl.classList.remove("reordering");
  indicator?.remove();
  indicator = null;
}

function updateReorderIndicator(id: string, clientY: number): void {
  // Find the insertion point among the OTHER collection rows.
  const rows = [...listEl.querySelectorAll<HTMLElement>(".favorites-row[data-collection-id]")];
  let beforeId: string | null = null;
  for (const row of rows) {
    if (row.dataset.collectionId === id) continue;
    const r = row.getBoundingClientRect();
    if (clientY < r.top + r.height / 2) { beforeId = row.dataset.collectionId!; break; }
  }
  if (!indicator) {
    indicator = document.createElement("div");
    indicator.className = "drop-indicator";
    listEl.append(indicator);
  }
  const beforeRow = beforeId ? listEl.querySelector<HTMLElement>(`.favorites-row[data-collection-id="${beforeId}"]`) : null;
  if (beforeRow) beforeRow.before(indicator);
  else listEl.append(indicator);
}

async function commitReorder(id: string): Promise<void> {
  const before = indicator?.nextElementSibling as HTMLElement | null;
  const beforeId = before?.dataset.collectionId ?? null;
  const order = insertBefore(collections.map((c) => c.id), id, beforeId);
  await invoke("reorder_collections", { ids: order }).then(() => loadCollections()).catch((err) => { showToast(String(err)); loadCollections(); });
}

// === cross-window item drop target ===
function screenRect(el: HTMLElement): { left: number; top: number; right: number; bottom: number } {
  const r = el.getBoundingClientRect();
  return {
    left: windowOffset.x + r.left * scaleFactor,
    top: windowOffset.y + r.top * scaleFactor,
    right: windowOffset.x + r.right * scaleFactor,
    bottom: windowOffset.y + r.bottom * scaleFactor,
  };
}

function collectionUnderPoint(x: number, y: number): string | null {
  const rows = [...listEl.querySelectorAll<HTMLElement>(".favorites-row[data-collection-id]")];
  for (const row of rows) {
    if (rectContains(screenRect(row), x, y)) return row.dataset.collectionId ?? null;
  }
  return null;
}

function targetUnderPoint(x: number, y: number): string | null {
  const id = collectionUnderPoint(x, y);
  if (!id) return null;
  return dragMembershipReady && !isAvailableDropTarget(id, dragMembershipIds) ? null : id;
}

function highlightTarget(x: number, y: number): string | null {
  const id = targetUnderPoint(x, y);
  listEl.querySelectorAll(".favorites-row[data-collection-id]").forEach((row) => {
    row.classList.toggle("drop-target", row.getAttribute("data-collection-id") === id);
  });
  return id;
}

function clearDropFeedback(): void {
  document.body.classList.remove("item-drag-active");
  listEl.querySelectorAll(".favorites-row.drop-target, .favorites-row.drop-available, .favorites-row.drop-unavailable").forEach((row) => {
    row.classList.remove("drop-target", "drop-available", "drop-unavailable");
  });
  listEl.querySelector(".favorites-history-return")?.classList.remove("drag-disabled");
}

function applyItemDragState(): void {
  const active = activeSessionId !== null && activeDragStart !== null;
  document.body.classList.toggle("item-drag-active", active);
  listEl.querySelector(".favorites-history-return")?.classList.toggle("drag-disabled", active);
  listEl.querySelectorAll<HTMLElement>(".favorites-row[data-collection-id]").forEach((row) => {
    const id = row.dataset.collectionId!;
    const unavailable = active && dragMembershipReady && !isAvailableDropTarget(id, dragMembershipIds);
    row.classList.toggle("drop-available", active && !unavailable);
    row.classList.toggle("drop-unavailable", unavailable);
    row.setAttribute("aria-disabled", String(unavailable));
    const label = row.querySelector<HTMLElement>(".favorites-row-drop-label");
    if (label) label.textContent = unavailable ? t("alreadyInDrawer") : t("dropHere");
  });
  if (activeDragPoint) highlightTarget(activeDragPoint.x, activeDragPoint.y);
}

function finishItemDrag(): void {
  clearDropFeedback();
  activeSessionId = null;
  activeDragStart = null;
  activeDragPoint = null;
  dragMembershipIds = [];
  dragMembershipReady = false;
  dragMembershipPromise = null;
}

function beginMembershipLookup(start: ItemDragStart): void {
  dragMembershipIds = [];
  dragMembershipReady = false;
  const sessionId = start.sessionId;
  const pending = invoke<string[]>("favorite_collection_ids", { locator: start.locator }).catch(() => []);
  dragMembershipPromise = pending;
  void pending.then((ids) => {
    if (activeSessionId !== sessionId) return;
    dragMembershipIds = ids;
    dragMembershipReady = true;
    applyItemDragState();
  });
}

function beginItemDrag(start: ItemDragStart): void {
  if (!acceptDropSession(start.sessionId, activeSessionId, cancelledSessionId)) return;
  if (activeSessionId !== null && activeSessionId !== start.sessionId) finishItemDrag();
  activeSessionId = start.sessionId;
  cancelledSessionId = null;
  activeDragStart = start;
  activeDragPoint = {
    sessionId: start.sessionId,
    locator: start.locator,
    x: start.x,
    y: start.y,
  };
  beginMembershipLookup(start);
  applyItemDragState();
}

function ensureItemDragFromPoint(point: ItemDragPoint): void {
  if (activeDragStart?.sessionId === point.sessionId) return;
  beginItemDrag({
    sessionId: point.sessionId,
    locator: point.locator,
    visual: { kind: "Text", preview: t("draggingItem"), thumbnailBase64: null },
    x: point.x,
    y: point.y,
  });
}

// === shortcut ===
function onKeyDown(e: KeyboardEvent): void {
  if (matcher.keydown(e.code, e.repeat)) {
    e.preventDefault();
    e.stopPropagation();
    invoke("set_favorites_open", { open: false }).catch(() => {});
  }
}

function onKeyUp(e: KeyboardEvent): void {
  matcher.keyup(e.code);
}

// === init ===
async function init(): Promise<void> {
  try {
    await refreshConfig();
  } catch {
    setLanguage("zh-TW");
  }
  applyI18n();
  await cacheWindowGeometry();
  await Promise.all([loadCollections(), loadUiState()]);

  addBtn.addEventListener("click", () => { moreMenuFor = null; updateMoreMenu(); if (editingCreate) cancelCreate(); else startCreate(); });
  wireNameEditor(createInput, {
    onCommit: (v) => commitCreate(v),
    onCancel: () => cancelCreate(),
  });

  removeCancel.addEventListener("click", closeRemoveModal);
  removeConfirm.addEventListener("click", confirmRemove);
  removeModal.addEventListener("keydown", (e) => {
    if (e.key === "Escape") { e.preventDefault(); closeRemoveModal(); }
    else if (e.key === "Tab") trapFocus(e);
  });
  document.addEventListener("keydown", (e) => {
    if (e.key === "Escape" && moreMenuFor) { moreMenuFor = null; updateMoreMenu(); }
  });
  document.addEventListener("click", () => { if (moreMenuFor) { moreMenuFor = null; updateMoreMenu(); } });

  await listen<void>("favorites-updated", () => { void loadCollections(); });
  await listen<FavoritesUiState>("favorites-ui-state-changed", (e) => {
    selected = e.payload.selected_collection;
    render();
  });
  // The main panel asks us to enter create mode when it has no collections.
  await listen("favorites-create-request", () => startCreate());

  // Cross-window item drag: the start event carries the visual snapshot once;
  // move/end remain self-contained so a missed start can still commit safely.
  await listen<ItemDragStart>("favorites-item-drag", (e) => beginItemDrag(e.payload));
  await listen<ItemDragPoint>("favorites-item-drag-move", (e) => {
    const p = e.payload;
    if (!acceptDropSession(p.sessionId, activeSessionId, cancelledSessionId)) return;
    ensureItemDragFromPoint(p);
    activeSessionId = p.sessionId;
    activeDragPoint = p;
    highlightTarget(p.x, p.y);
  });
  await listen<ItemDragPoint>("favorites-item-drag-end", async (e) => {
    const p = e.payload;
    if (!acceptDropSession(p.sessionId, activeSessionId, cancelledSessionId)) return;
    ensureItemDragFromPoint(p);
    activeSessionId = p.sessionId;
    activeDragPoint = p;
    const rawTargetId = collectionUnderPoint(p.x, p.y);
    const membershipIds = await (dragMembershipPromise ?? Promise.resolve([]));
    if (activeSessionId !== p.sessionId) return;
    dragMembershipIds = membershipIds;
    dragMembershipReady = true;
    applyItemDragState();
    const targetId = targetUnderPoint(p.x, p.y);
    if (!targetId) {
      const duplicate = rawTargetId !== null && !isAvailableDropTarget(rawTargetId, membershipIds);
      finishItemDrag();
      if (duplicate) {
        const row = listEl.querySelector<HTMLElement>(`.favorites-row[data-collection-id="${rawTargetId}"]`);
        row?.classList.add("drop-invalid");
        setTimeout(() => row?.classList.remove("drop-invalid"), 450);
        showToast(t("alreadyInDrawer"));
      }
      return;
    }
    try {
      await invoke("add_favorite", { collectionId: targetId, locator: p.locator });
      showToast(t("addedToFavorites"));
      finishItemDrag();
      await loadCollections();
      const row = listEl.querySelector<HTMLElement>(`.favorites-row[data-collection-id="${targetId}"]`);
      if (row) {
        row.classList.add("drop-success");
        setTimeout(() => row.classList.remove("drop-success"), 600);
      }
    } catch (err) {
      finishItemDrag();
      showToast(String(err));
    }
  });
  // The source aborted (pointercancel / re-render / window change): mark the
  // session cancelled and clear any lingering target highlight.
  await listen<number>("favorites-item-drag-cancel", (e) => {
    cancelledSessionId = e.payload;
    finishItemDrag();
  });

  document.addEventListener("keydown", onKeyDown, true);
  document.addEventListener("keyup", onKeyUp, true);
  window.addEventListener("focus", () => {
    void cacheWindowGeometry();
    // Reused drawer windows only got config on init; re-apply it on focus so
    // opacity/theme/language/shortcut changes made elsewhere show up here.
    refreshConfig()
      .then(() => {
        applyI18n();
        render();
      })
      .catch(() => {});
  });
}

function trapFocus(e: KeyboardEvent): void {
  const focusables = [removeCancel, removeConfirm].filter((b) => !b.disabled);
  if (focusables.length < 2) return;
  const first = focusables[0];
  const last = focusables[focusables.length - 1];
  if (e.shiftKey && document.activeElement === first) { e.preventDefault(); last.focus(); }
  else if (!e.shiftKey && document.activeElement === last) { e.preventDefault(); first.focus(); }
}

window.addEventListener("DOMContentLoaded", () => { void init(); });
