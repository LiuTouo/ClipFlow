import { invoke } from "@tauri-apps/api/core";
import { getCurrentWindow } from "@tauri-apps/api/window";
import { applyI18n, setLanguage, t, localizeBackendError } from "./i18n";
import { applyTheme } from "./theme";
import { shortcutLabel, isModifierCode, isFunctionCode, isPrintableCode, FAVORITES_DEFAULT_CODES } from "./shortcut";

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
  ui_opacity_percent: number;
  language: string;
  paste_files_as_files: boolean;
  auto_update: boolean;
  remember_history_filter: boolean;
  favorites_toggle_shortcut: { codes: string[] };
  tutorial_version: number;
}

// The saved config is the "baseline" the form is compared against for the
// dirty check. It is only replaced after a successful save.
let config: AppConfig;
let loading = true;
let saving = false;

// Favorites toggle chord — captured as physical KeyboardEvent.code values.
let favoritesShortcutCodes: string[] = FAVORITES_DEFAULT_CODES;
let recordingFavoritesShortcut = false;
let favoritesHeldModifiers: string[] = [];
let favoritesModifiersSeen = new Set<string>();

const form = document.getElementById("settings-form") as HTMLFormElement;
const fieldset = document.getElementById("settings-fieldset") as HTMLFieldSetElement;
const saveBtn = document.getElementById("btn-save") as HTMLButtonElement;
const cancelBtn = document.getElementById("btn-cancel") as HTMLButtonElement;
const statusEl = document.getElementById("settings-status") as HTMLElement;
const hotkeyInput = document.getElementById("setting-hotkey") as HTMLInputElement;
const hotkeyError = document.getElementById("hotkey-error") as HTMLElement;
const favoritesShortcutInput = document.getElementById("setting-favorites-shortcut") as HTMLInputElement;
const favoritesShortcutError = document.getElementById("favorites-shortcut-error") as HTMLElement;

function textInput(id: string): HTMLInputElement {
  return document.getElementById(id) as HTMLInputElement;
}

function selectInput(id: string): HTMLSelectElement {
  return document.getElementById(id) as HTMLSelectElement;
}

function parseExclusions(value: string): string[] {
  return value.split("\n").map(s => s.trim()).filter(s => s.length > 0);
}

/** Read the current form state into an AppConfig without mutating `config`. */
function readForm(): AppConfig {
  return {
    text_size_limit_kb: Number(textInput("setting-text-size-limit").value),
    text_count_limit: Number(textInput("setting-text-count-limit").value),
    image_count_limit: Number(textInput("setting-image-count-limit").value),
    image_memory_budget_mb: Number(textInput("setting-image-memory-budget").value),
    image_size_limit_mb: Number(textInput("setting-image-size-limit").value),
    hotkey: textInput("setting-hotkey").value,
    startup: textInput("setting-startup").checked,
    persist: textInput("setting-persist").checked,
    vim_mode: textInput("setting-vim-mode").checked,
    paste_files_as_files: textInput("setting-paste-files-as-files").checked,
    auto_update: textInput("setting-auto-update").checked,
    remember_history_filter: textInput("setting-remember-history-filter").checked,
    debounce_ms: Number(textInput("setting-debounce").value),
    theme: selectInput("setting-theme").value,
    ui_opacity_percent: Number(textInput("setting-ui-opacity").value),
    language: selectInput("setting-language").value,
    exclusion_list: parseExclusions((document.getElementById("setting-exclusions") as HTMLTextAreaElement).value),
    favorites_toggle_shortcut: { codes: [...favoritesShortcutCodes] },
    tutorial_version: config.tutorial_version,
  };
}

function exclusionListsEqual(a: string[], b: string[]): boolean {
  return a.length === b.length && a.every((v, i) => v === b[i]);
}

function favoritesCodesEqual(a: string[], b: string[]): boolean {
  return a.length === b.length && a.every((v, i) => v === b[i]);
}

function configsEqual(a: AppConfig, b: AppConfig): boolean {
  return a.text_size_limit_kb === b.text_size_limit_kb
    && a.text_count_limit === b.text_count_limit
    && a.image_count_limit === b.image_count_limit
    && a.image_memory_budget_mb === b.image_memory_budget_mb
    && a.image_size_limit_mb === b.image_size_limit_mb
    && a.hotkey === b.hotkey
    && a.startup === b.startup
    && a.persist === b.persist
    && a.vim_mode === b.vim_mode
    && a.paste_files_as_files === b.paste_files_as_files
    && a.auto_update === b.auto_update
    && a.remember_history_filter === b.remember_history_filter
    && a.debounce_ms === b.debounce_ms
    && a.theme === b.theme
    && a.ui_opacity_percent === b.ui_opacity_percent
    && a.language === b.language
    && exclusionListsEqual(a.exclusion_list, b.exclusion_list)
    && favoritesCodesEqual(a.favorites_toggle_shortcut.codes, b.favorites_toggle_shortcut.codes)
    && a.tutorial_version === b.tutorial_version;
}

function isDirty(): boolean {
  return !configsEqual(readForm(), config);
}

function setStatus(message: string, isError: boolean) {
  statusEl.textContent = message;
  statusEl.classList.toggle("is-error", isError);
  statusEl.setAttribute("role", isError ? "alert" : "status");
  statusEl.setAttribute("aria-live", isError ? "assertive" : "polite");
}

function clearStatus() {
  statusEl.textContent = "";
  statusEl.classList.remove("is-error");
  statusEl.setAttribute("role", "status");
  statusEl.setAttribute("aria-live", "polite");
}

function showHotkeyError(message: string) {
  hotkeyError.textContent = message;
  hotkeyError.classList.add("visible");
}

function clearHotkeyError() {
  hotkeyError.textContent = "";
  hotkeyError.classList.remove("visible");
}

/** Re-evaluate the Save button and page status from the current form state.
 * Save is enabled only when the form is both dirty and constraint-valid
 * (empty required numbers, min/max, and step mismatches all disable it). */
function updateDirtyState() {
  if (loading || saving) return;
  const dirty = isDirty();
  saveBtn.disabled = !dirty || !form.checkValidity();
  if (dirty) {
    setStatus(t("unsavedChanges"), false);
  } else {
    clearStatus();
  }
}

function setLoading(value: boolean) {
  loading = value;
  fieldset.disabled = value;
  saveBtn.disabled = true;
  cancelBtn.disabled = value;
  if (value) setStatus(t("loadingSettings"), false);
}

function setSaving(value: boolean) {
  saving = value;
  fieldset.disabled = value;
  saveBtn.disabled = true;
  cancelBtn.disabled = value;
  form.classList.toggle("is-saving", value);
  if (value) setStatus(t("saving"), false);
}

/** Terminal state when get_config fails: there is no config to edit against,
 * so the form stays disabled (never an enabled form operating on undefined
 * config), while Cancel/Escape remain usable as the only sane exit. */
function setLoadFailed() {
  loading = false;
  fieldset.disabled = true;
  saveBtn.disabled = true;
  cancelBtn.disabled = false;
  setStatus(t("settingsLoadFailed"), true);
}

async function init() {
  // Close paths must work even before (or without) a successful config load,
  // so a failed load still leaves the user a way out.
  bindCloseEvents();

  setLoading(true);
  try {
    config = await invoke<AppConfig>("get_config");
  } catch (err) {
    console.error("Failed to load config:", err);
    setLoadFailed();
    return;
  }

  setLanguage(config.language || "zh-TW");
  applyTheme(config.theme || "system");
  populateForm();
  applyI18n();
  document.title = `Mnemark ${t("settings")}`;
  bindFormEvents();
  setLoading(false);
  clearStatus();

  // When startup hotkey registration failed, the backend opened this window
  // and stashed the reason — show it inline so the user knows why they're
  // here (CONTEXT: Hotkey conflict detection).
  try {
    const startupError = await invoke<string | null>("take_startup_error");
    if (startupError) setStatus(localizeBackendError(startupError), true);
  } catch (_) {}
}

function populateForm() {
  textInput("setting-text-size-limit").value = String(config.text_size_limit_kb);
  textInput("setting-text-count-limit").value = String(config.text_count_limit);
  textInput("setting-image-count-limit").value = String(config.image_count_limit);
  textInput("setting-image-memory-budget").value = String(config.image_memory_budget_mb);
  textInput("setting-image-size-limit").value = String(config.image_size_limit_mb);
  textInput("setting-hotkey").value = config.hotkey;
  textInput("setting-startup").checked = config.startup;
  textInput("setting-persist").checked = config.persist;
  textInput("setting-vim-mode").checked = config.vim_mode;
  textInput("setting-paste-files-as-files").checked = config.paste_files_as_files;
  textInput("setting-auto-update").checked = config.auto_update;
  textInput("setting-remember-history-filter").checked = config.remember_history_filter;
  textInput("setting-debounce").value = String(config.debounce_ms);
  selectInput("setting-theme").value = config.theme;
  updateOpacityDisplay(config.ui_opacity_percent);
  selectInput("setting-language").value = config.language || "zh-TW";
  (document.getElementById("setting-exclusions") as HTMLTextAreaElement).value =
    config.exclusion_list.join("\n");
  favoritesShortcutCodes = (config.favorites_toggle_shortcut?.codes ?? FAVORITES_DEFAULT_CODES).slice();
  updateFavoritesShortcutDisplay();
}

function updateOpacityDisplay(value: number) {
  const opacity = Math.min(100, Math.max(50, Number.isFinite(value) ? value : 99));
  textInput("setting-ui-opacity").value = String(opacity);
  (document.getElementById("setting-ui-opacity-value") as HTMLOutputElement).value = `${opacity}%`;
}

async function onSubmit(e: Event) {
  e.preventDefault();
  if (saving) return;

  // Native constraint validation: numbers keep their min/max/step; report
  // before mutating anything so an invalid field blocks the save.
  if (!form.reportValidity()) return;

  if (!isDirty()) return;

  setSaving(true);
  const next = readForm();

  try {
    await invoke("update_config", { newConfig: next });
    config = next;
    await getCurrentWindow().close();
  } catch (err) {
    console.error("Save failed:", err);
    setSaving(false);
    // The form keeps its (dirty) values; re-enable Save so the user can retry.
    saveBtn.disabled = !isDirty();
    setStatus(localizeBackendError(String(err)), true);
  }
}

function startRecording() {
  clearHotkeyError();
  hotkeyInput.classList.add("recording");
  hotkeyInput.value = t("pressKeys");
  hotkeyInput.readOnly = true;
}

function onHotkeyBlur() {
  if (!hotkeyInput.classList.contains("recording")) return;
  hotkeyInput.value = config.hotkey;
  hotkeyInput.classList.remove("recording");
  hotkeyInput.readOnly = true;
  clearHotkeyError();
  updateDirtyState();
}

// === Favorites shortcut recording (physical KeyboardEvent.code values) ===
function updateFavoritesShortcutDisplay() {
  favoritesShortcutInput.value = shortcutLabel(favoritesShortcutCodes);
}

function clearFavoritesShortcutError() {
  favoritesShortcutError.textContent = "";
  favoritesShortcutError.classList.remove("visible");
}

function showFavoritesShortcutError(msg: string) {
  favoritesShortcutError.textContent = msg;
  favoritesShortcutError.classList.add("visible");
}

function startFavoritesRecording() {
  clearFavoritesShortcutError();
  recordingFavoritesShortcut = true;
  favoritesHeldModifiers = [];
  favoritesModifiersSeen = new Set();
  favoritesShortcutInput.classList.add("recording");
  favoritesShortcutInput.value = t("pressKeysFavorites");
  favoritesShortcutInput.readOnly = true;
}

function cancelFavoritesRecording() {
  recordingFavoritesShortcut = false;
  favoritesHeldModifiers = [];
  favoritesModifiersSeen = new Set();
  favoritesShortcutInput.classList.remove("recording");
  favoritesShortcutInput.readOnly = true;
  updateFavoritesShortcutDisplay();
  updateDirtyState();
}

function commitFavoritesShortcut(codes: string[]) {
  favoritesShortcutCodes = codes;
  recordingFavoritesShortcut = false;
  favoritesHeldModifiers = [];
  favoritesModifiersSeen = new Set();
  favoritesShortcutInput.classList.remove("recording");
  favoritesShortcutInput.readOnly = true;
  updateFavoritesShortcutDisplay();
  updateDirtyState();
}

function onFavoritesShortcutKeydown(e: KeyboardEvent) {
  if (!recordingFavoritesShortcut) return;
  e.preventDefault();
  e.stopPropagation();
  if (e.key === "Escape") { cancelFavoritesRecording(); return; }
  const code = e.code;
  if (isModifierCode(code)) {
    favoritesModifiersSeen.add(code);
    if (!favoritesHeldModifiers.includes(code)) favoritesHeldModifiers.push(code);
    return;
  }
  if (isFunctionCode(code)) {
    commitFavoritesShortcut([...favoritesHeldModifiers, code]);
    return;
  }
  if (isPrintableCode(code)) {
    if (favoritesHeldModifiers.length === 0) {
      showFavoritesShortcutError(t("hotkeyNeedModifier"));
      cancelFavoritesRecording();
      return;
    }
    commitFavoritesShortcut([...favoritesHeldModifiers, code]);
    return;
  }
  // Reserved or unrecognized physical code.
  showFavoritesShortcutError(t("hotkeyInvalid"));
  cancelFavoritesRecording();
}

function onFavoritesShortcutKeyup(e: KeyboardEvent) {
  if (!recordingFavoritesShortcut) return;
  if (!isModifierCode(e.code)) return;
  favoritesHeldModifiers = favoritesHeldModifiers.filter((c) => c !== e.code);
  // A bare modifier tap completes when the last held modifier is released.
  if (favoritesHeldModifiers.length === 0) {
    if (favoritesModifiersSeen.size === 1) {
      commitFavoritesShortcut([...favoritesModifiersSeen]);
    } else {
      showFavoritesShortcutError(t("hotkeyInvalid"));
      cancelFavoritesRecording();
    }
  }
}

function onFavoritesShortcutBlur() {
  if (recordingFavoritesShortcut) cancelFavoritesRecording();
}

function onHotkeyKeydown(e: KeyboardEvent) {
  // Not recording: make the click-to-record field keyboard-reachable via
  // Enter/Space when it already has focus.
  if (!hotkeyInput.classList.contains("recording")) {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      startRecording();
    }
    return;
  }

  // Recording: consume the key so it neither types nor bubbles to the global
  // Escape handler (which would close the window instead of cancelling).
  e.preventDefault();
  e.stopPropagation();

  if (e.key === "Escape") {
    hotkeyInput.value = config.hotkey;
    hotkeyInput.classList.remove("recording");
    hotkeyInput.readOnly = true;
    clearHotkeyError();
    updateDirtyState();
    return;
  }

  const parts: string[] = [];
  if (e.ctrlKey) parts.push("Ctrl");
  if (e.shiftKey) parts.push("Shift");
  if (e.altKey) parts.push("Alt");
  if (e.metaKey) parts.push("Super");

  const key = e.key;
  const isModifier = key === "Control" || key === "Shift" || key === "Alt" || key === "Meta";
  if (!isModifier) {
    parts.push(key.length === 1 ? key.toUpperCase() : key);

    if (parts.length === 1) {
      // Bare key without a modifier — as a global shortcut it would make
      // that key unusable in every other application.
      showHotkeyError(t("hotkeyNeedModifier"));
      hotkeyInput.value = config.hotkey;
    } else {
      clearHotkeyError();
      hotkeyInput.value = parts.join("+");
    }
    hotkeyInput.classList.remove("recording");
    // Stay readOnly: the field is click-to-record only, never free-typed.
    hotkeyInput.readOnly = true;
    updateDirtyState();
  }
}

/** Cancel button + Escape-to-close. Bound before config load so a failed load
 * still leaves a usable exit. Escape is ignored while recording (the input's
 * own handler cancels recording) and while saving (no mid-save close). */
function bindCloseEvents() {
  cancelBtn.addEventListener("click", () => {
    getCurrentWindow().close();
  });

  document.addEventListener("keydown", (e) => {
    if (e.key !== "Escape") return;
    if (hotkeyInput.classList.contains("recording")) return;
    if (saving) return;
    e.preventDefault();
    getCurrentWindow().close();
  });
}

function bindFormEvents() {
  // Live language preview
  selectInput("setting-language").addEventListener("change", (e) => {
    setLanguage((e.target as HTMLSelectElement).value);
    applyI18n();
    document.title = `Mnemark ${t("settings")}`;
  });

  // Live theme preview
  selectInput("setting-theme").addEventListener("change", (e) => {
    applyTheme((e.target as HTMLSelectElement).value);
  });

  // Live opacity readout
  textInput("setting-ui-opacity").addEventListener("input", (e) => {
    updateOpacityDisplay(Number((e.target as HTMLInputElement).value));
  });

  // Dirty tracking: `input` covers text/number/range/textarea, `change`
  // covers checkbox/select. populateForm() sets values programmatically and
  // therefore does not fire these events.
  form.addEventListener("input", updateDirtyState);
  form.addEventListener("change", updateDirtyState);

  form.addEventListener("submit", onSubmit);

  hotkeyInput.addEventListener("click", startRecording);
  hotkeyInput.addEventListener("keydown", onHotkeyKeydown);
  hotkeyInput.addEventListener("blur", onHotkeyBlur);

  favoritesShortcutInput.addEventListener("click", startFavoritesRecording);
  favoritesShortcutInput.addEventListener("keydown", onFavoritesShortcutKeydown);
  favoritesShortcutInput.addEventListener("keyup", onFavoritesShortcutKeyup);
  favoritesShortcutInput.addEventListener("blur", onFavoritesShortcutBlur);
}

window.addEventListener("DOMContentLoaded", init);
