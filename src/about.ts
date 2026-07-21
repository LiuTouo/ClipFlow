import { getVersion } from "@tauri-apps/api/app";
import { open } from "@tauri-apps/plugin-shell";
import { initLanguage, applyI18n, t } from "./i18n";

async function init() {
  await initLanguage();
  applyI18n();
  document.title = t("aboutTitle");

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
}

window.addEventListener("DOMContentLoaded", init);
