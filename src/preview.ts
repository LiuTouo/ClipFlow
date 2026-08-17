import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { applyI18n, setLanguage, t } from "./i18n";
import { applyTheme } from "./theme";

interface PreviewPayload {
  id: string;
  kind: "Text" | "Image" | "FilePaths";
  text_content: string | null;
  image_preview_base64: string | null;
  truncated: boolean;
  byte_size: number;
  captured_at: number;
  source_exe: string;
  source_title: string;
}

const kindEl = document.getElementById("preview-kind")!;
const warningEl = document.getElementById("preview-warning")!;
const contentEl = document.getElementById("preview-content")!;
const sourceEl = document.getElementById("preview-source")!;
const capturedEl = document.getElementById("preview-captured")!;
const sizeEl = document.getElementById("preview-size")!;

function typeLabel(kind: PreviewPayload["kind"]): string {
  switch (kind) {
    case "Image":
      return t("previewTypeImage");
    case "FilePaths":
      return t("previewTypeFiles");
    default:
      return t("previewTypeText");
  }
}

function formatBytes(n: number): string {
  if (n >= 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
  if (n >= 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${n} B`;
}

function formatCapturedAt(ts: number): string {
  const d = new Date(ts);
  const pad = (n: number) => String(n).padStart(2, "0");
  return `${d.getFullYear()}/${pad(d.getMonth() + 1)}/${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

function sourceText(p: PreviewPayload): string {
  const exe = !p.source_exe || p.source_exe === "Unknown" ? t("unknownSource") : p.source_exe;
  if (p.source_title && p.source_title !== p.source_exe) return `${exe} · ${p.source_title}`;
  return exe;
}

/** UTF-8 byte size of the saved (possibly truncated) text, matching byte_size units. */
function savedBytes(text: string | null): number {
  return text ? new Blob([text]).size : 0;
}

function render(payload: PreviewPayload) {
  kindEl.textContent = typeLabel(payload.kind);

  const source = sourceText(payload);
  sourceEl.textContent = source;
  sourceEl.title = source;
  capturedEl.textContent = formatCapturedAt(payload.captured_at);
  sizeEl.textContent = formatBytes(payload.byte_size);

  if (payload.truncated) {
    const saved = formatBytes(savedBytes(payload.text_content));
    warningEl.textContent = payload.byte_size > 0
      ? t("previewTruncatedSizes", { saved, original: formatBytes(payload.byte_size) })
      : t("previewTruncatedNoSizes");
    warningEl.classList.remove("hidden");
  } else {
    warningEl.classList.add("hidden");
  }

  // Rebuild content with createElement/textContent/value only — clip and
  // source data never pass through innerHTML.
  contentEl.replaceChildren();
  if (payload.kind === "Image") {
    const img = document.createElement("img");
    img.className = "preview-image";
    img.alt = t("previewTypeImage");
    const b64 = payload.image_preview_base64 || "";
    img.src = b64.startsWith("data:") || b64.startsWith("http")
      ? b64
      : `data:image/png;base64,${b64}`;
    contentEl.appendChild(img);
  } else {
    const ta = document.createElement("textarea");
    ta.className = "preview-textarea";
    ta.readOnly = true;
    ta.spellcheck = false;
    ta.wrap = "soft";
    ta.value = payload.text_content || "";
    ta.placeholder = t("previewEmpty");
    contentEl.appendChild(ta);
  }
}

async function init() {
  try {
    const config = await invoke<{ language?: string; theme?: string; ui_opacity_percent?: number }>("get_config");
    setLanguage(config.language || "zh-TW");
    applyTheme(config.theme || "system");
    const opacity = Math.min(100, Math.max(50, config.ui_opacity_percent ?? 96));
    document.documentElement.style.setProperty("--panel-opacity", String(opacity / 100));
  } catch {
    setLanguage("zh-TW");
  }
  applyI18n();

  await listen<PreviewPayload>("clip-preview-updated", (event) => render(event.payload));

  // Space or Escape in this window closes the preview and tells the main panel
  // to reset its toggle state. Space is handled on keydown so a press — not a
  // release — closes the preview, matching the panel's press-to-toggle gesture.
  // clip-preview-closed is emitted only after hide_clip_preview resolves, so
  // the main panel never treats an optimistic close as backend-confirmed.
  document.addEventListener("keydown", (e) => {
    if (e.key !== " " && e.key !== "Escape") return;
    if (e.repeat) return;
    e.preventDefault();
    invoke("hide_clip_preview")
      .then(() => { void emit("clip-preview-closed"); })
      .catch(() => {});
  });

  // Cover the first-load race: the backend may already hold an active preview
  // (window created before these listeners attached) — render it now.
  try {
    const active = await invoke<PreviewPayload | null>("get_active_clip_preview");
    if (active) render(active);
  } catch {
    // Command not present yet during early development — the listen path above
    // covers updates once the backend ships it.
  }
}

window.addEventListener("DOMContentLoaded", init);
