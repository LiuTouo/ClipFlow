// Pure menu placement + chooser state guards. No DOM, no Tauri. main.ts and
// favorites.ts share the placement math so both the history More menu and the
// favorites collection menu stay inside their panel; tests drive the math and
// the race guard directly.

/** Anchor rectangle in panel-local coordinates (px from the panel's top edge). */
export interface MenuAnchor {
  top: number;
  bottom: number;
}

export interface MenuPanel {
  width: number;
  height: number;
}

export interface MenuPlacement {
  /** px from the panel top. */
  top: number;
  /** px from the panel right edge (the menu is right-anchored). */
  right: number;
  /** Constrain the menu's height so it scrolls instead of leaving the panel. */
  maxHeight: number | null;
  /** Constrain width when the menu is wider than the panel. */
  maxWidth: number | null;
  /** True when the menu opened above the anchor. */
  flip: boolean;
}

/** Spacing between the anchor and the menu (px). */
const GAP = 4;

/**
 * Compute a boundary-safe position for a right-anchored menu inside a panel.
 * Prefers to open below the anchor, flips above when that side has more room,
 * and constrains the menu's height (vertical scroll) when it is taller than
 * either side. `right` is the anchor-right distance from the panel's right
 * edge (panel.width - anchor.right).
 */
export function computeMenuPlacement(
  anchor: MenuAnchor,
  panel: MenuPanel,
  menuWidth: number,
  menuHeight: number,
  right: number,
  gap = GAP,
): MenuPlacement {
  const spaceBelow = panel.height - anchor.bottom - gap;
  const spaceAbove = anchor.top - gap;
  const fitsBelow = menuHeight <= spaceBelow;
  const flip = !fitsBelow && spaceAbove > spaceBelow;

  let top: number;
  let maxHeight: number | null = null;

  if (!flip) {
    top = anchor.bottom + gap;
    if (!fitsBelow) maxHeight = Math.max(0, spaceBelow);
  } else if (menuHeight <= spaceAbove) {
    top = anchor.top - gap - menuHeight;
  } else {
    top = 0;
    maxHeight = Math.max(0, spaceAbove);
  }

  top = Math.max(0, top);
  // Clamp right into [0, max(0, panel.width - menuWidth)] so a negative offset
  // (or a menu wider than the panel) never pushes it past an edge.
  const maxRight = Math.max(0, panel.width - menuWidth);
  const clampedRight = Math.max(0, Math.min(right, maxRight));
  const maxWidth = menuWidth > panel.width ? panel.width : null;

  return { top, right: clampedRight, maxHeight, maxWidth, flip };
}

/**
 * Monotonic token guard for the async add-to-collection chooser. `open` mints
 * a fresh generation token each call, so a late `invoke` response is accepted
 * only when both the id and the token are still current — closing and then
 * reopening the same id still rejects the first (stale) response.
 */
export class ChooserGate {
  private generation = 0;
  private current: string | null = null;

  /** Begin a request for `id`; returns a token that any later open/close
   * makes stale. */
  open(id: string): number {
    this.generation += 1;
    this.current = id;
    return this.generation;
  }

  /** Dismiss the chooser; any in-flight request becomes stale. */
  close(): void {
    this.current = null;
  }

  /** True when `id` is still the active request and `token` is the latest
   * generation. */
  isCurrent(id: string, token: number): boolean {
    return this.current === id && this.generation === token;
  }

  /** True when a chooser is open or pending. */
  get isOpen(): boolean {
    return this.current !== null;
  }
}
