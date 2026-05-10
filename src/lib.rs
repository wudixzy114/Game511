pub mod core;
pub mod game;

use bevy::prelude::*;

pub fn build_app() -> App {
    let mut app = App::new();
    app.add_plugins(core::CorePlugin);
    app.add_plugins(game::GamePlugin);
    app
}
