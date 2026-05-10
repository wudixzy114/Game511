use std::{
    fs,
    path::{Path, PathBuf},
};

use bevy::prelude::Resource;
use serde::Deserialize;

use super::error::DaoError;

#[derive(Debug, Clone, Deserialize, PartialEq, Resource)]
pub struct AppConfig {
    pub window_title: String,
    pub log_directory: PathBuf,
    pub performance_log_name: String,
    pub frame_log_interval: u32,
    pub world: WorldConfig,
    pub signs: SignConfig,
    pub quality: QualityConfig,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct WorldConfig {
    pub seed: u64,
    pub world_radius: i32,
    pub terrain_scale: f32,
    pub height_variation: f32,
    pub water_level: f32,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct SignConfig {
    pub resonance_threshold: f32,
    pub calm_recovery: f32,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct QualityConfig {
    pub target_fps: f32,
    pub frame_time_budget_ms: f32,
}

impl AppConfig {
    pub fn load_from_default_path() -> Result<Self, DaoError> {
        Self::load_from_path(Path::new("config/app.toml"))
    }

    pub fn load_from_path(path: &Path) -> Result<Self, DaoError> {
        let raw = fs::read_to_string(path).map_err(|source| DaoError::ReadFile {
            path: path.to_path_buf(),
            source,
        })?;
        toml::from_str(&raw).map_err(|source| DaoError::ParseToml {
            path: path.to_path_buf(),
            source,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path};

    use super::AppConfig;

    #[test]
    fn load_config_from_file() {
        let temp_dir = tempfile::tempdir().expect("tempdir should exist");
        let config_path = temp_dir.path().join("app.toml");

        fs::write(
            &config_path,
            r#"
window_title = "Test Title"
log_directory = "logs"
performance_log_name = "performance.log"
frame_log_interval = 30

[world]
seed = 7
world_radius = 3
terrain_scale = 5.0
height_variation = 2.5
water_level = 0.1

[signs]
resonance_threshold = 0.5
calm_recovery = 0.02

[quality]
target_fps = 144.0
frame_time_budget_ms = 6.9
"#,
        )
        .expect("config file should be written");

        let config = AppConfig::load_from_path(Path::new(&config_path))
            .expect("config should load successfully");

        assert_eq!(config.window_title, "Test Title");
        assert_eq!(config.frame_log_interval, 30);
        assert_eq!(config.world.seed, 7);
        assert_eq!(config.quality.target_fps, 144.0);
    }
}
