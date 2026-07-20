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
fn update_config(new_config: AppConfig, state: tauri::State<AppState>) -> Result<(), String> {
    new_config.save()?;
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

#[tauri::command]
fn paste_text(text: String, _state: tauri::State<AppState>) -> Result<(), String> {
    clipboard::write_text_to_clipboard(&text)?;
    clipboard::simulate_ctrl_v();
    Ok(())
}

#[tauri::command]
fn paste_image(image_data: Vec<u8>, _state: tauri::State<AppState>) -> Result<(), String> {
    clipboard::write_image_to_clipboard(&image_data)?;
    clipboard::simulate_ctrl_v();
    Ok(())
}

#[tauri::command]
fn paste_file_paths(paths: String, _state: tauri::State<AppState>) -> Result<(), String> {
    clipboard::write_file_paths_to_clipboard(&paths)?;
    clipboard::simulate_ctrl_v();
    Ok(())
}

#[tauri::command]
fn copy_only_text(text: String, _state: tauri::State<AppState>) -> Result<(), String> {
    clipboard::write_text_to_clipboard(&text)
}

fn start_monitor(app_handle: tauri::AppHandle, history: Arc<Mutex<HistoryStore>>, config: Arc<Mutex<AppConfig>>, monitor_running: Arc<Mutex<bool>>) {
    std::thread::spawn(move || {
        let mut last_seq: u32 = 0;
        let mut last_hash: Option<(String, u64)> = None;

        loop {
            std::thread::sleep(std::time::Duration::from_millis(200));

            {
                let running = monitor_running.lock().unwrap();
                if !*running {
                    continue;
                }
            }

            use windows::Win32::System::DataExchange::GetClipboardSequenceNumber;
            let current_seq = unsafe { GetClipboardSequenceNumber() };

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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run(_hidden: bool) {
    let config = AppConfig::load();
    let history = Arc::new(Mutex::new(HistoryStore::new()));
    let config_store = Arc::new(Mutex::new(config.clone()));
    let monitor_running = Arc::new(Mutex::new(true));
    let last_deleted = Arc::new(Mutex::new(None));

    log("[ClipFlow] run() called");

    // Override resource_dir to look in exe directory for portable mode
    let exe_dir = std::env::current_exe()
        .unwrap_or_default()
        .parent()
        .unwrap_or_else(|| std::path::Path::new("."))
        .to_path_buf();
    let frontend_dist = exe_dir.join("dist");
    std::env::set_var("TAURI_FRONTEND_DIST", &frontend_dist);
    log(&format!("[ClipFlow] frontend dist: {:?}", frontend_dist));

    // Copy dist files from project dir to exe dir if needed
    let cwd = std::env::current_dir().unwrap_or_default();
    let project_dist = cwd.join("dist");
    if !frontend_dist.join("index.html").exists() && project_dist.join("index.html").exists() {
        log("[ClipFlow] copying dist from project dir");
        let _ = std::fs::create_dir_all(&frontend_dist);
        if let Ok(entries) = std::fs::read_dir(&project_dist) {
            for entry in entries.flatten() {
                if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                    let src = entry.path();
                    let dst = frontend_dist.join(entry.file_name());
                    let _ = std::fs::create_dir_all(&dst);
                    if let Ok(sub_entries) = std::fs::read_dir(&src) {
                        for se in sub_entries.flatten() {
                            let _ = std::fs::copy(se.path(), dst.join(se.file_name()));
                        }
                    }
                } else {
                    let _ = std::fs::copy(entry.path(), frontend_dist.join(entry.file_name()));
                }
            }
        }
    }

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
            use tauri_plugin_global_shortcut::GlobalShortcutExt;
            let hotkey_str = {
                let config = config_store.lock().unwrap();
                config.hotkey.clone()
            };

            let handle_clone = handle.clone();
            let handle_toggle = handle.clone();

            if let Ok(shortcut) = hotkey_str.parse::<tauri_plugin_global_shortcut::Shortcut>() {
                let _ = app.global_shortcut().on_shortcut(shortcut, move |_app, _sc, event| {
                    if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        if handle_toggle.get_webview_window("main")
                            .map(|w| w.is_visible().unwrap_or(false))
                            .unwrap_or(false)
                        {
                            hide_panel(&handle_toggle);
                        } else {
                            show_panel(&handle_toggle);
                        }
                    }
                });
            }

            let handle_debug = handle.clone();
            if let Ok(debug_sc) = "Ctrl+Shift+I".parse::<tauri_plugin_global_shortcut::Shortcut>() {
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
            use tauri::tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};

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
