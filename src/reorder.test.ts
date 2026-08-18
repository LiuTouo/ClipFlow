import { describe, expect, it } from "vitest";
import { insertBefore, insertionIndex, moveItem, moveOne } from "./reorder";

describe("moveItem", () => {
  it("moves an item forward", () => {
    expect(moveItem(["a", "b", "c", "d"], 1, 3)).toEqual(["a", "c", "d", "b"]);
  });

  it("moves an item backward", () => {
    expect(moveItem(["a", "b", "c", "d"], 3, 0)).toEqual(["d", "a", "b", "c"]);
  });

  it("is a no-op for same index", () => {
    expect(moveItem(["a", "b", "c"], 1, 1)).toEqual(["a", "b", "c"]);
  });

  it("returns a clone unchanged for out-of-range", () => {
    expect(moveItem(["a", "b"], -1, 0)).toEqual(["a", "b"]);
    expect(moveItem(["a", "b"], 5, 0)).toEqual(["a", "b"]);
  });
});

describe("moveOne up/down boundaries", () => {
  const abc = ["A", "B", "C"];

  it("moves B down one slot", () => {
    expect(moveOne(abc, 1, 1)).toEqual(["A", "C", "B"]);
  });

  it("moves B up one slot", () => {
    expect(moveOne(abc, 1, -1)).toEqual(["B", "A", "C"]);
  });

  it("moves the first item down one slot", () => {
    expect(moveOne(abc, 0, 1)).toEqual(["B", "A", "C"]);
  });

  it("moves the last item up one slot", () => {
    expect(moveOne(abc, 2, -1)).toEqual(["A", "C", "B"]);
  });

  it("first item cannot move up (boundary no-op)", () => {
    expect(moveOne(abc, 0, -1)).toEqual(abc);
  });

  it("last item cannot move down (boundary no-op)", () => {
    expect(moveOne(abc, 2, 1)).toEqual(abc);
  });
});

describe("insertBefore / insertionIndex", () => {
  it("moves an id before a target", () => {
    expect(insertBefore(["a", "b", "c", "d"], "d", "b")).toEqual(["a", "d", "b", "c"]);
  });

  it("appends when beforeId is null", () => {
    expect(insertBefore(["a", "b", "c"], "a", null)).toEqual(["b", "c", "a"]);
  });

  it("appends when the target id is missing", () => {
    expect(insertBefore(["a", "b"], "b", "nope")).toEqual(["a", "b"]);
  });

  it("reports the insertion index for a drop before a row", () => {
    expect(insertionIndex(["a", "b", "c", "d"], "d", "b")).toBe(1);
    expect(insertionIndex(["a", "b", "c"], "a", null)).toBe(2);
  });
});
