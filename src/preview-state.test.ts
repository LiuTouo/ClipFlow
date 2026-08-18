import { describe, expect, it } from "vitest";
import { PreviewController } from "./preview-state";

function openController(id = "clip-a"): PreviewController {
  const c = new PreviewController();
  c.beginShow(id);
  return c;
}

describe("open / close", () => {
  it("beginShow marks the preview open with the clip id", () => {
    const c = new PreviewController();
    c.beginShow("clip-a");
    expect(c.isOpen).toBe(true);
    expect(c.currentId).toBe("clip-a");
  });

  it("a hide is not confirmed until resolveHide", () => {
    const c = openController("clip-a");
    c.beginHide();
    expect(c.isOpen).toBe(true);
    expect(c.currentId).toBe("clip-a");
  });

  it("a confirmed hide closes the preview", () => {
    const c = openController("clip-a");
    const token = c.beginHide();
    c.resolveHide(token);
    expect(c.isOpen).toBe(false);
    expect(c.currentId).toBeNull();
  });
});

describe("Space keydown decision", () => {
  it("opens on a non-repeat Space over a hovered row", () => {
    const c = new PreviewController();
    expect(c.decideSpaceKeydown(false, "clip-a", null)).toEqual({ type: "open", id: "clip-a" });
  });

  it("opens on the selected row when nothing is hovered", () => {
    const c = new PreviewController();
    expect(c.decideSpaceKeydown(false, null, "clip-selected")).toEqual({ type: "open", id: "clip-selected" });
  });

  it("prefers the hovered row over the selected row", () => {
    const c = new PreviewController();
    expect(c.decideSpaceKeydown(false, "clip-hover", "clip-selected")).toEqual({ type: "open", id: "clip-hover" });
  });

  it("closes on a non-repeat Space while open, even with no hover", () => {
    const c = openController("clip-a");
    expect(c.decideSpaceKeydown(false, null, null)).toEqual({ type: "close" });
  });

  it("ignores a non-repeat Space with no hovered or selected row", () => {
    const c = new PreviewController();
    expect(c.decideSpaceKeydown(false, null, null)).toEqual({ type: "ignore" });
  });

  it("swallows the auto-repeat of a consumed Space press", () => {
    const c = new PreviewController();
    c.consumeSpace();
    expect(c.decideSpaceKeydown(true, "clip-a", null)).toEqual({ type: "swallow" });
  });

  it("keyup re-arms: a repeat after release is ignored", () => {
    const c = new PreviewController();
    c.consumeSpace();
    c.releaseSpace();
    expect(c.decideSpaceKeydown(true, "clip-a", null)).toEqual({ type: "ignore" });
  });
});

describe("races and stale completions", () => {
  it("a stale hide completion cannot clear a newer show", () => {
    const c = openController("clip-a");
    const hideToken = c.beginHide(); // Space to close
    c.beginShow("clip-b"); // hover B + Space before the hide resolved
    c.resolveHide(hideToken); // stale
    expect(c.currentId).toBe("clip-b");
  });

  it("a stale resync cannot overwrite a newer show", () => {
    const c = openController("clip-a");
    const resyncToken = c.beginResync();
    c.beginShow("clip-b");
    c.resolveResync(resyncToken, null);
    expect(c.currentId).toBe("clip-b");
  });

  it("rapid row changes keep only the newest id", () => {
    const c = openController("clip-a");
    c.beginShow("clip-b");
    c.beginShow("clip-c");
    expect(c.currentId).toBe("clip-c");
  });
});

describe("show completion vs resync ordering", () => {
  it("a show that commits after an earlier resync-null reopens the preview", () => {
    const c = new PreviewController();
    const showToken = c.beginShow("clip-a"); // optimistic open
    const resyncToken = c.beginResync(); // read captures the mutation, no bump
    c.resolveResync(resyncToken, null); // read saw the backend still null
    expect(c.isOpen).toBe(false);
    c.resolveShow(showToken, "clip-a"); // backend show then committed
    expect(c.isOpen).toBe(true);
    expect(c.currentId).toBe("clip-a");
  });

  it("a stale show commit cannot reopen after a newer hide confirmed", () => {
    const c = new PreviewController();
    const showToken = c.beginShow("clip-a");
    const hideToken = c.beginHide();
    c.resolveHide(hideToken); // hide confirmed → closed
    c.resolveShow(showToken, "clip-a"); // stale show commit arrives late
    expect(c.isOpen).toBe(false);
  });

  it("a stale show commit cannot reopen after a newer show", () => {
    const c = new PreviewController();
    const staleToken = c.beginShow("clip-a");
    c.beginShow("clip-b");
    c.resolveShow(staleToken, "clip-a"); // stale
    expect(c.currentId).toBe("clip-b");
  });
});

describe("dataset/visibility changes and target switching", () => {
  it("switching preview target while open stays open", () => {
    const c = openController("clip-a");
    c.beginShow("clip-b"); // hover a different item while preview is open
    expect(c.isOpen).toBe(true);
    expect(c.currentId).toBe("clip-b");
  });

  it("a resync reading the same still-active id does not close the preview", () => {
    const c = openController("clip-a");
    const token = c.beginResync(); // dataset switch triggers a backend read
    c.resolveResync(token, "clip-a"); // backend still shows clip-a
    expect(c.isOpen).toBe(true);
    expect(c.currentId).toBe("clip-a");
  });

  it("a non-repeat Space while open closes regardless of the hovered id", () => {
    const c = openController("clip-a");
    expect(c.decideSpaceKeydown(false, "clip-b", null)).toEqual({ type: "close" });
    expect(c.decideSpaceKeydown(false, null, "clip-b")).toEqual({ type: "close" });
  });
});

describe("backend-authoritative resync", () => {
  it("backend focus-loss hide resyncs to closed", () => {
    const c = openController("clip-a");
    const token = c.beginResync();
    c.resolveResync(token, null);
    expect(c.isOpen).toBe(false);
    expect(c.currentId).toBeNull();
  });

  it("preview-window close notification resyncs from backend truth", () => {
    const c = openController("clip-a");
    const token = c.beginResync(); // fired by clip-preview-closed
    c.resolveResync(token, null); // backend confirms nothing is active
    expect(c.currentId).toBeNull();
  });

  it("a hide failure resyncs to the backend's still-open preview", () => {
    const c = openController("clip-a");
    const hideToken = c.beginHide();
    expect(c.isCurrent(hideToken)).toBe(true);
    const resyncToken = c.beginResync();
    c.resolveResync(resyncToken, "clip-a"); // backend still shows clip-a
    expect(c.currentId).toBe("clip-a");
  });

  it("a superseded show failure does not trigger resync", () => {
    const c = new PreviewController();
    const showToken = c.beginShow("clip-a");
    c.beginShow("clip-b"); // superseded
    expect(c.isCurrent(showToken)).toBe(false);
  });
});
