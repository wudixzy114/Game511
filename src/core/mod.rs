pub mod config;
pub mod error;
pub mod logging;
pub mod performance;

use std::{env, time::Duration};

use bevy::prelude::*;

pub struct CorePlugin;

impl Plugin for CorePlugin {
    fn build(&self, app: &mut App) {
        let config = config::AppConfig::load_from_default_path()
            .unwrap_or_else(|error| panic!("failed to load config/app.toml: {error}"));

        logging::init_logging(&config)
            .unwrap_or_else(|error| panic!("failed to initialize logging: {error}"));

        app.insert_resource(config.clone());
        app.insert_resource(ClearColor(Color::srgb(0.08, 0.11, 0.12)));
        app.insert_resource(performance::FramePerformance::default());
        if let Some(duration) = read_auto_exit_duration() {
            app.insert_resource(AutoExit(duration));
            app.add_systems(Update, auto_exit_after_duration);
        }
        app.add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: config.window_title.clone(),
                present_mode: bevy::window::PresentMode::AutoVsync,
                ..Default::default()
            }),
            ..Default::default()
        }));
        app.add_message::<performance::PerformanceAlert>();
        app.add_systems(Update, performance::track_frame_timing);
    }
}

#[derive(Debug, Resource, Clone, Copy)]
struct AutoExit(Duration);

fn read_auto_exit_duration() -> Option<Duration> {
    let raw = env::var("DAO_AUTO_EXIT_SECONDS").ok()?;
    let seconds = raw.parse::<f32>().ok()?;
    if seconds.is_sign_negative() {
        return None;
    }
    Some(Duration::from_secs_f32(seconds))
}

fn auto_exit_after_duration(
    time: Res<Time>,
    auto_exit: Res<AutoExit>,
    mut exit: MessageWriter<AppExit>,
) {
    if time.elapsed() >= auto_exit.0 {
        tracing::info!(
            target: "dao_game::bootstrap",
            seconds = auto_exit.0.as_secs_f32(),
            "auto exit requested"
        );
        exit.write(AppExit::Success);
    }
}
