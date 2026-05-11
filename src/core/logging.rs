use std::{
    fs::{self, File},
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
};

use tracing_subscriber::{
    EnvFilter, Layer, Registry,
    fmt::{self, writer::BoxMakeWriter},
    layer::SubscriberExt,
};

use super::{config::AppConfig, error::DaoError};

#[derive(Clone)]
struct SharedFileWriter {
    file: Arc<File>,
}

impl std::io::Write for SharedFileWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        (&*self.file).write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        (&*self.file).flush()
    }
}

static LOG_GUARDS: OnceLock<Vec<tracing_appender::non_blocking::WorkerGuard>> = OnceLock::new();

pub fn init_logging(config: &AppConfig) -> Result<(), DaoError> {
    let log_dir = &config.log_directory;
    fs::create_dir_all(log_dir).map_err(|source| DaoError::CreateDirectory {
        path: log_dir.clone(),
        source,
    })?;

    let app_log = rolling_path(log_dir, "application.log");
    let error_log = rolling_path(log_dir, "error.log");
    let perf_log = rolling_path(log_dir, &config.performance_log_name);

    let appender = tracing_appender::rolling::daily(log_dir, "application.log");
    let error_file = File::options()
        .create(true)
        .append(true)
        .open(&error_log)
        .map_err(|source| DaoError::CreateLogFile {
            path: error_log.clone(),
            source,
        })?;
    let perf_file = File::options()
        .create(true)
        .append(true)
        .open(&perf_log)
        .map_err(|source| DaoError::CreateLogFile {
            path: perf_log.clone(),
            source,
        })?;

    let (app_writer, app_guard) = tracing_appender::non_blocking(appender);
    let (error_writer, error_guard) = tracing_appender::non_blocking(SharedFileWriter {
        file: Arc::new(error_file),
    });
    let (perf_writer, perf_guard) = tracing_appender::non_blocking(SharedFileWriter {
        file: Arc::new(perf_file),
    });

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,dao_game=debug,wgpu=warn,naga=warn"));

    let app_layer = fmt::layer()
        .with_writer(app_writer)
        .with_ansi(false)
        .with_filter(env_filter);
    let error_layer = fmt::layer()
        .with_writer(error_writer)
        .with_ansi(false)
        .with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
            metadata.level() <= &tracing::Level::ERROR
        }));
    let perf_layer = fmt::layer()
        .json()
        .with_writer(BoxMakeWriter::new(perf_writer))
        .with_filter(tracing_subscriber::filter::filter_fn(|metadata| {
            metadata.target().starts_with("dao_game::performance")
        }));

    tracing::subscriber::set_global_default(
        Registry::default()
            .with(app_layer)
            .with(error_layer)
            .with(perf_layer),
    )?;

    LOG_GUARDS
        .set(vec![app_guard, error_guard, perf_guard])
        .ok();

    tracing::info!(target: "dao_game::bootstrap", log_file = %app_log.display(), "logging initialized");
    Ok(())
}

fn rolling_path(directory: &Path, file_name: &str) -> PathBuf {
    directory.join(file_name)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::core::config::{
        AppConfig, EnvironmentConfig, PlayerConfig, PresentationConfig, QualityConfig, SignConfig,
        WorldConfig,
    };

    use super::rolling_path;

    #[test]
    fn rolling_path_joins_directory_and_filename() {
        let path = rolling_path(&PathBuf::from("logs"), "application.log");
        assert_eq!(path, PathBuf::from("logs").join("application.log"));
    }

    #[test]
    fn config_clone_can_prepare_log_directory() {
        let config = AppConfig {
            window_title: "Dao".to_string(),
            log_directory: PathBuf::from("logs"),
            performance_log_name: "performance.log".to_string(),
            frame_log_interval: 60,
            presentation: PresentationConfig {
                enabled: true,
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
                showcase_search_radius: 4,
                streaming_chunk_budget: 1,
                background_generation_budget: 2,
                streaming_cache_capacity: 16,
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
        };

        assert_eq!(config.log_directory, PathBuf::from("logs"));
    }
}
