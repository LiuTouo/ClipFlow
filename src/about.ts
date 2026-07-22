import { invoke } from "@tauri-apps/api/core";
import { getVersion } from "@tauri-apps/api/app";
import { open } from "@tauri-apps/plugin-shell";
import { resourceDir } from "@tauri-apps/api/path";
import { initLanguage, applyI18n, t } from "./i18n";
import { applyTheme } from "./theme";

const REPO = "LiuTouo/ClipFlow";

interface UpdateCheck {
  status: string; // "up_to_date" | "available"
  version: string | null;
}

interface GhAsset {
  name: string;
  browser_download_url: string;
}

/** Compare two semver-ish strings ("v1.2.3" or "1.2.3"): >0 if a is newer. */
function cmpSemver(a: string, b: string): number {
  const pa = a.replace(/^v/, "").split(".").map(Number);
  const pb = b.replace(/^v/, "").split(".").map(Number);
  for (let i = 0; i < 3; i++) {
    const d = (pa[i] || 0) - (pb[i] || 0);
    if (d !== 0) return d;
  }
  return 0;
}

function setStatus(text: string) {
  document.getElementById("update-status")!.textContent = text;
}

function show(id: string, visible: boolean) {
  document.getElementById(id)!.classList.toggle("hidden", !visible);
}

/** Installed build: check → install → restart, all via the updater plugin. */
async function installedCheck() {
  setStatus(t("checkingUpdate"));
  try {
    const result = await invoke<UpdateCheck>("check_for_updates");
    if (result.status === "available" && result.version) {
      setStatus(t("updateAvailable", { v: result.version }));
      show("btn-install-update", true);
    } else {
      setStatus(t("updateUpToDate"));
    }
  } catch (err) {
    console.error("Update check failed:", err);
    setStatus(t("updateError"));
  }
}

async function installedInstall() {
  show("btn-install-update", false);
  setStatus(t("installing"));
  try {
    await invoke<string>("install_update");
    show("btn-restart", true);
  } catch (err) {
    console.error("Install failed:", err);
    setStatus(t("updateError"));
  }
}

/** Portable build: GitHub API check → download new exe next to the current
 * one → user quits and overwrites manually (a running exe can't be replaced). */
async function portableCheck() {
  setStatus(t("checkingUpdate"));
  let data: { tag_name: string; assets: GhAsset[] };
  try {
    const res = await fetch(`https://api.github.com/repos/${REPO}/releases/latest`, {
      headers: { Accept: "application/vnd.github+json" },
    });
    if (res.status === 404) {
      setStatus(t("noReleaseYet"));
      return;
    }
    if (!res.ok) throw new Error(`HTTP ${res.status}`);
    data = await res.json();
  } catch (err) {
    console.error("Release check failed:", err);
    setStatus(t("updateError"));
    return;
  }

  const current = await getVersion();
  const latest = data.tag_name.replace(/^v/, "");
  if (cmpSemver(latest, current) <= 0) {
    setStatus(t("updateUpToDate"));
    return;
  }

  const asset = data.assets.find((a) => /portable/i.test(a.name) && a.name.endsWith(".exe"));
  if (!asset) {
    setStatus(t("portableAssetMissing"));
    return;
  }

  setStatus(t("updateAvailable", { v: latest }));
  await portableDownload(asset);
}

/** The actual download runs in Rust (update::download_portable_update):
 * GitHub's asset CDN omits CORS headers, so webview fetch always fails. */
async function portableDownload(asset: GhAsset) {
  setStatus(t("downloadingUpdate"));
  try {
    const path = await invoke<string>("download_portable_update", {
      url: asset.browser_download_url,
    });
    setStatus(t("portableUpdateReady", { path }));
    show("btn-open-folder", true);
  } catch (err) {
    console.error("Portable download failed:", err);
    setStatus(t("updateError"));
  }
}

async function init() {
  await initLanguage();
  applyI18n();
  document.title = t("aboutTitle");

  let autoUpdate = true;
  try {
    const config = await invoke<{ theme?: string; auto_update?: boolean }>("get_config");
    applyTheme(config.theme || "system");
    autoUpdate = config.auto_update !== false;
  } catch (_) {}

  // Version comes from the single source of truth (Cargo.toml via app config).
  try {
    const version = await getVersion();
    document.getElementById("about-version")!.textContent = `v${version}`;
  } catch (_) {}

  // Open links in the system browser instead of navigating the webview.
  const openLink = async (e: Event, url: string) => {
    e.preventDefault();
    try {
      await open(url);
    } catch (err) {
      console.error("Failed to open link:", err);
    }
  };
  document.getElementById("link-github")!.addEventListener("click", (e) =>
    openLink(e, "https://github.com/LiuTouo/ClipFlow")
  );
  document.getElementById("link-changelog")!.addEventListener("click", (e) =>
    openLink(e, "https://github.com/LiuTouo/ClipFlow/blob/main/CHANGELOG.md")
  );

  // Update section: the backend reports which channel this binary serves.
  let channel = "portable";
  try {
    channel = await invoke<string>("update_channel");
  } catch (_) {}

  const runCheck = channel === "installed" ? installedCheck : portableCheck;
  document.getElementById("btn-check-update")!.addEventListener("click", () => {
    show("btn-install-update", false);
    show("btn-restart", false);
    show("btn-open-folder", false);
    runCheck();
  });
  document.getElementById("btn-install-update")!.addEventListener("click", installedInstall);
  document.getElementById("btn-restart")!.addEventListener("click", () => invoke("restart_app"));
  document.getElementById("btn-open-folder")!.addEventListener("click", async () => {
    try {
      await open(await resourceDir());
    } catch (err) {
      console.error("Failed to open folder:", err);
    }
  });

  // Auto-check on open when enabled (installed builds also get a background
  // updater pass at app startup; this just surfaces the result here).
  if (autoUpdate) runCheck();
}

window.addEventListener("DOMContentLoaded", init);
