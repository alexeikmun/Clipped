#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod app;
mod d2d;
mod db;
mod model;
mod ocr;
mod paste;
mod syntax;
mod ui;

fn main() -> windows::core::Result<()> {
    let args: Vec<String> = std::env::args().collect();
    if let Some(pos) = args.iter().position(|a| a == "--export-png") {
        if let Some(path_str) = args.get(pos + 1) {
            let mut d2d = d2d::D2dContext::new()?;
            let mut ui = ui::UiState::new(std::path::PathBuf::from(".clipped"), model::AppSettings::default());
            let sample_json = "{\n  \"name\": \"clipped-app\",\n  \"private\": true,\n  \"version\": \"0.2.0\",\n  \"type\": \"module\",\n  \"scripts\": {\n    \"dev\": \"vite\",\n    \"build\": \"tsc && vite build\",\n    \"preview\": \"vite preview\",\n    \"tauri\": \"tauri\",\n    \"build:release\": \"tauri build && node scripts/copy-release.js\"\n  },\n  \"dependencies\": {\n    \"@tauri-apps/api\": \"^2.1.1\",\n    \"@tauri-apps/plugin-clipboard-manager\": \"^2.0.1\",\n    \"@tauri-apps/plugin-global-shortcut\": \"^2.0.1\",\n    \"@tauri-apps/plugin-shell\": \"^2.0.1\"\n  }\n}";
            let item1 = db::ClipItem {
                id: "1".into(),
                text: sample_json.into(),
                is_favorite: false,
                clip_type: "text".into(),
                image_path: None,
                image_width: None,
                image_height: None,
                ocr_text: None,
                full_text_len: sample_json.len(),
            };
            let item2 = db::ClipItem {
                id: "2".into(),
                text: "Second clip item".into(),
                is_favorite: true,
                clip_type: "text".into(),
                image_path: None,
                image_width: None,
                image_height: None,
                ocr_text: None,
                full_text_len: 16,
            };
            ui.set_clips(vec![item1.clone(), item2.clone()], Some(0));
            if let Err(_e) = d2d.export_ui_to_png(&mut ui, model::WINDOW_WIDTH as u32, model::WINDOW_HEIGHT as u32, std::path::Path::new(path_str)) {
            } else {
                println!("Exported UI to {}", path_str);
            }
            return Ok(());
        }
    }
    if let Some(pos) = args.iter().position(|a| a == "--export-starred") {
        if let Some(path_str) = args.get(pos + 1) {
            let mut d2d = d2d::D2dContext::new()?;
            let mut ui = ui::UiState::new(std::path::PathBuf::from(".clipped"), model::AppSettings::default());
            let sample_json = "{\n  \"name\": \"clipped-app\",\n  \"private\": true,\n  \"version\": \"0.2.0\"\n}";
            let item1 = db::ClipItem {
                id: "1".into(),
                text: sample_json.into(),
                is_favorite: false,
                clip_type: "text".into(),
                image_path: None,
                image_width: None,
                image_height: None,
                ocr_text: None,
                full_text_len: sample_json.len(),
            };
            let sample_code = "fn handle_paste(id: &str) -> Result<bool> {\n    // Auto-paste into active window\n    let item = db.get_clip(id)?;\n    enigo.key_sequence(\"paste\");\n    Ok(true)\n}";
            let item2 = db::ClipItem {
                id: "2".into(),
                text: sample_code.into(),
                is_favorite: true,
                clip_type: "text".into(),
                image_path: None,
                image_width: None,
                image_height: None,
                ocr_text: None,
                full_text_len: sample_code.len(),
            };
            ui.set_clips(vec![item1, item2], Some(1));
            if let Err(_e) = d2d.export_ui_to_png(&mut ui, model::WINDOW_WIDTH as u32, model::WINDOW_HEIGHT as u32, std::path::Path::new(path_str)) {
            } else {
                println!("Exported Starred UI to {}", path_str);
            }
            return Ok(());
        }
    }
    if let Some(pos) = args.iter().position(|a| a == "--export-settings") {
        if let Some(path_str) = args.get(pos + 1) {
            let mut d2d = d2d::D2dContext::new()?;
            let mut ui = ui::UiState::new(std::path::PathBuf::from(".clipped"), model::AppSettings::default());
            ui.mode = model::UiMode::Settings;
            if let Err(_e) = d2d.export_ui_to_png(&mut ui, model::WINDOW_WIDTH as u32, model::WINDOW_HEIGHT as u32, std::path::Path::new(path_str)) {
                eprintln!("Export error: {}", _e);
            } else {
                println!("Exported Settings UI to {}", path_str);
            }
            return Ok(());
        }
    }
    if let Some(pos) = args.iter().position(|a| a == "--export-search") {
        if let Some(path_str) = args.get(pos + 1) {
            let mut d2d = d2d::D2dContext::new()?;
            let mut ui = ui::UiState::new(std::path::PathBuf::from(".clipped"), model::AppSettings::default());
            ui.mode = model::UiMode::SearchList;
            ui.search_query = "d".into();
            let item1 = db::ClipItem {
                id: "1".into(),
                text: "OP516FL1:/system/xbin $ pm path gg.now.ads.service".into(),
                is_favorite: false,
                clip_type: "text".into(),
                image_path: None,
                image_width: None,
                image_height: None,
                ocr_text: None,
                full_text_len: 48,
            };
            let sample_json = "{\n  \"name\": \"clipped-app\",\n  \"dependencies\": {\n    \"windows\": \"0.58\"\n  }\n}";
            let item2 = db::ClipItem {
                id: "2".into(),
                text: sample_json.into(),
                is_favorite: true,
                clip_type: "text".into(),
                image_path: None,
                image_width: None,
                image_height: None,
                ocr_text: None,
                full_text_len: sample_json.len(),
            };
            let sample_json2 = "{\n  \"description\": \"Fast clipboard manager\"\n}";
            let item3 = db::ClipItem {
                id: "3".into(),
                text: sample_json2.into(),
                is_favorite: false,
                clip_type: "text".into(),
                image_path: None,
                image_width: None,
                image_height: None,
                ocr_text: None,
                full_text_len: sample_json2.len(),
            };
            ui.set_clips(vec![item1, item2, item3], Some(1));
            if let Err(_e) = d2d.export_ui_to_png(&mut ui, model::WINDOW_WIDTH as u32, model::WINDOW_HEIGHT as u32, std::path::Path::new(path_str)) {
                eprintln!("Export error: {}", _e);
            } else {
                println!("Exported Search UI to {}", path_str);
            }
            return Ok(());
        }
    }
    app::run_app()
}
