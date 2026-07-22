/**
 * Apply the theme setting to the document. "system" removes the override
 * and lets the prefers-color-scheme media query decide.
 */
export function applyTheme(theme: string) {
  if (theme === "dark" || theme === "light") {
    document.documentElement.dataset.theme = theme;
  } else {
    delete document.documentElement.dataset.theme;
  }
}
