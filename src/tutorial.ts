// Tutorial wizard (tutorial.html). Reads the current config so shortcut labels
// are live; Back/Next/Skip/Start map to the backend complete_tutorial contract.

import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { applyI18n, setLanguage, t } from "./i18n";
import { applyTheme } from "./theme";
import { TUTORIAL_PAGES, TutorialNav, TutorialSession } from "./tutorial-state";
import { shortcutLabel, FAVORITES_DEFAULT_CODES } from "./shortcut";

interface AppConfig {
  language?: string;
  theme?: string;
  ui_opacity_percent?: number;
  hotkey?: string;
  favorites_toggle_shortcut?: { codes: string[] };
}

const nav = new TutorialNav();
const session = new TutorialSession();
let config: AppConfig = {};

const titleEl = document.getElementById("tutorial-page-title")!;
const bodyEl = document.getElementById("tutorial-page-body")!;
const progressEl = document.getElementById("tutorial-progress")!;
const dotsEl = document.getElementById("tutorial-dots")!;
const backBtn = document.getElementById("tutorial-back") as HTMLButtonElement;
const nextBtn = document.getElementById("tutorial-next") as HTMLButtonElement;
const startBtn = document.getElementById("tutorial-start") as HTMLButtonElement;
const skipBtn = document.getElementById("tutorial-skip") as HTMLButtonElement;

async function refreshConfig(): Promise<void> {
  try {
    config = await invoke<AppConfig>("get_config");
  } catch {
    config = {};
  }
  setLanguage(config.language || "zh-TW");
  applyTheme(config.theme || "system");
  const opacity = Math.min(100, Math.max(50, config.ui_opacity_percent ?? 96));
  document.documentElement.style.setProperty("--panel-opacity", String(opacity / 100));
}

/** Render the current page, injecting the live hotkey into shortcut copy. */
function render(): void {
  const page = nav.page;
  titleEl.textContent = t(page.titleKey);

  bodyEl.replaceChildren();
  for (const key of page.bodyKeys) {
    const p = document.createElement("p");
    // {hotkey} is the global panel hotkey; {shortcut} is the favorites chord.
    p.textContent = t(key, {
      hotkey: config.hotkey || "Ctrl+Shift+V",
      shortcut: shortcutLabel(config.favorites_toggle_shortcut?.codes ?? FAVORITES_DEFAULT_CODES),
    });
    bodyEl.append(p);
  }

  progressEl.textContent = t("tutorialProgress", {
    current: String(nav.current + 1),
    total: String(TUTORIAL_PAGES.length),
  });

  dotsEl.replaceChildren();
  TUTORIAL_PAGES.forEach((pg, i) => {
    const dot = document.createElement("button");
    dot.className = `tutorial-dot${i === nav.current ? " active" : ""}`;
    dot.setAttribute("role", "tab");
    dot.setAttribute("aria-selected", String(i === nav.current));
    dot.setAttribute("aria-label", t(pg.titleKey));
    dot.addEventListener("click", () => { nav.goto(i); render(); });
    dotsEl.append(dot);
  });

  backBtn.disabled = nav.isFirst;
  const last = nav.isLast;
  nextBtn.classList.toggle("hidden", last);
  startBtn.classList.toggle("hidden", !last);
  // Skip is always available; on the last page "Next" hides and "Start" shows.
  skipBtn.classList.toggle("hidden", false);
}

async function finish(openHistory: boolean): Promise<void> {
  if (!session.beginCompletion()) return;
  try {
    await invoke("complete_tutorial", { openHistory });
    session.settle(true);
  } catch (err) {
    session.settle(false);
    console.error("Failed to complete tutorial:", err);
  }
}

function keydown(e: KeyboardEvent): void {
  // Escape equals Skip: mark seen (no history). The session guard prevents
  // duplicate submits and re-arms on reopen or failure.
  if (e.key === "Escape") {
    e.preventDefault();
    finish(false);
    return;
  }
  // Left/Right navigate only when focus is not in an editable control.
  const el = document.activeElement as HTMLElement | null;
  const editable = el && (el.tagName === "INPUT" || el.tagName === "TEXTAREA" || el.isContentEditable);
  if (editable) return;
  if (e.key === "ArrowRight") { e.preventDefault(); if (nav.next()) render(); }
  else if (e.key === "ArrowLeft") { e.preventDefault(); if (nav.back()) render(); }
}

async function init(): Promise<void> {
  await refreshConfig();
  applyI18n();
  document.title = `Mnemark ${t("tutorial")}`;

  backBtn.addEventListener("click", () => { nav.back(); render(); });
  nextBtn.addEventListener("click", () => { nav.next(); render(); });
  startBtn.addEventListener("click", () => finish(true));
  skipBtn.addEventListener("click", () => finish(false));
  document.addEventListener("keydown", keydown);

  // The Rust backend hides (never destroys) this window; on each reopen it
  // emits "tutorial-reopened" so a reused session re-arms Skip/Start and
  // restarts from the first page.
  await listen("tutorial-reopened", () => {
    session.reopen();
    nav.goto(0);
    render();
  });

  render();
  nextBtn.focus();
}

window.addEventListener("DOMContentLoaded", () => { void init(); });
