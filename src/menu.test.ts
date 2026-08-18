import { describe, expect, it } from "vitest";
import { ChooserGate, computeMenuPlacement } from "./menu";

const PANEL = { width: 420, height: 540 };

describe("computeMenuPlacement", () => {
  it("opens below a top anchor when the menu fits", () => {
    const p = computeMenuPlacement(
      { top: 100, bottom: 136 },
      PANEL,
      40,
      30,
      40,
    );
    expect(p.flip).toBe(false);
    expect(p.top).toBe(140);
    expect(p.right).toBe(40);
    expect(p.maxHeight).toBeNull();
    expect(p.maxWidth).toBeNull();
  });

  it("flips above a bottom-edge anchor when there is no room below", () => {
    const p = computeMenuPlacement(
      { top: 480, bottom: 516 },
      PANEL,
      40,
      30,
      40,
    );
    expect(p.flip).toBe(true);
    expect(p.top).toBe(446);
    expect(p.maxHeight).toBeNull();
  });

  it("clamps right so the menu's left edge stays inside a narrow panel", () => {
    // Anchor near the panel's left edge (right offset 140 from panel width 200);
    // a 150px menu would otherwise overflow 10px past the left edge.
    const p = computeMenuPlacement(
      { top: 100, bottom: 136 },
      { width: 200, height: 540 },
      150,
      30,
      140,
    );
    expect(p.right).toBe(50);
    expect(p.maxWidth).toBeNull();
  });

  it("caps width when the menu is wider than the panel", () => {
    const p = computeMenuPlacement(
      { top: 100, bottom: 136 },
      { width: 120, height: 540 },
      150,
      30,
      40,
    );
    expect(p.right).toBe(0);
    expect(p.maxWidth).toBe(120);
  });

  it("clamps a negative right offset to 0", () => {
    const p = computeMenuPlacement(
      { top: 100, bottom: 136 },
      PANEL,
      40,
      30,
      -10,
    );
    expect(p.right).toBe(0);
  });

  it("constrains an oversized menu to the space below and scrolls", () => {
    const p = computeMenuPlacement(
      { top: 200, bottom: 220 },
      PANEL,
      40,
      600,
      40,
    );
    expect(p.flip).toBe(false);
    expect(p.top).toBe(224);
    expect(p.maxHeight).toBe(316); // 540 - 220 - 4
  });

  it("flips and constrains an oversized menu to the space above", () => {
    const p = computeMenuPlacement(
      { top: 480, bottom: 500 },
      PANEL,
      40,
      600,
      40,
    );
    expect(p.flip).toBe(true);
    expect(p.top).toBe(0);
    expect(p.maxHeight).toBe(476); // 480 - 4
  });
});

describe("ChooserGate stale-state guard", () => {
  it("reports a request current only until a newer open advances it", () => {
    const gate = new ChooserGate();
    expect(gate.isOpen).toBe(false);

    const tokenA = gate.open("A");
    expect(gate.isOpen).toBe(true);
    expect(gate.isCurrent("A", tokenA)).toBe(true);
    expect(gate.isCurrent("B", tokenA)).toBe(false);

    const tokenB = gate.open("B");
    expect(gate.isCurrent("A", tokenA)).toBe(false);
    expect(gate.isCurrent("B", tokenB)).toBe(true);
  });

  it("close invalidates any pending request", () => {
    const gate = new ChooserGate();
    const token = gate.open("A");
    gate.close();
    expect(gate.isOpen).toBe(false);
    expect(gate.isCurrent("A", token)).toBe(false);
  });

  it("rejects a stale response after close then reopen the same id", () => {
    const gate = new ChooserGate();
    const token = gate.open("A");
    gate.close();
    gate.open("A");
    expect(gate.isCurrent("A", token)).toBe(false);
  });
});
