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
    pub presentation: PresentationConfig,
    pub world: WorldConfig,
    pub environment: EnvironmentConfig,
    pub player: PlayerConfig,
    pub signs: SignConfig,
    pub quality: QualityConfig,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct WorldConfig {
    pub seed: u64,
    pub world_radius: i32,
    pub chunk_radius: i32,
    pub cell_size: f32,
    pub terrain_subdivisions: u32,
    pub terrain_scale: f32,
    pub height_variation: f32,
    pub water_level: f32,
    pub noise_octaves: u32,
    pub ridge_sharpness: f32,
    pub shoreline_blend: f32,
    pub river_frequency: f32,
    pub river_depth: f32,
    pub erosion_strength: f32,
    pub sediment_bias: f32,
    pub visible_chunk_radius: i32,
    pub high_detail_chunk_radius: i32,
    pub low_detail_chunk_radius: i32,
    pub preload_chunk_radius: i32,
    pub showcase_search_radius: i32,
    pub streaming_chunk_budget: u32,
    pub background_generation_budget: u32,
    pub streaming_cache_capacity: usize,
    pub material_texture_resolution: u32,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct PresentationConfig {
    pub enabled: bool,
    pub scene_duration_seconds: f32,
    pub camera_blend_speed: f32,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct EnvironmentConfig {
    pub day_length_seconds: f32,
    pub wander_radius: f32,
    pub wander_speed: f32,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct PlayerConfig {
    pub walk_speed: f32,
    pub sprint_multiplier: f32,
    pub mouse_sensitivity: f32,
    pub eye_height: f32,
    pub body_height: f32,
    pub jump_velocity: f32,
    pub gravity: f32,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct SignConfig {
    pub resonance_threshold: f32,
    pub resonance_smoothing: f32,
    pub calm_recovery: f32,
    pub calm_threshold: f32,
    pub omen_beacon_height: f32,
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
chunk_radius = 1
cell_size = 3.0
terrain_subdivisions = 6
terrain_scale = 5.0
height_variation = 2.5
water_level = 0.1
noise_octaves = 4
ridge_sharpness = 1.8
shoreline_blend = 0.18
river_frequency = 0.22
river_depth = 0.55
erosion_strength = 0.4
sediment_bias = 0.22
visible_chunk_radius = 2
high_detail_chunk_radius = 1
low_detail_chunk_radius = 2
preload_chunk_radius = 3
showcase_search_radius = 12
streaming_chunk_budget = 2
background_generation_budget = 3
streaming_cache_capacity = 64
material_texture_resolution = 192

[presentation]
enabled = true
scene_duration_seconds = 8.0
camera_blend_speed = 1.8

[environment]
day_length_seconds = 180.0
wander_radius = 4.5
wander_speed = 0.7

[player]
walk_speed = 7.0
sprint_multiplier = 1.65
mouse_sensitivity = 0.0022
eye_height = 1.65
body_height = 1.2
jump_velocity = 6.2
gravity = 18.0

[signs]
resonance_threshold = 0.5
resonance_smoothing = 0.12
calm_recovery = 0.02
calm_threshold = 0.4
omen_beacon_height = 3.0

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
        assert!(config.presentation.enabled);
        assert_eq!(config.presentation.scene_duration_seconds, 8.0);
        assert_eq!(config.world.seed, 7);
        assert_eq!(config.world.chunk_radius, 1);
        assert_eq!(config.world.cell_size, 3.0);
        assert_eq!(config.world.terrain_subdivisions, 6);
        assert_eq!(config.world.river_frequency, 0.22);
        assert_eq!(config.world.visible_chunk_radius, 2);
        assert_eq!(config.world.high_detail_chunk_radius, 1);
        assert_eq!(config.world.low_detail_chunk_radius, 2);
        assert_eq!(config.world.preload_chunk_radius, 3);
        assert_eq!(config.world.showcase_search_radius, 12);
        assert_eq!(config.world.streaming_chunk_budget, 2);
        assert_eq!(config.world.background_generation_budget, 3);
        assert_eq!(config.world.streaming_cache_capacity, 64);
        assert_eq!(config.world.material_texture_resolution, 192);
        assert_eq!(config.player.walk_speed, 7.0);
        assert_eq!(config.player.eye_height, 1.65);
        assert_eq!(config.environment.day_length_seconds, 180.0);
        assert_eq!(config.environment.wander_radius, 4.5);
        assert_eq!(config.quality.target_fps, 144.0);
    }
}
