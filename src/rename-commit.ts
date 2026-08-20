// Rename-editing state for the favorites collection list, extracted from
// favorites.ts so the commit/rejection flow is testable. The backend stays
// the authority: after a commit (success OR rejection) the UI always exits
// editing and re-renders from authoritative state, never the typed value.

export interface RenameDeps {
  rename(id: string, name: string): Promise<void>;
  reload(): Promise<void>;
  render(): void;
  showError(message: string): void;
}

export interface RenameController {
  readonly editingId: string | null;
  begin(id: string): void;
  cancel(): void;
  commit(id: string, value: string): void;
}

export function createRenameController(deps: RenameDeps): RenameController {
  let editing: string | null = null;

  return {
    get editingId() {
      return editing;
    },
    begin(id: string): void {
      editing = id;
      deps.render();
    },
    cancel(): void {
      editing = null;
      deps.render();
    },
    commit(id: string, value: string): void {
      if (editing !== id) return;
      editing = null;
      const name = value.trim();
      if (!name) {
        deps.render();
        return;
      }
      deps
        .rename(id, name)
        .then(() => deps.reload())
        .catch((err: unknown) => {
          // Regression: the rejection used to leave the row stuck in its
          // editing DOM. Exit editing and re-render synchronously right here,
          // independent of the background refresh below.
          deps.showError(String(err));
          deps.render();
          // Backend-authoritative refresh; its own failure must not become
          // an unhandled rejection on top of the already-shown error.
          deps.reload().catch(() => {});
        });
    },
  };
}
