use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
use arboard::Clipboard;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, KEYEVENTF_KEYUP, VK_CONTROL, VK_MENU, VK_SHIFT, VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::SetForegroundWindow;

use crate::db::ClipItem;

pub fn perform_paste(
    item: &ClipItem,
    data_dir: &Path,
    previous_hwnd: isize,
    is_monitoring: &AtomicBool,
) {
    // Temporarily pause clipboard change monitoring so our own paste doesn't re-trigger a new clip
    is_monitoring.store(false, Ordering::Relaxed);

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

    if let Ok(mut clipboard) = Clipboard::new() {
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
    }

    // Restore focus to target application
    if previous_hwnd != 0 {
        unsafe {
            let target = HWND(previous_hwnd as _);
            let _ = SetForegroundWindow(target);
        }
    }

    thread::sleep(Duration::from_millis(60));

    unsafe {
        // Release any modifiers that might be held from global hotkey
        keybd_event(VK_MENU.0 as u8, 0, KEYEVENTF_KEYUP, 0);
        keybd_event(VK_SHIFT.0 as u8, 0, KEYEVENTF_KEYUP, 0);
        keybd_event(VK_CONTROL.0 as u8, 0, KEYEVENTF_KEYUP, 0);

        // Synthesize Ctrl+V
        keybd_event(VK_CONTROL.0 as u8, 0, windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(0), 0);
        keybd_event(VK_V.0 as u8, 0, windows::Win32::UI::Input::KeyboardAndMouse::KEYBD_EVENT_FLAGS(0), 0);
        keybd_event(VK_V.0 as u8, 0, KEYEVENTF_KEYUP, 0);
        keybd_event(VK_CONTROL.0 as u8, 0, KEYEVENTF_KEYUP, 0);
    }

    // Give the target app a moment to receive the paste before resuming clipboard monitoring
    thread::sleep(Duration::from_millis(250));
    is_monitoring.store(true, Ordering::Relaxed);
}
