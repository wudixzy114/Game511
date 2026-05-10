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
        app.add_systems(
            Startup,
            (configure_world_seed, spawn_camera, spawn_light, spawn_world),
        );
    }
}

#[derive(Debug, Resource, Clone, Copy, PartialEq, Eq)]
pub struct WorldSeed(pub u64);

#[derive(Debug, Clone, Copy, PartialEq)]
struct TerrainSample {
    height: f32,
    moisture: f32,
}

fn configure_world_seed(config: Res<AppConfig>, mut seed: ResMut<WorldSeed>) {
    seed.0 = config.world.seed;
    tracing::info!(
        target: "dao_game::world::bootstrap",
        seed = seed.0,
        "world seed configured"
    );
}

fn spawn_camera(mut commands: Commands) {
    commands.spawn((
        Camera3d::default(),
        Transform::from_xyz(-8.0, 10.0, 14.0).looking_at(Vec3::new(0.0, 0.5, 0.0), Vec3::Y),
    ));
}

fn spawn_light(mut commands: Commands) {
    commands.spawn((
        DirectionalLight {
            shadows_enabled: true,
            illuminance: 12_500.0,
            ..Default::default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.9, 0.6, 0.0)),
    ));
}

fn spawn_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    config: Res<AppConfig>,
    seed: Res<WorldSeed>,
) {
    let radius = config.world.world_radius;
    let plane_mesh = meshes.add(Mesh::from(
        Plane3d::default()
            .mesh()
            .size((radius as f32 + 1.0) * 4.0, (radius as f32 + 1.0) * 4.0),
    ));
    let ground_material = materials.add(StandardMaterial {
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

    commands.spawn((
        Mesh3d(plane_mesh),
        MeshMaterial3d(ground_material),
        Transform::from_xyz(0.0, config.world.water_level - 0.02, 0.0),
    ));

    commands.spawn((
        Mesh3d(meshes.add(Mesh::from(Cuboid::new(
            (radius as f32 + 1.0) * 4.0,
            0.02,
            (radius as f32 + 1.0) * 4.0,
        )))),
        MeshMaterial3d(water_material),
        Transform::from_xyz(0.0, config.world.water_level, 0.0),
    ));

    for x in -radius..=radius {
        for z in -radius..=radius {
            let sample = sample_terrain(x, z, seed.0, &config);
            let color = biome_color(sample.moisture, sample.height, config.world.water_level);
            commands.spawn((
                Mesh3d(meshes.add(Mesh::from(Cuboid::new(1.8, sample.height.max(0.2), 1.8)))),
                MeshMaterial3d(materials.add(StandardMaterial {
                    base_color: color,
                    perceptual_roughness: 0.98,
                    ..Default::default()
                })),
                Transform::from_xyz(x as f32 * 2.0, sample.height * 0.5, z as f32 * 2.0),
            ));
        }
    }

    commands.spawn((
        Name::new("WandererPrototype"),
        Mesh3d(meshes.add(Mesh::from(Capsule3d::new(0.35, 1.2)))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.82, 0.72, 0.6),
            ..Default::default()
        })),
        Transform::from_xyz(0.0, 1.2, 0.0),
    ));

    tracing::info!(
        target: "dao_game::world::generation",
        radius = radius,
        seed = seed.0,
        "procedural world prototype spawned"
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

fn biome_color(moisture: f32, height: f32, water_level: f32) -> Color {
    if height <= water_level + 0.2 {
        return Color::srgb(0.42, 0.37, 0.26);
    }
    if moisture > 0.65 {
        Color::srgb(0.18, 0.42, 0.2)
    } else if moisture > 0.45 {
        Color::srgb(0.35, 0.42, 0.23)
    } else {
        Color::srgb(0.52, 0.44, 0.24)
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::core::config::{AppConfig, QualityConfig, SignConfig, WorldConfig};

    use super::{WorldSeed, biome_color, sample_terrain};

    fn test_config() -> AppConfig {
        AppConfig {
            window_title: "Dao".to_string(),
            log_directory: PathBuf::from("logs"),
            performance_log_name: "performance.log".to_string(),
            frame_log_interval: 60,
            world: WorldConfig {
                seed: 42,
                world_radius: 2,
                terrain_scale: 8.0,
                height_variation: 3.5,
                water_level: -0.1,
            },
            signs: SignConfig {
                resonance_threshold: 0.7,
                calm_recovery: 0.01,
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
    fn biome_color_changes_near_water() {
        let low = biome_color(0.8, 0.0, 0.1);
        let high = biome_color(0.8, 2.0, 0.1);

        assert_ne!(low, high);
    }

    #[test]
    fn world_seed_resource_wraps_seed_value() {
        assert_eq!(WorldSeed(511).0, 511);
    }
}
