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

const RETAINED_ROTATED_LOGS: usize = 1;

pub fn init_logging(config: &AppConfig) -> Result<(), DaoError> {
    let log_dir = &config.log_directory;
    fs::create_dir_all(log_dir).map_err(|source| DaoError::CreateDirectory {
        path: log_dir.clone(),
        source,
    })?;

    let app_log = active_log_path(log_dir, "application.log");
    let error_log = active_log_path(log_dir, "error.log");
    let perf_log = active_log_path(log_dir, &config.performance_log_name);

    rotate_log_file(&app_log, RETAINED_ROTATED_LOGS)?;
    rotate_log_file(&error_log, RETAINED_ROTATED_LOGS)?;
    rotate_log_file(&perf_log, RETAINED_ROTATED_LOGS)?;

    let app_file = create_log_file(&app_log)?;
    let error_file = File::options()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&error_log)
        .map_err(|source| DaoError::CreateLogFile {
            path: error_log.clone(),
            source,
        })?;
    let perf_file = File::options()
        .create(true)
        .write(true)
        .truncate(true)
        .open(&perf_log)
        .map_err(|source| DaoError::CreateLogFile {
            path: perf_log.clone(),
            source,
        })?;

    let (app_writer, app_guard) = tracing_appender::non_blocking(SharedFileWriter {
        file: Arc::new(app_file),
    });
    let (error_writer, error_guard) = tracing_appender::non_blocking(SharedFileWriter {
        file: Arc::new(error_file),
    });
    let (perf_writer, perf_guard) = tracing_appender::non_blocking(SharedFileWriter {
        file: Arc::new(perf_file),
    });

    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,dao_game=debug,wgpu=warn,naga=warn"));

    let app_layer = fmt::layer()
        .json()
        .with_writer(app_writer)
        .with_ansi(false)
        .with_filter(env_filter);
    let error_layer = fmt::layer()
        .json()
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

fn active_log_path(directory: &Path, file_name: &str) -> PathBuf {
    directory.join(file_name)
}

fn rotated_log_path(path: &Path, generation: usize) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("log");
    path.with_file_name(format!("{file_name}.{generation}"))
}

fn rotate_log_file(path: &Path, retained_rotated_logs: usize) -> Result<(), DaoError> {
    if retained_rotated_logs == 0 {
        if path.exists() {
            fs::remove_file(path).map_err(|source| DaoError::RotateLogFile {
                path: path.to_path_buf(),
                source,
            })?;
        }
        return Ok(());
    }

    let oldest = rotated_log_path(path, retained_rotated_logs);
    if oldest.exists() {
        fs::remove_file(&oldest).map_err(|source| DaoError::RotateLogFile {
            path: oldest,
            source,
        })?;
    }

    for generation in (1..retained_rotated_logs).rev() {
        let source_path = rotated_log_path(path, generation);
        if source_path.exists() {
            let target_path = rotated_log_path(path, generation + 1);
            fs::rename(&source_path, &target_path).map_err(|source| DaoError::RotateLogFile {
                path: source_path,
                source,
            })?;
        }
    }

    if path.exists() {
        let target_path = rotated_log_path(path, 1);
        fs::rename(path, &target_path).map_err(|source| DaoError::RotateLogFile {
            path: path.to_path_buf(),
            source,
        })?;
    }

    Ok(())
}

fn create_log_file(path: &Path) -> Result<File, DaoError> {
    File::options()
        .create(true)
        .write(true)
        .truncate(true)
        .open(path)
        .map_err(|source| DaoError::CreateLogFile {
            path: path.to_path_buf(),
            source,
        })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::core::config::{
        AppConfig, CameraConfig, DesertConfig, EcologyConfig, EnvironmentConfig, PlayerConfig,
        PresentationConfig, QualityConfig, SignConfig, WorldConfig,
    };

    use std::fs;

    use super::{active_log_path, rotate_log_file, rotated_log_path};

    #[test]
    fn active_log_path_joins_directory_and_filename() {
        let path = active_log_path(&PathBuf::from("logs"), "application.log");
        assert_eq!(path, PathBuf::from("logs").join("application.log"));
    }

    #[test]
    fn rotated_log_path_appends_generation() {
        let path = rotated_log_path(&PathBuf::from("logs").join("application.log"), 1);
        assert_eq!(path, PathBuf::from("logs").join("application.log.1"));
    }

    #[test]
    fn rotate_log_file_keeps_current_and_previous_generation() {
        let temp_dir = tempfile::tempdir().expect("tempdir should exist");
        let path = temp_dir.path().join("application.log");
        fs::write(&path, "first").expect("first log should be written");

        rotate_log_file(&path, 1).expect("first rotate should work");
        fs::write(&path, "second").expect("second log should be written");
        rotate_log_file(&path, 1).expect("second rotate should work");

        assert!(!path.exists());
        assert_eq!(
            fs::read_to_string(rotated_log_path(&path, 1)).expect("rotated log should exist"),
            "second"
        );
        assert!(!rotated_log_path(&path, 2).exists());
    }

    #[test]
    fn config_clone_can_prepare_log_directory() {
        let config = AppConfig {
            window_title: "Dao".to_string(),
            log_directory: PathBuf::from("logs"),
            performance_log_name: "performance.log".to_string(),
            frame_log_interval: 60,
            performance_detail_interval: 1,
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
            camera: CameraConfig {
                third_person_default_distance: 6.2,
                third_person_min_distance: 3.2,
                third_person_max_distance: 9.5,
                third_person_height: 2.25,
                third_person_side_offset: 0.42,
                third_person_smoothness: 12.0,
                third_person_ground_clearance: 0.55,
            },
            ecology: EcologyConfig {
                bird_count: 18,
                fish_count: 10,
                state_update_interval_seconds: 0.2,
                visual_update_interval_seconds: 0.066,
                max_visible_bird_distance: 240.0,
            },
            desert: DesertConfig {
                dune_height: 3.2,
                dune_frequency: 0.22,
                gobi_flatness: 0.48,
                oasis_radius: 38.0,
                oasis_moisture: 0.86,
                sandstorm_visibility: 46.0,
                sandstorm_particle_strength: 1.0,
                sandstorm_wind_speed: 4.2,
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
