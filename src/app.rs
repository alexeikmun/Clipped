use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Instant, SystemTime, UNIX_EPOCH};

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
use windows::Win32::System::Threading::{
    CreateMutexW, GetCurrentProcess, OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_FORMAT,
    PROCESS_QUERY_LIMITED_INFORMATION,
};
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
use crate::paste::perform_paste;
use crate::typesafe::JevSearch;
use crate::ui::JevStatus;

const WM_TRAYICON: u32 = WM_USER + 1;
const WM_RERANK_COMPLETE: u32 = WM_USER + 2;
pub(crate) const WM_OCR_COMPLETE: u32 = WM_USER + 3;
const WM_ACTIVATE_INSTANCE: u32 = WM_USER + 4;
const HOTKEY_ID: i32 = 1;
const TIMER_BLINK_ID: usize = 101;
const TIMER_RERANK_DEBOUNCE_ID: usize = 102;

const ID_TRAY_OPEN: usize = 1001;
const ID_TRAY_SETTINGS: usize = 1002;
const ID_TRAY_EXIT: usize = 1003;
type JevResults = Mutex<Option<(u64, Result<JevSearch, String>)>>;
const JEV_MIN_QUERY_CHARS: usize = 2;

/// Applies a finished search only if it belongs to the current query; returns whether it did.
fn apply_jev_completion(
    ui: &mut crate::ui::UiState,
    seq: u64,
    current_seq: u64,
    pending: &JevResults,
) -> bool {
    if seq == 0 || seq != current_seq {
        return false;
    }
    let Some((_, outcome)) = pending.lock().unwrap().take_if(|(stored, _)| *stored == seq) else {
        return false;
    };
    match outcome {
        Ok(search) => {
            ui.jev_status = JevStatus::Ready;
            ui.jev_result = Some(search);
        }
        Err(msg) => {
            ui.jev_status = JevStatus::Failed(msg);
            ui.jev_result = None;
        }
    }
    true
}

/// What a query should do on its own, before any network call.
fn jev_status_for(query: Option<&str>, enabled: bool, has_key: bool) -> JevStatus {
    match query {
        Some(q) if q.chars().count() >= JEV_MIN_QUERY_CHARS => {
            if !enabled {
                JevStatus::Failed("Jev search is turned off in settings".into())
            } else if !has_key {
                JevStatus::Failed("Add a TypeSafe API key to use > search".into())
            } else {
                JevStatus::Searching
            }
        }
        _ => JevStatus::Idle,
    }
}

/// The list for the current query: keyword search, history, or the last Jev result re-applied
/// to the live database (so deletes, favorites, and new copies show up without a new request).
fn visible_clips(db: &Database, ui: &crate::ui::UiState) -> Vec<ClipItem> {
    let raw_query = ui.search_query.trim();
    let history = || db.get_history(ui.settings.max_items, ui.show_favorites).unwrap_or_default();
    match crate::typesafe::semantic_query(raw_query) {
        None if raw_query.is_empty() => history(),
        None => db.search_clips(raw_query, ui.show_favorites, ui.settings.max_items).unwrap_or_default(),
        Some(_) => match (&ui.jev_result, &ui.jev_status) {
            (Some(search), _) => search.arrange(
                db.get_filtered_clips(&search.filter, ui.show_favorites).unwrap_or_default(),
            ),
            (None, JevStatus::Failed(_)) => Vec::new(),
            // `>` alone, a too-short query, or the first search still loading.
            (None, _) => history(),
        },
    }
}

pub struct AppState {
    pub hwnd: HWND,
    pub d2d: D2dContext,
    pub ui: crate::ui::UiState,
    pub db: Arc<Database>,
    ocr_jobs: Option<std::sync::mpsc::SyncSender<(String, PathBuf)>>,
    pub is_monitoring: Arc<AtomicBool>,
    pub previous_foreground: AtomicIsize,
    pub dpi_scale: f32,
    pub app_icon_big: HICON,
    pub app_icon_sm: HICON,
    pub nid: NOTIFYICONDATAW,
    pub last_text_hash: u128,
    pub last_image_hash: u128,
    pub ignore_blur_until: Instant,
    pub search_sequence_id: Arc<AtomicU64>,
    pub pending_jev_results: Arc<JevResults>,
    pending_surrogate: Option<u16>,
}

impl AppState {
    pub fn new(data_dir: PathBuf, db: Database, settings: AppSettings) -> windows::core::Result<Self> {
        let d2d = D2dContext::new()?;
        let ui = crate::ui::UiState::new(data_dir.clone(), settings);

        Ok(Self {
            hwnd: HWND(0 as _),
            d2d,
            ui,
            db: Arc::new(db),
            ocr_jobs: None,
            is_monitoring: Arc::new(AtomicBool::new(true)),
            previous_foreground: AtomicIsize::new(0),
            dpi_scale: 1.0,
            app_icon_big: HICON::default(),
            app_icon_sm: HICON::default(),
            nid: NOTIFYICONDATAW::default(),
            last_text_hash: 0,
            last_image_hash: 0,
            ignore_blur_until: Instant::now(),
            search_sequence_id: Arc::new(AtomicU64::new(0)),
            pending_jev_results: Arc::new(Mutex::new(None)),
            pending_surrogate: None,
        })
    }

    pub fn prepare_show(&mut self) {
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
            self.pending_surrogate = None;
            *self.pending_jev_results.lock().unwrap() = None;
            let _ = KillTimer(self.hwnd, TIMER_BLINK_ID);
            self.ui.jev_status = JevStatus::Idle;
            self.ui.jev_result = None;
            let _ = KillTimer(self.hwnd, TIMER_RERANK_DEBOUNCE_ID);
            self.search_sequence_id.fetch_add(1, Ordering::SeqCst);
            self.ui.set_clips(clips, Some(0));

            // Ignore blur for 250ms to allow smooth focus transition
            self.ignore_blur_until = Instant::now() + std::time::Duration::from_millis(250);
        }
    }

    pub fn calculate_window_rect(&self) -> (i32, i32, i32, i32) {
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

            (x, y, width, height)
        }
    }

    pub fn get_clipboard_source_app() -> Option<String> {
        // SAFETY: Win32 FFI queries clipboard owner / foreground HWND and its thread process ID.
        // Stack buffer passed to QueryFullProcessImageNameW has fixed capacity and length bounds.
        // The process handle opened with PROCESS_QUERY_LIMITED_INFORMATION is closed via CloseHandle.
        unsafe {
            let hwnd = GetClipboardOwner()
                .ok()
                .filter(|h| !h.0.is_null())
                .unwrap_or_else(|| GetForegroundWindow());
            if hwnd.0.is_null() {
                return None;
            }

            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&mut pid));
            if pid == 0 || pid == std::process::id() {
                return None;
            }

            let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
                return None;
            };

            let mut buf = [0u16; 1024];
            let mut len = buf.len() as u32;
            let success = QueryFullProcessImageNameW(
                process,
                PROCESS_NAME_FORMAT(0),
                windows::core::PWSTR(buf.as_mut_ptr()),
                &mut len,
            ).is_ok();
            let _ = CloseHandle(process);

            if !success || len == 0 {
                return None;
            }

            let full_path = String::from_utf16_lossy(&buf[..len as usize]);
            Path::new(&full_path)
                .file_stem()
                .and_then(|s| s.to_str())
                .map(|s| s.to_string())
        }
    }

    pub fn on_clipboard_update(&mut self) {
        if !self.is_monitoring.load(Ordering::Relaxed) {
            return;
        }

        let Ok(mut clipboard) = Clipboard::new() else {
            return;
        };

        let source_app = Self::get_clipboard_source_app();

        // 1. Try reading image
        if let Ok(img) = clipboard.get_image() {
            let hash = xxh3_128(&img.bytes);
            if hash != self.last_image_hash {
                let hash_str = format!("{:032x}", hash);
                let (w, h) = (img.width as u32, img.height as u32);

                if let Ok(Some(existing)) = self.db.find_image_by_hash(&hash_str) {
                    if self.db.bump_clip_timestamp(&existing.id, source_app.as_deref()).is_err() {
                        return;
                    }
                } else {
                    let id = Uuid::new_v4().to_string();
                    let filename = format!("{}.png", id);
                    let images_dir = self.ui.data_dir.join("images");
                    let _ = fs::create_dir_all(&images_dir);
                    let full_path = images_dir.join(&filename);

                    if save_image_fast_png(&full_path, &img.bytes, w, h).is_ok() {
                        let full_path_str = full_path.to_string_lossy().replace('\\', "/");
                        let item = ClipItem {
                            id: id.clone(),
                            text: format!("[Image {}x{}]", w, h),
                            full_text_len: 0,
                            is_favorite: false,
                            clip_type: "image".to_string(),
                            image_path: Some(full_path_str),
                            image_width: Some(w),
                            image_height: Some(h),
                            ocr_text: None,
                            created_at: SystemTime::now()
                                .duration_since(UNIX_EPOCH)
                                .unwrap_or_default()
                                .as_millis() as i64,
                            source_app: source_app.clone(),
                        };
                        if self.db.insert_image_clip(&item, &hash_str).is_err() {
                            let _ = fs::remove_file(&full_path);
                            return;
                        }
                        if let Some(jobs) = &self.ocr_jobs {
                            // OCR is optional: keep the clip even if the bounded queue is full.
                            let _ = jobs.try_send((id, full_path));
                        }
                    } else {
                        let _ = fs::remove_file(&full_path);
                        return;
                    }
                }
                self.last_image_hash = hash;
                self.last_text_hash = 0;

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
            return;
        }

        // 2. Try reading text if not an image
        {
            if let Ok(text) = clipboard.get_text() {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    let hash = xxh3_128(text.as_bytes());
                    if hash != self.last_text_hash {
                        let hash_str = format!("{:032x}", hash);
                        if self.db.save_or_bump_text(text, &hash_str, source_app.as_deref()).is_err() {
                            return;
                        }
                        self.last_text_hash = hash;
                        self.last_image_hash = 0;

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
                let is_search = !self.ui.search_query.trim().is_empty();
                let clips = visible_clips(&self.db, &self.ui);
                let current_id = self.ui.selected_clip().map(|c| c.id.clone());
                let effective_idx = select_idx
                    .or_else(|| {
                        current_id.as_ref().and_then(|id| {
                            clips.iter().position(|c| &c.id == id)
                        })
                    })
                    .or(if is_search { Some(0) } else { None });
                self.ui.set_clips(clips, effective_idx);
                let _ = InvalidateRect(self.hwnd, None, false);
            }
        }
    }

    /// Call after every edit to the search text.
    pub fn query_changed(&mut self) {
        self.schedule_jev_search();
        self.refresh_ui_clips_with_selection(Some(0));
    }

    fn schedule_jev_search(&mut self) {
        // Invalidate in-flight work immediately, including during the debounce interval.
        self.search_sequence_id.fetch_add(1, Ordering::SeqCst);
        *self.pending_jev_results.lock().unwrap() = None;
        unsafe {
            if self.ui.mode != UiMode::SearchList {
                let _ = KillTimer(self.hwnd, TIMER_BLINK_ID);
            }
            let _ = KillTimer(self.hwnd, TIMER_RERANK_DEBOUNCE_ID);
        }
        let query = crate::typesafe::semantic_query(self.ui.search_query.trim())
            .filter(|_| self.ui.mode == UiMode::SearchList);
        let has_key = query.is_some() && self.ui.settings.get_typesafe_key().is_some();
        self.ui.jev_status = jev_status_for(query, self.ui.settings.enable_semantic_search, has_key);
        // Only an in-progress search keeps the previous result on screen.
        if self.ui.jev_status != JevStatus::Searching {
            self.ui.jev_result = None;
        } else {
            unsafe {
                let _ = SetTimer(self.hwnd, TIMER_RERANK_DEBOUNCE_ID, 300, None);
            }
        }
    }

    fn start_jev_search(&mut self) {
        if self.ui.mode != UiMode::SearchList || self.ui.jev_status != JevStatus::Searching {
            return;
        }
        let Some(query) = crate::typesafe::semantic_query(self.ui.search_query.trim()).map(str::to_string) else {
            return;
        };
        let Some(api_key) = self.ui.settings.get_typesafe_key() else {
            return;
        };
        // The sequence was bumped when the query last changed; any later edit supersedes this run.
        let seq = self.search_sequence_id.load(Ordering::SeqCst);
        let db = self.db.clone();
        let seq_id_arc = self.search_sequence_id.clone();
        let pending_arc = self.pending_jev_results.clone();
        let hwnd_raw = self.hwnd.0 as isize;

        // ponytail: superseded requests run to completion and are discarded; cancel the WinHTTP handle if concurrent requests pile up.
        let spawned = std::thread::Builder::new().name("clipped-jev".into()).spawn(move || {
            let apps = db.get_source_apps().unwrap_or_default();
            let outcome = crate::typesafe::search(
                &query,
                &apps,
                |body| crate::typesafe::post_system_one(&api_key, body),
                |filter| db.get_filtered_clips(filter, false).unwrap_or_default(),
            );
            if let Err(err) = &outcome {
                eprintln!("Jev search error: {err}");
            }
            {
                let mut lock = pending_arc.lock().unwrap();
                if seq_id_arc.load(Ordering::SeqCst) == seq {
                    *lock = Some((seq, outcome));
                }
            }
            unsafe {
                let _ = PostMessageW(
                    HWND(hwnd_raw as *mut _), WM_RERANK_COMPLETE,
                    WPARAM(seq as usize), LPARAM(0),
                );
            }
        });
        if let Err(err) = spawned {
            self.ui.jev_status = JevStatus::Failed(format!("Could not start Jev search: {err}"));
            self.ui.jev_result = None;
            self.refresh_ui_clips_with_selection(Some(0));
        }
    }

    pub fn delete_selected(&mut self) {
        if self.ui.mode == UiMode::Settings {
            return;
        }
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
            self.last_text_hash = 0;
            self.last_image_hash = 0;
            self.d2d.clear_bitmap_cache();
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

}

unsafe fn show_modal(state_ptr: *mut AppState) {
    if state_ptr.is_null() {
        return;
    }
    let (hwnd, (x, y, width, height)) = {
        let state = &mut *state_ptr;
        state.prepare_show();
        (state.hwnd, state.calculate_window_rect())
    };

    let _ = SetWindowPos(
        hwnd,
        HWND_TOPMOST,
        x,
        y,
        width,
        height,
        SWP_NOACTIVATE | SWP_FRAMECHANGED,
    );
    let _ = ShowWindow(hwnd, SW_SHOW);
    let _ = SetForegroundWindow(hwnd);
    let _ = SetFocus(hwnd);
    let _ = InvalidateRect(hwnd, None, false);
}

unsafe fn hide_modal(state_ptr: *mut AppState) {
    if state_ptr.is_null() {
        return;
    }
    let hwnd = (*state_ptr).hwnd;
    if !IsWindowVisible(hwnd).as_bool() {
        return;
    }

    {
        let state = &mut *state_ptr;
        let _ = KillTimer(hwnd, TIMER_BLINK_ID);
        let _ = KillTimer(hwnd, TIMER_RERANK_DEBOUNCE_ID);
        state.search_sequence_id.fetch_add(1, Ordering::SeqCst);
        *state.pending_jev_results.lock().unwrap() = None;
        state.pending_surrogate = None;
        state.ui.set_clips(Vec::new(), Some(0));
        state.ui.mode = UiMode::SingleCard;
        state.ui.search_query.clear();
        state.ui.jev_status = JevStatus::Idle;
        state.ui.jev_result = None;
        state.ui.hover_target = HitTarget::None;

        // Evict cached Direct2D textures from VRAM/RAM
        state.d2d.clear_bitmap_cache();

        // Memory trim on modal close
        state.db.shrink_memory();
    }

    let _ = ShowWindow(hwnd, SW_HIDE);
    let _ = EmptyWorkingSet(GetCurrentProcess());
}

unsafe fn show_settings(state_ptr: *mut AppState) {
    show_modal(state_ptr);
    if !state_ptr.is_null() {
        let state = &mut *state_ptr;
        state.ui.mode = UiMode::Settings;
        let _ = InvalidateRect(state.hwnd, None, false);
    }
}

unsafe fn show_tray_menu(state_ptr: *mut AppState) {
    if state_ptr.is_null() {
        return;
    }
    let hwnd = (*state_ptr).hwnd;
    let Ok(hmenu) = CreatePopupMenu() else {
        return;
    };

    let _ = AppendMenuW(hmenu, MF_STRING, ID_TRAY_OPEN, w!("Open Clipped"));
    let _ = AppendMenuW(hmenu, MF_STRING, ID_TRAY_SETTINGS, w!("Settings"));
    let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
    let _ = AppendMenuW(hmenu, MF_STRING, ID_TRAY_EXIT, w!("Exit"));

    let mut pt = POINT::default();
    let _ = GetCursorPos(&mut pt);

    let _ = SetForegroundWindow(hwnd);
    let _ = TrackPopupMenuEx(
        hmenu,
        (TPM_RIGHTBUTTON | TPM_BOTTOMALIGN).0,
        pt.x,
        pt.y,
        hwnd,
        None,
    );
    let _ = DestroyMenu(hmenu);
}

unsafe fn paste_selected(state_ptr: *mut AppState) {
    if state_ptr.is_null() {
        return;
    }
    let paste_data = {
        let state = &mut *state_ptr;
        if state.ui.mode == UiMode::Settings {
            return;
        }
        let Some(clip) = state.ui.selected_clip() else {
            return;
        };
        let prev_hwnd = state.previous_foreground.load(Ordering::Relaxed);
        let data_dir = state.ui.data_dir.clone();

        let Ok(Some(clip_to_paste)) = state.db.get_full_clip(&clip.id) else {
            return;
        };
        if state.is_monitoring.compare_exchange(true, false, Ordering::AcqRel, Ordering::Acquire).is_err() {
            return;
        }
        Some((clip_to_paste, data_dir, prev_hwnd, state.is_monitoring.clone()))
    };

    let Some((clip_to_paste, data_dir, prev_hwnd, is_monitoring)) = paste_data else {
        return;
    };

    hide_modal(state_ptr);

    let mon = is_monitoring.clone();
    if let Err(err) = std::thread::Builder::new().name("clipped-paste".into()).spawn(move || {
        if let Err(err) = perform_paste(&clip_to_paste, &data_dir, prev_hwnd, &mon) {
            eprintln!("Paste failed: {err}");
        }
    }) {
        is_monitoring.store(true, Ordering::Release);
        eprintln!("Could not start paste: {err}");
    }
}

pub fn run_app() -> windows::core::Result<()> {
    unsafe {
        // Enforce single running instance
        let mutex_name = w!("Local\\Clipped_SingleInstance_Mutex");
        let _single_instance_mutex = windows::core::Owned::new(CreateMutexW(None, false, mutex_name)?);
        if GetLastError() == ERROR_ALREADY_EXISTS {
            let class_name = w!("ClippedWindowClass");
            if let Ok(existing_hwnd) = FindWindowW(class_name, None) {
                if !existing_hwnd.0.is_null() {
                    let _ = PostMessageW(existing_hwnd, WM_ACTIVATE_INSTANCE, WPARAM(0), LPARAM(0));
                    let _ = SetForegroundWindow(existing_hwnd);
                }
            }
            return Ok(());
        }

        // Initialize COM apartment
        let _com = crate::ocr::ComApartment::new(COINIT_APARTMENTTHREADED)?;

        // Set Per-Monitor V2 DPI awareness
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);

        // Explicit AppUserModelID for Windows Taskbar and Task Manager association
        let _ = SetCurrentProcessExplicitAppUserModelID(w!("Alexis.Clipped.App"));

        let data_dir = get_data_dir();
        fs::create_dir_all(&data_dir).map_err(|e| windows::core::Error::new(E_FAIL, e.to_string()))?;
        let settings = load_settings(&data_dir);
        let db = Database::init(&data_dir).map_err(|e| windows::core::Error::new(E_FAIL, e.to_string()))?;
        db.reconcile_orphaned_images();
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
        if RegisterClassExW(&wnd_class) == 0 {
            return Err(windows::core::Error::from_win32());
        }

        let mut app_state = app_state;
        app_state.app_icon_big = app_icon_big;
        app_state.app_icon_sm = app_icon_sm;

        // Keep ownership outside the HWND so creation failures and destruction
        // cannot lose the allocation. The address remains stable for callbacks.
        let mut state_box = Box::new(app_state);
        let state_ptr: *mut AppState = &mut *state_box;

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
        (*state_ptr).ocr_jobs = crate::ocr::start_worker((*state_ptr).db.clone(), hwnd).ok();

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

        // Initial memory purge
        let _ = EmptyWorkingSet(GetCurrentProcess());

        println!("Clipped (Pure Win32 + Direct2D) running with ~3 MB memory footprint!");

        let is_autostart = std::env::args().any(|a| a == "--autostart" || a == "--minimized");

        if (*state_ptr).ui.settings.launch_on_boot {
            set_launch_on_boot(true);
        }

        if !is_autostart {
            show_modal(state_ptr);
        }

        // Message Loop
        let mut msg = MSG::default();
        let loop_result = loop {
            match GetMessageW(&mut msg, None, 0, 0).0 {
                -1 => break Err(windows::core::Error::from_win32()),
                0 => break Ok(()),
                _ => {}
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        };

        // Cleanup
        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
        let _ = RemoveClipboardFormatListener(hwnd);
        let _ = UnregisterHotKey(hwnd, HOTKEY_ID);

        if IsWindow(hwnd).as_bool() {
            SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
            let _ = DestroyWindow(hwnd);
        }
        (*state_ptr).search_sequence_id.fetch_add(1, Ordering::SeqCst);
        (*state_ptr).ocr_jobs.take();
        drop(state_box);
        loop_result?;
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

    match msg {
        WM_GETICON => {
            let icon = {
                let state = &*state_ptr;
                match wparam.0 as u32 {
                    ICON_BIG => state.app_icon_big,
                    _ => state.app_icon_sm,
                }
            };
            LRESULT(icon.0 as isize)
        }

        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let _ = BeginPaint(hwnd, &mut ps);

            {
                let state = &mut *state_ptr;
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
                    if state.d2d.end_draw().is_err() {
                        state.d2d.discard_target();
                    }
                }
            }

            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }

        WM_HOTKEY => {
            if (wparam.0 as i32) == HOTKEY_ID {
                if IsWindowVisible(hwnd).as_bool() {
                    let state = &mut *state_ptr;
                    let len = state.ui.clips.len();
                    if len > 0 {
                        state.ui.selected_index = (state.ui.selected_index + 1) % len;
                        state.ui.ensure_selected_visible();
                        let _ = InvalidateRect(hwnd, None, false);
                    }
                } else {
                    show_modal(state_ptr);
                }
            }
            LRESULT(0)
        }

        WM_CLIPBOARDUPDATE => {
            let state = &mut *state_ptr;
            state.on_clipboard_update();
            LRESULT(0)
        }

        WM_OCR_COMPLETE => {
            let state = &mut *state_ptr;
            state.refresh_ui_clips();
            LRESULT(0)
        }

        WM_ACTIVATE_INSTANCE => {
            show_modal(state_ptr);
            LRESULT(0)
        }

        WM_TRAYICON => {
            let event = (lparam.0 & 0xFFFF) as u32;
            if event == WM_LBUTTONUP {
                if IsWindowVisible(hwnd).as_bool() {
                    hide_modal(state_ptr);
                } else {
                    show_modal(state_ptr);
                }
            } else if event == WM_RBUTTONUP {
                show_tray_menu(state_ptr);
            }
            LRESULT(0)
        }

        WM_COMMAND => {
            let cmd_id = wparam.0 & 0xFFFF;
            match cmd_id {
                ID_TRAY_OPEN => show_modal(state_ptr),
                ID_TRAY_SETTINGS => show_settings(state_ptr),
                ID_TRAY_EXIT => {
                    let _ = DestroyWindow(hwnd);
                }
                _ => {}
            }
            LRESULT(0)
        }

        WM_ACTIVATE => {
            let active = (wparam.0 & 0xFFFF) as u32;
            let should_hide = {
                let state = &*state_ptr;
                active == WA_INACTIVE && Instant::now() > state.ignore_blur_until
            };
            if should_hide {
                hide_modal(state_ptr);
            }
            LRESULT(0)
        }

        WM_KILLFOCUS => {
            let should_hide = {
                let state = &*state_ptr;
                Instant::now() > state.ignore_blur_until
            };
            if should_hide {
                hide_modal(state_ptr);
            }
            LRESULT(0)
        }

        WM_DPICHANGED => {
            let new_dpi = (wparam.0 & 0xFFFF) as u32;
            let (x, y, width, height) = {
                let state = &mut *state_ptr;
                state.dpi_scale = (new_dpi as f32) / 96.0;
                state.calculate_window_rect()
            };
            let _ = SetWindowPos(
                hwnd,
                HWND_TOPMOST,
                x,
                y,
                width,
                height,
                SWP_NOACTIVATE | SWP_FRAMECHANGED,
            );
            let _ = InvalidateRect(hwnd, None, false);
            LRESULT(0)
        }

        WM_TIMER => {
            if wparam.0 == TIMER_BLINK_ID {
                let is_search = {
                    let state = &*state_ptr;
                    state.ui.mode == UiMode::SearchList
                };
                if is_search && IsWindowVisible(hwnd).as_bool() {
                    let _ = InvalidateRect(hwnd, None, false);
                }
            } else if wparam.0 == TIMER_RERANK_DEBOUNCE_ID {
                let _ = KillTimer(hwnd, TIMER_RERANK_DEBOUNCE_ID);
                let state = &mut *state_ptr;
                state.start_jev_search();
            }
            LRESULT(0)
        }

        WM_MOUSEMOVE => {
            let state = &mut *state_ptr;
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
            let state = &mut *state_ptr;
            if state.ui.hover_target != HitTarget::None {
                state.ui.hover_target = HitTarget::None;
                let _ = InvalidateRect(hwnd, None, false);
            }
            LRESULT(0)
        }

        WM_LBUTTONDOWN => {
            let px = (lparam.0 & 0xFFFF) as i16 as f32;
            let py = ((lparam.0 >> 16) & 0xFFFF) as i16 as f32;
            let hit = {
                let state = &*state_ptr;
                let lx = px / state.dpi_scale;
                let ly = py / state.dpi_scale;
                state.ui.hit_test(lx, ly)
            };

            match hit {
                HitTarget::FavFilter => {
                    let state = &mut *state_ptr;
                    state.ui.show_favorites = !state.ui.show_favorites;
                    state.refresh_ui_clips_with_selection(Some(0));
                }
                HitTarget::SearchBox => {
                    let state = &mut *state_ptr;
                    if state.ui.mode == UiMode::SingleCard {
                        state.ui.mode = UiMode::SearchList;
                        state.ui.selected_index = 0;
                        state.ui.ensure_selected_visible();
                        let _ = InvalidateRect(hwnd, None, false);
                    }
                }
                HitTarget::CloseSettings => {
                    let state = &mut *state_ptr;
                    state.ui.mode = UiMode::SingleCard;
                    let _ = InvalidateRect(hwnd, None, false);
                }
                HitTarget::DockTrash => {
                    let state = &mut *state_ptr;
                    state.delete_selected();
                }
                HitTarget::DockStar => {
                    let state = &mut *state_ptr;
                    state.toggle_favorite_selected();
                }
                HitTarget::ListItem(idx) => {
                    let should_paste = {
                        let state = &mut *state_ptr;
                        if state.ui.selected_index == idx {
                            true
                        } else {
                            state.ui.selected_index = idx;
                            let _ = InvalidateRect(hwnd, None, false);
                            false
                        }
                    };
                    if should_paste {
                        paste_selected(state_ptr);
                    }
                }
                HitTarget::SettingsBootToggle => {
                    let state = &mut *state_ptr;
                    state.ui.settings.launch_on_boot = !state.ui.settings.launch_on_boot;
                    set_launch_on_boot(state.ui.settings.launch_on_boot);
                    save_settings(&state.ui.data_dir, &state.ui.settings);
                    let _ = InvalidateRect(hwnd, None, false);
                }
                HitTarget::ClearNonFavorites => {
                    let state = &mut *state_ptr;
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
                    state.last_text_hash = 0;
                    state.last_image_hash = 0;
                    state.d2d.clear_bitmap_cache();
                    state.refresh_ui_clips_with_selection(Some(0));
                }
                HitTarget::ClearAll => {
                    let state = &mut *state_ptr;
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
                    state.last_text_hash = 0;
                    state.last_image_hash = 0;
                    state.d2d.clear_bitmap_cache();
                    state.refresh_ui_clips_with_selection(Some(0));
                }
                HitTarget::None => {}
            }
            LRESULT(0)
        }

        WM_MOUSEWHEEL => {
            let state = &mut *state_ptr;
            if state.ui.mode == UiMode::SearchList {
                let delta = ((wparam.0 >> 16) & 0xFFFF) as i16;
                let step = 56.0 * 2.0;
                let max_scroll = ((state.ui.clips.len() as f32) * 56.0 - 260.0).max(0.0);
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
                    let state = &mut *state_ptr;
                    if state.ui.mode != UiMode::Settings {
                        state.ui.show_favorites = !state.ui.show_favorites;
                        state.refresh_ui_clips_with_selection(Some(0));
                        let _ = InvalidateRect(hwnd, None, false);
                    }
                }
                VK_ESCAPE => {
                    let should_hide = {
                        let state = &mut *state_ptr;
                        if state.ui.mode == UiMode::Settings {
                            state.ui.mode = UiMode::SingleCard;
                            let _ = InvalidateRect(hwnd, None, false);
                            false
                        } else if state.ui.mode == UiMode::SearchList {
                            let had_query = !state.ui.search_query.is_empty();
                            state.ui.search_query.clear();
                            state.pending_surrogate = None;
                            if !had_query {
                                state.ui.mode = UiMode::SingleCard;
                            }
                            state.query_changed();
                            false
                        } else {
                            true
                        }
                    };
                    if should_hide {
                        hide_modal(state_ptr);
                    }
                }
                VK_RETURN => {
                    paste_selected(state_ptr);
                }
                VK_DELETE => {
                    let state = &mut *state_ptr;
                    state.delete_selected();
                }
                VK_UP | VK_LEFT => {
                    let state = &mut *state_ptr;
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
                    let state = &mut *state_ptr;
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
                    let state = &mut *state_ptr;
                    if state.ui.mode == UiMode::SearchList {
                        state.pending_surrogate = None;
                        if state.ui.search_query.pop().is_none() {
                            state.ui.mode = UiMode::SingleCard;
                        }
                        state.query_changed();
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }

        WM_CHAR => {
            let state = &mut *state_ptr;
            let Some(ch) = decode_input_unit(&mut state.pending_surrogate, wparam.0 as u16) else {
                return LRESULT(0);
            };
            if !ch.is_control() && state.ui.mode != UiMode::Settings {
                // In SingleCard mode, check for number navigation 1-9
                if state.ui.mode == UiMode::SingleCard && ('1'..='9').contains(&ch) {
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
                let _ = SetTimer(hwnd, TIMER_BLINK_ID, 500, None);
                state.query_changed();
            }
            LRESULT(0)
        }

        WM_RERANK_COMPLETE => {
            let state = &mut *state_ptr;
            if apply_jev_completion(
                &mut state.ui,
                wparam.0 as u64,
                state.search_sequence_id.load(Ordering::SeqCst),
                &state.pending_jev_results,
            ) {
                state.refresh_ui_clips_with_selection(Some(0));
            }
            let _ = InvalidateRect(hwnd, None, false);
            LRESULT(0)
        }

        WM_DESTROY => {
            let _ = KillTimer(hwnd, TIMER_BLINK_ID);
            let _ = KillTimer(hwnd, TIMER_RERANK_DEBOUNCE_ID);
            {
                let state = &mut *state_ptr;
                state.search_sequence_id.fetch_add(1, Ordering::SeqCst);
                *state.pending_jev_results.lock().unwrap() = None;
            }
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

fn decode_input_unit(pending: &mut Option<u16>, unit: u16) -> Option<char> {
    if (0xD800..=0xDBFF).contains(&unit) {
        *pending = Some(unit);
        return None;
    }
    let high = pending.take();
    if (0xDC00..=0xDFFF).contains(&unit) {
        let high = high?;
        char::from_u32(0x10000 + ((high as u32 - 0xD800) << 10) + unit as u32 - 0xDC00)
    } else {
        char::from_u32(unit as u32)
    }
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
    writer.flush()?;
    Ok(())
}

#[allow(clippy::manual_dangling_ptr)] // MAKEINTRESOURCEW(1), not a dereferenceable pointer.
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
    fn input_decodes_surrogate_pairs() {
        let mut pending = None;
        assert_eq!(decode_input_unit(&mut pending, 0xD83D), None);
        assert_eq!(decode_input_unit(&mut pending, 0xDE00), Some('\u{1f600}'));
        assert_eq!(decode_input_unit(&mut pending, 0xDE00), None);
        assert_eq!(decode_input_unit(&mut pending, b'a' as u16), Some('a'));
    }

    fn test_clip(id: &str, clip_type: &str, text: &str) -> ClipItem {
        ClipItem {
            id: id.into(),
            text: text.into(),
            clip_type: clip_type.into(),
            full_text_len: text.len(),
            ..ClipItem::default()
        }
    }

    #[test]
    fn test_jev_completion_applies_only_the_current_query() {
        let mut ui = crate::ui::UiState::new(PathBuf::new(), AppSettings::default());
        ui.mode = UiMode::SearchList;
        ui.jev_status = JevStatus::Searching;
        let newer = JevSearch { ranked_ids: Some(vec!["new".into()]), ..JevSearch::default() };
        let pending: JevResults = Mutex::new(Some((2, Ok(newer.clone()))));

        // A slow response for an older query arrives after the user kept typing.
        assert!(!apply_jev_completion(&mut ui, 1, 2, &pending));
        assert_eq!(ui.jev_status, JevStatus::Searching);
        assert!(ui.jev_result.is_none());
        assert!(pending.lock().unwrap().is_some(), "stale message must not consume the newer result");

        assert!(apply_jev_completion(&mut ui, 2, 2, &pending));
        assert_eq!(ui.jev_status, JevStatus::Ready);
        assert_eq!(ui.jev_result.as_ref(), Some(&newer));
        assert!(pending.lock().unwrap().is_none());
        // A duplicate completion message is a no-op.
        assert!(!apply_jev_completion(&mut ui, 2, 2, &pending));

        // A result stored for an older sequence is ignored even if the message says "current".
        *pending.lock().unwrap() = Some((3, Ok(JevSearch::default())));
        assert!(!apply_jev_completion(&mut ui, 4, 4, &pending));
        assert_eq!(ui.jev_result.as_ref(), Some(&newer));

        *pending.lock().unwrap() = Some((5, Err("Jev did not respond in time".into())));
        assert!(apply_jev_completion(&mut ui, 5, 5, &pending));
        assert_eq!(ui.jev_status, JevStatus::Failed("Jev did not respond in time".into()));
        assert!(ui.jev_result.is_none());
        assert_eq!(ui.empty_list_message(), "No matching clips found", "no `>` query, no Jev message");
        ui.search_query = "> all chrome clips".into();
        assert_eq!(ui.empty_list_message(), "Jev did not respond in time");
    }

    #[test]
    fn test_jev_status_for_mode_transitions() {
        use crate::typesafe::semantic_query;
        let status = |raw: &str| jev_status_for(semantic_query(raw), true, true);
        assert_eq!(status(""), JevStatus::Idle);
        assert_eq!(status("images"), JevStatus::Idle, "plain searches never call Jev");
        assert_eq!(status(">"), JevStatus::Idle);
        assert_eq!(status("> a"), JevStatus::Idle);
        assert_eq!(status("> Show me all the images"), JevStatus::Searching);
        assert_eq!(status(">All Chrome clips"), JevStatus::Searching);
        assert!(matches!(
            jev_status_for(semantic_query("> images"), true, false),
            JevStatus::Failed(msg) if msg.contains("API key")
        ));
        assert!(matches!(
            jev_status_for(semantic_query("> images"), false, true),
            JevStatus::Failed(msg) if msg.contains("turned off")
        ));
        // Missing key only matters once Jev would actually run.
        assert_eq!(jev_status_for(semantic_query("images"), true, false), JevStatus::Idle);
    }

    #[test]
    fn test_visible_clips_per_mode() {
        use crate::typesafe::{AppFilter, ClipFilter};
        let dir = std::env::temp_dir().join(format!("clipped_app_test_{}", Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        let db = Database::init(&dir).unwrap();
        db.save_or_bump_text("chrome notes".into(), "h1", Some("chrome")).unwrap();
        db.save_or_bump_text("images are great".into(), "h2", Some("code")).unwrap();
        let mut image = test_clip("img", "image", "[Image]");
        image.source_app = Some("chrome".into());
        db.insert_image_clip(&image, "h3").unwrap();

        let mut ui = crate::ui::UiState::new(PathBuf::new(), AppSettings::default());
        ui.mode = UiMode::SearchList;
        let ids = |ui: &crate::ui::UiState| {
            let mut ids: Vec<_> = visible_clips(&db, ui).into_iter().map(|c| c.text).collect();
            ids.sort();
            ids
        };

        // Plain keyword search is untouched by Jev mode.
        ui.search_query = "images".into();
        assert_eq!(ids(&ui), ["images are great"]);

        // `>` alone and the first in-flight search keep showing history, not a blank list.
        ui.search_query = ">".into();
        assert_eq!(ids(&ui).len(), 3);
        ui.search_query = "> Show me all the images".into();
        ui.jev_status = JevStatus::Searching;
        assert_eq!(ids(&ui).len(), 3);

        // A finished search is re-applied to the live database.
        ui.jev_status = JevStatus::Ready;
        ui.jev_result = Some(JevSearch {
            filter: ClipFilter { format: Some("image"), ..ClipFilter::default() },
            ranked_ids: None,
        });
        assert_eq!(ids(&ui), ["[Image]"]);
        ui.search_query = "> All Chrome clips".into();
        ui.jev_result = Some(JevSearch {
            filter: ClipFilter { app: AppFilter::App("chrome".into()), ..ClipFilter::default() },
            ranked_ids: None,
        });
        assert_eq!(ids(&ui), ["[Image]", "chrome notes"]);
        // Toggling the favorites filter re-filters locally.
        ui.show_favorites = true;
        assert!(ids(&ui).is_empty());
        ui.show_favorites = false;

        // Failures show an empty list (with the error message), never unrelated history.
        ui.jev_status = JevStatus::Failed("TypeSafe rejected the API key".into());
        ui.jev_result = None;
        assert!(ids(&ui).is_empty());
        assert_eq!(ui.empty_list_message(), "TypeSafe rejected the API key");

        drop(db);
        let _ = fs::remove_dir_all(dir);
    }

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
