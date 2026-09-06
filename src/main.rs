#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

slint::include_modules!();

mod db;

use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::str::FromStr;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use arboard::Clipboard;
use db::{ClipItem, Database};
use global_hotkey::hotkey::{Code, HotKey, Modifiers};
use global_hotkey::{GlobalHotKeyEvent, GlobalHotKeyManager, HotKeyState};
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::ImageEncoder;
use muda::{Menu, MenuItem};
use serde::{Deserialize, Serialize};
use slint::{ComponentHandle, ModelRc, VecModel};
use tray_icon::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent};
use uuid::Uuid;
use xxhash_rust::xxh3::xxh3_128;

const SETTINGS_FILE: &str = "settings.json";
const DEFAULT_SHORTCUT: &str = "Ctrl+Alt+Shift+.";
const MAX_HISTORY: usize = 999;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct AppSettings {
    pub shortcut: String,
}

fn load_settings(data_dir: &Path) -> AppSettings {
    let settings_path = data_dir.join(SETTINGS_FILE);
    if settings_path.exists() {
        if let Ok(content) = fs::read_to_string(&settings_path) {
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
    if let Ok(content) = serde_json::to_string_pretty(settings) {
        let _ = fs::write(settings_path, content);
    }
}

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

fn save_image_fast_png(
    path: &Path,
    bytes: &[u8],
    width: u32,
    height: u32,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let file = File::create(path)?;
    let mut writer = BufWriter::new(file);
    let encoder = PngEncoder::new_with_quality(&mut writer, CompressionType::Fast, FilterType::NoFilter);
    encoder.write_image(bytes, width, height, image::ExtendedColorType::Rgba8)?;
    Ok(())
}

fn clip_to_data(item: &ClipItem) -> ClipData {
    let dimension_text = if item.clip_type == "image" {
        format!("{}x{}", item.image_width.unwrap_or(0), item.image_height.unwrap_or(0))
    } else {
        String::new()
    };

    ClipData {
        id: item.id.as_str().into(),
        text: item.text.as_str().into(),
        is_favorite: item.is_favorite,
        clip_type: item.clip_type.as_str().into(),
        image_path: item.image_path.as_deref().unwrap_or_default().into(),
        image_width: item.image_width.unwrap_or(0) as i32,
        image_height: item.image_height.unwrap_or(0) as i32,
        ocr_text: item.ocr_text.as_deref().unwrap_or_default().into(),
        dimension_text: dimension_text.as_str().into(),
    }
}

enum ClipboardEvent {
    Text(String),
    Image {
        width: usize,
        height: usize,
        bytes: Vec<u8>,
    },
}

#[cfg(target_os = "windows")]
static CACHED_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);
#[cfg(target_os = "windows")]
static PREVIOUS_FOREGROUND_HWND: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

#[cfg(target_os = "windows")]
fn center_and_focus_window(window: &MainWindow) {
    use windows::core::HSTRING;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GetSystemMetrics, GetWindowLongW, SetForegroundWindow,
        SetWindowLongW, SetWindowPos, GWL_EXSTYLE, HWND_TOPMOST, SM_CXSCREEN, SM_CYSCREEN,
        SWP_SHOWWINDOW, WS_EX_TOOLWINDOW,
    };
    use windows::Win32::UI::Input::KeyboardAndMouse::SetFocus;

    let screen_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let screen_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    let win_w = 580;
    let win_h = 380;
    let x = (screen_w - win_w) / 2;
    let y = (screen_h - win_h) / 2;

    window.window().set_position(slint::PhysicalPosition::new(x, y));
    let _ = window.show();

    unsafe {
        let cached = CACHED_HWND.load(Ordering::Relaxed);
        let hwnd = if cached != 0 {
            HWND(cached as _)
        } else {
            let title = HSTRING::from("Clipped");
            FindWindowW(None, &title).unwrap_or(HWND(std::ptr::null_mut()))
        };

        if !hwnd.0.is_null() {
            CACHED_HWND.store(hwnd.0 as isize, Ordering::Relaxed);
            let ex_style = GetWindowLongW(hwnd, GWL_EXSTYLE);
            SetWindowLongW(hwnd, GWL_EXSTYLE, ex_style | WS_EX_TOOLWINDOW.0 as i32);

            use windows::Win32::Graphics::Dwm::{
                DwmExtendFrameIntoClientArea, DwmSetWindowAttribute,
                DWMWA_USE_IMMERSIVE_DARK_MODE, DWMWA_WINDOW_CORNER_PREFERENCE,
                DWMWCP_ROUND,
            };
            use windows::Win32::UI::Controls::MARGINS;

            let margins = MARGINS {
                cxLeftWidth: -1,
                cxRightWidth: -1,
                cyTopHeight: -1,
                cyBottomHeight: -1,
            };
            let _ = DwmExtendFrameIntoClientArea(hwnd, &margins);

            let dark_mode = windows::Win32::Foundation::TRUE;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                &dark_mode as *const _ as _,
                std::mem::size_of_val(&dark_mode) as u32,
            );

            let corner = DWMWCP_ROUND;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &corner as *const _ as _,
                std::mem::size_of_val(&corner) as u32,
            );

            let _ = SetWindowPos(hwnd, HWND_TOPMOST, x, y, win_w, win_h, SWP_SHOWWINDOW);
            let _ = SetForegroundWindow(hwnd);
            let _ = SetFocus(hwnd);
        }
    }
}

#[cfg(not(target_os = "windows"))]
fn center_and_focus_window(window: &MainWindow) {
    let _ = window.show();
}

#[cfg(target_os = "windows")]
fn is_window_foreground() -> bool {
    use windows::core::HSTRING;
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, GetForegroundWindow};

    unsafe {
        let fg = GetForegroundWindow();
        if fg.0.is_null() {
            return true;
        }
        let cached = CACHED_HWND.load(Ordering::Relaxed);
        if cached != 0 {
            return fg.0 as isize == cached;
        }
        let title = HSTRING::from("Clipped");
        if let Ok(hwnd) = FindWindowW(None, &title) {
            CACHED_HWND.store(hwnd.0 as isize, Ordering::Relaxed);
            return fg == hwnd;
        }
    }
    true
}

#[cfg(not(target_os = "windows"))]
fn is_window_foreground() -> bool {
    true
}

fn perform_paste(item: &ClipItem, data_dir: &Path, is_monitoring: &AtomicBool) {
    let mut is_image = false;
    let mut image_file_path: Option<PathBuf> = None;
    let paste_text = item.text.clone();

    if item.clip_type == "image" {
        if let Some(ref rel_or_abs) = item.image_path {
            let path = Path::new(rel_or_abs);
            let full_path = if path.is_absolute() {
                path.to_path_buf()
            } else {
                data_dir.join(path)
            };
            if full_path.exists() {
                is_image = true;
                image_file_path = Some(full_path);
            }
        }
    }

    let mut clipboard = match Clipboard::new() {
        Ok(c) => c,
        Err(_) => {
            is_monitoring.store(true, Ordering::Relaxed);
            return;
        }
    };

    if is_image {
        if let Some(path) = image_file_path {
            if let Ok(img) = image::open(&path) {
                let rgba = img.to_rgba8();
                let (w, h) = rgba.dimensions();
                let img_data = arboard::ImageData {
                    width: w as usize,
                    height: h as usize,
                    bytes: rgba.into_raw().into(),
                };
                let _ = clipboard.set_image(img_data);
            }
        }
    } else {
        let _ = clipboard.set_text(paste_text);
    }

    #[cfg(target_os = "windows")]
    {
        use windows::Win32::Foundation::HWND;
        use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;
        let prev = PREVIOUS_FOREGROUND_HWND.load(Ordering::Relaxed);
        if prev != 0 {
            unsafe {
                let target = HWND(prev as _);
                let _ = SetForegroundWindow(target);
            }
        }
    }

    thread::sleep(Duration::from_millis(100));

    #[cfg(target_os = "windows")]
    {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            keybd_event, KEYEVENTF_KEYUP, VK_CONTROL, VK_MENU, VK_SHIFT, VK_V,
        };
        unsafe {
            // Release modifier keys that might have been held when opening via hotkey
            keybd_event(VK_MENU.0 as u8, 0, KEYEVENTF_KEYUP, 0);
            keybd_event(VK_SHIFT.0 as u8, 0, KEYEVENTF_KEYUP, 0);
            keybd_event(VK_CONTROL.0 as u8, 0, KEYEVENTF_KEYUP, 0);

            // Send Ctrl+V
            keybd_event(VK_CONTROL.0 as u8, 0, Default::default(), 0);
            keybd_event(VK_V.0 as u8, 0, Default::default(), 0);
            keybd_event(VK_V.0 as u8, 0, KEYEVENTF_KEYUP, 0);
            keybd_event(VK_CONTROL.0 as u8, 0, KEYEVENTF_KEYUP, 0);
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Ok(mut enigo) = enigo::Enigo::new(&enigo::Settings::default()) {
            use enigo::{Direction, Key, Keyboard};
            let _ = enigo.key(Key::Control, Direction::Press);
            let _ = enigo.key(Key::Unicode('v'), Direction::Click);
            let _ = enigo.key(Key::Control, Direction::Release);
        }
    }

    thread::sleep(Duration::from_millis(200));
    is_monitoring.store(true, Ordering::Relaxed);
}

fn refresh_active_image(window: &MainWindow, current_clips: &[ClipItem], index: usize, data_dir: &Path) {
    if index < current_clips.len() && current_clips[index].clip_type == "image" {
        if let Some(ref rel_or_abs) = current_clips[index].image_path {
            let path = Path::new(rel_or_abs);
            let full_path = if path.is_absolute() {
                path.to_path_buf()
            } else {
                data_dir.join(path)
            };
            if let Ok(img) = slint::Image::load_from_path(&full_path) {
                window.set_active_image(img);
                return;
            }
        }
    }
    window.set_active_image(slint::Image::default());
}

fn reload_clips(
    window: &MainWindow,
    db: &Database,
    data_dir: &Path,
    show_favorites: bool,
    search_query: &str,
    target_index: Option<i32>,
) -> Vec<ClipItem> {
    let items = if !search_query.trim().is_empty() {
        db.search_clips(search_query, show_favorites, MAX_HISTORY).unwrap_or_default()
    } else {
        db.get_history(MAX_HISTORY, show_favorites).unwrap_or_default()
    };

    let slint_items: Vec<ClipData> = items.iter().map(clip_to_data).collect();
    let model = Rc::new(VecModel::from(slint_items));
    window.set_clips(ModelRc::from(model));

    let len = items.len() as i32;
    let new_idx = if len == 0 {
        0
    } else if let Some(idx) = target_index {
        idx.clamp(0, len - 1)
    } else {
        0
    };
    window.set_selected_index(new_idx);
    refresh_active_image(window, &items, new_idx as usize, data_dir);

    items
}

fn main() {
    if std::env::var_os("SLINT_BACKEND").is_none() {
        std::env::set_var("SLINT_BACKEND", "winit-software");
    }

    #[cfg(target_os = "windows")]
    unsafe {
        use windows::Win32::System::Ole::OleInitialize;
        let _ = OleInitialize(None);
    }

    // 1. Locate AppData directory
    let data_dir = std::env::var("APPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."))
        .join("com.alexis.clipped-app");

    if !data_dir.exists() {
        let _ = fs::create_dir_all(&data_dir);
    }

    // 2. Initialize Database and Settings
    let db = Arc::new(Database::init(&data_dir).expect("failed to initialize sqlite database"));
    let settings = Arc::new(Mutex::new(load_settings(&data_dir)));

    // Background reconciliation of orphaned images
    let db_reconcile = db.clone();
    thread::spawn(move || {
        db_reconcile.reconcile_orphaned_images();
    });

    // 3. Initialize Slint Window
    let main_window = MainWindow::new().unwrap();
    let current_shortcut = settings.lock().unwrap().shortcut.clone();
    main_window.set_shortcut_current(current_shortcut.as_str().into());
    main_window.set_shortcut_input(current_shortcut.as_str().into());

    // 4. State tracking
    let is_monitoring = Arc::new(AtomicBool::new(true));
    let cached_clips = Arc::new(Mutex::new(Vec::<ClipItem>::new()));

    // Initial load
    let initial_clips = reload_clips(&main_window, &db, &data_dir, false, "", Some(0));
    *cached_clips.lock().unwrap() = initial_clips;

    let is_hidden_flag = std::env::args().any(|arg| arg == "--hidden");
    let show_on_startup = (cfg!(debug_assertions) || std::env::args().any(|arg| arg == "--show" || arg == "-s")) && !is_hidden_flag;
    if show_on_startup {
        is_monitoring.store(false, Ordering::Relaxed);
        main_window.set_is_search_visible(false);
        main_window.set_search_query("".into());
        main_window.set_search_enabled(false);
        center_and_focus_window(&main_window);
        main_window.invoke_focus_main();
    }

    // 5. Global HotKey Manager
    let hotkeys_manager = Arc::new(Mutex::new(GlobalHotKeyManager::new().unwrap()));
    let active_hotkey = Arc::new(Mutex::new(
        HotKey::from_str(&current_shortcut).unwrap_or_else(|_| {
            HotKey::new(Some(Modifiers::CONTROL | Modifiers::ALT | Modifiers::SHIFT), Code::Period)
        }),
    ));
    let _ = hotkeys_manager.lock().unwrap().register(*active_hotkey.lock().unwrap());

    let tray_menu = Menu::new();
    let exit_item = MenuItem::new("Exit", true, None);
    let exit_id = exit_item.id().clone();
    let _ = tray_menu.append(&exit_item);

    let tray_icon_img = tray_icon::Icon::from_path(Path::new("assets/icon.ico"), Some((32, 32)))
        .or_else(|_| {
            let icon_img = image::load_from_memory(include_bytes!("../assets/icon.png"))
                .unwrap()
                .resize(32, 32, image::imageops::FilterType::Lanczos3)
                .to_rgba8();
            let (w, h) = icon_img.dimensions();
            tray_icon::Icon::from_rgba(icon_img.into_raw(), w, h)
        })
        .ok();

    let mut tray_builder = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu))
        .with_tooltip("Clipped - Clipboard Manager");

    if let Some(icon) = tray_icon_img {
        tray_builder = tray_builder.with_icon(icon);
    }

    let _tray = match tray_builder.build() {
        Ok(t) => {
            println!("System tray icon registered successfully.");
            Some(t)
        }
        Err(e) => {
            eprintln!("Warning: failed to build system tray icon: {:?}", e);
            None
        }
    };

    // 7. Clipboard Monitor & Worker Threads
    let (tx, rx) = channel::<ClipboardEvent>();
    let db_worker = db.clone();
    let data_dir_worker = data_dir.clone();
    let window_weak = main_window.as_weak();
    let cached_clips_worker = cached_clips.clone();

    thread::spawn(move || {
        while let Ok(event) = rx.recv() {
            match event {
                ClipboardEvent::Text(text) => {
                    let hash = format!("{:032x}", xxh3_128(text.as_bytes()));
                    if let Ok(_) = db_worker.save_or_bump_text(text, &hash) {
                        if let Ok(deleted_images) = db_worker.prune_history(MAX_HISTORY) {
                            for p in deleted_images {
                                let path = Path::new(&p);
                                let full = if path.is_absolute() {
                                    path.to_path_buf()
                                } else {
                                    data_dir_worker.join(path)
                                };
                                let _ = fs::remove_file(full);
                            }
                        }

                        let db_c = db_worker.clone();
                        let dir_c = data_dir_worker.clone();
                        let cached_c = cached_clips_worker.clone();
                        let _ = window_weak.upgrade_in_event_loop(move |w| {
                            let show_fav = w.get_show_favorites();
                            let query = w.get_search_query();
                            let is_vis = w.window().is_visible();
                            let target_idx = if is_vis { Some(w.get_selected_index()) } else { Some(0) };
                            let updated = reload_clips(&w, &db_c, &dir_c, show_fav, query.as_str(), target_idx);
                            *cached_c.lock().unwrap() = updated;
                        });
                    }
                }
                ClipboardEvent::Image { width, height, bytes } => {
                    let hash = format!("{:032x}", xxh3_128(&bytes));
                    if let Ok(Some(existing_item)) = db_worker.find_image_by_hash(&hash) {
                        let _ = db_worker.bump_clip_timestamp(&existing_item.id);
                        let db_c = db_worker.clone();
                        let dir_c = data_dir_worker.clone();
                        let cached_c = cached_clips_worker.clone();
                        let _ = window_weak.upgrade_in_event_loop(move |w| {
                            let show_fav = w.get_show_favorites();
                            let query = w.get_search_query();
                            let is_vis = w.window().is_visible();
                            let target_idx = if is_vis { Some(w.get_selected_index()) } else { Some(0) };
                            let updated = reload_clips(&w, &db_c, &dir_c, show_fav, query.as_str(), target_idx);
                            *cached_c.lock().unwrap() = updated;
                        });
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
                                        let full = if path.is_absolute() {
                                            path.to_path_buf()
                                        } else {
                                            data_dir_worker.join(path)
                                        };
                                        let _ = fs::remove_file(full);
                                    }
                                }

                                let db_c = db_worker.clone();
                                let dir_c = data_dir_worker.clone();
                                let cached_c = cached_clips_worker.clone();
                                let _ = window_weak.upgrade_in_event_loop(move |w| {
                                    let show_fav = w.get_show_favorites();
                                    let query = w.get_search_query();
                                    let is_vis = w.window().is_visible();
                                    let target_idx = if is_vis { Some(w.get_selected_index()) } else { Some(0) };
                                    let updated = reload_clips(&w, &db_c, &dir_c, show_fav, query.as_str(), target_idx);
                                    *cached_c.lock().unwrap() = updated;
                                });
                            }
                        }
                    }
                }
            }
        }
    });

    // Fast clipboard poller thread
    let is_monitoring_clone = is_monitoring.clone();
    thread::spawn(move || {
        let mut last_text_hash: u128 = 0;
        let mut last_image_hash: u128 = 0;
        let mut clipboard_opt: Option<Clipboard> = Clipboard::new().ok();

        loop {
            if is_monitoring_clone.load(Ordering::Relaxed) {
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

    // 8. Wire Slint Callbacks
    {
        let cached_clips_c = cached_clips.clone();
        let data_dir_c = data_dir.clone();
        let window_weak = main_window.as_weak();
        main_window.on_select_index(move |idx| {
            if let Some(w) = window_weak.upgrade() {
                w.set_selected_index(idx);
                let clips = cached_clips_c.lock().unwrap();
                refresh_active_image(&w, &clips, idx as usize, &data_dir_c);
            }
        });
    }

    {
        let db_c = db.clone();
        let data_dir_c = data_dir.clone();
        let cached_clips_c = cached_clips.clone();
        let window_weak = main_window.as_weak();
        main_window.on_toggle_favorite(move |id| {
            if let Some(w) = window_weak.upgrade() {
                let _ = db_c.toggle_favorite(id.as_str());
                let show_fav = w.get_show_favorites();
                let query = w.get_search_query();
                let current_idx = w.get_selected_index();
                let updated = reload_clips(&w, &db_c, &data_dir_c, show_fav, query.as_str(), Some(current_idx));
                *cached_clips_c.lock().unwrap() = updated;
            }
        });
    }

    {
        let db_c = db.clone();
        let data_dir_c = data_dir.clone();
        let cached_clips_c = cached_clips.clone();
        let window_weak = main_window.as_weak();
        main_window.on_delete_clip(move |id| {
            if let Some(w) = window_weak.upgrade() {
                if let Ok(Some(img_path)) = db_c.delete_clip(id.as_str()) {
                    let p = Path::new(&img_path);
                    let full = if p.is_absolute() { p.to_path_buf() } else { data_dir_c.join(p) };
                    let _ = fs::remove_file(full);
                }
                let show_fav = w.get_show_favorites();
                let query = w.get_search_query();
                let current_idx = w.get_selected_index();
                let updated = reload_clips(&w, &db_c, &data_dir_c, show_fav, query.as_str(), Some(current_idx));
                *cached_clips_c.lock().unwrap() = updated;
            }
        });
    }

    {
        let db_c = db.clone();
        let data_dir_c = data_dir.clone();
        let cached_clips_c = cached_clips.clone();
        let window_weak = main_window.as_weak();
        main_window.on_toggle_favorites_filter(move || {
            if let Some(w) = window_weak.upgrade() {
                let new_fav = !w.get_show_favorites();
                w.set_show_favorites(new_fav);
                let query = w.get_search_query();
                let updated = reload_clips(&w, &db_c, &data_dir_c, new_fav, query.as_str(), Some(0));
                *cached_clips_c.lock().unwrap() = updated;
            }
        });
    }

    {
        let db_c = db.clone();
        let data_dir_c = data_dir.clone();
        let cached_clips_c = cached_clips.clone();
        let window_weak = main_window.as_weak();
        main_window.on_search_changed(move |query| {
            if let Some(w) = window_weak.upgrade() {
                let show_fav = w.get_show_favorites();
                let is_search = !query.trim().is_empty();
                w.set_is_search_visible(is_search);
                let updated = reload_clips(&w, &db_c, &data_dir_c, show_fav, query.as_str(), Some(0));
                *cached_clips_c.lock().unwrap() = updated;
            }
        });
    }

    {
        let db_c = db.clone();
        let data_dir_c = data_dir.clone();
        let cached_clips_c = cached_clips.clone();
        let window_weak = main_window.as_weak();
        main_window.on_search_closed(move || {
            if let Some(w) = window_weak.upgrade() {
                w.set_search_query("".into());
                w.set_is_search_visible(false);
                let show_fav = w.get_show_favorites();
                let updated = reload_clips(&w, &db_c, &data_dir_c, show_fav, "", Some(0));
                *cached_clips_c.lock().unwrap() = updated;
                w.invoke_focus_main();
            }
        });
    }

    {
        let window_weak = main_window.as_weak();
        let is_monitoring_c = is_monitoring.clone();
        main_window.on_hide_requested(move || {
            if let Some(w) = window_weak.upgrade() {
                w.set_is_search_visible(false);
                w.set_search_query("".into());
                w.set_search_enabled(false);
                let _ = w.hide();
                is_monitoring_c.store(true, Ordering::Relaxed);
            }
        });
    }

    {
        let window_weak = main_window.as_weak();
        let cached_clips_c = cached_clips.clone();
        let data_dir_c = data_dir.clone();
        let is_monitoring_c = is_monitoring.clone();
        main_window.on_paste_selected(move || {
            if let Some(w) = window_weak.upgrade() {
                let _ = w.hide();
                let idx = w.get_selected_index() as usize;
                let clips = cached_clips_c.lock().unwrap().clone();
                if idx < clips.len() {
                    let item = clips[idx].clone();
                    let dir = data_dir_c.clone();
                    let mon = is_monitoring_c.clone();
                    thread::spawn(move || {
                        perform_paste(&item, &dir, &mon);
                    });
                } else {
                    is_monitoring_c.store(true, Ordering::Relaxed);
                }
            }
        });
    }

    {
        let window_weak = main_window.as_weak();
        main_window.on_close_settings(move || {
            if let Some(w) = window_weak.upgrade() {
                w.set_show_settings(false);
                w.invoke_focus_main();
            }
        });
    }

    {
        let window_weak = main_window.as_weak();
        let hotkeys_manager_c = hotkeys_manager.clone();
        let active_hotkey_c = active_hotkey.clone();
        let settings_c = settings.clone();
        let data_dir_c = data_dir.clone();
        main_window.on_save_shortcut(move || {
            if let Some(w) = window_weak.upgrade() {
                let input = w.get_shortcut_input().to_string();
                let trimmed = input.trim();
                match HotKey::from_str(trimmed) {
                    Ok(new_hotkey) => {
                        let mgr = hotkeys_manager_c.lock().unwrap();
                        let mut active = active_hotkey_c.lock().unwrap();
                        let _ = mgr.unregister(*active);
                        if let Err(e) = mgr.register(new_hotkey) {
                            let _ = mgr.register(*active);
                            w.set_shortcut_error(format!("Registration error: {}", e).into());
                        } else {
                            *active = new_hotkey;
                            let mut s = settings_c.lock().unwrap();
                            s.shortcut = trimmed.to_string();
                            save_settings(&data_dir_c, &s);
                            w.set_shortcut_current(trimmed.into());
                            w.set_shortcut_error("".into());
                            w.set_show_settings(false);
                        }
                    }
                    Err(_) => {
                        w.set_shortcut_error("Invalid shortcut format. Example: Ctrl+Alt+Shift+.".into());
                    }
                }
            }
        });
    }

    // 9. Main Event Timer: Global HotKey + Tray + Auto-Hide on Blur
    let timer = slint::Timer::default();
    let window_weak = main_window.as_weak();
    let db_timer = db.clone();
    let data_dir_timer = data_dir.clone();
    let cached_clips_timer = cached_clips.clone();
    let is_monitoring_timer = is_monitoring.clone();
    let mut ignore_blur_counter: u32 = if show_on_startup { 12 } else { 0 };
    let mut search_enable_counter: u32 = if show_on_startup { 6 } else { 0 };

    timer.start(slint::TimerMode::Repeated, Duration::from_millis(40), move || {
        let Some(w) = window_weak.upgrade() else { return; };

        // Enable search after hotkey release window
        if search_enable_counter > 0 {
            search_enable_counter -= 1;
            if search_enable_counter == 0 {
                w.set_search_enabled(true);
            }
        }

        // A. Drain Global HotKey Events
        while let Ok(event) = GlobalHotKeyEvent::receiver().try_recv() {
            if event.state == HotKeyState::Pressed {
                let is_vis = w.window().is_visible();
                if is_vis {
                    // Cycle to next clip
                    let clips = cached_clips_timer.lock().unwrap();
                    let len = clips.len() as i32;
                    if len > 0 {
                        let next_idx = (w.get_selected_index() + 1) % len;
                        w.set_selected_index(next_idx);
                        refresh_active_image(&w, &clips, next_idx as usize, &data_dir_timer);
                    }
                } else {
                    #[cfg(target_os = "windows")]
                    {
                        use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
                        let fg = unsafe { GetForegroundWindow() };
                        let cached = CACHED_HWND.load(Ordering::Relaxed);
                        if !fg.0.is_null() && fg.0 as isize != cached {
                            PREVIOUS_FOREGROUND_HWND.store(fg.0 as isize, Ordering::Relaxed);
                        }
                    }

                    // Modal opened: reload history, center, show, focus
                    is_monitoring_timer.store(false, Ordering::Relaxed);
                    w.set_show_settings(false);
                    w.set_is_search_visible(false);
                    w.set_search_query("".into());
                    w.set_show_favorites(false);
                    w.set_search_enabled(false);
                    search_enable_counter = 6;

                    let updated = reload_clips(&w, &db_timer, &data_dir_timer, false, "", Some(0));
                    *cached_clips_timer.lock().unwrap() = updated;

                    center_and_focus_window(&w);
                    w.invoke_focus_main();
                    ignore_blur_counter = 12; // Give window ~480ms to gain focus before checking blur
                }
            }
        }

        // B. Drain Tray Icon Events
        while let Ok(tray_event) = TrayIconEvent::receiver().try_recv() {
            if let TrayIconEvent::Click { button: MouseButton::Left, button_state: MouseButtonState::Down, .. } = tray_event {
                #[cfg(target_os = "windows")]
                {
                    use windows::Win32::UI::WindowsAndMessaging::GetForegroundWindow;
                    let fg = unsafe { GetForegroundWindow() };
                    let cached = CACHED_HWND.load(Ordering::Relaxed);
                    if !fg.0.is_null() && fg.0 as isize != cached {
                        PREVIOUS_FOREGROUND_HWND.store(fg.0 as isize, Ordering::Relaxed);
                    }
                }

                is_monitoring_timer.store(false, Ordering::Relaxed);
                w.set_show_settings(true);
                w.set_is_search_visible(false);
                w.set_search_query("".into());
                w.set_search_enabled(false);
                search_enable_counter = 6;
                center_and_focus_window(&w);
                w.invoke_focus_main();
                ignore_blur_counter = 12;
            }
        }

        // C. Drain Muda Menu Events (Exit)
        while let Ok(menu_event) = muda::MenuEvent::receiver().try_recv() {
            if menu_event.id() == &exit_id {
                let _ = slint::quit_event_loop();
                std::process::exit(0);
            }
        }

        // D. Auto-Hide on Blur
        if w.window().is_visible() {
            if ignore_blur_counter > 0 {
                ignore_blur_counter -= 1;
            } else if !is_window_foreground() {
                w.set_is_search_visible(false);
                w.set_search_query("".into());
                w.set_search_enabled(false);
                let _ = w.hide();
                is_monitoring_timer.store(true, Ordering::Relaxed);
            }
        }
    });

    println!("Clipped (Pure Rust Native) running with Slint GUI!");
    let _keep_alive = main_window;
    slint::run_event_loop_until_quit().unwrap();
}
