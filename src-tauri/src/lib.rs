mod clipboard;
mod history;
mod models;
mod persistence;
mod startup;
mod update;

use history::HistoryStore;
use models::{AppConfig, Clip, ClipboardUpdate};
use persistence::Persistence;
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};

struct AppState {
    history: Arc<Mutex<HistoryStore>>,
    config: Arc<Mutex<AppConfig>>,
    monitor_running: Arc<Mutex<bool>>,
    last_deleted: Arc<Mutex<Option<Clip>>>,
    persistence: Arc<Mutex<Option<Persistence>>>,
    tray_items: Arc<Mutex<Option<TrayMenuItems>>>,
}

/// Handles to the tray menu items, kept so their labels can be re-localized
/// when the UI language changes.
struct TrayMenuItems {
    pause: tauri::menu::MenuItem<tauri::Wry>,
    settings: tauri::menu::MenuItem<tauri::Wry>,
    about: tauri::menu::MenuItem<tauri::Wry>,
    quit: tauri::menu::MenuItem<tauri::Wry>,
}

struct TrayLabels {
    pause: &'static str,
    resume: &'static str,
    settings: &'static str,
    about: &'static str,
    quit: &'static str,
}

fn tray_labels(lang: &str) -> TrayLabels {
    match lang {
        "en" => TrayLabels {
            pause: "Pause Monitoring",
            resume: "Resume Monitoring",
            settings: "Settings",
            about: "About",
            quit: "Quit",
        },
        _ => TrayLabels {
            pause: "暫停監聽",
            resume: "繼續監聽",
            settings: "設定",
            about: "關於",
            quit: "結束",
        },
    }
}

/// Write-through to SQLite when persistence is enabled. Failures are
/// logged (debug builds) but never block the in-memory operation.
fn persist_with<F>(state: &AppState, f: F)
where
    F: FnOnce(&Persistence),
{
    let guard = state.persistence.lock().unwrap();
    if let Some(p) = guard.as_ref() {
        let _ = f(p);
    }
}

#[tauri::command]
fn get_clips(state: tauri::State<AppState>) -> Vec<Clip> {
    let history = state.history.lock().unwrap();
    history.get_all()
}

#[tauri::command]
fn delete_clip(id: String, state: tauri::State<AppState>) -> Result<(), String> {
    // Scoped guards: never hold one state lock while acquiring another —
    // keeps every command on the same lock order as undo_delete.
    let deleted = {
        let mut history = state.history.lock().unwrap();
        history.delete(&id)
    };
    if let Some(clip) = deleted {
        let clip_id = clip.id.clone();
        *state.last_deleted.lock().unwrap() = Some(clip);
        persist_with(&state, |p| {
            let _ = p.delete(&clip_id);
        });
        Ok(())
    } else {
        Err("Clip not found".to_string())
    }
}

#[tauri::command]
fn undo_delete(state: tauri::State<AppState>) -> Result<Clip, String> {
    let clip = {
        let mut last = state.last_deleted.lock().unwrap();
        last.take()
    };
    if let Some(clip) = clip {
        let (restored, evicted) = {
            let mut history = state.history.lock().unwrap();
            let config = state.config.lock().unwrap();
            history.insert(clip, &config)
        };
        persist_with(&state, |p| {
            let _ = p.upsert_capture(&restored);
            for id in &evicted {
                let _ = p.delete(id);
            }
        });
        Ok(restored)
    } else {
        Err("Nothing to undo".to_string())
    }
}

#[tauri::command]
fn set_pinned(id: String, pinned: bool, state: tauri::State<AppState>) -> Result<(), String> {
    {
        let mut history = state.history.lock().unwrap();
        history.set_pinned(&id, pinned)?;
    }
    persist_with(&state, |p| {
        let _ = p.set_pinned(&id, pinned);
    });
    Ok(())
}

#[tauri::command]
fn get_config(state: tauri::State<AppState>) -> AppConfig {
    let config = state.config.lock().unwrap();
    config.clone()
}

/// Undo a hotkey swap so runtime state matches the on-disk config.
fn rollback_hotkey_swap(app: &tauri::AppHandle, new_hotkey: &str, old_hotkey: &str) {
    if let Ok(new_sc) = new_hotkey.parse::<tauri_plugin_global_shortcut::Shortcut>() {
        use tauri_plugin_global_shortcut::GlobalShortcutExt;
        let _ = app.global_shortcut().unregister(new_sc);
    }
    let _ = register_panel_hotkey(app, old_hotkey);
}

/// Apply the persistence side of a config change. When enabling: open the
/// database and dump the current in-memory History. When disabling: delete
/// the database file, then drop the handle.
fn apply_persist(state: &AppState, enabled: bool) -> Result<(), String> {
    if enabled {
        let p = Persistence::open()?;
        let clips = state.history.lock().unwrap().get_all();
        p.dump(&clips)?;
        *state.persistence.lock().unwrap() = Some(p);
    } else {
        Persistence::delete_file()?;
        *state.persistence.lock().unwrap() = None;
    }
    Ok(())
}

/// Undo a persistence toggle after a later step failed.
fn rollback_persist(state: &AppState, failed_new_value: bool) {
    let _ = apply_persist(state, !failed_new_value);
}

#[tauri::command]
fn update_config(new_config: AppConfig, app: tauri::AppHandle, state: tauri::State<AppState>) -> Result<(), String> {
    let new_config = new_config.sanitized();
    let (old_hotkey, old_startup, old_persist, old_language, old_auto_update) = {
        let config = state.config.lock().unwrap();
        (config.hotkey.clone(), config.startup, config.persist, config.language.clone(), config.auto_update)
    };
    let mut swapped_hotkey = false;
    let mut swapped_startup = false;
    let mut swapped_persist = false;

    // 1. Hotkey swap (validated + registered before anything is persisted).
    if new_config.hotkey != old_hotkey {
        // A bare key (e.g. "A" or "F1") as a global shortcut makes that key
        // unusable in every other application — require a modifier.
        let has_modifier = ["Ctrl", "Shift", "Alt", "Super"]
            .iter()
            .any(|m| new_config.hotkey.contains(m));
        if !has_modifier {
            return Err(format!(
                "Hotkey '{}' must include at least one modifier (Ctrl/Shift/Alt)",
                new_config.hotkey
            ));
        }

        let new_shortcut = new_config
            .hotkey
            .parse::<tauri_plugin_global_shortcut::Shortcut>()
            .map_err(|e| format!("Invalid hotkey '{}': {}", new_config.hotkey, e))?;
        let old_shortcut = old_hotkey
            .parse::<tauri_plugin_global_shortcut::Shortcut>()
            .ok();

        if old_shortcut.as_ref() != Some(&new_shortcut) {
            // Register the new hotkey first; if it conflicts, the old one stays active.
            register_panel_hotkey(&app, &new_config.hotkey)?;
            if let Some(old) = &old_shortcut {
                use tauri_plugin_global_shortcut::GlobalShortcutExt;
                let _ = app.global_shortcut().unregister(old.clone());
            }
            swapped_hotkey = true;
        }
    }

    // 2. Autostart shortcut sync.
    if new_config.startup != old_startup {
        if let Err(e) = startup::set_startup(new_config.startup) {
            if swapped_hotkey {
                rollback_hotkey_swap(&app, &new_config.hotkey, &old_hotkey);
            }
            return Err(e);
        }
        swapped_startup = true;
    }

    // 3. History persistence toggle.
    if new_config.persist != old_persist {
        if let Err(e) = apply_persist(&state, new_config.persist) {
            if swapped_startup {
                let _ = startup::set_startup(old_startup);
            }
            if swapped_hotkey {
                rollback_hotkey_swap(&app, &new_config.hotkey, &old_hotkey);
            }
            return Err(e);
        }
        swapped_persist = true;
    }

    // 4. Persist config to disk; on failure roll back every side effect above.
    if let Err(e) = new_config.save() {
        if swapped_persist {
            rollback_persist(&state, new_config.persist);
        }
        if swapped_startup {
            let _ = startup::set_startup(old_startup);
        }
        if swapped_hotkey {
            rollback_hotkey_swap(&app, &new_config.hotkey, &old_hotkey);
        }
        return Err(e);
    }

    // 5. Config is on disk — sync cosmetic runtime state (tray menu labels).
    if new_config.language != old_language {
        let labels = tray_labels(&new_config.language);
        let running = *state.monitor_running.lock().unwrap();
        let items = state.tray_items.lock().unwrap();
        if let Some(items) = items.as_ref() {
            let _ = items.pause.set_text(if running { labels.pause } else { labels.resume });
            let _ = items.settings.set_text(labels.settings);
            let _ = items.about.set_text(labels.about);
            let _ = items.quit.set_text(labels.quit);
        }
    }

    // Toggling auto_update on takes effect without an app restart: run one
    // check now (installed builds only — spawn_auto_update_check re-verifies).
    let auto_update_turned_on = !old_auto_update && new_config.auto_update;

    let mut config = state.config.lock().unwrap();
    *config = new_config;
    drop(config);

    if auto_update_turned_on {
        update::spawn_auto_update_check(app.clone(), state.config.clone());
    }
    Ok(())
}

/// Write content to the clipboard, hide the Panel so focus returns to the
/// previous window, wait for focus to settle, then simulate Ctrl+V.
async fn hide_and_paste(app: &tauri::AppHandle) {
    hide_panel(app);
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    clipboard::simulate_ctrl_v();
}

#[tauri::command]
async fn paste_text(app: tauri::AppHandle, text: String) -> Result<(), String> {
    clipboard::write_text_to_clipboard(&text)?;
    hide_and_paste(&app).await;
    Ok(())
}

/// Fetch an Image Clip's raw DIB bytes from the History by id. Raw images
/// never cross IPC (see models::Clip::image_data), so paste/copy ask the
/// backend for the bytes at use time.
fn image_data_by_id(state: &AppState, id: &str) -> Result<Vec<u8>, String> {
    let history = state.history.lock().unwrap();
    history
        .clips
        .iter()
        .find(|c| c.id == id)
        .and_then(|c| c.image_data.clone())
        .ok_or_else(|| "Clip not found".to_string())
}

#[tauri::command]
async fn paste_image(app: tauri::AppHandle, id: String, state: tauri::State<'_, AppState>) -> Result<(), String> {
    let image_data = image_data_by_id(&state, &id)?;
    clipboard::write_image_to_clipboard(&image_data)?;
    hide_and_paste(&app).await;
    Ok(())
}

#[tauri::command]
fn copy_only_text(text: String, _state: tauri::State<AppState>) -> Result<(), String> {
    clipboard::write_text_to_clipboard(&text)
}

#[tauri::command]
fn copy_only_image(id: String, state: tauri::State<AppState>) -> Result<(), String> {
    let image_data = image_data_by_id(&state, &id)?;
    clipboard::write_image_to_clipboard(&image_data)
}

/// Paste a FilePaths entry as real files (CF_HDROP). Returns "files" or
/// "text" (all source files gone → path-text fallback).
#[tauri::command]
async fn paste_files(app: tauri::AppHandle, text: String) -> Result<String, String> {
    let outcome = clipboard::write_files_to_clipboard_from_text(&text)?;
    hide_and_paste(&app).await;
    Ok(outcome)
}

#[tauri::command]
fn copy_only_files(text: String) -> Result<String, String> {
    clipboard::write_files_to_clipboard_from_text(&text)
}

fn start_monitor(app_handle: tauri::AppHandle, history: Arc<Mutex<HistoryStore>>, config: Arc<Mutex<AppConfig>>, monitor_running: Arc<Mutex<bool>>, persistence: Arc<Mutex<Option<Persistence>>>) {
    std::thread::spawn(move || {
        use windows::Win32::System::DataExchange::GetClipboardSequenceNumber;

        let mut last_seq: u32 = 0;
        let mut last_hash: Option<(String, u64)> = None;
        // Own exe name, so content ClipFlow itself wrote (paste / copy-only
        // while the Panel had focus) keeps its original source attribution.
        let self_exe = std::env::current_exe()
            .ok()
            .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
            .unwrap_or_default();

        loop {
            std::thread::sleep(std::time::Duration::from_millis(200));

            let current_seq = unsafe { GetClipboardSequenceNumber() };

            {
                let running = monitor_running.lock().unwrap();
                if !*running {
                    // Keep last_seq in sync while paused: copies made during
                    // the pause are permanently lost, not captured on resume.
                    last_seq = current_seq;
                    continue;
                }
            }

            if current_seq == last_seq {
                continue;
            }

            let config = config.lock().unwrap().clone();

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;

            // Debounce: too soon after the last capture. Do NOT consume the
            // sequence number — the next poll retries and picks up the latest
            // content once the window has passed.
            if let Some((ref _hash, ts)) = last_hash {
                if now - ts < config.debounce_ms {
                    continue;
                }
            }

            // The sequence number is only consumed on success or definitive
            // failure (Skip). A Locked clipboard stays pending for next poll,
            // so copies made while another app holds the clipboard are not lost.
            match clipboard::capture_clipboard(&config) {
                Ok(mut clip) => {
                    last_seq = current_seq;
                    let content_hash = clip.content_hash.clone();

                    if let Some((ref hash, _)) = last_hash {
                        if *hash == content_hash {
                            continue;
                        }
                    }
                    last_hash = Some((content_hash.clone(), now));

                    if !self_exe.is_empty() && clip.source_exe.eq_ignore_ascii_case(&self_exe) {
                        if let Some(existing) = history.lock().unwrap().find_by_hash(&content_hash) {
                            clip.source_exe = existing.source_exe;
                            clip.source_title = existing.source_title;
                        }
                    }

                    let (clip, evicted) = {
                        let mut history = history.lock().unwrap();
                        history.insert(clip, &config)
                    };
                    {
                        let guard = persistence.lock().unwrap();
                        if let Some(p) = guard.as_ref() {
                            let _ = p.upsert_capture(&clip);
                            for id in &evicted {
                                let _ = p.delete(id);
                            }
                        }
                    }
                    let _ = app_handle.emit("clipboard-update", ClipboardUpdate { clip, evicted });
                }
                Err(clipboard::CaptureError::Locked) => continue,
                Err(clipboard::CaptureError::Skip(reason)) => {
                    log(&format!("[ClipFlow] capture skipped: {}", reason));
                    last_seq = current_seq;
                    continue;
                }
            }
        }
    });
}

/// Debug-only log. Release builds compile to a no-op (the app has no
/// console under windows_subsystem = "windows" anyway).
fn log(msg: &str) {
    #[cfg(debug_assertions)]
    eprintln!("{}", msg);
    #[cfg(not(debug_assertions))]
    let _ = msg;
}

fn show_panel(app: &tauri::AppHandle) {
    use tauri::WebviewUrl;
    use tauri::WebviewWindowBuilder;

    log("[ClipFlow] show_panel() called");
    if let Some(window) = app.get_webview_window("main") {
        log("[ClipFlow] panel exists, showing");
        let _ = window.show();
        let _ = window.set_focus();
    } else {
        log("[ClipFlow] creating new panel window");
        match WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
            .title("ClipFlow")
            // Window is larger than the panel (420x540) so the rounded
            // corners and CSS drop shadow have room inside a transparent frame.
            .inner_size(480.0, 620.0)
            .decorations(false)
            .transparent(true)
            // Disable the DWM undecorated shadow: tao defaults it on, which
            // draws a 1px white border + shadow around the whole window rect
            // instead of following the rounded panel. The panel has its own
            // CSS drop shadow.
            .shadow(false)
            .resizable(false)
            .skip_taskbar(true)
            .always_on_top(true)
            .visible(true)
            .focused(true)
            .center()
            .build()
        {
            Ok(w) => {
                log(&format!("[ClipFlow] panel created: {:?}", w.label()));
                // Click outside (focus loss) dismisses the Panel. The handler
                // is armed only after the window has gained focus once (with a
                // grace-period backstop), so a transient focus bounce during
                // creation doesn't immediately dismiss the Panel.
                let app_handle = app.clone();
                let armed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                let armed_for_event = armed.clone();
                w.on_window_event(move |event| {
                    match event {
                        tauri::WindowEvent::Focused(true) => {
                            armed_for_event.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                        tauri::WindowEvent::Focused(false) => {
                            if armed_for_event.load(std::sync::atomic::Ordering::Relaxed) {
                                hide_panel(&app_handle);
                            }
                        }
                        _ => {}
                    }
                });
                // Backstop: arm even if the initial focus event never arrives.
                std::thread::spawn(move || {
                    std::thread::sleep(std::time::Duration::from_millis(500));
                    armed.store(true, std::sync::atomic::Ordering::Relaxed);
                });
                let _ = w.show();
                let _ = w.set_focus();
            }
            Err(e) => {
                log(&format!("[ClipFlow] panel creation failed: {:?}", e));
            }
        }
    }
}

fn hide_panel(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
}

fn toggle_panel(app: &tauri::AppHandle) {
    let visible = app
        .get_webview_window("main")
        .map(|w| w.is_visible().unwrap_or(false))
        .unwrap_or(false);
    if visible {
        hide_panel(app);
    } else {
        show_panel(app);
    }
}

/// Register the global hotkey that toggles the Panel.
/// Returns Err if the combination is invalid or already owned by another app.
fn register_panel_hotkey(app: &tauri::AppHandle, hotkey_str: &str) -> Result<(), String> {
    use tauri_plugin_global_shortcut::GlobalShortcutExt;
    let shortcut = hotkey_str
        .parse::<tauri_plugin_global_shortcut::Shortcut>()
        .map_err(|e| format!("Invalid hotkey '{}': {}", hotkey_str, e))?;
    let handle = app.clone();
    app.global_shortcut()
        .on_shortcut(shortcut, move |_app, _sc, event| {
            if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                toggle_panel(&handle);
            }
        })
        .map_err(|e| format!("Hotkey '{}' is already in use: {}", hotkey_str, e))
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run(_hidden: bool) {
    let config = AppConfig::load();
    let mut history_store = HistoryStore::new();

    // Optional SQLite persistence: reload history left from previous runs.
    let persistence = if config.persist {
        match Persistence::open() {
            Ok(p) => {
                match p.load_all() {
                    Ok(clips) => {
                        for clip in clips {
                            history_store.insert(clip, &config);
                        }
                    }
                    Err(e) => log(&format!("[ClipFlow] failed to load persisted history: {}", e)),
                }
                Some(p)
            }
            Err(e) => {
                log(&format!("[ClipFlow] failed to open persistence database: {}", e));
                None
            }
        }
    } else {
        None
    };

    let history = Arc::new(Mutex::new(history_store));
    let config_store = Arc::new(Mutex::new(config.clone()));
    let monitor_running = Arc::new(Mutex::new(true));
    let last_deleted = Arc::new(Mutex::new(None));
    let persistence = Arc::new(Mutex::new(persistence));
    let tray_items = Arc::new(Mutex::new(None));

    log("[ClipFlow] run() called");

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_fs::init())
        .manage(AppState {
            history: history.clone(),
            config: config_store.clone(),
            monitor_running: monitor_running.clone(),
            last_deleted: last_deleted.clone(),
            persistence: persistence.clone(),
            tray_items: tray_items.clone(),
        })
        .setup(move |app| {
            let resource_dir = app.path().resource_dir().unwrap_or_default();
            log(&format!("[ClipFlow] resource_dir: {:?}", resource_dir));
            log("[ClipFlow] setup closure entered");
            let handle = app.handle().clone();

            log("[ClipFlow] registering hotkey");
            // Register global hotkey
            let hotkey_str = {
                let config = config_store.lock().unwrap();
                config.hotkey.clone()
            };

            if let Err(e) = register_panel_hotkey(&handle, &hotkey_str) {
                log(&format!("[ClipFlow] hotkey registration failed: {}", e));
                // Per spec: on conflict, open Settings so the user picks another combination.
                let _ = open_settings_window(&handle);
            }

            // Debug-only shortcut to force-show the Panel. Never registered
            // in release builds: a global Ctrl+Shift+I would steal the
            // devtools key from browsers and IDEs system-wide.
            #[cfg(debug_assertions)]
            {
                let handle_debug = handle.clone();
                if let Ok(debug_sc) = "Ctrl+Shift+I".parse::<tauri_plugin_global_shortcut::Shortcut>() {
                    use tauri_plugin_global_shortcut::GlobalShortcutExt;
                    let _ = app.global_shortcut().on_shortcut(debug_sc, move |_app, _sc, event| {
                        if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                            show_panel(&handle_debug);
                        }
                    });
                }
            }

            log("[ClipFlow] hotkey registered, starting tray setup");
            // Start clipboard monitor
            start_monitor(handle.clone(), history.clone(), config_store.clone(), monitor_running.clone(), persistence.clone());

            // Background auto-update check (installed builds only, and only
            // when auto_update is on — portable builds never touch the updater).
            update::spawn_auto_update_check(handle.clone(), config_store.clone());

            // Build tray (programmatic only — no trayIcon in config)
            use tauri::menu::{MenuBuilder, MenuItemBuilder};
            use tauri::tray::TrayIconBuilder;

            let tray_lang = config_store.lock().unwrap().language.clone();
            let labels = tray_labels(&tray_lang);

            let pause_item = MenuItemBuilder::with_id("pause", labels.pause).build(app)?;
            let settings_item = MenuItemBuilder::with_id("settings", labels.settings).build(app)?;
            let about_item = MenuItemBuilder::with_id("about", labels.about).build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", labels.quit).build(app)?;

            let menu = MenuBuilder::new(app)
                .item(&pause_item)
                .item(&settings_item)
                .separator()
                .item(&about_item)
                .item(&quit_item)
                .build()?;

            let icon = app.default_window_icon().cloned().unwrap();
            let pause_item_handle = pause_item.clone();

            let _tray = TrayIconBuilder::new()
                .icon(icon)
                .tooltip(&format!("ClipFlow v{}", env!("CARGO_PKG_VERSION")))
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, event| {
                    match event.id().as_ref() {
                        "pause" => {
                            let state = app.state::<AppState>();
                            let mut running = state.monitor_running.lock().unwrap();
                            *running = !*running;
                            let lang = state.config.lock().unwrap().language.clone();
                            let labels = tray_labels(&lang);
                            let _ = pause_item_handle.set_text(if *running {
                                labels.pause
                            } else {
                                labels.resume
                            });
                        }
                        "settings" => {
                            let _ = open_settings_window(app);
                        }
                        "about" => {
                            let _ = open_about_dialog(app);
                        }
                        "quit" => {
                            app.exit(0);
                        }
                        _ => {}
                    }
                })
                .build(app)?;
            log("[ClipFlow] tray built successfully");

            // Keep item handles so labels can be re-localized on language change.
            *tray_items.lock().unwrap() = Some(TrayMenuItems {
                pause: pause_item.clone(),
                settings: settings_item.clone(),
                about: about_item.clone(),
                quit: quit_item.clone(),
            });

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_clips,
            delete_clip,
            undo_delete,
            set_pinned,
            get_config,
            update_config,
            paste_text,
            paste_image,
            copy_only_text,
            copy_only_image,
            paste_files,
            copy_only_files,
            update::update_channel,
            update::check_for_updates,
            update::install_update,
            update::restart_app,
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run(|_app_handle, event| {
            // Tray app: closing the last window only returns to the
            // background — never exits. Quit is explicit via the tray menu
            // (app.exit bypasses this handler).
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}

fn open_settings_window(app: &tauri::AppHandle) -> Result<(), tauri::Error> {
    use tauri::WebviewUrl;
    use tauri::WebviewWindowBuilder;

    log("[ClipFlow] open_settings_window() called");
    if let Some(window) = app.get_webview_window("settings") {
        log("[ClipFlow] settings exists, focusing");
        window.set_focus()?;
        return Ok(());
    }

    log("[ClipFlow] creating settings window");
    let _ = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
        .title("ClipFlow Settings")
        .inner_size(500.0, 700.0)
        .resizable(false)
        .visible(true)
        .center()
        .build()?;

    log("[ClipFlow] settings window created");
    Ok(())
}

fn open_about_dialog(app: &tauri::AppHandle) -> Result<(), tauri::Error> {
    use tauri::WebviewUrl;
    use tauri::WebviewWindowBuilder;

    if let Some(window) = app.get_webview_window("about") {
        window.set_focus()?;
        return Ok(());
    }

    let _ = WebviewWindowBuilder::new(app, "about", WebviewUrl::App("about.html".into()))
        .title("About ClipFlow")
        .inner_size(360.0, 420.0)
        .resizable(false)
        .center()
        .build()?;

    Ok(())
}
