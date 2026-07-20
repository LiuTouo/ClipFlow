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
    panel_visible: Arc<Mutex<bool>>,
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
    let old_hotkey = {
        let config = state.config.lock().unwrap();
        config.hotkey.clone()
    };
    new_config.save()?;
    let new_hotkey = new_config.hotkey.clone();
    let mut config = state.config.lock().unwrap();
    *config = new_config;
    // If hotkey changed, re-register
    if old_hotkey != new_hotkey {
        // Unregister old, register new
        // Handled by frontend calling back after settings save + re-invoking hotkey setup
    }
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

fn toggle_panel(
    app: &tauri::AppHandle,
    history: &Arc<Mutex<HistoryStore>>,
    config: &Arc<Mutex<AppConfig>>,
    monitor_running: &Arc<Mutex<bool>>,
    panel_visible: &Arc<Mutex<bool>>,
) {
    let mut visible = panel_visible.lock().unwrap();

    if *visible {
        // Close panel
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.hide();
        }
        *visible = false;
    } else {
        // Open panel
        use tauri::WebviewUrl;
        use tauri::WebviewWindowBuilder;

        if let Some(window) = app.get_webview_window("main") {
            let _ = window.show();
            let _ = window.set_focus();
            *visible = true;
        } else {
            let _main_window = WebviewWindowBuilder::new(app, "main", WebviewUrl::App("index.html".into()))
                .title("ClipFlow")
                .inner_size(420.0, 540.0)
                .decorations(false)
                .resizable(false)
                .skip_taskbar(true)
                .visible(true)
                .build();
            *visible = true;
        }
    }
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run(hidden: bool) {
    let config = AppConfig::load();
    let history = Arc::new(Mutex::new(HistoryStore::new()));
    let config_store = Arc::new(Mutex::new(config.clone()));
    let monitor_running = Arc::new(Mutex::new(true));
    let last_deleted = Arc::new(Mutex::new(None));
    let panel_visible = Arc::new(Mutex::new(false));

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .manage(AppState {
            history: history.clone(),
            config: config_store.clone(),
            monitor_running: monitor_running.clone(),
            last_deleted: last_deleted.clone(),
            panel_visible: panel_visible.clone(),
        })
        .setup(move |app| {
            let handle = app.handle().clone();

            // Register global hotkey from config
            use tauri_plugin_global_shortcut::GlobalShortcutExt;
            let hotkey_str = {
                let config = config_store.lock().unwrap();
                config.hotkey.clone()
            };

            let handle_ref = handle.clone();
            let history_ref = history.clone();
            let config_ref = config_store.clone();
            let monitor_ref = monitor_running.clone();
            let panel_ref = panel_visible.clone();

            if let Ok(shortcut) = hotkey_str.parse::<tauri_plugin_global_shortcut::Shortcut>() {
                let result = app.global_shortcut().on_shortcut(shortcut, move |_app, _sc, event| {
                    if event.state == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        toggle_panel(
                            &handle_ref,
                            &history_ref,
                            &config_ref,
                            &monitor_ref,
                            &panel_ref,
                        );
                    }
                });
                if let Err(e) = result {
                    eprintln!("Failed to register hotkey '{}': {:?}", hotkey_str, e);
                    // Hotkey conflict — could show settings window here
                }
            } else {
                eprintln!("Invalid hotkey in config: {}", hotkey_str);
            }

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
                .on_tray_icon_event(|_tray, event| {
                    if let TrayIconEvent::Click {
                        button: MouseButton::Left,
                        button_state: MouseButtonState::Up,
                        ..
                    } = event
                    {
                        // Left click placeholder
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
