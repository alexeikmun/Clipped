use std::path::Path;

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
