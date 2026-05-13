use std::time::Instant;

use bevy::{
    color::LinearRgba,
    pbr::{MeshMaterial3d, StandardMaterial},
    prelude::*,
};

use crate::{
    core::performance::{FramePerformance, PerformancePhase},
    game::{
        assets::{
            AssetPresentationAnchor, ProceduralAnimationRole, ProceduralAssetKind,
            ProceduralAssetLod, ProceduralAssetMaterials, ProceduralSpawnRequest,
            spawn_procedural_asset, spawn_procedural_asset_entity,
        },
        ecology::{AnimalBehavior, AnimalFlockState, AnimalKind, EcologyState},
        environment::{EnvironmentSnapshot, WeatherKind, WindField},
        flow::{AppScreen, InGameState},
        intent::{IntentState, apply_village_dialogue_intent},
        journey::{DreamPhase, JourneyState},
        notebook::{
            NotebookEntryKind, NotebookRecord, NotebookSource, NotebookState, NotebookTag,
            record_notebook_entry,
        },
        places::planar_distance,
        world::{WandererPrototype, WorldCamera, WorldMap, WorldShowcaseSpots},
    },
};

pub struct VillagePlugin;

type VillageInitQueries<'w, 's> = (
    Query<'w, 's, &'static mut Transform, With<WandererPrototype>>,
    Query<'w, 's, &'static mut Transform, (With<WorldCamera>, Without<WandererPrototype>)>,
);

type VillageInitAssets<'w> = (ResMut<'w, Assets<Mesh>>, Res<'w, ProceduralAssetMaterials>);
type VillageVisualMaterialQuery<'w, 's> = Query<
    'w,
    's,
    (
        Entity,
        &'static MeshMaterial3d<StandardMaterial>,
        &'static VillageMaterialOverride,
    ),
    (
        Without<WandererPrototype>,
        With<VillageMaterialOverridePending>,
    ),
>;
type VillageVisualRuntimeMaterialQuery<'w, 's> = Query<
    'w,
    's,
    (
        &'static VillageVisualPartKind,
        &'static MeshMaterial3d<StandardMaterial>,
        &'static VillageMaterialOverride,
    ),
    Without<WandererPrototype>,
>;

type VillageInteractionResources<'w> = (
    Res<'w, Time>,
    Res<'w, ButtonInput<KeyCode>>,
    Option<ResMut<'w, VillageState>>,
    Option<ResMut<'w, EcologyState>>,
    Option<ResMut<'w, IntentState>>,
    Option<ResMut<'w, NotebookState>>,
);

impl Plugin for VillagePlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(VillageAtmosphere::default());
        app.add_systems(
            Update,
            (
                initialize_village_session,
                update_village_atmosphere,
                update_herding_state,
                update_village_actor_behavior,
                sync_village_collider_centers,
                animate_village_asset_parts,
                ensure_village_material_overrides,
                update_village_visual_materials,
                update_village_interaction,
            )
                .chain()
                .run_if(in_state(InGameState::Running)),
        );
        app.add_systems(OnExit(AppScreen::InGame), cleanup_village_session);
    }
}

#[derive(Debug, Resource, Clone, PartialEq)]
pub struct VillageState {
    pub origin: Vec3,
    pub spawn_point: Vec3,
    pub areas: Vec<VillageArea>,
    pub houses: Vec<VillageHouseState>,
    pub actors: Vec<VillageActorState>,
    pub nearest_actor: Option<VillageActorSnapshot>,
    pub nearest_house: Option<VillageHouseSnapshot>,
    pub interaction_prompt: Option<String>,
    pub player_was_bootstrapped: bool,
    pub herding: HerdingState,
}

impl VillageState {
    pub fn area(&self, kind: VillageAreaKind) -> Option<&VillageArea> {
        self.areas.iter().find(|area| area.kind == kind)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct VillageArea {
    pub kind: VillageAreaKind,
    pub position: Vec3,
    pub radius: f32,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum VillageAreaKind {
    Houses,
    SheepPen,
    Well,
    Market,
    Shore,
    OuterPath,
}

impl VillageAreaKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Houses => "屋舍",
            Self::SheepPen => "羊圈",
            Self::Well => "水井",
            Self::Market => "集市",
            Self::Shore => "海边",
            Self::OuterPath => "村外小路",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct VillageHouseState {
    pub id: u64,
    pub position: Vec3,
    pub yaw: f32,
    pub half_extents: Vec2,
    pub door_position: Vec3,
    pub interior_position: Vec3,
    pub occupied_by_player: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VillageHouseSnapshot {
    pub id: u64,
    pub distance: f32,
    pub inside: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VillageActorState {
    pub id: u64,
    pub kind: VillageActorKind,
    pub home: Vec3,
    pub radius: f32,
    pub behavior: VillageBehavior,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HerdingState {
    pub phase: HerdingPhase,
    pub task_available: bool,
    pub flock_following_player: bool,
    pub known_grass_patch: Vec3,
    pub active_grass_patch: Vec3,
    pub return_pen_target: Vec3,
    pub flock_center: Vec3,
    pub player_has_seen_flock: bool,
    pub grass_patch_reached: bool,
    pub pen_returned: bool,
    pub first_task_completed: bool,
    pub phase_started_at: f32,
}

impl Default for HerdingState {
    fn default() -> Self {
        Self {
            phase: HerdingPhase::NotStarted,
            task_available: false,
            flock_following_player: false,
            known_grass_patch: Vec3::ZERO,
            active_grass_patch: Vec3::ZERO,
            return_pen_target: Vec3::ZERO,
            flock_center: Vec3::ZERO,
            player_has_seen_flock: false,
            grass_patch_reached: false,
            pen_returned: false,
            first_task_completed: false,
            phase_started_at: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum HerdingPhase {
    NotStarted,
    Prompted,
    FollowingToGrass,
    GrazingAtPatch,
    ReturningToPen,
    Completed,
}

impl HerdingPhase {
    pub fn label(self) -> &'static str {
        match self {
            Self::NotStarted => "未开始",
            Self::Prompted => "准备放羊",
            Self::FollowingToGrass => "带羊去草地",
            Self::GrazingAtPatch => "羊群吃草",
            Self::ReturningToPen => "带羊回圈",
            Self::Completed => "第一次放羊完成",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum VillageActorKind {
    Sheep,
    Shepherd,
    Merchant,
}

impl VillageActorKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Sheep => "羊",
            Self::Shepherd => "牧羊人",
            Self::Merchant => "商人",
        }
    }

    fn prompt(self) -> &'static str {
        match self {
            Self::Sheep => "可安抚",
            Self::Shepherd | Self::Merchant => "可交谈",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum VillageBehavior {
    Grazing,
    Tending,
    Trading,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VillageActorSnapshot {
    pub id: u64,
    pub kind: VillageActorKind,
    pub distance: f32,
}

#[derive(Debug, Component, Clone, Copy, PartialEq)]
struct VillageActor {
    id: u64,
    kind: VillageActorKind,
    home: Vec3,
    radius: f32,
    seed: u32,
}

#[derive(Debug, Component, Clone, Copy, PartialEq)]
struct VillageAnimatedPart {
    actor_id: Option<u64>,
    actor_kind: Option<VillageActorKind>,
    role: ProceduralAnimationRole,
    base_translation: Vec3,
    base_rotation: Quat,
    base_scale: Vec3,
}

#[derive(Debug, Component)]
struct VillageVisual;

#[derive(Debug, Component, Clone, Copy, Eq, PartialEq, Hash)]
enum VillageVisualPartKind {
    WarmWindow,
    WarmLantern,
    SmokeWisp,
    WetGround,
    ShoreWater,
    ShoreFoam,
    PathStone,
}

#[derive(Debug, Component, Clone)]
struct VillageMaterialOverride {
    original: Handle<StandardMaterial>,
}

#[derive(Debug, Component)]
struct VillageMaterialOverridePending;

#[derive(Debug, Component, Clone, Copy, PartialEq)]
pub struct VillageCollider {
    pub kind: VillageColliderKind,
    pub center: Vec2,
    pub half_extents: Vec2,
    pub yaw: f32,
    pub radius: f32,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum VillageColliderKind {
    House,
    InteriorProp,
    Well,
    SheepPenRail,
    MarketStall,
    Actor,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VillageLayoutConfig {
    pub house_count: usize,
    pub sheep_count: usize,
    pub radius: f32,
}

#[derive(Debug, Resource, Clone, Copy, PartialEq)]
pub struct VillageAtmosphere {
    pub day_phase: VillageDayPhase,
    pub wind_strength: f32,
    pub wind_dir: Vec2,
    pub canopy_sway: f32,
    pub smoke_rise: f32,
    pub sea_mist: f32,
    pub shoreline_wash: f32,
    pub unease: f32,
    pub life_density: f32,
    pub warm_window_glow: f32,
    pub ground_dampness: f32,
    pub shoreline_foam: f32,
    pub departure_pull: f32,
}

impl Default for VillageAtmosphere {
    fn default() -> Self {
        Self {
            day_phase: VillageDayPhase::Dawn,
            wind_strength: 0.0,
            wind_dir: Vec2::ZERO,
            canopy_sway: 0.0,
            smoke_rise: 0.0,
            sea_mist: 0.0,
            shoreline_wash: 0.0,
            unease: 0.0,
            life_density: 0.45,
            warm_window_glow: 0.52,
            ground_dampness: 0.22,
            shoreline_foam: 0.2,
            departure_pull: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum VillageDayPhase {
    Dawn,
    Day,
    Dusk,
    Night,
}

impl VillageDayPhase {
    fn label(self) -> &'static str {
        match self {
            Self::Dawn => "dawn",
            Self::Day => "day",
            Self::Dusk => "dusk",
            Self::Night => "night",
        }
    }
}

impl Default for VillageLayoutConfig {
    fn default() -> Self {
        Self {
            house_count: 5,
            sheep_count: 9,
            radius: 38.0,
        }
    }
}

const PLAYER_SPAWN_OFFSET: Vec3 = Vec3::new(-7.0, 0.0, 7.0);
const INTERACTION_RADIUS: f32 = 4.2;
const HERDING_GRAZE_RADIUS: f32 = 9.5;
const HERDING_RETURN_RADIUS: f32 = 11.0;
const HERDING_NEAR_FLOCK_RADIUS: f32 = 15.0;

fn initialize_village_session(
    mut commands: Commands,
    world_map: Option<Res<WorldMap>>,
    spots: Option<Res<WorldShowcaseSpots>>,
    village: Option<ResMut<VillageState>>,
    config: Res<crate::core::config::AppConfig>,
    mut queries: VillageInitQueries<'_, '_>,
    mut assets: VillageInitAssets<'_>,
) {
    let Some(world_map) = world_map else {
        return;
    };
    let Some(spots) = spots else {
        return;
    };

    if let Some(mut village) = village {
        if !village.player_was_bootstrapped {
            bootstrap_player_to_village(&world_map, &mut village, &mut queries.0, &mut queries.1);
        }
        return;
    }

    let origin = choose_village_origin(&world_map, spots.meadow.position);
    let layout = build_layout_internal(&world_map, origin, VillageLayoutConfig::default());
    spawn_village_visuals(
        &mut commands,
        &mut assets.0,
        &assets.1,
        &config.assets,
        &layout,
        &world_map,
    );
    spawn_village_actors(
        &mut commands,
        &mut assets.0,
        &assets.1,
        &config.assets,
        &layout,
        &world_map,
    );

    tracing::info!(
        target: "dao_game::village::generation",
        origin_x = layout.origin.x,
        origin_z = layout.origin.z,
        actor_count = layout.actors.len(),
        area_count = layout.areas.len(),
        "opening village generated"
    );

    let herding = initialize_herding_state(&layout);
    let mut state = VillageState {
        origin: layout.origin,
        spawn_point: layout.spawn_point,
        areas: layout.areas,
        houses: layout.houses,
        actors: layout.actors,
        nearest_actor: None,
        nearest_house: None,
        interaction_prompt: None,
        player_was_bootstrapped: false,
        herding,
    };
    bootstrap_player_to_village(&world_map, &mut state, &mut queries.0, &mut queries.1);
    commands.insert_resource(state);
}

fn cleanup_village_session(mut commands: Commands) {
    commands.remove_resource::<VillageState>();
    commands.insert_resource(VillageAtmosphere::default());
}

fn update_village_atmosphere(
    environment: Res<EnvironmentSnapshot>,
    wind_field: Res<WindField>,
    journey: Option<Res<JourneyState>>,
    mut atmosphere: ResMut<VillageAtmosphere>,
    performance: Option<ResMut<FramePerformance>>,
) {
    let started_at = Instant::now();
    let dream_afterglow = journey
        .as_deref()
        .filter(|journey| journey.dream.phase == DreamPhase::Afterglow)
        .map(|journey| journey.dream.echo_strength)
        .unwrap_or(0.0);
    let response = journey
        .as_deref()
        .map(|journey| journey.response.intensity)
        .unwrap_or(0.0);
    let stage_departure_bias = journey
        .as_deref()
        .map(|journey| match journey.story_stage {
            crate::game::journey::StoryArcStage::VillageAwakening => 0.0,
            crate::game::journey::StoryArcStage::VillageLife => 0.08,
            crate::game::journey::StoryArcStage::DreamApproaching => 0.16,
            crate::game::journey::StoryArcStage::Dreaming => 0.24,
            crate::game::journey::StoryArcStage::DreamAfterglow => 0.38,
            crate::game::journey::StoryArcStage::BoundaryCrossing => 0.7,
            crate::game::journey::StoryArcStage::FarBankOutpost => 0.84,
            crate::game::journey::StoryArcStage::TownPreparation => 0.58,
            crate::game::journey::StoryArcStage::FirstLoss => 0.64,
            crate::game::journey::StoryArcStage::DesertDeparture => 0.9,
        })
        .unwrap_or(0.0);
    let day_phase = village_day_phase(environment.daylight);
    let weather_life_bias = match environment.weather {
        WeatherKind::Storm => -0.26,
        WeatherKind::Sandstorm => -0.34,
        WeatherKind::Snow => -0.18,
        WeatherKind::Rain => -0.14,
        WeatherKind::Mist => -0.06,
        WeatherKind::Clear => 0.06,
    };

    let next = VillageAtmosphere {
        day_phase,
        wind_strength: wind_field.speed,
        wind_dir: wind_field.direction,
        canopy_sway: (wind_field.gust * 0.75 + environment.sea_mist * 0.12).clamp(0.0, 1.0),
        smoke_rise: (0.28 + environment.ambient_energy * 0.18 - wind_field.speed * 0.16)
            .clamp(0.08, 0.68),
        sea_mist: environment.sea_mist,
        shoreline_wash: (environment.humidity * 0.42
            + wind_field.gust * 0.32
            + environment.storm_weight * 0.24)
            .clamp(0.0, 1.0),
        unease: (dream_afterglow * 0.58 + response * 0.26 + wind_field.omen_bias * 0.18)
            .clamp(0.0, 1.0),
        life_density: (0.56 + weather_life_bias + environment.daylight * 0.12
            - dream_afterglow * 0.18
            - environment.storm_weight * 0.12)
            .clamp(0.18, 0.9),
        warm_window_glow: (0.18
            + (1.0 - environment.daylight).clamp(0.0, 1.0) * 0.56
            + environment.dawn_warmth * 0.32
            + dream_afterglow * 0.24
            + environment.boundary_glow * 0.18)
            .clamp(0.12, 1.0),
        ground_dampness: (environment.ground_wetness * 0.7
            + environment.sea_mist * 0.16
            + environment.storm_weight * 0.16)
            .clamp(0.0, 1.0),
        shoreline_foam: (environment.ground_wetness * 0.2
            + environment.humidity * 0.22
            + environment.sea_mist * 0.2
            + environment.storm_weight * 0.2
            + wind_field.gust * 0.2)
            .clamp(0.0, 1.0),
        departure_pull: (stage_departure_bias * 0.58
            + dream_afterglow * 0.22
            + response * 0.12
            + environment.horizon_tension * 0.12)
            .clamp(0.0, 1.0),
    };
    let changed = atmosphere.day_phase != next.day_phase
        || (atmosphere.unease - next.unease).abs() > 0.12
        || (atmosphere.wind_strength - next.wind_strength).abs() > 0.12
        || (atmosphere.sea_mist - next.sea_mist).abs() > 0.12;
    if changed {
        tracing::info!(
            target: "dao_game::village::atmosphere",
            day_phase = next.day_phase.label(),
            wind_strength = next.wind_strength,
            canopy_sway = next.canopy_sway,
            smoke_rise = next.smoke_rise,
            sea_mist = next.sea_mist,
            shoreline_wash = next.shoreline_wash,
            unease = next.unease,
            life_density = next.life_density,
            warm_window_glow = next.warm_window_glow,
            ground_dampness = next.ground_dampness,
            shoreline_foam = next.shoreline_foam,
            departure_pull = next.departure_pull,
            "village atmosphere updated"
        );
    }
    *atmosphere = next;
    if let Some(mut performance) = performance {
        performance.record_phase_duration(PerformancePhase::Village, started_at.elapsed());
    }
}

fn bootstrap_player_to_village(
    world_map: &WorldMap,
    village: &mut VillageState,
    player_query: &mut Query<&mut Transform, With<WandererPrototype>>,
    camera_query: &mut Query<&mut Transform, (With<WorldCamera>, Without<WandererPrototype>)>,
) {
    let Some(mut player_transform) = player_query.iter_mut().next() else {
        return;
    };
    let spawn = ground_position(world_map, village.spawn_point, 1.2);
    player_transform.translation = spawn;
    player_transform.look_at(village.origin + Vec3::Y * 1.4, Vec3::Y);
    if let Some(mut camera_transform) = camera_query.iter_mut().next() {
        camera_transform.translation = spawn + Vec3::Y * 1.68;
        camera_transform.look_at(village.origin + Vec3::Y * 1.5, Vec3::Y);
    }
    village.spawn_point = spawn;
    village.player_was_bootstrapped = true;
}

#[derive(Debug, Clone, PartialEq)]
struct VillageLayout {
    origin: Vec3,
    spawn_point: Vec3,
    areas: Vec<VillageArea>,
    actors: Vec<VillageActorState>,
    houses: Vec<VillageHouseState>,
}

pub fn build_village_layout(
    world_map: &WorldMap,
    origin: Vec3,
    config: VillageLayoutConfig,
) -> VillageState {
    let layout = build_layout_internal(world_map, origin, config);
    let herding = initialize_herding_state(&layout);
    VillageState {
        origin: layout.origin,
        spawn_point: layout.spawn_point,
        areas: layout.areas,
        houses: layout.houses,
        actors: layout.actors,
        nearest_actor: None,
        nearest_house: None,
        interaction_prompt: None,
        player_was_bootstrapped: false,
        herding,
    }
}

fn initialize_herding_state(layout: &VillageLayout) -> HerdingState {
    let sheep_pen = layout
        .areas
        .iter()
        .find(|area| area.kind == VillageAreaKind::SheepPen)
        .map(|area| area.position)
        .unwrap_or(layout.origin + Vec3::new(19.0, 0.0, -13.0));
    let grass_patch = layout.origin + Vec3::new(12.0, 0.0, -30.0);
    HerdingState {
        known_grass_patch: grass_patch,
        active_grass_patch: grass_patch,
        return_pen_target: sheep_pen,
        flock_center: sheep_pen,
        ..Default::default()
    }
}

fn build_layout_internal(
    world_map: &WorldMap,
    origin: Vec3,
    config: VillageLayoutConfig,
) -> VillageLayout {
    let origin = ground_position(world_map, origin, 0.0);
    let spawn_point = ground_position(world_map, origin + PLAYER_SPAWN_OFFSET, 1.2);
    let areas = vec![
        VillageArea {
            kind: VillageAreaKind::Houses,
            position: ground_position(world_map, origin, 0.0),
            radius: 18.0,
        },
        VillageArea {
            kind: VillageAreaKind::SheepPen,
            position: ground_position(world_map, origin + Vec3::new(19.0, 0.0, -13.0), 0.0),
            radius: 13.5,
        },
        VillageArea {
            kind: VillageAreaKind::Well,
            position: ground_position(world_map, origin + Vec3::new(-4.0, 0.0, -3.0), 0.0),
            radius: 7.5,
        },
        VillageArea {
            kind: VillageAreaKind::Market,
            position: ground_position(world_map, origin + Vec3::new(-16.0, 0.0, -8.0), 0.0),
            radius: 11.0,
        },
        VillageArea {
            kind: VillageAreaKind::Shore,
            position: ground_position(world_map, origin + Vec3::new(0.0, 0.0, 32.0), 0.0),
            radius: 18.0,
        },
        VillageArea {
            kind: VillageAreaKind::OuterPath,
            position: ground_position(world_map, origin + Vec3::new(0.0, 0.0, -36.0), 0.0),
            radius: 14.0,
        },
    ];

    let house_count = config.house_count.max(3);
    let houses = (0..house_count)
        .map(|index| {
            let angle = index as f32 / house_count as f32 * std::f32::consts::TAU + 0.35;
            let radius = 8.0 + (index % 2) as f32 * 4.5;
            let position = ground_position(
                world_map,
                origin + Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius),
                0.0,
            );
            let yaw = house_yaw(index);
            let half_extents = house_half_extents(index);
            let forward = Quat::from_rotation_y(yaw) * -Vec3::Z;
            let door_position = ground_position(
                world_map,
                position + forward * (half_extents.y + 0.92),
                0.08,
            );
            let interior_position = ground_position(world_map, position + forward * 0.72, 1.22);
            VillageHouseState {
                id: stable_house_id(index as u64),
                position,
                yaw,
                half_extents,
                door_position,
                interior_position,
                occupied_by_player: false,
            }
        })
        .collect();

    let sheep_pen = areas
        .iter()
        .find(|area| area.kind == VillageAreaKind::SheepPen)
        .expect("sheep pen area");
    let market = areas
        .iter()
        .find(|area| area.kind == VillageAreaKind::Market)
        .expect("market area");
    let mut actors = Vec::new();
    for index in 0..config.sheep_count.max(4) {
        let angle = index as f32 * 1.31;
        let radius = 2.2 + (index % 4) as f32 * 1.45;
        actors.push(VillageActorState {
            id: stable_actor_id(index as u64, VillageActorKind::Sheep),
            kind: VillageActorKind::Sheep,
            home: ground_position(
                world_map,
                sheep_pen.position + Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius),
                0.6,
            ),
            radius: 7.5,
            behavior: VillageBehavior::Grazing,
        });
    }
    actors.push(VillageActorState {
        id: stable_actor_id(91, VillageActorKind::Shepherd),
        kind: VillageActorKind::Shepherd,
        home: ground_position(
            world_map,
            sheep_pen.position + Vec3::new(-4.5, 0.0, 3.0),
            1.0,
        ),
        radius: 9.0,
        behavior: VillageBehavior::Tending,
    });
    actors.push(VillageActorState {
        id: stable_actor_id(137, VillageActorKind::Merchant),
        kind: VillageActorKind::Merchant,
        home: ground_position(world_map, market.position + Vec3::new(1.0, 0.0, 0.8), 1.0),
        radius: 4.0,
        behavior: VillageBehavior::Trading,
    });

    VillageLayout {
        origin,
        spawn_point,
        areas,
        actors,
        houses,
    }
}

fn choose_village_origin(world_map: &WorldMap, preferred: Vec3) -> Vec3 {
    let preferred_grid_x = (preferred.x / world_map.cell_size()).round() as i32;
    let preferred_grid_z = (preferred.z / world_map.cell_size()).round() as i32;
    let mut best = None;
    for radius in 0_i32..=18 {
        for z in preferred_grid_z - radius..=preferred_grid_z + radius {
            for x in preferred_grid_x - radius..=preferred_grid_x + radius {
                if x != preferred_grid_x - radius
                    && x != preferred_grid_x + radius
                    && z != preferred_grid_z - radius
                    && z != preferred_grid_z + radius
                {
                    continue;
                }
                let Some(tile) = world_map.tile_at_grid(x, z) else {
                    continue;
                };
                let score = (1.0 - tile.slope()).clamp(0.0, 1.0) * 0.6
                    + tile.moisture() * 0.16
                    + (tile.height() - world_map.water_level()).clamp(0.0, 2.0) * 0.12;
                let position = world_map.tile_translation(x, z, tile.height());
                match best {
                    None => best = Some((score, position)),
                    Some((best_score, _)) if score > best_score => best = Some((score, position)),
                    _ => {}
                }
            }
        }
    }
    best.map(|(_, position)| position).unwrap_or(preferred)
}

fn ground_position(world_map: &WorldMap, position: Vec3, y_offset: f32) -> Vec3 {
    let height = world_map
        .sample_height(position.x, position.z)
        .unwrap_or(position.y)
        .max(world_map.water_level() + 0.05);
    Vec3::new(position.x, height + y_offset, position.z)
}

fn stable_house_id(index: u64) -> u64 {
    let mut value = index
        .wrapping_mul(0xD1B5_4A32_D192_ED03)
        .wrapping_add(0xA24B_AED4_963E_E407);
    value ^= value >> 29;
    value = value.wrapping_mul(0x9FB2_1C65_1E98_DF25);
    value ^ (value >> 32)
}

fn house_yaw(index: usize) -> f32 {
    index as f32 * 0.43
}

fn house_half_extents(index: usize) -> Vec2 {
    Vec2::new(
        2.55 + (index % 3) as f32 * 0.16,
        2.18 + (index % 2) as f32 * 0.2,
    )
}

fn spawn_village_visuals(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &ProceduralAssetMaterials,
    asset_config: &crate::core::config::AssetConfig,
    layout: &VillageLayout,
    world_map: &WorldMap,
) {
    let root = commands
        .spawn((
            Name::new("OpeningVillage"),
            DespawnOnExit(AppScreen::InGame),
            Transform::from_translation(layout.origin),
            VillageVisual,
        ))
        .id();
    commands.entity(root).with_children(|parent| {
        for (index, house) in layout.houses.iter().enumerate() {
            let local = house.position - layout.origin;
            spawn_house(
                parent,
                meshes,
                materials,
                asset_config,
                local,
                house.yaw,
                index,
            );
        }
        for area in &layout.areas {
            let local = area.position - layout.origin;
            match area.kind {
                VillageAreaKind::Well => spawn_well(parent, meshes, materials, asset_config, local),
                VillageAreaKind::SheepPen => {
                    spawn_sheep_pen(parent, meshes, materials, asset_config, local)
                }
                VillageAreaKind::Market => {
                    spawn_market(parent, meshes, materials, asset_config, local)
                }
                VillageAreaKind::Shore => {
                    spawn_shore(parent, meshes, materials, asset_config, local, world_map)
                }
                VillageAreaKind::OuterPath => {
                    spawn_path_marker(parent, meshes, materials, asset_config, local)
                }
                VillageAreaKind::Houses => {}
            }
        }
    });
    tag_village_ambient_parts(commands, root);
    spawn_village_colliders(commands, layout);
}

fn spawn_village_colliders(commands: &mut Commands, layout: &VillageLayout) {
    for house in &layout.houses {
        commands.spawn((
            Name::new("VillageHouseCollider"),
            DespawnOnExit(AppScreen::InGame),
            Transform::from_translation(house.position),
            VillageCollider {
                kind: VillageColliderKind::House,
                center: Vec2::new(house.position.x, house.position.z),
                half_extents: house.half_extents,
                yaw: house.yaw,
                radius: house.half_extents.length() + 0.2,
            },
        ));

        let rotation = Quat::from_rotation_y(house.yaw);
        for (name, local, half_extents) in [
            (
                "VillageHouseHearthCollider",
                Vec3::new(
                    -house.half_extents.x * 0.32,
                    0.0,
                    house.half_extents.y * 0.18,
                ),
                Vec2::new(0.48, 0.36),
            ),
            (
                "VillageHouseBedCollider",
                Vec3::new(
                    house.half_extents.x * 0.28,
                    0.0,
                    house.half_extents.y * 0.18,
                ),
                Vec2::new(0.72, 0.42),
            ),
            (
                "VillageHouseTableCollider",
                Vec3::new(0.0, 0.0, -house.half_extents.y * 0.05),
                Vec2::new(0.46, 0.34),
            ),
        ] {
            let world = house.position + rotation * local;
            commands.spawn((
                Name::new(name),
                DespawnOnExit(AppScreen::InGame),
                Transform::from_translation(world),
                VillageCollider {
                    kind: VillageColliderKind::InteriorProp,
                    center: Vec2::new(world.x, world.z),
                    half_extents,
                    yaw: house.yaw,
                    radius: half_extents.length() + 0.2,
                },
            ));
        }
    }

    for area in &layout.areas {
        match area.kind {
            VillageAreaKind::Well => {
                commands.spawn((
                    Name::new("VillageWellCollider"),
                    DespawnOnExit(AppScreen::InGame),
                    Transform::from_translation(area.position),
                    VillageCollider::circle(
                        VillageColliderKind::Well,
                        Vec2::new(area.position.x, area.position.z),
                        1.35,
                    ),
                ));
            }
            VillageAreaKind::SheepPen => {
                for side in 0..4 {
                    let horizontal = side < 2;
                    let offset = match side {
                        0 => Vec3::new(0.0, 0.0, -8.0),
                        1 => Vec3::new(0.0, 0.0, 8.0),
                        2 => Vec3::new(-8.0, 0.0, 0.0),
                        _ => Vec3::new(8.0, 0.0, 0.0),
                    };
                    let world = area.position + offset;
                    commands.spawn((
                        Name::new("SheepPenRailCollider"),
                        DespawnOnExit(AppScreen::InGame),
                        Transform::from_translation(world),
                        VillageCollider {
                            kind: VillageColliderKind::SheepPenRail,
                            center: Vec2::new(world.x, world.z),
                            half_extents: if horizontal {
                                Vec2::new(8.1, 0.28)
                            } else {
                                Vec2::new(0.28, 8.1)
                            },
                            yaw: 0.0,
                            radius: 8.4,
                        },
                    ));
                }
            }
            VillageAreaKind::Market => {
                commands.spawn((
                    Name::new("MarketStallCollider"),
                    DespawnOnExit(AppScreen::InGame),
                    Transform::from_translation(area.position),
                    VillageCollider {
                        kind: VillageColliderKind::MarketStall,
                        center: Vec2::new(area.position.x, area.position.z),
                        half_extents: Vec2::new(2.75, 1.55),
                        yaw: 0.0,
                        radius: 3.2,
                    },
                ));
            }
            VillageAreaKind::Houses | VillageAreaKind::Shore | VillageAreaKind::OuterPath => {}
        }
    }
}

impl VillageCollider {
    fn circle(kind: VillageColliderKind, center: Vec2, radius: f32) -> Self {
        Self {
            kind,
            center,
            half_extents: Vec2::splat(radius.max(0.05)),
            yaw: 0.0,
            radius: radius.max(0.05),
        }
    }
}

pub fn resolve_village_collision(
    start: Vec2,
    desired: Vec2,
    capsule_radius: f32,
    colliders: impl IntoIterator<Item = VillageCollider>,
) -> (Vec2, bool) {
    let mut position = desired;
    let mut blocked = false;
    for collider in colliders {
        if let Some(resolved) = resolve_single_village_collider(position, capsule_radius, collider)
        {
            position = resolved;
            blocked = true;
        }
    }

    if blocked && (position - desired).length_squared() > (desired - start).length_squared() * 1.8 {
        (start, true)
    } else {
        (position, blocked)
    }
}

fn resolve_single_village_collider(
    position: Vec2,
    capsule_radius: f32,
    collider: VillageCollider,
) -> Option<Vec2> {
    if collider.kind == VillageColliderKind::House
        && inside_house_door_gap(
            position,
            collider.center,
            collider.half_extents,
            collider.yaw,
        )
    {
        return None;
    }
    if position.distance_squared(collider.center) > (collider.radius + capsule_radius + 0.6).powi(2)
    {
        return None;
    }

    if collider.kind == VillageColliderKind::Actor {
        return resolve_circle_collision(
            position,
            collider.center,
            collider.radius + capsule_radius,
        );
    }

    resolve_oriented_box_collision(
        position,
        collider.center,
        collider.half_extents + Vec2::splat(capsule_radius),
        collider.yaw,
    )
}

fn resolve_circle_collision(position: Vec2, center: Vec2, radius: f32) -> Option<Vec2> {
    let delta = position - center;
    let distance = delta.length();
    if distance >= radius {
        return None;
    }
    let normal = if distance > 0.0001 {
        delta / distance
    } else {
        Vec2::X
    };
    Some(center + normal * radius)
}

fn resolve_oriented_box_collision(
    position: Vec2,
    center: Vec2,
    half_extents: Vec2,
    yaw: f32,
) -> Option<Vec2> {
    let (local, right, forward) = oriented_local_position(position, center, yaw);
    if local.x.abs() >= half_extents.x || local.y.abs() >= half_extents.y {
        return None;
    }

    let push_x = half_extents.x - local.x.abs();
    let push_z = half_extents.y - local.y.abs();
    let resolved_local = if push_x < push_z {
        Vec2::new(local.x.signum() * half_extents.x, local.y)
    } else {
        Vec2::new(local.x, local.y.signum() * half_extents.y)
    };
    Some(center + right * resolved_local.x + forward * resolved_local.y)
}

fn point_inside_oriented_box(position: Vec2, center: Vec2, half_extents: Vec2, yaw: f32) -> bool {
    let (local, _, _) = oriented_local_position(position, center, yaw);
    local.x.abs() <= half_extents.x && local.y.abs() <= half_extents.y
}

fn inside_house_door_gap(position: Vec2, center: Vec2, half_extents: Vec2, yaw: f32) -> bool {
    let (local, _, _) = oriented_local_position(position, center, yaw);
    let near_front = local.y <= -half_extents.y + 0.72 && local.y >= -half_extents.y - 1.35;
    near_front && local.x.abs() <= 0.82
}

fn oriented_local_position(position: Vec2, center: Vec2, yaw: f32) -> (Vec2, Vec2, Vec2) {
    let right = Vec2::new(yaw.cos(), -yaw.sin());
    let forward = Vec2::new(yaw.sin(), yaw.cos());
    let delta = position - center;
    (
        Vec2::new(delta.dot(right), delta.dot(forward)),
        right,
        forward,
    )
}

fn spawn_house(
    parent: &mut ChildSpawnerCommands<'_>,
    meshes: &mut Assets<Mesh>,
    materials: &ProceduralAssetMaterials,
    asset_config: &crate::core::config::AssetConfig,
    position: Vec3,
    yaw: f32,
    index: usize,
) {
    spawn_procedural_asset(
        parent,
        meshes,
        materials,
        asset_config,
        ProceduralSpawnRequest::new(
            ProceduralAssetKind::VillageHouse,
            index as u64,
            "VillageHouse",
            Transform::from_translation(position).with_rotation(Quat::from_rotation_y(yaw)),
        )
        .with_lod(ProceduralAssetLod::Near),
    );
}

fn spawn_well(
    parent: &mut ChildSpawnerCommands<'_>,
    meshes: &mut Assets<Mesh>,
    materials: &ProceduralAssetMaterials,
    asset_config: &crate::core::config::AssetConfig,
    position: Vec3,
) {
    spawn_procedural_asset(
        parent,
        meshes,
        materials,
        asset_config,
        ProceduralSpawnRequest::new(
            ProceduralAssetKind::VillageWell,
            1,
            "VillageWell",
            Transform::from_translation(position),
        )
        .with_lod(ProceduralAssetLod::Near),
    );
}

fn spawn_sheep_pen(
    parent: &mut ChildSpawnerCommands<'_>,
    meshes: &mut Assets<Mesh>,
    materials: &ProceduralAssetMaterials,
    asset_config: &crate::core::config::AssetConfig,
    position: Vec3,
) {
    for side in 0..4 {
        let horizontal = side < 2;
        let offset = match side {
            0 => Vec3::new(0.0, 0.55, -8.0),
            1 => Vec3::new(0.0, 0.55, 8.0),
            2 => Vec3::new(-8.0, 0.55, 0.0),
            _ => Vec3::new(8.0, 0.55, 0.0),
        };
        let rotation = if horizontal {
            Quat::IDENTITY
        } else {
            Quat::from_rotation_y(std::f32::consts::FRAC_PI_2)
        };
        spawn_procedural_asset(
            parent,
            meshes,
            materials,
            asset_config,
            ProceduralSpawnRequest::new(
                ProceduralAssetKind::SheepPenRail,
                side as u64,
                "SheepPenRail",
                Transform::from_translation(position + offset).with_rotation(rotation),
            )
            .with_lod(ProceduralAssetLod::Near),
        );
    }
}

fn spawn_market(
    parent: &mut ChildSpawnerCommands<'_>,
    meshes: &mut Assets<Mesh>,
    materials: &ProceduralAssetMaterials,
    asset_config: &crate::core::config::AssetConfig,
    position: Vec3,
) {
    spawn_procedural_asset(
        parent,
        meshes,
        materials,
        asset_config,
        ProceduralSpawnRequest::new(
            ProceduralAssetKind::MarketStall,
            1,
            "MarketStall",
            Transform::from_translation(position),
        )
        .with_lod(ProceduralAssetLod::Near),
    );
}

fn spawn_shore(
    parent: &mut ChildSpawnerCommands<'_>,
    meshes: &mut Assets<Mesh>,
    materials: &ProceduralAssetMaterials,
    asset_config: &crate::core::config::AssetConfig,
    position: Vec3,
    _world_map: &WorldMap,
) {
    spawn_procedural_asset(
        parent,
        meshes,
        materials,
        asset_config,
        ProceduralSpawnRequest::new(
            ProceduralAssetKind::VillageShore,
            1,
            "VillageShore",
            Transform::from_translation(position),
        )
        .with_lod(ProceduralAssetLod::Near),
    );
}

fn spawn_path_marker(
    parent: &mut ChildSpawnerCommands<'_>,
    meshes: &mut Assets<Mesh>,
    materials: &ProceduralAssetMaterials,
    asset_config: &crate::core::config::AssetConfig,
    position: Vec3,
) {
    for index in 0..5 {
        spawn_procedural_asset(
            parent,
            meshes,
            materials,
            asset_config,
            ProceduralSpawnRequest::new(
                ProceduralAssetKind::PathStone,
                index as u64,
                "OuterPathStone",
                Transform::from_translation(
                    position + Vec3::new((index as f32 - 2.0) * 1.8, 0.0, index as f32 * -1.1),
                ),
            )
            .with_lod(ProceduralAssetLod::Near),
        );
    }
}

fn spawn_village_actors(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &ProceduralAssetMaterials,
    asset_config: &crate::core::config::AssetConfig,
    layout: &VillageLayout,
    world_map: &WorldMap,
) {
    for actor in &layout.actors {
        if actor.kind == VillageActorKind::Sheep {
            continue;
        }
        let seed = actor.id as u32;
        let position = ground_position(world_map, actor.home, actor_y_offset(actor.kind));
        match actor.kind {
            VillageActorKind::Sheep => {
                let entity = spawn_procedural_asset_entity(
                    commands,
                    meshes,
                    materials,
                    asset_config,
                    ProceduralSpawnRequest::new(
                        ProceduralAssetKind::Sheep,
                        actor.id,
                        "VillageSheep",
                        Transform::from_translation(position),
                    )
                    .with_lod(ProceduralAssetLod::Near),
                );
                commands.entity(entity).insert(VillageActor {
                    id: actor.id,
                    kind: actor.kind,
                    home: actor.home,
                    radius: actor.radius,
                    seed,
                });
                commands.entity(entity).insert(VillageCollider::circle(
                    VillageColliderKind::Actor,
                    Vec2::new(position.x, position.z),
                    0.52,
                ));
                tag_village_actor_parts(commands, entity, actor.id, actor.kind);
            }
            VillageActorKind::Shepherd | VillageActorKind::Merchant => {
                let asset_kind = match actor.kind {
                    VillageActorKind::Shepherd => ProceduralAssetKind::Shepherd,
                    VillageActorKind::Merchant => ProceduralAssetKind::Merchant,
                    VillageActorKind::Sheep => ProceduralAssetKind::Sheep,
                };
                let entity = spawn_procedural_asset_entity(
                    commands,
                    meshes,
                    materials,
                    asset_config,
                    ProceduralSpawnRequest::new(
                        asset_kind,
                        actor.id,
                        actor.kind.label(),
                        Transform::from_translation(position),
                    )
                    .with_lod(ProceduralAssetLod::Near),
                );
                commands.entity(entity).insert(VillageActor {
                    id: actor.id,
                    kind: actor.kind,
                    home: actor.home,
                    radius: actor.radius,
                    seed,
                });
                commands.entity(entity).insert(VillageCollider::circle(
                    VillageColliderKind::Actor,
                    Vec2::new(position.x, position.z),
                    0.48,
                ));
                tag_village_actor_parts(commands, entity, actor.id, actor.kind);
            }
        }
    }
}

fn sync_village_collider_centers(
    mut query: Query<(&Transform, &mut VillageCollider)>,
    performance: Option<ResMut<FramePerformance>>,
) {
    let started_at = Instant::now();
    for (transform, mut collider) in &mut query {
        collider.center = Vec2::new(transform.translation.x, transform.translation.z);
    }
    if let Some(mut performance) = performance {
        performance.record_phase_duration(PerformancePhase::Village, started_at.elapsed());
    }
}

fn tag_village_actor_parts(
    commands: &mut Commands,
    root: Entity,
    actor_id: u64,
    actor_kind: VillageActorKind,
) {
    commands.queue(move |world: &mut World| {
        let placeholder_enabled = world
            .get::<AssetPresentationAnchor>(root)
            .is_some_and(|anchor| anchor.placeholder_enabled);
        if !placeholder_enabled {
            return;
        }
        let Some(children) = world
            .get::<Children>(root)
            .map(|children| children.iter().collect::<Vec<_>>())
        else {
            return;
        };
        for child in children {
            let Some(role) = world.get::<ProceduralAnimationRole>(child).copied() else {
                continue;
            };
            let Some(transform) = world.get::<Transform>(child).copied() else {
                continue;
            };
            world.entity_mut(child).insert(VillageAnimatedPart {
                actor_id: Some(actor_id),
                actor_kind: Some(actor_kind),
                role,
                base_translation: transform.translation,
                base_rotation: transform.rotation,
                base_scale: transform.scale,
            });
        }
    });
}

fn tag_village_ambient_parts(commands: &mut Commands, root: Entity) {
    commands.queue(move |world: &mut World| {
        let Some(children) = world
            .get::<Children>(root)
            .map(|children| children.iter().collect::<Vec<_>>())
        else {
            return;
        };
        for asset_root in children {
            let placeholder_enabled = world
                .get::<AssetPresentationAnchor>(asset_root)
                .is_some_and(|anchor| anchor.placeholder_enabled);
            if !placeholder_enabled {
                continue;
            }
            let Some(part_children) = world
                .get::<Children>(asset_root)
                .map(|children| children.iter().collect::<Vec<_>>())
            else {
                continue;
            };
            for child in part_children {
                let part_name = world
                    .get::<Name>(child)
                    .map(|name| name.as_str().to_string())
                    .unwrap_or_default();
                let Some(role) = world.get::<ProceduralAnimationRole>(child).copied() else {
                    continue;
                };
                let Some(transform) = world.get::<Transform>(child).copied() else {
                    continue;
                };
                let original_material = world
                    .get::<MeshMaterial3d<StandardMaterial>>(child)
                    .map(|material| material.0.clone());
                let mut entity = world.entity_mut(child);
                entity.insert(VillageAnimatedPart {
                    actor_id: None,
                    actor_kind: None,
                    role,
                    base_translation: transform.translation,
                    base_rotation: transform.rotation,
                    base_scale: transform.scale,
                });
                if let Some(kind) = village_part_kind(part_name.as_str()) {
                    entity.insert(kind);
                    if let Some(handle) = original_material.clone() {
                        entity.insert((
                            VillageMaterialOverride { original: handle },
                            VillageMaterialOverridePending,
                        ));
                    }
                }
            }
        }
    });
}

fn update_village_actor_behavior(
    time: Res<Time>,
    world_map: Option<Res<WorldMap>>,
    village: Option<Res<VillageState>>,
    ecology: Option<Res<EcologyState>>,
    player_query: Query<&Transform, With<WandererPrototype>>,
    mut actor_query: Query<(&VillageActor, &mut Transform), Without<WandererPrototype>>,
    performance: Option<ResMut<FramePerformance>>,
) {
    let Some(world_map) = world_map else {
        return;
    };
    let started_at = Instant::now();
    let player_position = player_query
        .iter()
        .next()
        .map(|transform| transform.translation);
    let herding = village.as_deref().map(|village| village.herding);
    let sheep_flock = ecology.as_deref().and_then(sheep_flock_state).cloned();
    for (actor, mut transform) in &mut actor_query {
        let target = actor_target_position(
            actor,
            time.elapsed_secs(),
            player_position,
            herding,
            sheep_flock.as_ref(),
        );
        let target = ground_position(&world_map, target, actor_y_offset(actor.kind));
        let smoothing = 1.0 - (-actor_speed(actor.kind) * time.delta_secs()).exp();
        transform.translation = transform.translation.lerp(target, smoothing);

        match actor.kind {
            VillageActorKind::Merchant => {
                if let Some(player_position) = player_position {
                    let y = transform.translation.y;
                    transform.look_at(Vec3::new(player_position.x, y, player_position.z), Vec3::Y);
                }
            }
            _ => {
                let look_target = Vec3::new(target.x, transform.translation.y, target.z);
                if planar_distance(transform.translation, look_target) > 0.2 {
                    transform.look_at(look_target, Vec3::Y);
                }
            }
        }
    }
    if let Some(mut performance) = performance {
        performance.record_phase_duration(PerformancePhase::Village, started_at.elapsed());
    }
}

fn animate_village_asset_parts(
    time: Res<Time>,
    config: Res<crate::core::config::AppConfig>,
    atmosphere: Res<VillageAtmosphere>,
    mut part_query: Query<(&VillageAnimatedPart, &mut Transform)>,
    performance: Option<ResMut<FramePerformance>>,
) {
    if !config.assets.animate_placeholder_characters && !config.assets.animate_placeholder_ambience
    {
        return;
    }
    let started_at = Instant::now();
    let elapsed = time.elapsed_secs();
    for (part, mut transform) in &mut part_query {
        let phase_seed = part.actor_id.unwrap_or(17) as f32 * 0.013;
        let phase = elapsed + phase_seed;
        transform.translation = part.base_translation;
        transform.rotation = part.base_rotation;
        transform.scale = part.base_scale;

        match part.role {
            ProceduralAnimationRole::SheepHead => {
                let grazing = part.actor_kind == Some(VillageActorKind::Sheep);
                let nod = if grazing {
                    (phase * (1.5 + atmosphere.unease * 1.6)).sin().max(0.0)
                        * (0.24 + atmosphere.unease * 0.16)
                } else {
                    0.04 * (phase * 0.8).sin()
                };
                transform.rotation = part.base_rotation * Quat::from_rotation_x(0.28 + nod);
                transform.translation.y += nod * -0.08;
            }
            ProceduralAnimationRole::SheepLegFrontLeft
            | ProceduralAnimationRole::SheepLegBackRight => {
                let swing = (phase * 4.4).sin() * 0.18;
                transform.rotation = part.base_rotation * Quat::from_rotation_x(swing);
            }
            ProceduralAnimationRole::SheepLegFrontRight
            | ProceduralAnimationRole::SheepLegBackLeft => {
                let swing = (phase * 4.4 + std::f32::consts::PI).sin() * 0.18;
                transform.rotation = part.base_rotation * Quat::from_rotation_x(swing);
            }
            ProceduralAnimationRole::NpcHead => {
                transform.rotation =
                    part.base_rotation * Quat::from_rotation_y((phase * 0.7).sin() * 0.12);
            }
            ProceduralAnimationRole::NpcHandLeft => {
                transform.translation.y +=
                    (phase * 1.4).sin() * (0.025 + atmosphere.life_density * 0.018);
            }
            ProceduralAnimationRole::NpcHandRight => {
                transform.translation.y +=
                    (phase * 1.2 + 0.6).sin() * (0.032 + atmosphere.life_density * 0.028);
            }
            ProceduralAnimationRole::ClothCanopy => {
                let flutter = (phase * (1.3 + atmosphere.wind_strength * 2.8)).sin()
                    * (0.03 + atmosphere.canopy_sway * 0.12);
                transform.rotation = part.base_rotation * Quat::from_rotation_x(flutter);
                let lateral = atmosphere.wind_dir * atmosphere.canopy_sway * 0.08;
                transform.translation += Vec3::new(lateral.x, flutter.abs() * 0.08, lateral.y);
            }
            ProceduralAnimationRole::Smoke => {
                let lateral = atmosphere.wind_dir * (0.08 + atmosphere.wind_strength * 0.26);
                let drift = Vec3::new(
                    lateral.x + (phase * 0.7).sin() * 0.04,
                    atmosphere.smoke_rise + (phase * 0.33).sin() * 0.04,
                    lateral.y,
                );
                transform.translation = part.base_translation + drift;
                transform.scale = part.base_scale
                    * (1.0 + (phase * 0.9).sin().abs() * (0.08 + atmosphere.smoke_rise * 0.18));
            }
            ProceduralAnimationRole::WaterRipple => {
                let pulse = 1.0
                    + (phase * (0.9 + atmosphere.shoreline_wash * 1.2)).sin()
                        * (0.015 + atmosphere.shoreline_wash * 0.05);
                transform.scale = Vec3::new(
                    part.base_scale.x * pulse,
                    part.base_scale.y,
                    part.base_scale.z * (1.0 + atmosphere.sea_mist * 0.06),
                );
            }
            ProceduralAnimationRole::BirdLeftWing
            | ProceduralAnimationRole::BirdRightWing
            | ProceduralAnimationRole::FishTail => {}
        }
    }
    if let Some(mut performance) = performance {
        performance.record_phase_duration(PerformancePhase::Village, started_at.elapsed());
    }
}

fn village_part_kind(part_name: &str) -> Option<VillageVisualPartKind> {
    if matches!(part_name, "HouseWindowWarmLeft" | "HouseWindowWarmRight") {
        return Some(VillageVisualPartKind::WarmWindow);
    }
    if part_name == "HouseInteriorLantern" || part_name == "MarketHangingScale" {
        return Some(VillageVisualPartKind::WarmLantern);
    }
    if part_name == "HouseSmokeWisp" {
        return Some(VillageVisualPartKind::SmokeWisp);
    }
    if matches!(part_name, "WellWetGround" | "ShoreWetSand") {
        return Some(VillageVisualPartKind::WetGround);
    }
    if matches!(part_name, "ShoreWater" | "WellWater") {
        return Some(VillageVisualPartKind::ShoreWater);
    }
    if matches!(part_name, "ShoreFoamLineA" | "ShoreFoamLineB") {
        return Some(VillageVisualPartKind::ShoreFoam);
    }
    if part_name == "PathStone" || part_name == "PathDustPatch" {
        return Some(VillageVisualPartKind::PathStone);
    }
    None
}

fn ensure_village_material_overrides(
    mut commands: Commands,
    mut materials: ResMut<Assets<StandardMaterial>>,
    query: VillageVisualMaterialQuery<'_, '_>,
    performance: Option<ResMut<FramePerformance>>,
) {
    let started_at = Instant::now();
    for (entity, material, override_tag) in &query {
        if material.0 != override_tag.original {
            commands
                .entity(entity)
                .remove::<VillageMaterialOverridePending>();
            continue;
        }
        let Some(existing) = materials.get(&material.0).cloned() else {
            commands
                .entity(entity)
                .remove::<VillageMaterialOverridePending>();
            continue;
        };
        let cloned_handle = materials.add(existing);
        commands
            .entity(entity)
            .insert(MeshMaterial3d(cloned_handle));
        commands
            .entity(entity)
            .remove::<VillageMaterialOverridePending>();
    }
    if let Some(mut performance) = performance {
        performance.record_phase_duration(PerformancePhase::Village, started_at.elapsed());
    }
}

fn update_village_visual_materials(
    atmosphere: Res<VillageAtmosphere>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut query: VillageVisualRuntimeMaterialQuery<'_, '_>,
    performance: Option<ResMut<FramePerformance>>,
) {
    if !atmosphere.is_changed() {
        return;
    }
    let started_at = Instant::now();

    for (kind, material_handle, _override_tag) in &mut query {
        let Some(material) = materials.get_mut(&material_handle.0) else {
            continue;
        };
        match kind {
            VillageVisualPartKind::WarmWindow => {
                let glow = atmosphere.warm_window_glow;
                material.base_color = Color::srgba(
                    0.92 + glow * 0.07,
                    0.62 + glow * 0.26,
                    0.34 + glow * 0.18,
                    0.62 + glow * 0.33,
                );
                material.emissive =
                    LinearRgba::rgb(1.8 + glow * 3.2, 0.95 + glow * 1.9, 0.35 + glow * 0.95);
                material.alpha_mode = AlphaMode::Blend;
            }
            VillageVisualPartKind::WarmLantern => {
                let glow =
                    (atmosphere.warm_window_glow * 0.88 + atmosphere.unease * 0.12).clamp(0.0, 1.0);
                material.base_color = Color::srgb(0.96, 0.74 + glow * 0.14, 0.4 + glow * 0.12);
                material.emissive =
                    LinearRgba::rgb(2.1 + glow * 3.8, 1.2 + glow * 2.3, 0.48 + glow * 1.18);
            }
            VillageVisualPartKind::SmokeWisp => {
                let alpha =
                    (0.3 + atmosphere.sea_mist * 0.34 + atmosphere.unease * 0.18).clamp(0.2, 0.9);
                material.base_color = Color::srgba(0.15, 0.16, 0.17, alpha);
                material.emissive = LinearRgba::rgb(
                    0.02 + atmosphere.warm_window_glow * 0.04,
                    0.02 + atmosphere.warm_window_glow * 0.03,
                    0.02 + atmosphere.warm_window_glow * 0.025,
                );
                material.alpha_mode = AlphaMode::Blend;
            }
            VillageVisualPartKind::WetGround => {
                let wet = atmosphere.ground_dampness;
                material.base_color =
                    Color::srgb(0.22 - wet * 0.05, 0.19 - wet * 0.02, 0.16 - wet * 0.005);
                material.perceptual_roughness = (0.92 - wet * 0.54).clamp(0.22, 1.0);
                material.metallic = (0.01 + wet * 0.08).clamp(0.0, 0.25);
            }
            VillageVisualPartKind::ShoreWater => {
                let wash = atmosphere.shoreline_wash;
                material.base_color = Color::srgba(
                    0.16 + wash * 0.07,
                    0.38 + wash * 0.1,
                    0.45 + wash * 0.13,
                    0.5 + wash * 0.32,
                );
                material.emissive =
                    LinearRgba::rgb(0.02 + wash * 0.06, 0.05 + wash * 0.09, 0.06 + wash * 0.11);
                material.perceptual_roughness = (0.36 - wash * 0.22).clamp(0.1, 0.5);
                material.alpha_mode = AlphaMode::Blend;
            }
            VillageVisualPartKind::ShoreFoam => {
                let foam = atmosphere.shoreline_foam;
                material.base_color = Color::srgba(
                    0.88 + foam * 0.1,
                    0.9 + foam * 0.08,
                    0.84 + foam * 0.04,
                    0.34 + foam * 0.5,
                );
                material.emissive =
                    LinearRgba::rgb(0.16 + foam * 0.34, 0.17 + foam * 0.32, 0.12 + foam * 0.2);
                material.alpha_mode = AlphaMode::Blend;
            }
            VillageVisualPartKind::PathStone => {
                let pull = atmosphere.departure_pull;
                material.base_color =
                    Color::srgb(0.42 + pull * 0.08, 0.39 + pull * 0.06, 0.35 + pull * 0.05);
                material.emissive =
                    LinearRgba::rgb(0.01 + pull * 0.08, 0.01 + pull * 0.05, 0.01 + pull * 0.03);
            }
        }
    }
    if let Some(mut performance) = performance {
        performance.record_phase_duration(PerformancePhase::Village, started_at.elapsed());
    }
}

fn village_day_phase(daylight: f32) -> VillageDayPhase {
    if daylight < 0.16 {
        VillageDayPhase::Night
    } else if daylight < 0.4 {
        VillageDayPhase::Dawn
    } else if daylight < 0.78 {
        VillageDayPhase::Day
    } else {
        VillageDayPhase::Dusk
    }
}

fn actor_target_position(
    actor: &VillageActor,
    elapsed: f32,
    player_position: Option<Vec3>,
    herding: Option<HerdingState>,
    sheep_flock: Option<&AnimalFlockState>,
) -> Vec3 {
    if actor.kind == VillageActorKind::Shepherd {
        return shepherd_schedule_position(actor.home, actor.radius, elapsed).target;
    }
    if actor.kind == VillageActorKind::Sheep
        && let Some(herding) = herding
        && herding.flock_following_player
        && let Some(player_position) = player_position
    {
        let phase = elapsed * 0.56 + actor.seed as f32 * 0.021;
        let radius = 3.6 + (actor.seed % 4) as f32 * 0.48;
        let follow_anchor = player_position + Vec3::new(-2.4, 0.0, 1.6);
        return follow_anchor
            + Vec3::new(
                phase.cos() * radius,
                0.0,
                (phase * 0.83).sin() * radius * 0.72,
            );
    }
    if actor.kind == VillageActorKind::Sheep
        && let Some(flock) = sheep_flock
    {
        let phase = elapsed * 0.42 + actor.seed as f32 * 0.019;
        let restless = flock.behavior == AnimalBehavior::Scattering;
        let radius = flock.radius * if restless { 0.68 } else { 0.42 };
        return flock.center
            + Vec3::new(
                phase.cos() * radius + ((actor.id % 5) as f32 - 2.0) * 0.42,
                0.0,
                (phase * 0.71).sin() * radius + ((actor.id % 7) as f32 - 3.0) * 0.36,
            );
    }

    let phase = elapsed * actor_motion_frequency(actor.kind) + actor.seed as f32 * 0.017;
    let radius = match actor.kind {
        VillageActorKind::Sheep => actor.radius * 0.48,
        VillageActorKind::Shepherd => actor.radius * 0.34,
        VillageActorKind::Merchant => actor.radius * 0.16,
    };
    actor.home
        + Vec3::new(
            phase.cos() * radius,
            0.0,
            (phase * 0.73 + 0.4).sin() * radius,
        )
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum ShepherdSchedulePhase {
    TendingFlock,
    WatchingGate,
    ReturningVillage,
    RestingVillage,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShepherdSchedulePosition {
    pub phase: ShepherdSchedulePhase,
    pub target: Vec3,
}

pub fn shepherd_schedule_position(
    sheep_pen_home: Vec3,
    radius: f32,
    elapsed: f32,
) -> ShepherdSchedulePosition {
    let day_progress = (elapsed / 48.0).fract();
    let village_rest = sheep_pen_home + Vec3::new(-18.0, 0.0, 14.0);
    let gate_watch = sheep_pen_home + Vec3::new(radius * 0.42, 0.0, -radius * 0.18);
    let phase_wave = elapsed * 0.18;
    if day_progress < 0.48 {
        ShepherdSchedulePosition {
            phase: ShepherdSchedulePhase::TendingFlock,
            target: sheep_pen_home
                + Vec3::new(
                    phase_wave.cos() * radius * 0.22,
                    0.0,
                    (phase_wave * 0.7).sin() * radius * 0.18,
                ),
        }
    } else if day_progress < 0.64 {
        ShepherdSchedulePosition {
            phase: ShepherdSchedulePhase::WatchingGate,
            target: gate_watch,
        }
    } else if day_progress < 0.82 {
        ShepherdSchedulePosition {
            phase: ShepherdSchedulePhase::ReturningVillage,
            target: village_rest.lerp(sheep_pen_home, (day_progress - 0.64) / 0.18),
        }
    } else {
        ShepherdSchedulePosition {
            phase: ShepherdSchedulePhase::RestingVillage,
            target: village_rest,
        }
    }
}

fn actor_motion_frequency(kind: VillageActorKind) -> f32 {
    match kind {
        VillageActorKind::Sheep => 0.34,
        VillageActorKind::Shepherd => 0.16,
        VillageActorKind::Merchant => 0.08,
    }
}

fn actor_speed(kind: VillageActorKind) -> f32 {
    match kind {
        VillageActorKind::Sheep => 1.8,
        VillageActorKind::Shepherd => 0.9,
        VillageActorKind::Merchant => 0.55,
    }
}

fn actor_y_offset(kind: VillageActorKind) -> f32 {
    match kind {
        VillageActorKind::Sheep => 0.58,
        VillageActorKind::Shepherd | VillageActorKind::Merchant => 1.05,
    }
}

fn update_village_interaction(
    resources: VillageInteractionResources<'_>,
    mut player_query: Query<&mut Transform, With<WandererPrototype>>,
    actor_query: Query<(&VillageActor, &Transform), Without<WandererPrototype>>,
    performance: Option<ResMut<FramePerformance>>,
) {
    let (time, keys, village, ecology, mut intent, mut notebook) = resources;
    let Some(mut village) = village else {
        return;
    };
    let Some(mut player_transform) = player_query.iter_mut().next() else {
        return;
    };
    let started_at = Instant::now();

    'interaction: {
        let nearest_house = nearest_house_interaction(&village, player_transform.translation);
        let nearest = actor_query
            .iter()
            .filter_map(|(actor, transform)| {
                let distance = planar_distance(player_transform.translation, transform.translation);
                (distance <= INTERACTION_RADIUS).then_some(VillageActorSnapshot {
                    id: actor.id,
                    kind: actor.kind,
                    distance,
                })
            })
            .min_by(|a, b| a.distance.total_cmp(&b.distance));

        village.nearest_actor = nearest;
        village.nearest_house = nearest_house;
        village.interaction_prompt = build_village_interaction_prompt(
            nearest,
            nearest_house,
            &village.herding,
            player_transform.translation,
        );

        if !keys.just_pressed(KeyCode::KeyF) {
            break 'interaction;
        }

        if try_handle_house_interaction(
            &mut village,
            &mut player_transform,
            nearest_house,
            time.elapsed_secs(),
            notebook.as_deref_mut(),
        ) {
            break 'interaction;
        }

        if try_handle_herding_interaction(
            &mut village,
            ecology,
            &mut intent,
            notebook.as_deref_mut(),
            player_transform.translation,
            time.elapsed_secs(),
        ) {
            break 'interaction;
        }

        let Some(actor) = nearest else {
            break 'interaction;
        };
        let record = village_interaction_record(actor.kind, time.elapsed_secs());
        let _ = record_notebook_entry(notebook.as_deref_mut(), record);
        if let Some(intent) = intent.as_deref_mut() {
            let changed = apply_village_dialogue_intent(intent, actor.kind, time.elapsed_secs());
            if let Some(kind) = changed {
                tracing::info!(
                    target: "dao_game::village::interaction",
                    actor = actor.kind.label(),
                    intent = kind.label(),
                    strength = intent.strength(kind),
                    "village dialogue shaped player intent"
                );
            }
        }
        tracing::info!(
            target: "dao_game::village::interaction",
            actor_id = actor.id,
            actor = actor.kind.label(),
            distance = actor.distance,
            "village light interaction completed"
        );
    }
    if let Some(mut performance) = performance {
        performance.record_phase_duration(PerformancePhase::Village, started_at.elapsed());
    }
}

fn build_village_interaction_prompt(
    nearest: Option<VillageActorSnapshot>,
    nearest_house: Option<VillageHouseSnapshot>,
    herding: &HerdingState,
    player_position: Vec3,
) -> Option<String> {
    if let Some(house) = nearest_house {
        return Some(if house.inside {
            "可离开屋舍".to_string()
        } else {
            "可推门入内".to_string()
        });
    }

    let herding_prompt = match herding.phase {
        HerdingPhase::NotStarted | HerdingPhase::Prompted if herding.task_available => {
            Some("可开始放羊".to_string())
        }
        HerdingPhase::FollowingToGrass
            if planar_distance(player_position, herding.flock_center)
                <= HERDING_NEAR_FLOCK_RADIUS =>
        {
            Some("可引羊去草地".to_string())
        }
        HerdingPhase::GrazingAtPatch
            if planar_distance(player_position, herding.active_grass_patch)
                <= HERDING_GRAZE_RADIUS + 3.0 =>
        {
            Some("可唤羊回圈".to_string())
        }
        HerdingPhase::ReturningToPen
            if planar_distance(player_position, herding.flock_center)
                <= HERDING_NEAR_FLOCK_RADIUS =>
        {
            Some("可引羊回圈".to_string())
        }
        _ => None,
    };
    herding_prompt.or_else(|| nearest.map(|actor| actor.kind.prompt().to_string()))
}

fn nearest_house_interaction(
    village: &VillageState,
    player_position: Vec3,
) -> Option<VillageHouseSnapshot> {
    village
        .houses
        .iter()
        .filter_map(|house| {
            let inside = house.occupied_by_player
                || point_inside_oriented_box(
                    Vec2::new(player_position.x, player_position.z),
                    Vec2::new(house.position.x, house.position.z),
                    house.half_extents + Vec2::splat(0.18),
                    house.yaw,
                );
            let reference = if inside {
                house.interior_position
            } else {
                house.door_position
            };
            let distance = planar_distance(player_position, reference);
            let radius = if inside { 4.2 } else { INTERACTION_RADIUS };
            (distance <= radius).then_some(VillageHouseSnapshot {
                id: house.id,
                distance,
                inside,
            })
        })
        .min_by(|a, b| a.distance.total_cmp(&b.distance))
}

fn try_handle_house_interaction(
    village: &mut VillageState,
    player_transform: &mut Transform,
    nearest_house: Option<VillageHouseSnapshot>,
    at_seconds: f32,
    notebook: Option<&mut NotebookState>,
) -> bool {
    let Some(snapshot) = nearest_house else {
        return false;
    };
    let Some(house_index) = village
        .houses
        .iter()
        .position(|house| house.id == snapshot.id)
    else {
        return false;
    };

    let house = &mut village.houses[house_index];
    if snapshot.inside {
        let forward = Quat::from_rotation_y(house.yaw) * -Vec3::Z;
        player_transform.translation = house.door_position + forward * 1.8 + Vec3::Y * 1.12;
        house.occupied_by_player = false;
        tracing::info!(
            target: "dao_game::village::house",
            house_id = house.id,
            "player exited procedural house"
        );
    } else {
        player_transform.translation = house.interior_position;
        player_transform.rotation = Quat::from_rotation_y(house.yaw + std::f32::consts::PI);
        house.occupied_by_player = true;
        let _ = record_notebook_entry(
            notebook,
            NotebookRecord {
                kind: NotebookEntryKind::Observation,
                at_seconds,
                location: Some("屋舍".to_string()),
                source: NotebookSource::Observation,
                title: "推门入内".to_string(),
                body: "屋里有泥地、床铺、炉灶和木桌。门没有把你挡在外面，村庄也不再只是外壳。"
                    .to_string(),
                tags: vec![NotebookTag::Village, NotebookTag::Memory],
            },
        );
        tracing::info!(
            target: "dao_game::village::house",
            house_id = house.id,
            "player entered procedural house"
        );
    }
    true
}

fn try_handle_herding_interaction(
    village: &mut VillageState,
    ecology: Option<ResMut<EcologyState>>,
    intent: &mut Option<ResMut<IntentState>>,
    notebook: Option<&mut NotebookState>,
    player_position: Vec3,
    at_seconds: f32,
) -> bool {
    let Some(mut ecology) = ecology else {
        return false;
    };
    let Some(flock) = sheep_flock_state_mut(&mut ecology) else {
        return false;
    };
    let near_flock = planar_distance(player_position, flock.center)
        <= HERDING_NEAR_FLOCK_RADIUS.max(flock.radius);
    match village.herding.phase {
        HerdingPhase::NotStarted | HerdingPhase::Prompted if village.herding.task_available => {
            village.herding.phase = HerdingPhase::FollowingToGrass;
            village.herding.phase_started_at = at_seconds;
            village.herding.flock_following_player = true;
            flock.behavior = AnimalBehavior::Migrating;
            flock.radius = 7.8;
            let _ = record_notebook_entry(
                notebook,
                NotebookRecord {
                    kind: NotebookEntryKind::Observation,
                    at_seconds,
                    location: Some("羊圈".to_string()),
                    source: NotebookSource::Observation,
                    title: "开始放羊".to_string(),
                    body: "你拍了拍羊群边上的木栏，先带它们去村外有风和草的地方。".to_string(),
                    tags: vec![
                        NotebookTag::Village,
                        NotebookTag::Flock,
                        NotebookTag::Shepherd,
                    ],
                },
            );
            if let Some(intent) = intent.as_deref_mut() {
                let _ = crate::game::intent::advance_intent_state(
                    intent,
                    0.0,
                    at_seconds,
                    [
                        crate::game::intent::IntentSample {
                            kind: crate::game::intent::IntentKind::Animals,
                            source: crate::game::intent::IntentSource::Dialogue,
                            amount: 1.1,
                        },
                        crate::game::intent::IntentSample {
                            kind: crate::game::intent::IntentKind::BeyondVillage,
                            source: crate::game::intent::IntentSource::Dialogue,
                            amount: 0.8,
                        },
                    ],
                );
            }
            tracing::info!(
                target: "dao_game::village::herding",
                phase = village.herding.phase.label(),
                "opening herding task started"
            );
            true
        }
        HerdingPhase::FollowingToGrass if near_flock && village.herding.grass_patch_reached => {
            village.herding.phase = HerdingPhase::ReturningToPen;
            village.herding.phase_started_at = at_seconds;
            village.herding.flock_following_player = true;
            flock.behavior = AnimalBehavior::Migrating;
            flock.radius = 7.2;
            let _ = record_notebook_entry(
                notebook,
                NotebookRecord {
                    kind: NotebookEntryKind::Observation,
                    at_seconds,
                    location: Some("村外草地".to_string()),
                    source: NotebookSource::Observation,
                    title: "羊群已经吃饱".to_string(),
                    body: "风慢了些，羊群低头啃完这片草，开始愿意跟着你往回走。".to_string(),
                    tags: vec![
                        NotebookTag::Village,
                        NotebookTag::Flock,
                        NotebookTag::Memory,
                    ],
                },
            );
            tracing::info!(
                target: "dao_game::village::herding",
                phase = village.herding.phase.label(),
                "herding task switched to return phase"
            );
            true
        }
        HerdingPhase::GrazingAtPatch if near_flock => {
            village.herding.phase = HerdingPhase::ReturningToPen;
            village.herding.phase_started_at = at_seconds;
            village.herding.flock_following_player = true;
            flock.behavior = AnimalBehavior::Migrating;
            flock.radius = 7.2;
            true
        }
        HerdingPhase::ReturningToPen if near_flock && village.herding.pen_returned => {
            village.herding.phase = HerdingPhase::Completed;
            village.herding.phase_started_at = at_seconds;
            village.herding.flock_following_player = false;
            village.herding.first_task_completed = true;
            flock.behavior = AnimalBehavior::Grazing;
            flock.home = village.herding.return_pen_target;
            flock.center = village.herding.return_pen_target;
            flock.radius = 10.8;
            let _ = record_notebook_entry(
                notebook,
                NotebookRecord {
                    kind: NotebookEntryKind::JourneyEcho,
                    at_seconds,
                    location: Some("羊圈".to_string()),
                    source: NotebookSource::Journey,
                    title: "第一次放羊结束".to_string(),
                    body: "羊群重新安静下来。村庄没有催你，但你知道自己迟早会像它们一样，顺着风走出村外。".to_string(),
                    tags: vec![
                        NotebookTag::Village,
                        NotebookTag::Flock,
                        NotebookTag::Memory,
                        NotebookTag::Omen,
                    ],
                },
            );
            if let Some(intent) = intent.as_deref_mut() {
                let _ = crate::game::intent::advance_intent_state(
                    intent,
                    0.0,
                    at_seconds,
                    [crate::game::intent::IntentSample {
                        kind: crate::game::intent::IntentKind::BeyondVillage,
                        source: crate::game::intent::IntentSource::Dialogue,
                        amount: 1.2,
                    }],
                );
            }
            tracing::info!(
                target: "dao_game::village::herding",
                phase = village.herding.phase.label(),
                "opening herding task completed"
            );
            true
        }
        _ => false,
    }
}

fn sheep_flock_state(ecology: &EcologyState) -> Option<&AnimalFlockState> {
    ecology
        .flocks
        .iter()
        .find(|flock| flock.kind == AnimalKind::SheepHerd)
}

fn sheep_flock_state_mut(ecology: &mut EcologyState) -> Option<&mut AnimalFlockState> {
    ecology
        .flocks
        .iter_mut()
        .find(|flock| flock.kind == AnimalKind::SheepHerd)
}

fn update_herding_state(
    time: Res<Time>,
    village: Option<ResMut<VillageState>>,
    ecology: Option<ResMut<EcologyState>>,
    player_query: Query<&Transform, With<WandererPrototype>>,
    mut notebook: Option<ResMut<NotebookState>>,
    performance: Option<ResMut<FramePerformance>>,
) {
    let (Some(mut village), Some(mut ecology)) = (village, ecology) else {
        return;
    };
    let Some(player_transform) = player_query.iter().next() else {
        return;
    };
    let Some(flock) = sheep_flock_state_mut(&mut ecology) else {
        return;
    };
    let started_at = Instant::now();
    let sheep_pen = village
        .area(VillageAreaKind::SheepPen)
        .map(|area| area.position)
        .unwrap_or(village.herding.return_pen_target);
    if village.herding.return_pen_target == Vec3::ZERO {
        village.herding.return_pen_target = sheep_pen;
    }
    village.herding.flock_center = flock.center;

    let player_to_flock = planar_distance(player_transform.translation, flock.center);
    if !village.herding.player_has_seen_flock && player_to_flock <= HERDING_NEAR_FLOCK_RADIUS {
        village.herding.player_has_seen_flock = true;
        village.herding.task_available = true;
        village.herding.phase = HerdingPhase::Prompted;
        village.herding.phase_started_at = time.elapsed_secs();
        let _ = record_notebook_entry(
            notebook.as_deref_mut(),
            NotebookRecord {
                kind: NotebookEntryKind::Observation,
                at_seconds: time.elapsed_secs(),
                location: Some("羊圈".to_string()),
                source: NotebookSource::Observation,
                title: "羊群在等你".to_string(),
                body: "几只羊抬头看了你一眼，又看向村外有草和风的方向。".to_string(),
                tags: vec![NotebookTag::Village, NotebookTag::Flock],
            },
        );
    }

    match village.herding.phase {
        HerdingPhase::FollowingToGrass => {
            flock.center = player_transform.translation + Vec3::new(-2.8, 0.0, 1.4);
            if planar_distance(
                player_transform.translation,
                village.herding.active_grass_patch,
            ) <= HERDING_GRAZE_RADIUS
            {
                village.herding.phase = HerdingPhase::GrazingAtPatch;
                village.herding.phase_started_at = time.elapsed_secs();
                village.herding.flock_following_player = false;
                village.herding.grass_patch_reached = true;
                flock.behavior = AnimalBehavior::Grazing;
                flock.home = village.herding.active_grass_patch;
                flock.center = village.herding.active_grass_patch;
                flock.radius = 8.8;
                let _ = record_notebook_entry(
                    notebook.as_deref_mut(),
                    NotebookRecord {
                        kind: NotebookEntryKind::Observation,
                        at_seconds: time.elapsed_secs(),
                        location: Some("村外草地".to_string()),
                        source: NotebookSource::Observation,
                        title: "羊群找到草地".to_string(),
                        body: "羊群自己散开去吃草，远处的风声也比村里更开阔。".to_string(),
                        tags: vec![
                            NotebookTag::Village,
                            NotebookTag::Flock,
                            NotebookTag::Memory,
                        ],
                    },
                );
            }
        }
        HerdingPhase::ReturningToPen => {
            flock.center = player_transform.translation + Vec3::new(-2.4, 0.0, 1.3);
            if planar_distance(
                player_transform.translation,
                village.herding.return_pen_target,
            ) <= HERDING_RETURN_RADIUS
            {
                village.herding.pen_returned = true;
                flock.center = village.herding.return_pen_target;
                flock.home = village.herding.return_pen_target;
            }
        }
        HerdingPhase::Completed => {
            flock.behavior = AnimalBehavior::Grazing;
            flock.home = village.herding.return_pen_target;
        }
        HerdingPhase::GrazingAtPatch | HerdingPhase::NotStarted | HerdingPhase::Prompted => {}
    }
    if let Some(mut performance) = performance {
        performance.record_phase_duration(PerformancePhase::Village, started_at.elapsed());
    }
}

fn village_interaction_record(kind: VillageActorKind, at_seconds: f32) -> NotebookRecord {
    match kind {
        VillageActorKind::Sheep => NotebookRecord {
            kind: NotebookEntryKind::Observation,
            at_seconds,
            location: Some("羊圈".to_string()),
            source: NotebookSource::Observation,
            title: "羊群的安静".to_string(),
            body: "羊在木栏边慢慢移动，偶尔抬头看向村外的风。".to_string(),
            tags: vec![NotebookTag::Village, NotebookTag::Flock],
        },
        VillageActorKind::Shepherd => NotebookRecord {
            kind: NotebookEntryKind::Person,
            at_seconds,
            location: Some("羊圈".to_string()),
            source: NotebookSource::Dialogue,
            title: "牧羊人的话".to_string(),
            body: "牧羊人说，羊有时会比人更早听见远方的天气。".to_string(),
            tags: vec![
                NotebookTag::Village,
                NotebookTag::Shepherd,
                NotebookTag::Flock,
            ],
        },
        VillageActorKind::Merchant => NotebookRecord {
            kind: NotebookEntryKind::Person,
            at_seconds,
            location: Some("集市".to_string()),
            source: NotebookSource::Dialogue,
            title: "商人的传闻".to_string(),
            body: "商人提到风沙另一侧有古老的石影，但他说这只是旅人反复带回来的梦话。".to_string(),
            tags: vec![
                NotebookTag::Village,
                NotebookTag::Merchant,
                NotebookTag::Desert,
                NotebookTag::Pyramid,
            ],
        },
    }
}

fn stable_actor_id(index: u64, kind: VillageActorKind) -> u64 {
    let salt = match kind {
        VillageActorKind::Sheep => 11,
        VillageActorKind::Shepherd => 23,
        VillageActorKind::Merchant => 37,
    };
    let mut value = index.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(salt);
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^ (value >> 27)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use bevy::{
        ecs::system::RunSystemOnce,
        prelude::{Vec2, Vec3},
    };

    use crate::{
        core::config::{
            AppConfig, AssetConfig, CameraConfig, DesertConfig, EcologyConfig, EnvironmentConfig,
            PlayerConfig, PresentationConfig, QualityConfig, SignConfig, WorldConfig,
        },
        core::performance::FramePerformance,
        game::{
            environment::{EnvironmentSnapshot, WeatherKind, WindField},
            journey::{DreamPhase, DreamState, JourneyResponseState, JourneyState, StoryArcStage},
            village::{
                ShepherdSchedulePhase, VillageActorKind, VillageAreaKind, VillageAtmosphere,
                VillageCollider, VillageColliderKind, VillageDayPhase, VillageLayoutConfig,
                actor_target_position, build_village_layout, resolve_village_collision,
                shepherd_schedule_position, update_village_atmosphere, village_day_phase,
            },
            world::WorldMap,
        },
    };
    use bevy::prelude::{Res, ResMut};

    #[test]
    fn village_layout_contains_required_areas_and_actors() {
        let config = test_config();
        let world_map = WorldMap::new_for_testing(config.world.seed, &config);
        let village = build_village_layout(
            &world_map,
            Vec3::ZERO,
            VillageLayoutConfig {
                house_count: 5,
                sheep_count: 8,
                radius: 38.0,
            },
        );

        for kind in [
            VillageAreaKind::Houses,
            VillageAreaKind::SheepPen,
            VillageAreaKind::Well,
            VillageAreaKind::Market,
            VillageAreaKind::Shore,
            VillageAreaKind::OuterPath,
        ] {
            assert!(village.areas.iter().any(|area| area.kind == kind));
        }
        assert_eq!(
            village
                .actors
                .iter()
                .filter(|actor| actor.kind == VillageActorKind::Sheep)
                .count(),
            8
        );
        assert!(
            village
                .actors
                .iter()
                .any(|actor| actor.kind == VillageActorKind::Merchant)
        );
        assert!(
            village
                .actors
                .iter()
                .any(|actor| actor.kind == VillageActorKind::Shepherd)
        );
        assert_eq!(village.houses.len(), 5);
        assert!(
            village
                .houses
                .iter()
                .all(|house| house.door_position.distance(house.position) > 1.5)
        );
    }

    #[test]
    fn village_layout_is_deterministic() {
        let config = test_config();
        let world_map = WorldMap::new_for_testing(config.world.seed, &config);
        let first = build_village_layout(&world_map, Vec3::new(2.0, 0.0, -3.0), Default::default());
        let second =
            build_village_layout(&world_map, Vec3::new(2.0, 0.0, -3.0), Default::default());

        assert_eq!(first.origin, second.origin);
        assert_eq!(first.areas, second.areas);
        assert_eq!(first.houses, second.houses);
        assert_eq!(first.actors, second.actors);
    }

    #[test]
    fn village_collision_blocks_house_wall_but_keeps_door_gap_open() {
        let collider = VillageCollider {
            kind: VillageColliderKind::House,
            center: Vec2::ZERO,
            half_extents: Vec2::new(2.5, 2.0),
            yaw: 0.0,
            radius: 3.4,
        };

        let (wall_position, wall_blocked) =
            resolve_village_collision(Vec2::new(2.9, 0.0), Vec2::new(2.2, 0.0), 0.4, [collider]);
        let (door_position, door_blocked) =
            resolve_village_collision(Vec2::new(0.0, -3.0), Vec2::new(0.0, -2.15), 0.4, [collider]);

        assert!(wall_blocked);
        assert!(wall_position.x.abs() >= 2.9);
        assert!(!door_blocked);
        assert_eq!(door_position, Vec2::new(0.0, -2.15));
    }

    #[test]
    fn village_collision_pushes_out_of_actor_radius() {
        let collider = VillageCollider::circle(VillageColliderKind::Actor, Vec2::ZERO, 0.5);
        let (position, blocked) =
            resolve_village_collision(Vec2::new(1.2, 0.0), Vec2::new(0.2, 0.0), 0.4, [collider]);

        assert!(blocked);
        assert!(position.length() >= 0.9);
    }

    #[test]
    fn sheep_actor_motion_stays_near_home() {
        let actor = super::VillageActor {
            id: 1,
            kind: VillageActorKind::Sheep,
            home: Vec3::new(10.0, 0.0, -2.0),
            radius: 6.0,
            seed: 3,
        };
        let target = actor_target_position(&actor, 12.0, None, None, None);

        assert!(target.distance(actor.home) <= actor.radius);
    }

    #[test]
    fn shepherd_schedule_visits_flock_and_village_rest() {
        let home = Vec3::new(10.0, 0.0, -2.0);
        let tending = shepherd_schedule_position(home, 9.0, 2.0);
        let resting = shepherd_schedule_position(home, 9.0, 42.0);

        assert_eq!(tending.phase, ShepherdSchedulePhase::TendingFlock);
        assert!(tending.target.distance(home) < 9.0);
        assert_eq!(resting.phase, ShepherdSchedulePhase::RestingVillage);
        assert!(resting.target.distance(home) > 15.0);
    }

    #[test]
    fn village_day_phase_maps_daylight_bands() {
        assert_eq!(village_day_phase(0.08), VillageDayPhase::Night);
        assert_eq!(village_day_phase(0.24), VillageDayPhase::Dawn);
        assert_eq!(village_day_phase(0.62), VillageDayPhase::Day);
        assert_eq!(village_day_phase(0.88), VillageDayPhase::Dusk);
    }

    #[test]
    fn village_atmosphere_reflects_afterglow_and_wind() {
        let mut app = bevy::prelude::App::new();
        app.insert_resource(EnvironmentSnapshot {
            weather: WeatherKind::Mist,
            daylight: 0.32,
            visibility: 92.0,
            humidity: 0.84,
            fog_density: 0.52,
            cloud_cover: 0.56,
            ambient_energy: 0.78,
            sea_mist: 0.74,
            storm_weight: 0.0,
            sandstorm_weight: 0.0,
            snow_weight: 0.0,
            dawn_warmth: 0.62,
            ground_wetness: 0.68,
            horizon_tension: 0.52,
            boundary_glow: 0.58,
        });
        app.insert_resource(WindField {
            direction: Vec2::new(0.9, -0.2).normalize(),
            raw_speed: 2.2,
            speed: 0.62,
            gust: 0.7,
            swirl: 0.28,
            omen_bias: 0.48,
        });
        app.insert_resource(JourneyState {
            story_stage: StoryArcStage::DreamAfterglow,
            dream: DreamState {
                phase: DreamPhase::Afterglow,
                phase_elapsed: 0.0,
                seen_pyramid: true,
                echo_strength: 0.86,
            },
            response: JourneyResponseState {
                active: true,
                elapsed_seconds: 0.6,
                duration_seconds: 7.5,
                intensity: 0.52,
                place_id: None,
                place_kind: None,
            },
            ..Default::default()
        });
        app.insert_resource(VillageAtmosphere::default());
        app.insert_resource(FramePerformance::default());
        let _ = app.world_mut().run_system_once(
            |environment: Res<EnvironmentSnapshot>,
             wind_field: Res<WindField>,
             journey: Option<Res<JourneyState>>,
             atmosphere: ResMut<VillageAtmosphere>,
             performance: Option<ResMut<FramePerformance>>| {
                update_village_atmosphere(
                    environment,
                    wind_field,
                    journey,
                    atmosphere,
                    performance,
                );
            },
        );

        let atmosphere = app.world().resource::<VillageAtmosphere>();
        assert_eq!(atmosphere.day_phase, VillageDayPhase::Dawn);
        assert!(atmosphere.canopy_sway > 0.4);
        assert!(atmosphere.sea_mist > 0.6);
        assert!(atmosphere.unease > 0.5);
        assert!(atmosphere.warm_window_glow > 0.5);
        assert!(atmosphere.departure_pull > 0.4);
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
                seed: 511,
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
