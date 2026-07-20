import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow } from "@tauri-apps/api/window";

interface Clip {
  id: string;
  kind: "Text" | "Image" | "FilePaths";
  text_content: string | null;
  image_data: number[] | null;
  thumbnail_base64: string | null;
  content_hash: string;
  preview: string;
  truncated: boolean;
  source_exe: string;
  source_title: string;
  source_icon: string | null;
  captured_at: number;
  pinned: boolean;
  byte_size: number;
}

let clips: Clip[] = [];
let selectedIndex = -1;
let vimMode = false;
let toastTimer: ReturnType<typeof setTimeout> | null = null;

const searchInput = document.getElementById("search-input") as HTMLInputElement;
const clipList = document.getElementById("clip-list")!;
const emptyState = document.getElementById("empty-state")!;
const toast = document.getElementById("toast")!;

// === Init ===
async function init() {
  clips = await invoke("get_clips");
  render();

  // Listen for clipboard updates
  await listen<Clip>("clipboard-update", (event) => {
    const clip = event.payload;
    // Dedup locally
    const existingIndex = clips.findIndex(c => c.content_hash === clip.content_hash);
    if (existingIndex >= 0) {
      clips[existingIndex] = clip;
    } else {
      clips.unshift(clip);
    }
    // Match backend ordering: pinned first, then newest captured_at.
    clips.sort((a, b) => Number(b.pinned) - Number(a.pinned) || b.captured_at - a.captured_at);
    render();
  });

  // Load vim mode setting
  try {
    const config = await invoke("get_config") as any;
    vimMode = config.vim_mode;
  } catch (_) {}
}

// === Render ===
function render() {
  const query = searchInput.value.toLowerCase();
  const filtered = clips.filter(c => {
    if (!query) return true;
    return c.preview.toLowerCase().includes(query)
      || c.source_exe.toLowerCase().includes(query)
      || c.source_title.toLowerCase().includes(query);
  });

  clipList.innerHTML = "";
  emptyState.classList.toggle("hidden", clips.length > 0);

  let hasPinned = false;
  let hasUnpinned = false;

  filtered.forEach((clip, index) => {
    if (clip.pinned && !hasPinned) {
      hasPinned = true;
    }
    if (!clip.pinned && !hasUnpinned && hasPinned) {
      // Insert divider
      const divider = document.createElement("div");
      divider.className = "pinned-divider";
      divider.textContent = "Pinned";
      clipList.appendChild(divider);
      hasUnpinned = true;
    }

    const el = document.createElement("div");
    el.className = `clip-item${clip.truncated ? " truncated" : ""}${index === selectedIndex ? " selected" : ""}`;
    el.dataset.index = String(index);
    el.addEventListener("click", (e) => {
      // Don't paste if clicking action buttons
      const target = e.target as HTMLElement;
      if (target.closest(".clip-action-btn")) return;
      pasteClip(clip);
    });

    // Icon / Thumbnail
    const iconDiv = document.createElement("div");

    if (clip.kind === "Image" && clip.thumbnail_base64) {
      iconDiv.className = "thumbnail-container";
      const img = document.createElement("img");
      img.src = clip.thumbnail_base64;
      img.alt = "Image";
      iconDiv.appendChild(img);
    } else if (clip.kind === "FilePaths") {
      iconDiv.className = "clip-icon text-icon";
      iconDiv.innerHTML = `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><path d="M14 2H6a2 2 0 0 0-2 2v16a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V8z"/><polyline points="14 2 14 8 20 8"/></svg>`;
    } else {
      iconDiv.className = "clip-icon text-icon";
      iconDiv.innerHTML = `<svg width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><rect x="9" y="9" width="13" height="13" rx="2"/><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1"/></svg>`;
    }

    el.appendChild(iconDiv);

    // Content
    const contentDiv = document.createElement("div");
    contentDiv.className = "clip-content";

    const title = document.createElement("div");
    title.className = "clip-title";
    let titleText = clip.preview || "(empty)";
    if (clip.kind === "Text") {
      titleText = titleText.replace(/\n/g, " ");
    }
    title.textContent = titleText;
    contentDiv.appendChild(title);

    const meta = document.createElement("div");
    meta.className = "clip-meta";
    const source = document.createElement("span");
    source.className = "source";
    source.textContent = clip.source_exe || "Unknown";
    meta.appendChild(source);

    const size = document.createElement("span");
    size.textContent = clip.kind === "Image"
      ? `${(clip.byte_size / 1024 / 1024).toFixed(1)}MB`
      : `${clip.byte_size} B`;
    meta.appendChild(size);

    contentDiv.appendChild(meta);
    el.appendChild(contentDiv);

    // Time
    const time = document.createElement("span");
    time.className = "clip-time";
    time.textContent = formatTime(clip.captured_at);
    el.appendChild(time);

    // Actions
    const actions = document.createElement("div");
    actions.className = "clip-actions";

    const pinBtn = document.createElement("button");
    pinBtn.className = `clip-action-btn pin-btn${clip.pinned ? " pinned" : ""}`;
    pinBtn.innerHTML = "📌";
    pinBtn.title = clip.pinned ? "Unpin" : "Pin";
    pinBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      togglePin(clip);
    });
    actions.appendChild(pinBtn);

    const copyBtn = document.createElement("button");
    copyBtn.className = "clip-action-btn";
    copyBtn.innerHTML = "📋";
    copyBtn.title = "Copy only";
    copyBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      copyOnly(clip);
    });
    actions.appendChild(copyBtn);

    const deleteBtn = document.createElement("button");
    deleteBtn.className = "clip-action-btn delete-btn";
    deleteBtn.innerHTML = "🗑";
    deleteBtn.title = "Delete";
    deleteBtn.addEventListener("click", (e) => {
      e.stopPropagation();
      deleteClip(clip);
    });
    actions.appendChild(deleteBtn);

    el.appendChild(actions);
    clipList.appendChild(el);
  });

  // Scroll selected into view
  if (selectedIndex >= 0) {
    const selected = clipList.querySelector(".clip-item.selected");
    selected?.scrollIntoView({ block: "nearest" });
  }
}

// === Actions ===
async function pasteClip(clip: Clip) {
  // The backend writes to the clipboard, hides the Panel (returning focus to
  // the previous app), then simulates Ctrl+V.
  try {
    switch (clip.kind) {
      case "Text":
      case "FilePaths":
        await invoke("paste_text", { text: clip.text_content || "" });
        break;
      case "Image":
        if (clip.image_data) {
          await invoke("paste_image", { imageData: Array.from(clip.image_data) });
        }
        break;
    }
  } catch (err) {
    console.error("Paste failed:", err);
  }
}

async function copyOnly(clip: Clip) {
  try {
    switch (clip.kind) {
      case "Text":
      case "FilePaths":
        await invoke("copy_only_text", { text: clip.text_content || "" });
        break;
      case "Image":
        if (clip.image_data) {
          await invoke("copy_only_image", { imageData: Array.from(clip.image_data) });
        }
        break;
    }
    showToast("Copied to clipboard");
  } catch (err) {
    console.error("Copy failed:", err);
  }
}

async function deleteClip(clip: Clip) {
  try {
    await invoke("delete_clip", { id: clip.id });
    clips = clips.filter(c => c.id !== clip.id);
    if (selectedIndex >= clips.length) {
      selectedIndex = clips.length - 1;
    }
    render();

    // Show undo toast
    showToast('Deleted <button class="undo-btn">Undo</button>', async () => {
      await invoke("undo_delete");
      clips = await invoke("get_clips");
      render();
    });
  } catch (err) {
    console.error("Delete failed:", err);
  }
}

async function togglePin(clip: Clip) {
  try {
    await invoke("set_pinned", { id: clip.id, pinned: !clip.pinned });
    clip.pinned = !clip.pinned;
    render();
  } catch (err) {
    showToast(String(err));
  }
}

async function closePanel() {
  await getCurrentWindow().hide();
}

// === Toast ===
function showToast(message: string, onUndo?: () => void) {
  if (toastTimer) clearTimeout(toastTimer);

  toast.innerHTML = "";
  const span = document.createElement("span");
  span.textContent = message;
  toast.appendChild(span);

  if (onUndo) {
    const undoBtn = document.createElement("button");
    undoBtn.className = "undo-btn";
    undoBtn.textContent = "Undo";
    undoBtn.addEventListener("click", () => {
      onUndo();
      hideToast();
    });
    toast.appendChild(undoBtn);
  }

  toast.classList.remove("hidden");

  toastTimer = setTimeout(() => {
    hideToast();
  }, 3000);
}

function hideToast() {
  toast.classList.add("hidden");
  if (toastTimer) clearTimeout(toastTimer);
}

// === Formatting ===
function formatTime(ts: number): string {
  const now = Date.now();
  const diff = now - ts;
  const sec = Math.floor(diff / 1000);
  const min = Math.floor(sec / 60);
  const hr = Math.floor(min / 60);

  if (sec < 60) return "just now";
  if (min < 60) return `${min}m ago`;
  if (hr < 24) return `${hr}h ago`;
  const days = Math.floor(hr / 24);
  return `${days}d ago`;
}

// === Keyboard Navigation ===
searchInput.addEventListener("keydown", (e) => {
  switch (e.key) {
    case "ArrowDown":
      e.preventDefault();
      selectedIndex = Math.min(selectedIndex + 1, clips.length - 1);
      render();
      break;
    case "ArrowUp":
      e.preventDefault();
      selectedIndex = Math.max(selectedIndex - 1, 0);
      render();
      break;
    case "Enter":
      e.preventDefault();
      if (selectedIndex >= 0 && selectedIndex < clips.length) {
        pasteClip(clips[selectedIndex]);
      }
      break;
    case "Escape":
      e.preventDefault();
      closePanel();
      break;
    case "j":
      if (vimMode) {
        e.preventDefault();
        selectedIndex = Math.min(selectedIndex + 1, clips.length - 1);
        render();
      }
      break;
    case "k":
      if (vimMode) {
        e.preventDefault();
        selectedIndex = Math.max(selectedIndex - 1, 0);
        render();
      }
      break;
  }
});

// Reset selected on new search input
searchInput.addEventListener("input", () => {
  selectedIndex = 0;
  render();
});

// Clicks on the transparent margin around the panel dismiss it.
document.body.addEventListener("click", (e) => {
  if (e.target === document.body || e.target === document.documentElement) {
    closePanel();
  }
});

// Focus-loss dismissal is handled on the Rust side (WindowEvent::Focused).

// === Initialize ===
window.addEventListener("DOMContentLoaded", init);
