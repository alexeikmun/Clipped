use std::fs::{self, File};
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};
use std::sync::Arc;
use std::time::Instant;

use arboard::Clipboard;
use image::codecs::png::{CompressionType, FilterType, PngEncoder};
use image::ImageEncoder;
use uuid::Uuid;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Direct2D::Common::D2D1_COLOR_F;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Com::*;
use windows::Win32::System::DataExchange::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::ProcessStatus::EmptyWorkingSet;
use windows::Win32::System::Registry::*;
use windows::Win32::System::Threading::{CreateMutexW, GetCurrentProcess};
use windows::Win32::UI::Controls::WM_MOUSELEAVE;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use xxhash_rust::xxh3::xxh3_128;

use crate::d2d::D2dContext;
use crate::db::{ClipItem, Database};
use crate::model::{
    AppSettings, HitTarget, UiMode, SETTINGS_FILE, WINDOW_HEIGHT, WINDOW_WIDTH,
};
use crate::ocr::extract_ocr_text;
use crate::paste::perform_paste;

const WM_TRAYICON: u32 = WM_USER + 1;
const HOTKEY_ID: i32 = 1;
const TIMER_BLINK_ID: usize = 101;

const ID_TRAY_OPEN: usize = 1001;
const ID_TRAY_SETTINGS: usize = 1002;
const ID_TRAY_EXIT: usize = 1003;

pub struct AppState {
    pub hwnd: HWND,
    pub d2d: D2dContext,
    pub ui: crate::ui::UiState,
    pub db: Database,
    pub is_monitoring: Arc<AtomicBool>,
    pub previous_foreground: AtomicIsize,
    pub dpi_scale: f32,
    pub app_icon_big: HICON,
    pub app_icon_sm: HICON,
    pub nid: NOTIFYICONDATAW,
    pub _hotkey_id: i32,
    pub last_text_hash: u128,
    pub last_image_hash: u128,
    pub ignore_blur_until: Instant,
}

impl AppState {
    pub fn new(data_dir: PathBuf, db: Database, settings: AppSettings) -> windows::core::Result<Self> {
        let d2d = D2dContext::new()?;
        let ui = crate::ui::UiState::new(data_dir.clone(), settings);

        Ok(Self {
            hwnd: HWND(0 as _),
            d2d,
            ui,
            db,
            is_monitoring: Arc::new(AtomicBool::new(true)),
            previous_foreground: AtomicIsize::new(0),
            dpi_scale: 1.0,
            app_icon_big: HICON::default(),
            app_icon_sm: HICON::default(),
            nid: NOTIFYICONDATAW::default(),
            _hotkey_id: HOTKEY_ID,
            last_text_hash: 0,
            last_image_hash: 0,
            ignore_blur_until: Instant::now(),
        })
    }

    pub fn show_modal(&mut self) {
        unsafe {
            // Save foreground window so we know where to paste back later
            let fg = GetForegroundWindow();
            if fg.0 != self.hwnd.0 && !fg.0.is_null() {
                self.previous_foreground.store(fg.0 as isize, Ordering::Relaxed);
            }

            // Reload items from DB
            let clips = self
                .db
                .get_history(self.ui.settings.max_items, self.ui.show_favorites)
                .unwrap_or_default();
            self.ui.mode = UiMode::SingleCard;
            self.ui.search_query.clear();
            self.ui.set_clips(clips, Some(0));

            // Center window on primary monitor
            self.center_window();

            // Ignore blur for 250ms to allow smooth focus transition
            self.ignore_blur_until = Instant::now() + std::time::Duration::from_millis(250);

            let _ = ShowWindow(self.hwnd, SW_SHOW);
            let _ = SetForegroundWindow(self.hwnd);
            let _ = SetFocus(self.hwnd);

            let _ = InvalidateRect(self.hwnd, None, false);
        }
    }

    pub fn show_settings(&mut self) {
        self.ui.mode = UiMode::Settings;
        self.show_modal();
    }

    pub fn hide_modal(&mut self) {
        unsafe {
            let is_visible = IsWindowVisible(self.hwnd).as_bool();
            if is_visible {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
                self.ui.mode = UiMode::SingleCard;
                self.ui.search_query.clear();
                self.ui.hover_target = HitTarget::None;

                // Evict cached Direct2D textures from VRAM/RAM
                self.d2d.clear_bitmap_cache();

                // Memory trim on modal close
                self.db.shrink_memory();
                let _ = EmptyWorkingSet(GetCurrentProcess());
            }
        }
    }

    pub fn center_window(&self) {
        unsafe {
            let width = (WINDOW_WIDTH * self.dpi_scale).round() as i32;
            let height = (WINDOW_HEIGHT * self.dpi_scale).round() as i32;

            let prev_fg = self.previous_foreground.load(Ordering::Relaxed);
            let target_hwnd = if prev_fg != 0 {
                HWND(prev_fg as _)
            } else {
                GetForegroundWindow()
            };

            let hmon = if !target_hwnd.0.is_null() && target_hwnd != self.hwnd {
                MonitorFromWindow(target_hwnd, MONITOR_DEFAULTTONEAREST)
            } else {
                let mut pt = POINT::default();
                let _ = GetCursorPos(&mut pt);
                MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST)
            };

            let mut mi = MONITORINFO {
                cbSize: std::mem::size_of::<MONITORINFO>() as u32,
                ..Default::default()
            };

            let (x, y) = if GetMonitorInfoW(hmon, &mut mi).as_bool() {
                let work_w = mi.rcWork.right - mi.rcWork.left;
                let work_h = mi.rcWork.bottom - mi.rcWork.top;
                (
                    mi.rcWork.left + (work_w - width) / 2,
                    mi.rcWork.top + (work_h - height) / 2,
                )
            } else {
                let screen_w = GetSystemMetrics(SM_CXSCREEN);
                let screen_h = GetSystemMetrics(SM_CYSCREEN);
                ((screen_w - width) / 2, (screen_h - height) / 2)
            };

            let _ = SetWindowPos(
                self.hwnd,
                HWND_TOPMOST,
                x,
                y,
                width,
                height,
                SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
        }
    }

    pub fn on_clipboard_update(&mut self) {
        if !self.is_monitoring.load(Ordering::Relaxed) {
            return;
        }

        let Ok(mut clipboard) = Clipboard::new() else {
            return;
        };

        // 1. Try reading image
        let mut image_handled = false;
        if let Ok(img) = clipboard.get_image() {
            let hash = xxh3_128(&img.bytes);
            if hash != self.last_image_hash {
                self.last_image_hash = hash;
                self.last_text_hash = 0;
                image_handled = true;

                let hash_str = format!("{:032x}", hash);
                let (w, h) = (img.width as u32, img.height as u32);

                if let Ok(Some(existing)) = self.db.find_image_by_hash(&hash_str) {
                    let _ = self.db.bump_clip_timestamp(&existing.id);
                } else {
                    let id = Uuid::new_v4().to_string();
                    let filename = format!("{}.png", id);
                    let images_dir = self.ui.data_dir.join("images");
                    let _ = fs::create_dir_all(&images_dir);
                    let full_path = images_dir.join(&filename);

                    if save_image_fast_png(&full_path, &img.bytes, w, h).is_ok() {
                        let ocr = extract_ocr_text(&full_path);
                        let full_path_str = full_path.to_string_lossy().replace('\\', "/");
                        let item = ClipItem {
                            id,
                            text: format!("[Image {}x{}]", w, h),
                            full_text_len: 0,
                            is_favorite: false,
                            clip_type: "image".to_string(),
                            image_path: Some(full_path_str),
                            image_width: Some(w),
                            image_height: Some(h),
                            ocr_text: ocr,
                        };
                        let _ = self.db.insert_image_clip(&item, &hash_str);
                    }
                }

                // Prune
                if let Ok(deleted) = self.db.prune_history(self.ui.settings.max_items) {
                    for p in deleted {
                        let path = Path::new(&p);
                        let full = if path.is_absolute() {
                            path.to_path_buf()
                        } else {
                            self.ui.data_dir.join(path)
                        };
                        let _ = fs::remove_file(full);
                    }
                }

                self.refresh_ui_clips();
            }
        }

        // 2. Try reading text if not an image
        if !image_handled {
            if let Ok(text) = clipboard.get_text() {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    let hash = xxh3_128(text.as_bytes());
                    if hash != self.last_text_hash {
                        self.last_text_hash = hash;
                        self.last_image_hash = 0;

                        let hash_str = format!("{:032x}", hash);
                        let _ = self.db.save_or_bump_text(text, &hash_str);

                        // Prune
                        if let Ok(deleted) = self.db.prune_history(self.ui.settings.max_items) {
                            for p in deleted {
                                let path = Path::new(&p);
                                let full = if path.is_absolute() {
                                    path.to_path_buf()
                                } else {
                                    self.ui.data_dir.join(path)
                                };
                                let _ = fs::remove_file(full);
                            }
                        }

                        self.refresh_ui_clips();
                    }
                }
            }
        }
    }

    pub fn refresh_ui_clips(&mut self) {
        self.refresh_ui_clips_with_selection(None);
    }

    pub fn refresh_ui_clips_with_selection(&mut self, select_idx: Option<usize>) {
        unsafe {
            if IsWindowVisible(self.hwnd).as_bool() {
                let is_search = !self.ui.search_query.is_empty();
                let clips = if !is_search {
                    self.db
                        .get_history(self.ui.settings.max_items, self.ui.show_favorites)
                        .unwrap_or_default()
                } else {
                    self.db
                        .search_clips(
                            &self.ui.search_query,
                            self.ui.show_favorites,
                            self.ui.settings.max_items,
                        )
                        .unwrap_or_default()
                };
                let effective_idx = select_idx.or_else(|| if is_search { Some(0) } else { None });
                self.ui.set_clips(clips, effective_idx);
                let _ = InvalidateRect(self.hwnd, None, false);
            }
        }
    }

    pub fn paste_selected(&mut self) {
        if let Some(clip) = self.ui.selected_clip().cloned() {
            let prev_hwnd = self.previous_foreground.load(Ordering::Relaxed);
            let data_dir = self.ui.data_dir.clone();

            // Fetch full clip from database to avoid truncating long clips
            let clip_to_paste = self.db.get_full_clip(&clip.id).ok().flatten().unwrap_or(clip);

            self.hide_modal();

            let mon = self.is_monitoring.clone();
            std::thread::spawn(move || {
                perform_paste(&clip_to_paste, &data_dir, prev_hwnd, &mon);
            });
        }
    }

    pub fn delete_selected(&mut self) {
        if let Some(clip) = self.ui.selected_clip() {
            let id = clip.id.clone();
            if let Ok(Some(img_path)) = self.db.delete_clip(&id) {
                let path = Path::new(&img_path);
                let full = if path.is_absolute() {
                    path.to_path_buf()
                } else {
                    self.ui.data_dir.join(path)
                };
                let _ = fs::remove_file(full);
            }
            let curr_idx = self.ui.selected_index;
            self.refresh_ui_clips_with_selection(Some(curr_idx));
        }
    }

    pub fn toggle_favorite_selected(&mut self) {
        if let Some(clip) = self.ui.selected_clip() {
            let id = clip.id.clone();
            let _ = self.db.toggle_favorite(&id);
            let curr_idx = self.ui.selected_index;
            self.refresh_ui_clips_with_selection(Some(curr_idx));
        }
    }

    pub fn show_tray_menu(&self) {
        unsafe {
            let hmenu = match CreatePopupMenu() {
                Ok(m) => m,
                Err(_) => return,
            };

            let _ = AppendMenuW(hmenu, MF_STRING, ID_TRAY_OPEN, w!("Open Clipped"));
            let _ = AppendMenuW(hmenu, MF_STRING, ID_TRAY_SETTINGS, w!("Settings"));
            let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
            let _ = AppendMenuW(hmenu, MF_STRING, ID_TRAY_EXIT, w!("Exit"));

            let mut pt = POINT::default();
            let _ = GetCursorPos(&mut pt);

            let _ = SetForegroundWindow(self.hwnd);
            let _ = TrackPopupMenuEx(
                hmenu,
                (TPM_RIGHTBUTTON | TPM_BOTTOMALIGN).0 as u32,
                pt.x,
                pt.y,
                self.hwnd,
                None,
            );
            let _ = DestroyMenu(hmenu);
        }
    }
}

pub fn run_app() -> windows::core::Result<()> {
    unsafe {
        // Enforce single running instance
        let mutex_name = w!("Local\\Clipped_SingleInstance_Mutex");
        let _single_instance_mutex = CreateMutexW(None, true, mutex_name);
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let class_name = w!("ClippedWindowClass");
            if let Ok(existing_hwnd) = FindWindowW(class_name, None) {
                if !existing_hwnd.0.is_null() {
                    let _ = ShowWindow(existing_hwnd, SW_SHOW);
                    let _ = SetForegroundWindow(existing_hwnd);
                }
            }
            return Ok(());
        }

        // Initialize COM apartment
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        // Set Per-Monitor V2 DPI awareness
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        // Explicit AppUserModelID for Windows Taskbar and Task Manager association
        let _ = SetCurrentProcessExplicitAppUserModelID(w!("Alexis.Clipped.App"));

        let data_dir = get_data_dir();
        let _ = fs::create_dir_all(&data_dir);
        let settings = load_settings(&data_dir);
        let db = Database::init(&data_dir).expect("Failed to initialize SQLite database");
        let app_state = AppState::new(data_dir.clone(), db, settings)?;

        // Register window class
        let class_name = w!("ClippedWindowClass");
        let h_instance = GetModuleHandleW(None)?;
        let app_icon_big = load_app_icon(h_instance.into(), false);
        let app_icon_sm = load_app_icon(h_instance.into(), true);

        let wnd_class = WNDCLASSEXW {
            cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(wnd_proc),
            hInstance: h_instance.into(),
            hIcon: app_icon_big,
            hIconSm: app_icon_sm,
            hCursor: LoadCursorW(None, IDC_ARROW)?,
            hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
            lpszClassName: class_name,
            ..Default::default()
        };
        RegisterClassExW(&wnd_class);

        let mut app_state = app_state;
        app_state.app_icon_big = app_icon_big;
        app_state.app_icon_sm = app_icon_sm;

        let state_box = Box::new(app_state);
        let state_ptr = Box::into_raw(state_box);

        let hwnd = CreateWindowExW(
            WS_EX_TOOLWINDOW | WS_EX_TOPMOST,
            class_name,
            w!("Clipped"),
            WS_POPUP,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            WINDOW_WIDTH as i32,
            WINDOW_HEIGHT as i32,
            None,
            None,
            h_instance,
            Some(state_ptr as *const _),
        )?;

        SetWindowLongPtrW(hwnd, GWLP_USERDATA, state_ptr as isize);
        (*state_ptr).hwnd = hwnd;

        let _ = SendMessageW(hwnd, WM_SETICON, WPARAM(ICON_BIG as usize), LPARAM(app_icon_big.0 as isize));
        let _ = SendMessageW(hwnd, WM_SETICON, WPARAM(ICON_SMALL as usize), LPARAM(app_icon_sm.0 as isize));
        let _ = SendMessageW(hwnd, WM_SETICON, WPARAM(2 /* ICON_SMALL2 */), LPARAM(app_icon_sm.0 as isize));

        // Apply DWM attributes (rounded corners & dark theme)
        let corner_pref = DWMWCP_ROUND;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner_pref as *const _ as *const _,
            std::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
        );

        let use_dark_mode: BOOL = true.into();
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &use_dark_mode as *const _ as *const _,
            std::mem::size_of::<BOOL>() as u32,
        );

        // DPI Scaling
        let dpi = GetDpiForWindow(hwnd);
        (*state_ptr).dpi_scale = (dpi as f32) / 96.0;

        // System Tray Icon
        let mut tip = [0u16; 128];
        let tip_src: Vec<u16> = "Clipped - Clipboard Manager\0".encode_utf16().collect();
        tip[..tip_src.len()].copy_from_slice(&tip_src);

        let nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: 1,
            uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
            uCallbackMessage: WM_TRAYICON,
            hIcon: app_icon_sm,
            szTip: tip,
            ..Default::default()
        };
        let _ = Shell_NotifyIconW(NIM_ADD, &nid);
        (*state_ptr).nid = nid;

        // Global Hotkey
        if let Some((mods, vk)) = parse_hotkey(&(*state_ptr).ui.settings.shortcut) {
            let _ = RegisterHotKey(hwnd, HOTKEY_ID, mods | MOD_NOREPEAT, vk);
        }

        // Clipboard Format Listener
        let _ = AddClipboardFormatListener(hwnd);

        // Blinking cursor timer
        let _ = SetTimer(hwnd, TIMER_BLINK_ID, 400, None);

        // Initial memory purge
        let _ = EmptyWorkingSet(GetCurrentProcess());

        println!("Clipped (Pure Win32 + Direct2D) running with ~3 MB memory footprint!");

        let is_autostart = std::env::args().any(|a| a == "--autostart" || a == "--minimized");

        if (*state_ptr).ui.settings.launch_on_boot {
            set_launch_on_boot(true);
        }

        if !is_autostart {
            (*state_ptr).show_modal();
        }

        // Message Loop
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        // Cleanup
        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
        let _ = RemoveClipboardFormatListener(hwnd);
        let _ = UnregisterHotKey(hwnd, HOTKEY_ID);

        let ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AppState;
        if !ptr.is_null() {
            let _ = Box::from_raw(ptr);
        }
    }

    Ok(())
}

unsafe extern "system" fn wnd_proc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut AppState;
    if state_ptr.is_null() {
        return DefWindowProcW(hwnd, msg, wparam, lparam);
    }
    let state = &mut *state_ptr;

    match msg {
        WM_GETICON => {
            let icon = match wparam.0 as u32 {
                ICON_BIG => state.app_icon_big,
                _ => state.app_icon_sm,
            };
            LRESULT(icon.0 as isize)
        }

        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let _ = BeginPaint(hwnd, &mut ps);

            let width = (WINDOW_WIDTH * state.dpi_scale).round() as u32;
            let height = (WINDOW_HEIGHT * state.dpi_scale).round() as u32;

            if state.d2d.ensure_target(hwnd, width, height).is_ok() {
                state.d2d.set_dpi_scale(state.dpi_scale);
                state.d2d.begin_draw();
                state.d2d.clear(&D2D1_COLOR_F {
                    r: 24.0 / 255.0,
                    g: 24.0 / 255.0,
                    b: 37.0 / 255.0,
                    a: 1.0,
                });
                state.ui.render(&mut state.d2d);
                let _ = state.d2d.end_draw();
            }

            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }

        WM_HOTKEY => {
            if (wparam.0 as i32) == HOTKEY_ID {
                if IsWindowVisible(hwnd).as_bool() {
                    // Cycle to next clip
                    let len = state.ui.clips.len();
                    if len > 0 {
                        state.ui.selected_index = (state.ui.selected_index + 1) % len;
                        state.ui.ensure_selected_visible();
                        let _ = InvalidateRect(hwnd, None, false);
                    }
                } else {
                    state.show_modal();
                }
            }
            LRESULT(0)
        }

        WM_CLIPBOARDUPDATE => {
            state.on_clipboard_update();
            LRESULT(0)
        }

        WM_TRAYICON => {
            let event = (lparam.0 & 0xFFFF) as u32;
            if event == WM_LBUTTONUP {
                if IsWindowVisible(hwnd).as_bool() {
                    state.hide_modal();
                } else {
                    state.show_modal();
                }
            } else if event == WM_RBUTTONUP {
                state.show_tray_menu();
            }
            LRESULT(0)
        }

        WM_COMMAND => {
            let cmd_id = (wparam.0 & 0xFFFF) as usize;
            match cmd_id {
                ID_TRAY_OPEN => state.show_modal(),
                ID_TRAY_SETTINGS => state.show_settings(),
                ID_TRAY_EXIT => {
                    let _ = DestroyWindow(hwnd);
                }
                _ => {}
            }
            LRESULT(0)
        }

        WM_ACTIVATE => {
            let active = (wparam.0 & 0xFFFF) as u32;
            if active == WA_INACTIVE {
                if Instant::now() > state.ignore_blur_until {
                    state.hide_modal();
                }
            }
            LRESULT(0)
        }

        WM_KILLFOCUS => {
            if Instant::now() > state.ignore_blur_until {
                state.hide_modal();
            }
            LRESULT(0)
        }

        WM_DPICHANGED => {
            let new_dpi = (wparam.0 & 0xFFFF) as u32;
            state.dpi_scale = (new_dpi as f32) / 96.0;
            state.center_window();
            let _ = InvalidateRect(hwnd, None, false);
            LRESULT(0)
        }

        WM_TIMER => {
            if wparam.0 == TIMER_BLINK_ID {
                if state.ui.mode == UiMode::SearchList && IsWindowVisible(hwnd).as_bool() {
                    let _ = InvalidateRect(hwnd, None, false);
                }
            }
            LRESULT(0)
        }

        WM_MOUSEMOVE => {
            let px = (lparam.0 & 0xFFFF) as i16 as f32;
            let py = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
            let lx = px / state.dpi_scale;
            let ly = py / state.dpi_scale;

            let mut tme = TRACKMOUSEEVENT {
                cbSize: std::mem::size_of::<TRACKMOUSEEVENT>() as u32,
                dwFlags: TME_LEAVE,
                hwndTrack: hwnd,
                dwHoverTime: 0,
            };
            let _ = TrackMouseEvent(&mut tme);

            let hit = state.ui.hit_test(lx, ly);
            if hit != state.ui.hover_target {
                state.ui.hover_target = hit;
                let _ = InvalidateRect(hwnd, None, false);
            }
            LRESULT(0)
        }

        WM_MOUSELEAVE => {
            if state.ui.hover_target != HitTarget::None {
                state.ui.hover_target = HitTarget::None;
                let _ = InvalidateRect(hwnd, None, false);
            }
            LRESULT(0)
        }

        WM_LBUTTONDOWN => {
            let px = (lparam.0 & 0xFFFF) as i16 as f32;
            let py = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
            let lx = px / state.dpi_scale;
            let ly = py / state.dpi_scale;

            let hit = state.ui.hit_test(lx, ly);
            match hit {
                HitTarget::FavFilter => {
                    state.ui.show_favorites = !state.ui.show_favorites;
                    state.refresh_ui_clips_with_selection(Some(0));
                }
                HitTarget::SearchBox => {
                    if state.ui.mode == UiMode::SingleCard {
                        state.ui.mode = UiMode::SearchList;
                        state.ui.selected_index = 0;
                        state.ui.ensure_selected_visible();
                        let _ = InvalidateRect(hwnd, None, false);
                    }
                }
                HitTarget::CloseSettings => {
                    state.ui.mode = UiMode::SingleCard;
                    let _ = InvalidateRect(hwnd, None, false);
                }
                HitTarget::DockTrash => {
                    state.delete_selected();
                }
                HitTarget::DockStar => {
                    state.toggle_favorite_selected();
                }
                HitTarget::ListItem(idx) => {
                    if state.ui.selected_index == idx {
                        state.paste_selected();
                    } else {
                        state.ui.selected_index = idx;
                        let _ = InvalidateRect(hwnd, None, false);
                    }
                }
                HitTarget::SettingsBootToggle => {
                    state.ui.settings.launch_on_boot = !state.ui.settings.launch_on_boot;
                    set_launch_on_boot(state.ui.settings.launch_on_boot);
                    save_settings(&state.ui.data_dir, &state.ui.settings);
                    let _ = InvalidateRect(hwnd, None, false);
                }
                HitTarget::ClearNonFavorites => {
                    if let Ok(deleted) = state.db.prune_history(0) {
                        for p in deleted {
                            let path = Path::new(&p);
                            let full = if path.is_absolute() {
                                path.to_path_buf()
                            } else {
                                state.ui.data_dir.join(path)
                            };
                            let _ = fs::remove_file(full);
                        }
                    }
                    state.ui.settings_message = "Non-favorites cleared!".to_string();
                    state.refresh_ui_clips_with_selection(Some(0));
                }
                HitTarget::ClearAll => {
                    if let Ok(deleted) = state.db.clear_all_history() {
                        for p in deleted {
                            let path = Path::new(&p);
                            let full = if path.is_absolute() {
                                path.to_path_buf()
                            } else {
                                state.ui.data_dir.join(path)
                            };
                            let _ = fs::remove_file(full);
                        }
                    }
                    state.ui.settings_message = "History cleared!".to_string();
                    state.refresh_ui_clips_with_selection(Some(0));
                }
                HitTarget::None => {}
            }
            LRESULT(0)
        }

        WM_MOUSEWHEEL => {
            if state.ui.mode == UiMode::SearchList {
                let delta = ((wparam.0 >> 16) & 0xFFFF) as i16;
                let step = 56.0 * 2.0;
                let max_scroll = ((state.ui.clips.len() as f32) * 56.0 - 250.0).max(0.0);
                state.ui.scroll_offset = (state.ui.scroll_offset - (delta as f32 / 120.0) * step)
                    .clamp(0.0, max_scroll);
                let _ = InvalidateRect(hwnd, None, false);
            }
            LRESULT(0)
        }

        WM_KEYDOWN => {
            let key = VIRTUAL_KEY(wparam.0 as u16);
            match key {
                VK_TAB => {
                    if state.ui.mode != UiMode::Settings {
                        state.ui.show_favorites = !state.ui.show_favorites;
                        state.refresh_ui_clips_with_selection(Some(0));
                        let _ = InvalidateRect(hwnd, None, false);
                    }
                }
                VK_ESCAPE => {
                    if state.ui.mode == UiMode::Settings {
                        state.ui.mode = UiMode::SingleCard;
                        let _ = InvalidateRect(hwnd, None, false);
                    } else if state.ui.mode == UiMode::SearchList {
                        state.ui.search_query.clear();
                        state.ui.mode = UiMode::SingleCard;
                        state.refresh_ui_clips_with_selection(Some(0));
                    } else {
                        state.hide_modal();
                    }
                }
                VK_RETURN => {
                    state.paste_selected();
                }
                VK_DELETE => {
                    state.delete_selected();
                }
                VK_UP | VK_LEFT => {
                    let len = state.ui.clips.len();
                    if len > 0 {
                        if state.ui.selected_index > 0 {
                            state.ui.selected_index -= 1;
                        } else {
                            state.ui.selected_index = len - 1;
                        }
                        state.ui.ensure_selected_visible();
                        let _ = InvalidateRect(hwnd, None, false);
                    }
                }
                VK_DOWN | VK_RIGHT => {
                    let len = state.ui.clips.len();
                    if len > 0 {
                        if state.ui.selected_index < len - 1 {
                            state.ui.selected_index += 1;
                        } else {
                            state.ui.selected_index = 0;
                        }
                        state.ui.ensure_selected_visible();
                        let _ = InvalidateRect(hwnd, None, false);
                    }
                }
                VK_BACK => {
                    if state.ui.mode == UiMode::SearchList {
                        if state.ui.search_query.pop().is_some() {
                            if state.ui.search_query.is_empty() {
                                state.ui.mode = UiMode::SingleCard;
                                state.refresh_ui_clips_with_selection(Some(0));
                            } else {
                                state.refresh_ui_clips_with_selection(Some(0));
                            }
                        }
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }

        WM_CHAR => {
            let ch = char::from_u32(wparam.0 as u32).unwrap_or('\0');
            if !ch.is_control() && state.ui.mode != UiMode::Settings {
                // In SingleCard mode, check for number navigation 1-9
                if state.ui.mode == UiMode::SingleCard && ch >= '1' && ch <= '9' {
                    let idx = (ch as usize) - ('1' as usize);
                    if idx < state.ui.clips.len() {
                        state.ui.selected_index = idx;
                        let _ = InvalidateRect(hwnd, None, false);
                        return LRESULT(0);
                    }
                }

                // Otherwise, switch to SearchList and append character
                if state.ui.mode == UiMode::SingleCard {
                    state.ui.mode = UiMode::SearchList;
                    state.ui.search_query.clear();
                }
                state.ui.search_query.push(ch);
                state.refresh_ui_clips_with_selection(Some(0));
            }
            LRESULT(0)
        }

        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }

        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

fn parse_hotkey(shortcut: &str) -> Option<(HOT_KEY_MODIFIERS, u32)> {
    let mut mods = HOT_KEY_MODIFIERS(0);
    let parts: Vec<&str> = shortcut.split('+').map(|s| s.trim()).collect();
    if parts.is_empty() {
        return None;
    }

    let key_part = parts.last()?;
    for &part in &parts[..parts.len() - 1] {
        match part.to_lowercase().as_str() {
            "ctrl" | "control" => mods |= MOD_CONTROL,
            "alt" => mods |= MOD_ALT,
            "shift" => mods |= MOD_SHIFT,
            "win" | "super" => mods |= MOD_WIN,
            _ => {}
        }
    }

    let vk = match key_part.to_lowercase().as_str() {
        "." => VK_OEM_PERIOD.0 as u32,
        "," => VK_OEM_COMMA.0 as u32,
        "/" => VK_OEM_2.0 as u32,
        ";" => VK_OEM_1.0 as u32,
        "space" => VK_SPACE.0 as u32,
        "return" | "enter" => VK_RETURN.0 as u32,
        s if s.len() == 1 => {
            let c = s.chars().next().unwrap().to_ascii_uppercase();
            c as u32
        }
        _ => VK_OEM_PERIOD.0 as u32,
    };

    Some((mods, vk))
}

fn set_launch_on_boot(enable: bool) {
    unsafe {
        let subkey = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
        let mut hkey = HKEY::default();
        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey, 0, KEY_SET_VALUE, &mut hkey).is_ok() {
            let val_name = w!("Clipped");
            if enable {
                if let Ok(exe) = std::env::current_exe() {
                    let path_str = format!("\"{}\" --autostart", exe.to_string_lossy());
                    let utf16: Vec<u16> = path_str.encode_utf16().chain(std::iter::once(0)).collect();
                    let bytes = std::slice::from_raw_parts(
                        utf16.as_ptr() as *const u8,
                        utf16.len() * std::mem::size_of::<u16>(),
                    );
                    let _ = RegSetValueExW(hkey, val_name, 0, REG_SZ, Some(bytes));
                }
            } else {
                let _ = RegDeleteValueW(hkey, val_name);
            }
            let _ = RegCloseKey(hkey);
        }
    }
}

fn get_data_dir() -> PathBuf {
    if let Ok(appdata) = std::env::var("APPDATA") {
        PathBuf::from(appdata).join("clipped")
    } else {
        PathBuf::from(".clipped")
    }
}

fn load_settings(data_dir: &Path) -> AppSettings {
    let settings_path = data_dir.join(SETTINGS_FILE);
    if settings_path.exists() {
        if let Ok(content) = fs::read_to_string(&settings_path) {
            if let Ok(s) = serde_json::from_str::<AppSettings>(&content) {
                return s;
            }
        }
    }
    AppSettings::default()
}

fn save_settings(data_dir: &Path, settings: &AppSettings) {
    let settings_path = data_dir.join(SETTINGS_FILE);
    if let Ok(content) = serde_json::to_string_pretty(settings) {
        let _ = fs::write(settings_path, content);
    }
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

unsafe fn load_app_icon(h_instance: HINSTANCE, sm: bool) -> HICON {
    let (cx, cy) = if sm {
        (GetSystemMetrics(SM_CXSMICON), GetSystemMetrics(SM_CYSMICON))
    } else {
        (GetSystemMetrics(SM_CXICON), GetSystemMetrics(SM_CYICON))
    };

    // 1. Try loading from compiled PE resource (resource ID 1 embedded via winres)
    if let Ok(handle) = LoadImageW(
        h_instance,
        PCWSTR(1 as *const u16),
        IMAGE_ICON,
        cx,
        cy,
        LR_SHARED,
    ) {
        let hicon = HICON(handle.0);
        if !hicon.is_invalid() {
            return hicon;
        }
    }
    if let Ok(icon) = LoadIconW(h_instance, PCWSTR(1 as *const u16)) {
        if !icon.is_invalid() {
            return icon;
        }
    }

    // 2. Try loading from assets/icon.ico on disk
    let candidates = [
        PathBuf::from("assets/icon.ico"),
        PathBuf::from("../assets/icon.ico"),
        std::env::current_exe()
            .ok()
            .and_then(|p| p.parent().map(|d| d.join("assets/icon.ico")))
            .unwrap_or_default(),
    ];
    for p in &candidates {
        if p.exists() {
            let wide: Vec<u16> = p
                .to_string_lossy()
                .encode_utf16()
                .chain(std::iter::once(0))
                .collect();
            if let Ok(handle) = LoadImageW(
                None,
                PCWSTR(wide.as_ptr()),
                IMAGE_ICON,
                cx,
                cy,
                LR_LOADFROMFILE | LR_SHARED,
            ) {
                let hicon = HICON(handle.0);
                if !hicon.is_invalid() {
                    return hicon;
                }
            }
        }
    }

    // 3. Fallback to default application icon
    LoadIconW(None, IDI_APPLICATION).unwrap_or_default()
}

#[cfg(test)]
mod icon_tests {
    use super::*;

    #[test]
    fn test_load_icon() {
        unsafe {
            let h_instance = GetModuleHandleW(None).unwrap();
            let big = load_app_icon(h_instance.into(), false);
            let sm = load_app_icon(h_instance.into(), true);
            println!("test_load_icon: big={:?}, sm={:?}", big.0, sm.0);
            assert!(!big.0.is_null());
            assert!(!sm.0.is_null());
        }
    }
}

