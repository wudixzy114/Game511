pub mod signs;
pub mod world;

use bevy::prelude::*;

pub struct GamePlugin;

impl Plugin for GamePlugin {
    fn build(&self, app: &mut App) {
        app.add_plugins(world::WorldPlugin);
        app.add_plugins(signs::SignPlugin);
    }
}
