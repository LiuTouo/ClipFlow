// Pure state machine for the press-Space preview toggle. No DOM, no Tauri:
// main.ts feeds it key decisions and backend results; tests drive it directly.

/** Outcome of a Space keydown, decided purely from toggle state + hover. */
export type SpaceAction =
  | { type: "open"; id: string }
  | { type: "close" }
  | { type: "swallow" }
  | { type: "ignore" };

/**
 * Owns three things:
 *  1. The visibility flag (`previewId`): which clip the UI believes is shown,
 *     or null when closed. Backend `get_active_clip_preview` is the authority;
 *     this flag is only ever a mirror of it (optimistic on show, confirmed on
 *     hide/resync/show-commit).
 *  2. A monotonic mutation token, bumped only by show/hide intents (real
 *     changes the frontend requested). A resync is a read, not a mutation: it
 *     captures the token and applies its result only if no newer mutation
 *     superseded the read. This is what lets a backend show commit that lands
 *     after an earlier resync-null still re-open the preview.
 *  3. The physical-Space consumption flag: a consumed press swallows its own
 *     auto-repeat keydowns until keyup.
 */
export class PreviewController {
  private previewId: string | null = null;
  private mutation = 0;
  private spaceHeld = false;

  get currentId(): string | null {
    return this.previewId;
  }

  get isOpen(): boolean {
    return this.previewId !== null;
  }

  get spaceConsumed(): boolean {
    return this.spaceHeld;
  }

  /** Decide what a Space keydown should do. `editableActive` is whether an
   * INPUT/TEXTAREA/SELECT/contenteditable holds focus — an editable always
   * wins, open preview included: the key must reach it untouched. `hoveredId`
   * is the clip id of the live row under the pointer (or null); `selectedId`
   * is the keyboard-selected row's id (or null), fallback when nothing is
   * hovered. */
  decideSpaceKeydown(editableActive: boolean, repeat: boolean, hoveredId: string | null, selectedId: string | null): SpaceAction {
    if (editableActive) return { type: "ignore" };
    if (repeat) return this.spaceHeld ? { type: "swallow" } : { type: "ignore" };
    if (this.isOpen) return { type: "close" };
    const target = hoveredId ?? selectedId;
    if (target === null) return { type: "ignore" };
    return { type: "open", id: target };
  }

  /** A non-repeat Space press (open or close) is consumed: suppress its
   * auto-repeat until the matching keyup. */
  consumeSpace(): void {
    this.spaceHeld = true;
  }

  /** Keyup re-arms ordinary Space input. Releasing must not close the preview. */
  releaseSpace(): void {
    this.spaceHeld = false;
  }

  /** Begin a show intent. Optimistically marks the preview open — the toggle
   * must reflect the press immediately so key-repeat suppression and
   * hover-to-update behave. Returns the mutation token guarding this intent. */
  beginShow(id: string): number {
    this.mutation += 1;
    this.previewId = id;
    return this.mutation;
  }

  /** Begin a hide intent. Does NOT clear the flag: the preview stays "open"
   * until the backend confirms, so a concurrent newer show/hide wins by token.
   * Returns the mutation token guarding this intent. */
  beginHide(): number {
    this.mutation += 1;
    return this.mutation;
  }

  /** Capture the current mutation token for a backend-authoritative read. The
   * read does not bump it: a show whose commit lands after this read must
   * still be able to re-open, and only a newer show/hide makes the read stale. */
  beginResync(): number {
    return this.mutation;
  }

  /** A show committed on the backend. Re-opens `id` unless a newer show/hide
   * superseded this intent. This reconciles a resync read that ran before the
   * backend commit: the commit is newer truth, so it wins over that read. */
  resolveShow(token: number, id: string): void {
    if (token !== this.mutation) return;
    this.previewId = id;
  }

  /** A hide resolved. No-op when a newer mutation superseded this token. */
  resolveHide(token: number): void {
    if (token !== this.mutation) return;
    this.previewId = null;
  }

  /** A resync read the backend: adopt its truth. No-op when a newer show/hide
   * superseded the read. */
  resolveResync(token: number, activeId: string | null): void {
    if (token !== this.mutation) return;
    this.previewId = activeId;
  }

  /** True when `token` is still the newest mutation. A show/hide whose token is
   * still current failed while it owned the state — the caller must resync. */
  isCurrent(token: number): boolean {
    return token === this.mutation;
  }
}
