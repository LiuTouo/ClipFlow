// Pure filter/search decisions for the clip list. Works identically over the
// history dataset and a collection's favorite dataset, since both carry the
// same surface fields. No DOM, no Tauri.

import type { ClipKind } from "./types";

export type FilterKind = "all" | "text" | "image" | "files" | "links";

/** The minimal shape both `Clip` and `FavoriteItem` satisfy for filter/search. */
export interface SearchableClip {
  kind: ClipKind;
  text_content: string | null;
  preview: string;
  source_exe: string;
  source_title: string;
}

/** True when text_content trims to a single valid http/https URL. */
export function isLink(text: string | null): boolean {
  if (!text) return false;
  const trimmed = text.trim();
  try {
    const url = new URL(trimmed);
    return url.protocol === "http:" || url.protocol === "https:";
  } catch {
    return false;
  }
}

/** Classify a clip for filter matching. */
export function classifyClip(clip: SearchableClip): FilterKind {
  if (clip.kind === "Image") return "image";
  if (clip.kind === "FilePaths") return "files";
  if (isLink(clip.text_content)) return "links";
  return "text";
}

/** Does this clip pass the active category filter? */
export function matchesFilter(clip: SearchableClip, filter: FilterKind): boolean {
  if (filter === "all") return true;
  return classifyClip(clip) === filter;
}

/** Apply the category filter and the search query to a dataset. `query` matches
 * preview / source exe / source title, case-insensitively. */
export function filterItems<T extends SearchableClip>(items: readonly T[], query: string, filter: FilterKind): T[] {
  const q = query.toLowerCase();
  return items.filter((clip) => {
    if (!matchesFilter(clip, filter)) return false;
    if (!q) return true;
    return (
      clip.preview.toLowerCase().includes(q) ||
      clip.source_exe.toLowerCase().includes(q) ||
      clip.source_title.toLowerCase().includes(q)
    );
  });
}
