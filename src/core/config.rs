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
    pub performance_detail_interval: u32,
    pub presentation: PresentationConfig,
    pub world: WorldConfig,
    pub environment: EnvironmentConfig,
    pub player: PlayerConfig,
    pub camera: CameraConfig,
    pub ecology: EcologyConfig,
    pub assets: AssetConfig,
    pub desert: DesertConfig,
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
    pub impostor_chunk_radius: i32,
    pub impostor_radial_bands: u32,
    pub impostor_angular_segments: u32,
    pub showcase_search_radius: i32,
    pub streaming_chunk_budget: u32,
    pub background_generation_budget: u32,
    pub streaming_cache_capacity: usize,
    pub collision_proxy_radius: i32,
    pub collision_subdivisions: u32,
    pub collision_chunk_budget: u32,
    pub collision_cache_capacity: usize,
    pub material_texture_resolution: u32,
    pub detail_density: f32,
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
    pub capsule_radius: f32,
    pub max_slope_degrees: f32,
    pub step_height: f32,
    pub ground_snap_distance: f32,
    pub contact_substeps: u32,
    pub jump_velocity: f32,
    pub gravity: f32,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct CameraConfig {
    pub third_person_default_distance: f32,
    pub third_person_min_distance: f32,
    pub third_person_max_distance: f32,
    pub third_person_height: f32,
    pub third_person_side_offset: f32,
    pub third_person_smoothness: f32,
    pub third_person_ground_clearance: f32,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct EcologyConfig {
    pub bird_count: u32,
    pub fish_count: u32,
    pub sheep_count: u32,
    pub state_update_interval_seconds: f32,
    pub visual_update_interval_seconds: f32,
    pub max_visible_bird_distance: f32,
    pub max_visible_fish_distance: f32,
    pub max_visible_sheep_distance: f32,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct AssetConfig {
    pub color_saturation: f32,
    pub warm_light_intensity: f32,
    pub water_alpha: f32,
    pub shadow_alpha: f32,
}

#[derive(Debug, Clone, Deserialize, PartialEq)]
pub struct DesertConfig {
    pub dune_height: f32,
    pub dune_frequency: f32,
    pub gobi_flatness: f32,
    pub oasis_radius: f32,
    pub oasis_moisture: f32,
    pub sandstorm_visibility: f32,
    pub sandstorm_particle_strength: f32,
    pub sandstorm_wind_speed: f32,
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
performance_detail_interval = 1

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
impostor_chunk_radius = 8
impostor_radial_bands = 4
impostor_angular_segments = 56
showcase_search_radius = 12
streaming_chunk_budget = 2
background_generation_budget = 3
streaming_cache_capacity = 64
collision_proxy_radius = 2
collision_subdivisions = 10
collision_chunk_budget = 2
collision_cache_capacity = 48
material_texture_resolution = 192
detail_density = 1.0

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
capsule_radius = 0.42
max_slope_degrees = 46.0
step_height = 0.7
ground_snap_distance = 1.1
contact_substeps = 4
jump_velocity = 6.2
gravity = 18.0

[camera]
third_person_default_distance = 6.2
third_person_min_distance = 3.2
third_person_max_distance = 9.5
third_person_height = 2.25
third_person_side_offset = 0.42
third_person_smoothness = 12.0
third_person_ground_clearance = 0.55

[ecology]
bird_count = 18
fish_count = 10
sheep_count = 9
state_update_interval_seconds = 0.2
visual_update_interval_seconds = 0.066
max_visible_bird_distance = 240.0
max_visible_fish_distance = 90.0
max_visible_sheep_distance = 120.0

[assets]
color_saturation = 1.0
warm_light_intensity = 1.0
water_alpha = 0.64
shadow_alpha = 0.58

[desert]
dune_height = 3.2
dune_frequency = 0.22
gobi_flatness = 0.48
oasis_radius = 38.0
oasis_moisture = 0.86
sandstorm_visibility = 46.0
sandstorm_particle_strength = 1.0
sandstorm_wind_speed = 4.2

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
        assert_eq!(config.world.impostor_chunk_radius, 8);
        assert_eq!(config.world.impostor_radial_bands, 4);
        assert_eq!(config.world.impostor_angular_segments, 56);
        assert_eq!(config.world.showcase_search_radius, 12);
        assert_eq!(config.world.streaming_chunk_budget, 2);
        assert_eq!(config.world.background_generation_budget, 3);
        assert_eq!(config.world.streaming_cache_capacity, 64);
        assert_eq!(config.world.collision_proxy_radius, 2);
        assert_eq!(config.world.collision_subdivisions, 10);
        assert_eq!(config.world.collision_chunk_budget, 2);
        assert_eq!(config.world.collision_cache_capacity, 48);
        assert_eq!(config.world.material_texture_resolution, 192);
        assert_eq!(config.world.detail_density, 1.0);
        assert_eq!(config.player.walk_speed, 7.0);
        assert_eq!(config.player.eye_height, 1.65);
        assert_eq!(config.player.capsule_radius, 0.42);
        assert_eq!(config.player.max_slope_degrees, 46.0);
        assert_eq!(config.player.step_height, 0.7);
        assert_eq!(config.player.ground_snap_distance, 1.1);
        assert_eq!(config.player.contact_substeps, 4);
        assert_eq!(config.camera.third_person_default_distance, 6.2);
        assert_eq!(config.ecology.bird_count, 18);
        assert_eq!(config.ecology.sheep_count, 9);
        assert_eq!(config.ecology.max_visible_fish_distance, 90.0);
        assert_eq!(config.assets.color_saturation, 1.0);
        assert_eq!(config.assets.warm_light_intensity, 1.0);
        assert_eq!(config.assets.water_alpha, 0.64);
        assert_eq!(config.assets.shadow_alpha, 0.58);
        assert_eq!(config.desert.dune_height, 3.2);
        assert_eq!(config.environment.day_length_seconds, 180.0);
        assert_eq!(config.environment.wander_radius, 4.5);
        assert_eq!(config.quality.target_fps, 144.0);
    }
}
