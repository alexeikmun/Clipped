use serde::{Deserialize, Serialize};

pub const DEFAULT_SHORTCUT: &str = "Ctrl+Alt+Shift+.";
pub const SETTINGS_FILE: &str = "settings.json";
pub const WINDOW_WIDTH: f32 = 480.0;
pub const WINDOW_HEIGHT: f32 = 360.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub shortcut: String,
    #[serde(default = "default_max_items")]
    pub max_items: usize,
    #[serde(default = "default_launch_on_boot")]
    pub launch_on_boot: bool,
    #[serde(default)]
    pub typesafe_api_key: Option<String>,
    #[serde(default = "default_true")]
    pub enable_semantic_search: bool,
}

fn default_max_items() -> usize {
    999
}

fn default_launch_on_boot() -> bool {
    true
}

fn default_true() -> bool {
    true
}

impl AppSettings {
    pub fn get_typesafe_key(&self) -> Option<String> {
        if let Ok(key) = std::env::var("TYPESAFE_API_KEY") {
            let trimmed = key.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
        self.typesafe_api_key.as_ref().and_then(|k| {
            let trimmed = k.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
    }
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            shortcut: DEFAULT_SHORTCUT.to_string(),
            max_items: 999,
            launch_on_boot: true,
            typesafe_api_key: None,
            enable_semantic_search: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UiMode {
    SingleCard,
    SearchList,
    Settings,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HitTarget {
    None,
    FavFilter,
    SearchBox,
    CloseSettings,
    DockTrash,
    DockStar,
    ListItem(usize),
    SettingsBootToggle,
    ApiKeyField,
    ClearNonFavorites,
    ClearAll,
}
