use std::time::Instant;

use bevy::{
    math::primitives::{Capsule3d, Cuboid, Plane3d},
    pbr::MeshMaterial3d,
    prelude::*,
};

use crate::core::config::AppConfig;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(WorldSeed(0));
        app.insert_resource(WorldCycle::default());
        app.add_systems(Startup, (configure_world_seed, generate_world_map).chain());
        app.add_systems(
            Startup,
            (spawn_camera, spawn_light, spawn_world).after(generate_world_map),
        );
        app.add_systems(
            Update,
            (advance_world_cycle, animate_wanderer, animate_sunlight),
        );
    }
}

const TILE_SIZE: f32 = 2.0;

#[derive(Debug, Resource, Clone, Copy, PartialEq, Eq)]
pub struct WorldSeed(pub u64);

#[derive(Debug, Resource, Clone, Copy, PartialEq)]
pub struct WorldCycle {
    pub normalized_time: f32,
    pub daylight: f32,
}

impl Default for WorldCycle {
    fn default() -> Self {
        Self {
            normalized_time: 0.12,
            daylight: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct TerrainSample {
    height: f32,
    moisture: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainTile {
    height: f32,
    moisture: f32,
    slope: f32,
    biome: BiomeKind,
}

impl TerrainTile {
    pub fn height(self) -> f32 {
        self.height
    }

    pub fn moisture(self) -> f32 {
        self.moisture
    }

    pub fn slope(self) -> f32 {
        self.slope
    }

    pub fn biome(self) -> BiomeKind {
        self.biome
    }
}

#[derive(Debug, Component)]
pub struct WandererPrototype;

#[derive(Debug, Component)]
pub struct WorldCamera;

#[derive(Debug, Component)]
struct SunLight;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BiomeKind {
    Water,
    Meadow,
    Grove,
    Steppe,
    Ridge,
}

#[derive(Debug, Resource, Clone)]
pub struct WorldMap {
    radius: i32,
    tile_size: f32,
    water_level: f32,
    tiles: Vec<TerrainTile>,
}

impl WorldMap {
    fn new(seed: u64, config: &AppConfig) -> Self {
        let radius = config.world.world_radius;
        let mut tiles = Vec::with_capacity(((radius * 2 + 1) * (radius * 2 + 1)) as usize);

        for z in -radius..=radius {
            for x in -radius..=radius {
                let center = sample_terrain(x, z, seed, config);
                let east = sample_terrain(x + 1, z, seed, config);
                let west = sample_terrain(x - 1, z, seed, config);
                let north = sample_terrain(x, z + 1, seed, config);
                let south = sample_terrain(x, z - 1, seed, config);
                let slope = [
                    (center.height - east.height).abs(),
                    (center.height - west.height).abs(),
                    (center.height - north.height).abs(),
                    (center.height - south.height).abs(),
                ]
                .into_iter()
                .fold(0.0_f32, f32::max);
                let biome = determine_biome(center, slope, config.world.water_level);

                tiles.push(TerrainTile {
                    height: center.height,
                    moisture: center.moisture,
                    slope,
                    biome,
                });
            }
        }

        Self {
            radius,
            tile_size: TILE_SIZE,
            water_level: config.world.water_level,
            tiles,
        }
    }

    pub fn radius(&self) -> i32 {
        self.radius
    }

    pub fn tile_size(&self) -> f32 {
        self.tile_size
    }

    pub fn water_level(&self) -> f32 {
        self.water_level
    }

    pub fn tile_at_grid(&self, x: i32, z: i32) -> Option<TerrainTile> {
        if x < -self.radius || x > self.radius || z < -self.radius || z > self.radius {
            return None;
        }

        let diameter = self.radius * 2 + 1;
        let local_x = x + self.radius;
        let local_z = z + self.radius;
        let index = (local_z * diameter + local_x) as usize;
        self.tiles.get(index).copied()
    }

    pub fn sample_world_position(&self, position: Vec3) -> Option<TerrainTile> {
        let x = (position.x / self.tile_size).round() as i32;
        let z = (position.z / self.tile_size).round() as i32;
        self.tile_at_grid(x, z)
    }

    pub fn tile_translation(&self, x: i32, z: i32, height: f32) -> Vec3 {
        Vec3::new(
            x as f32 * self.tile_size,
            height * 0.5,
            z as f32 * self.tile_size,
        )
    }
}

#[derive(Debug, Resource, Clone, Copy)]
pub struct WorldPresentationControl {
    pub time_override: Option<f32>,
    pub wander_target: Option<Vec3>,
    pub wander_speed_multiplier: f32,
}

impl Default for WorldPresentationControl {
    fn default() -> Self {
        Self {
            time_override: None,
            wander_target: None,
            wander_speed_multiplier: 1.0,
        }
    }
}

#[derive(Debug, Clone)]
struct DetailMaterials {
    foliage: Handle<StandardMaterial>,
    stone: Handle<StandardMaterial>,
}

#[derive(Debug, Clone, Copy)]
struct DetailPlacement {
    x: i32,
    z: i32,
    tile: TerrainTile,
    base_translation: Vec3,
}

fn configure_world_seed(config: Res<AppConfig>, mut seed: ResMut<WorldSeed>) {
    seed.0 = config.world.seed;
    tracing::info!(
        target: "dao_game::world::bootstrap",
        seed = seed.0,
        "world seed configured"
    );
}

fn generate_world_map(mut commands: Commands, config: Res<AppConfig>, seed: Res<WorldSeed>) {
    let started_at = Instant::now();
    let world_map = WorldMap::new(seed.0, &config);
    let tile_count = world_map.tiles.len();
    commands.insert_resource(world_map);
    tracing::info!(
        target: "dao_game::world::generation",
        radius = config.world.world_radius,
        seed = seed.0,
        tile_count = tile_count,
        generation_ms = started_at.elapsed().as_secs_f32() * 1000.0,
        "world map generated"
    );
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-8.0, 10.0, 14.0).looking_at(Vec3::new(0.0, 0.5, 0.0), Vec3::Y),
        WorldCamera,
    ));
}

fn spawn_light(mut commands: Commands) {
    commands.spawn((
        Name::new("SunLight"),
        DirectionalLight {
            shadows_enabled: true,
            illuminance: 12_500.0,
            ..Default::default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.9, 0.6, 0.0)),
        SunLight,
    ));
}

fn spawn_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    world_map: Res<WorldMap>,
    seed: Res<WorldSeed>,
) {
    let started_at = Instant::now();
    let radius = world_map.radius();
    let plane_mesh = meshes.add(Mesh::from(
        Plane3d::default()
            .mesh()
            .size((radius as f32 + 1.0) * 4.0, (radius as f32 + 1.0) * 4.0),
    ));
    let bedrock_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.2, 0.31, 0.18),
        perceptual_roughness: 0.96,
        ..Default::default()
    });
    let water_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.15, 0.34, 0.46, 0.72),
        alpha_mode: AlphaMode::Blend,
        metallic: 0.02,
        perceptual_roughness: 0.14,
        ..Default::default()
    });
    let meadow_material = materials.add(StandardMaterial {
        base_color: biome_color(BiomeKind::Meadow),
        perceptual_roughness: 0.94,
        ..Default::default()
    });
    let grove_material = materials.add(StandardMaterial {
        base_color: biome_color(BiomeKind::Grove),
        perceptual_roughness: 0.96,
        ..Default::default()
    });
    let steppe_material = materials.add(StandardMaterial {
        base_color: biome_color(BiomeKind::Steppe),
        perceptual_roughness: 0.98,
        ..Default::default()
    });
    let ridge_material = materials.add(StandardMaterial {
        base_color: biome_color(BiomeKind::Ridge),
        perceptual_roughness: 0.99,
        ..Default::default()
    });
    let shore_material = materials.add(StandardMaterial {
        base_color: biome_color(BiomeKind::Water),
        perceptual_roughness: 0.97,
        ..Default::default()
    });
    let detail_materials = DetailMaterials {
        foliage: materials.add(StandardMaterial {
            base_color: Color::srgb(0.2, 0.43, 0.25),
            perceptual_roughness: 0.9,
            ..Default::default()
        }),
        stone: materials.add(StandardMaterial {
            base_color: Color::srgb(0.45, 0.45, 0.48),
            perceptual_roughness: 0.99,
            ..Default::default()
        }),
    };

    commands.spawn((
        Name::new("WorldBedrock"),
        Mesh3d(plane_mesh),
        MeshMaterial3d(bedrock_material),
        Transform::from_xyz(0.0, world_map.water_level() - 0.06, 0.0),
    ));

    commands.spawn((
        Name::new("WaterPlane"),
        Mesh3d(meshes.add(Mesh::from(Cuboid::new(
            (radius as f32 + 1.0) * 4.0,
            0.02,
            (radius as f32 + 1.0) * 4.0,
        )))),
        MeshMaterial3d(water_material),
        Transform::from_xyz(0.0, world_map.water_level(), 0.0),
    ));

    for x in -radius..=radius {
        for z in -radius..=radius {
            let Some(tile) = world_map.tile_at_grid(x, z) else {
                continue;
            };
            let material = biome_material(
                tile.biome(),
                &meadow_material,
                &grove_material,
                &steppe_material,
                &ridge_material,
                &shore_material,
            );
            commands.spawn((
                Name::new(format!("TerrainTile({x},{z})")),
                Mesh3d(meshes.add(Mesh::from(Cuboid::new(1.8, tile.height().max(0.2), 1.8)))),
                MeshMaterial3d(material),
                Transform::from_translation(world_map.tile_translation(x, z, tile.height())),
            ));

            spawn_detail(
                &mut commands,
                &mut meshes,
                seed.0,
                DetailPlacement {
                    x,
                    z,
                    tile,
                    base_translation: world_map.tile_translation(x, z, tile.height()),
                },
                &detail_materials,
            );
        }
    }

    let start_height = world_map
        .tile_at_grid(0, 0)
        .map(TerrainTile::height)
        .unwrap_or(0.0)
        + 1.2;
    commands.spawn((
        Name::new("WandererPrototype"),
        Mesh3d(meshes.add(Mesh::from(Capsule3d::new(0.35, 1.2)))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.82, 0.72, 0.6),
            ..Default::default()
        })),
        Transform::from_xyz(0.0, start_height, 0.0),
        WandererPrototype,
    ));

    tracing::info!(
        target: "dao_game::world::generation",
        radius = radius,
        seed = seed.0,
        generation_ms = started_at.elapsed().as_secs_f32() * 1000.0,
        "procedural world prototype spawned"
    );
}

fn advance_world_cycle(
    time: Res<Time>,
    config: Res<AppConfig>,
    control: Option<Res<WorldPresentationControl>>,
    mut cycle: ResMut<WorldCycle>,
) {
    if let Some(control) = control.and_then(|control| control.time_override) {
        cycle.normalized_time = control.rem_euclid(1.0);
    } else {
        let cycle_length = config.environment.day_length_seconds.max(1.0);
        cycle.normalized_time =
            (cycle.normalized_time + time.delta_secs() / cycle_length).rem_euclid(1.0);
    }
    let sun_height = (cycle.normalized_time * std::f32::consts::TAU).sin();
    cycle.daylight = (sun_height * 0.5 + 0.5).clamp(0.0, 1.0);
}

fn animate_wanderer(
    time: Res<Time>,
    config: Res<AppConfig>,
    control: Option<Res<WorldPresentationControl>>,
    world_map: Res<WorldMap>,
    mut query: Query<&mut Transform, With<WandererPrototype>>,
) {
    let Some(mut transform) = query.iter_mut().next() else {
        return;
    };

    if let Some(control) = control
        .as_deref()
        .filter(|control| control.wander_target.is_some())
    {
        animate_controlled_wanderer(
            time.delta_secs(),
            config.environment.wander_speed,
            control,
            &world_map,
            &mut transform,
        );
        return;
    }

    let t = time.elapsed_secs() * config.environment.wander_speed.max(0.05);
    let radius = config
        .environment
        .wander_radius
        .min(world_map.radius() as f32 * 1.7);
    let x = t.cos() * radius * 0.75 + (t * 0.43).sin() * radius * 0.2;
    let z = (t * 0.72).sin() * radius;
    let next_x = (t + 0.2).cos() * radius * 0.75 + ((t + 0.2) * 0.43).sin() * radius * 0.2;
    let next_z = ((t + 0.2) * 0.72).sin() * radius;

    let Some(tile) = world_map.sample_world_position(Vec3::new(x, 0.0, z)) else {
        return;
    };
    let next_height = world_map
        .sample_world_position(Vec3::new(next_x, 0.0, next_z))
        .map(TerrainTile::height)
        .unwrap_or(tile.height());

    let target_position = Vec3::new(x, tile.height() + 1.2, z);
    let next_position = Vec3::new(next_x, next_height + 1.2, next_z);
    let smoothing = 1.0 - (-5.0 * time.delta_secs()).exp();
    transform.translation = transform.translation.lerp(target_position, smoothing);
    transform.look_at(next_position, Vec3::Y);
}

fn animate_controlled_wanderer(
    delta_secs: f32,
    base_speed: f32,
    control: &WorldPresentationControl,
    world_map: &WorldMap,
    transform: &mut Transform,
) {
    let Some(mut target_position) = control.wander_target else {
        return;
    };
    let Some(tile) = world_map.sample_world_position(target_position) else {
        return;
    };
    target_position.y = tile.height() + 1.2;

    let direction = target_position - transform.translation;
    let distance = direction.length();
    if distance > 0.01 {
        let step = (base_speed * 4.5 * control.wander_speed_multiplier.max(0.1) * delta_secs)
            .min(distance);
        let movement = direction.normalize() * step;
        transform.translation += movement;
        transform.look_at(target_position + Vec3::new(0.0, 0.0, 0.2), Vec3::Y);
    } else {
        transform.translation = transform.translation.lerp(target_position, 0.18);
    }
}

fn animate_sunlight(
    cycle: Res<WorldCycle>,
    mut clear_color: ResMut<ClearColor>,
    mut lights: Query<(&mut DirectionalLight, &mut Transform), With<SunLight>>,
) {
    let Some((mut light, mut transform)) = lights.iter_mut().next() else {
        return;
    };

    let phase = cycle.normalized_time * std::f32::consts::TAU;
    let sun_height = phase.sin();
    let daylight = cycle.daylight;
    let pitch = -0.35 - sun_height * 1.05;
    let yaw = 0.45 + phase * 0.18;

    transform.rotation = Quat::from_euler(EulerRot::XYZ, pitch, yaw, 0.0);
    light.illuminance = 900.0 + daylight.powf(1.6) * 45_000.0;
    light.color = Color::srgb(1.0, 0.72 + daylight * 0.22, 0.62 + daylight * 0.32);
    clear_color.0 = Color::srgb(
        0.03 + daylight * 0.18,
        0.05 + daylight * 0.24,
        0.09 + daylight * 0.28,
    );
}

fn sample_terrain(x: i32, z: i32, seed: u64, config: &AppConfig) -> TerrainSample {
    let xf = x as f32 / config.world.terrain_scale;
    let zf = z as f32 / config.world.terrain_scale;
    let seed_bias = (seed % 97) as f32 * 0.013;

    let ridge = (xf.sin() * 0.65 + zf.cos() * 0.35 + seed_bias).sin();
    let basin = ((xf * 0.7 + seed_bias).cos() + (zf * 1.3 - seed_bias).sin()) * 0.5;
    let moisture = ((xf * 0.5).cos() * 0.5 + (zf * 0.8).sin() * 0.5 + 1.0) * 0.5;
    let height = (ridge + basin) * config.world.height_variation + 1.6;

    TerrainSample { height, moisture }
}

fn determine_biome(sample: TerrainSample, slope: f32, water_level: f32) -> BiomeKind {
    if sample.height <= water_level + 0.15 {
        BiomeKind::Water
    } else if slope > 1.55 || sample.height > water_level + 3.5 {
        BiomeKind::Ridge
    } else if sample.moisture > 0.7 {
        BiomeKind::Grove
    } else if sample.moisture > 0.45 {
        BiomeKind::Meadow
    } else {
        BiomeKind::Steppe
    }
}

fn biome_color(biome: BiomeKind) -> Color {
    match biome {
        BiomeKind::Water => Color::srgb(0.42, 0.37, 0.26),
        BiomeKind::Meadow => Color::srgb(0.35, 0.44, 0.24),
        BiomeKind::Grove => Color::srgb(0.18, 0.39, 0.2),
        BiomeKind::Steppe => Color::srgb(0.56, 0.47, 0.28),
        BiomeKind::Ridge => Color::srgb(0.43, 0.41, 0.39),
    }
}

fn biome_material(
    biome: BiomeKind,
    meadow: &Handle<StandardMaterial>,
    grove: &Handle<StandardMaterial>,
    steppe: &Handle<StandardMaterial>,
    ridge: &Handle<StandardMaterial>,
    shore: &Handle<StandardMaterial>,
) -> Handle<StandardMaterial> {
    match biome {
        BiomeKind::Water => shore.clone(),
        BiomeKind::Meadow => meadow.clone(),
        BiomeKind::Grove => grove.clone(),
        BiomeKind::Steppe => steppe.clone(),
        BiomeKind::Ridge => ridge.clone(),
    }
}

fn spawn_detail(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    seed: u64,
    placement: DetailPlacement,
    materials: &DetailMaterials,
) {
    let offset = detail_offset(seed, placement.x, placement.z);
    match placement.tile.biome() {
        BiomeKind::Water => {}
        BiomeKind::Meadow if detail_noise(seed, placement.x, placement.z, 17) > 0.48 => {
            commands.spawn((
                Name::new(format!("MeadowReed({},{})", placement.x, placement.z)),
                Mesh3d(meshes.add(Mesh::from(Capsule3d::new(0.08, 0.55)))),
                MeshMaterial3d(materials.foliage.clone()),
                Transform::from_translation(
                    placement.base_translation + Vec3::new(offset.x, 0.45, offset.y),
                ),
            ));
        }
        BiomeKind::Grove if detail_noise(seed, placement.x, placement.z, 41) > 0.28 => {
            commands.spawn((
                Name::new(format!("GrovePillar({},{})", placement.x, placement.z)),
                Mesh3d(meshes.add(Mesh::from(Capsule3d::new(0.18, 0.95)))),
                MeshMaterial3d(materials.foliage.clone()),
                Transform::from_translation(
                    placement.base_translation + Vec3::new(offset.x, 0.75, offset.y),
                ),
            ));
        }
        BiomeKind::Steppe if detail_noise(seed, placement.x, placement.z, 23) > 0.52 => {
            commands.spawn((
                Name::new(format!("SteppeStone({},{})", placement.x, placement.z)),
                Mesh3d(meshes.add(Mesh::from(Cuboid::new(0.5, 0.18, 0.36)))),
                MeshMaterial3d(materials.stone.clone()),
                Transform::from_translation(
                    placement.base_translation + Vec3::new(offset.x, 0.14, offset.y),
                ),
            ));
        }
        BiomeKind::Ridge if detail_noise(seed, placement.x, placement.z, 7) > 0.34 => {
            commands.spawn((
                Name::new(format!("RidgeMonolith({},{})", placement.x, placement.z)),
                Mesh3d(meshes.add(Mesh::from(Cuboid::new(0.34, 1.2, 0.34)))),
                MeshMaterial3d(materials.stone.clone()),
                Transform::from_translation(
                    placement.base_translation + Vec3::new(offset.x, 0.7, offset.y),
                ),
            ));
        }
        _ => {}
    }
}

fn detail_offset(seed: u64, x: i32, z: i32) -> Vec2 {
    let x_offset = detail_noise(seed, x, z, 3) * 0.7 - 0.35;
    let z_offset = detail_noise(seed, x, z, 9) * 0.7 - 0.35;
    Vec2::new(x_offset, z_offset)
}

fn detail_noise(seed: u64, x: i32, z: i32, salt: u64) -> f32 {
    let mut value = seed
        .wrapping_add((x as i64 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
        .wrapping_add((z as i64 as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9))
        .wrapping_add(salt.wrapping_mul(0x94D0_49BB_1331_11EB));
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^= value >> 31;
    (value as f64 / u64::MAX as f64) as f32
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use bevy::prelude::Vec3;

    use crate::core::config::{
        AppConfig, EnvironmentConfig, PresentationConfig, QualityConfig, SignConfig, WorldConfig,
    };

    use super::{BiomeKind, WorldMap, WorldSeed, biome_color, determine_biome, sample_terrain};

    fn test_config() -> AppConfig {
        AppConfig {
            window_title: "Dao".to_string(),
            log_directory: PathBuf::from("logs"),
            performance_log_name: "performance.log".to_string(),
            frame_log_interval: 60,
            presentation: PresentationConfig {
                enabled: true,
                scene_duration_seconds: 7.0,
                camera_blend_speed: 2.0,
            },
            world: WorldConfig {
                seed: 42,
                world_radius: 2,
                terrain_scale: 8.0,
                height_variation: 3.5,
                water_level: -0.1,
            },
            environment: EnvironmentConfig {
                day_length_seconds: 180.0,
                wander_radius: 4.5,
                wander_speed: 0.7,
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
    fn terrain_sampling_is_deterministic() {
        let config = test_config();
        let a = sample_terrain(2, -1, 42, &config);
        let b = sample_terrain(2, -1, 42, &config);

        assert_eq!(a, b);
    }

    #[test]
    fn biome_color_differs_between_biomes() {
        let grove = biome_color(BiomeKind::Grove);
        let ridge = biome_color(BiomeKind::Ridge);

        assert_ne!(grove, ridge);
    }

    #[test]
    fn determine_biome_marks_water_and_ridge() {
        let water = determine_biome(
            super::TerrainSample {
                height: -0.2,
                moisture: 0.6,
            },
            0.1,
            -0.1,
        );
        let ridge = determine_biome(
            super::TerrainSample {
                height: 3.8,
                moisture: 0.2,
            },
            1.8,
            -0.1,
        );

        assert_eq!(water, BiomeKind::Water);
        assert_eq!(ridge, BiomeKind::Ridge);
    }

    #[test]
    fn world_map_can_sample_center_tile() {
        let config = test_config();
        let world_map = WorldMap::new(42, &config);
        let center = world_map
            .sample_world_position(Vec3::new(0.1, 0.0, -0.2))
            .expect("center tile should exist");

        assert!(center.height() > -0.1);
    }

    #[test]
    fn world_seed_resource_wraps_seed_value() {
        assert_eq!(WorldSeed(511).0, 511);
    }
}
