use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize)]
pub struct AppSettings {
    pub update_interval_ms: u32,
    pub history_retention_hours: u32,
    pub show_percentage_on_icon: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            update_interval_ms: 30000,
            history_retention_hours: 168,
            show_percentage_on_icon: true,
        }
    }
}

impl AppSettings {
    pub fn load() -> Self {
        let config_path = Self::get_config_path();
        std::fs::read_to_string(&config_path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let config_path = Self::get_config_path();
        if let Ok(json) = serde_json::to_string_pretty(&self) {
            let _ = std::fs::write(&config_path, json);
        }
    }

    fn get_config_path() -> std::path::PathBuf {
        let mut path = std::env::current_exe().unwrap();
        path.pop();
        path.push("battesty_config.json");
        path
    }
}