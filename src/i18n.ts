import { invoke } from "@tauri-apps/api/core";

/**
 * Shared UI strings for all ClipFlow pages (panel, settings, about).
 * Language comes from AppConfig.language: "zh-TW" (default) or "en".
 */
const I18N: Record<string, Record<string, string>> = {
  "zh-TW": {
    // Settings page
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
    // Panel
    searchPlaceholder: "搜尋剪貼簿歷史…",
    emptyTitle: "尚無剪貼簿歷史",
    emptyHint: "複製一些內容就會出現在這裡",
    pinnedDivider: "釘選",
    copied: "已複製到剪貼簿",
    deleted: "已刪除",
    undo: "復原",
    imageClip: "圖片",
    unknownSource: "未知",
    justNow: "剛剛",
    minutesAgo: "{n} 分鐘前",
    hoursAgo: "{n} 小時前",
    daysAgo: "{n} 天前",
    pinTitle: "釘選",
    unpinTitle: "取消釘選",
    copyOnlyTitle: "純複製",
    deleteTitle: "刪除",
    // About
    aboutTitle: "關於 ClipFlow",
    tagline: "現代、輕量的 Windows 剪貼簿歷史工具。",
    changelog: "更新日誌",
  },
  en: {
    // Settings page
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
    // Panel
    searchPlaceholder: "Search clipboard history...",
    emptyTitle: "No clipboard history yet",
    emptyHint: "Copy something to get started",
    pinnedDivider: "Pinned",
    copied: "Copied to clipboard",
    deleted: "Deleted",
    undo: "Undo",
    imageClip: "Image",
    unknownSource: "Unknown",
    justNow: "just now",
    minutesAgo: "{n}m ago",
    hoursAgo: "{n}h ago",
    daysAgo: "{n}d ago",
    pinTitle: "Pin",
    unpinTitle: "Unpin",
    copyOnlyTitle: "Copy only",
    deleteTitle: "Delete",
    // About
    aboutTitle: "About ClipFlow",
    tagline: "A modern, lightweight clipboard history tool for Windows.",
    changelog: "Changelog",
  },
};

let lang = "zh-TW";

export function currentLang(): string {
  return lang;
}

export function setLanguage(l: string) {
  lang = I18N[l] ? l : "zh-TW";
}

/** Look up a string; `{name}` placeholders are filled from vars. */
export function t(key: string, vars?: Record<string, string | number>): string {
  const dict = I18N[lang] || I18N["zh-TW"];
  let s = dict[key] ?? I18N["en"][key] ?? key;
  if (vars) {
    for (const [k, v] of Object.entries(vars)) {
      s = s.replace(`{${k}}`, String(v));
    }
  }
  return s;
}

/** Apply the current language to all [data-i18n] / [data-i18n-placeholder] elements. */
export function applyI18n(root: ParentNode = document) {
  const dict = I18N[lang] || I18N["zh-TW"];
  root.querySelectorAll<HTMLElement>("[data-i18n]").forEach((el) => {
    const key = el.dataset.i18n!;
    if (dict[key]) el.textContent = dict[key];
  });
  root.querySelectorAll<HTMLInputElement>("[data-i18n-placeholder]").forEach((el) => {
    const key = el.dataset.i18nPlaceholder!;
    if (dict[key]) el.placeholder = dict[key];
  });
}

/** Load the configured language from the backend into this module. */
export async function initLanguage(): Promise<string> {
  try {
    const config = await invoke<{ language?: string }>("get_config");
    setLanguage(config.language || "zh-TW");
  } catch (_) {
    setLanguage("zh-TW");
  }
  return lang;
}
