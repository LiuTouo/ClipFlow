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
fn paste_text(text: String, state: tauri::State<AppState>) -> Result<(), String> {
    // Copy text to clipboard, then simulate Ctrl+V
    clipboard::write_text_to_clipboard(&text)?;
    // Simulate Ctrl+V
    clipboard::simulate_ctrl_v();
    Ok(())
}

#[tauri::command]
fn paste_image(image_data: Vec<u8>, state: tauri::State<AppState>) -> Result<(), String> {
    clipboard::write_image_to_clipboard(&image_data)?;
    clipboard::simulate_ctrl_v();
    Ok(())
}

#[tauri::command]
fn paste_file_paths(paths: String, state: tauri::State<AppState>) -> Result<(), String> {
    clipboard::write_file_paths_to_clipboard(&paths)?;
    clipboard::simulate_ctrl_v();
    Ok(())
}

#[tauri::command]
fn copy_only_text(text: String, state: tauri::State<AppState>) -> Result<(), String> {
    clipboard::write_text_to_clipboard(&text)
}

/// Start the clipboard monitoring background thread.
fn start_monitor(app_handle: tauri::AppHandle, history: Arc<Mutex<HistoryStore>>, config: Arc<Mutex<AppConfig>>, monitor_running: Arc<Mutex<bool>>) {
    std::thread::spawn(move || {
        let mut last_seq: u32 = 0;
        let mut last_hash: Option<(String, u64)> = None;

        loop {
            std::thread::sleep(std::time::Duration::from_millis(200));

            // Check if monitoring is paused
            {
                let running = monitor_running.lock().unwrap();
                if !*running {
                    continue;
                }
            }

            // Check clipboard sequence number
            let current_seq = unsafe {
                windows::Win32::System::DataExchange::GetClipboardSequenceNumber()
            };

            if current_seq == last_seq {
                continue;
            }
            last_seq = current_seq;

            let config = config.lock().unwrap().clone();

            // Debounce check
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as u64;

            if let Some((ref hash, ts)) = last_hash {
                if now - ts < config.debounce_ms {
                    continue;
                }
            }

            // Capture clipboard
            match clipboard::capture_clipboard(&config) {
                Ok(clip) => {
                    let content_hash = clip.content_hash.clone();

                    // Debounce dedup
                    if let Some((ref hash, _)) = last_hash {
                        if *hash == content_hash {
                            continue;
                        }
                    }
                    last_hash = Some((content_hash, now));

                    // Insert into history
                    let mut history = history.lock().unwrap();
                    let clip = history.insert(clip, &config);

                    // Emit event to frontend
                    let _ = app_handle.emit("clipboard-update", &clip);
                }
                Err(_) => {
                    // Unsupported format or excluded — silently skip
                }
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run(_hidden: bool) {
    let config = AppConfig::load();
    let history = Arc::new(Mutex::new(HistoryStore::new()));
    let config_store = Arc::new(Mutex::new(config.clone()));
    let monitor_running = Arc::new(Mutex::new(true));
    let last_deleted = Arc::new(Mutex::new(None));

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
            let handle = app.handle().clone();

            // Start clipboard monitor
            start_monitor(handle.clone(), history.clone(), config_store.clone(), monitor_running.clone());

            // Build tray menu
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

            let _tray = TrayIconBuilder::new()
                .icon(app.default_window_icon().unwrap().clone())
                .tooltip("ClipFlow")
                .menu(&menu)
                .on_menu_event(move |app, event| {
                    match event.id().as_ref() {
                        "pause" => {
                            let state = app.state::<AppState>();
                            let mut running = state.monitor_running.lock().unwrap();
                            *running = !*running;
                        }
                        "settings" => {
                            // Open settings window
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
                .on_tray_icon_event(|_tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        // Left click opens the panel
                    }
                })
                .build(app)?;

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

    // Check if settings window already exists
    if let Some(window) = app.get_webview_window("settings") {
        window.set_focus()?;
        return Ok(());
    }

    let _window = WebviewWindowBuilder::new(app, "settings", WebviewUrl::App("settings.html".into()))
        .title("ClipFlow Settings")
        .inner_size(500.0, 600.0)
        .resizable(false)
        .build()?;

    Ok(())
}

fn open_about_dialog(app: &tauri::AppHandle) -> Result<(), tauri::Error> {
    use tauri::WebviewUrl;
    use tauri::WebviewWindowBuilder;

    if let Some(window) = app.get_webview_window("about") {
        window.set_focus()?;
        return Ok(());
    }

    let _window = WebviewWindowBuilder::new(app, "about", WebviewUrl::App("about.html".into()))
        .title("About ClipFlow")
        .inner_size(360.0, 320.0)
        .resizable(false)
        .build()?;

    Ok(())
}
