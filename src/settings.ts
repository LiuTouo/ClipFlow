import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { applyI18n, setLanguage, t } from "./i18n";

interface AppConfig {
  text_size_limit_kb: number;
  text_count_limit: number;
  image_count_limit: number;
  image_memory_budget_mb: number;
  image_size_limit_mb: number;
  hotkey: string;
  startup: boolean;
  persist: boolean;
  exclusion_list: string[];
  vim_mode: boolean;
  debounce_ms: number;
  theme: string;
  language: string;
}

let config: AppConfig;

async function init() {
  config = await invoke("get_config");
  setLanguage(config.language || "zh-TW");
  populateForm();
  applyI18n();
  document.title = `ClipFlow ${t("settings")}`;
  bindEvents();
}

function populateForm() {
  (document.getElementById("setting-text-size-limit") as HTMLInputElement).value = String(config.text_size_limit_kb);
  (document.getElementById("setting-text-count-limit") as HTMLInputElement).value = String(config.text_count_limit);
  (document.getElementById("setting-image-count-limit") as HTMLInputElement).value = String(config.image_count_limit);
  (document.getElementById("setting-image-memory-budget") as HTMLInputElement).value = String(config.image_memory_budget_mb);
  (document.getElementById("setting-image-size-limit") as HTMLInputElement).value = String(config.image_size_limit_mb);
  (document.getElementById("setting-hotkey") as HTMLInputElement).value = config.hotkey;
  (document.getElementById("setting-startup") as HTMLInputElement).checked = config.startup;
  (document.getElementById("setting-persist") as HTMLInputElement).checked = config.persist;
  (document.getElementById("setting-vim-mode") as HTMLInputElement).checked = config.vim_mode;
  (document.getElementById("setting-debounce") as HTMLInputElement).value = String(config.debounce_ms);
  (document.getElementById("setting-theme") as HTMLSelectElement).value = config.theme;
  (document.getElementById("setting-language") as HTMLSelectElement).value = config.language || "zh-TW";
  (document.getElementById("setting-exclusions") as HTMLTextAreaElement).value = config.exclusion_list.join("\n");
}

function showError(message: string) {
  const el = document.getElementById("hotkey-error")!;
  el.textContent = message;
  el.classList.add("visible");
}

function clearError() {
  const el = document.getElementById("hotkey-error")!;
  el.textContent = "";
  el.classList.remove("visible");
}

function bindEvents() {
  // Live language preview
  document.getElementById("setting-language")!.addEventListener("change", (e) => {
    setLanguage((e.target as HTMLSelectElement).value);
    applyI18n();
    document.title = `ClipFlow ${t("settings")}`;
  });

  document.getElementById("settings-form")!.addEventListener("submit", async (e) => {
    e.preventDefault();
    clearError();

    config.text_size_limit_kb = Number((document.getElementById("setting-text-size-limit") as HTMLInputElement).value);
    config.text_count_limit = Number((document.getElementById("setting-text-count-limit") as HTMLInputElement).value);
    config.image_count_limit = Number((document.getElementById("setting-image-count-limit") as HTMLInputElement).value);
    config.image_memory_budget_mb = Number((document.getElementById("setting-image-memory-budget") as HTMLInputElement).value);
    config.image_size_limit_mb = Number((document.getElementById("setting-image-size-limit") as HTMLInputElement).value);
    config.hotkey = (document.getElementById("setting-hotkey") as HTMLInputElement).value;
    config.startup = (document.getElementById("setting-startup") as HTMLInputElement).checked;
    config.persist = (document.getElementById("setting-persist") as HTMLInputElement).checked;
    config.vim_mode = (document.getElementById("setting-vim-mode") as HTMLInputElement).checked;
    config.debounce_ms = Number((document.getElementById("setting-debounce") as HTMLInputElement).value);
    config.theme = (document.getElementById("setting-theme") as HTMLSelectElement).value;
    config.language = (document.getElementById("setting-language") as HTMLSelectElement).value;
    config.exclusion_list = (document.getElementById("setting-exclusions") as HTMLTextAreaElement).value
      .split("\n")
      .map(s => s.trim())
      .filter(s => s.length > 0);

    try {
      await invoke("update_config", { newConfig: config });
      // Close settings window
      await getCurrentWindow().close();
    } catch (err) {
      console.error("Save failed:", err);
      const msg = String(err);
      showError(msg.includes("already in use") ? t("hotkeyInUse") : msg);
    }
  });

  document.getElementById("btn-cancel")!.addEventListener("click", () => {
    getCurrentWindow().close();
  });

  // Hotkey recording
  const hotkeyInput = document.getElementById("setting-hotkey") as HTMLInputElement;
  hotkeyInput.addEventListener("click", () => {
    clearError();
    hotkeyInput.classList.add("recording");
    hotkeyInput.value = t("pressKeys");
    hotkeyInput.readOnly = true;
  });

  hotkeyInput.addEventListener("keydown", (e) => {
    if (!hotkeyInput.classList.contains("recording")) return;
    e.preventDefault();

    if (e.key === "Escape") {
      hotkeyInput.value = config.hotkey;
      hotkeyInput.classList.remove("recording");
      hotkeyInput.readOnly = false;
      return;
    }

    const parts: string[] = [];
    if (e.ctrlKey) parts.push("Ctrl");
    if (e.shiftKey) parts.push("Shift");
    if (e.altKey) parts.push("Alt");

    const key = e.key;
    if (key !== "Control" && key !== "Shift" && key !== "Alt") {
      parts.push(key.length === 1 ? key.toUpperCase() : key);
    }

    if (parts.length > 0) {
      hotkeyInput.value = parts.join("+");
      hotkeyInput.classList.remove("recording");
      hotkeyInput.readOnly = false;
    }
  });
}

window.addEventListener("DOMContentLoaded", init);
