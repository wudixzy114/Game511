use bevy::prelude::*;

use crate::{
    core::{
        config::AppConfig,
        performance::{FramePerformance, PerformancePhase},
    },
    game::{
        environment::WeatherKind,
        flow::{AppScreen, InGameState, SessionMode, in_session_mode},
        journey::{
            DreamPhase, JourneyAdvanceContext, JourneyStage, JourneyState, StoryArcStage,
            advance_journey_state,
        },
        places::{MeaningfulPlaces, PlaceKind, planar_distance},
        signs::{OmenKind, SignState},
        village::{VillageAreaKind, VillageState},
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
            (initialize_presentation, advance_presentation_director)
                .chain()
                .run_if(in_state(InGameState::Running))
                .run_if(in_session_mode(SessionMode::Presentation)),
        );
        app.add_systems(
            Update,
            drive_presentation_camera
                .run_if(in_state(InGameState::Running))
                .run_if(in_session_mode(SessionMode::Presentation)),
        );
        app.add_systems(OnExit(AppScreen::InGame), cleanup_presentation_session);
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
    weather: WeatherKind,
    expected_omen: Option<OmenKind>,
    journey_step: PresentationJourneyStep,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PresentationJourneyStep {
    Scenic,
    VillageBirth,
    VillageLife,
    Dream,
    DreamEcho,
    Spawn,
    Omen,
    Approach,
    Response,
    Echo,
}

#[derive(Debug, Clone, Copy)]
struct SceneAnchor {
    x: i32,
    z: i32,
    tile: TerrainTile,
}

#[derive(Debug, Clone, Copy)]
struct SceneVisuals {
    camera_offset: Vec3,
    time_override: f32,
    weather: WeatherKind,
    expected_omen: Option<OmenKind>,
}

fn initialize_presentation(
    mut commands: Commands,
    config: Res<AppConfig>,
    world_map: Option<Res<WorldMap>>,
    places: Option<Res<MeaningfulPlaces>>,
    village: Option<Res<VillageState>>,
    director: Option<Res<PresentationDirector>>,
) {
    if director.is_some() {
        return;
    }
    let Some(world_map) = world_map else {
        return;
    };
    let Some(places) = places else {
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
    let steppe_anchor = find_anchor(&world_map, BiomeKind::Steppe).unwrap_or(meadow_anchor);

    let scenes =
        build_presentation_scenes(village.as_deref(), &places, &world_map).unwrap_or_else(|| {
            vec![
                build_scene(
                    "Panorama Sweep",
                    "overview of terrain layers, clear daylight and long-range sky tone",
                    meadow_anchor,
                    &world_map,
                    SceneVisuals {
                        camera_offset: Vec3::new(-15.0, 9.5, 15.0),
                        time_override: 0.18,
                        weather: WeatherKind::Clear,
                        expected_omen: None,
                    },
                    PresentationJourneyStep::Scenic,
                ),
                build_scene(
                    "Grove Whisper",
                    "misty grove atmosphere with close-range vegetation silhouettes",
                    grove_anchor,
                    &world_map,
                    SceneVisuals {
                        camera_offset: Vec3::new(-6.0, 4.8, 7.0),
                        time_override: 0.34,
                        weather: WeatherKind::Mist,
                        expected_omen: Some(OmenKind::GroveWhisper),
                    },
                    PresentationJourneyStep::Scenic,
                ),
                build_scene(
                    "Ridge Dawn",
                    "sunrise lighting shift with a clear east-west solar arc",
                    ridge_anchor,
                    &world_map,
                    SceneVisuals {
                        camera_offset: Vec3::new(-9.0, 6.2, 10.0),
                        time_override: 0.02,
                        weather: WeatherKind::Clear,
                        expected_omen: Some(OmenKind::DawnLight),
                    },
                    PresentationJourneyStep::Scenic,
                ),
                build_scene(
                    "Storm Front",
                    "heavy rain, dark atmosphere and lightning-driven contrast",
                    meadow_anchor,
                    &world_map,
                    SceneVisuals {
                        camera_offset: Vec3::new(-8.5, 5.4, 9.5),
                        time_override: 0.46,
                        weather: WeatherKind::Storm,
                        expected_omen: None,
                    },
                    PresentationJourneyStep::Scenic,
                ),
                build_scene(
                    "Snow Ridge",
                    "cold snowfall, reduced visibility and summit response",
                    ridge_anchor,
                    &world_map,
                    SceneVisuals {
                        camera_offset: Vec3::new(-10.0, 7.0, 11.5),
                        time_override: 0.61,
                        weather: WeatherKind::Snow,
                        expected_omen: Some(OmenKind::SummitCall),
                    },
                    PresentationJourneyStep::Scenic,
                ),
                build_scene(
                    "Starfield Watch",
                    "clear midnight sky for star visibility and moon transition",
                    steppe_anchor,
                    &world_map,
                    SceneVisuals {
                        camera_offset: Vec3::new(-7.2, 4.5, 8.6),
                        time_override: 0.76,
                        weather: WeatherKind::Clear,
                        expected_omen: None,
                    },
                    PresentationJourneyStep::Scenic,
                ),
                build_scene(
                    "Moonlit Water",
                    "waterline calm test with moonlight and cool omen response",
                    water_anchor,
                    &world_map,
                    SceneVisuals {
                        camera_offset: Vec3::new(0.0, 4.2, 8.5),
                        time_override: 0.84,
                        weather: WeatherKind::Clear,
                        expected_omen: Some(OmenKind::StillWater),
                    },
                    PresentationJourneyStep::Scenic,
                ),
            ]
        });

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
    resources: (
        Res<Time>,
        Res<AppConfig>,
        ResMut<FramePerformance>,
        Res<WorldMap>,
    ),
    director: Option<ResMut<PresentationDirector>>,
    control: Option<ResMut<WorldPresentationControl>>,
    mut signs: ResMut<SignState>,
    mut journey: Option<ResMut<JourneyState>>,
    mut wanderer_query: Query<&mut Transform, With<WandererPrototype>>,
) {
    let (time, config, mut performance, world_map) = resources;
    let started_at = std::time::Instant::now();
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
    control.weather_override = Some(scene.weather);
    control.wander_target = Some(scene.wander_target);
    control.wander_speed_multiplier = if scene_progress < 0.4 { 1.3 } else { 0.35 };

    if let Some(expected_omen) = scene.expected_omen {
        nudge_sign_state_for_showcase(&mut signs, &config, scene_progress, expected_omen);
    }
    if let Some(journey) = journey.as_deref_mut() {
        drive_journey_showcase(journey, &mut signs, &scene, scene_progress);
    }
    performance.record_phase_duration(PerformancePhase::Presentation, started_at.elapsed());
}

fn drive_presentation_camera(
    time: Res<Time>,
    config: Res<AppConfig>,
    director: Option<Res<PresentationDirector>>,
    mut query: Query<&mut Transform, With<WorldCamera>>,
) {
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
    visuals: SceneVisuals,
    journey_step: PresentationJourneyStep,
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
        camera_offset: visuals.camera_offset,
        wander_target,
        time_override: visuals.time_override,
        weather: visuals.weather,
        expected_omen: visuals.expected_omen,
        journey_step,
    }
}

fn build_journey_scenes(
    places: &MeaningfulPlaces,
    world_map: &WorldMap,
) -> Option<Vec<PresentationScene>> {
    let place = places.active_place().or_else(|| places.places.first())?;
    let target = place.position + Vec3::Y * 1.1;
    let direction = Vec3::new(-0.72, 0.0, 0.52).normalize();
    let origin = target - direction * 72.0;
    let mid = target - direction * 28.0;
    let near = target - direction * (place.interaction_radius * 0.72).max(4.0);
    let omen = omen_for_place(place.kind);

    Some(vec![
        build_journey_scene(
            "Journey Birth",
            "first playable journey begins at the spawn horizon",
            origin,
            target,
            world_map,
            Vec3::new(-10.0, 5.6, 10.5),
            0.16,
            WeatherKind::Clear,
            Some(omen),
            PresentationJourneyStep::Spawn,
        ),
        build_journey_scene(
            "Journey Omen",
            "sustained omen points through light, mist and distance rather than a route marker",
            mid,
            target,
            world_map,
            Vec3::new(-8.0, 4.8, 8.0),
            0.04,
            WeatherKind::Mist,
            Some(omen),
            PresentationJourneyStep::Omen,
        ),
        build_journey_scene(
            "Journey Approach",
            "wanderer enters the meaningful place and the omen changes intensity",
            near,
            target,
            world_map,
            Vec3::new(-5.2, 3.8, 5.8),
            0.28,
            WeatherKind::Clear,
            Some(omen),
            PresentationJourneyStep::Approach,
        ),
        build_journey_scene(
            "Journey Response",
            "light interaction causes environmental response and omen response",
            near,
            target,
            world_map,
            Vec3::new(-4.5, 3.2, 4.6),
            0.49,
            WeatherKind::Mist,
            Some(omen),
            PresentationJourneyStep::Response,
        ),
        build_journey_scene(
            "Journey Echo",
            "journey memory keeps the experience without becoming a task list",
            near + direction * 2.0,
            target,
            world_map,
            Vec3::new(-5.8, 4.0, 6.4),
            0.68,
            WeatherKind::Clear,
            Some(omen),
            PresentationJourneyStep::Echo,
        ),
    ])
}

fn build_presentation_scenes(
    village: Option<&VillageState>,
    places: &MeaningfulPlaces,
    world_map: &WorldMap,
) -> Option<Vec<PresentationScene>> {
    let mut scenes = Vec::new();
    if let Some(village) = village {
        scenes.extend(build_village_scenes(village, world_map));
    }
    if let Some(mut journey_scenes) = build_journey_scenes(places, world_map) {
        scenes.append(&mut journey_scenes);
    }
    (!scenes.is_empty()).then_some(scenes)
}

fn build_village_scenes(village: &VillageState, world_map: &WorldMap) -> Vec<PresentationScene> {
    let sheep_pen = village
        .area(VillageAreaKind::SheepPen)
        .map(|area| area.position)
        .unwrap_or(village.origin + Vec3::new(18.0, 0.0, -12.0));
    let market = village
        .area(VillageAreaKind::Market)
        .map(|area| area.position)
        .unwrap_or(village.origin + Vec3::new(-16.0, 0.0, -8.0));
    let shore = village
        .area(VillageAreaKind::Shore)
        .map(|area| area.position)
        .unwrap_or(village.origin + Vec3::new(0.0, 0.0, 30.0));
    let outer = village
        .area(VillageAreaKind::OuterPath)
        .map(|area| area.position)
        .unwrap_or(village.origin + Vec3::new(0.0, 0.0, -36.0));

    vec![
        village_scene(
            "Village Birth",
            "opening village spawn without task prompts",
            village.spawn_point,
            village.origin + Vec3::Y * 1.2,
            world_map,
            Vec3::new(-10.0, 6.0, 12.0),
            0.18,
            WeatherKind::Clear,
            PresentationJourneyStep::VillageBirth,
        ),
        village_scene(
            "Village Flock",
            "sheep pen, shepherd and quiet daily life",
            sheep_pen,
            sheep_pen + Vec3::Y * 1.0,
            world_map,
            Vec3::new(-7.0, 4.6, 8.0),
            0.3,
            WeatherKind::Clear,
            PresentationJourneyStep::VillageLife,
        ),
        village_scene(
            "Village Market",
            "merchant rumor and human place before departure",
            market,
            market + Vec3::Y * 1.0,
            world_map,
            Vec3::new(-6.5, 4.5, 7.0),
            0.42,
            WeatherKind::Mist,
            PresentationJourneyStep::VillageLife,
        ),
        village_scene(
            "Pyramid Dream",
            "hard-coded first dream of desert, sandstorm and pyramid",
            shore,
            shore + Vec3::Y * 1.0,
            world_map,
            Vec3::new(-8.0, 5.8, 9.0),
            0.74,
            WeatherKind::Storm,
            PresentationJourneyStep::Dream,
        ),
        village_scene(
            "Dream Afterglow",
            "dream echo becomes non-task omen toward the outer path",
            outer,
            outer + Vec3::Y * 1.0,
            world_map,
            Vec3::new(-9.0, 5.2, 10.0),
            0.05,
            WeatherKind::Mist,
            PresentationJourneyStep::DreamEcho,
        ),
    ]
}

#[allow(clippy::too_many_arguments)]
fn village_scene(
    name: &'static str,
    description: &'static str,
    wander_target: Vec3,
    focus: Vec3,
    world_map: &WorldMap,
    camera_offset: Vec3,
    time_override: f32,
    weather: WeatherKind,
    journey_step: PresentationJourneyStep,
) -> PresentationScene {
    let focus_height = world_map
        .sample_height(focus.x, focus.z)
        .unwrap_or(focus.y)
        .max(world_map.water_level())
        + 1.25;
    let wander_height = world_map
        .sample_height(wander_target.x, wander_target.z)
        .unwrap_or(wander_target.y)
        .max(world_map.water_level())
        + 1.2;
    PresentationScene {
        name,
        description,
        focus: Vec3::new(focus.x, focus_height, focus.z),
        camera_offset,
        wander_target: Vec3::new(wander_target.x, wander_height, wander_target.z),
        time_override,
        weather,
        expected_omen: Some(OmenKind::DawnLight),
        journey_step,
    }
}

#[allow(clippy::too_many_arguments)]
fn build_journey_scene(
    name: &'static str,
    description: &'static str,
    wander_target: Vec3,
    focus: Vec3,
    world_map: &WorldMap,
    camera_offset: Vec3,
    time_override: f32,
    weather: WeatherKind,
    expected_omen: Option<OmenKind>,
    journey_step: PresentationJourneyStep,
) -> PresentationScene {
    let focus_height = world_map
        .sample_height(focus.x, focus.z)
        .unwrap_or(focus.y)
        .max(world_map.water_level())
        + 1.25;
    let wander_height = world_map
        .sample_height(wander_target.x, wander_target.z)
        .unwrap_or(wander_target.y)
        .max(world_map.water_level())
        + 1.2;
    PresentationScene {
        name,
        description,
        focus: Vec3::new(focus.x, focus_height, focus.z),
        camera_offset,
        wander_target: Vec3::new(wander_target.x, wander_height, wander_target.z),
        time_override,
        weather,
        expected_omen,
        journey_step,
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
    *signs = SignState {
        resonance: 0.18,
        calm: 0.92,
        ..Default::default()
    };
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

fn drive_journey_showcase(
    journey: &mut JourneyState,
    signs: &mut SignState,
    scene: &PresentationScene,
    scene_progress: f32,
) {
    let Some(target) = journey.target else {
        return;
    };
    let distance = planar_distance(scene.wander_target, target.position);
    let omen_triggered = scene_progress > 0.2 && scene.expected_omen.is_some();
    if scene.journey_step == PresentationJourneyStep::Spawn
        && journey.stage != JourneyStage::FirstArrival
    {
        return;
    }
    if matches!(
        scene.journey_step,
        PresentationJourneyStep::VillageBirth
            | PresentationJourneyStep::VillageLife
            | PresentationJourneyStep::Dream
            | PresentationJourneyStep::DreamEcho
    ) {
        match scene.journey_step {
            PresentationJourneyStep::VillageBirth => {
                journey.story_stage = StoryArcStage::VillageAwakening;
                journey.dream.phase = DreamPhase::Unseen;
            }
            PresentationJourneyStep::VillageLife => {
                journey.story_stage = StoryArcStage::VillageLife;
                journey.dream.phase = DreamPhase::Unseen;
            }
            PresentationJourneyStep::Dream => {
                journey.story_stage = StoryArcStage::Dreaming;
                journey.dream.phase = DreamPhase::InDream;
                journey.dream.seen_pyramid = true;
                journey.dream.echo_strength = 1.0;
                signs.omen_triggered = true;
                signs.current_omen = Some(OmenKind::DawnLight);
                signs.omen_intensity = signs.omen_intensity.max(0.96);
            }
            PresentationJourneyStep::DreamEcho => {
                journey.story_stage = StoryArcStage::DreamAfterglow;
                journey.dream.phase = DreamPhase::Afterglow;
                journey.dream.seen_pyramid = true;
                journey.dream.echo_strength = 0.84;
                signs.omen_triggered = true;
                signs.current_omen = Some(OmenKind::DawnLight);
                signs.omen_intensity = signs.omen_intensity.max(0.78);
            }
            _ => {}
        }
        return;
    }

    match scene.journey_step {
        PresentationJourneyStep::Scenic => {}
        PresentationJourneyStep::VillageBirth
        | PresentationJourneyStep::VillageLife
        | PresentationJourneyStep::Dream
        | PresentationJourneyStep::DreamEcho => {}
        PresentationJourneyStep::Spawn => {
            let _ = advance_journey_state(
                journey,
                presentation_context(scene, distance + 40.0, false, 0.2),
            );
        }
        PresentationJourneyStep::Omen => {
            signs.omen_triggered = omen_triggered;
            signs.current_omen = scene.expected_omen;
            signs.omen_intensity = signs.omen_intensity.max(0.58);
            let _ = advance_journey_state(
                journey,
                presentation_context(scene, distance.max(28.0), true, 0.92),
            );
        }
        PresentationJourneyStep::Approach => {
            signs.omen_triggered = true;
            signs.current_omen = scene.expected_omen;
            signs.omen_intensity = signs.omen_intensity.max(0.78);
            let _ = advance_journey_state(
                journey,
                presentation_context(scene, target.arrival_radius * 0.72, true, 0.95),
            );
        }
        PresentationJourneyStep::Response => {
            signs.omen_triggered = true;
            signs.current_omen = scene.expected_omen;
            signs.omen_intensity = signs.omen_intensity.max(0.94);
            for _ in 0..2 {
                let _ = advance_journey_state(
                    journey,
                    presentation_context(scene, target.interaction_radius * 0.5, true, 1.0),
                );
            }
        }
        PresentationJourneyStep::Echo => {
            signs.omen_triggered = true;
            signs.current_omen = scene.expected_omen;
            signs.omen_intensity = signs.omen_intensity.max(0.66);
            let _ = advance_journey_state(
                journey,
                presentation_context(scene, target.interaction_radius * 0.5, true, 1.0),
            );
        }
    }
}

fn presentation_context(
    scene: &PresentationScene,
    distance_to_target: f32,
    omen_triggered: bool,
    delta_seconds: f32,
) -> JourneyAdvanceContext {
    JourneyAdvanceContext {
        delta_seconds,
        player_position: scene.wander_target,
        distance_to_target: Some(distance_to_target),
        target_arrival_radius: None,
        target_interaction_radius: None,
        look_alignment_to_target: Some(0.96),
        calm: 0.92,
        omen_triggered,
        current_omen: if omen_triggered {
            scene.expected_omen
        } else {
            None
        },
        village_focus: false,
        leaving_village: false,
    }
}

fn omen_for_place(kind: PlaceKind) -> OmenKind {
    match kind {
        PlaceKind::AncientTree => OmenKind::GroveWhisper,
        PlaceKind::SpringEye | PlaceKind::QuietBay => OmenKind::StillWater,
        PlaceKind::RidgeGate => OmenKind::SummitCall,
        PlaceKind::StoneRing => OmenKind::DawnLight,
    }
}

fn cleanup_presentation_session(mut commands: Commands) {
    commands.remove_resource::<PresentationDirector>();
    commands.remove_resource::<WorldPresentationControl>();
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use bevy::prelude::Vec3;

    use crate::{
        core::config::{
            AppConfig, EnvironmentConfig, PlayerConfig, PresentationConfig, QualityConfig,
            SignConfig, WorldConfig,
        },
        game::{
            places::{MeaningfulPlace, MeaningfulPlaces, PlaceKind, PlaceTag},
            presentation::{
                PresentationJourneyStep, build_journey_scenes, omen_for_place,
                scene_index_at_elapsed, scene_progress,
            },
            signs::OmenKind,
            world::{BiomeKind, WorldGridCoord, WorldMap},
        },
    };

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

    #[test]
    fn journey_presentation_covers_first_playable_loop() {
        let config = test_config();
        let world_map = WorldMap::new_for_testing(42, &config);
        let places = MeaningfulPlaces {
            places: vec![MeaningfulPlace {
                id: 7,
                kind: PlaceKind::StoneRing,
                grid: WorldGridCoord { x: 18, z: -12 },
                position: Vec3::new(40.0, 1.0, -24.0),
                biome: BiomeKind::Meadow,
                tags: vec![PlaceTag::Memory],
                score: 0.9,
                arrival_radius: 14.0,
                interaction_radius: 7.0,
            }],
            active_place_id: Some(7),
            nearest_place_id: None,
            nearest_distance: None,
        };

        let scenes = build_journey_scenes(&places, &world_map).expect("journey scenes");
        let steps: Vec<PresentationJourneyStep> =
            scenes.iter().map(|scene| scene.journey_step).collect();

        assert_eq!(
            steps,
            vec![
                PresentationJourneyStep::Spawn,
                PresentationJourneyStep::Omen,
                PresentationJourneyStep::Approach,
                PresentationJourneyStep::Response,
                PresentationJourneyStep::Echo,
            ]
        );
        assert!(
            scenes
                .iter()
                .all(|scene| scene.expected_omen == Some(OmenKind::DawnLight))
        );
    }

    #[test]
    fn place_kind_maps_to_showcase_omen() {
        assert_eq!(
            omen_for_place(PlaceKind::AncientTree),
            OmenKind::GroveWhisper
        );
        assert_eq!(omen_for_place(PlaceKind::SpringEye), OmenKind::StillWater);
        assert_eq!(omen_for_place(PlaceKind::RidgeGate), OmenKind::SummitCall);
        assert_eq!(omen_for_place(PlaceKind::StoneRing), OmenKind::DawnLight);
    }

    fn test_config() -> AppConfig {
        AppConfig {
            window_title: "Dao".to_string(),
            log_directory: PathBuf::from("logs"),
            performance_log_name: "performance.log".to_string(),
            frame_log_interval: 60,
            performance_detail_interval: 1,
            presentation: PresentationConfig {
                enabled: false,
                scene_duration_seconds: 7.0,
                camera_blend_speed: 2.0,
            },
            world: WorldConfig {
                seed: 42,
                world_radius: 64,
                chunk_radius: 4,
                cell_size: 3.2,
                terrain_subdivisions: 6,
                terrain_scale: 18.0,
                height_variation: 6.0,
                water_level: -0.2,
                noise_octaves: 5,
                ridge_sharpness: 2.1,
                shoreline_blend: 0.2,
                river_frequency: 0.19,
                river_depth: 0.72,
                erosion_strength: 0.52,
                sediment_bias: 0.28,
                visible_chunk_radius: 2,
                high_detail_chunk_radius: 1,
                low_detail_chunk_radius: 2,
                preload_chunk_radius: 3,
                impostor_chunk_radius: 6,
                impostor_radial_bands: 3,
                impostor_angular_segments: 32,
                showcase_search_radius: 24,
                streaming_chunk_budget: 1,
                background_generation_budget: 2,
                streaming_cache_capacity: 32,
                collision_proxy_radius: 1,
                collision_subdivisions: 8,
                collision_chunk_budget: 1,
                collision_cache_capacity: 16,
                material_texture_resolution: 64,
            },
            environment: EnvironmentConfig {
                day_length_seconds: 180.0,
                wander_radius: 4.5,
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
                resonance_threshold: 0.7,
                resonance_smoothing: 0.12,
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
}
