use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
use arboard::Clipboard;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VK_CONTROL, VK_MENU, VK_SHIFT, VK_V,
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
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYBD_EVENT_FLAGS,
            KEYEVENTF_KEYUP, VK_LWIN, VK_RWIN,
        };

        let make_key = |vk, keyup: bool| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: vk,
                    wScan: 0,
                    dwFlags: if keyup { KEYEVENTF_KEYUP } else { KEYBD_EVENT_FLAGS(0) },
                    time: 0,
                    dwExtraInfo: 0,
                },
            },
        };

        // 1. Release all modifier keys that could be held from the global hotkey
        let releases = [
            make_key(VK_MENU, true),
            make_key(VK_SHIFT, true),
            make_key(VK_CONTROL, true),
            make_key(VK_LWIN, true),
            make_key(VK_RWIN, true),
        ];
        SendInput(&releases, std::mem::size_of::<INPUT>() as i32);

        // 2. Synthesize atomic Ctrl+V keystroke
        let paste_inputs = [
            make_key(VK_CONTROL, false),
            make_key(VK_V, false),
            make_key(VK_V, true),
            make_key(VK_CONTROL, true),
        ];
        SendInput(&paste_inputs, std::mem::size_of::<INPUT>() as i32);
    }

    // Give the target app a moment to receive the paste before resuming clipboard monitoring
    thread::sleep(Duration::from_millis(250));
    is_monitoring.store(true, Ordering::Relaxed);
}
