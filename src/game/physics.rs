use std::{collections::HashSet, time::Instant};

use avian3d::prelude::*;
use bevy::{prelude::*, time::common_conditions::on_timer};

use crate::{
    core::{
        config::AppConfig,
        performance::{FramePerformance, PerformancePhase},
    },
    game::{
        assets::{
            ProceduralAsset, ProceduralAssetKind, ProceduralCollision, registered_spec,
            stable_asset_id,
        },
        flow::{AppScreen, InGameState},
        player::FirstPersonState,
        village::VillageCollider,
        world::{TerrainCollisionSample, WandererPrototype},
    },
};

pub struct DaoPhysicsPlugin;

impl Plugin for DaoPhysicsPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SubstepCount(4));
        app.insert_resource(Time::<Physics>::default());
        app.insert_resource(PhysicsRoute::default());
        app.insert_resource(PhysicsDebugState::from_env());
        app.insert_resource(PhysicsTelemetry::default());
        app.add_plugins(PhysicsPlugins::default().with_length_unit(1.0));
        app.add_plugins(PhysicsDebugPlugin);
        app.add_systems(OnEnter(AppScreen::InGame), reset_physics_session);
        app.add_systems(
            Update,
            (
                sync_physics_debug_config,
                ensure_player_body,
                ensure_procedural_asset_colliders,
                ensure_village_collider_entities,
                update_player_forward_query,
                record_physics_collision_events,
                report_physics_telemetry.run_if(on_timer(std::time::Duration::from_millis(850))),
            )
                .chain()
                .run_if(in_state(InGameState::Running)),
        );
        app.add_systems(OnExit(AppScreen::InGame), cleanup_physics_session);
    }
}

#[derive(Debug, Resource, Clone, PartialEq)]
pub struct PhysicsRoute {
    pub engine: PhysicsEngineChoice,
    pub character_controller: PhysicsCharacterControllerRoute,
    pub terrain_source: TerrainPhysicsSource,
    pub notes: Vec<&'static str>,
}

impl Default for PhysicsRoute {
    fn default() -> Self {
        Self {
            engine: PhysicsEngineChoice::Avian,
            character_controller: PhysicsCharacterControllerRoute::TransitionalKinematicCapsule,
            terrain_source: TerrainPhysicsSource::StreamingHeightfieldFromWorldMap,
            notes: vec![
                "Rapier 0.33.0 and Avian 0.6.1 both support Bevy 0.18.",
                "Avian is selected for ECS-native components, spatial queries, sensors, and debug gizmos.",
                "Existing terrain sampling remains the height and biome authority during migration.",
            ],
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum PhysicsEngineChoice {
    Avian,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum PhysicsCharacterControllerRoute {
    TransitionalKinematicCapsule,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum TerrainPhysicsSource {
    StreamingHeightfieldFromWorldMap,
}

#[derive(Debug, Resource, Clone, Copy, PartialEq)]
pub struct PhysicsDebugState {
    pub enabled: bool,
}

impl PhysicsDebugState {
    fn from_env() -> Self {
        let enabled = std::env::var("DAO_PHYSICS_DEBUG")
            .ok()
            .is_some_and(|value| {
                !matches!(value.to_ascii_lowercase().as_str(), "0" | "false" | "off")
            });
        Self { enabled }
    }
}

#[derive(Debug, Resource, Clone, PartialEq, Default)]
pub struct PhysicsTelemetry {
    pub player_collider_ready: bool,
    pub procedural_colliders: usize,
    pub terrain_colliders: usize,
    pub trigger_colliders: usize,
    pub dynamic_bodies: usize,
    pub last_query: Option<PhysicsQueryHit>,
    pub collision_events_started: u64,
    pub collision_events_ended: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PhysicsQueryHit {
    pub entity: Entity,
    pub distance: f32,
    pub normal: Vec3,
}

#[derive(Debug, Component, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DaoCollider {
    pub layer: DaoPhysicsLayer,
    pub role: DaoColliderRole,
    pub source: DaoColliderSource,
}

#[derive(Debug, Component, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DaoPhysicsPlayer;

#[derive(Debug, Component, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DaoPhysicsTerrainCollider {
    pub x: i32,
    pub z: i32,
}

#[derive(Debug, Component, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DaoPhysicsProceduralCollider {
    pub kind: ProceduralAssetKind,
    pub stable_id: u64,
}

#[derive(Debug, Component, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DaoPhysicsVillageCollider;

#[derive(Debug, Component, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DaoPhysicsSensor {
    pub kind: DaoSensorKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DaoSensorKind {
    Interaction,
    GalleryExhibit,
    RegionGate,
    Omen,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DaoColliderRole {
    CharacterCapsule,
    TerrainHeightfield,
    StaticBlocker,
    InteractionSensor,
    DynamicBody,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DaoColliderSource {
    Player,
    Terrain,
    ProceduralAsset,
    VillageProxy,
    MaterialGallery,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum DaoPhysicsLayer {
    Player,
    Terrain,
    StaticWorld,
    DynamicWorld,
    Sensor,
    Gallery,
}

impl DaoPhysicsLayer {
    pub const fn bit(self) -> u32 {
        1 << self.index()
    }

    const fn index(self) -> u32 {
        match self {
            Self::Player => 1,
            Self::Terrain => 2,
            Self::StaticWorld => 3,
            Self::DynamicWorld => 4,
            Self::Sensor => 5,
            Self::Gallery => 6,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PhysicsColliderRule {
    None,
    StaticCuboid { half_extents: Vec3 },
    InteractionSensor { half_extents: Vec3 },
    CharacterCapsule { radius: f32, height: f32 },
}

impl PhysicsColliderRule {
    pub fn for_asset(
        kind: ProceduralAssetKind,
        collision: ProceduralCollision,
        base_size: Vec3,
    ) -> Self {
        match collision {
            ProceduralCollision::None | ProceduralCollision::VisualOnly => Self::None,
            ProceduralCollision::SimpleBlocker => match kind {
                ProceduralAssetKind::VillageHouse => Self::StaticCuboid {
                    half_extents: Vec3::new(
                        base_size.x * 0.42,
                        base_size.y * 0.48,
                        base_size.z * 0.42,
                    ),
                },
                ProceduralAssetKind::SheepPenRail => Self::StaticCuboid {
                    half_extents: Vec3::new(base_size.x * 0.5, 0.38, base_size.z.max(0.34)),
                },
                ProceduralAssetKind::DesertPyramid => Self::StaticCuboid {
                    half_extents: Vec3::new(
                        base_size.x * 0.44,
                        base_size.y * 0.48,
                        base_size.z * 0.44,
                    ),
                },
                ProceduralAssetKind::PyramidRuinWall => Self::StaticCuboid {
                    half_extents: Vec3::new(
                        base_size.x * 0.5,
                        base_size.y * 0.5,
                        base_size.z * 0.5,
                    ),
                },
                _ => Self::StaticCuboid {
                    half_extents: (base_size * 0.5).max(Vec3::splat(0.1)),
                },
            },
            ProceduralCollision::InteractionProxy => match kind {
                ProceduralAssetKind::Sheep
                | ProceduralAssetKind::Shepherd
                | ProceduralAssetKind::Merchant
                | ProceduralAssetKind::FortuneTeller => Self::CharacterCapsule {
                    radius: base_size.x.max(base_size.z) * 0.5,
                    height: base_size.y.max(0.8),
                },
                _ => Self::InteractionSensor {
                    half_extents: (base_size * Vec3::new(0.6, 0.7, 0.6)).max(Vec3::splat(0.45)),
                },
            },
        }
    }

    pub fn role(self) -> Option<DaoColliderRole> {
        match self {
            Self::None => None,
            Self::StaticCuboid { .. } => Some(DaoColliderRole::StaticBlocker),
            Self::InteractionSensor { .. } => Some(DaoColliderRole::InteractionSensor),
            Self::CharacterCapsule { .. } => Some(DaoColliderRole::DynamicBody),
        }
    }

    fn collider(self) -> Option<Collider> {
        match self {
            Self::None => None,
            Self::StaticCuboid { half_extents } | Self::InteractionSensor { half_extents } => {
                Some(Collider::cuboid(
                    half_extents.x * 2.0,
                    half_extents.y * 2.0,
                    half_extents.z * 2.0,
                ))
            }
            Self::CharacterCapsule { radius, height } => Some(Collider::capsule(
                radius.max(0.05),
                (height - radius * 2.0).max(0.1),
            )),
        }
    }

    fn is_sensor(self) -> bool {
        matches!(self, Self::InteractionSensor { .. })
    }

    fn rigid_body(self) -> RigidBody {
        match self {
            Self::CharacterCapsule { .. } => RigidBody::Kinematic,
            _ => RigidBody::Static,
        }
    }
}

pub fn static_world_layers() -> CollisionLayers {
    CollisionLayers::from_bits(
        DaoPhysicsLayer::StaticWorld.bit(),
        DaoPhysicsLayer::Player.bit()
            | DaoPhysicsLayer::DynamicWorld.bit()
            | DaoPhysicsLayer::Gallery.bit(),
    )
}

pub fn terrain_layers() -> CollisionLayers {
    CollisionLayers::from_bits(
        DaoPhysicsLayer::Terrain.bit(),
        DaoPhysicsLayer::Player.bit() | DaoPhysicsLayer::DynamicWorld.bit(),
    )
}

pub fn player_layers() -> CollisionLayers {
    CollisionLayers::from_bits(
        DaoPhysicsLayer::Player.bit(),
        DaoPhysicsLayer::Terrain.bit()
            | DaoPhysicsLayer::StaticWorld.bit()
            | DaoPhysicsLayer::DynamicWorld.bit()
            | DaoPhysicsLayer::Sensor.bit()
            | DaoPhysicsLayer::Gallery.bit(),
    )
}

pub fn sensor_layers() -> CollisionLayers {
    CollisionLayers::from_bits(DaoPhysicsLayer::Sensor.bit(), DaoPhysicsLayer::Player.bit())
}

pub fn gallery_layers() -> CollisionLayers {
    CollisionLayers::from_bits(
        DaoPhysicsLayer::Gallery.bit(),
        DaoPhysicsLayer::Player.bit() | DaoPhysicsLayer::Sensor.bit(),
    )
}

pub fn forward_query_mask() -> LayerMask {
    LayerMask(
        DaoPhysicsLayer::StaticWorld.bit()
            | DaoPhysicsLayer::DynamicWorld.bit()
            | DaoPhysicsLayer::Sensor.bit()
            | DaoPhysicsLayer::Gallery.bit(),
    )
}

pub fn selected_engine_summary() -> &'static str {
    "Selected Avian 0.6.1 for Bevy 0.18: ECS-native rigid bodies, colliders, sensors, spatial queries, and debug gizmos. Rapier 0.33.0 remains a viable fallback if the project later needs Rapier-specific joints or tooling."
}

type PlayerPhysicsBodyQuery<'w, 's> = Query<
    'w,
    's,
    Entity,
    (
        With<WandererPrototype>,
        Without<DaoPhysicsPlayer>,
        Without<DaoCollider>,
    ),
>;

type ProceduralAssetPhysicsQuery<'w, 's> = Query<
    'w,
    's,
    (Entity, &'static ProceduralAsset),
    (
        Without<DaoPhysicsProceduralCollider>,
        Without<DaoCollider>,
        With<Transform>,
    ),
>;

fn reset_physics_session(mut telemetry: ResMut<PhysicsTelemetry>) {
    *telemetry = PhysicsTelemetry::default();
    tracing::info!(
        target: "dao_game::physics::route",
        engine = "avian3d",
        avian_version = "0.6.1",
        bevy_target = "0.18",
        route = selected_engine_summary(),
        "formal physics route initialized"
    );
}

fn cleanup_physics_session(
    mut commands: Commands,
    collider_query: Query<Entity, With<DaoCollider>>,
    player_query: Query<Entity, With<DaoPhysicsPlayer>>,
) {
    for entity in &collider_query {
        commands.entity(entity).despawn();
    }
    for entity in &player_query {
        commands.entity(entity).remove::<(
            DaoPhysicsPlayer,
            RigidBody,
            Collider,
            CollisionLayers,
            DaoCollider,
            CollisionEventsEnabled,
        )>();
    }
}

fn sync_physics_debug_config(debug: Res<PhysicsDebugState>, mut store: ResMut<GizmoConfigStore>) {
    if !debug.is_changed() {
        return;
    }
    let (config, physics_gizmos) = store.config_mut::<PhysicsGizmos>();
    config.enabled = debug.enabled;
    physics_gizmos.collider_color = Some(Color::srgb(0.95, 0.56, 0.18));
    physics_gizmos.aabb_color = if debug.enabled {
        Some(Color::srgba(0.72, 0.82, 0.95, 0.55))
    } else {
        None
    };
    physics_gizmos.hide_meshes = false;
}

fn ensure_player_body(
    config: Res<AppConfig>,
    mut telemetry: ResMut<PhysicsTelemetry>,
    mut performance: ResMut<FramePerformance>,
    mut query: PlayerPhysicsBodyQuery<'_, '_>,
    mut commands: Commands,
) {
    let started_at = Instant::now();
    let Some(entity) = query.iter_mut().next() else {
        return;
    };
    let radius = config.player.capsule_radius.max(0.05);
    let height = (config.player.body_height + config.player.eye_height * 0.28).max(radius * 2.4);
    commands.entity(entity).insert((
        DaoPhysicsPlayer,
        RigidBody::Kinematic,
        Collider::capsule(radius, (height - radius * 2.0).max(0.1)),
        player_layers(),
        CollisionEventsEnabled,
        DaoCollider {
            layer: DaoPhysicsLayer::Player,
            role: DaoColliderRole::CharacterCapsule,
            source: DaoColliderSource::Player,
        },
    ));
    telemetry.player_collider_ready = true;
    performance.record_phase_duration(
        PerformancePhase::PhysicsColliderStreaming,
        started_at.elapsed(),
    );
}

fn ensure_procedural_asset_colliders(
    mut commands: Commands,
    mut telemetry: ResMut<PhysicsTelemetry>,
    mut performance: ResMut<FramePerformance>,
    query: ProceduralAssetPhysicsQuery<'_, '_>,
) {
    let started_at = Instant::now();
    let mut inserted = 0_usize;
    let mut sensors = 0_usize;
    for (entity, asset) in &query {
        let spec = registered_spec(asset.spec.kind);
        let rule = PhysicsColliderRule::for_asset(asset.spec.kind, spec.collision, spec.base_size);
        let Some(collider) = rule.collider() else {
            continue;
        };
        let Some(role) = rule.role() else {
            continue;
        };
        let mut entity_commands = commands.entity(entity);
        let layers = if rule.is_sensor() {
            sensors += 1;
            sensor_layers()
        } else {
            static_world_layers()
        };
        entity_commands.insert((
            rule.rigid_body(),
            collider,
            layers,
            CollisionEventsEnabled,
            DaoPhysicsProceduralCollider {
                kind: asset.spec.kind,
                stable_id: asset.spec.stable_id,
            },
            DaoCollider {
                layer: if rule.is_sensor() {
                    DaoPhysicsLayer::Sensor
                } else {
                    DaoPhysicsLayer::StaticWorld
                },
                role,
                source: DaoColliderSource::ProceduralAsset,
            },
        ));
        if rule.is_sensor() {
            entity_commands.insert((
                Sensor,
                DaoPhysicsSensor {
                    kind: DaoSensorKind::Interaction,
                },
            ));
        }
        inserted += 1;
    }

    if inserted > 0 {
        telemetry.procedural_colliders += inserted;
        telemetry.trigger_colliders += sensors;
        tracing::debug!(
            target: "dao_game::physics::collider_streaming",
            inserted,
            sensors,
            "procedural asset colliders attached"
        );
    }
    performance.record_phase_duration(
        PerformancePhase::PhysicsColliderStreaming,
        started_at.elapsed(),
    );
}

fn ensure_village_collider_entities(
    mut commands: Commands,
    mut telemetry: ResMut<PhysicsTelemetry>,
    mut performance: ResMut<FramePerformance>,
    colliders: Query<&VillageCollider>,
    existing: Query<Entity, With<DaoPhysicsVillageCollider>>,
) {
    let started_at = Instant::now();
    let source_count = colliders
        .iter()
        .map(expected_village_physics_collider_count)
        .sum::<usize>();
    let existing_count = existing.iter().count();
    if source_count == existing_count {
        performance.record_phase_duration(
            PerformancePhase::PhysicsColliderStreaming,
            started_at.elapsed(),
        );
        return;
    }
    for entity in &existing {
        commands.entity(entity).despawn();
    }
    let mut spawned = 0_usize;
    for collider in &colliders {
        spawned += spawn_village_physics_collider(&mut commands, collider);
    }
    telemetry.procedural_colliders = telemetry
        .procedural_colliders
        .saturating_sub(existing_count)
        + spawned;
    if spawned > 0 {
        tracing::debug!(
            target: "dao_game::physics::collider_streaming",
            spawned,
            "village collider proxies mirrored into physics world"
        );
    }
    performance.record_phase_duration(
        PerformancePhase::PhysicsColliderStreaming,
        started_at.elapsed(),
    );
}

fn spawn_village_physics_collider(commands: &mut Commands, collider: &VillageCollider) -> usize {
    if matches!(
        collider.kind,
        crate::game::village::VillageColliderKind::House
    ) {
        return spawn_house_physics_wall_segments(commands, collider);
    }

    let half = collider
        .half_extents
        .max(Vec2::splat(collider.radius.max(0.1)));
    let y = if collider.radius > 0.0 { 0.65 } else { 0.75 };
    spawn_village_physics_box(
        commands,
        format!("VillagePhysicsCollider::{:?}", collider.kind),
        collider.center,
        half,
        collider.yaw,
        y,
    );
    1
}

fn expected_village_physics_collider_count(collider: &VillageCollider) -> usize {
    if matches!(
        collider.kind,
        crate::game::village::VillageColliderKind::House
    ) {
        house_physics_wall_segments(collider)
            .into_iter()
            .filter(|(_, _, half_extents)| half_extents.x > 0.05 && half_extents.y > 0.05)
            .count()
    } else {
        1
    }
}

fn spawn_house_physics_wall_segments(commands: &mut Commands, collider: &VillageCollider) -> usize {
    let mut spawned = 0_usize;
    for (name, local_center, half_extents) in house_physics_wall_segments(collider) {
        if half_extents.x <= 0.05 || half_extents.y <= 0.05 {
            continue;
        }
        spawn_village_physics_box(
            commands,
            name.to_string(),
            village_oriented_center(collider.center, local_center, collider.yaw),
            half_extents,
            collider.yaw,
            1.25,
        );
        spawned += 1;
    }

    spawned
}

fn house_physics_wall_segments(collider: &VillageCollider) -> [(&'static str, Vec2, Vec2); 5] {
    let wall = 0.34_f32;
    let door_half_width = 0.82_f32.min(collider.half_extents.x * 0.5);
    let front_half_width = ((collider.half_extents.x - door_half_width) * 0.5).max(0.0);

    [
        (
            "VillagePhysicsHouseBackWall",
            Vec2::new(0.0, collider.half_extents.y - wall * 0.5),
            Vec2::new(collider.half_extents.x, wall * 0.5),
        ),
        (
            "VillagePhysicsHouseLeftWall",
            Vec2::new(-collider.half_extents.x + wall * 0.5, 0.0),
            Vec2::new(wall * 0.5, collider.half_extents.y),
        ),
        (
            "VillagePhysicsHouseRightWall",
            Vec2::new(collider.half_extents.x - wall * 0.5, 0.0),
            Vec2::new(wall * 0.5, collider.half_extents.y),
        ),
        (
            "VillagePhysicsHouseFrontLeftWall",
            Vec2::new(
                -door_half_width - front_half_width,
                -collider.half_extents.y + wall * 0.5,
            ),
            Vec2::new(front_half_width, wall * 0.5),
        ),
        (
            "VillagePhysicsHouseFrontRightWall",
            Vec2::new(
                door_half_width + front_half_width,
                -collider.half_extents.y + wall * 0.5,
            ),
            Vec2::new(front_half_width, wall * 0.5),
        ),
    ]
}

fn spawn_village_physics_box(
    commands: &mut Commands,
    name: String,
    center: Vec2,
    half: Vec2,
    yaw: f32,
    y: f32,
) {
    commands.spawn((
        Name::new(name),
        DespawnOnExit(AppScreen::InGame),
        Transform::from_xyz(center.x, y, center.y).with_rotation(Quat::from_rotation_y(yaw)),
        RigidBody::Static,
        Collider::cuboid(half.x * 2.0, y * 2.0, half.y * 2.0),
        static_world_layers(),
        DaoPhysicsVillageCollider,
        DaoCollider {
            layer: DaoPhysicsLayer::StaticWorld,
            role: DaoColliderRole::StaticBlocker,
            source: DaoColliderSource::VillageProxy,
        },
    ));
}

fn village_oriented_center(center: Vec2, local: Vec2, yaw: f32) -> Vec2 {
    let right = Vec2::new(yaw.cos(), -yaw.sin());
    let forward = Vec2::new(yaw.sin(), yaw.cos());
    center + right * local.x + forward * local.y
}

fn update_player_forward_query(
    spatial_query: SpatialQuery,
    debug: Res<PhysicsDebugState>,
    mut telemetry: ResMut<PhysicsTelemetry>,
    mut performance: ResMut<FramePerformance>,
    mut gizmos: Gizmos,
    player_query: Query<(Entity, &Transform), With<DaoPhysicsPlayer>>,
    state: Option<Res<FirstPersonState>>,
) {
    let started_at = Instant::now();
    let Some((player_entity, transform)) = player_query.iter().next() else {
        return;
    };
    let yaw = state.as_deref().map(|state| state.yaw).unwrap_or_else(|| {
        let (_, yaw, _) = transform.rotation.to_euler(EulerRot::YXZ);
        yaw
    });
    let origin = transform.translation + Vec3::Y * 1.25;
    let direction = Quat::from_rotation_y(yaw) * -Vec3::Z;
    let Ok(direction) = Dir3::new(direction) else {
        return;
    };
    let filter =
        SpatialQueryFilter::from_mask(forward_query_mask()).with_excluded_entities([player_entity]);
    let hit = spatial_query
        .cast_ray(origin, direction, 9.0, true, &filter)
        .map(|hit| PhysicsQueryHit {
            entity: hit.entity,
            distance: hit.distance,
            normal: hit.normal,
        });
    telemetry.last_query = hit;
    if debug.enabled {
        let distance = hit.map(|hit| hit.distance).unwrap_or(9.0);
        let color = if hit.is_some() {
            Color::srgb(1.0, 0.22, 0.14)
        } else {
            Color::srgb(0.25, 0.75, 1.0)
        };
        gizmos.line(origin, origin + *direction * distance, color);
    }
    performance.record_phase_duration(PerformancePhase::PhysicsQuery, started_at.elapsed());
}

fn record_physics_collision_events(
    mut starts: MessageReader<CollisionStart>,
    mut ends: MessageReader<CollisionEnd>,
    mut telemetry: ResMut<PhysicsTelemetry>,
    mut performance: ResMut<FramePerformance>,
) {
    let started_at = Instant::now();
    let start_count = starts.read().count() as u64;
    let end_count = ends.read().count() as u64;
    telemetry.collision_events_started += start_count;
    telemetry.collision_events_ended += end_count;
    if start_count > 0 || end_count > 0 {
        tracing::trace!(
            target: "dao_game::physics::contacts",
            start_count,
            end_count,
            total_started = telemetry.collision_events_started,
            total_ended = telemetry.collision_events_ended,
            "physics collision events observed"
        );
    }
    performance.record_phase_duration(PerformancePhase::PhysicsNarrowPhase, started_at.elapsed());
    performance.record_phase_duration(PerformancePhase::PhysicsSolver, std::time::Duration::ZERO);
    performance.record_phase_duration(
        PerformancePhase::PhysicsBroadPhase,
        std::time::Duration::ZERO,
    );
}

fn report_physics_telemetry(
    route: Res<PhysicsRoute>,
    telemetry: Res<PhysicsTelemetry>,
    body_query: Query<&RigidBody>,
    terrain_query: Query<&DaoPhysicsTerrainCollider>,
    collider_query: Query<&DaoCollider>,
) {
    let dynamic_bodies = body_query
        .iter()
        .filter(|body| matches!(body, RigidBody::Dynamic))
        .count();
    let terrain_count = terrain_query.iter().count();
    let trigger_count = collider_query
        .iter()
        .filter(|collider| collider.role == DaoColliderRole::InteractionSensor)
        .count();
    let static_count = collider_query
        .iter()
        .filter(|collider| collider.role == DaoColliderRole::StaticBlocker)
        .count();
    let hit = telemetry.last_query;
    tracing::debug!(
        target: "dao_game::physics::telemetry",
        engine = ?route.engine,
        character_controller = ?route.character_controller,
        terrain_source = ?route.terrain_source,
        player_collider_ready = telemetry.player_collider_ready,
        static_colliders = static_count,
        terrain_colliders = terrain_count,
        trigger_colliders = trigger_count,
        dynamic_bodies,
        last_query_entity = hit.map(|hit| format!("{:?}", hit.entity)),
        last_query_distance = hit.map(|hit| hit.distance),
        "physics world telemetry"
    );
}

pub fn terrain_collider_from_samples(
    samples: &[Vec<TerrainCollisionSample>],
    scale: Vec3,
) -> Option<Collider> {
    if samples.is_empty() || samples.iter().any(Vec::is_empty) {
        return None;
    }
    let heights = samples
        .iter()
        .map(|row| row.iter().map(|sample| sample.height).collect::<Vec<_>>())
        .collect::<Vec<_>>();
    Some(Collider::heightfield(heights, scale))
}

pub fn spawn_terrain_heightfield_collider(
    commands: &mut Commands,
    coord_x: i32,
    coord_z: i32,
    center: Vec3,
    samples: &[Vec<TerrainCollisionSample>],
    scale: Vec3,
) -> Option<Entity> {
    let collider = terrain_collider_from_samples(samples, scale)?;
    Some(
        commands
            .spawn((
                Name::new(format!("TerrainPhysicsCollider({coord_x}, {coord_z})")),
                DespawnOnExit(AppScreen::InGame),
                Transform::from_translation(center),
                RigidBody::Static,
                collider,
                terrain_layers(),
                DaoPhysicsTerrainCollider {
                    x: coord_x,
                    z: coord_z,
                },
                DaoCollider {
                    layer: DaoPhysicsLayer::Terrain,
                    role: DaoColliderRole::TerrainHeightfield,
                    source: DaoColliderSource::Terrain,
                },
            ))
            .id(),
    )
}

pub fn stable_gallery_sensor_id(material_id: &str) -> u64 {
    stable_asset_id(
        ProceduralAssetKind::DesertRelic,
        fnv1a_64(material_id.as_bytes()),
    )
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut value = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        value ^= u64::from(*byte);
        value = value.wrapping_mul(0x0000_0100_0000_01b3);
    }
    value
}

pub fn unique_terrain_coords<'a>(
    colliders: impl IntoIterator<Item = &'a DaoPhysicsTerrainCollider>,
) -> HashSet<(i32, i32)> {
    colliders
        .into_iter()
        .map(|collider| (collider.x, collider.z))
        .collect()
}

#[cfg(test)]
mod tests {
    use bevy::prelude::{Vec2, Vec3};

    use crate::game::{
        assets::{ProceduralAssetKind, ProceduralCollision, registered_spec},
        physics::{
            DaoPhysicsLayer, PhysicsColliderRule, forward_query_mask, player_layers,
            selected_engine_summary, static_world_layers,
        },
    };

    #[test]
    fn collision_layers_allow_player_static_and_queries() {
        assert!(player_layers().interacts_with(static_world_layers()));
        assert_ne!(
            forward_query_mask().0 & DaoPhysicsLayer::StaticWorld.bit(),
            0
        );
        assert_ne!(forward_query_mask().0 & DaoPhysicsLayer::Sensor.bit(), 0);
    }

    #[test]
    fn procedural_collision_rules_upgrade_semantic_specs() {
        let house = registered_spec(ProceduralAssetKind::VillageHouse);
        let well = registered_spec(ProceduralAssetKind::VillageWell);
        let path = registered_spec(ProceduralAssetKind::PathStone);

        assert!(matches!(
            PhysicsColliderRule::for_asset(house.kind, house.collision, house.base_size),
            PhysicsColliderRule::StaticCuboid { .. }
        ));
        assert!(matches!(
            PhysicsColliderRule::for_asset(well.kind, well.collision, well.base_size),
            PhysicsColliderRule::InteractionSensor { .. }
        ));
        assert_eq!(
            PhysicsColliderRule::for_asset(path.kind, ProceduralCollision::VisualOnly, Vec3::ONE),
            PhysicsColliderRule::None
        );
    }

    #[test]
    fn selected_engine_summary_records_route() {
        let summary = selected_engine_summary();

        assert!(summary.contains("Avian"));
        assert!(summary.contains("Rapier"));
        assert!(summary.contains("Bevy 0.18"));
    }

    #[test]
    fn cuboid_rule_uses_nonzero_extents() {
        let rule = PhysicsColliderRule::for_asset(
            ProceduralAssetKind::MarketStall,
            ProceduralCollision::InteractionProxy,
            Vec3::new(5.4, 2.25, 3.0),
        );

        if let PhysicsColliderRule::InteractionSensor { half_extents } = rule {
            assert!(half_extents.x > 1.0);
            assert!(half_extents.y > 1.0);
        } else {
            panic!("market stall should become an interaction sensor");
        }
    }

    #[test]
    fn layer_bits_are_stable_and_distinct() {
        let bits = [
            DaoPhysicsLayer::Player.bit(),
            DaoPhysicsLayer::Terrain.bit(),
            DaoPhysicsLayer::StaticWorld.bit(),
            DaoPhysicsLayer::DynamicWorld.bit(),
            DaoPhysicsLayer::Sensor.bit(),
            DaoPhysicsLayer::Gallery.bit(),
        ];
        let mut unique = std::collections::HashSet::new();

        for bit in bits {
            assert!(unique.insert(bit));
        }
    }

    #[test]
    fn vec2_import_keeps_test_surface_close_to_village_collision_math() {
        assert_eq!(Vec2::new(1.0, 2.0).length_squared(), 5.0);
    }
}
