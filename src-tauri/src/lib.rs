use tauri::{AppHandle, Manager, Emitter, WindowEvent};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::thread;
use std::time::Duration;
use std::fs;
use std::path::{Path, PathBuf};
use arboard::Clipboard;
use enigo::{Enigo, Settings, Keyboard, Direction, Key};

const HISTORY_FILE: &str = "clipboard_history.json";
const MAX_HISTORY: usize = 999;

struct AppState {
    is_monitoring: AtomicBool,
    history: Mutex<Vec<String>>,
    data_dir: PathBuf,
}

fn load_history(data_dir: &Path) -> Vec<String> {
    let history_path = data_dir.join(HISTORY_FILE);
    if history_path.exists() {
        if let Ok(content) = fs::read_to_string(history_path) {
            if let Ok(history) = serde_json::from_str::<Vec<String>>(&content) {
                return history;
            }
        }
    }
    Vec::new()
}

fn save_history(data_dir: &Path, history: &[String]) {
    let history_path = data_dir.join(HISTORY_FILE);
    if let Ok(content) = serde_json::to_string(history) {
        let _ = fs::write(history_path, content);
    }
}

#[tauri::command]
fn set_monitoring(state: tauri::State<AppState>, monitoring: bool) {
    state.is_monitoring.store(monitoring, Ordering::Relaxed);
}

#[tauri::command]
fn get_history(state: tauri::State<AppState>) -> Vec<String> {
    let history = state.history.lock().unwrap();
    history.clone()
}

#[tauri::command]
fn paste_item(app: AppHandle, state: tauri::State<AppState>, text: String) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }

    let mut clipboard = Clipboard::new().map_err(|e| e.to_string())?;
    let backup_text = clipboard.get_text().ok(); 
    clipboard.set_text(&text).map_err(|e| e.to_string())?;

    thread::sleep(Duration::from_millis(100));
    
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
    let _ = enigo.key(Key::Control, Direction::Press);
    let _ = enigo.key(Key::Unicode('v'), Direction::Click);
    let _ = enigo.key(Key::Control, Direction::Release);

    thread::sleep(Duration::from_millis(200)); 
    if let Some(backup) = backup_text {
        let _ = clipboard.set_text(backup);
    }
    
    state.is_monitoring.store(true, Ordering::Relaxed);
    Ok(())
}

#[tauri::command]
fn hide_app(app: AppHandle, state: tauri::State<AppState>) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }
    state.is_monitoring.store(true, Ordering::Relaxed);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_shortcut("Ctrl+Alt+Shift+.")
                .expect("Failed to register shortcut")
                .with_handler(|app, _shortcut, event| {
                    if event.state() == tauri_plugin_global_shortcut::ShortcutState::Pressed {
                        if let Some(window) = app.get_webview_window("main") {
                            let state = app.state::<AppState>();
                            if window.is_visible().unwrap_or(false) {
                                let _ = window.emit("shortcut-cycle-next", ());
                            } else {
                                let _ = window.center();
                                let _ = window.show();
                                let _ = window.set_focus();
                                state.is_monitoring.store(false, Ordering::Relaxed);
                            }
                        }
                    }
                })
                .build()
        )
        .on_window_event(|window, event| {
            if let WindowEvent::Focused(focused) = event {
                // If window loses focus and is visible, hide it
                if !focused && window.is_visible().unwrap_or(false) {
                     let _ = window.hide();
                     let state = window.state::<AppState>();
                     state.is_monitoring.store(true, Ordering::Relaxed);
                }
            }
        })
        .setup(|app| {
            let data_dir = app.path().app_data_dir().expect("failed to get app data dir");
            
            // Ensure data dir exists
            if !data_dir.exists() {
                let _ = fs::create_dir_all(&data_dir);
            }

            let history = load_history(&data_dir);

            let state = AppState {
                is_monitoring: AtomicBool::new(true),
                history: Mutex::new(history),
                data_dir: data_dir.clone(),
            };
            app.manage(state);

            let app_handle_clone = app.handle().clone();
            thread::spawn(move || {
                let mut last_text = String::new();
                loop {
                    let state = app_handle_clone.state::<AppState>();
                    if state.is_monitoring.load(Ordering::Relaxed) {
                        if let Ok(mut clipboard) = Clipboard::new() {
                            if let Ok(text) = clipboard.get_text() {
                                let trimmed = text.trim();
                                if !trimmed.is_empty() && text != last_text {
                                    last_text = text.clone();
                                    
                                    // Update shared state and persist
                                    if let Ok(mut history) = state.history.lock() {
                                        // Check for duplicate at top
                                        if history.first() != Some(&text) {
                                            history.insert(0, text.clone());
                                            if history.len() > MAX_HISTORY {
                                                history.truncate(MAX_HISTORY);
                                            }
                                            save_history(&state.data_dir, &history);
                                            let _ = app_handle_clone.emit("clipboard-new", &text);
                                        }
                                    }
                                }
                            }
                        }
                    }
                    thread::sleep(Duration::from_millis(500));
                }
            });
            
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![paste_item, set_monitoring, get_history, hide_app])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
