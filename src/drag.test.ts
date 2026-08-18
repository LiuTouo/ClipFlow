import { describe, expect, it } from "vitest";
import {
  DragController,
  itemDragPayload,
  clipLocator,
  rectContains,
  physicalScreenPoint,
  acceptDropSession,
  itemDragStartPayload,
  isAvailableDropTarget,
  placeDragOverlay,
} from "./drag";
import type { Clip, FavoriteItem } from "./types";

function clipFixture(id: string): Clip {
  return {
    id,
    kind: "Text",
    text_content: null,
    thumbnail_base64: null,
    content_hash: id,
    preview: "",
    truncated: false,
    source_exe: "",
    source_title: "",
    source_icon: null,
    captured_at: 0,
    pinned: false,
    byte_size: 0,
  };
}

function favoriteFixture(id: string): FavoriteItem {
  return {
    id,
    kind: "Text",
    text_content: null,
    thumbnail_base64: null,
    content_hash: id,
    preview: "",
    truncated: false,
    source_exe: "",
    source_title: "",
    source_icon: null,
    captured_at: 0,
    byte_size: 0,
    added_at: null,
  };
}

describe("DragController threshold", () => {
  it("stays pending under the threshold", () => {
    const d = new DragController(6);
    d.pointerDown(0, 0);
    expect(d.pointerMove(3, 4)).toBe(false); // hypot 5 < 6
    expect(d.isDragging).toBe(false);
    expect(d.didDrag).toBe(false);
  });

  it("begins dragging once the threshold is crossed", () => {
    const d = new DragController(6);
    d.pointerDown(0, 0);
    expect(d.pointerMove(6, 0)).toBe(true);
    expect(d.isDragging).toBe(true);
    expect(d.didDrag).toBe(true);
  });

  it("remembers the drag completed for click suppression after pointer up", () => {
    const d = new DragController(6);
    d.pointerDown(0, 0);
    d.pointerMove(10, 0);
    d.pointerUp();
    expect(d.isDragging).toBe(false);
    expect(d.didDrag).toBe(true);
  });

  it("a click without movement is not a drag", () => {
    const d = new DragController(6);
    d.pointerDown(5, 5);
    d.pointerUp();
    expect(d.didDrag).toBe(false);
  });

  it("can begin immediately on pointerdown for a dedicated drag handle", () => {
    const d = new DragController(6);
    d.beginImmediately(10, 20);
    expect(d.isDragging).toBe(true);
    expect(d.didDrag).toBe(true);
  });
});

describe("payloads", () => {
  it("item drag carries the scope and id", () => {
    expect(itemDragPayload("history", "clip-a")).toEqual({
      kind: "item",
      locator: { scope: "history", id: "clip-a" },
    });
    expect(itemDragPayload("favorite", "hash-x")).toEqual({
      kind: "item",
      locator: { scope: "favorite", id: "hash-x" },
    });
  });

  it("item drag start carries one lightweight visual snapshot", () => {
    const clip = clipFixture("clip-a");
    clip.preview = "Selected text";
    expect(itemDragStartPayload(7, clip, { x: 120, y: 80 })).toEqual({
      sessionId: 7,
      locator: { scope: "history", id: "clip-a" },
      visual: { kind: "Text", preview: "Selected text", thumbnailBase64: null },
      x: 120,
      y: 80,
    });

    const image = favoriteFixture("image-hash");
    image.kind = "Image";
    image.thumbnail_base64 = "data:image/png;base64,thumb";
    expect(itemDragStartPayload(8, image, { x: 0, y: 0 }).visual.thumbnailBase64).toBe("data:image/png;base64,thumb");
  });
});

describe("drag overlay placement", () => {
  const workArea = { left: 0, top: 0, right: 1920, bottom: 1040 };
  const size = { width: 288, height: 112 };

  it("sits below and to the right when space is available", () => {
    expect(placeDragOverlay({ x: 500, y: 300 }, size, workArea, 16)).toEqual({ x: 516, y: 316 });
  });

  it("flips left and up at the far work-area edges", () => {
    expect(placeDragOverlay({ x: 1900, y: 1020 }, size, workArea, 16)).toEqual({ x: 1596, y: 892 });
  });

  it("clamps correctly on a negative-origin monitor", () => {
    const negative = { left: -1920, top: -200, right: 0, bottom: 880 };
    expect(placeDragOverlay({ x: -10, y: -190 }, size, negative, 16)).toEqual({ x: -314, y: -174 });
  });
});

describe("drop target availability", () => {
  it("rejects drawers that already contain the item", () => {
    expect(isAvailableDropTarget("drawer-a", ["drawer-a", "drawer-b"])).toBe(false);
  });

  it("accepts drawers that do not contain the item", () => {
    expect(isAvailableDropTarget("drawer-c", ["drawer-a", "drawer-b"])).toBe(true);
  });
});

describe("clipLocator", () => {
  it("maps a history Clip to a history locator", () => {
    expect(clipLocator(clipFixture("clip-a"))).toEqual({ scope: "history", id: "clip-a" });
  });
  it("maps a FavoriteItem to a favorite locator", () => {
    expect(clipLocator(favoriteFixture("hash-x"))).toEqual({ scope: "favorite", id: "hash-x" });
  });
});

describe("rectContains", () => {
  const rect = { left: 0, top: 0, right: 100, bottom: 40 };
  it("hits inside", () => {
    expect(rectContains(rect, 50, 20)).toBe(true);
  });
  it("misses outside", () => {
    expect(rectContains(rect, 101, 20)).toBe(false);
    expect(rectContains(rect, 50, 41)).toBe(false);
  });
});

describe("physicalScreenPoint", () => {
  const origin = { x: 128, y: 64 };
  it("100% DPI leaves the client point unscaled", () => {
    expect(physicalScreenPoint(origin, { x: 40, y: 20 }, 1)).toEqual({ x: 168, y: 84 });
  });
  it("125% DPI scales the client point", () => {
    expect(physicalScreenPoint(origin, { x: 40, y: 20 }, 1.25)).toEqual({ x: 178, y: 89 });
  });
  it("150% DPI scales the client point", () => {
    expect(physicalScreenPoint(origin, { x: 40, y: 20 }, 1.5)).toEqual({ x: 188, y: 94 });
  });
});

describe("acceptDropSession gate", () => {
  it("accepts an end with no prior start", () => {
    expect(acceptDropSession(1, null, null)).toBe(true);
  });
  it("rejects a session older than the newest seen", () => {
    expect(acceptDropSession(2, 5, null)).toBe(false);
  });
  it("accepts a session at least as new as the newest seen", () => {
    expect(acceptDropSession(5, 5, null)).toBe(true);
    expect(acceptDropSession(6, 5, null)).toBe(true);
  });
  it("rejects an end for a cancelled session", () => {
    expect(acceptDropSession(3, null, 3)).toBe(false);
  });
  it("rejects a stale end after a cancel then a newer drag", () => {
    expect(acceptDropSession(3, 4, 3)).toBe(false);
    expect(acceptDropSession(4, 4, 3)).toBe(true);
  });
});
