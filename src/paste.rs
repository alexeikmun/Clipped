use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::Duration;
use arboard::Clipboard;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    VK_CONTROL, VK_MENU, VK_SHIFT, VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, IsWindow, SetForegroundWindow};

use crate::db::ClipItem;

pub fn perform_paste(
    item: &ClipItem,
    data_dir: &Path,
    previous_hwnd: isize,
    is_monitoring: &AtomicBool,
) -> Result<(), String> {
    let _resume = ResumeMonitoring(is_monitoring);
    let image = load_paste_image(item, data_dir)?;
    let target = HWND(previous_hwnd as _);
    unsafe {
        if previous_hwnd == 0 || !IsWindow(target).as_bool() {
            return Err("Paste target is no longer available".into());
        }
    }
    let mut clipboard = Clipboard::new().map_err(|e| e.to_string())?;
    if let Some(rgba) = image {
        let (w, h) = rgba.dimensions();
        clipboard.set_image(arboard::ImageData {
            width: w as usize,
            height: h as usize,
            bytes: rgba.into_raw().into(),
        }).map_err(|e| e.to_string())?;
    } else {
        clipboard.set_text(&item.text).map_err(|e| e.to_string())?;
    }

    // Restore focus to target application
    unsafe {
        if GetForegroundWindow() != target && !SetForegroundWindow(target).as_bool() {
            return Err("Could not focus paste target".into());
        }
    }

    thread::sleep(Duration::from_millis(60));

    unsafe {
        if GetForegroundWindow() != target {
            return Err("Paste target lost focus".into());
        }
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
        if SendInput(&releases, std::mem::size_of::<INPUT>() as i32) != releases.len() as u32 {
            return Err("Could not release paste modifiers".into());
        }

        // 2. Synthesize atomic Ctrl+V keystroke
        let paste_inputs = [
            make_key(VK_CONTROL, false),
            make_key(VK_V, false),
            make_key(VK_V, true),
            make_key(VK_CONTROL, true),
        ];
        if SendInput(&paste_inputs, std::mem::size_of::<INPUT>() as i32) != paste_inputs.len() as u32 {
            let _ = SendInput(
                &[make_key(VK_V, true), make_key(VK_CONTROL, true)],
                std::mem::size_of::<INPUT>() as i32,
            );
            return Err("Could not send paste keystrokes".into());
        }
    }

    // Give the target app a moment to receive the paste before resuming clipboard monitoring
    thread::sleep(Duration::from_millis(250));
    Ok(())
}

struct ResumeMonitoring<'a>(&'a AtomicBool);

impl Drop for ResumeMonitoring<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

fn load_paste_image(item: &ClipItem, data_dir: &Path) -> Result<Option<image::RgbaImage>, String> {
    if item.clip_type != "image" {
        return Ok(None);
    }
    let path = item.image_path.as_deref().ok_or("Image clip has no file")?;
    let path = data_dir.join(path);
    image::open(path).map(|image| Some(image.into_rgba8())).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn monitoring_resumes_on_error() {
        let monitoring = AtomicBool::new(false);
        {
            let _guard = ResumeMonitoring(&monitoring);
        }
        assert!(monitoring.load(Ordering::Acquire));
    }

    #[test]
    fn missing_image_is_not_pasted_as_text() {
        let item: ClipItem = serde_json::from_value(serde_json::json!({
            "id": "missing", "text": "[Image]", "is_favorite": false, "clip_type": "image"
        })).unwrap();
        assert!(load_paste_image(&item, Path::new(".")).is_err());
    }
}
