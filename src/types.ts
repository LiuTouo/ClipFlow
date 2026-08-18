// Shared IPC types mirrored from the Rust backend (src-tauri/src/models.rs).
// Serialized field names are snake_case to match serde's default field naming.

export type ClipKind = "Text" | "Image" | "FilePaths";

/** A history clipboard entry, as received from `get_clips` / `clipboard-update`. */
export interface Clip {
  id: string;
  kind: ClipKind;
  text_content: string | null;
  thumbnail_base64: string | null;
  content_hash: string;
  preview: string;
  truncated: boolean;
  source_exe: string;
  source_title: string;
  source_icon: string | null;
  captured_at: number;
  pinned: boolean;
  byte_size: number;
}

/** A durable favorite snapshot. Mirrors `Clip` minus `pinned`, plus membership time. */
export interface FavoriteItem {
  id: string;
  kind: ClipKind;
  text_content: string | null;
  thumbnail_base64: string | null;
  content_hash: string;
  preview: string;
  truncated: boolean;
  source_exe: string;
  source_title: string;
  source_icon: string | null;
  captured_at: number;
  byte_size: number;
  added_at: number | null;
}

/** Payload of the `clipboard-update` event. */
export interface ClipboardUpdate {
  clip: Clip;
  evicted: string[];
}

/** A favorites collection, as returned by `list_collections`. */
export interface CollectionSummary {
  id: string;
  name: string;
  sort_order: number;
  created_at: number;
  item_count: number;
}

export type ClipScope = "history" | "favorite";

/** Identifies a clip by scope (see the backend `ClipLocator`). */
export interface ClipLocator {
  scope: ClipScope;
  id: string;
}

/** Session-only favorites sidebar state (see backend `FavoritesUiState`). */
export interface FavoritesUiState {
  open: boolean;
  selected_collection: string | null;
}

/** The favorites toggle chord, stored in AppConfig. */
export interface PanelShortcut {
  codes: string[];
}
