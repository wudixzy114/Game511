use bevy::prelude::*;

use crate::game::{
    assets::{
        ProceduralAnimationRole, ProceduralAssetKind, ProceduralAssetLod, ProceduralAssetMaterials,
        ProceduralSpawnRequest, spawn_procedural_asset, spawn_procedural_asset_entity,
    },
    flow::{AppScreen, InGameState},
    intent::{IntentState, apply_village_dialogue_intent},
    notebook::{
        NotebookEntryKind, NotebookRecord, NotebookSource, NotebookState, NotebookTag,
        record_notebook_entry,
    },
    places::planar_distance,
    world::{WandererPrototype, WorldCamera, WorldMap, WorldShowcaseSpots},
};

pub struct VillagePlugin;

type VillageInitQueries<'w, 's> = (
    Query<'w, 's, &'static mut Transform, With<WandererPrototype>>,
    Query<'w, 's, &'static mut Transform, (With<WorldCamera>, Without<WandererPrototype>)>,
);

type VillageInitAssets<'w> = (ResMut<'w, Assets<Mesh>>, Res<'w, ProceduralAssetMaterials>);

impl Plugin for VillagePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                initialize_village_session,
                update_village_actor_behavior,
                animate_village_asset_parts,
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
    pub actors: Vec<VillageActorState>,
    pub nearest_actor: Option<VillageActorSnapshot>,
    pub interaction_prompt: Option<String>,
    pub player_was_bootstrapped: bool,
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
pub struct VillageActorState {
    pub id: u64,
    pub kind: VillageActorKind,
    pub home: Vec3,
    pub radius: f32,
    pub behavior: VillageBehavior,
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
    role: ProceduralAnimationRole,
    base_translation: Vec3,
    base_rotation: Quat,
    base_scale: Vec3,
}

#[derive(Debug, Component)]
struct VillageVisual;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VillageLayoutConfig {
    pub house_count: usize,
    pub sheep_count: usize,
    pub radius: f32,
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

fn initialize_village_session(
    mut commands: Commands,
    world_map: Option<Res<WorldMap>>,
    spots: Option<Res<WorldShowcaseSpots>>,
    village: Option<ResMut<VillageState>>,
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
    spawn_village_visuals(&mut commands, &mut assets.0, &assets.1, &layout, &world_map);
    spawn_village_actors(&mut commands, &mut assets.0, &assets.1, &layout, &world_map);

    tracing::info!(
        target: "dao_game::village::generation",
        origin_x = layout.origin.x,
        origin_z = layout.origin.z,
        actor_count = layout.actors.len(),
        area_count = layout.areas.len(),
        "opening village generated"
    );

    let mut state = VillageState {
        origin: layout.origin,
        spawn_point: layout.spawn_point,
        areas: layout.areas,
        actors: layout.actors,
        nearest_actor: None,
        interaction_prompt: None,
        player_was_bootstrapped: false,
    };
    bootstrap_player_to_village(&world_map, &mut state, &mut queries.0, &mut queries.1);
    commands.insert_resource(state);
}

fn cleanup_village_session(mut commands: Commands) {
    commands.remove_resource::<VillageState>();
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
    houses: Vec<Vec3>,
}

pub fn build_village_layout(
    world_map: &WorldMap,
    origin: Vec3,
    config: VillageLayoutConfig,
) -> VillageState {
    let layout = build_layout_internal(world_map, origin, config);
    VillageState {
        origin: layout.origin,
        spawn_point: layout.spawn_point,
        areas: layout.areas,
        actors: layout.actors,
        nearest_actor: None,
        interaction_prompt: None,
        player_was_bootstrapped: false,
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
            ground_position(
                world_map,
                origin + Vec3::new(angle.cos() * radius, 0.0, angle.sin() * radius),
                0.0,
            )
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

fn spawn_village_visuals(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &ProceduralAssetMaterials,
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
            let local = *house - layout.origin;
            spawn_house(parent, meshes, materials, local, index);
        }
        for area in &layout.areas {
            let local = area.position - layout.origin;
            match area.kind {
                VillageAreaKind::Well => spawn_well(parent, meshes, materials, local),
                VillageAreaKind::SheepPen => spawn_sheep_pen(parent, meshes, materials, local),
                VillageAreaKind::Market => spawn_market(parent, meshes, materials, local),
                VillageAreaKind::Shore => spawn_shore(parent, meshes, materials, local, world_map),
                VillageAreaKind::OuterPath => spawn_path_marker(parent, meshes, materials, local),
                VillageAreaKind::Houses => {}
            }
        }
    });
    tag_village_ambient_parts(commands, root);
}

fn spawn_house(
    parent: &mut ChildSpawnerCommands<'_>,
    meshes: &mut Assets<Mesh>,
    materials: &ProceduralAssetMaterials,
    position: Vec3,
    index: usize,
) {
    let yaw = index as f32 * 0.43;
    spawn_procedural_asset(
        parent,
        meshes,
        materials,
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
    position: Vec3,
) {
    spawn_procedural_asset(
        parent,
        meshes,
        materials,
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
    position: Vec3,
) {
    spawn_procedural_asset(
        parent,
        meshes,
        materials,
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
    position: Vec3,
    _world_map: &WorldMap,
) {
    spawn_procedural_asset(
        parent,
        meshes,
        materials,
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
    position: Vec3,
) {
    for index in 0..5 {
        spawn_procedural_asset(
            parent,
            meshes,
            materials,
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
                tag_village_actor_parts(commands, entity, actor.id);
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
                tag_village_actor_parts(commands, entity, actor.id);
            }
        }
    }
}

fn tag_village_actor_parts(commands: &mut Commands, root: Entity, actor_id: u64) {
    commands.queue(move |world: &mut World| {
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
            let Some(part_children) = world
                .get::<Children>(asset_root)
                .map(|children| children.iter().collect::<Vec<_>>())
            else {
                continue;
            };
            for child in part_children {
                let Some(role) = world.get::<ProceduralAnimationRole>(child).copied() else {
                    continue;
                };
                let Some(transform) = world.get::<Transform>(child).copied() else {
                    continue;
                };
                world.entity_mut(child).insert(VillageAnimatedPart {
                    actor_id: None,
                    role,
                    base_translation: transform.translation,
                    base_rotation: transform.rotation,
                    base_scale: transform.scale,
                });
            }
        }
    });
}

fn update_village_actor_behavior(
    time: Res<Time>,
    world_map: Option<Res<WorldMap>>,
    player_query: Query<&Transform, With<WandererPrototype>>,
    mut actor_query: Query<(&VillageActor, &mut Transform), Without<WandererPrototype>>,
) {
    let Some(world_map) = world_map else {
        return;
    };
    let player_position = player_query
        .iter()
        .next()
        .map(|transform| transform.translation);
    for (actor, mut transform) in &mut actor_query {
        let target = actor_target_position(actor, time.elapsed_secs());
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
}

fn animate_village_asset_parts(
    time: Res<Time>,
    actor_query: Query<&VillageActor>,
    mut part_query: Query<(&VillageAnimatedPart, &mut Transform)>,
) {
    let elapsed = time.elapsed_secs();
    for (part, mut transform) in &mut part_query {
        let actor = part
            .actor_id
            .and_then(|id| actor_query.iter().find(|actor| actor.id == id));
        let phase_seed = part.actor_id.unwrap_or(17) as f32 * 0.013;
        let phase = elapsed + phase_seed;
        transform.translation = part.base_translation;
        transform.rotation = part.base_rotation;
        transform.scale = part.base_scale;

        match part.role {
            ProceduralAnimationRole::SheepHead => {
                let grazing = actor.is_some_and(|actor| actor.kind == VillageActorKind::Sheep);
                let nod = if grazing {
                    (phase * 1.7).sin().max(0.0) * 0.32
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
                transform.translation.y += (phase * 1.4).sin() * 0.035;
            }
            ProceduralAnimationRole::NpcHandRight => {
                transform.translation.y += (phase * 1.2 + 0.6).sin() * 0.05;
            }
            ProceduralAnimationRole::ClothCanopy => {
                let flutter = (phase * 1.8).sin() * 0.06;
                transform.rotation = part.base_rotation * Quat::from_rotation_x(flutter);
                transform.translation.y += flutter.abs() * 0.08;
            }
            ProceduralAnimationRole::Smoke => {
                let drift = Vec3::new((phase * 0.7).sin() * 0.08, (phase * 0.33).sin() * 0.04, 0.0);
                transform.translation = part.base_translation + drift;
                transform.scale = part.base_scale * (1.0 + (phase * 0.9).sin().abs() * 0.18);
            }
            ProceduralAnimationRole::WaterRipple => {
                let pulse = 1.0 + (phase * 1.15).sin() * 0.025;
                transform.scale = Vec3::new(
                    part.base_scale.x * pulse,
                    part.base_scale.y,
                    part.base_scale.z,
                );
            }
            ProceduralAnimationRole::BirdLeftWing
            | ProceduralAnimationRole::BirdRightWing
            | ProceduralAnimationRole::FishTail => {}
        }
    }
}

fn actor_target_position(actor: &VillageActor, elapsed: f32) -> Vec3 {
    if actor.kind == VillageActorKind::Shepherd {
        return shepherd_schedule_position(actor.home, actor.radius, elapsed).target;
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
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    village: Option<ResMut<VillageState>>,
    player_query: Query<&Transform, With<WandererPrototype>>,
    actor_query: Query<(&VillageActor, &Transform), Without<WandererPrototype>>,
    mut intent: Option<ResMut<IntentState>>,
    mut notebook: Option<ResMut<NotebookState>>,
) {
    let Some(mut village) = village else {
        return;
    };
    let Some(player_transform) = player_query.iter().next() else {
        return;
    };

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
    village.interaction_prompt = nearest.map(|actor| actor.kind.prompt().to_string());

    if !keys.just_pressed(KeyCode::KeyF) {
        return;
    }

    let Some(actor) = nearest else {
        return;
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

    use bevy::prelude::Vec3;

    use crate::{
        core::config::{
            AppConfig, AssetConfig, CameraConfig, DesertConfig, EcologyConfig, EnvironmentConfig,
            PlayerConfig, PresentationConfig, QualityConfig, SignConfig, WorldConfig,
        },
        game::{
            village::{
                ShepherdSchedulePhase, VillageActorKind, VillageAreaKind, VillageLayoutConfig,
                actor_target_position, build_village_layout, shepherd_schedule_position,
            },
            world::WorldMap,
        },
    };

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
        assert_eq!(first.actors, second.actors);
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
        let target = actor_target_position(&actor, 12.0);

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
