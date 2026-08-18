// Pure shortcut handling for the favorites sidebar toggle. No DOM, no Tauri:
// main.ts and favorites.ts both feed it keyboard events; tests drive it directly.
//
// The chord is a set of physical `KeyboardEvent.code` values that must all be
// held at once. The backend already validates the stored set (see
// PanelShortcut::validate in models.rs); this module only matches and displays.

/**
 * The backend-default favorites toggle chord. Must stay in sync with
 * `PanelShortcut::default` in src-tauri/src/models.rs (Left Alt). Every
 * frontend fallback uses this so a missing field never drifts from the Rust
 * default.
 */
export const FAVORITES_DEFAULT_CODES: string[] = ["AltLeft"];

/** The sided modifier codes the backend accepts. */
export function isModifierCode(code: string): boolean {
  return (
    code === "ControlLeft" ||
    code === "ControlRight" ||
    code === "AltLeft" ||
    code === "AltRight" ||
    code === "ShiftLeft" ||
    code === "ShiftRight" ||
    code === "MetaLeft" ||
    code === "MetaRight"
  );
}

/** F1..F12. */
export function isFunctionCode(code: string): boolean {
  return /^F([1-9]|1[0-2])$/.test(code);
}

/** KeyA..KeyZ and Digit0..Digit9. */
export function isPrintableCode(code: string): boolean {
  return /^Key[A-Z]$/.test(code) || /^Digit[0-9]$/.test(code);
}

/** A stable human-readable label for a single physical code (English, side-aware). */
export function codeLabel(code: string): string {
  switch (code) {
    case "ControlLeft":
      return "Ctrl Left";
    case "ControlRight":
      return "Ctrl Right";
    case "AltLeft":
      return "Alt Left";
    case "AltRight":
      return "Alt Right";
    case "ShiftLeft":
      return "Shift Left";
    case "ShiftRight":
      return "Shift Right";
    case "MetaLeft":
      return "Meta Left";
    case "MetaRight":
      return "Meta Right";
    default:
      if (isFunctionCode(code)) return code; // "F1"
      if (code.startsWith("Key")) return code.slice(3); // "A"
      if (code.startsWith("Digit")) return code.slice(5); // "0"
      return code;
  }
}

/** Join a chord's codes into a display string (e.g. "Ctrl Left + A"). */
export function shortcutLabel(codes: string[]): string {
  return codes.map(codeLabel).join(" + ");
}

/**
 * Matches a chord against live key events. The toggle fires exactly once when
 * the set of held target codes first becomes complete, ignores auto-repeat, and
 * re-arms only after every target key is released — so the next press toggles
 * again.
 */
export class ShortcutMatcher {
  private readonly target: Set<string>;
  private readonly held = new Set<string>();
  private triggered = false;

  constructor(codes: string[]) {
    this.target = new Set(codes);
  }

  get codes(): string[] {
    return [...this.target];
  }

  /** A keydown. Returns true exactly when this press completes the chord. */
  keydown(code: string, repeat: boolean): boolean {
    if (repeat) return false; // ignore auto-repeat
    if (this.triggered) return false; // one toggle per press cycle
    if (!this.target.has(code)) return false; // unrelated key
    this.held.add(code);
    if (this.held.size === this.target.size && [...this.target].every((c) => this.held.has(c))) {
      this.triggered = true;
      return true;
    }
    return false;
  }

  /** A keyup. Re-arms the toggle once every target key is released. */
  keyup(code: string): void {
    if (!this.target.has(code)) return;
    this.held.delete(code);
    if (this.held.size === 0) this.triggered = false;
  }

  /** Whether a given physical code is part of this chord (for event filtering). */
  involves(code: string): boolean {
    return this.target.has(code);
  }
}
