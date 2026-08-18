// Standalone transparent window that owns the only floating item-drag card.
// Because the card is itself a native window, it can cross the main/sidebar
// WebView boundary without either window clipping it.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import {
  availableMonitors,
  getCurrentWindow,
  PhysicalPosition,
  type Monitor,
} from "@tauri-apps/api/window";
import { acceptDropSession, placeDragOverlay } from "./drag";
import type { ItemDragPoint, ItemDragStart, ItemDragVisual } from "./drag";
import { setLanguage, t } from "./i18n";
import { applyTheme } from "./theme";

interface AppConfig {
  language?: string;
  theme?: string;
}

const OVERLAY_LOGICAL_WIDTH = 288;
const OVERLAY_LOGICAL_HEIGHT = 112;
const CURSOR_OFFSET_LOGICAL = 14;

const overlayWindow = getCurrentWindow();
const card = document.getElementById("drag-overlay-card")!;
let monitors: Monitor[] = [];
let activeSessionId: number | null = null;
let cancelledSessionId: number | null = null;
let pendingPoint: { sessionId: number; x: number; y: number } | null = null;
let moveFrame: number | null = null;
let moveInFlight = false;
let visible = false;
let hideTimer: ReturnType<typeof setTimeout> | null = null;

function monitorForPoint(x: number, y: number): Monitor | null {
  return monitors.find((monitor) => {
    const wa = monitor.workArea;
    return x >= wa.position.x
      && x <= wa.position.x + wa.size.width
      && y >= wa.position.y
      && y <= wa.position.y + wa.size.height;
  }) ?? monitors[0] ?? null;
}

function renderVisual(visual: ItemDragVisual): void {
  card.replaceChildren();
  const visualEl = document.createElement("div");
  if (visual.kind === "Image" && visual.thumbnailBase64) {
    visualEl.className = "item-drag-preview-visual image";
    const image = document.createElement("img");
    image.src = visual.thumbnailBase64;
    image.alt = "";
    visualEl.append(image);
  } else {
    visualEl.className = "item-drag-preview-visual kind";
    visualEl.textContent = visual.kind === "FilePaths" ? "F" : "T";
  }

  const copy = document.createElement("div");
  copy.className = "item-drag-preview-copy";
  const label = document.createElement("span");
  label.className = "item-drag-preview-label";
  label.textContent = t("draggingItem");
  const preview = document.createElement("span");
  preview.className = "item-drag-preview-text";
  preview.textContent = visual.preview || t("emptyPreview");
  copy.append(label, preview);

  const add = document.createElement("span");
  add.className = "item-drag-preview-add";
  add.textContent = "+";
  card.append(visualEl, copy, add);
}

function scheduleMove(sessionId: number, x: number, y: number): void {
  pendingPoint = { sessionId, x, y };
  if (moveFrame !== null || moveInFlight) return;
  moveFrame = requestAnimationFrame(() => {
    moveFrame = null;
    void flushMove();
  });
}

function moveImmediately(sessionId: number, x: number, y: number): void {
  pendingPoint = { sessionId, x, y };
  if (moveFrame !== null) cancelAnimationFrame(moveFrame);
  moveFrame = null;
  if (!moveInFlight) void flushMove();
}

function schedulePendingMove(): void {
  const next = pendingPoint;
  if (next && activeSessionId === next.sessionId) {
    scheduleMove(next.sessionId, next.x, next.y);
  }
}

async function flushMove(): Promise<void> {
  const point = pendingPoint;
  pendingPoint = null;
  if (!point || activeSessionId !== point.sessionId) return;
  moveInFlight = true;
  try {
    if (monitors.length === 0) monitors = await availableMonitors();
    const monitor = monitorForPoint(point.x, point.y);
    const scale = monitor?.scaleFactor ?? 1;
    const workArea = monitor
      ? {
          left: monitor.workArea.position.x,
          top: monitor.workArea.position.y,
          right: monitor.workArea.position.x + monitor.workArea.size.width,
          bottom: monitor.workArea.position.y + monitor.workArea.size.height,
        }
      : { left: point.x - 2000, top: point.y - 2000, right: point.x + 2000, bottom: point.y + 2000 };
    const position = placeDragOverlay(
      { x: point.x, y: point.y },
      { width: OVERLAY_LOGICAL_WIDTH * scale, height: OVERLAY_LOGICAL_HEIGHT * scale },
      workArea,
      CURSOR_OFFSET_LOGICAL * scale,
    );
    await overlayWindow.setPosition(new PhysicalPosition(Math.round(position.x), Math.round(position.y)));
    if (activeSessionId === point.sessionId && !visible) {
      // Reassert the native topmost band whenever a new drag shows the reused
      // overlay. A hidden, non-focusable window can otherwise return behind
      // another application even though it was created as always-on-top.
      await overlayWindow.setAlwaysOnTop(true);
      await overlayWindow.show();
      visible = true;
    }
  } catch {
    // Source/target row feedback remains usable if the overlay cannot move.
  } finally {
    moveInFlight = false;
    schedulePendingMove();
  }
}

async function hideOverlay(): Promise<void> {
  visible = false;
  card.classList.add("hidden");
  card.classList.remove("dropping", "leaving");
  await overlayWindow.hide().catch(() => {});
}

function finishDrag(cancelled: boolean): void {
  activeSessionId = null;
  pendingPoint = null;
  if (moveFrame !== null) cancelAnimationFrame(moveFrame);
  moveFrame = null;
  if (hideTimer) clearTimeout(hideTimer);
  const reduceMotion = matchMedia("(prefers-reduced-motion: reduce)").matches;
  if (cancelled || reduceMotion || !visible) {
    void hideOverlay();
    return;
  }
  card.classList.add("dropping");
  hideTimer = setTimeout(() => { void hideOverlay(); }, 180);
}

function beginDrag(start: ItemDragStart): void {
  if (!acceptDropSession(start.sessionId, activeSessionId, cancelledSessionId)) return;
  if (hideTimer) clearTimeout(hideTimer);
  hideTimer = null;
  activeSessionId = start.sessionId;
  cancelledSessionId = null;
  renderVisual(start.visual);
  card.classList.remove("hidden", "dropping", "leaving");
  moveImmediately(start.sessionId, start.x, start.y);
}

async function init(): Promise<void> {
  try {
    const config = await invoke<AppConfig>("get_config");
    setLanguage(config.language || "zh-TW");
    applyTheme(config.theme || "system");
  } catch {
    setLanguage("zh-TW");
  }
  monitors = await availableMonitors().catch(() => []);

  await listen<ItemDragStart>("favorites-item-drag", (event) => beginDrag(event.payload));
  await listen<ItemDragPoint>("favorites-item-drag-move", (event) => {
    const point = event.payload;
    if (!acceptDropSession(point.sessionId, activeSessionId, cancelledSessionId)) return;
    if (activeSessionId === null) {
      beginDrag({
        sessionId: point.sessionId,
        locator: point.locator,
        visual: { kind: "Text", preview: t("draggingItem"), thumbnailBase64: null },
        x: point.x,
        y: point.y,
      });
    }
    scheduleMove(point.sessionId, point.x, point.y);
  });
  await listen<ItemDragPoint>("favorites-item-drag-end", (event) => {
    if (!acceptDropSession(event.payload.sessionId, activeSessionId, cancelledSessionId)) return;
    finishDrag(false);
  });
  await listen<number>("favorites-item-drag-cancel", (event) => {
    cancelledSessionId = event.payload;
    finishDrag(true);
  });
}

window.addEventListener("DOMContentLoaded", () => { void init(); });
