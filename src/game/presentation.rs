use bevy::prelude::*;

use crate::{
    core::config::AppConfig,
    game::{
        signs::{OmenKind, SignState},
        world::{
            BiomeKind, TerrainTile, WandererPrototype, WorldCamera, WorldMap,
            WorldPresentationControl,
        },
    },
};

pub struct PresentationPlugin;

impl Plugin for PresentationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            First,
            (initialize_presentation, advance_presentation_director).chain(),
        );
        app.add_systems(Update, drive_presentation_camera);
    }
}

#[derive(Debug, Resource, Clone)]
struct PresentationDirector {
    scene_duration: f32,
    elapsed: f32,
    current_scene_index: usize,
    scenes: Vec<PresentationScene>,
}

impl PresentationDirector {
    fn current_scene(&self) -> &PresentationScene {
        &self.scenes[self.current_scene_index]
    }
}

#[derive(Debug, Clone)]
struct PresentationScene {
    name: &'static str,
    description: &'static str,
    focus: Vec3,
    camera_offset: Vec3,
    wander_target: Vec3,
    time_override: f32,
    expected_omen: Option<OmenKind>,
}

#[derive(Debug, Clone, Copy)]
struct SceneAnchor {
    x: i32,
    z: i32,
    tile: TerrainTile,
}

fn initialize_presentation(
    mut commands: Commands,
    config: Res<AppConfig>,
    world_map: Option<Res<WorldMap>>,
    director: Option<Res<PresentationDirector>>,
) {
    if director.is_some() || !config.presentation.enabled {
        return;
    }
    let Some(world_map) = world_map else {
        return;
    };
    let center_anchor = SceneAnchor {
        x: 0,
        z: 0,
        tile: world_map
            .tile_at_grid(0, 0)
            .expect("world center tile should exist"),
    };

    let ridge_anchor = find_anchor(&world_map, BiomeKind::Ridge).unwrap_or(center_anchor);
    let grove_anchor = find_anchor(&world_map, BiomeKind::Grove).unwrap_or(ridge_anchor);
    let water_anchor = find_anchor(&world_map, BiomeKind::Water).unwrap_or(grove_anchor);
    let meadow_anchor = find_anchor(&world_map, BiomeKind::Meadow).unwrap_or(grove_anchor);

    let scenes = vec![
        build_scene(
            "Panorama Sweep",
            "overview of terrain layers, sky lighting and traversal silhouette",
            meadow_anchor,
            &world_map,
            Vec3::new(-15.0, 9.5, 15.0),
            0.18,
            None,
        ),
        build_scene(
            "Grove Whisper",
            "stable grove omen showcase with close-range vegetation silhouettes",
            grove_anchor,
            &world_map,
            Vec3::new(-6.0, 4.8, 7.0),
            0.34,
            Some(OmenKind::GroveWhisper),
        ),
        build_scene(
            "Ridge Dawn",
            "sunrise lighting shift and high-ground omen response",
            ridge_anchor,
            &world_map,
            Vec3::new(-9.0, 6.2, 10.0),
            0.02,
            Some(OmenKind::DawnLight),
        ),
        build_scene(
            "Still Water",
            "waterline calm test and cool omen beacon response",
            water_anchor,
            &world_map,
            Vec3::new(0.0, 4.2, 8.5),
            0.74,
            Some(OmenKind::StillWater),
        ),
    ];

    tracing::info!(
        target: "dao_game::presentation",
        scene_count = scenes.len(),
        scene_duration_seconds = config.presentation.scene_duration_seconds,
        "presentation mode initialized"
    );

    commands.insert_resource(PresentationDirector {
        scene_duration: config.presentation.scene_duration_seconds.max(1.0),
        elapsed: 0.0,
        current_scene_index: 0,
        scenes,
    });
    commands.insert_resource(WorldPresentationControl::default());
}

fn advance_presentation_director(
    time: Res<Time>,
    config: Res<AppConfig>,
    director: Option<ResMut<PresentationDirector>>,
    control: Option<ResMut<WorldPresentationControl>>,
    mut signs: ResMut<SignState>,
    world_map: Res<WorldMap>,
    mut wanderer_query: Query<&mut Transform, With<WandererPrototype>>,
) {
    if !config.presentation.enabled {
        return;
    }
    let Some(mut director) = director else {
        return;
    };
    let Some(mut control) = control else {
        return;
    };

    let previous_scene_index = director.current_scene_index;
    director.elapsed += time.delta_secs();
    let scene_index = scene_index_at_elapsed(
        director.elapsed,
        director.scene_duration,
        director.scenes.len(),
    );
    let scene_progress = scene_progress(director.elapsed, director.scene_duration);
    director.current_scene_index = scene_index;
    let scene = director.current_scene().clone();

    if previous_scene_index != scene_index {
        reset_sign_state_for_scene(&mut signs);
        teleport_wanderer_to_scene(&mut wanderer_query, &scene, &world_map);
        tracing::info!(
            target: "dao_game::presentation::scene",
            scene = scene.name,
            description = scene.description,
            expected_omen = ?scene.expected_omen,
            "presentation scene activated"
        );
    }

    control.time_override = Some(scene.time_override);
    control.wander_target = Some(scene.wander_target);
    control.wander_speed_multiplier = if scene_progress < 0.4 { 1.3 } else { 0.35 };

    if let Some(expected_omen) = scene.expected_omen {
        nudge_sign_state_for_showcase(&mut signs, &config, scene_progress, expected_omen);
    }
}

fn drive_presentation_camera(
    time: Res<Time>,
    config: Res<AppConfig>,
    director: Option<Res<PresentationDirector>>,
    mut query: Query<&mut Transform, With<WorldCamera>>,
) {
    if !config.presentation.enabled {
        return;
    }
    let Some(director) = director else {
        return;
    };
    let Some(mut transform) = query.iter_mut().next() else {
        return;
    };

    let scene = director.current_scene();
    let desired_position = scene.focus + scene.camera_offset;
    let blend = 1.0 - (-config.presentation.camera_blend_speed.max(0.1) * time.delta_secs()).exp();

    transform.translation = transform.translation.lerp(desired_position, blend);
    transform.look_at(scene.focus, Vec3::Y);
}

fn build_scene(
    name: &'static str,
    description: &'static str,
    anchor: SceneAnchor,
    world_map: &WorldMap,
    camera_offset: Vec3,
    time_override: f32,
    expected_omen: Option<OmenKind>,
) -> PresentationScene {
    let base_focus = world_map.tile_translation(anchor.x, anchor.z, anchor.tile.height());
    let focus = Vec3::new(
        base_focus.x,
        anchor.tile.height().max(world_map.water_level()) + 0.9,
        base_focus.z,
    );
    let wander_target = Vec3::new(
        base_focus.x,
        anchor.tile.height().max(world_map.water_level()) + 1.2,
        base_focus.z,
    );

    PresentationScene {
        name,
        description,
        focus,
        camera_offset,
        wander_target,
        time_override,
        expected_omen,
    }
}

fn find_anchor(world_map: &WorldMap, biome: BiomeKind) -> Option<SceneAnchor> {
    let mut best: Option<SceneAnchor> = None;
    let search_radius = world_map.showcase_search_radius();

    for z in -search_radius..=search_radius {
        for x in -search_radius..=search_radius {
            let Some(tile) = world_map.tile_at_grid(x, z) else {
                continue;
            };
            if tile.biome() != biome {
                continue;
            }

            let candidate = SceneAnchor { x, z, tile };
            match best {
                None => best = Some(candidate),
                Some(current) if anchor_score(candidate, biome) > anchor_score(current, biome) => {
                    best = Some(candidate);
                }
                _ => {}
            }
        }
    }

    best
}

fn anchor_score(anchor: SceneAnchor, biome: BiomeKind) -> f32 {
    match biome {
        BiomeKind::Water => -anchor.tile.height(),
        BiomeKind::Meadow => anchor.tile.moisture() - anchor.tile.slope() * 0.2,
        BiomeKind::Grove => anchor.tile.moisture() + anchor.tile.height() * 0.04,
        BiomeKind::Steppe => -anchor.tile.moisture() + anchor.tile.height() * 0.03,
        BiomeKind::Ridge => anchor.tile.height() + anchor.tile.slope() * 0.5,
    }
}

fn scene_index_at_elapsed(elapsed: f32, scene_duration: f32, scene_count: usize) -> usize {
    (((elapsed / scene_duration).floor() as usize) % scene_count.max(1))
        .min(scene_count.saturating_sub(1))
}

fn scene_progress(elapsed: f32, scene_duration: f32) -> f32 {
    (elapsed / scene_duration).fract()
}

fn reset_sign_state_for_scene(signs: &mut SignState) {
    signs.resonance = 0.18;
    signs.calm = 0.92;
    signs.omen_triggered = false;
    signs.current_omen = None;
}

fn teleport_wanderer_to_scene(
    wanderer_query: &mut Query<&mut Transform, With<WandererPrototype>>,
    scene: &PresentationScene,
    _world_map: &WorldMap,
) {
    let Some(mut transform) = wanderer_query.iter_mut().next() else {
        return;
    };
    transform.translation = scene.wander_target;
}

fn nudge_sign_state_for_showcase(
    signs: &mut SignState,
    config: &AppConfig,
    scene_progress: f32,
    expected_omen: OmenKind,
) {
    let threshold = config.signs.resonance_threshold;
    let calm_threshold = config.signs.calm_threshold;

    if scene_progress > 0.45 {
        signs.calm = signs.calm.max(calm_threshold + 0.08);
    }
    if scene_progress > 0.58 {
        signs.resonance = signs.resonance.max(threshold + 0.02);
    }
    if scene_progress > 0.72 && !signs.omen_triggered {
        signs.current_omen = Some(expected_omen);
    }
}

#[cfg(test)]
mod tests {
    use super::{scene_index_at_elapsed, scene_progress};

    #[test]
    fn scene_index_wraps_across_full_cycle() {
        assert_eq!(scene_index_at_elapsed(0.2, 7.0, 4), 0);
        assert_eq!(scene_index_at_elapsed(7.2, 7.0, 4), 1);
        assert_eq!(scene_index_at_elapsed(28.1, 7.0, 4), 0);
    }

    #[test]
    fn scene_progress_stays_within_single_scene() {
        let progress = scene_progress(15.75, 7.0);
        assert!(progress > 0.0);
        assert!(progress < 1.0);
    }
}
