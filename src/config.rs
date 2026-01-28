use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Serialize, Deserialize, Debug)]
pub struct Config {
    pub theme_name: String,
    pub is_transparent: bool,
    pub vercel_token: Option<String>,
    pub last_project_id: Option<String>,
    pub last_project_name: Option<String>,
    pub enable_mouse: bool,
    pub stat_period: String,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            theme_name: "Default".to_string(),
            is_transparent: false,
            vercel_token: None,
            last_project_id: None,
            last_project_name: None,
            enable_mouse: false,
            stat_period: "24h".to_string(),
        }
    }
}

impl Config {
    pub fn load() -> Self {
        if let Some(config_path) = Self::get_config_path() {
            if config_path.exists() {
                if let Ok(content) = fs::read_to_string(config_path) {
                    if let Ok(config) = serde_json::from_str(&content) {
                        return config;
                    }
                }
            }
        }
        Self::default()
    }

    pub fn save(&self) {
        if let Some(config_path) = Self::get_config_path() {
            if let Some(parent) = config_path.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Ok(content) = serde_json::to_string_pretty(self) {
                let _ = fs::write(config_path, content);
            }
        }
    }

    fn get_config_path() -> Option<PathBuf> {
        ProjectDirs::from("com", "polymer", "polymer")
            .map(|proj_dirs| proj_dirs.config_dir().join("config.json"))
    }
}
