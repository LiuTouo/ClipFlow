import { describe, expect, it } from "vitest";
import { FAVORITES_DEFAULT_CODES, ShortcutMatcher, codeLabel, shortcutLabel } from "./shortcut";

describe("ShortcutMatcher exact matching", () => {
  it("a single modifier toggles once on its press", () => {
    const m = new ShortcutMatcher(["ControlLeft"]);
    expect(m.keydown("ControlLeft", false)).toBe(true);
  });

  it("a two-key chord only toggles when both are held", () => {
    const m = new ShortcutMatcher(["ControlLeft", "KeyF"]);
    expect(m.keydown("ControlLeft", false)).toBe(false);
    expect(m.keydown("KeyF", false)).toBe(true);
  });

  it("an unrelated key does not complete the chord", () => {
    const m = new ShortcutMatcher(["ControlLeft", "KeyF"]);
    m.keydown("ControlLeft", false);
    expect(m.keydown("KeyG", false)).toBe(false);
  });

  it("a partial hold never fires for a two-key chord", () => {
    const m = new ShortcutMatcher(["ControlLeft", "ShiftLeft", "KeyV"]);
    m.keydown("ControlLeft", false);
    m.keydown("ShiftLeft", false);
    // not yet: KeyV missing
    expect(m.keydown("ControlLeft", false)).toBe(false);
  });
});

describe("repeat suppression and re-arm", () => {
  it("ignores auto-repeat of the trigger key", () => {
    const m = new ShortcutMatcher(["ControlLeft"]);
    expect(m.keydown("ControlLeft", false)).toBe(true);
    expect(m.keydown("ControlLeft", true)).toBe(false); // repeat
    expect(m.keydown("ControlLeft", true)).toBe(false);
  });

  it("does not double-toggle while still held", () => {
    const m = new ShortcutMatcher(["ControlLeft"]);
    expect(m.keydown("ControlLeft", false)).toBe(true);
    // A second distinct keydown with the trigger still held is not a new toggle.
    expect(m.keydown("ControlLeft", false)).toBe(false);
  });

  it("re-arms after release so the next press toggles again", () => {
    const m = new ShortcutMatcher(["ControlLeft"]);
    expect(m.keydown("ControlLeft", false)).toBe(true);
    m.keyup("ControlLeft");
    expect(m.keydown("ControlLeft", false)).toBe(true);
  });

  it("does not re-arm until every target key is released", () => {
    const m = new ShortcutMatcher(["ControlLeft", "KeyF"]);
    m.keydown("ControlLeft", false);
    m.keydown("KeyF", false);
    m.keyup("KeyF");
    // ControlLeft still held → not re-armed.
    expect(m.keydown("KeyF", false)).toBe(false);
    m.keyup("ControlLeft");
    m.keydown("ControlLeft", false);
    expect(m.keydown("KeyF", false)).toBe(true);
  });
});

describe("labels", () => {
  it("labels sided modifiers", () => {
    expect(codeLabel("ControlLeft")).toBe("Ctrl Left");
    expect(codeLabel("MetaRight")).toBe("Meta Right");
  });

  it("labels function and printable keys", () => {
    expect(codeLabel("F12")).toBe("F12");
    expect(codeLabel("KeyA")).toBe("A");
    expect(codeLabel("Digit0")).toBe("0");
  });

  it("joins a chord for display", () => {
    expect(shortcutLabel(["ControlLeft", "KeyF"])).toBe("Ctrl Left + F");
  });
});

describe("default favorites chord", () => {
  it("is Left Alt, matching PanelShortcut::default in the backend", () => {
    expect(FAVORITES_DEFAULT_CODES).toEqual(["AltLeft"]);
  });
});
