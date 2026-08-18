import { invoke } from "@tauri-apps/api/core";

/**
 * Shared UI strings for all Mnemark pages (panel, settings, about).
 * Language comes from AppConfig.language: "zh-TW" (default) or "en".
 */
const I18N: Record<string, Record<string, string>> = {
  "zh-TW": {
    // Settings page
    settings: "設定",
    settingsSubtitle: "調整 Mnemark 的行為與外觀。",
    hotkey: "快捷鍵",
    hotkeyLabel: "快捷鍵組合",
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
    uiOpacity: "UI 不透明度",
    language: "語言",
    exclusionList: "排除清單",
    exclusionHint: "執行檔名稱（每行一個）。來自這些應用程式的剪貼簿內容不會被記錄。",
    save: "儲存",
    cancel: "取消",
    loadingSettings: "載入設定中…",
    saving: "儲存中…",
    unsavedChanges: "有未儲存的變更",
    settingsLoadFailed: "載入設定失敗，請重新開啟設定頁。",
    pressKeys: "請按下按鍵…",
    hotkeyInUse: "此按鍵組合已被其他應用程式使用",
    hotkeyNeedModifier: "快捷鍵需包含 Ctrl、Shift 或 Alt 至少一個",
    pasteFilesAsFiles: "貼上檔案歷史時複製實際檔案",
    pasteFilesAsFilesHint: "開啟時，貼上檔案歷史等同在檔案總管複製該檔案（貼上時來源檔案必須仍存在）；關閉時改貼上路徑文字。",
    autoUpdate: "自動檢查更新",
    autoUpdateHint: "安裝版會在背景自動下載並安裝更新；免安裝版請到「關於」頁手動檢查。",
    rememberHistoryFilter: "記住歷史記錄過濾器",
    rememberHistoryFilterHint: "關閉時每次開啟面板皆回到「全部」；開啟時在 Mnemark 結束前會記住上次選擇的過濾器。",
    // Panel
    searchPlaceholder: "搜尋剪貼簿歷史…",
    searchShortcutHint: "按 / 聚焦搜尋",
    emptyTitle: "尚無剪貼簿歷史",
    emptyHint: "複製一些內容就會出現在這裡",
    noResults: "無符合的項目",
    categoryEmpty: "此分類尚無項目",
    filterAll: "全部",
    filterText: "文字",
    filterImage: "圖片",
    filterFiles: "檔案",
    filterLinks: "連結",
    filterBarLabel: "依類型過濾",
    moreTitle: "更多",
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
    filesMissingFallback: "來源檔案已不存在，改複製路徑文字",
    pasteFailed: "貼上失敗，請重試（剪貼簿可能被其他程式佔用）",
    copyFailed: "複製失敗，請重試（剪貼簿可能被其他程式佔用）",
    pinLimitReached: "最多只能釘選 10 則",
    nothingToUndo: "沒有可復原的刪除",
    hotkeyInvalid: "無效的快捷鍵格式",
    // Preview
    previewTypeText: "文字",
    previewTypeImage: "圖片",
    previewTypeFiles: "檔案",
    previewSource: "來源",
    previewCaptured: "擷取時間",
    previewSize: "大小",
    previewEmpty: "（無內容）",
    previewTruncatedSizes: "內容已截斷：僅保存開頭部分（已保存 {saved}，原始 {original}），完整內容無法復原。",
    previewTruncatedNoSizes: "內容已截斷：僅保存開頭部分，完整內容無法復原。",
    previewHintBefore: "選取或將滑鼠移到項目後，按",
    previewHintAfter: "開啟預覽，再按一次關閉",
    pressToPreview: "按下預覽",
    // About
    aboutTitle: "關於 Mnemark",
    tagline: "Find anything you've copied.（找到任何你複製過的內容。）",
    nameMeaning: "mneme（記憶）+ mark（標記）",
    brandMeaning: "你複製的每一件事都會留下痕跡——Mnemark 讓這些痕跡可被搜尋，讓你能隨時找回曾複製過的內容。",
    changelog: "更新日誌",
    checkUpdate: "檢查更新",
    checkingUpdate: "正在檢查更新…",
    updateUpToDate: "已是最新版本",
    updateAvailable: "有新版本 v{v}",
    updateError: "檢查更新失敗，請稍後再試",
    noReleaseYet: "尚無正式發行版本",
    installNow: "立即更新",
    installing: "正在下載並安裝更新…",
    restartNow: "立即重新啟動",
    downloadingUpdate: "正在下載更新…",
    portableAssetMissing: "此版本未提供免安裝檔案",
    portableSigMissing: "此版本未提供簽章檔，無法驗證更新",
    portableUpdateReady: "已下載至 {path}。請結束 Mnemark，再用新檔案取代舊的執行檔。",
    openFolder: "開啟資料夾",
    // Drawer sidebar
    favorites: "抽屜",
    favoritesAdd: "新增抽屜",
    history: "歷史紀錄",
    returnToHistory: "返回歷史紀錄",
    currentlyViewing: "目前瀏覽：{name}",
    addToCollection: "加入抽屜",
    removeFromCollection: "從抽屜移除",
    addToOtherCollection: "加入其他抽屜",
    addedToFavorites: "已加入抽屜",
    removedFromFavorites: "已從抽屜移除",
    collectionAdded: "已建立抽屜",
    collectionRemoved: "已刪除抽屜",
    collectionRenamed: "已重新命名",
    createCollection: "建立抽屜",
    renameCollection: "重新命名",
    moveUp: "上移",
    moveDown: "下移",
    remove: "移除",
    confirm: "確認",
    collectionNamePlaceholder: "抽屜名稱",
    collectionNameInvalid: "抽屜名稱需為 1-64 個字元",
    noCollections: "尚無抽屜",
    noCollectionsHint: "點擊右上角「新增」建立第一個抽屜",
    favoritesEmptyTitle: "此抽屜尚無項目",
    favoritesEmptyHint: "將滑鼠移到歷史紀錄項目，從「更多」加入抽屜",
    removeCollectionTitle: "刪除抽屜",
    removeCollectionBody: "確定要刪除「{name}」？其中 {count} 個項目不會被刪除。",
    removeCollectionConfirm: "刪除",
    dragHandleLabel: "拖曳排序",
    dragToDrawer: "拖曳到抽屜",
    favoritesToggle: "抽屜快捷鍵",
    favoritesToggleHint: "按下的按鍵組合用於開啟/關閉抽屜",
    pressKeysFavorites: "請按下按鍵…",
    sidebarOpen: "開啟抽屜",
    sidebarClose: "關閉抽屜",
    dragToAdd: "拖曳至此加入抽屜",
    draggingItem: "正在拖曳",
    emptyPreview: "無預覽內容",
    dropHere: "放到此抽屜",
    alreadyInDrawer: "已在此抽屜",
    // Tutorial
    tutorial: "教學",
    tutorialBack: "上一步",
    tutorialNext: "下一步",
    tutorialSkip: "略過",
    tutorialStart: "開始使用",
    tutorialProgress: "第 {current} 頁，共 {total} 頁",
    tutorialTitleBackground: "常駐背景與系統匣",
    tutorialBodyBackground: "Mnemark 在背景安靜記錄你複製的內容，並常駐於系統匣。可從系統匣隨時暫停監聽、開啟設定、教學或結束程式。",
    tutorialTitleShortcut: "全域快捷鍵",
    tutorialBodyShortcut: "按 {hotkey} 隨時開啟或關閉歷史面板。此快捷鍵可在設定中變更。",
    tutorialTitleSearch: "搜尋與篩選",
    tutorialBodySearch: "在面板內按 / 聚焦搜尋，或使用分類按鈕篩選文字、圖片、檔案與連結。以方向鍵移動、Enter 貼上。",
    tutorialTitleActions: "貼上、複製、釘選、刪除",
    tutorialBodyActions: "點擊項目即可貼上；複製、釘選與刪除在每個項目的按鈕中。刪除後可復原。",
    tutorialTitlePreview: "Space 預覽",
    tutorialBodyPreview: "選取或將滑鼠移到項目上按 Space 開啟預覽，再按一次關閉。",
    tutorialTitleFavorites: "抽屜",
    tutorialBodyFavoritesOpen: "點擊歷史面板右上角的星號，或按 {shortcut}，即可開啟或關閉抽屜介面；快捷鍵可在設定中變更。",
    tutorialBodyFavoritesAdd: "先按「新增抽屜」建立抽屜，再從歷史紀錄項目的拖曳把手將項目拖到目標抽屜。也可以從項目的「更多」功能表選擇要加入的抽屜。拖曳抽屜本身的把手則可調整抽屜順序。",
    tutorialBodyFavoritesBrowse: "點擊抽屜會讓歷史面板顯示該抽屜的項目；要回到完整的剪貼簿紀錄，請點擊抽屜介面上方的「返回歷史紀錄」。",
    tutorialTitleSettings: "設定與隱私",
    tutorialBodySettings: "在設定中調整歷史上限、語言、主題、排除清單，以及是否將歷史保存到磁碟。",
  },
  en: {
    // Settings page
    settings: "Settings",
    settingsSubtitle: "Tune Mnemark's behavior and appearance.",
    hotkey: "Hotkey",
    hotkeyLabel: "Hotkey combination",
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
    uiOpacity: "UI opacity",
    language: "Language",
    exclusionList: "Exclusion List",
    exclusionHint: "Executable names (one per line). Clipboard content from these apps will not be recorded.",
    save: "Save",
    cancel: "Cancel",
    loadingSettings: "Loading settings…",
    saving: "Saving…",
    unsavedChanges: "Unsaved changes",
    settingsLoadFailed: "Failed to load settings — please reopen the settings page.",
    pressKeys: "Press keys...",
    hotkeyInUse: "This combination is already in use",
    hotkeyNeedModifier: "Hotkey must include at least one of Ctrl, Shift, or Alt",
    pasteFilesAsFiles: "Paste file entries as real files",
    pasteFilesAsFilesHint: "When on, pasting a file entry copies the actual file(s) like Explorer does — the source files must still exist at paste time. When off, the path is pasted as text.",
    autoUpdate: "Automatically check for updates",
    autoUpdateHint: "Installed builds download and install updates in the background. Portable builds: check manually from the About page.",
    rememberHistoryFilter: "Remember history filter",
    rememberHistoryFilterHint: "When off, the panel resets to \"All\" each time it opens. When on, the last filter is remembered until Mnemark exits.",
    // Panel
    searchPlaceholder: "Search clipboard history...",
    searchShortcutHint: "Press / to focus search",
    emptyTitle: "No clipboard history yet",
    emptyHint: "Copy something to get started",
    noResults: "No matching clips",
    categoryEmpty: "Nothing in this category yet",
    filterAll: "All",
    filterText: "Text",
    filterImage: "Image",
    filterFiles: "Files",
    filterLinks: "Links",
    filterBarLabel: "Filter by type",
    moreTitle: "More",
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
    filesMissingFallback: "Source files no longer exist — copied the path text instead",
    pasteFailed: "Paste failed — please try again (the clipboard may be busy)",
    copyFailed: "Copy failed — please try again (the clipboard may be busy)",
    pinLimitReached: "Maximum 10 pinned Clips",
    nothingToUndo: "Nothing to undo",
    hotkeyInvalid: "Invalid hotkey format",
    // Preview
    previewTypeText: "Text",
    previewTypeImage: "Image",
    previewTypeFiles: "Files",
    previewSource: "Source",
    previewCaptured: "Captured",
    previewSize: "Size",
    previewEmpty: "(no content)",
    previewTruncatedSizes: "Content truncated: only the opening portion was saved ({saved} saved, {original} original). The full content cannot be recovered.",
    previewTruncatedNoSizes: "Content truncated: only the opening portion was saved. The full content cannot be recovered.",
    previewHintBefore: "Select or point at an item, then press",
    previewHintAfter: "to open the preview, press again to close",
    pressToPreview: "Press to preview",
    // About
    aboutTitle: "About Mnemark",
    tagline: "Find anything you've copied.",
    nameMeaning: "mneme (memory) + mark",
    brandMeaning: "Everything you copy leaves a mark — Mnemark keeps those marks searchable, so you can always find your way back to something you've copied before.",
    changelog: "Changelog",
    checkUpdate: "Check for updates",
    checkingUpdate: "Checking for updates…",
    updateUpToDate: "Mnemark is up to date",
    updateAvailable: "Version v{v} is available",
    updateError: "Update check failed — please try again later",
    noReleaseYet: "No release published yet",
    installNow: "Install update",
    installing: "Downloading and installing…",
    restartNow: "Restart now",
    downloadingUpdate: "Downloading update…",
    portableAssetMissing: "No portable build in this release",
    portableSigMissing: "This release has no signature file — cannot verify the update",
    portableUpdateReady: "Downloaded to {path}. Quit Mnemark, then replace the old exe with the new file.",
    openFolder: "Open folder",
    // Drawer sidebar
    favorites: "Drawer",
    favoritesAdd: "New drawer",
    history: "History",
    returnToHistory: "Back to history",
    currentlyViewing: "Viewing: {name}",
    addToCollection: "Add to drawer",
    removeFromCollection: "Remove from drawer",
    addToOtherCollection: "Add to another drawer",
    addedToFavorites: "Added to drawer",
    removedFromFavorites: "Removed from drawer",
    collectionAdded: "Drawer created",
    collectionRemoved: "Drawer removed",
    collectionRenamed: "Drawer renamed",
    createCollection: "Create drawer",
    renameCollection: "Rename",
    moveUp: "Move Up",
    moveDown: "Move Down",
    remove: "Remove",
    confirm: "Confirm",
    collectionNamePlaceholder: "Drawer name",
    collectionNameInvalid: "Drawer name must be 1-64 characters",
    noCollections: "No drawers yet",
    noCollectionsHint: "Click \"Add\" in the top-right to create your first drawer",
    favoritesEmptyTitle: "No items in this drawer",
    favoritesEmptyHint: "Point at a history item and use \"More\" to add it to a drawer",
    removeCollectionTitle: "Delete drawer",
    removeCollectionBody: "Delete \"{name}\"? Its {count} items will not be deleted.",
    removeCollectionConfirm: "Delete",
    dragHandleLabel: "Drag to reorder",
    dragToDrawer: "Drag to drawer",
    favoritesToggle: "Drawer shortcut",
    favoritesToggleHint: "The key combination that opens/closes the drawer",
    pressKeysFavorites: "Press keys...",
    sidebarOpen: "Open drawer",
    sidebarClose: "Close drawer",
    dragToAdd: "Drag here to add",
    draggingItem: "Dragging",
    emptyPreview: "No preview available",
    dropHere: "Drop into drawer",
    alreadyInDrawer: "Already in this drawer",
    // Tutorial
    tutorial: "Tutorial",
    tutorialBack: "Back",
    tutorialNext: "Next",
    tutorialSkip: "Skip",
    tutorialStart: "Start Using",
    tutorialProgress: "Page {current} of {total}",
    tutorialTitleBackground: "Background and tray",
    tutorialBodyBackground: "Mnemark quietly records what you copy and lives in the system tray. Pause monitoring, open settings, tutorial, or quit from the tray anytime.",
    tutorialTitleShortcut: "Global shortcut",
    tutorialBodyShortcut: "Press {hotkey} anytime to show or hide the history panel. You can change it in Settings.",
    tutorialTitleSearch: "Search and filter",
    tutorialBodySearch: "Press / to focus search inside the panel, or use the category buttons to filter text, image, file, and link clips. Navigate with arrows, paste with Enter.",
    tutorialTitleActions: "Paste, copy, pin, delete",
    tutorialBodyActions: "Click an item to paste; copy, pin, and delete live on each row's buttons. Deletes can be undone.",
    tutorialTitlePreview: "Space preview",
    tutorialBodyPreview: "Select or point at an item and press Space to open the preview; press again to close.",
    tutorialTitleFavorites: "Drawer",
    tutorialBodyFavoritesOpen: "Click the star in the top-right of the history panel, or press {shortcut}, to open or close the drawer interface. You can change this shortcut in Settings.",
    tutorialBodyFavoritesAdd: "First select \"New drawer\" to create one. Then drag an item from its drag handle in History and drop it onto the target drawer. You can also choose a drawer from the item's \"More\" menu. Drag a drawer's own handle to reorder the drawers.",
    tutorialBodyFavoritesBrowse: "Select a drawer to show its items in the history panel. To return to the full clipboard history, select \"Back to history\" at the top of the drawer interface.",
    tutorialTitleSettings: "Settings and privacy",
    tutorialBodySettings: "In Settings you can tune history limits, language, theme, exclusions, and whether history is saved to disk.",
  },
};

let lang = "zh-TW";

export function currentLang(): string {
  return lang;
}

export function setLanguage(l: string) {
  lang = I18N[l] ? l : "zh-TW";
  document.documentElement.lang = lang;
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

/** Map a backend error string (a stable frontend/backend protocol, always
 * English) to a localized message. Unknown strings pass through unchanged. */
export function localizeBackendError(msg: string): string {
  if (msg.includes("Maximum") && msg.includes("pinned")) return t("pinLimitReached");
  if (msg.includes("Nothing to undo")) return t("nothingToUndo");
  if (msg.includes("Invalid hotkey")) return t("hotkeyInvalid");
  if (msg.includes("already in use")) return t("hotkeyInUse");
  if (msg.includes("must include")) return t("hotkeyNeedModifier");
  return msg;
}

/** Apply the current language to all [data-i18n] / [data-i18n-placeholder] /
 * [data-i18n-title] / [data-i18n-aria-label] elements. */
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
  root.querySelectorAll<HTMLElement>("[data-i18n-title]").forEach((el) => {
    const key = el.dataset.i18nTitle!;
    if (dict[key]) el.title = dict[key];
  });
  root.querySelectorAll<HTMLElement>("[data-i18n-aria-label]").forEach((el) => {
    const key = el.dataset.i18nAriaLabel!;
    if (dict[key]) el.setAttribute("aria-label", dict[key]);
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
