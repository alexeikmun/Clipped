use std::path::Path;
use std::path::PathBuf;
use std::sync::{mpsc, Arc};
use crate::db::Database;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Com::{CoInitializeEx, CoUninitialize, COINIT, COINIT_MULTITHREADED};
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

pub(crate) struct ComApartment;

impl ComApartment {
    pub(crate) fn new(mode: COINIT) -> windows::core::Result<Self> {
        unsafe { CoInitializeEx(None, mode).ok()?; }
        Ok(Self)
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize(); }
    }
}

pub(crate) fn start_worker(
    db: Arc<Database>,
    hwnd: HWND,
) -> std::io::Result<mpsc::SyncSender<(String, PathBuf)>> {
    let (sender, receiver) = mpsc::sync_channel::<(String, PathBuf)>(8);
    let hwnd = hwnd.0 as isize;
    std::thread::Builder::new().name("clipped-ocr".into()).spawn(move || {
        let Ok(_com) = ComApartment::new(COINIT_MULTITHREADED) else { return; };
        while let Ok((id, path)) = receiver.recv() {
            // A clip may have been deleted while waiting for OCR.
            if !matches!(db.get_full_clip(&id), Ok(Some(_))) {
                continue;
            }
            if let Some(text) = extract_ocr_text(&path) {
                if db.update_ocr(&id, &text).is_ok() {
                    unsafe {
                        let _ = PostMessageW(
                            HWND(hwnd as _), crate::app::WM_OCR_COMPLETE, WPARAM(0), LPARAM(0),
                        );
                    }
                }
            }
        }
    })?;
    Ok(sender)
}

#[cfg(target_os = "windows")]
pub fn extract_ocr_text(image_path: &Path) -> Option<String> {
    use windows::Graphics::Imaging::BitmapDecoder;
    use windows::Media::Ocr::OcrEngine;
    use windows::Storage::StorageFile;

    let path_str = image_path.canonicalize().ok()?.to_string_lossy().to_string();
    let clean_path = path_str.strip_prefix(r"\\?\").unwrap_or(&path_str);
    let file = StorageFile::GetFileFromPathAsync(&windows::core::HSTRING::from(clean_path)).ok()?.get().ok()?;
    let stream = file.OpenAsync(windows::Storage::FileAccessMode::Read).ok()?.get().ok()?;
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
pub fn extract_ocr_text(_image_path: &Path) -> Option<String> {
    None
}
