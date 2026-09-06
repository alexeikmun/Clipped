use tauri::{AppHandle, Manager, Emitter, WindowEvent};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{TrayIconBuilder, TrayIconEvent, MouseButton, MouseButtonState};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::RwLock;
use std::thread;
use std::time::Duration;
use std::fs;
use std::path::{Path, PathBuf};
use std::hash::{Hash, Hasher};
use std::collections::hash_map::DefaultHasher;
use arboard::Clipboard;
use enigo::{Enigo, Settings, Keyboard, Direction, Key};
use uuid::Uuid;
use serde::{Serialize, Deserialize};

const HISTORY_FILE: &str = "clipboard_history.json";
const SETTINGS_FILE: &str = "settings.json";
const DEFAULT_SHORTCUT: &str = "Ctrl+Alt+Shift+.";
const MAX_HISTORY: usize = 999;

fn default_clip_type() -> String {
    "text".to_string()
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ClipItem {
    pub id: String,
    pub text: String,
    pub is_favorite: bool,
    #[serde(default = "default_clip_type")]
    pub clip_type: String,
    #[serde(default)]
    pub image_path: Option<String>,
    #[serde(default)]
    pub image_width: Option<u32>,
    #[serde(default)]
    pub image_height: Option<u32>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppSettings {
    pub shortcut: String,
}

struct AppState {
    is_monitoring: AtomicBool,
    history: RwLock<Vec<ClipItem>>,
    data_dir: PathBuf,
    settings: RwLock<AppSettings>,
}

fn load_history(data_dir: &Path) -> Vec<ClipItem> {
    let history_path = data_dir.join(HISTORY_FILE);
    let Ok(content) = fs::read_to_string(&history_path) else {
        return Vec::new();
    };

    // Try to parse as new format
    if let Ok(mut history) = serde_json::from_str::<Vec<ClipItem>>(&content) {
        for item in &mut history {
            if item.clip_type == "image" {
                if let Some(ref rel_or_abs) = item.image_path {
                    let path = Path::new(rel_or_abs);
                    if !path.is_absolute() {
                        item.image_path = Some(data_dir.join(path).to_string_lossy().replace('\\', "/"));
                    }
                }
            }
        }
        return history;
    }
    // Fallback: try to parse as old format (Vec<String>) and migrate
    if let Ok(old_history) = serde_json::from_str::<Vec<String>>(&content) {
        return old_history.into_iter().map(|text| ClipItem {
            id: Uuid::new_v4().to_string(),
            text,
            is_favorite: false,
            clip_type: "text".to_string(),
            image_path: None,
            image_width: None,
            image_height: None,
        }).collect();
    }
    Vec::new()
}

fn save_history(data_dir: &Path, history: &[ClipItem]) {
    let history_path = data_dir.join(HISTORY_FILE);
    if let Ok(content) = serde_json::to_string(history) {
        let _ = fs::write(history_path, content);
    }
}

fn load_settings(data_dir: &Path) -> AppSettings {
    let settings_path = data_dir.join(SETTINGS_FILE);
    if settings_path.exists() {
        if let Ok(content) = fs::read_to_string(settings_path) {
            if let Ok(settings) = serde_json::from_str::<AppSettings>(&content) {
                if !settings.shortcut.trim().is_empty() {
                    return settings;
                }
            }
        }
    }
    AppSettings {
        shortcut: DEFAULT_SHORTCUT.to_string(),
    }
}

fn save_settings(data_dir: &Path, settings: &AppSettings) {
    let settings_path = data_dir.join(SETTINGS_FILE);
    if let Ok(content) = serde_json::to_string(settings) {
        let _ = fs::write(settings_path, content);
    }
}

#[tauri::command]
fn set_monitoring(state: tauri::State<AppState>, monitoring: bool) {
    state.is_monitoring.store(monitoring, Ordering::Relaxed);
}

#[tauri::command]
fn get_history(state: tauri::State<AppState>) -> Vec<ClipItem> {
    let history = state.history.read().unwrap();
    history.clone()
}

#[tauri::command]
fn toggle_favorite(state: tauri::State<AppState>, id: String) -> Result<Vec<ClipItem>, String> {
    let mut history = state.history.write().unwrap();
    if let Some(item) = history.iter_mut().find(|item| item.id == id) {
        item.is_favorite = !item.is_favorite;
        save_history(&state.data_dir, &history);
        Ok(history.clone())
    } else {
        Err("Item not found".to_string())
    }
}

#[tauri::command]
fn paste_item(app: AppHandle, state: tauri::State<AppState>, text: String, id: Option<String>) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }

    let mut is_image = false;
    let mut image_file_path: Option<PathBuf> = None;

    if let Some(ref item_id) = id {
        if let Ok(history) = state.history.read() {
            if let Some(item) = history.iter().find(|i| &i.id == item_id) {
                if item.clip_type == "image" {
                    if let Some(ref rel_or_abs) = item.image_path {
                        let path = Path::new(rel_or_abs);
                        let full_path = if path.is_absolute() {
                            path.to_path_buf()
                        } else {
                            state.data_dir.join(path)
                        };
                        if full_path.exists() {
                            is_image = true;
                            image_file_path = Some(full_path);
                        }
                    }
                }
            }
        }
    }

    let mut clipboard = Clipboard::new().map_err(|e| e.to_string())?;

    if is_image {
        if let Some(path) = image_file_path {
            let img = image::open(&path).map_err(|e| e.to_string())?.to_rgba8();
            let (w, h) = img.dimensions();
            let img_data = arboard::ImageData {
                width: w as usize,
                height: h as usize,
                bytes: img.into_raw().into(),
            };
            clipboard.set_image(img_data).map_err(|e| e.to_string())?;
        }
    } else {
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
        return Ok(());
    }

    thread::sleep(Duration::from_millis(100));
    
    let mut enigo = Enigo::new(&Settings::default()).map_err(|e| e.to_string())?;
    let _ = enigo.key(Key::Control, Direction::Press);
    let _ = enigo.key(Key::Unicode('v'), Direction::Click);
    let _ = enigo.key(Key::Control, Direction::Release);

    thread::sleep(Duration::from_millis(200)); 

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

#[tauri::command]
fn get_settings(state: tauri::State<AppState>) -> AppSettings {
    let settings = state.settings.read().unwrap();
    settings.clone()
}

#[tauri::command]
fn set_shortcut(app: AppHandle, state: tauri::State<AppState>, shortcut: String) -> Result<String, String> {
    let trimmed = shortcut.trim();
    if trimmed.is_empty() {
        return Err("Shortcut cannot be empty".to_string());
    }

    let parsed = trimmed.parse::<Shortcut>().map_err(|e| e.to_string())?;
    let mut settings = state.settings.write().unwrap();
    let current = settings.shortcut.clone();
    if current == trimmed {
        return Ok(current);
    }

    let manager = app.global_shortcut();
    let _ = manager.unregister_all();
    if let Err(error) = manager.register(parsed) {
        if let Ok(previous) = current.parse::<Shortcut>() {
            let _ = manager.register(previous);
        }
        return Err(error.to_string());
    }

    settings.shortcut = trimmed.to_string();
    save_settings(&state.data_dir, &settings);
    Ok(settings.shortcut.clone())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_clipboard_manager::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
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
                                let _ = window.emit("modal-opened", ());
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
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec![]),
        ))
        .setup(|app| {
            #[cfg(desktop)]
            {
                use tauri_plugin_autostart::ManagerExt;
                let autostart_manager = app.autolaunch();
                if !autostart_manager.is_enabled().unwrap_or(false) {
                    let _ = autostart_manager.enable();

                }
            }

            let data_dir = app.path().app_data_dir().expect("failed to get app data dir");
            
            // Ensure data dir exists
            if !data_dir.exists() {
                let _ = fs::create_dir_all(&data_dir);
            }

            let history = load_history(&data_dir);
            let settings = load_settings(&data_dir);

            let state = AppState {
                is_monitoring: AtomicBool::new(true),
                history: RwLock::new(history),
                data_dir: data_dir.clone(),
                settings: RwLock::new(settings.clone()),
            };
            app.manage(state);

            let manager = app.global_shortcut();
            if let Ok(parsed) = settings.shortcut.parse::<Shortcut>() {
                if let Err(err) = manager.register(parsed) {
                    eprintln!("Warning: could not register global shortcut '{}': {}", settings.shortcut, err);
                }
            } else if let Ok(default_parsed) = DEFAULT_SHORTCUT.parse::<Shortcut>() {
                let _ = manager.register(default_parsed);
            }

            let icon = app.default_window_icon().cloned();
            if let Some(icon) = icon {
                let tray_menu = Menu::with_items(
                    app,
                    &[&MenuItem::with_id(app, "tray-exit", "Exit", true, None::<&str>)?],
                )?;
                let tray_icon = TrayIconBuilder::new()
                    .icon(icon)
                    .menu(&tray_menu)
                    .show_menu_on_left_click(false)
                    .on_menu_event(|app, event| {
                        if event.id() == "tray-exit" {
                            app.exit(0);
                        }
                    })
                    .on_tray_icon_event(|tray, event| {
                        if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Down, .. } = event {
                            let app = tray.app_handle();
                            if let Some(window) = app.get_webview_window("main") {
                                let _ = window.center();
                                let _ = window.show();
                                let _ = window.set_focus();
                                let _ = window.emit("open-settings", ());
                                let state = app.state::<AppState>();
                                state.is_monitoring.store(false, Ordering::Relaxed);
                            }
                        }
                    })
                    .build(app);
                if let Ok(tray_icon) = tray_icon {
                    app.manage(tray_icon);
                }
            }

            let app_handle_clone = app.handle().clone();
            thread::spawn(move || {
                let mut last_text = String::new();
                let mut last_image_hash: u64 = 0;
                let mut clipboard_opt: Option<Clipboard> = Clipboard::new().ok();
                loop {
                    let state = app_handle_clone.state::<AppState>();
                    if state.is_monitoring.load(Ordering::Relaxed) {
                        if clipboard_opt.is_none() {
                            clipboard_opt = Clipboard::new().ok();
                        }
                        if let Some(ref mut clipboard) = clipboard_opt {
                            let mut text_found = false;
                            match clipboard.get_text() {
                                Ok(text) => {
                                    let trimmed = text.trim();
                                    if !trimmed.is_empty() {
                                        text_found = true;
                                        if text != last_text {
                                            last_text = text.clone();
                                            last_image_hash = 0;

                                            // Update shared state and persist
                                            if let Ok(mut history) = state.history.write() {
                                                let is_duplicate = history.first().map(|item| item.text == text).unwrap_or(false);

                                                if !is_duplicate {
                                                    let new_item = ClipItem {
                                                        id: Uuid::new_v4().to_string(),
                                                        text: text.clone(),
                                                        is_favorite: false,
                                                        clip_type: "text".to_string(),
                                                        image_path: None,
                                                        image_width: None,
                                                        image_height: None,
                                                    };

                                                    history.insert(0, new_item.clone());

                                                    // Smart truncation: remove non-favorites from the bottom
                                                    while history.len() > MAX_HISTORY {
                                                        let last_non_fav_index = history.iter().rposition(|item| !item.is_favorite);

                                                        if let Some(index) = last_non_fav_index {
                                                            let removed = history.remove(index);
                                                            if let Some(ref rel_or_abs) = removed.image_path {
                                                                let p = Path::new(rel_or_abs);
                                                                let full_p = if p.is_absolute() { p.to_path_buf() } else { state.data_dir.join(p) };
                                                                let _ = fs::remove_file(full_p);
                                                            }
                                                        } else {
                                                            break;
                                                        }
                                                    }

                                                    save_history(&state.data_dir, &history);
                                                    let _ = app_handle_clone.emit("clipboard-new", &new_item);
                                                }
                                            }
                                        }
                                    }
                                }
                                Err(arboard::Error::ContentNotAvailable) => {
                                    // Clipboard has image or non-text, expected
                                }
                                Err(_) => {
                                    // Transient error
                                }
                            }

                            if !text_found {
                                match clipboard.get_image() {
                                    Ok(img) => {
                                        let mut hasher = DefaultHasher::new();
                                        img.bytes.hash(&mut hasher);
                                        let img_hash = hasher.finish();

                                        if img_hash != last_image_hash {
                                            last_image_hash = img_hash;
                                            last_text.clear();

                                            let uuid_str = Uuid::new_v4().to_string();
                                            let img_filename = format!("{}.png", uuid_str);
                                            let images_dir = state.data_dir.join("images");
                                            if !images_dir.exists() {
                                                let _ = fs::create_dir_all(&images_dir);
                                            }
                                            let full_img_path = images_dir.join(&img_filename);
                                            let full_img_path_str = full_img_path.to_string_lossy().replace('\\', "/");

                                            if image::save_buffer_with_format(
                                                &full_img_path,
                                                &img.bytes,
                                                img.width as u32,
                                                img.height as u32,
                                                image::ExtendedColorType::Rgba8,
                                                image::ImageFormat::Png,
                                            ).is_ok() {
                                                let new_item = ClipItem {
                                                    id: uuid_str,
                                                    text: format!("[Image {}x{}]", img.width, img.height),
                                                    is_favorite: false,
                                                    clip_type: "image".to_string(),
                                                    image_path: Some(full_img_path_str),
                                                    image_width: Some(img.width as u32),
                                                    image_height: Some(img.height as u32),
                                                };

                                                if let Ok(mut history) = state.history.write() {
                                                    history.insert(0, new_item.clone());

                                                    // Smart truncation: remove non-favorites from the bottom
                                                    while history.len() > MAX_HISTORY {
                                                        let last_non_fav_index = history.iter().rposition(|item| !item.is_favorite);

                                                        if let Some(index) = last_non_fav_index {
                                                            let removed = history.remove(index);
                                                            if let Some(ref rel_or_abs) = removed.image_path {
                                                                let p = Path::new(rel_or_abs);
                                                                let full_p = if p.is_absolute() { p.to_path_buf() } else { state.data_dir.join(p) };
                                                                let _ = fs::remove_file(full_p);
                                                            }
                                                        } else {
                                                            break;
                                                        }
                                                    }

                                                    save_history(&state.data_dir, &history);
                                                    let _ = app_handle_clone.emit("clipboard-new", &new_item);
                                                }
                                            }
                                        }
                                    }
                                    Err(arboard::Error::ContentNotAvailable) => {
                                        // No image on clipboard
                                    }
                                    Err(_) => {
                                        // Transient error
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
        .invoke_handler(tauri::generate_handler![paste_item, set_monitoring, get_history, hide_app, toggle_favorite, get_settings, set_shortcut])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
