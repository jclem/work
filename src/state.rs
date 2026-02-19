use serde::{Deserialize, Serialize};

use crate::paths;

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SessionsView {
    #[default]
    Tree,
    Flat,
}

#[derive(Debug, Default, Serialize, Deserialize)]
pub struct UiState {
    #[serde(rename = "show-empty-projects", default)]
    pub show_empty_projects: bool,

    #[serde(rename = "sessions-view", default)]
    pub sessions_view: SessionsView,
}

pub fn load() -> UiState {
    let path = paths::state_path();
    let Ok(content) = std::fs::read_to_string(&path) else {
        return UiState::default();
    };
    toml::from_str(&content).unwrap_or_default()
}

pub fn save(state: &UiState) {
    let path = paths::state_path();
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    if let Ok(content) = toml::to_string_pretty(state) {
        let _ = std::fs::write(&path, content);
    }
}
