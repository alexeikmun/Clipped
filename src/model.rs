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
}

fn default_max_items() -> usize {
    999
}

fn default_launch_on_boot() -> bool {
    false
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            shortcut: DEFAULT_SHORTCUT.to_string(),
            max_items: 999,
            launch_on_boot: false,
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
    ClearNonFavorites,
    ClearAll,
}
