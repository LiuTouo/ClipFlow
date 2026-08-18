// Pure collection-order math for the sidebar reorder + Move Up/Down menu.
// No DOM, no Tauri.

/** New array with the item at `fromIndex` moved to `toIndex` (indices in the
 * source array, before removal). Out-of-range indices return a clone unchanged. */
export function moveItem<T>(items: readonly T[], fromIndex: number, toIndex: number): T[] {
  if (fromIndex === toIndex) return [...items];
  if (fromIndex < 0 || fromIndex >= items.length) return [...items];
  const out = [...items];
  const [moved] = out.splice(fromIndex, 1);
  out.splice(toIndex, 0, moved);
  return out;
}

/**
 * Reorder `ids` so `movedId` lands before `beforeId` (or at the end when
 * `beforeId` is null). Used by both the drag insertion indicator and the
 * Move Up/Down menu, so both produce the same canonical order.
 */
export function insertBefore(ids: readonly string[], movedId: string, beforeId: string | null): string[] {
  const rest = ids.filter((id) => id !== movedId);
  if (beforeId === null) {
    rest.push(movedId);
    return rest;
  }
  const idx = rest.indexOf(beforeId);
  if (idx === -1) {
    rest.push(movedId);
    return rest;
  }
  rest.splice(idx, 0, movedId);
  return rest;
}

/** Index `movedId` should occupy to sit before `beforeId` in the filtered list. */
export function insertionIndex(ids: readonly string[], movedId: string, beforeId: string | null): number {
  const rest = ids.filter((id) => id !== movedId);
  if (beforeId === null) return rest.length;
  const idx = rest.indexOf(beforeId);
  return idx === -1 ? rest.length : idx;
}

/**
 * Move the item at `index` by `delta` slots (delta = -1 up, +1 down). Returns a
 * clone unchanged when the move would cross a boundary, so the Move Up/Down menu
 * can hand the boundary cases to this function instead of re-deriving them.
 */
export function moveOne(ids: readonly string[], index: number, delta: number): string[] {
  if (index < 0 || index >= ids.length) return [...ids];
  const to = index + delta;
  if (to < 0 || to >= ids.length) return [...ids];
  return moveItem(ids, index, to);
}
