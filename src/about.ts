import { open } from "@tauri-apps/plugin-shell";
import { initLanguage, applyI18n, t } from "./i18n";

async function init() {
  await initLanguage();
  applyI18n();
  document.title = t("aboutTitle");

  // Open the repo in the system browser instead of navigating the webview.
  document.getElementById("link-github")!.addEventListener("click", async (e) => {
    e.preventDefault();
    try {
      await open("https://github.com/LiuTouo/ClipFlow");
    } catch (err) {
      console.error("Failed to open link:", err);
    }
  });
}

window.addEventListener("DOMContentLoaded", init);
