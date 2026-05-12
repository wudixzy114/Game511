use bevy::prelude::*;

use crate::core::config::AppConfig;

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Hash, States)]
pub enum AppScreen {
    #[default]
    MainMenu,
    InGame,
}

#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Hash, SubStates)]
#[source(AppScreen = AppScreen::InGame)]
#[states(scoped_entities)]
pub enum InGameState {
    #[default]
    Running,
    Paused,
}

#[derive(Debug, Resource, Clone, Copy, Default, Eq, PartialEq)]
pub enum SessionMode {
    #[default]
    Exploration,
    Presentation,
}

impl SessionMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::Exploration => "探索",
            Self::Presentation => "展示",
        }
    }
}

#[derive(Debug, Resource, Clone, Copy, Default, Eq, PartialEq)]
pub struct PendingSessionLaunch(pub Option<SessionMode>);

pub fn in_session_mode(mode: SessionMode) -> impl Fn(Res<SessionMode>) -> bool + Clone {
    move |session_mode: Res<SessionMode>| *session_mode == mode
}

pub fn auto_start_session_mode_internal(
    auto_start_value: Option<&str>,
    presentation_value: Option<&str>,
    config: Option<&AppConfig>,
) -> Option<SessionMode> {
    if let Some(raw) = auto_start_value {
        return parse_auto_start_mode(raw);
    }

    match presentation_value {
        Some(raw) => {
            if matches!(raw.to_ascii_lowercase().as_str(), "0" | "false" | "off") {
                None
            } else {
                Some(SessionMode::Presentation)
            }
        }
        None => config
            .is_some_and(|config| config.presentation.enabled)
            .then_some(SessionMode::Presentation),
    }
}

fn parse_auto_start_mode(raw: &str) -> Option<SessionMode> {
    match raw.trim().to_ascii_lowercase().as_str() {
        "" | "0" | "false" | "off" | "none" | "menu" => None,
        "1" | "true" | "on" | "explore" | "exploration" | "game" | "world" => {
            Some(SessionMode::Exploration)
        }
        "presentation" | "present" | "showcase" | "demo" => Some(SessionMode::Presentation),
        unknown => {
            tracing::warn!(
                value = unknown,
                "unknown DAO_AUTO_START_MODE, falling back to presentation"
            );
            Some(SessionMode::Presentation)
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::core::config::{
        AppConfig, EnvironmentConfig, PlayerConfig, PresentationConfig, QualityConfig, SignConfig,
        WorldConfig,
    };

    use super::{SessionMode, auto_start_session_mode_internal, parse_auto_start_mode};

    fn test_config(enabled: bool) -> AppConfig {
        AppConfig {
            window_title: "Dao".to_string(),
            log_directory: PathBuf::from("logs"),
            performance_log_name: "performance.log".to_string(),
            frame_log_interval: 60,
            performance_detail_interval: 1,
            presentation: PresentationConfig {
                enabled,
                scene_duration_seconds: 7.0,
                camera_blend_speed: 2.0,
            },
            world: WorldConfig {
                seed: 1,
                world_radius: 1,
                chunk_radius: 1,
                cell_size: 2.0,
                terrain_subdivisions: 4,
                terrain_scale: 1.0,
                height_variation: 1.0,
                water_level: 0.0,
                noise_octaves: 3,
                ridge_sharpness: 1.5,
                shoreline_blend: 0.15,
                river_frequency: 0.2,
                river_depth: 0.4,
                erosion_strength: 0.25,
                sediment_bias: 0.15,
                visible_chunk_radius: 1,
                high_detail_chunk_radius: 1,
                low_detail_chunk_radius: 1,
                preload_chunk_radius: 2,
                impostor_chunk_radius: 4,
                impostor_radial_bands: 3,
                impostor_angular_segments: 32,
                showcase_search_radius: 4,
                streaming_chunk_budget: 1,
                background_generation_budget: 2,
                streaming_cache_capacity: 16,
                collision_proxy_radius: 1,
                collision_subdivisions: 6,
                collision_chunk_budget: 1,
                collision_cache_capacity: 12,
                material_texture_resolution: 64,
            },
            environment: EnvironmentConfig {
                day_length_seconds: 180.0,
                wander_radius: 4.0,
                wander_speed: 0.7,
            },
            player: PlayerConfig {
                walk_speed: 7.0,
                sprint_multiplier: 1.6,
                mouse_sensitivity: 0.002,
                eye_height: 1.65,
                body_height: 1.2,
                capsule_radius: 0.4,
                max_slope_degrees: 45.0,
                step_height: 0.6,
                ground_snap_distance: 1.0,
                contact_substeps: 4,
                jump_velocity: 6.0,
                gravity: 18.0,
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
    fn auto_start_uses_config_when_env_missing() {
        assert_eq!(
            auto_start_session_mode_internal(None, None, Some(&test_config(true))),
            Some(SessionMode::Presentation)
        );
        assert_eq!(
            auto_start_session_mode_internal(None, None, Some(&test_config(false))),
            None
        );
    }

    #[test]
    fn legacy_presentation_env_can_disable_or_enable_presentation() {
        assert_eq!(
            auto_start_session_mode_internal(None, Some("false"), Some(&test_config(true))),
            None
        );
        assert_eq!(
            auto_start_session_mode_internal(None, Some("1"), Some(&test_config(false))),
            Some(SessionMode::Presentation)
        );
    }

    #[test]
    fn auto_start_mode_env_selects_exploration_or_presentation() {
        assert_eq!(
            parse_auto_start_mode("exploration"),
            Some(SessionMode::Exploration)
        );
        assert_eq!(
            parse_auto_start_mode("presentation"),
            Some(SessionMode::Presentation)
        );
        assert_eq!(parse_auto_start_mode("menu"), None);
        assert_eq!(
            auto_start_session_mode_internal(
                Some("exploration"),
                Some("presentation"),
                Some(&test_config(true))
            ),
            Some(SessionMode::Exploration)
        );
    }
}
