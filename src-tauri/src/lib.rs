mod clipboard;
mod history;
mod models;
mod persistence;
mod startup;
mod update;

use history::HistoryStore;
use models::{AppConfig, Clip, ClipKind, ClipboardUpdate, PreviewPayload};
use persistence::Persistence;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use tauri::{Emitter, Manager};

/// Lock a shared-state mutex, recovering from poisoning instead of
/// panicking. Clipboard state is best-effort: every mutation is a simple
/// Vec/field update that cannot leave the structure inconsistent, so a
/// guard poisoned by a panicking caller is safe to recover — panicking
/// here instead would cascade into the monitor thread (via its own lock
/// calls) and silently kill clipboard capture.
pub(crate) fn lock<T>(m: &Mutex<T>) -> std::sync::MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|e| e.into_inner())
}

struct AppState {
    history: Arc<Mutex<HistoryStore>>,
    config: Arc<Mutex<AppConfig>>,
    monitor_running: Arc<Mutex<bool>>,
    last_deleted: Arc<Mutex<Option<Clip>>>,
    persistence: Arc<Mutex<Option<Persistence>>>,
    tray_items: Arc<Mutex<Option<TrayMenuItems>>>,
    /// Hotkey-registration failure that opened Settings at startup, shown
    /// inline there (CONTEXT: Hotkey conflict detection).
    startup_error: Arc<Mutex<Option<String>>>,
    /// Active clip preview payload. Kept so a freshly loaded preview page can
    /// call get_active_clip_preview and cannot miss the first update event.
    preview: Arc<Mutex<Option<PreviewPayload>>>,
    /// Monotonic preview-generation token. Every show and hide intent bumps it;
    /// a show may display only while its claimed generation is still the newest.
    preview_generation: Arc<AtomicU64>,
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
    let guard = lock(&state.persistence);
    if let Some(p) = guard.as_ref() {
        let _ = f(p);
    }
}

#[tauri::command]
fn get_clips(state: tauri::State<AppState>) -> Vec<Clip> {
    let history = lock(&state.history);
    history.get_all_for_ipc()
}

#[tauri::command]
fn delete_clip(id: String, state: tauri::State<AppState>) -> Result<(), String> {
    // Scoped guards: never hold one state lock while acquiring another —
    // keeps every command on the same lock order as undo_delete.
    let deleted = {
        let mut history = lock(&state.history);
        history.delete(&id)
    };
    if let Some(clip) = deleted {
        let clip_id = clip.id.clone();
        *lock(&state.last_deleted) = Some(clip);
        persist_with(&state, |p| {
            let _ = p.delete(&clip_id);
        });
        Ok(())
    } else {
        Err("Clip not found".to_string())
    }
}

#[tauri::command]
fn undo_delete(id: String, state: tauri::State<AppState>) -> Result<Clip, String> {
    let clip = {
        let mut last = lock(&state.last_deleted);
        // Undo is keyed to the deleted Clip's id: only the most recent delete
        // is restorable, and a stale undo request (e.g. from an outdated
        // toast) must not restore some other Clip.
        if last.as_ref().is_some_and(|c| c.id == id) {
            last.take()
        } else {
            None
        }
    };
    if let Some(clip) = clip {
        let (restored, evicted) = {
            let mut history = lock(&state.history);
            let config = lock(&state.config);
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
        let mut history = lock(&state.history);
        history.set_pinned(&id, pinned)?;
    }
    persist_with(&state, |p| {
        let _ = p.set_pinned(&id, pinned);
    });
    Ok(())
}

#[tauri::command]
fn get_config(state: tauri::State<AppState>) -> AppConfig {
    let config = lock(&state.config);
    config.clone()
}

/// The hotkey-registration failure that opened Settings at startup, if any.
/// Taken (read once, then cleared) so the page shows it exactly once.
#[tauri::command]
fn take_startup_error(state: tauri::State<AppState>) -> Option<String> {
    lock(&state.startup_error).take()
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
        let mut p = Persistence::open()?;
        let clips = lock(&state.history).get_all();
        p.dump(&clips)?;
        *lock(&state.persistence) = Some(p);
    } else {
        Persistence::delete_file()?;
        *lock(&state.persistence) = None;
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
        let config = lock(&state.config);
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
        let running = *lock(&state.monitor_running);
        let items = lock(&state.tray_items);
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

    let mut config = lock(&state.config);
    *config = new_config;
    drop(config);

    if auto_update_turned_on {
        update::spawn_auto_update_check(app.clone(), state.config.clone());
    }
    Ok(())
}

/// Write content to the clipboard, hide the Panel so focus returns to the
/// previous window, WAIT for that focus change to actually happen (a blind
/// fixed sleep loses pastes whenever focus is slow to move), then send
/// Ctrl+V.
async fn hide_and_paste(app: &tauri::AppHandle) {
    let panel_hwnd = clipboard::foreground_hwnd(); // the Panel has focus now
    hide_panel(app);
    // Poll until the foreground leaves our Panel (max ~1s), then let it
    // settle briefly. On timeout, paste anyway — best effort.
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(1000);
    loop {
        let fg = clipboard::foreground_hwnd();
        if (fg != 0 && fg != panel_hwnd) || std::time::Instant::now() >= deadline {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(25)).await;
    }
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    // Never paste into the desktop shell: with a file clip, Ctrl+V on the
    // desktop copies the referenced files there. The content stays on the
    // clipboard for a manual paste instead (per the Paste spec).
    if clipboard::foreground_is_desktop() {
        log("[ClipFlow] paste suppressed: foreground is the desktop shell");
        return;
    }
    if let Err(e) = clipboard::simulate_ctrl_v() {
        // Phase-2 failure path per the Paste spec: the content is already
        // on the clipboard, so the user can still Ctrl+V manually.
        log(&format!("[ClipFlow] paste simulation failed: {}", e));
    }
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
    lock(&state.history).get_clip_image(id)
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

/// Build the serializable preview payload for one Clip. For Image entries the
/// stored DIB is decoded and re-encoded as a bounded display-only JPEG data
/// URL — done here, outside any AppState/HistoryStore lock (see the caller).
fn build_preview_payload(clip: Clip) -> Result<PreviewPayload, String> {
    let image_preview_base64 = if clip.kind == ClipKind::Image {
        let dib = clip
            .image_data
            .as_deref()
            .ok_or_else(|| "Image data missing".to_string())?;
        Some(clipboard::generate_preview_data_url(dib)?)
    } else {
        None
    };
    Ok(PreviewPayload {
        id: clip.id,
        kind: clip.kind,
        text_content: clip.text_content,
        image_preview_base64,
        truncated: clip.truncated,
        byte_size: clip.byte_size,
        captured_at: clip.captured_at,
        source_exe: clip.source_exe,
        source_title: clip.source_title,
    })
}

/// True when a show whose generation is `mine` is still the newest intent
/// (`now` has not advanced past it). A later show or hide bumps the shared
/// generation and supersedes every earlier claim.
fn show_is_current(now: u64, mine: u64) -> bool {
    now == mine
}

#[tauri::command]
async fn show_clip_preview(
    id: String,
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<(), String> {
    // Claim a fresh generation before any work. Heavy DIB/JPEG/base64 work
    // below runs on the async runtime (off the UI main thread); a later show
    // or hide bumps the generation and supersedes us. SeqCst gives one total
    // order over every show/hide intent, which is exactly what latest-wins
    // needs — and is negligible for a token bumped a few times per interaction.
    let generation = state.preview_generation.fetch_add(1, Ordering::SeqCst) + 1;

    // Clone a single Clip (never get_all), then release the history lock
    // before image decode.
    let clip = lock(&state.history)
        .get_clip(&id)
        .ok_or_else(|| "Clip not found".to_string())?;

    let payload = build_preview_payload(clip)?;

    commit_preview_on_main_thread(&app, generation, payload).await
}

/// Commit a prepared preview to the UI on the Tauri main thread and hand the
/// result back to the awaiting async command. Heavy work stays off the main
/// thread; only this one non-blocking closure runs there, so its generation
/// re-check and window mutation are ordered with respect to every other
/// main-thread task (and to the hide/clear ordering).
async fn commit_preview_on_main_thread(
    app: &tauri::AppHandle,
    generation: u64,
    payload: PreviewPayload,
) -> Result<(), String> {
    let (tx, rx) = tokio::sync::oneshot::channel();
    let handle = app.clone();
    app.run_on_main_thread(move || {
        let state = handle.state::<AppState>();
        // Re-check the generation before any window mutation: a superseded
        // show completes as a no-op.
        if !show_is_current(state.preview_generation.load(Ordering::SeqCst), generation) {
            let _ = tx.send(Ok(()));
            return;
        }
        let _ = tx.send(commit_preview_window(&handle, generation, &payload));
    })
    .map_err(|e| format!("run_on_main_thread failed: {:?}", e))?;

    rx.await
        .map_err(|_| "preview commit channel closed".to_string())?
}

/// Perform the current-generation preview commit on the main thread: create or
/// reuse the window, position it, set the active payload, emit the update
/// event, then show it. Must only run on the main thread — its position
/// getters resolve inline there, so it never blocks on the main loop.
fn commit_preview_window(
    app: &tauri::AppHandle,
    generation: u64,
    payload: &PreviewPayload,
) -> Result<(), String> {
    let window = get_or_create_preview_window(app)?;
    position_preview(app, &window)?;

    let state = app.state::<AppState>();
    *lock(&state.preview) = Some(payload.clone());
    // Event emission is best-effort: the active payload covers a listener that
    // races page load.
    let _ = app.emit("clip-preview-updated", payload);
    if let Err(e) = window.show() {
        // Clear the stale active payload only if we are still current, so a
        // concurrent hide/new show that already cleared or overwrote it wins.
        if show_is_current(state.preview_generation.load(Ordering::SeqCst), generation) {
            *lock(&state.preview) = None;
        }
        return Err(format!("preview window show failed: {:?}", e));
    }
    Ok(())
}

#[tauri::command]
fn hide_clip_preview(app: tauri::AppHandle) {
    hide_preview_window(&app);
}

#[tauri::command]
fn get_active_clip_preview(state: tauri::State<AppState>) -> Option<PreviewPayload> {
    lock(&state.preview).clone()
}

/// True while `now` is still inside the debounce window of the last capture.
fn within_debounce(now: u64, last_capture_ts: u64, debounce_ms: u64) -> bool {
    now.saturating_sub(last_capture_ts) < debounce_ms
}

/// True when this capture repeats content first observed inside the debounce
/// window (double Ctrl+C noise). The same content observed AFTER the window
/// is a deliberate re-copy and must be kept.
fn is_double_copy(hash: &str, first_seen: u64, last_hash: &Option<(String, u64)>, debounce_ms: u64) -> bool {
    matches!(last_hash, Some((h, ts)) if *h == hash && within_debounce(first_seen, *ts, debounce_ms))
}

/// Track when the CURRENT pending clipboard change was first observed. A new
/// sequence number resets the clock: otherwise the double-copy comparison
/// runs against the first sighting of older, since-replaced content and can
/// misread a deliberate re-copy as double-copy noise. Returns the updated
/// (pending_seq, pending_since) plus the first-observation time to use.
fn track_first_seen(
    pending_seq: Option<u32>,
    pending_since: Option<u64>,
    current_seq: u32,
    now: u64,
) -> (Option<u32>, Option<u64>, u64) {
    match (pending_seq, pending_since) {
        (Some(s), Some(t)) if s == current_seq => (pending_seq, pending_since, t),
        _ => (Some(current_seq), Some(now), now),
    }
}

/// Clipboard monitor state. One instance lives on the monitor thread for the
/// lifetime of the app; `tick` runs one poll iteration.
struct Monitor {
    app: tauri::AppHandle,
    history: Arc<Mutex<HistoryStore>>,
    config: Arc<Mutex<AppConfig>>,
    running: Arc<Mutex<bool>>,
    persistence: Arc<Mutex<Option<Persistence>>>,
    /// Own exe name, so content ClipFlow itself wrote (paste / copy-only
    /// while the Panel had focus) keeps its original source attribution.
    self_exe: String,
    last_seq: u32,
    last_hash: Option<(String, u64)>,
    /// First-observation time + sequence number of the unconsumed clipboard
    /// change, used for debounce comparisons (see tick's capture match).
    pending_since: Option<u64>,
    pending_seq: Option<u32>,
}

impl Monitor {
    fn tick(&mut self) {
        use windows::Win32::System::DataExchange::GetClipboardSequenceNumber;

        let current_seq = unsafe { GetClipboardSequenceNumber() };

        {
            let running = lock(&self.running);
            if !*running {
                // Keep last_seq in sync while paused: copies made during
                // the pause are permanently lost, not captured on resume.
                self.last_seq = current_seq;
                self.pending_since = None;
                self.pending_seq = None;
                return;
            }
        }

        if current_seq == self.last_seq {
            return;
        }

        let config = lock(&self.config).clone();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let (pending_seq, pending_since, first_seen) =
            track_first_seen(self.pending_seq, self.pending_since, current_seq, now);
        self.pending_seq = pending_seq;
        self.pending_since = pending_since;

        // Debounce: too soon after the last capture. Do NOT consume the
        // sequence number — the next poll retries and picks up the latest
        // content once the window has passed.
        if let Some((_, ts)) = self.last_hash {
            if within_debounce(now, ts, config.debounce_ms) {
                return;
            }
        }

        // The sequence number is only consumed on success or definitive
        // failure (Skip). A Locked clipboard stays pending for next poll,
        // so copies made while another app holds the clipboard are not lost.
        match clipboard::capture_clipboard(&config) {
            Ok(mut clip) => {
                self.last_seq = current_seq;
                self.pending_since = None;
                self.pending_seq = None;
                let content_hash = clip.content_hash.clone();

                if is_double_copy(&content_hash, first_seen, &self.last_hash, config.debounce_ms) {
                    return;
                }
                self.last_hash = Some((content_hash.clone(), now));

                if !self.self_exe.is_empty() && clip.source_exe.eq_ignore_ascii_case(&self.self_exe) {
                    if let Some((exe, title)) = lock(&self.history).source_by_hash(&content_hash) {
                        clip.source_exe = exe;
                        clip.source_title = title;
                    }
                }

                let (clip, evicted) = {
                    let mut history = lock(&self.history);
                    history.insert(clip, &config)
                };
                {
                    let guard = lock(&self.persistence);
                    if let Some(p) = guard.as_ref() {
                        let _ = p.upsert_capture(&clip);
                        for id in &evicted {
                            let _ = p.delete(id);
                        }
                    }
                }
                let _ = self.app.emit("clipboard-update", ClipboardUpdate { clip, evicted });
            }
            Err(clipboard::CaptureError::Locked) => {}
            Err(clipboard::CaptureError::Skip(reason)) => {
                log(&format!("[ClipFlow] capture skipped: {}", reason));
                self.last_seq = current_seq;
                self.pending_since = None;
                self.pending_seq = None;
            }
        }
    }
}

fn start_monitor(app_handle: tauri::AppHandle, history: Arc<Mutex<HistoryStore>>, config: Arc<Mutex<AppConfig>>, monitor_running: Arc<Mutex<bool>>, persistence: Arc<Mutex<Option<Persistence>>>) {
    std::thread::spawn(move || {
        let mut monitor = Monitor {
            app: app_handle,
            history,
            config,
            running: monitor_running,
            persistence,
            self_exe: std::env::current_exe()
                .ok()
                .and_then(|p| p.file_name().map(|n| n.to_string_lossy().to_string()))
                .unwrap_or_default(),
            last_seq: 0,
            last_hash: None,
            pending_since: None,
            pending_seq: None,
        };

        loop {
            std::thread::sleep(std::time::Duration::from_millis(200));
            // A panicking iteration must not kill clipboard monitoring:
            // untrusted clipboard bytes reach the image decoders, and a dead
            // monitor thread fails silently — the user never notices history
            // has stopped. Log and keep polling.
            if std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| monitor.tick())).is_err() {
                log("[ClipFlow] monitor iteration panicked; clipboard watching continues");
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

/// Preview window sizing/positioning constants, in logical pixels. The main
/// window is a 480x620 transparent host whose visual panel sits at logical
/// offset (30, 30) with width 420; the preview attaches beside that panel.
const PANEL_OFFSET: i32 = 30;
const PANEL_WIDTH: i32 = 420;
const PREVIEW_GAP: i32 = 8;
/// Preferred logical size of the preview UI window. Distinct from the image
/// preview JPEG bound (720x480), which lives inside generate_preview_data_url.
const PREVIEW_WINDOW_W: u32 = 360;
const PREVIEW_WINDOW_H: u32 = 540;

/// Computed preview window placement, all in physical pixels.
struct PreviewPlacement {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

/// Pure positioning math for the clip-preview window. Inputs: the main
/// window's physical outer position, its scale factor, and the current
/// monitor's physical work area; plus the logical panel offset/width, the
/// logical gap, and the logical preferred preview size. Output is the preview
/// window's physical (x, y) and size.
///
/// Rules: right of the panel preferred, left fallback; width clamped to the
/// available side space; fully clamped into the work area; top aligned with
/// the panel top; never overlaps the panel.
fn place_preview(
    main_pos: (i32, i32),
    scale: f64,
    work_area: (i32, i32, u32, u32),
    panel_offset: i32,
    panel_width: i32,
    gap: i32,
    content_width: u32,
    content_height: u32,
) -> PreviewPlacement {
    let to_phys = |v: i32| -> i32 { ((v as f64) * scale).round() as i32 };

    let (mx, my) = main_pos;
    let (wx, wy, ww, wh) = work_area;
    let work_right = wx + ww as i32;
    let work_bottom = wy + wh as i32;

    let panel_left = mx + to_phys(panel_offset);
    let panel_right = panel_left + to_phys(panel_width);
    let panel_top = my + to_phys(panel_offset);

    let pref_w = to_phys(content_width as i32).max(1) as u32;
    let pref_h = to_phys(content_height as i32).max(1) as u32;
    let gap_px = to_phys(gap);

    // Available horizontal space on each side (physical), never negative.
    let right_avail = (work_right - panel_right - gap_px).max(0) as u32;
    let left_avail = (panel_left - wx - gap_px).max(0) as u32;

    // Right preferred; left fallback; if neither fits, the side with more room
    // (ties to the right) and the width is clamped.
    let use_right = right_avail >= pref_w || right_avail >= left_avail;
    let avail = if use_right { right_avail } else { left_avail };
    let width = pref_w.min(avail).max(1);

    // Height clamped to what fits below the panel top within the work area.
    let max_h = (work_bottom - panel_top).max(0) as u32;
    let height = pref_h.min(max_h).max(1);

    let x = if use_right {
        panel_right + gap_px
    } else {
        panel_left - gap_px - width as i32
    };
    let y = panel_top.clamp(wy, (work_bottom - height as i32).max(wy));

    PreviewPlacement {
        x,
        y,
        width,
        height,
    }
}

/// Pure coordinate math: compute the i32 (x, y) that centers window of
/// `win_size` inside a monitor at `mon_pos` with `mon_size`.
///
/// Safe for all inputs: intermediate i64 arithmetic prevents overflow, the
/// final i32 conversion saturates to the i32 range, and the result is clamped
/// so the window never lands above or left of the monitor origin (handles both
/// negative monitor coords and windows larger than the monitor).
fn center_coords(mon_pos: (i32, i32), mon_size: (u32, u32), win_size: (u32, u32)) -> (i32, i32) {
    let cx = mon_pos.0 as i64 + (mon_size.0 as i64 - win_size.0 as i64) / 2;
    let cy = mon_pos.1 as i64 + (mon_size.1 as i64 - win_size.1 as i64) / 2;
    let cx = cx.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
    let cy = cy.clamp(i32::MIN as i64, i32::MAX as i64) as i32;
    (cx.max(mon_pos.0), cy.max(mon_pos.1))
}

/// Position `window` centered on the monitor that currently contains the
/// cursor. Every failure is logged and swallowed: a transient cursor/monitor
/// lookup failure must not prevent showing the panel.
fn center_on_cursor_monitor(app: &tauri::AppHandle, window: &tauri::WebviewWindow) {
    let cursor = match app.cursor_position() {
        Ok(p) => p,
        Err(e) => {
            log(&format!("[ClipFlow] cursor_position failed: {:?}", e));
            return;
        }
    };

    let monitor = match app.monitor_from_point(cursor.x, cursor.y) {
        Ok(Some(m)) => m,
        Ok(None) => {
            log("[ClipFlow] monitor_from_point returned None");
            return;
        }
        Err(e) => {
            log(&format!("[ClipFlow] monitor_from_point failed: {:?}", e));
            return;
        }
    };

    let window_size = window.outer_size().unwrap_or(tauri::PhysicalSize {
        width: 480,
        height: 620,
    });

    let mon_pos = monitor.position();
    let mon_size = monitor.size();

    let (x, y) = center_coords(
        (mon_pos.x, mon_pos.y),
        (mon_size.width, mon_size.height),
        (window_size.width, window_size.height),
    );

    if let Err(e) = window.set_position(tauri::PhysicalPosition::new(x, y)) {
        log(&format!("[ClipFlow] set_position failed: {:?}", e));
    }
}

fn show_panel(app: &tauri::AppHandle) {
    use tauri::WebviewUrl;
    use tauri::WebviewWindowBuilder;

    log("[ClipFlow] show_panel() called");
    if let Some(window) = app.get_webview_window("main") {
        log("[ClipFlow] panel exists, showing");
        center_on_cursor_monitor(app, &window);
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
            .visible(false)
            .focused(false)
            .build()
        {
            Ok(w) => {
                log(&format!("[ClipFlow] panel created: {:?}", w.label()));
                center_on_cursor_monitor(app, &w);
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
                            // Focus may have moved to the attached preview, not
                            // away from the composite group: defer to a delayed
                            // re-check that dismisses only when NEITHER window
                            // owns focus.
                            if armed_for_event.load(std::sync::atomic::Ordering::Relaxed) {
                                schedule_focus_group_check(&app_handle);
                            }
                        }
                        tauri::WindowEvent::Destroyed => {
                            // A destroyed main window must not strand a visible
                            // preview.
                            hide_preview_window(&app_handle);
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
    // The preview is an attached part of the panel: hiding the panel (paste,
    // toggle, focus loss) must never leave the preview visible.
    hide_preview_window(app);
}

/// Hide only the clip-preview window and clear its active payload. Never
/// touches the main panel. Bumps the generation first so any in-flight show
/// that has not yet committed sees a stale generation and no-ops. No operation
/// lock: the show commit is serialized on the main thread, so a hide intent
/// either lands before it (making it stale) or after it (and hides it).
fn hide_preview_window(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    state.preview_generation.fetch_add(1, Ordering::SeqCst);
    if let Some(window) = app.get_webview_window("clip-preview") {
        let _ = window.hide();
    }
    *lock(&state.preview) = None;
}

/// Main + preview are one composite focus group. On focus loss, sleep briefly
/// off-thread (the OS focus transition to/from the preview is not
/// instantaneous), then run the actual focus queries and hide decision on the
/// Tauri main thread. The delay closes the race where the main window's
/// Focused(false) fired because the preview just took focus.
fn schedule_focus_group_check(app: &tauri::AppHandle) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(150));
        let handle = app.clone();
        if let Err(e) = app.run_on_main_thread(move || {
            let focused = |label: &str| {
                handle
                    .get_webview_window(label)
                    .and_then(|w| w.is_focused().ok())
                    .unwrap_or(false)
            };
            if !focused("main") && !focused("clip-preview") {
                hide_panel(&handle);
            }
        }) {
            log(&format!("[ClipFlow] run_on_main_thread failed: {:?}", e));
        }
    });
}

/// Lazily create (or reuse) the attached clip-preview window. The window is
/// created hidden and unfocused so hover alone never steals focus; explicit
/// pointer interaction may focus it. Returns Err when creation fails.
fn get_or_create_preview_window(app: &tauri::AppHandle) -> Result<tauri::WebviewWindow, String> {
    use tauri::{WebviewUrl, WebviewWindowBuilder};

    if let Some(w) = app.get_webview_window("clip-preview") {
        return Ok(w);
    }

    let w = WebviewWindowBuilder::new(app, "clip-preview", WebviewUrl::App("preview.html".into()))
        .title("ClipFlow Preview")
        .decorations(false)
        .transparent(true)
        .shadow(false)
        .skip_taskbar(true)
        .always_on_top(true)
        .resizable(false)
        .inner_size(PREVIEW_WINDOW_W as f64, PREVIEW_WINDOW_H as f64)
        .visible(false)
        .focused(false)
        .build()
        .map_err(|e| format!("preview window creation failed: {:?}", e))?;
    // Preview owns focus only through explicit pointer interaction; losing it
    // must not dismiss the pair while main still has focus, so route through
    // the same composite re-check.
    let app_handle = app.clone();
    w.on_window_event(move |event| {
        if let tauri::WindowEvent::Focused(false) = event {
            schedule_focus_group_check(&app_handle);
        }
    });
    Ok(w)
}

/// Position the preview window beside the visual main panel (main outer
/// position + panel offset), and size it to the available side space.
fn position_preview(app: &tauri::AppHandle, window: &tauri::WebviewWindow) -> Result<(), String> {
    let main = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    let main_pos = main
        .outer_position()
        .map_err(|e| format!("main outer_position failed: {:?}", e))?;
    let scale = main.scale_factor().unwrap_or(1.0);
    let monitor = main
        .current_monitor()
        .map_err(|e| format!("current_monitor failed: {:?}", e))?
        .ok_or_else(|| "no monitor".to_string())?;
    let wa = monitor.work_area();
    let placement = place_preview(
        (main_pos.x, main_pos.y),
        scale,
        (wa.position.x, wa.position.y, wa.size.width, wa.size.height),
        PANEL_OFFSET,
        PANEL_WIDTH,
        PREVIEW_GAP,
        PREVIEW_WINDOW_W,
        PREVIEW_WINDOW_H,
    );
    window
        .set_size(tauri::PhysicalSize::new(placement.width, placement.height))
        .map_err(|e| format!("preview set_size failed: {:?}", e))?;
    window
        .set_position(tauri::PhysicalPosition::new(placement.x, placement.y))
        .map_err(|e| format!("preview set_position failed: {:?}", e))?;
    Ok(())
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
    update::cleanup_stale_portable_update();
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
    let startup_error: Arc<Mutex<Option<String>>> = Arc::new(Mutex::new(None));

    log("[ClipFlow] run() called");

    tauri::Builder::default()
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .manage(AppState {
            history: history.clone(),
            config: config_store.clone(),
            monitor_running: monitor_running.clone(),
            last_deleted: last_deleted.clone(),
            persistence: persistence.clone(),
            tray_items: tray_items.clone(),
            startup_error: startup_error.clone(),
            preview: Arc::new(Mutex::new(None)),
            preview_generation: Arc::new(AtomicU64::new(0)),
        })
        .setup(move |app| {
            let resource_dir = app.path().resource_dir().unwrap_or_default();
            log(&format!("[ClipFlow] resource_dir: {:?}", resource_dir));
            log("[ClipFlow] setup closure entered");
            let handle = app.handle().clone();

            log("[ClipFlow] registering hotkey");
            // Register global hotkey
            let hotkey_str = {
                let config = lock(&config_store);
                config.hotkey.clone()
            };

            if let Err(e) = register_panel_hotkey(&handle, &hotkey_str) {
                log(&format!("[ClipFlow] hotkey registration failed: {}", e));
                // Per spec: on conflict, open Settings so the user picks
                // another combination — with the reason shown inline.
                *lock(&startup_error) = Some(e);
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

            let tray_lang = lock(&config_store).language.clone();
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
                            let mut running = lock(&state.monitor_running);
                            *running = !*running;
                            let lang = lock(&state.config).language.clone();
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
            *lock(&tray_items) = Some(TrayMenuItems {
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
            take_startup_error,
            update_config,
            paste_text,
            paste_image,
            copy_only_text,
            copy_only_image,
            paste_files,
            copy_only_files,
            show_clip_preview,
            hide_clip_preview,
            get_active_clip_preview,
            update::update_channel,
            update::check_for_updates,
            update::install_update,
            update::restart_app,
            update::download_portable_update,
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

#[cfg(test)]
mod shell_open_scope_tests {
    /// tauri-plugin-shell compiles plugins.shell.open at startup wrapped as
    /// ^{pattern}$ and panics on an invalid regex — guard the config here.
    #[test]
    fn shell_open_regex_compiles_and_scopes_correctly() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let pattern = conf["plugins"]["shell"]["open"]
            .as_str()
            .expect("plugins.shell.open must be set");
        let re = regex::Regex::new(&format!("^{pattern}$")).unwrap();

        // About-page links and the open-folder button must keep working.
        assert!(re.is_match("https://github.com/LiuTouo/ClipFlow"));
        assert!(re.is_match("C:\\Users\\me\\AppData\\Local\\ClipFlow"));
        assert!(re.is_match("D:/portable/ClipFlow"));

        // Everything else must be rejected by the webview surface.
        for bad in [
            "http://github.com/x",
            "javascript:alert(1)",
            "file:///C:/Windows",
            "mailto:a@b.c",
            "\\\\server\\share\\x",
        ] {
            assert!(!re.is_match(bad), "should reject: {bad}");
        }
    }
}

#[cfg(test)]
mod monitor_debounce_tests {
    use super::{is_double_copy, track_first_seen, within_debounce};

    #[test]
    fn within_debounce_window() {
        assert!(within_debounce(150, 0, 200));
        // Boundary: exactly debounce_ms later is OUTSIDE the window.
        assert!(!within_debounce(200, 0, 200));
        assert!(!within_debounce(500, 0, 200));
    }

    #[test]
    fn double_copy_inside_window_is_dropped() {
        let last = Some(("hashA".to_string(), 0u64));
        assert!(is_double_copy("hashA", 150, &last, 200));
    }

    #[test]
    fn same_content_after_window_is_a_deliberate_recopy() {
        let last = Some(("hashA".to_string(), 0u64));
        assert!(!is_double_copy("hashA", 300, &last, 200));
    }

    #[test]
    fn different_content_is_never_double_copy() {
        let last = Some(("hashA".to_string(), 0u64));
        assert!(!is_double_copy("hashB", 50, &last, 200));
    }

    #[test]
    fn no_previous_capture_is_never_double_copy() {
        assert!(!is_double_copy("hashA", 50, &None, 200));
    }

    #[test]
    fn first_seen_persists_while_the_same_sequence_is_pending() {
        // Second poll of the same pending change keeps the original time.
        let (seq, since, first) = track_first_seen(Some(7), Some(1000), 7, 1200);
        assert_eq!(seq, Some(7));
        assert_eq!(since, Some(1000));
        assert_eq!(first, 1000);
    }

    #[test]
    fn first_seen_resets_when_a_newer_sequence_arrives() {
        // Copy B replaced copy A while A was still pending: the debounce
        // clock must run from B's first observation, not A's.
        let (seq, since, first) = track_first_seen(Some(7), Some(1000), 8, 1200);
        assert_eq!(seq, Some(8));
        assert_eq!(since, Some(1200));
        assert_eq!(first, 1200);
    }

    #[test]
    fn first_seen_starts_on_first_observation() {
        let (seq, since, first) = track_first_seen(None, None, 7, 1000);
        assert_eq!(seq, Some(7));
        assert_eq!(since, Some(1000));
        assert_eq!(first, 1000);
    }
}

#[cfg(test)]
mod center_coords_tests {
    use super::center_coords;

    #[test]
    fn center_on_positive_monitor() {
        let (x, y) = center_coords((0, 0), (1920, 1080), (480, 620));
        assert_eq!(x, 720);
        assert_eq!(y, 230);
    }

    #[test]
    fn center_on_negative_monitor_origin() {
        let (x, y) = center_coords((-1920, 0), (1920, 1080), (480, 620));
        assert_eq!(x, -1200);
        assert_eq!(y, 230);
    }

    #[test]
    fn window_larger_than_monitor_clamps_to_origin() {
        let (x, y) = center_coords((0, 0), (800, 600), (1024, 768));
        assert_eq!(x, 0);
        assert_eq!(y, 0);
    }

    #[test]
    fn odd_dimensions_truncate_correctly() {
        let (x, y) = center_coords((0, 0), (1921, 1079), (480, 620));
        assert_eq!(x, 720);
        assert_eq!(y, 229);
    }

    #[test]
    fn negative_monitor_with_window_larger_clamps() {
        let (x, y) = center_coords((-500, -300), (640, 480), (800, 600));
        assert_eq!(x, -500);
        assert_eq!(y, -300);
    }

    #[test]
    fn extreme_monitor_position_saturates_to_i32_range() {
        // i64 center would overflow i32 — the saturating clamp keeps it in range.
        let (x, y) = center_coords(
            (i32::MAX - 100, i32::MIN + 100),
            (2000, 2000),
            (480, 620),
        );
        // x: center adds (2000-480)/2=760, overflows i32 → saturates at i32::MAX.
        assert_eq!(x, i32::MAX);
        // y: center adds (2000-620)/2=690 → i32::MIN+100+690 = i32::MIN+790, fits.
        assert_eq!(y, i32::MIN + 790);
        // Neither wrapped.
        assert!(x >= i32::MAX - 100);
        assert!(y >= i32::MIN);
        assert!(y <= i32::MIN + 1000);
    }
}

#[cfg(test)]
mod preview_placement_tests {
    use super::place_preview;

    fn place(
        main_pos: (i32, i32),
        scale: f64,
        work_area: (i32, i32, u32, u32),
    ) -> super::PreviewPlacement {
        place_preview(main_pos, scale, work_area, 30, 420, 8, 360, 540)
    }

    #[test]
    fn right_side_when_space_available() {
        let p = place((100, 100), 1.0, (0, 0, 1920, 1080));
        // panel: left 130, right 550, top 130. gap 8. right fits 360.
        assert_eq!(p.x, 558);
        assert_eq!(p.y, 130);
        assert_eq!(p.width, 360);
        assert_eq!(p.height, 540);
        // Never overlaps the panel.
        assert!(p.x >= 550);
    }

    #[test]
    fn left_fallback_when_right_is_short() {
        // panel right edge at work right (1600), so no room on the right.
        let p = place((1150, 100), 1.0, (0, 0, 1600, 900));
        // panel left 1180, right 1600. left avail 1172 >= 360 → left.
        assert_eq!(p.width, 360);
        assert_eq!(p.x, 1180 - 8 - 360); // 812
        assert!(p.x + p.width as i32 <= 1180); // no overlap
    }

    #[test]
    fn width_clamped_to_available_side_space() {
        // 1024 monitor, main centered: panel left 302, right 722.
        let p = place(((1024 - 480) / 2, 100), 1.0, (0, 0, 1024, 768));
        // right avail = 1024 - 722 - 8 = 294 (< 360). Preferred right.
        assert_eq!(p.x, 722 + 8); // 730
        assert_eq!(p.width, 294);
        assert_eq!(p.x + p.width as i32, 1024); // clamped flush to work right
    }

    #[test]
    fn dpi_scale_applies_to_panel_and_preview() {
        let p = place((100, 100), 1.5, (0, 0, 1920, 1080));
        // 30*1.5=45 → panel left 145, right 775. 360*1.5=540, 540*1.5=810, gap 12.
        assert_eq!(p.x, 775 + 12);
        assert_eq!(p.width, 540);
        assert_eq!(p.height, 810);
        assert_eq!(p.y, 145);
    }

    #[test]
    fn height_clamped_to_work_area_bottom() {
        let p = place((100, 700), 1.0, (0, 0, 1920, 800));
        // panel top 730; only 70px to work bottom.
        assert_eq!(p.y, 730);
        assert_eq!(p.height, 70);
    }

    #[test]
    fn top_aligned_and_never_overlapping() {
        let p = place((100, 100), 1.0, (0, 0, 1920, 1080));
        // Panel top is main_y + 30 = 130.
        assert_eq!(p.y, 130);
        // Panel occupies x in [130, 550]; preview starts after 550 + gap.
        assert!(p.x >= 550);
    }
}

#[cfg(test)]
mod preview_generation_tests {
    use super::show_is_current;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn fetch_add_yields_strictly_increasing_tokens() {
        let gen = AtomicU64::new(0);
        let a = gen.fetch_add(1, Ordering::SeqCst) + 1;
        let b = gen.fetch_add(1, Ordering::SeqCst) + 1;
        assert_eq!((a, b), (1, 2));
    }

    #[test]
    fn later_generation_supersedes_earlier_show() {
        let gen = AtomicU64::new(0);
        let first = gen.fetch_add(1, Ordering::SeqCst) + 1;
        let second = gen.fetch_add(1, Ordering::SeqCst) + 1;
        // The first show is stale once the second intent lands.
        assert!(!show_is_current(second, first));
        // The newest intent is current; an unchanged token stays current.
        assert!(show_is_current(second, second));
        assert!(show_is_current(first, first));
    }
}
