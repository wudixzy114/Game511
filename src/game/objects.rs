use bevy::{
    math::primitives::{Cylinder, Plane3d, Sphere},
    pbr::MeshMaterial3d,
    prelude::*,
};

use crate::game::{
    environment::WindField,
    flow::{AppScreen, InGameState, SessionMode, in_session_mode},
};

pub struct ProceduralObjectPlugin;

impl Plugin for ProceduralObjectPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(AppScreen::InGame),
            spawn_procedural_object_gallery.run_if(in_session_mode(SessionMode::MaterialGallery)),
        );
        app.add_systems(
            Update,
            animate_procedural_tree_wind
                .run_if(in_state(InGameState::Running))
                .run_if(in_session_mode(SessionMode::MaterialGallery)),
        );
        app.add_systems(OnExit(AppScreen::InGame), cleanup_procedural_object_gallery);
    }
}

#[derive(Debug, Component)]
struct ProceduralObjectGalleryRoot;

#[derive(Debug, Component, Clone, Copy)]
struct ProceduralTreeWindPart {
    base_transform: Transform,
    phase: f32,
    amplitude: f32,
    stiffness: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ProceduralTreeProfile {
    pub seed: u64,
    pub height: f32,
    pub trunk_radius: f32,
    pub lean: Vec2,
    pub branch_count: usize,
    pub leaf_cluster_count: usize,
    pub canopy_radius: f32,
    pub leaf_color: [f32; 3],
    pub bark_color: [f32; 3],
    pub wind_flex: f32,
}

#[derive(Debug)]
struct TreeGalleryMeshes {
    floor: Handle<Mesh>,
    trunk: Handle<Mesh>,
    branch: Handle<Mesh>,
    leaf: Handle<Mesh>,
}

#[derive(Debug)]
struct TreeGalleryMaterials {
    floor: Handle<StandardMaterial>,
    bark: Handle<StandardMaterial>,
    leaf_primary: Handle<StandardMaterial>,
    leaf_secondary: Handle<StandardMaterial>,
}

#[derive(Debug, Clone, Copy)]
struct BranchSegment {
    start: Vec3,
    end: Vec3,
    radius: f32,
}

const TREE_VARIANT_COUNT: usize = 12;
const TREE_GALLERY_BASE_SEED: u64 = 0xA11C_EE05_13AA_700D;

pub fn procedural_tree_profile(seed: u64) -> ProceduralTreeProfile {
    let height = lerp(4.8, 8.8, seeded_unit(seed, 11));
    let trunk_radius = lerp(0.22, 0.48, seeded_unit(seed, 13));
    let lean_angle = seeded_signed(seed, 17) * 0.42;
    let lean_distance = lerp(0.08, 0.32, seeded_unit(seed, 19)) * height;
    let lean = Vec2::new(lean_angle.cos(), lean_angle.sin()) * lean_distance;
    let branch_count = 5 + (seeded_unit(seed, 23) * 6.0).floor() as usize;
    let leaf_cluster_count = branch_count + 4 + (seeded_unit(seed, 29) * 5.0).floor() as usize;
    let canopy_radius = lerp(1.65, 3.1, seeded_unit(seed, 31));
    let hue_shift = seeded_signed(seed, 37);
    let dryness = seeded_unit(seed, 41);
    let leaf_color = [
        (0.18 + hue_shift * 0.045 + dryness * 0.08).clamp(0.08, 0.34),
        (0.36 + seeded_unit(seed, 43) * 0.32 - dryness * 0.08).clamp(0.26, 0.68),
        (0.16 + seeded_unit(seed, 47) * 0.16 - dryness * 0.04).clamp(0.1, 0.36),
    ];
    let bark_color = [
        lerp(0.22, 0.38, seeded_unit(seed, 53)),
        lerp(0.16, 0.26, seeded_unit(seed, 59)),
        lerp(0.09, 0.16, seeded_unit(seed, 61)),
    ];
    let wind_flex = lerp(0.55, 1.25, seeded_unit(seed, 67));

    ProceduralTreeProfile {
        seed,
        height,
        trunk_radius,
        lean,
        branch_count,
        leaf_cluster_count,
        canopy_radius,
        leaf_color,
        bark_color,
        wind_flex,
    }
}

fn spawn_procedural_object_gallery(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let floor_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.095, 0.12, 0.1),
        perceptual_roughness: 0.92,
        metallic: 0.0,
        ..Default::default()
    });
    let gallery_meshes = TreeGalleryMeshes {
        floor: meshes.add(Mesh::from(Plane3d::new(Vec3::Y, Vec2::new(36.0, 15.0)))),
        trunk: meshes.add(Mesh::from(Cylinder::new(1.0, 1.0))),
        branch: meshes.add(Mesh::from(Cylinder::new(1.0, 1.0))),
        leaf: meshes.add(Sphere::new(1.0).mesh().uv(18, 12)),
    };

    commands
        .spawn((
            Name::new("ProceduralObjectGallery"),
            DespawnOnExit(AppScreen::InGame),
            ProceduralObjectGalleryRoot,
            Transform::default(),
            Visibility::Visible,
        ))
        .with_children(|parent| {
            parent.spawn((
                Name::new("ProceduralTreeGalleryFloor"),
                Mesh3d(gallery_meshes.floor.clone()),
                MeshMaterial3d(floor_material),
                Transform::from_xyz(9.5, -0.035, -9.2),
            ));

            for index in 0..TREE_VARIANT_COUNT {
                let seed = TREE_GALLERY_BASE_SEED.wrapping_add(index as u64 * 0x9E37_79B9);
                let profile = procedural_tree_profile(seed);
                let column = index % 6;
                let row = index / 6;
                let position = Vec3::new(column as f32 * 5.7 - 4.8, 0.0, -6.5 - row as f32 * 5.4);
                spawn_tree_variant(parent, &mut materials, &gallery_meshes, profile, position);
            }
        });

    tracing::info!(
        target: "dao_game::objects::gallery",
        variant_count = TREE_VARIANT_COUNT,
        seed = TREE_GALLERY_BASE_SEED,
        "procedural object gallery spawned"
    );
}

fn spawn_tree_variant(
    parent: &mut ChildSpawnerCommands<'_>,
    materials: &mut Assets<StandardMaterial>,
    meshes: &TreeGalleryMeshes,
    profile: ProceduralTreeProfile,
    position: Vec3,
) {
    let gallery_materials = tree_materials(materials, profile);
    parent
        .spawn((
            Name::new(format!("ProceduralTree::{:016x}", profile.seed)),
            Transform::from_translation(position),
            Visibility::Visible,
        ))
        .with_children(|tree| {
            let trunk_top = Vec3::new(profile.lean.x, profile.height, profile.lean.y);
            let trunk = BranchSegment {
                start: Vec3::Y * (profile.trunk_radius * 0.45),
                end: trunk_top,
                radius: profile.trunk_radius,
            };
            spawn_segment(
                tree,
                &meshes.trunk,
                gallery_materials.bark.clone(),
                trunk,
                Some(wind_part(
                    profile,
                    transform_for_segment(trunk),
                    0.34,
                    0.34,
                    1.0,
                )),
                "ProceduralTreeTrunk",
            );

            let mut branch_tips = Vec::with_capacity(profile.branch_count + 1);
            for branch_index in 0..profile.branch_count {
                let branch = tree_branch(profile, branch_index, trunk_top);
                branch_tips.push(branch.end);
                spawn_segment(
                    tree,
                    &meshes.branch,
                    gallery_materials.bark.clone(),
                    branch,
                    Some(wind_part(
                        profile,
                        transform_for_segment(branch),
                        branch_index as f32 * 0.37,
                        0.72,
                        0.62,
                    )),
                    "ProceduralTreeBranch",
                );
            }
            branch_tips.push(trunk_top);

            for cluster_index in 0..profile.leaf_cluster_count {
                let transform = leaf_cluster_transform(profile, cluster_index, &branch_tips);
                let material = if cluster_index % 3 == 0 {
                    gallery_materials.leaf_secondary.clone()
                } else {
                    gallery_materials.leaf_primary.clone()
                };
                tree.spawn((
                    Name::new("ProceduralTreeLeafCluster"),
                    Mesh3d(meshes.leaf.clone()),
                    MeshMaterial3d(material),
                    transform,
                    wind_part(
                        profile,
                        transform,
                        cluster_index as f32 * 0.51 + 1.7,
                        1.0,
                        0.36,
                    ),
                ));
            }

            spawn_tree_shadow(tree, meshes, &gallery_materials, profile);
        });

    tracing::info!(
        target: "dao_game::objects::tree",
        seed = profile.seed,
        height = profile.height,
        branches = profile.branch_count,
        leaf_clusters = profile.leaf_cluster_count,
        leaf_r = profile.leaf_color[0],
        leaf_g = profile.leaf_color[1],
        leaf_b = profile.leaf_color[2],
        "procedural tree variant generated"
    );
}

fn tree_materials(
    materials: &mut Assets<StandardMaterial>,
    profile: ProceduralTreeProfile,
) -> TreeGalleryMaterials {
    let bark = materials.add(StandardMaterial {
        base_color: Color::srgb(
            profile.bark_color[0],
            profile.bark_color[1],
            profile.bark_color[2],
        ),
        perceptual_roughness: 0.94,
        metallic: 0.0,
        ..Default::default()
    });
    let leaf_primary = materials.add(StandardMaterial {
        base_color: Color::srgb(
            profile.leaf_color[0],
            profile.leaf_color[1],
            profile.leaf_color[2],
        ),
        perceptual_roughness: 0.74,
        metallic: 0.0,
        ..Default::default()
    });
    let leaf_secondary = materials.add(StandardMaterial {
        base_color: Color::srgb(
            (profile.leaf_color[0] * 1.14).min(0.42),
            (profile.leaf_color[1] * 0.92).max(0.22),
            (profile.leaf_color[2] * 1.08).min(0.4),
        ),
        perceptual_roughness: 0.8,
        metallic: 0.0,
        ..Default::default()
    });
    let floor = materials.add(StandardMaterial {
        base_color: Color::srgb(0.08, 0.11, 0.085),
        perceptual_roughness: 0.96,
        metallic: 0.0,
        ..Default::default()
    });
    TreeGalleryMaterials {
        floor,
        bark,
        leaf_primary,
        leaf_secondary,
    }
}

fn spawn_segment(
    parent: &mut ChildSpawnerCommands<'_>,
    mesh: &Handle<Mesh>,
    material: Handle<StandardMaterial>,
    segment: BranchSegment,
    wind: Option<ProceduralTreeWindPart>,
    name: &'static str,
) {
    let mut entity = parent.spawn((
        Name::new(name),
        Mesh3d(mesh.clone()),
        MeshMaterial3d(material),
        transform_for_segment(segment),
    ));
    if let Some(wind) = wind {
        entity.insert(wind);
    }
}

fn transform_for_segment(segment: BranchSegment) -> Transform {
    let axis = segment.end - segment.start;
    let length = axis.length().max(0.01);
    let midpoint = (segment.start + segment.end) * 0.5;
    Transform::from_translation(midpoint)
        .with_rotation(Quat::from_rotation_arc(Vec3::Y, axis / length))
        .with_scale(Vec3::new(segment.radius, length, segment.radius))
}

fn tree_branch(
    profile: ProceduralTreeProfile,
    branch_index: usize,
    trunk_top: Vec3,
) -> BranchSegment {
    let seed = profile.seed.wrapping_add(branch_index as u64 * 977);
    let height_t = lerp(0.34, 0.9, seeded_unit(seed, 101));
    let radial_angle = branch_index as f32 * 2.399_963 + seeded_signed(seed, 103) * 0.42;
    let radial = Vec3::new(radial_angle.cos(), 0.0, radial_angle.sin());
    let trunk_point = Vec3::new(
        profile.lean.x * height_t,
        profile.height * height_t,
        profile.lean.y * height_t,
    );
    let length = profile.canopy_radius * lerp(0.62, 1.12, seeded_unit(seed, 107));
    let rise = profile.height * lerp(0.08, 0.22, seeded_unit(seed, 109));
    let end = trunk_point
        + radial * length
        + Vec3::Y * rise
        + (trunk_top - trunk_point) * lerp(0.04, 0.18, seeded_unit(seed, 113));
    BranchSegment {
        start: trunk_point,
        end,
        radius: profile.trunk_radius * lerp(0.26, 0.48, seeded_unit(seed, 127)),
    }
}

fn leaf_cluster_transform(
    profile: ProceduralTreeProfile,
    cluster_index: usize,
    branch_tips: &[Vec3],
) -> Transform {
    let seed = profile.seed.wrapping_add(cluster_index as u64 * 2_653);
    let anchor = branch_tips[cluster_index % branch_tips.len()];
    let angle = cluster_index as f32 * 1.713 + seeded_signed(seed, 151) * 0.58;
    let offset = Vec3::new(angle.cos(), seeded_signed(seed, 157) * 0.28, angle.sin())
        * profile.canopy_radius
        * lerp(0.12, 0.42, seeded_unit(seed, 163));
    let squash = lerp(0.72, 1.18, seeded_unit(seed, 167));
    let scale = Vec3::new(
        profile.canopy_radius * lerp(0.42, 0.78, seeded_unit(seed, 173)),
        profile.canopy_radius * lerp(0.24, 0.5, seeded_unit(seed, 179)),
        profile.canopy_radius * lerp(0.38, 0.72, seeded_unit(seed, 181)) * squash,
    );
    Transform::from_translation(anchor + offset)
        .with_rotation(Quat::from_euler(
            EulerRot::XYZ,
            seeded_signed(seed, 191) * 0.24,
            angle,
            seeded_signed(seed, 193) * 0.16,
        ))
        .with_scale(scale)
}

fn spawn_tree_shadow(
    parent: &mut ChildSpawnerCommands<'_>,
    meshes: &TreeGalleryMeshes,
    materials: &TreeGalleryMaterials,
    profile: ProceduralTreeProfile,
) {
    parent.spawn((
        Name::new("ProceduralTreeRootShadow"),
        Mesh3d(meshes.leaf.clone()),
        MeshMaterial3d(materials.floor.clone()),
        Transform::from_xyz(profile.lean.x * 0.16, 0.025, profile.lean.y * 0.16).with_scale(
            Vec3::new(
                profile.canopy_radius * 0.95,
                0.045,
                profile.canopy_radius * 0.76,
            ),
        ),
    ));
}

fn wind_part(
    profile: ProceduralTreeProfile,
    base_transform: Transform,
    phase_offset: f32,
    amplitude_scale: f32,
    stiffness: f32,
) -> ProceduralTreeWindPart {
    ProceduralTreeWindPart {
        base_transform,
        phase: seeded_unit(profile.seed, 211) * 10.0 + phase_offset,
        amplitude: 0.045 * profile.wind_flex * amplitude_scale,
        stiffness,
    }
}

fn animate_procedural_tree_wind(
    time: Res<Time>,
    wind: Option<Res<WindField>>,
    mut query: Query<(&ProceduralTreeWindPart, &mut Transform)>,
) {
    let elapsed = time.elapsed_secs();
    let wind_direction = wind
        .as_deref()
        .map(|wind| wind.direction.normalize_or_zero())
        .filter(|direction| direction.length_squared() > f32::EPSILON)
        .unwrap_or(Vec2::new(0.72, 0.32).normalize());
    let wind_energy = wind
        .as_deref()
        .map(|wind| 0.65 + wind.speed * 0.75 + wind.gust * 0.55 + wind.swirl.abs() * 0.18)
        .unwrap_or(0.75)
        .clamp(0.25, 1.8);

    for (part, mut transform) in &mut query {
        let sway = (elapsed * (0.8 + wind_energy * 0.72) + part.phase).sin()
            * part.amplitude
            * wind_energy;
        let flutter = (elapsed * 2.15 + part.phase * 1.7).sin() * part.amplitude * 0.24;
        let bend = Quat::from_rotation_x(wind_direction.y * sway * part.stiffness)
            * Quat::from_rotation_z(-wind_direction.x * sway * part.stiffness);
        *transform = part.base_transform;
        transform.rotation = bend * part.base_transform.rotation;
        transform.translation +=
            Vec3::new(wind_direction.x, flutter.abs() * 0.12, wind_direction.y) * sway.abs() * 0.28;
    }
}

fn cleanup_procedural_object_gallery(
    mut commands: Commands,
    roots: Query<Entity, With<ProceduralObjectGalleryRoot>>,
) {
    for entity in &roots {
        commands.entity(entity).despawn();
    }
}

fn seeded_unit(seed: u64, salt: u64) -> f32 {
    let mut value = seed
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(salt.wrapping_mul(0xBF58_476D_1CE4_E5B9));
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^= value >> 31;
    (value as f64 / u64::MAX as f64) as f32
}

fn seeded_signed(seed: u64, salt: u64) -> f32 {
    seeded_unit(seed, salt) * 2.0 - 1.0
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

#[cfg(test)]
mod tests {
    use super::{TREE_GALLERY_BASE_SEED, procedural_tree_profile};

    #[test]
    fn tree_profiles_are_deterministic_for_same_seed() {
        let first = procedural_tree_profile(TREE_GALLERY_BASE_SEED);
        let second = procedural_tree_profile(TREE_GALLERY_BASE_SEED);

        assert_eq!(first, second);
    }

    #[test]
    fn tree_profiles_vary_shape_and_leaf_color() {
        let first = procedural_tree_profile(TREE_GALLERY_BASE_SEED);
        let second = procedural_tree_profile(TREE_GALLERY_BASE_SEED + 97);

        assert_ne!(first.height, second.height);
        assert_ne!(first.leaf_color, second.leaf_color);
        assert_ne!(first.branch_count, second.branch_count);
    }

    #[test]
    fn tree_profile_bounds_support_close_gallery_review() {
        for index in 0..32 {
            let profile = procedural_tree_profile(TREE_GALLERY_BASE_SEED + index);

            assert!((4.8..=8.8).contains(&profile.height));
            assert!((0.22..=0.48).contains(&profile.trunk_radius));
            assert!((5..=10).contains(&profile.branch_count));
            assert!(profile.leaf_cluster_count >= profile.branch_count + 4);
            assert!((0.55..=1.25).contains(&profile.wind_flex));
        }
    }
}
