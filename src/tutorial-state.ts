// Pure navigation state for the tutorial wizard. No DOM, no Tauri.

export interface TutorialPage {
  id: string;
  /** i18n key for the page title. */
  titleKey: string;
  /** i18n keys for the body paragraphs (rendered in order). */
  bodyKeys: string[];
}

export const TUTORIAL_PAGES: TutorialPage[] = [
  {
    id: "background",
    titleKey: "tutorialTitleBackground",
    bodyKeys: ["tutorialBodyBackground"],
  },
  {
    id: "shortcut",
    titleKey: "tutorialTitleShortcut",
    bodyKeys: ["tutorialBodyShortcut"],
  },
  {
    id: "search",
    titleKey: "tutorialTitleSearch",
    bodyKeys: ["tutorialBodySearch"],
  },
  {
    id: "actions",
    titleKey: "tutorialTitleActions",
    bodyKeys: ["tutorialBodyActions"],
  },
  {
    id: "preview",
    titleKey: "tutorialTitlePreview",
    bodyKeys: ["tutorialBodyPreview"],
  },
  {
    id: "favorites",
    titleKey: "tutorialTitleFavorites",
    bodyKeys: [
      "tutorialBodyFavoritesOpen",
      "tutorialBodyFavoritesAdd",
      "tutorialBodyFavoritesBrowse",
    ],
  },
  {
    id: "settings",
    titleKey: "tutorialTitleSettings",
    bodyKeys: ["tutorialBodySettings"],
  },
];

/** Owns the current page index and enforces the first/last boundaries. */
export class TutorialNav {
  private index = 0;

  constructor(private readonly length = TUTORIAL_PAGES.length) {
    this.index = 0;
  }

  get current(): number {
    return this.index;
  }

  get page(): TutorialPage {
    return TUTORIAL_PAGES[this.index];
  }

  get isFirst(): boolean {
    return this.index === 0;
  }

  get isLast(): boolean {
    return this.index === this.length - 1;
  }

  /** Advance one page; no-op (returns false) on the last page. */
  next(): boolean {
    if (this.isLast) return false;
    this.index += 1;
    return true;
  }

  /** Go back one page; no-op (returns false) on the first page. */
  back(): boolean {
    if (this.isFirst) return false;
    this.index -= 1;
    return true;
  }

  /** Jump to a specific page (clamped). */
  goto(index: number): void {
    this.index = Math.max(0, Math.min(this.length - 1, index));
  }
}

/**
 * Pure one-shot completion state for the tutorial wizard. Owns the two guards
 * behind Skip/Start:
 *  - `finished`: set when a completion is dispatched; a reopened window clears
 *    it so the next session is actionable again.
 *  - `submitting`: true only while the `complete_tutorial` invoke is in flight,
 *    so a double-click submits once. A rejected invoke clears `finished` too,
 *    so the user can retry instead of being left with a dead UI.
 */
export class TutorialSession {
  private finished = false;
  private submitting = false;

  get isFinished(): boolean {
    return this.finished;
  }

  get isSubmitting(): boolean {
    return this.submitting;
  }

  /** Begin a completion. Returns false when already finished for this session
   * or an invoke is still in flight (double-click). */
  beginCompletion(): boolean {
    if (this.finished || this.submitting) return false;
    this.finished = true;
    this.submitting = true;
    return true;
  }

  /** The invoke settled. Success keeps `finished` (the window has hidden);
   * failure clears it so Skip/Start can retry. Always ends the in-flight window. */
  settle(succeeded: boolean): void {
    this.submitting = false;
    if (!succeeded) this.finished = false;
  }

  /** A fresh session: the reused window was shown again. Clears `finished`
   * unless an invoke is still in flight (never reset mid-submission). */
  reopen(): void {
    if (this.submitting) return;
    this.finished = false;
  }
}
