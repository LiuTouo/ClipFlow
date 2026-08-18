// Pure pointer-drag state machine shared by item drag (history/favorite row to
// a sidebar collection) and collection reorder (sidebar handle). No DOM: callers
// feed pointer coordinates and read the decisions; tests drive it directly.

import type { Clip, FavoriteItem } from "./types";

export type DragPhase = "idle" | "pending" | "dragging";

/** Cross-window drag payload, typed so a collection reorder is never mistaken
 * for an item drop (and vice versa). Emitted on the `favorites-drag` channel. */
export type FavoritesDragPayload =
  | { kind: "item"; locator: { scope: "history" | "favorite"; id: string } }
  | { kind: "collection"; collectionId: string };

/** A single pointer movement threshold gate. Distinguishes a real drag from a
 * click jitter, and remembers whether a drag completed so the caller can
 * suppress the click that would otherwise follow a drop. */
export class DragController {
  private phase: DragPhase = "idle";
  private startX = 0;
  private startY = 0;
  private completed = false;
  private readonly thresholdPx: number;

  constructor(thresholdPx = 6) {
    this.thresholdPx = thresholdPx;
  }

  pointerDown(x: number, y: number): void {
    this.phase = "pending";
    this.startX = x;
    this.startY = y;
    this.completed = false;
  }

  /** Begin a drag on pointerdown without waiting for movement. Item drag
   * handles use this because the handle has no click action; collection
   * reordering keeps the threshold-based pointerDown/pointerMove path. */
  beginImmediately(x: number, y: number): void {
    this.pointerDown(x, y);
    this.phase = "dragging";
    this.completed = true;
  }

  /** Advances state; returns true on the movement that crosses the threshold. */
  pointerMove(x: number, y: number): boolean {
    if (this.phase !== "pending") return false;
    const dist = Math.hypot(x - this.startX, y - this.startY);
    if (dist >= this.thresholdPx) {
      this.phase = "dragging";
      this.completed = true;
      return true;
    }
    return false;
  }

  pointerUp(): void {
    this.phase = "idle";
  }

  get isDragging(): boolean {
    return this.phase === "dragging";
  }

  /** True once a drag actually began (threshold crossed), even after pointer up —
   * lets the caller suppress the synthetic click that follows a completed drag. */
  get didDrag(): boolean {
    return this.completed;
  }

  reset(): void {
    this.phase = "idle";
    this.completed = false;
  }
}

/** A history Clip carries a `pinned` flag; a FavoriteItem does not. */
export function isFavoriteItem(item: Clip | FavoriteItem): item is FavoriteItem {
  return !("pinned" in item);
}

/** Build the item-drop locator for a history Clip or a drawer FavoriteItem.
 * Each type's `id` is the right key for its scope: a Clip id for `history`,
 * a FavoriteItem's content hash for `favorite`. */
export function clipLocator(item: Clip | FavoriteItem): { scope: "history" | "favorite"; id: string } {
  return isFavoriteItem(item) ? { scope: "favorite", id: item.id } : { scope: "history", id: item.id };
}

/** Build the item-drag payload for a history or favorite item. */
export function itemDragPayload(scope: "history" | "favorite", id: string): FavoritesDragPayload {
  return { kind: "item", locator: { scope, id } };
}

/** Build the collection-reorder payload for a sidebar handle drag. */
export function collectionDragPayload(collectionId: string): FavoritesDragPayload {
  return { kind: "collection", collectionId };
}

/** Hit-test: is a viewport point inside a DOM rect? */
export function rectContains(
  rect: { left: number; top: number; right: number; bottom: number },
  x: number,
  y: number,
): boolean {
  return x >= rect.left && x <= rect.right && y >= rect.top && y <= rect.bottom;
}

/** Physical (device-pixel) screen coordinates — the space the sidebar
 * hit-tests in. Pointer `screenX/Y` are CSS pixels and lie at non-100% DPI, so
 * the source window converts a client CSS point through this helper before
 * emitting, and the receiver's rects are already in this space. */
export interface PhysicalPoint {
  x: number;
  y: number;
}

/** Convert a window-relative CSS-pixel point into physical screen pixels. */
export function physicalScreenPoint(
  windowPhysicalOrigin: { x: number; y: number },
  clientCssPoint: { x: number; y: number },
  scale: number,
): PhysicalPoint {
  return {
    x: windowPhysicalOrigin.x + clientCssPoint.x * scale,
    y: windowPhysicalOrigin.y + clientCssPoint.y * scale,
  };
}

/** A move/end point for a cross-window item drop. `sessionId` is a monotonic
 * counter from the source window, and `locator` is carried so the receiver can
 * resolve the drop without any prior start event. */
export interface ItemDragPoint {
  sessionId: number;
  locator: { scope: "history" | "favorite"; id: string };
  x: number;
  y: number;
}

/** Lightweight visual data sent once when an item drag begins. Movement events
 * intentionally carry only coordinates so thumbnails are never re-broadcast on
 * every pointermove. */
export interface ItemDragVisual {
  kind: Clip["kind"];
  preview: string;
  thumbnailBase64: string | null;
}

export interface ItemDragStart {
  sessionId: number;
  locator: { scope: "history" | "favorite"; id: string };
  visual: ItemDragVisual;
  x: number;
  y: number;
}

/** Build the one-shot cross-window drag-start payload. */
export function itemDragStartPayload(
  sessionId: number,
  item: Clip | FavoriteItem,
  point: PhysicalPoint,
): ItemDragStart {
  return {
    sessionId,
    locator: clipLocator(item),
    visual: {
      kind: item.kind,
      preview: item.preview,
      thumbnailBase64: item.kind === "Image" ? item.thumbnail_base64 : null,
    },
    x: point.x,
    y: point.y,
  };
}

/** A drawer already containing the item is visible feedback, not a valid drop
 * target. Kept pure so the async membership lookup and DOM hit-testing share
 * exactly the same decision. */
export function isAvailableDropTarget(collectionId: string, membershipIds: readonly string[]): boolean {
  return !membershipIds.includes(collectionId);
}

export interface PhysicalRect {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/** Place the standalone drag-overlay window beside the cursor, flipping to the
 * opposite side at work-area edges. Every value is in physical pixels. */
export function placeDragOverlay(
  point: PhysicalPoint,
  size: { width: number; height: number },
  workArea: PhysicalRect,
  offset: number,
): PhysicalPoint {
  const maxX = Math.max(workArea.left, workArea.right - size.width);
  const maxY = Math.max(workArea.top, workArea.bottom - size.height);
  let x = point.x + offset;
  let y = point.y + offset;
  if (x + size.width > workArea.right) x = point.x - size.width - offset;
  if (y + size.height > workArea.bottom) y = point.y - size.height - offset;
  return {
    x: Math.min(maxX, Math.max(workArea.left, x)),
    y: Math.min(maxY, Math.max(workArea.top, y)),
  };
}

/** Receiver-side gate for a cross-window drop point. Rejects a cancelled
 * session and any session older than the newest already seen, so a stale `end`
 * after a cancel or a newer drag cannot commit a bogus drop. */
export function acceptDropSession(
  sessionId: number,
  currentSessionId: number | null,
  cancelledSessionId: number | null,
): boolean {
  if (sessionId === cancelledSessionId) return false;
  if (currentSessionId !== null && sessionId < currentSessionId) return false;
  return true;
}
