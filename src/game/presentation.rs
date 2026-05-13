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
        landmarks::{LandmarkState, PyramidSignal},
        places::{MeaningfulPlaces, PlaceKind, planar_distance},
        player::{CameraMode, FirstPersonState},
        regions::{RegionGraphState, TransitionGateState},
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
    activated_scene_index: Option<usize>,
    scenes: Vec<PresentationScene>,
}

impl PresentationDirector {
    fn current_scene(&self) -> &PresentationScene {
        &self.scenes[self.current_scene_index]
    }

    fn has_full_visual_baseline(&self) -> bool {
        visual_baseline_count(&self.scenes) >= PresentationVisualSample::BASELINE.len()
    }
}

#[derive(Debug, Clone)]
struct PresentationScene {
    name: &'static str,
    description: &'static str,
    visual_sample: PresentationVisualSample,
    composition: CompositionGuide,
    focus: Vec3,
    camera_offset: Vec3,
    camera_drift: Vec3,
    camera_mode: CameraMode,
    wander_start: Vec3,
    wander_target: Vec3,
    time_override: f32,
    weather: WeatherKind,
    expected_omen: Option<OmenKind>,
    journey_step: PresentationJourneyStep,
}

impl PresentationScene {
    fn with_visual_sample(
        mut self,
        visual_sample: PresentationVisualSample,
        camera_mode: CameraMode,
        composition: CompositionGuide,
    ) -> Self {
        self.visual_sample = visual_sample;
        self.camera_mode = camera_mode;
        self.composition = composition;
        self
    }

    fn with_camera_drift(mut self, camera_drift: Vec3) -> Self {
        self.camera_drift = camera_drift;
        self
    }

    fn with_wander_start(mut self, wander_start: Vec3, world_map: &WorldMap) -> Self {
        let wander_height = world_map
            .sample_height(wander_start.x, wander_start.z)
            .unwrap_or(wander_start.y)
            .max(world_map.water_level())
            + 1.2;
        self.wander_start = Vec3::new(wander_start.x, wander_height, wander_start.z);
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PresentationVisualSample {
    Functional,
    VillageDawn,
    PostDreamDeparture,
    DreamAfterSeaWind,
    BoundaryCrossing,
    FarBankArrival,
    TownPreparation,
    FirstLoss,
    DesertDeparture,
    MistBoundary,
    SandstormPyramid,
    OasisRuins,
    NightSea,
}

impl PresentationVisualSample {
    const BASELINE: [Self; 6] = [
        Self::VillageDawn,
        Self::PostDreamDeparture,
        Self::DreamAfterSeaWind,
        Self::BoundaryCrossing,
        Self::FarBankArrival,
        Self::MistBoundary,
    ];

    fn label(self) -> &'static str {
        match self {
            Self::Functional => "functional-showcase",
            Self::VillageDawn => "village-dawn",
            Self::PostDreamDeparture => "post-dream-departure",
            Self::DreamAfterSeaWind => "dream-after-sea-wind",
            Self::BoundaryCrossing => "boundary-crossing",
            Self::FarBankArrival => "far-bank-arrival",
            Self::TownPreparation => "town-preparation",
            Self::FirstLoss => "first-loss",
            Self::DesertDeparture => "desert-departure",
            Self::MistBoundary => "mist-boundary",
            Self::SandstormPyramid => "sandstorm-pyramid",
            Self::OasisRuins => "oasis-ruins",
            Self::NightSea => "night-sea",
        }
    }

    fn order(self) -> usize {
        Self::BASELINE
            .iter()
            .position(|sample| *sample == self)
            .unwrap_or(Self::BASELINE.len())
    }

    fn is_visual_baseline(self) -> bool {
        self != Self::Functional
    }
}

#[derive(Debug, Clone, Copy)]
struct CompositionGuide {
    foreground: &'static str,
    midground: &'static str,
    background: &'static str,
    quality_gate: &'static str,
}

impl CompositionGuide {
    const fn functional() -> Self {
        Self {
            foreground: "system anchor",
            midground: "showcase target",
            background: "terrain and atmosphere",
            quality_gate: "scene remains readable without task UI",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PresentationJourneyStep {
    Scenic,
    VillageBirth,
    VillageLife,
    Dream,
    DreamEcho,
    PostDreamDeparture,
    Boundary,
    BoundaryCrossing,
    FarBankArrival,
    TownPreparation,
    FirstLoss,
    DesertDeparture,
    DesertPyramid,
    Ecology,
    ThirdPerson,
    Director,
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

type PresentationInitResources<'w> = (
    Res<'w, AppConfig>,
    Option<Res<'w, WorldMap>>,
    Option<Res<'w, MeaningfulPlaces>>,
    Option<Res<'w, VillageState>>,
    Option<Res<'w, RegionGraphState>>,
    Option<Res<'w, LandmarkState>>,
    Option<Res<'w, PresentationDirector>>,
);

type PresentationDriveResources<'w> = (
    Res<'w, Time>,
    Res<'w, AppConfig>,
    Res<'w, FramePerformance>,
    Res<'w, WorldMap>,
    Option<ResMut<'w, PresentationDirector>>,
    Option<ResMut<'w, WorldPresentationControl>>,
    ResMut<'w, SignState>,
    Option<ResMut<'w, JourneyState>>,
    Option<ResMut<'w, RegionGraphState>>,
    Option<ResMut<'w, LandmarkState>>,
    Option<ResMut<'w, FirstPersonState>>,
);

fn initialize_presentation(mut commands: Commands, resources: PresentationInitResources<'_>) {
    let (config, world_map, places, village, regions, landmarks, director) = resources;
    if director
        .as_deref()
        .is_some_and(PresentationDirector::has_full_visual_baseline)
    {
        return;
    }
    let Some(world_map) = world_map else {
        return;
    };
    let Some(places) = places else {
        return;
    };
    if village.is_none() || regions.is_none() || landmarks.is_none() {
        return;
    }
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

    let scenes = build_presentation_scenes(
        village.as_deref(),
        regions.as_deref(),
        landmarks.as_deref(),
        &places,
        &world_map,
    )
    .unwrap_or_else(|| {
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
            )
            .with_camera_drift(Vec3::new(3.2, -0.4, -2.8)),
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
            )
            .with_camera_drift(Vec3::new(1.6, 0.1, -1.2)),
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
            )
            .with_camera_drift(Vec3::new(2.4, 0.3, -2.0)),
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
            )
            .with_camera_drift(Vec3::new(1.1, 0.0, -1.8)),
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
            )
            .with_camera_drift(Vec3::new(2.0, 0.2, -2.4)),
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
            )
            .with_visual_sample(
                PresentationVisualSample::NightSea,
                CameraMode::FirstPerson,
                CompositionGuide {
                    foreground: "quiet traveler silhouette or shoreline",
                    midground: "dark waterline and low horizon",
                    background: "moon, star field and cool fog",
                    quality_gate: "sky remains readable without UI or debug overlays",
                },
            )
            .with_camera_drift(Vec3::new(0.8, 0.15, -1.1)),
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
            )
            .with_visual_sample(
                PresentationVisualSample::NightSea,
                CameraMode::FirstPerson,
                CompositionGuide {
                    foreground: "water edge and low land shape",
                    midground: "moonlit water surface",
                    background: "clear night sky and distant shore",
                    quality_gate: "water, sky and horizon are not collapsed into a dark flat plane",
                },
            )
            .with_camera_drift(Vec3::new(0.6, 0.0, -1.4)),
        ]
    });

    tracing::info!(
        target: "dao_game::presentation",
        scene_count = scenes.len(),
        visual_baseline_count = visual_baseline_count(&scenes),
        scene_duration_seconds = config.presentation.scene_duration_seconds,
        "presentation mode initialized"
    );
    log_visual_baseline_matrix(&scenes);

    commands.insert_resource(PresentationDirector {
        scene_duration: config.presentation.scene_duration_seconds.max(1.0),
        elapsed: 0.0,
        current_scene_index: 0,
        activated_scene_index: None,
        scenes,
    });
    commands.insert_resource(WorldPresentationControl::default());
}

fn advance_presentation_director(
    resources: PresentationDriveResources<'_>,
    mut wanderer_query: Query<&mut Transform, With<WandererPrototype>>,
) {
    let (
        time,
        config,
        performance,
        world_map,
        director,
        control,
        mut signs,
        mut journey,
        mut regions,
        mut landmarks,
        mut camera_state,
    ) = resources;
    let started_at = std::time::Instant::now();
    let Some(mut director) = director else {
        return;
    };
    let Some(mut control) = control else {
        return;
    };

    director.elapsed += time.delta_secs();
    let scene_index = scene_index_at_elapsed(
        director.elapsed,
        director.scene_duration,
        director.scenes.len(),
    );
    let scene_progress = scene_progress(director.elapsed, director.scene_duration);
    director.current_scene_index = scene_index;
    let scene = director.current_scene().clone();

    if director.activated_scene_index != Some(scene_index) {
        reset_sign_state_for_scene(&mut signs);
        teleport_wanderer_to_scene(&mut wanderer_query, &scene, &world_map);
        director.activated_scene_index = Some(scene_index);
        tracing::info!(
            target: "dao_game::presentation::scene",
            scene = scene.name,
            description = scene.description,
            visual_sample = scene.visual_sample.label(),
            camera_mode = scene.camera_mode.label(),
            foreground = scene.composition.foreground,
            midground = scene.composition.midground,
            background = scene.composition.background,
            quality_gate = scene.composition.quality_gate,
            time_override = scene.time_override,
            weather = ?scene.weather,
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
    if let Some(regions) = regions.as_deref_mut() {
        drive_region_showcase(regions, &scene, scene_progress);
    }
    if let Some(landmarks) = landmarks.as_deref_mut() {
        drive_landmark_showcase(landmarks, &scene, scene_progress);
    }
    if let Some(camera_state) = camera_state.as_deref_mut() {
        drive_camera_mode_showcase(camera_state, &scene);
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
    let progress = scene_progress(director.elapsed, director.scene_duration);
    let drift = scene.camera_drift * smoothstep_unit(progress);
    let desired_position = scene.focus + scene.camera_offset + drift;
    let blend = 1.0 - (-config.presentation.camera_blend_speed.max(0.1) * time.delta_secs()).exp();

    transform.translation = transform.translation.lerp(desired_position, blend);
    transform.look_at(scene.focus, Vec3::Y);
}

fn log_visual_baseline_matrix(scenes: &[PresentationScene]) {
    for scene in scenes
        .iter()
        .filter(|scene| scene.visual_sample.is_visual_baseline())
    {
        let manifest = screenshot_manifest_for_scene(scene, "current-date", "working-tree");
        let checklist_keys = manifest
            .checklist
            .iter()
            .map(|item| item.key)
            .collect::<Vec<_>>()
            .join(",");
        let required_metadata = manifest.required_metadata.join(",");
        tracing::info!(
            target: "dao_game::presentation::visual_matrix",
            scene = scene.name,
            visual_sample = scene.visual_sample.label(),
            camera_mode = scene.camera_mode.label(),
            weather = ?scene.weather,
            time_override = scene.time_override,
            foreground = scene.composition.foreground,
            midground = scene.composition.midground,
            background = scene.composition.background,
            quality_gate = scene.composition.quality_gate,
            screenshot_directory_rule = manifest.directory_rule,
            screenshot_scene_label = manifest.scene_label,
            required_metadata,
            screenshot_checklist = checklist_keys,
            "visual baseline scene registered"
        );
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ScreenshotCheckItem {
    key: &'static str,
    description: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ScreenshotManifest {
    directory_rule: String,
    scene_label: String,
    required_metadata: Vec<&'static str>,
    checklist: Vec<ScreenshotCheckItem>,
}

fn screenshot_manifest_for_scene(
    scene: &PresentationScene,
    date: &str,
    commit: &str,
) -> ScreenshotManifest {
    ScreenshotManifest {
        directory_rule: format!(
            "captures/{date}/{:02}-{}-{commit}",
            scene.visual_sample.order() + 1,
            scene.visual_sample.label()
        ),
        scene_label: scene.visual_sample.label().to_string(),
        required_metadata: vec![
            "scene",
            "visual_sample",
            "camera_mode",
            "weather",
            "time_override",
            "config_summary",
            "git_commit",
        ],
        checklist: screenshot_checklist(),
    }
}

fn screenshot_checklist() -> Vec<ScreenshotCheckItem> {
    vec![
        ScreenshotCheckItem {
            key: "landmark_visibility",
            description: "core landmark or local visual focus is readable",
        },
        ScreenshotCheckItem {
            key: "ui_occlusion",
            description: "formal UI does not cover the intended subject",
        },
        ScreenshotCheckItem {
            key: "text_overflow",
            description: "visible text stays inside its container",
        },
        ScreenshotCheckItem {
            key: "material_variety",
            description: "surface colors and details are not flat single-color placeholders",
        },
        ScreenshotCheckItem {
            key: "entity_intersection",
            description: "characters and assets do not visibly float or pierce the ground",
        },
        ScreenshotCheckItem {
            key: "non_blank_frame",
            description: "camera framing contains terrain, sky and intended subject",
        },
    ]
}

fn visual_baseline_count(scenes: &[PresentationScene]) -> usize {
    PresentationVisualSample::BASELINE
        .iter()
        .filter(|sample| scenes.iter().any(|scene| scene.visual_sample == **sample))
        .count()
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
        visual_sample: PresentationVisualSample::Functional,
        composition: CompositionGuide::functional(),
        focus,
        camera_offset: visuals.camera_offset,
        camera_drift: Vec3::ZERO,
        camera_mode: CameraMode::FirstPerson,
        wander_start: wander_target,
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
    regions: Option<&RegionGraphState>,
    landmarks: Option<&LandmarkState>,
    places: &MeaningfulPlaces,
    world_map: &WorldMap,
) -> Option<Vec<PresentationScene>> {
    let mut scenes = Vec::new();
    if let Some(village) = village {
        scenes.extend(build_village_scenes(village, world_map));
    }
    if let Some(regions) = regions {
        scenes.extend(build_region_scenes(regions, world_map));
        scenes.extend(build_director_scenes(regions, world_map));
    }
    if let Some(landmarks) = landmarks {
        scenes.extend(build_landmark_scenes(landmarks, world_map));
    }
    if let Some(mut journey_scenes) = build_journey_scenes(places, world_map) {
        scenes.append(&mut journey_scenes);
    }
    scenes.sort_by_key(|scene| scene.visual_sample.order());
    (!scenes.is_empty()).then_some(scenes)
}

#[cfg(test)]
fn sort_presentation_scenes_for_testing(scenes: &mut [PresentationScene]) {
    scenes.sort_by_key(|scene| scene.visual_sample.order());
}

fn build_village_scenes(village: &VillageState, world_map: &WorldMap) -> Vec<PresentationScene> {
    let houses = village
        .area(VillageAreaKind::Houses)
        .map(|area| area.position)
        .unwrap_or(village.origin + Vec3::new(-8.0, 0.0, 6.0));
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
    let sea_watch = shore + Vec3::new(0.0, 0.0, 11.0);

    vec![
        village_scene(
            "Village Dawn Baseline",
            "warm village dawn with houses, flock life, smoke-ready roofs and a quiet choice to stay",
            sheep_pen + Vec3::new(-5.0, 0.0, 5.0),
            (houses + sheep_pen) * 0.5 + Vec3::Y * 1.2,
            world_map,
            Vec3::new(-14.0, 6.8, 13.5),
            0.04,
            WeatherKind::Clear,
            PresentationJourneyStep::VillageBirth,
        )
        .with_visual_sample(
            PresentationVisualSample::VillageDawn,
            CameraMode::ThirdPerson,
            CompositionGuide {
                foreground: "traveler, sheep pen and warm village ground",
                midground: "houses, well path and daily-life actors",
                background: "low dawn sky and sea-side brightness",
                quality_gate: "opening village reads as a place to remain, with no task wording or debug dominance",
            },
        )
        .with_wander_start(village.spawn_point + Vec3::new(-6.0, 0.0, 5.0), world_map)
        .with_camera_drift(Vec3::new(2.8, -0.2, -2.6)),
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
        )
        .with_camera_drift(Vec3::new(1.4, 0.0, -1.2)),
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
        )
        .with_camera_drift(Vec3::new(1.2, 0.0, -1.0)),
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
        )
        .with_camera_drift(Vec3::new(0.8, 0.15, -1.6)),
        village_scene(
            "Dream After Sea Wind",
            "after the pyramid dream, sea mist and bird direction make the outer path feel newly charged",
            outer,
            (outer + shore) * 0.5 + Vec3::Y * 1.0,
            world_map,
            Vec3::new(-12.0, 5.6, 13.0),
            0.01,
            WeatherKind::Mist,
            PresentationJourneyStep::DreamEcho,
        )
        .with_visual_sample(
            PresentationVisualSample::DreamAfterSeaWind,
            CameraMode::FirstPerson,
            CompositionGuide {
                foreground: "village edge and player-height path",
                midground: "sea mist, birds and outer road glow",
                background: "open coast and pale post-dream dawn",
                quality_gate: "omen is visible as atmosphere, never as an arrow or task marker",
            },
        )
        .with_wander_start(shore + Vec3::new(-4.0, 0.0, -6.0), world_map)
        .with_camera_drift(Vec3::new(2.4, 0.05, -2.2)),
        village_scene(
            "Post Dream Departure",
            "after the pyramid dream, the village road, wind and distant birds make leaving understandable without an objective line",
            outer + Vec3::new(-2.0, 0.0, 4.0),
            outer + Vec3::new(0.0, 1.0, -12.0),
            world_map,
            Vec3::new(-8.5, 4.8, 10.0),
            0.03,
            WeatherKind::Mist,
            PresentationJourneyStep::PostDreamDeparture,
        )
        .with_visual_sample(
            PresentationVisualSample::PostDreamDeparture,
            CameraMode::FirstPerson,
            CompositionGuide {
                foreground: "village edge path and low grass",
                midground: "outer road, moving birds and light wind response",
                background: "fogged river direction beyond the familiar homes",
                quality_gate: "departure feels motivated by world behavior, not by explicit task UI",
            },
        )
        .with_wander_start(village.spawn_point + Vec3::new(0.0, 0.0, -18.0), world_map)
        .with_camera_drift(Vec3::new(1.8, 0.0, -2.0)),
        village_scene(
            "Night Sea Baseline",
            "quiet sea at night with stars and water carrying memory rather than instructions",
            sea_watch,
            shore + Vec3::new(0.0, 1.0, 26.0),
            world_map,
            Vec3::new(-6.0, 3.8, 7.0),
            0.78,
            WeatherKind::Clear,
            PresentationJourneyStep::DreamEcho,
        )
        .with_visual_sample(
            PresentationVisualSample::NightSea,
            CameraMode::FirstPerson,
            CompositionGuide {
                foreground: "shoreline and low traveler viewpoint",
                midground: "dark water band with readable horizon",
                background: "moon, star field and far sea haze",
                quality_gate: "night scene keeps horizon, water and sky separated without UI clutter",
            },
        )
        .with_wander_start(shore + Vec3::new(-6.0, 0.0, 5.0), world_map)
        .with_camera_drift(Vec3::new(0.8, 0.0, -1.6)),
        village_scene(
            "Ecology Omen",
            "birds and village life carry early omen behavior",
            sheep_pen + Vec3::new(0.0, 0.0, -4.0),
            sheep_pen + Vec3::Y * 1.0,
            world_map,
            Vec3::new(-10.0, 6.4, 12.0),
            0.22,
            WeatherKind::Clear,
            PresentationJourneyStep::Ecology,
        )
        .with_camera_drift(Vec3::new(1.8, 0.0, -1.6)),
    ]
}

fn build_region_scenes(regions: &RegionGraphState, world_map: &WorldMap) -> Vec<PresentationScene> {
    regions
        .gates
        .iter()
        .take(2)
        .enumerate()
        .flat_map(|(index, gate)| {
            let scene = build_journey_scene(
                if index == 0 {
                    "Mist Boundary Baseline"
                } else {
                    "Natural Boundary"
                },
                if index == 0 {
                    "old ford, river mist and mountain shadow read as a natural border, not a portal"
                } else {
                    "mist, mountain or harbor boundary appears as part of the world"
                },
                gate.position + Vec3::new(0.0, 0.0, gate.radius * 0.5),
                gate.position + Vec3::Y * 1.3,
                world_map,
                Vec3::new(-10.5, 6.2, 11.5),
                0.08,
                WeatherKind::Mist,
                Some(OmenKind::SummitCall),
                PresentationJourneyStep::Boundary,
            )
            .with_camera_drift(Vec3::new(2.0, 0.1, -2.4));

            if index == 0 {
                let mut scenes = vec![
                    scene
                        .with_visual_sample(
                            PresentationVisualSample::MistBoundary,
                            CameraMode::FirstPerson,
                            CompositionGuide {
                                foreground: "river edge, old crossing ground and low mist",
                                midground: "gate stones or ford silhouette inside fog",
                                background: "mountain or far-bank shadow",
                                quality_gate: "boundary is readable as landscape, not a glowing UI doorway",
                            },
                        )
                        .with_wander_start(
                            gate.position + Vec3::new(-8.0, 0.0, gate.radius * 0.82),
                            world_map,
                        ),
                    build_journey_scene(
                        "Boundary Crossing",
                        "the player stands on old stones inside the mist river crossing rather than touching a portal",
                        gate.position + Vec3::new(-2.0, 0.0, gate.radius * 0.12),
                        gate.position + Vec3::new(3.5, 1.2, -gate.radius * 0.18),
                        world_map,
                        Vec3::new(-6.5, 3.6, 7.0),
                        0.09,
                        WeatherKind::Mist,
                        Some(OmenKind::SummitCall),
                        PresentationJourneyStep::BoundaryCrossing,
                    )
                    .with_visual_sample(
                        PresentationVisualSample::BoundaryCrossing,
                        CameraMode::FirstPerson,
                        CompositionGuide {
                            foreground: "wet ford stones and low water ribbon",
                            midground: "mist opening around the crossing line",
                            background: "far-bank landform beginning to replace the village horizon",
                            quality_gate: "crossing reads as physical movement through terrain, not a teleport effect",
                        },
                    )
                    .with_wander_start(
                        gate.position + Vec3::new(-5.0, 0.0, gate.radius * 0.22),
                        world_map,
                    )
                    .with_camera_drift(Vec3::new(1.0, 0.0, -1.4)),
                ];
                if let Some(outpost) = regions.outpost.as_ref() {
                    scenes.push(
                        build_journey_scene(
                            "Far Bank Arrival",
                            "first far-bank outpost frames the next town-preparation stage without jumping straight to a city",
                            outpost.center + Vec3::new(-6.0, 0.0, 8.0),
                            outpost.center + Vec3::new(1.5, 1.2, 1.0),
                            world_map,
                            Vec3::new(-9.0, 4.8, 9.5),
                            0.2,
                            WeatherKind::Clear,
                            Some(OmenKind::DawnLight),
                            PresentationJourneyStep::FarBankArrival,
                        )
                        .with_visual_sample(
                            PresentationVisualSample::FarBankArrival,
                            CameraMode::ThirdPerson,
                            CompositionGuide {
                                foreground: "traveler and trampled far-bank path",
                                midground: "temporary houses, stall and marker",
                                background: "ridge road toward the eventual town stage",
                                quality_gate: "outpost is a believable transition place, with room for trade and loss hooks",
                            },
                        )
                        .with_wander_start(
                            outpost.center + Vec3::new(-10.0, 0.0, 8.0),
                            world_map,
                        )
                        .with_camera_drift(Vec3::new(2.0, 0.1, -2.0)),
                    );
                }
                scenes.extend(build_town_milestone_scenes(regions, world_map));
                scenes
            } else {
                vec![scene]
            }
        })
        .collect()
}

fn build_town_milestone_scenes(
    regions: &RegionGraphState,
    world_map: &WorldMap,
) -> Vec<PresentationScene> {
    let town = &regions.milestones.town_edge;
    let loss = &regions.milestones.loss_crossroad;
    let desert = &regions.milestones.desert_road;
    vec![
        build_journey_scene(
            "Town Preparation",
            "trade traces and wider road stage the first town-preparation beat without a quest board",
            town.center + Vec3::new(-7.0, 0.0, 5.5),
            town.center + Vec3::new(1.0, 1.1, 0.2),
            world_map,
            Vec3::new(-8.0, 4.5, 8.0),
            0.33,
            WeatherKind::Clear,
            Some(OmenKind::DawnLight),
            PresentationJourneyStep::TownPreparation,
        )
        .with_visual_sample(
            PresentationVisualSample::TownPreparation,
            CameraMode::ThirdPerson,
            CompositionGuide {
                foreground: "cart ruts and footpath widening away from the outpost",
                midground: "trade stall and traveler-scale preparation space",
                background: "ridge road implying a larger town beyond sight",
                quality_gate: "preparation reads through world objects, not explicit objective UI",
            },
        )
        .with_camera_drift(Vec3::new(1.4, 0.0, -1.2)),
        build_journey_scene(
            "First Loss",
            "broken crates and scattered tracks form the first-loss foreshadowing point",
            loss.center + Vec3::new(-5.5, 0.0, 4.0),
            loss.center + Vec3::new(0.8, 1.0, -0.4),
            world_map,
            Vec3::new(-7.2, 4.0, 6.8),
            0.42,
            WeatherKind::Storm,
            Some(OmenKind::SummitCall),
            PresentationJourneyStep::FirstLoss,
        )
        .with_visual_sample(
            PresentationVisualSample::FirstLoss,
            CameraMode::FirstPerson,
            CompositionGuide {
                foreground: "empty crate, broken rope and hard-edged road dust",
                midground: "crossroad marker with no explicit warning sign",
                background: "road continuing despite the loss beat",
                quality_gate: "loss feels like an event seed in the world, not a punishment screen",
            },
        )
        .with_camera_drift(Vec3::new(0.8, 0.0, -1.0)),
        build_journey_scene(
            "Desert Departure",
            "grass gives way to dry ground as the route begins to face the desert",
            desert.center + Vec3::new(-8.0, 0.0, 6.5),
            desert.center + Vec3::new(1.2, 1.0, -2.0),
            world_map,
            Vec3::new(-9.5, 4.8, 8.0),
            0.58,
            WeatherKind::Sandstorm,
            Some(OmenKind::DawnLight),
            PresentationJourneyStep::DesertDeparture,
        )
        .with_visual_sample(
            PresentationVisualSample::DesertDeparture,
            CameraMode::ThirdPerson,
            CompositionGuide {
                foreground: "traveler on the last rough path before sand",
                midground: "small relic and thinning grass line",
                background: "dry haze and desert-facing ridge",
                quality_gate: "desert departure reads as a gradual biome threshold",
            },
        )
        .with_camera_drift(Vec3::new(2.2, 0.0, -1.8)),
    ]
}

fn build_director_scenes(
    regions: &RegionGraphState,
    world_map: &WorldMap,
) -> Vec<PresentationScene> {
    let Some(gate) = regions.gates.first() else {
        return Vec::new();
    };
    vec![
        build_journey_scene(
            "Director Interface",
            "deterministic director validates non-task suggestions near a world anchor",
            gate.position + Vec3::new(6.0, 0.0, gate.radius * 0.32),
            gate.position + Vec3::Y * 1.2,
            world_map,
            Vec3::new(-8.0, 5.2, 9.0),
            0.34,
            WeatherKind::Mist,
            Some(OmenKind::SummitCall),
            PresentationJourneyStep::Director,
        )
        .with_camera_drift(Vec3::new(1.6, 0.0, -1.8)),
    ]
}

fn build_landmark_scenes(
    landmarks: &LandmarkState,
    world_map: &WorldMap,
) -> Vec<PresentationScene> {
    let mut scenes = Vec::new();
    if let Some(pyramid) = landmarks.desert_pyramid() {
        scenes.push(build_journey_scene(
            "Sandstorm Pyramid Baseline",
            "sandstorm gap reveals a huge pyramid silhouette at world scale",
            pyramid.position + Vec3::new(-pyramid.scale * 2.7, 0.0, pyramid.scale * 1.55),
            pyramid.position + Vec3::Y * (pyramid.scale * 0.38),
            world_map,
            Vec3::new(
                -pyramid.scale * 1.82,
                pyramid.scale * 0.76,
                pyramid.scale * 1.34,
            ),
            0.18,
            WeatherKind::Sandstorm,
            Some(OmenKind::DawnLight),
            PresentationJourneyStep::DesertPyramid,
        )
        .with_visual_sample(
            PresentationVisualSample::SandstormPyramid,
            CameraMode::FirstPerson,
            CompositionGuide {
                foreground: "sand haze and low dune contour",
                midground: "traveler scale against open desert",
                background: "giant pyramid silhouette visible through storm gaps",
                quality_gate: "landmark remains identifiable under sandstorm and is not hidden by UI or foreground clutter",
            },
        )
        .with_wander_start(
            pyramid.position + Vec3::new(-pyramid.scale * 3.08, 0.0, pyramid.scale * 1.9),
            world_map,
        )
        .with_camera_drift(Vec3::new(pyramid.scale * 0.18, -1.0, -pyramid.scale * 0.12)));
        scenes.push(build_journey_scene(
            "Oasis Ruins Baseline",
            "near pyramid oasis, ruin walls and relics create a readable exploration pocket",
            pyramid.position + Vec3::new(-pyramid.scale * 0.92, 0.0, pyramid.scale * 0.52),
            pyramid.position + Vec3::new(-pyramid.scale * 0.42, pyramid.scale * 0.12, pyramid.scale * 0.18),
            world_map,
            Vec3::new(-pyramid.scale * 0.42, pyramid.scale * 0.18, pyramid.scale * 0.36),
            0.31,
            WeatherKind::Sandstorm,
            Some(OmenKind::StillWater),
            PresentationJourneyStep::DesertPyramid,
        )
        .with_visual_sample(
            PresentationVisualSample::OasisRuins,
            CameraMode::ThirdPerson,
            CompositionGuide {
                foreground: "traveler, oasis edge and sand-dark water",
                midground: "ruin walls, relic block and wind-cut stones",
                background: "pyramid mass beyond the local discovery space",
                quality_gate: "near detail and landmark scale coexist without entity intersections dominating the shot",
            },
        )
        .with_wander_start(
            pyramid.position + Vec3::new(-pyramid.scale * 1.18, 0.0, pyramid.scale * 0.72),
            world_map,
        )
        .with_camera_drift(Vec3::new(pyramid.scale * 0.08, 0.4, -pyramid.scale * 0.08)));
        scenes.push(
            build_journey_scene(
                "Third Person Travel",
                "third-person travel silhouette against a large landmark",
                pyramid.position + Vec3::new(-pyramid.scale * 1.1, 0.0, pyramid.scale * 0.7),
                pyramid.position + Vec3::Y * (pyramid.scale * 0.28),
                world_map,
                Vec3::new(-16.0, 7.0, 18.0),
                0.26,
                WeatherKind::Sandstorm,
                Some(OmenKind::DawnLight),
                PresentationJourneyStep::ThirdPerson,
            )
            .with_camera_drift(Vec3::new(2.4, 0.1, -2.8)),
        );
    }
    scenes
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
        visual_sample: PresentationVisualSample::Functional,
        composition: CompositionGuide::functional(),
        focus: Vec3::new(focus.x, focus_height, focus.z),
        camera_offset,
        camera_drift: Vec3::ZERO,
        camera_mode: CameraMode::FirstPerson,
        wander_start: Vec3::new(wander_target.x, wander_height, wander_target.z),
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
        visual_sample: PresentationVisualSample::Functional,
        composition: CompositionGuide::functional(),
        focus: Vec3::new(focus.x, focus_height, focus.z),
        camera_offset,
        camera_drift: Vec3::ZERO,
        camera_mode: CameraMode::FirstPerson,
        wander_start: Vec3::new(wander_target.x, wander_height, wander_target.z),
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
        BiomeKind::DesertSand => (1.0 - anchor.tile.moisture()) + anchor.tile.height() * 0.03,
        BiomeKind::Gobi => anchor.tile.erosion() + (1.0 - anchor.tile.moisture()) * 0.4,
        BiomeKind::Oasis => anchor.tile.moisture() - anchor.tile.slope() * 0.2,
    }
}

fn scene_index_at_elapsed(elapsed: f32, scene_duration: f32, scene_count: usize) -> usize {
    (((elapsed / scene_duration).floor() as usize) % scene_count.max(1))
        .min(scene_count.saturating_sub(1))
}

fn scene_progress(elapsed: f32, scene_duration: f32) -> f32 {
    (elapsed / scene_duration).fract()
}

fn smoothstep_unit(value: f32) -> f32 {
    let t = value.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
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
    transform.translation = scene.wander_start;
    transform.look_at(scene.wander_target + Vec3::new(0.0, 0.0, 0.2), Vec3::Y);
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
            | PresentationJourneyStep::PostDreamDeparture
            | PresentationJourneyStep::Boundary
            | PresentationJourneyStep::BoundaryCrossing
            | PresentationJourneyStep::FarBankArrival
            | PresentationJourneyStep::TownPreparation
            | PresentationJourneyStep::FirstLoss
            | PresentationJourneyStep::DesertDeparture
            | PresentationJourneyStep::DesertPyramid
            | PresentationJourneyStep::Ecology
            | PresentationJourneyStep::ThirdPerson
            | PresentationJourneyStep::Director
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
            PresentationJourneyStep::PostDreamDeparture => {
                journey.story_stage = StoryArcStage::DreamAfterglow;
                journey.dream.phase = DreamPhase::Afterglow;
                journey.dream.seen_pyramid = true;
                journey.dream.echo_strength = 0.72;
                signs.omen_triggered = true;
                signs.current_omen = Some(OmenKind::SummitCall);
                signs.omen_intensity = signs.omen_intensity.max(0.7);
            }
            PresentationJourneyStep::Boundary
            | PresentationJourneyStep::BoundaryCrossing
            | PresentationJourneyStep::DesertPyramid
            | PresentationJourneyStep::Ecology
            | PresentationJourneyStep::ThirdPerson
            | PresentationJourneyStep::Director => {
                journey.story_stage = StoryArcStage::DreamAfterglow;
                journey.dream.phase = DreamPhase::Afterglow;
                journey.dream.seen_pyramid = true;
                journey.dream.echo_strength = 0.9;
                signs.omen_triggered = true;
                signs.current_omen = scene.expected_omen.or(Some(OmenKind::DawnLight));
                signs.omen_intensity = signs.omen_intensity.max(0.82);
            }
            PresentationJourneyStep::FarBankArrival => {
                journey.story_stage = StoryArcStage::FarBankOutpost;
                journey.dream.phase = DreamPhase::Afterglow;
                journey.dream.seen_pyramid = true;
                journey.dream.echo_strength = 0.62;
                signs.omen_triggered = true;
                signs.current_omen = Some(OmenKind::DawnLight);
                signs.omen_intensity = signs.omen_intensity.max(0.64);
            }
            PresentationJourneyStep::TownPreparation => {
                journey.story_stage = StoryArcStage::TownPreparation;
                journey.dream.phase = DreamPhase::Afterglow;
                journey.dream.seen_pyramid = true;
                journey.dream.echo_strength = 0.56;
                signs.omen_triggered = true;
                signs.current_omen = Some(OmenKind::StillWater);
                signs.omen_intensity = signs.omen_intensity.max(0.56);
            }
            PresentationJourneyStep::FirstLoss => {
                journey.story_stage = StoryArcStage::FirstLoss;
                journey.dream.phase = DreamPhase::Afterglow;
                journey.dream.seen_pyramid = true;
                journey.dream.echo_strength = 0.5;
                signs.omen_triggered = true;
                signs.current_omen = Some(OmenKind::SummitCall);
                signs.omen_intensity = signs.omen_intensity.max(0.6);
            }
            PresentationJourneyStep::DesertDeparture => {
                journey.story_stage = StoryArcStage::DesertDeparture;
                journey.dream.phase = DreamPhase::Afterglow;
                journey.dream.seen_pyramid = true;
                journey.dream.echo_strength = 0.7;
                signs.omen_triggered = true;
                signs.current_omen = Some(OmenKind::DawnLight);
                signs.omen_intensity = signs.omen_intensity.max(0.7);
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
        | PresentationJourneyStep::DreamEcho
        | PresentationJourneyStep::PostDreamDeparture
        | PresentationJourneyStep::Boundary
        | PresentationJourneyStep::BoundaryCrossing
        | PresentationJourneyStep::FarBankArrival
        | PresentationJourneyStep::TownPreparation
        | PresentationJourneyStep::FirstLoss
        | PresentationJourneyStep::DesertDeparture
        | PresentationJourneyStep::DesertPyramid
        | PresentationJourneyStep::Ecology
        | PresentationJourneyStep::ThirdPerson
        | PresentationJourneyStep::Director => {}
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

fn drive_region_showcase(
    regions: &mut RegionGraphState,
    scene: &PresentationScene,
    scene_progress: f32,
) {
    if !matches!(
        scene.journey_step,
        PresentationJourneyStep::Boundary | PresentationJourneyStep::BoundaryCrossing
    ) {
        return;
    }
    let Some(gate) = regions.gates.iter_mut().min_by(|left, right| {
        planar_distance(left.position, scene.focus)
            .total_cmp(&planar_distance(right.position, scene.focus))
    }) else {
        return;
    };
    gate.state = if scene_progress > 0.6 {
        TransitionGateState::Open
    } else {
        TransitionGateState::Hinted
    };
    regions.current_region = gate.from;
    regions.nearest_gate = Some(crate::game::regions::GateProximity {
        gate_id: gate.id,
        distance: planar_distance(scene.wander_target, gate.position),
        open: gate.state == TransitionGateState::Open,
    });
}

fn drive_landmark_showcase(
    landmarks: &mut LandmarkState,
    scene: &PresentationScene,
    scene_progress: f32,
) {
    if scene.journey_step != PresentationJourneyStep::DesertPyramid
        && scene.journey_step != PresentationJourneyStep::ThirdPerson
    {
        return;
    }
    landmarks.pyramid_signal = PyramidSignal {
        visible: true,
        distance: Some(planar_distance(scene.wander_target, scene.focus)),
        sandstorm_strength: (0.84 - scene_progress * 0.42).clamp(0.2, 0.9),
        silhouette_strength: (0.48 + scene_progress * 0.45).clamp(0.0, 1.0),
    };
}

fn drive_camera_mode_showcase(state: &mut FirstPersonState, scene: &PresentationScene) {
    state.camera_mode = scene.camera_mode;
    if scene.camera_mode == CameraMode::ThirdPerson {
        state.third_person_distance = state.third_person_distance.max(5.8);
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
        herding_completed: false,
        afterglow_cue: None,
        boundary_crossed: false,
        outpost_reached: false,
        town_edge_reached: false,
        loss_crossroad_reached: false,
        desert_road_reached: false,
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
            AppConfig, AssetConfig, CameraConfig, DesertConfig, EcologyConfig, EnvironmentConfig,
            PlayerConfig, PresentationConfig, QualityConfig, SignConfig, WorldConfig,
        },
        game::{
            places::{MeaningfulPlace, MeaningfulPlaces, PlaceKind, PlaceTag},
            player::CameraMode,
            presentation::{
                CompositionGuide, PresentationJourneyStep, PresentationScene,
                PresentationVisualSample, build_journey_scenes, build_region_scenes,
                omen_for_place, scene_index_at_elapsed, scene_progress,
                screenshot_manifest_for_scene, sort_presentation_scenes_for_testing,
                visual_baseline_count,
            },
            regions::{
                GateProximity, RegionBiomeBias, RegionBoundaryKind, RegionGraphState, RegionId,
                RegionJourneyMilestones, RegionKind, RegionMilestoneKind, RegionMilestoneState,
                RegionOutpostState, RegionProfile, RegionWeatherBias, TransitionCondition,
                TransitionGate, TransitionGateKind, TransitionGateState, WorldRegion,
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

    #[test]
    fn visual_baseline_matrix_defines_six_exploration_samples() {
        let labels: Vec<&'static str> = PresentationVisualSample::BASELINE
            .iter()
            .map(|sample| sample.label())
            .collect();

        assert_eq!(
            labels,
            vec![
                "village-dawn",
                "post-dream-departure",
                "dream-after-sea-wind",
                "boundary-crossing",
                "far-bank-arrival",
                "mist-boundary",
            ]
        );
        assert!(
            PresentationVisualSample::BASELINE
                .iter()
                .all(|sample| sample.is_visual_baseline())
        );
    }

    #[test]
    fn visual_baseline_scenes_sort_before_functional_showcases() {
        let mut scenes = vec![
            test_scene(
                "functional",
                PresentationVisualSample::Functional,
                CameraMode::FirstPerson,
            ),
            test_scene(
                "departure",
                PresentationVisualSample::PostDreamDeparture,
                CameraMode::ThirdPerson,
            ),
            test_scene(
                "village",
                PresentationVisualSample::VillageDawn,
                CameraMode::ThirdPerson,
            ),
            test_scene(
                "far bank",
                PresentationVisualSample::FarBankArrival,
                CameraMode::FirstPerson,
            ),
        ];

        sort_presentation_scenes_for_testing(&mut scenes);

        assert_eq!(
            scenes
                .iter()
                .map(|scene| scene.visual_sample)
                .collect::<Vec<_>>(),
            vec![
                PresentationVisualSample::VillageDawn,
                PresentationVisualSample::PostDreamDeparture,
                PresentationVisualSample::FarBankArrival,
                PresentationVisualSample::Functional,
            ]
        );
        assert!(
            scenes
                .iter()
                .any(|scene| scene.camera_mode == CameraMode::ThirdPerson)
        );
        assert!(
            scenes
                .iter()
                .any(|scene| scene.camera_mode == CameraMode::FirstPerson)
        );
    }

    #[test]
    fn visual_baseline_count_tracks_unique_samples() {
        let scenes = vec![
            test_scene(
                "village",
                PresentationVisualSample::VillageDawn,
                CameraMode::ThirdPerson,
            ),
            test_scene(
                "another village",
                PresentationVisualSample::VillageDawn,
                CameraMode::FirstPerson,
            ),
            test_scene(
                "mist",
                PresentationVisualSample::MistBoundary,
                CameraMode::FirstPerson,
            ),
            test_scene(
                "functional",
                PresentationVisualSample::Functional,
                CameraMode::FirstPerson,
            ),
        ];

        assert_eq!(visual_baseline_count(&scenes), 2);
    }

    #[test]
    fn region_presentation_adds_departure_crossing_and_far_bank_samples() {
        let config = test_config();
        let world_map = WorldMap::new_for_testing(42, &config);
        let graph = test_region_graph();

        let scenes = build_region_scenes(&graph, &world_map);
        let samples = scenes
            .iter()
            .map(|scene| scene.visual_sample)
            .collect::<Vec<_>>();

        assert!(samples.contains(&PresentationVisualSample::BoundaryCrossing));
        assert!(samples.contains(&PresentationVisualSample::FarBankArrival));
        assert!(
            scenes
                .iter()
                .any(|scene| scene.journey_step == PresentationJourneyStep::FarBankArrival)
        );
    }

    #[test]
    fn screenshot_manifest_records_directory_metadata_and_checklist() {
        let scene = test_scene(
            "village",
            PresentationVisualSample::VillageDawn,
            CameraMode::ThirdPerson,
        );
        let manifest = screenshot_manifest_for_scene(&scene, "2026-05-12", "abc1234");

        assert!(manifest.directory_rule.contains("captures/2026-05-12"));
        assert!(manifest.directory_rule.contains("village-dawn"));
        assert!(manifest.required_metadata.contains(&"git_commit"));
        assert!(
            manifest
                .checklist
                .iter()
                .any(|item| item.key == "entity_intersection")
        );
    }

    fn test_scene(
        name: &'static str,
        visual_sample: PresentationVisualSample,
        camera_mode: CameraMode,
    ) -> PresentationScene {
        PresentationScene {
            name,
            description: "test scene",
            visual_sample,
            composition: CompositionGuide::functional(),
            focus: Vec3::ZERO,
            camera_offset: Vec3::new(-1.0, 1.0, 1.0),
            camera_drift: Vec3::ZERO,
            camera_mode,
            wander_start: Vec3::ZERO,
            wander_target: Vec3::ZERO,
            time_override: 0.0,
            weather: crate::game::environment::WeatherKind::Clear,
            expected_omen: None,
            journey_step: PresentationJourneyStep::Scenic,
        }
    }

    fn test_region_graph() -> RegionGraphState {
        let village = RegionId(1);
        let boundary = RegionId(2);
        RegionGraphState {
            regions: vec![
                test_region(village, RegionKind::VillageCoast, Vec3::ZERO),
                test_region(
                    boundary,
                    RegionKind::MountainBoundary,
                    Vec3::new(0.0, 0.0, -90.0),
                ),
            ],
            gates: vec![TransitionGate {
                id: 10,
                from: village,
                to: boundary,
                kind: TransitionGateKind::MistRiverFord,
                position: Vec3::new(0.0, 0.0, -42.0),
                radius: 22.0,
                condition: TransitionCondition::DreamAfterglowAndIntent,
                state: TransitionGateState::Hinted,
                hint: "雾里有旧水声。",
            }],
            current_region: village,
            nearest_gate: Some(GateProximity {
                gate_id: 10,
                distance: 12.0,
                open: false,
            }),
            discovered_gates: Vec::new(),
            crossing: None,
            outpost: Some(RegionOutpostState {
                region: boundary,
                center: Vec3::new(18.0, 0.0, -98.0),
                arrival_radius: 20.0,
                discovered: true,
                recorded: false,
            }),
            milestones: RegionJourneyMilestones {
                town_edge: test_milestone(
                    RegionMilestoneKind::TownEdge,
                    boundary,
                    Vec3::new(48.0, 0.0, -116.0),
                ),
                loss_crossroad: test_milestone(
                    RegionMilestoneKind::LossCrossroad,
                    boundary,
                    Vec3::new(92.0, 0.0, -156.0),
                ),
                desert_road: test_milestone(
                    RegionMilestoneKind::DesertRoad,
                    boundary,
                    Vec3::new(138.0, 0.0, -208.0),
                ),
            },
        }
    }

    fn test_milestone(
        kind: RegionMilestoneKind,
        region: RegionId,
        center: Vec3,
    ) -> RegionMilestoneState {
        RegionMilestoneState {
            kind,
            region,
            center,
            arrival_radius: 20.0,
            discovered: false,
            recorded: false,
        }
    }

    fn test_region(id: RegionId, kind: RegionKind, center: Vec3) -> WorldRegion {
        WorldRegion {
            id,
            kind,
            seed: id.0,
            center,
            radius: 120.0,
            landmark: None,
            boundary: RegionBoundaryKind::MistRiver,
            profile: RegionProfile {
                biome_bias: RegionBiomeBias::CoastalMeadow,
                weather_bias: RegionWeatherBias::ClearSeaMist,
                danger: 0.1,
                exploration_value: 0.4,
            },
        }
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
                detail_density: 1.0,
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
                sheep_count: 9,
                state_update_interval_seconds: 0.2,
                visual_update_interval_seconds: 0.066,
                max_visible_bird_distance: 240.0,
                max_visible_fish_distance: 90.0,
                max_visible_sheep_distance: 120.0,
            },
            assets: AssetConfig {
                color_saturation: 1.0,
                warm_light_intensity: 1.0,
                water_alpha: 0.64,
                shadow_alpha: 0.58,
                foundation_proxy_mode: true,
                animate_placeholder_characters: false,
                animate_placeholder_ambience: false,
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
