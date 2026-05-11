pub mod presentation;
pub mod signs;
pub mod world;

use bevy::prelude::*;

use crate::core::config::AppConfig;

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(world::WorldPlugin);
        app.add_plugins(signs::SignPlugin);
        if presentation_mode_enabled_internal(None, app.world().get_resource::<AppConfig>()) {
            app.add_plugins(presentation::PresentationPlugin);
        }
    }
}

fn presentation_mode_enabled_internal(env_value: Option<&str>, config: Option<&AppConfig>) -> bool {
    match env_value {
        Some(raw) => !matches!(raw.to_ascii_lowercase().as_str(), "0" | "false" | "off"),
        None => config.is_some_and(|config| config.presentation.enabled),
    }
}

#[cfg(test)]
mod tests {
    use super::presentation_mode_enabled_internal;
    use crate::core::config::{
        AppConfig, EnvironmentConfig, PresentationConfig, QualityConfig, SignConfig, WorldConfig,
    };
    use std::path::PathBuf;

    fn test_config(enabled: bool) -> AppConfig {
        AppConfig {
            window_title: "Dao".to_string(),
            log_directory: PathBuf::from("logs"),
            performance_log_name: "performance.log".to_string(),
            frame_log_interval: 60,
            presentation: PresentationConfig {
                enabled,
                scene_duration_seconds: 7.0,
                camera_blend_speed: 2.0,
            },
            world: WorldConfig {
                seed: 1,
                world_radius: 1,
                terrain_scale: 1.0,
                height_variation: 1.0,
                water_level: 0.0,
            },
            environment: EnvironmentConfig {
                day_length_seconds: 180.0,
                wander_radius: 4.0,
                wander_speed: 0.7,
            },
            signs: SignConfig {
                resonance_threshold: 0.5,
                resonance_smoothing: 0.1,
                calm_recovery: 0.01,
                calm_threshold: 0.35,
                omen_beacon_height: 3.0,
            },
            quality: QualityConfig {
                target_fps: 60.0,
                frame_time_budget_ms: 16.6,
            },
        }
    }

    #[test]
    fn presentation_mode_follows_config_when_env_missing() {
        assert!(presentation_mode_enabled_internal(
            None,
            Some(&test_config(true))
        ));
        assert!(!presentation_mode_enabled_internal(
            None,
            Some(&test_config(false))
        ));
    }

    #[test]
    fn presentation_mode_env_can_disable_or_enable() {
        assert!(!presentation_mode_enabled_internal(
            Some("false"),
            Some(&test_config(true)),
        ));
        assert!(presentation_mode_enabled_internal(
            Some("1"),
            Some(&test_config(false)),
        ));
    }
}
