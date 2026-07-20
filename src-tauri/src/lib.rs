mod clipboard;
mod history;
mod models;

use history::HistoryStore;
use models::{AppConfig, Clip};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};

struct AppState {
    history: Arc<Mutex<HistoryStore>>,
    config: Arc<Mutex<AppConfig>>,
    monitor_running: Arc<Mutex<bool>>,
    last_deleted: Arc<Mutex<Option<Clip>>>,
}

#[tauri::command]
fn get_clips(state: tauri::State<AppState>) -> Vec<Clip> {
    let history = state.history.lock().unwrap();
    history.get_all()
}

#[tauri::command]
fn delete_clip(id: String, state: tauri::State<AppState>) -> Result<(), String> {
    let mut history = state.history.lock().unwrap();
    let deleted = history.delete(&id);
    if let Some(clip) = deleted {
        let mut last = state.last_deleted.lock().unwrap();
        *last = Some(clip);
        Ok(())
    } else {
        Err("Clip not found".to_string())
    }
}

#[tauri::command]
fn undo_delete(state: tauri::State<AppState>) -> Result<Clip, String> {
    let mut last = state.last_deleted.lock().unwrap();
    if let Some(clip) = last.take() {
        let mut history = state.history.lock().unwrap();
        let config = state.config.lock().unwrap();
        let restored = history.insert(clip, &config);
        Ok(restored)
    } else {
        Err("Nothing to undo".to_string())
    }
}

#[tauri::command]
fn set_pinned(id: String, pinned: bool, state: tauri::State<AppState>) -> Result<(), String> {
    let mut history = state.history.lock().unwrap();
    history.set_pinned(&id, pinned)
}

#[tauri::command]
fn get_config(state: tauri::State<AppState>) -> AppConfig {
    let config = state.config.lock().unwrap();
    config.clone()
}

#[tauri::command]
fn update_config(new_config: AppConfig, app: tauri::AppHandle, state: tauri::State<AppState>) -> Result<(), String> {
    let old_hotkey = state.config.lock().unwrap().hotkey.clone();
    let mut swapped_hotkey = false;

    if new_config.hotkey != old_hotkey {
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

    if let Err(e) = new_config.save() {
        // Roll back the hotkey swap so runtime state matches the file on disk.
        if swapped_hotkey {
            if let Ok(new_sc) = new_config.hotkey.parse::<tauri_plugin_global_shortcut::Shortcut>() {
                use tauri_plugin_global_shortcut::GlobalShortcutExt;
                let _ = app.global_shortcut().unregister(new_sc);
            }
            let _ = register_panel_hotkey(&app, &old_hotkey);
        }
        return Err(e);
    }

    let mut config = state.config.lock().unwrap();
    *config = new_config;
    Ok(())
}

#[tauri::command]
fn pause_monitoring(state: tauri::State<AppState>) -> Result<(), String> {
    let mut running = state.monitor_running.lock().unwrap();
    *running = false;
    Ok(())
}

#[tauri::command]
fn resume_monitoring(state: tauri::State<AppState>) -> Result<(), String> {
    let mut running = state.monitor_running.lock().unwrap();
    *running = true;
    Ok(())
}

#[tauri::command]
fn is_monitoring(state: tauri::State<AppState>) -> bool {
    let running = state.monitor_running.lock().unwrap();
    *running
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

#[tauri::command]
async fn paste_image(app: tauri::AppHandle, image_data: Vec<u8>) -> Result<(), String> {
    clipboard::write_image_to_clipboard(&image_data)?;
    hide_and_paste(&app).await;
    Ok(())
}

#[tauri::command]
async fn paste_file_paths(app: tauri::AppHandle, paths: String) -> Result<(), String> {
    clipboard::write_file_paths_to_clipboard(&paths)?;
    hide_and_paste(&app).await;
    Ok(())
}

#[tauri::command]
fn copy_only_text(text: String, _state: tauri::State<AppState>) -> Result<(), String> {
    clipboard::write_text_to_clipboard(&text)
}

#[tauri::command]
fn copy_only_image(image_data: Vec<u8>, _state: tauri::State<AppState>) -> Result<(), String> {
    clipboard::write_image_to_clipboard(&image_data)
}

fn start_monitor(app_handle: tauri::AppHandle, history: Arc<Mutex<HistoryStore>>, config: Arc<Mutex<AppConfig>>, monitor_running: Arc<Mutex<bool>>) {
    std::thread::spawn(move || {
        use windows::Win32::System::DataExchange::GetClipboardSequenceNumber;

        let mut last_seq: u32 = 0;
        let mut last_hash: Option<(String, u64)> = None;

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
            last_seq = current_seq;

            let config = config.lock().unwrap().clone();

            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;

            if let Some((ref _hash, ts)) = last_hash {
                if now - ts < config.debounce_ms {
                    continue;
                }
            }

            match clipboard::capture_clipboard(&config) {
                Ok(clip) => {
                    let content_hash = clip.content_hash.clone();

                    if let Some((ref hash, _)) = last_hash {
                        if *hash == content_hash {
                            continue;
                        }
                    }
                    last_hash = Some((content_hash, now));

                    let mut history = history.lock().unwrap();
                    let clip = history.insert(clip, &config);
                    let _ = app_handle.emit("clipboard-update", &clip);
                }
                Err(_) => {}
            }
        }
    });
}

fn log(_msg: &str) {}

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
            .inner_size(420.0, 540.0)
            .decorations(false)
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
    let history = Arc::new(Mutex::new(HistoryStore::new()));
    let config_store = Arc::new(Mutex::new(config.clone()));
    let monitor_running = Arc::new(Mutex::new(true));
    let last_deleted = Arc::new(Mutex::new(None));

    log("[ClipFlow] run() called");

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            history: history.clone(),
            config: config_store.clone(),
            monitor_running: monitor_running.clone(),
            last_deleted: last_deleted.clone(),
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

            let handle_debug = handle.clone();
            if let Ok(debug_sc) = "Ctrl+Shift+I".parse::<tauri_plugin_global_shortcut::Shortcut>() {
                use tauri_plugin_global_shortcut::GlobalShortcutExt;
                let _ = app.global_shortcut().on_shortcut(debug_sc, move |_app, _sc, event| {
                    if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        show_panel(&handle_debug);
                    }
                });
            }

            log("[ClipFlow] hotkey registered, starting tray setup");
            // Start clipboard monitor
            start_monitor(handle.clone(), history.clone(), config_store.clone(), monitor_running.clone());

            // Build tray (programmatic only — no trayIcon in config)
            use tauri::menu::{MenuBuilder, MenuItemBuilder};
            use tauri::tray::TrayIconBuilder;

            let pause_item = MenuItemBuilder::with_id("pause", "Pause Monitoring").build(app)?;
            let settings_item = MenuItemBuilder::with_id("settings", "Settings").build(app)?;
            let about_item = MenuItemBuilder::with_id("about", "About").build(app)?;
            let quit_item = MenuItemBuilder::with_id("quit", "Quit").build(app)?;

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
                .tooltip("ClipFlow")
                .menu(&menu)
                .show_menu_on_left_click(false)
                .on_menu_event(move |app, event| {
                    match event.id().as_ref() {
                        "pause" => {
                            let state = app.state::<AppState>();
                            let mut running = state.monitor_running.lock().unwrap();
                            *running = !*running;
                            let _ = pause_item_handle.set_text(if *running {
                                "Pause Monitoring"
                            } else {
                                "Resume Monitoring"
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

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_clips,
            delete_clip,
            undo_delete,
            set_pinned,
            get_config,
            update_config,
            pause_monitoring,
            resume_monitoring,
            is_monitoring,
            paste_text,
            paste_image,
            paste_file_paths,
            copy_only_text,
            copy_only_image,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
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
        .inner_size(500.0, 600.0)
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
        .inner_size(360.0, 320.0)
        .resizable(false)
        .center()
        .build()?;

    Ok(())
}
