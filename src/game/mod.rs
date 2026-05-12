pub mod director;
pub mod ecology;
pub mod environment;
pub mod flow;
pub mod intent;
pub mod journey;
pub mod landmarks;
pub mod notebook;
pub mod places;
pub mod player;
pub mod presentation;
pub mod regions;
pub mod signs;
pub mod ui;
pub mod village;
pub mod world;

use std::env;

use bevy::prelude::*;

use crate::core::config::AppConfig;
use crate::game::flow::{
    AppScreen, PendingSessionLaunch, SessionMode, auto_start_session_mode_internal,
};

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        let auto_start_value = env::var("DAO_AUTO_START_MODE").ok();
        let presentation_value = env::var("DAO_PRESENTATION_MODE").ok();
        let auto_start_mode = auto_start_session_mode_internal(
            auto_start_value.as_deref(),
            presentation_value.as_deref(),
            app.world().get_resource::<AppConfig>(),
        );

        app.init_state::<AppScreen>();
        app.insert_resource(auto_start_mode.unwrap_or(SessionMode::Exploration));
        app.add_sub_state::<flow::InGameState>();
        app.insert_resource(PendingSessionLaunch(auto_start_mode));

        app.add_plugins(ui::UiPlugin);
        app.add_plugins(world::WorldPlugin);
        app.add_plugins(notebook::NotebookPlugin);
        app.add_plugins(village::VillagePlugin);
        app.add_plugins(regions::RegionPlugin);
        app.add_plugins(landmarks::LandmarkPlugin);
        app.add_plugins(ecology::EcologyPlugin);
        app.add_plugins(places::PlacesPlugin);
        app.add_plugins(environment::EnvironmentPlugin);
        app.add_plugins(signs::SignPlugin);
        app.add_plugins(intent::IntentPlugin);
        app.add_plugins(player::PlayerPlugin);
        app.add_plugins(journey::JourneyPlugin);
        app.add_plugins(director::DirectorPlugin);
        app.add_plugins(presentation::PresentationPlugin);
    }
}
