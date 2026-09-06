mod db;

use tauri::{AppHandle, Manager, Emitter, WindowEvent};
use tauri::menu::{Menu, MenuItem};
use tauri::tray::{TrayIconBuilder, TrayIconEvent, MouseButton, MouseButtonState};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::sync::mpsc::channel;
use std::thread;
use std::time::Duration;
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use arboard::Clipboard;
use enigo::{Enigo, Settings, Keyboard, Direction, Key};
use uuid::Uuid;
use serde::{Serialize, Deserialize};
use image::codecs::png::{PngEncoder, CompressionType, FilterType};
use image::ImageEncoder;
use xxhash_rust::xxh3::xxh3_128;

pub use db::{ClipItem, Database};

const SETTINGS_FILE: &str = "settings.json";
const DEFAULT_SHORTCUT: &str = "Ctrl+Alt+Shift+.";
const MAX_HISTORY: usize = 999;

#[cfg(target_os = "windows")]
fn extract_ocr_text(image_path: &Path) -> Option<String> {
    use windows::core::HSTRING;
    use windows::Graphics::Imaging::BitmapDecoder;
    use windows::Media::Ocr::OcrEngine;
    use windows::Storage::FileAccessMode;
    use windows::Storage::StorageFile;

    let path_buf = image_path.to_path_buf();
    let path_str = path_buf.to_string_lossy().replace('/', "\\");
    let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(path_str.as_str())).ok()?.get().ok()?;
    let stream = file.OpenAsync(FileAccessMode::Read).ok()?.get().ok()?;
    let decoder = BitmapDecoder::CreateAsync(&stream).ok()?.get().ok()?;
    let bitmap = decoder.GetSoftwareBitmapAsync().ok()?.get().ok()?;
    let engine = OcrEngine::TryCreateFromUserProfileLanguages().ok()?;
    let result = engine.RecognizeAsync(&bitmap).ok()?.get().ok()?;
    let text = result.Text().ok()?.to_string();
    let trimmed = text.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

#[cfg(not(target_os = "windows"))]
fn extract_ocr_text(_image_path: &Path) -> Option<String> {
    None
}

/// P4: Fast PNG encoding with Fast compression and NoFilter
fn save_image_fast_png(path: &Path, bytes: &[u8], width: u32, height: u32) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    let encoder = PngEncoder::new_with_quality(&mut writer, CompressionType::Fast, FilterType::NoFilter);
    encoder.write_image(bytes, width, height, image::ExtendedColorType::Rgba8)?;
    Ok(())
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppSettings {
    pub shortcut: String,
}

enum ClipboardEvent {
    Text(String),
    Image {
        width: usize,
        height: usize,
        bytes: Vec<u8>,
    },
}

struct AppState {
    is_monitoring: AtomicBool,
    db: Arc<Database>,
    data_dir: PathBuf,
    settings: RwLock<AppSettings>,
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
fn get_history(state: tauri::State<AppState>, favorites_only: Option<bool>) -> Result<Vec<ClipItem>, String> {
    state.db.get_history(MAX_HISTORY, favorites_only.unwrap_or(false)).map_err(|e| e.to_string())
}

#[tauri::command]
fn search_clips(state: tauri::State<AppState>, query: String, favorites_only: Option<bool>) -> Result<Vec<ClipItem>, String> {
    state.db.search_clips(&query, favorites_only.unwrap_or(false), MAX_HISTORY).map_err(|e| e.to_string())
}

#[tauri::command]
fn toggle_favorite(state: tauri::State<AppState>, id: String) -> Result<bool, String> {
    state.db.toggle_favorite(&id).map_err(|e| e.to_string())
}

#[tauri::command]
fn paste_item(app: AppHandle, state: tauri::State<AppState>, text: String, id: Option<String>) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.hide();
    }

    let mut is_image = false;
    let mut image_file_path: Option<PathBuf> = None;
    let mut paste_text = text;

    if let Some(ref item_id) = id {
        if let Ok(Some(clip)) = state.db.get_full_clip(item_id) {
            if clip.clip_type == "image" {
                if let Some(ref rel_or_abs) = clip.image_path {
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
            } else {
                paste_text = clip.text;
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
        clipboard.set_text(&paste_text).map_err(|e| e.to_string())?;

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

#[tauri::command]
fn copy_text(state: tauri::State<AppState>, text: String, id: Option<String>) -> Result<(), String> {
    let mut clipboard = Clipboard::new().map_err(|e| e.to_string())?;
    if let Some(id_str) = id {
        if let Ok(Some(clip)) = state.db.get_full_clip(&id_str) {
            clipboard.set_text(&clip.text).map_err(|e| e.to_string())?;
            return Ok(());
        }
    }
    clipboard.set_text(&text).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn delete_clip(state: tauri::State<AppState>, id: String) -> Result<bool, String> {
    let img_path = state.db.delete_clip(&id).map_err(|e| e.to_string())?;
    if let Some(p) = img_path {
        let path = Path::new(&p);
        let full = if path.is_absolute() {
            path.to_path_buf()
        } else {
            state.data_dir.join(path)
        };
        let _ = fs::remove_file(full);
    }
    Ok(true)
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
            match event {
                WindowEvent::Focused(focused) => {
                    // If window loses focus and is visible, hide it
                    if !focused && window.is_visible().unwrap_or(false) {
                        let _ = window.hide();
                        let state = window.state::<AppState>();
                        state.is_monitoring.store(true, Ordering::Relaxed);
                    }
                }
                WindowEvent::CloseRequested { api, .. } => {
                    api.prevent_close();
                    let _ = window.hide();
                    let state = window.state::<AppState>();
                    state.is_monitoring.store(true, Ordering::Relaxed);
                }
                _ => {}
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

            let db = Arc::new(Database::init(&data_dir).expect("failed to initialize sqlite database"));
            let settings = load_settings(&data_dir);

            let state = AppState {
                is_monitoring: AtomicBool::new(true),
                db: db.clone(),
                data_dir: data_dir.clone(),
                settings: RwLock::new(settings.clone()),
            };
            app.manage(state);

            // P1: Background reconciliation of orphaned images at startup
            let db_reconcile = db.clone();
            thread::spawn(move || {
                db_reconcile.reconcile_orphaned_images();
            });

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

            // Channel for decoupling clipboard monitoring from DB/OCR/Image writes
            let (tx, rx) = channel::<ClipboardEvent>();

            // Dedicated worker thread for DB writes, PNG encoding, and OCR
            let db_worker = db.clone();
            let app_worker = app.handle().clone();
            let data_dir_worker = data_dir.clone();

            thread::spawn(move || {
                while let Ok(event) = rx.recv() {
                    match event {
                        ClipboardEvent::Text(text) => {
                            let hash = format!("{:032x}", xxh3_128(text.as_bytes()));
                            if let Ok(item) = db_worker.save_or_bump_text(text, &hash) {
                                if let Ok(deleted_images) = db_worker.prune_history(MAX_HISTORY) {
                                    for p in deleted_images {
                                        let path = Path::new(&p);
                                        let full = if path.is_absolute() { path.to_path_buf() } else { data_dir_worker.join(path) };
                                        let _ = fs::remove_file(full);
                                    }
                                }

                                // Emit lean preview to UI
                                let mut preview_item = item;
                                if preview_item.text.chars().count() > 1000 {
                                    preview_item.text = preview_item.text.chars().take(1000).collect();
                                }
                                let _ = app_worker.emit("clipboard-new", &preview_item);
                            }
                        }
                        ClipboardEvent::Image { width, height, bytes } => {
                            let hash = format!("{:032x}", xxh3_128(&bytes));
                            if let Ok(Some(existing_item)) = db_worker.find_image_by_hash(&hash) {
                                // Image duplicate: bump to top
                                let _ = db_worker.bump_clip_timestamp(&existing_item.id);
                                let _ = app_worker.emit("clipboard-new", &existing_item);
                            } else {
                                let uuid_str = Uuid::new_v4().to_string();
                                let img_filename = format!("{}.png", uuid_str);
                                let images_dir = data_dir_worker.join("images");
                                if !images_dir.exists() {
                                    let _ = fs::create_dir_all(&images_dir);
                                }
                                let full_img_path = images_dir.join(&img_filename);
                                let full_img_path_str = full_img_path.to_string_lossy().replace('\\', "/");

                                if save_image_fast_png(&full_img_path, &bytes, width as u32, height as u32).is_ok() {
                                    let ocr_result = extract_ocr_text(&full_img_path);
                                    let item = ClipItem {
                                        id: uuid_str,
                                        text: format!("[Image {}x{}]", width, height),
                                        is_favorite: false,
                                        clip_type: "image".to_string(),
                                        image_path: Some(full_img_path_str),
                                        image_width: Some(width as u32),
                                        image_height: Some(height as u32),
                                        ocr_text: ocr_result,
                                        full_text_len: 0,
                                    };

                                    if db_worker.insert_image_clip(&item, &hash).is_ok() {
                                        if let Ok(deleted_images) = db_worker.prune_history(MAX_HISTORY) {
                                            for p in deleted_images {
                                                let path = Path::new(&p);
                                                let full = if path.is_absolute() { path.to_path_buf() } else { data_dir_worker.join(path) };
                                                let _ = fs::remove_file(full);
                                            }
                                        }

                                        let _ = app_worker.emit("clipboard-new", &item);
                                    }
                                }
                            }
                        }
                    }
                }
            });

            // Fast clipboard watcher thread: checks & hashes in <0.2ms, dispatches to mpsc channel
            let app_watcher = app.handle().clone();
            thread::spawn(move || {
                let mut last_text_hash: u128 = 0;
                let mut last_image_hash: u128 = 0;
                let mut clipboard_opt: Option<Clipboard> = Clipboard::new().ok();

                loop {
                    let state = app_watcher.state::<AppState>();
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
                                        let hash = xxh3_128(text.as_bytes());
                                        if hash != last_text_hash {
                                            last_text_hash = hash;
                                            last_image_hash = 0;
                                            let _ = tx.send(ClipboardEvent::Text(text));
                                        }
                                    }
                                }
                                Err(arboard::Error::ContentNotAvailable) => {}
                                Err(_) => {}
                            }

                            if !text_found {
                                match clipboard.get_image() {
                                    Ok(img) => {
                                        let hash = xxh3_128(&img.bytes);
                                        if hash != last_image_hash {
                                            last_image_hash = hash;
                                            last_text_hash = 0;
                                            let _ = tx.send(ClipboardEvent::Image {
                                                width: img.width,
                                                height: img.height,
                                                bytes: img.bytes.into_owned(),
                                            });
                                        }
                                    }
                                    Err(arboard::Error::ContentNotAvailable) => {}
                                    Err(_) => {}
                                }
                            }
                        }
                    }
                    thread::sleep(Duration::from_millis(500));
                }
            });
            
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            paste_item,
            set_monitoring,
            get_history,
            search_clips,
            hide_app,
            toggle_favorite,
            get_settings,
            set_shortcut,
            copy_text,
            delete_clip
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
