use bevy::{
    color::LinearRgba,
    light::NotShadowCaster,
    math::primitives::{Capsule3d, Cylinder, Sphere, Torus},
    pbr::MeshMaterial3d,
    prelude::*,
};

use crate::game::{
    flow::{AppScreen, InGameState},
    world::{
        BiomeKind, TerrainTile, WandererPrototype, WorldGridCoord, WorldMap, WorldShowcaseSpots,
    },
};

pub struct PlacesPlugin;

impl Plugin for PlacesPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (initialize_meaningful_places, update_place_proximity)
                .chain()
                .run_if(in_state(InGameState::Running)),
        );
        app.add_systems(OnExit(AppScreen::InGame), cleanup_places_session);
    }
}

#[derive(Debug, Resource, Clone, PartialEq)]
pub struct MeaningfulPlaces {
    pub places: Vec<MeaningfulPlace>,
    pub active_place_id: Option<u64>,
    pub nearest_place_id: Option<u64>,
    pub nearest_distance: Option<f32>,
}

impl MeaningfulPlaces {
    pub fn active_place(&self) -> Option<&MeaningfulPlace> {
        self.active_place_id
            .and_then(|id| self.places.iter().find(|place| place.id == id))
    }

    pub fn nearest_place(&self) -> Option<&MeaningfulPlace> {
        self.nearest_place_id
            .and_then(|id| self.places.iter().find(|place| place.id == id))
    }

    pub fn place_by_id(&self, id: u64) -> Option<&MeaningfulPlace> {
        self.places.iter().find(|place| place.id == id)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct MeaningfulPlace {
    pub id: u64,
    pub kind: PlaceKind,
    pub grid: WorldGridCoord,
    pub position: Vec3,
    pub biome: BiomeKind,
    pub tags: Vec<PlaceTag>,
    pub score: f32,
    pub arrival_radius: f32,
    pub interaction_radius: f32,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum PlaceKind {
    AncientTree,
    SpringEye,
    RidgeGate,
    QuietBay,
    StoneRing,
}

impl PlaceKind {
    pub const ALL: [Self; 5] = [
        Self::AncientTree,
        Self::SpringEye,
        Self::RidgeGate,
        Self::QuietBay,
        Self::StoneRing,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::AncientTree => "古树",
            Self::SpringEye => "泉眼",
            Self::RidgeGate => "山脊门",
            Self::QuietBay => "静水湾",
            Self::StoneRing => "石阵",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum PlaceTag {
    Shelter,
    Water,
    Threshold,
    Stillness,
    Memory,
    Height,
    Grove,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PlaceSearchConfig {
    pub search_radius_tiles: i32,
    pub step_tiles: usize,
    pub min_distance: f32,
    pub max_distance: f32,
    pub ideal_distance: f32,
    pub max_places: usize,
}

impl Default for PlaceSearchConfig {
    fn default() -> Self {
        Self {
            search_radius_tiles: 56,
            step_tiles: 2,
            min_distance: 42.0,
            max_distance: 190.0,
            ideal_distance: 98.0,
            max_places: 5,
        }
    }
}

#[derive(Debug, Component)]
struct MeaningfulPlaceEntity;

fn initialize_meaningful_places(
    mut commands: Commands,
    world_map: Option<Res<WorldMap>>,
    spots: Option<Res<WorldShowcaseSpots>>,
    existing_places: Option<Res<MeaningfulPlaces>>,
    wanderer_query: Query<&Transform, With<WandererPrototype>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if existing_places.is_some() {
        return;
    }
    let Some(world_map) = world_map else {
        return;
    };
    let origin = wanderer_query
        .iter()
        .next()
        .map(|transform| transform.translation)
        .or_else(|| spots.as_deref().map(|spots| spots.meadow.position))
        .unwrap_or(Vec3::ZERO);
    let places = select_meaningful_places(&world_map, origin, PlaceSearchConfig::default());
    let active_place_id = places.first().map(|place| place.id);

    tracing::info!(
        target: "dao_game::places::generation",
        place_count = places.len(),
        active_place_id,
        "meaningful places selected"
    );
    for place in &places {
        tracing::info!(
            target: "dao_game::places::candidate",
            place_id = place.id,
            kind = place.kind.label(),
            grid_x = place.grid.x,
            grid_z = place.grid.z,
            biome = ?place.biome,
            score = place.score,
            x = place.position.x,
            z = place.position.z,
            tags = ?place.tags,
            "meaningful place candidate accepted"
        );
    }

    commands.insert_resource(MeaningfulPlaces {
        places: places.clone(),
        active_place_id,
        nearest_place_id: None,
        nearest_distance: None,
    });

    let materials = PlaceMaterials::new(&mut materials);
    for place in &places {
        spawn_place_visual(&mut commands, &mut meshes, &materials, place);
    }
}

fn update_place_proximity(
    places: Option<ResMut<MeaningfulPlaces>>,
    wanderer_query: Query<&Transform, With<WandererPrototype>>,
) {
    let Some(mut places) = places else {
        return;
    };
    let Some(transform) = wanderer_query.iter().next() else {
        return;
    };
    let nearest = nearest_place(&places.places, transform.translation)
        .map(|(place, distance)| (place.id, distance));
    places.nearest_place_id = nearest.map(|(id, _)| id);
    places.nearest_distance = nearest.map(|(_, distance)| distance);
}

fn cleanup_places_session(mut commands: Commands) {
    commands.remove_resource::<MeaningfulPlaces>();
}

pub fn select_meaningful_places(
    world_map: &WorldMap,
    origin: Vec3,
    config: PlaceSearchConfig,
) -> Vec<MeaningfulPlace> {
    let origin_x = (origin.x / world_map.cell_size()).round() as i32;
    let origin_z = (origin.z / world_map.cell_size()).round() as i32;
    let search_radius = config.search_radius_tiles.min(world_map.radius()).max(1);
    let step_tiles = config.step_tiles.max(1);
    let mut candidates = Vec::new();

    for z in (origin_z - search_radius..=origin_z + search_radius).step_by(step_tiles) {
        for x in (origin_x - search_radius..=origin_x + search_radius).step_by(step_tiles) {
            let Some(tile) = world_map.tile_at_grid(x, z) else {
                continue;
            };
            let position =
                world_map.tile_translation(x, z, tile.height().max(world_map.water_level()) + 0.1);
            let distance = planar_distance(origin, position);
            if !(config.min_distance..=config.max_distance).contains(&distance) {
                continue;
            }

            for kind in PlaceKind::ALL {
                if suitability(tile, kind) < 0.34 {
                    continue;
                }
                candidates.push(build_candidate(
                    world_map,
                    PlaceCandidateSample {
                        tile,
                        grid: WorldGridCoord { x, z },
                        position,
                        distance,
                        kind,
                    },
                    config,
                ));
            }
        }
    }

    candidates.sort_by(|a, b| b.score.total_cmp(&a.score).then_with(|| a.id.cmp(&b.id)));

    let mut selected = Vec::new();
    for candidate in candidates {
        if selected
            .iter()
            .any(|place: &MeaningfulPlace| place.kind == candidate.kind)
        {
            continue;
        }
        if selected
            .iter()
            .any(|place| planar_distance(place.position, candidate.position) < 34.0)
        {
            continue;
        }
        selected.push(candidate);
        if selected.len() >= config.max_places {
            break;
        }
    }

    selected
}

pub fn choose_primary_place(
    places: &MeaningfulPlaces,
    player_position: Vec3,
) -> Option<&MeaningfulPlace> {
    if let Some(active) = places.active_place() {
        return Some(active);
    }
    nearest_place(&places.places, player_position).map(|(place, _)| place)
}

fn build_candidate(
    world_map: &WorldMap,
    sample: PlaceCandidateSample,
    config: PlaceSearchConfig,
) -> MeaningfulPlace {
    let suitability = suitability(sample.tile, sample.kind);
    let distance_score = 1.0
        - ((sample.distance - config.ideal_distance).abs() / config.ideal_distance).clamp(0.0, 1.0);
    let score = suitability * 0.68
        + distance_score * 0.3
        + hash_unit(sample.grid.x, sample.grid.z, sample.kind as u64).mul_add(0.02, 0.0);

    MeaningfulPlace {
        id: place_id(sample.grid.x, sample.grid.z, sample.kind),
        kind: sample.kind,
        grid: sample.grid,
        position: Vec3::new(
            sample.position.x,
            sample.position.y.max(world_map.water_level() + 0.12),
            sample.position.z,
        ),
        biome: sample.tile.biome(),
        tags: tags_for_place(sample.kind, sample.tile),
        score,
        arrival_radius: arrival_radius(sample.kind),
        interaction_radius: interaction_radius(sample.kind),
    }
}

#[derive(Debug, Clone, Copy)]
struct PlaceCandidateSample {
    tile: TerrainTile,
    grid: WorldGridCoord,
    position: Vec3,
    distance: f32,
    kind: PlaceKind,
}

fn suitability(tile: TerrainTile, kind: PlaceKind) -> f32 {
    let above_water = (tile.height() - 0.05).clamp(0.0, 1.0);
    let flatness = (1.0 - tile.slope()).clamp(0.0, 1.0);
    match kind {
        PlaceKind::AncientTree => {
            let biome = if tile.biome() == BiomeKind::Grove {
                0.42
            } else {
                0.0
            };
            biome + tile.moisture() * 0.32 + flatness * 0.18 + above_water * 0.08
        }
        PlaceKind::SpringEye => {
            let biome = if matches!(tile.biome(), BiomeKind::Water | BiomeKind::Meadow) {
                0.16
            } else {
                0.0
            };
            biome + tile.river() * 0.44 + tile.moisture() * 0.28 + flatness * 0.12
        }
        PlaceKind::RidgeGate => {
            let biome = if tile.biome() == BiomeKind::Ridge {
                0.35
            } else {
                0.0
            };
            biome + ((tile.height() - 1.8) / 5.0).clamp(0.0, 1.0) * 0.35 + tile.slope() * 0.3
        }
        PlaceKind::QuietBay => {
            let water = if tile.biome() == BiomeKind::Water {
                0.32
            } else {
                0.0
            };
            water + flatness * 0.26 + tile.moisture() * 0.24 + tile.river() * 0.18
        }
        PlaceKind::StoneRing => {
            let open_biome = if matches!(tile.biome(), BiomeKind::Meadow | BiomeKind::Steppe) {
                0.26
            } else {
                0.0
            };
            open_biome
                + (1.0 - tile.moisture()).clamp(0.0, 1.0) * 0.28
                + flatness * 0.22
                + tile.erosion() * 0.24
        }
    }
    .clamp(0.0, 1.0)
}

fn tags_for_place(kind: PlaceKind, tile: TerrainTile) -> Vec<PlaceTag> {
    let mut tags = match kind {
        PlaceKind::AncientTree => vec![PlaceTag::Shelter, PlaceTag::Grove, PlaceTag::Memory],
        PlaceKind::SpringEye => vec![PlaceTag::Water, PlaceTag::Stillness],
        PlaceKind::RidgeGate => vec![PlaceTag::Threshold, PlaceTag::Height],
        PlaceKind::QuietBay => vec![PlaceTag::Water, PlaceTag::Stillness, PlaceTag::Memory],
        PlaceKind::StoneRing => vec![PlaceTag::Threshold, PlaceTag::Memory],
    };
    if tile.height() > 2.4 && !tags.contains(&PlaceTag::Height) {
        tags.push(PlaceTag::Height);
    }
    if tile.moisture() > 0.68 && !tags.contains(&PlaceTag::Water) {
        tags.push(PlaceTag::Water);
    }
    tags
}

fn arrival_radius(kind: PlaceKind) -> f32 {
    match kind {
        PlaceKind::AncientTree => 16.0,
        PlaceKind::SpringEye => 12.5,
        PlaceKind::RidgeGate => 18.0,
        PlaceKind::QuietBay => 16.5,
        PlaceKind::StoneRing => 14.5,
    }
}

fn interaction_radius(kind: PlaceKind) -> f32 {
    match kind {
        PlaceKind::AncientTree => 8.5,
        PlaceKind::SpringEye => 6.5,
        PlaceKind::RidgeGate => 9.0,
        PlaceKind::QuietBay => 8.0,
        PlaceKind::StoneRing => 7.5,
    }
}

pub fn nearest_place(
    places: &[MeaningfulPlace],
    position: Vec3,
) -> Option<(&MeaningfulPlace, f32)> {
    places
        .iter()
        .map(|place| (place, planar_distance(place.position, position)))
        .min_by(|(_, a), (_, b)| a.total_cmp(b))
}

pub fn place_id(x: i32, z: i32, kind: PlaceKind) -> u64 {
    let mut value = (x as i64 as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add((z as i64 as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9))
        .wrapping_add((kind as u64).wrapping_mul(0x94D0_49BB_1331_11EB));
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn hash_unit(x: i32, z: i32, salt: u64) -> f32 {
    let value = place_id(x, z, PlaceKind::StoneRing).wrapping_add(salt);
    (value as f64 / u64::MAX as f64) as f32
}

pub fn planar_distance(a: Vec3, b: Vec3) -> f32 {
    Vec2::new(a.x - b.x, a.z - b.z).length()
}

struct PlaceMaterials {
    bark: Handle<StandardMaterial>,
    leaf: Handle<StandardMaterial>,
    water: Handle<StandardMaterial>,
    stone: Handle<StandardMaterial>,
    light: Handle<StandardMaterial>,
}

impl PlaceMaterials {
    fn new(materials: &mut Assets<StandardMaterial>) -> Self {
        Self {
            bark: materials.add(StandardMaterial {
                base_color: Color::srgb(0.32, 0.22, 0.14),
                perceptual_roughness: 0.95,
                ..Default::default()
            }),
            leaf: materials.add(StandardMaterial {
                base_color: Color::srgb(0.18, 0.47, 0.28),
                perceptual_roughness: 0.88,
                ..Default::default()
            }),
            water: materials.add(StandardMaterial {
                base_color: Color::srgba(0.22, 0.56, 0.72, 0.68),
                alpha_mode: AlphaMode::Blend,
                metallic: 0.02,
                perceptual_roughness: 0.18,
                emissive: LinearRgba::rgb(0.02, 0.06, 0.08),
                ..Default::default()
            }),
            stone: materials.add(StandardMaterial {
                base_color: Color::srgb(0.46, 0.45, 0.41),
                perceptual_roughness: 0.98,
                ..Default::default()
            }),
            light: materials.add(StandardMaterial {
                base_color: Color::srgb(0.78, 0.68, 0.46),
                emissive: LinearRgba::rgb(1.2, 0.86, 0.42),
                unlit: true,
                ..Default::default()
            }),
        }
    }
}

fn spawn_place_visual(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &PlaceMaterials,
    place: &MeaningfulPlace,
) {
    let root = commands
        .spawn((
            Name::new(format!("MeaningfulPlace:{}", place.kind.label())),
            DespawnOnExit(AppScreen::InGame),
            Transform::from_translation(place.position),
            MeaningfulPlaceEntity,
        ))
        .id();

    commands
        .entity(root)
        .with_children(|parent| match place.kind {
            PlaceKind::AncientTree => {
                parent.spawn((
                    Name::new("AncientTreeTrunk"),
                    Mesh3d(meshes.add(Mesh::from(Cylinder::new(0.58, 5.4)))),
                    MeshMaterial3d(materials.bark.clone()),
                    Transform::from_xyz(0.0, 2.7, 0.0),
                ));
                parent.spawn((
                    Name::new("AncientTreeCrown"),
                    Mesh3d(meshes.add(Sphere::new(2.45).mesh().uv(24, 14))),
                    MeshMaterial3d(materials.leaf.clone()),
                    Transform::from_xyz(0.0, 5.7, 0.0).with_scale(Vec3::new(1.12, 0.78, 1.05)),
                ));
                spawn_place_light(parent, Vec3::new(0.0, 3.7, 0.0), 58_000.0, 16.0);
            }
            PlaceKind::SpringEye => {
                parent.spawn((
                    Name::new("SpringPool"),
                    Mesh3d(meshes.add(Mesh::from(Cylinder::new(2.0, 0.08)))),
                    MeshMaterial3d(materials.water.clone()),
                    Transform::from_xyz(0.0, 0.05, 0.0),
                ));
                parent.spawn((
                    Name::new("SpringGlow"),
                    Mesh3d(meshes.add(Sphere::new(0.38).mesh().uv(16, 8))),
                    MeshMaterial3d(materials.light.clone()),
                    Transform::from_xyz(0.0, 0.42, 0.0),
                    NotShadowCaster,
                ));
                spawn_place_light(parent, Vec3::new(0.0, 1.1, 0.0), 44_000.0, 13.0);
            }
            PlaceKind::RidgeGate => {
                for x in [-1.7, 1.7] {
                    parent.spawn((
                        Name::new("RidgeGatePillar"),
                        Mesh3d(meshes.add(Mesh::from(Cylinder::new(0.34, 4.8)))),
                        MeshMaterial3d(materials.stone.clone()),
                        Transform::from_xyz(x, 2.4, 0.0)
                            .with_rotation(Quat::from_rotation_z(x * 0.04)),
                    ));
                }
                parent.spawn((
                    Name::new("RidgeGateLintel"),
                    Mesh3d(meshes.add(Mesh::from(Cylinder::new(0.2, 3.8)))),
                    MeshMaterial3d(materials.stone.clone()),
                    Transform::from_xyz(0.0, 4.75, 0.0)
                        .with_rotation(Quat::from_rotation_z(std::f32::consts::FRAC_PI_2)),
                ));
                spawn_place_light(parent, Vec3::new(0.0, 3.0, -0.5), 64_000.0, 18.0);
            }
            PlaceKind::QuietBay => {
                parent.spawn((
                    Name::new("QuietBayWater"),
                    Mesh3d(meshes.add(Mesh::from(Cylinder::new(3.2, 0.06)))),
                    MeshMaterial3d(materials.water.clone()),
                    Transform::from_xyz(0.0, 0.04, 0.0).with_scale(Vec3::new(1.4, 1.0, 0.72)),
                ));
                for index in 0..5 {
                    let angle = index as f32 / 5.0 * std::f32::consts::TAU;
                    parent.spawn((
                        Name::new("QuietBayStone"),
                        Mesh3d(meshes.add(Mesh::from(Cylinder::new(0.22, 0.42)))),
                        MeshMaterial3d(materials.stone.clone()),
                        Transform::from_xyz(angle.cos() * 3.4, 0.2, angle.sin() * 1.8),
                    ));
                }
                spawn_place_light(parent, Vec3::new(0.0, 1.0, 0.0), 38_000.0, 14.0);
            }
            PlaceKind::StoneRing => {
                parent.spawn((
                    Name::new("StoneRingOutline"),
                    Mesh3d(meshes.add(Mesh::from(Torus::new(2.2, 2.35)))),
                    MeshMaterial3d(materials.stone.clone()),
                    Transform::from_xyz(0.0, 0.08, 0.0)
                        .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
                ));
                for index in 0..7 {
                    let angle = index as f32 / 7.0 * std::f32::consts::TAU;
                    parent.spawn((
                        Name::new("StandingStone"),
                        Mesh3d(meshes.add(Mesh::from(Capsule3d::new(0.22, 1.45)))),
                        MeshMaterial3d(materials.stone.clone()),
                        Transform::from_xyz(angle.cos() * 2.4, 0.85, angle.sin() * 2.4)
                            .with_rotation(Quat::from_rotation_z(angle.sin() * 0.12)),
                    ));
                }
                spawn_place_light(parent, Vec3::new(0.0, 1.35, 0.0), 46_000.0, 15.0);
            }
        });
}

fn spawn_place_light(
    parent: &mut ChildSpawnerCommands<'_>,
    position: Vec3,
    intensity: f32,
    range: f32,
) {
    parent.spawn((
        Name::new("PlaceSoftLight"),
        PointLight {
            intensity,
            range,
            radius: 1.2,
            shadows_enabled: false,
            color: Color::srgb(0.9, 0.76, 0.48),
            ..Default::default()
        },
        Transform::from_translation(position),
    ));
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use bevy::prelude::{Vec2, Vec3};

    use crate::{
        core::config::{
            AppConfig, EnvironmentConfig, PlayerConfig, PresentationConfig, QualityConfig,
            SignConfig, WorldConfig,
        },
        game::{
            places::{PlaceKind, PlaceSearchConfig, PlaceTag, place_id, select_meaningful_places},
            world::WorldMap,
        },
    };

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
                world_radius: 96,
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

    #[test]
    fn place_id_is_deterministic_and_kind_sensitive() {
        assert_eq!(
            place_id(10, -4, PlaceKind::AncientTree),
            place_id(10, -4, PlaceKind::AncientTree)
        );
        assert_ne!(
            place_id(10, -4, PlaceKind::AncientTree),
            place_id(10, -4, PlaceKind::StoneRing)
        );
    }

    #[test]
    fn place_selection_is_deterministic_for_same_world() {
        let config = test_config();
        let world_map = WorldMap::new_for_testing(config.world.seed, &config);
        let search = PlaceSearchConfig {
            search_radius_tiles: 46,
            max_places: 5,
            ..Default::default()
        };

        let first = select_meaningful_places(&world_map, Vec3::ZERO, search);
        let second = select_meaningful_places(&world_map, Vec3::ZERO, search);

        assert_eq!(first, second);
        assert!(!first.is_empty());
        assert!(first.len() <= 5);
    }

    #[test]
    fn selected_places_keep_reasonable_distance_from_spawn() {
        let config = test_config();
        let world_map = WorldMap::new_for_testing(config.world.seed, &config);
        let search = PlaceSearchConfig {
            min_distance: 36.0,
            max_distance: 170.0,
            ..Default::default()
        };

        let places = select_meaningful_places(&world_map, Vec3::ZERO, search);

        assert!(places.iter().all(|place| {
            let distance = Vec2::new(place.position.x, place.position.z).length();
            (search.min_distance..=search.max_distance).contains(&distance)
        }));
    }

    #[test]
    fn semantic_tags_match_place_kind() {
        let config = test_config();
        let world_map = WorldMap::new_for_testing(config.world.seed, &config);
        let places = select_meaningful_places(&world_map, Vec3::ZERO, PlaceSearchConfig::default());

        for place in places {
            match place.kind {
                PlaceKind::AncientTree => assert!(place.tags.contains(&PlaceTag::Grove)),
                PlaceKind::SpringEye | PlaceKind::QuietBay => {
                    assert!(place.tags.contains(&PlaceTag::Water))
                }
                PlaceKind::RidgeGate => assert!(place.tags.contains(&PlaceTag::Threshold)),
                PlaceKind::StoneRing => assert!(place.tags.contains(&PlaceTag::Memory)),
            }
        }
    }
}
