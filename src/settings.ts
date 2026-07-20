import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";

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

const I18N: Record<string, Record<string, string>> = {
  "zh-TW": {
    settings: "設定",
    hotkey: "快捷鍵",
    hotkeyHint: "點擊以變更，按 Esc 取消",
    textHistory: "文字歷史",
    textSizeLimit: "單則文字大小上限 (KB)",
    textCountLimit: "文字歷史筆數上限",
    imageHistory: "圖片歷史",
    imageCountLimit: "圖片歷史筆數上限",
    imageMemoryBudget: "圖片記憶體上限 (MB)",
    imageSizeLimit: "單張圖片大小上限 (MB)",
    behavior: "行為",
    startup: "登入時自動啟動（免安裝捷徑）",
    persist: "將歷史紀錄保存到磁碟（SQLite）",
    vimMode: "Vim 模式（以 j/k 瀏覽）",
    debounce: "防抖動 (ms)",
    appearance: "外觀",
    theme: "主題",
    themeSystem: "跟隨系統",
    themeDark: "深色",
    themeLight: "淺色",
    language: "語言",
    exclusionList: "排除清單",
    exclusionHint: "執行檔名稱（每行一個）。來自這些應用程式的剪貼簿內容不會被記錄。",
    save: "儲存",
    cancel: "取消",
    pressKeys: "請按下按鍵…",
    hotkeyInUse: "此按鍵組合已被其他應用程式使用",
  },
  en: {
    settings: "Settings",
    hotkey: "Hotkey",
    hotkeyHint: "Click to change, press Esc to cancel",
    textHistory: "Text History",
    textSizeLimit: "Text size limit (KB)",
    textCountLimit: "Max text entries",
    imageHistory: "Image History",
    imageCountLimit: "Max image entries",
    imageMemoryBudget: "Image memory budget (MB)",
    imageSizeLimit: "Single image size limit (MB)",
    behavior: "Behavior",
    startup: "Start at login (portable shortcut)",
    persist: "Persist history to disk (SQLite)",
    vimMode: "Vim mode (j/k to navigate)",
    debounce: "Debounce (ms)",
    appearance: "Appearance",
    theme: "Theme",
    themeSystem: "Follow system",
    themeDark: "Dark",
    themeLight: "Light",
    language: "Language",
    exclusionList: "Exclusion List",
    exclusionHint: "Executable names (one per line). Clipboard content from these apps will not be recorded.",
    save: "Save",
    cancel: "Cancel",
    pressKeys: "Press keys...",
    hotkeyInUse: "This combination is already in use",
  },
};

function currentLang(): string {
  return (document.getElementById("setting-language") as HTMLSelectElement).value || "zh-TW";
}

function t(key: string): string {
  const dict = I18N[currentLang()] || I18N["zh-TW"];
  return dict[key] ?? key;
}

function applyI18n(lang: string) {
  const dict = I18N[lang] || I18N["zh-TW"];
  document.querySelectorAll<HTMLElement>("[data-i18n]").forEach((el) => {
    const key = el.dataset.i18n!;
    if (dict[key]) el.textContent = dict[key];
  });
  document.title = `ClipFlow ${dict.settings}`;
}

let config: AppConfig;

async function init() {
  config = await invoke("get_config");
  populateForm();
  applyI18n(config.language || "zh-TW");
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
    applyI18n((e.target as HTMLSelectElement).value);
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
