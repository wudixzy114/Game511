use std::time::Instant;

use bevy::prelude::*;

use crate::game::assets::ProceduralMaterialFamily;

use super::super::{
    GeneratedObjectPart, GeneratedObjectProfile, GeneratedObjectStats, ObjectAnimationRecipe,
    ObjectBiomeContext, ObjectCollisionMode, ObjectCollisionRecipe, ObjectFamilyDefinition,
    ObjectGeneratedAsset, ObjectGenerationRequest, ObjectKind, ObjectLod, ObjectMaterialBinding,
    ObjectMaterialSlot, ObjectMeshRecipe, ObjectSemantic, ObjectWeatherState,
    ProceduralRockProfile, TreeRootBlockerRecipe, TreeTrunkColliderRecipe, seeded_signed,
    seeded_unit, stable_object_id,
};

pub(crate) const PROFILE_VERSION: u32 = 1;
pub(crate) const GEOMETRY_VERSION: u32 = 1;
pub(crate) const GALLERY_BASE_SEED: u64 = 0xA11C_EE05_13AA_7A77;

const GOLDEN_SEEDS: [u64; 5] = [
    GALLERY_BASE_SEED,
    GALLERY_BASE_SEED + 41,
    GALLERY_BASE_SEED + 97,
    GALLERY_BASE_SEED + 353,
    GALLERY_BASE_SEED + 1_009,
];

pub(crate) fn definition() -> ObjectFamilyDefinition {
    ObjectFamilyDefinition {
        kind: ObjectKind::Rock,
        profile_version: PROFILE_VERSION,
        geometry_version: GEOMETRY_VERSION,
        semantics: vec![ObjectSemantic::Stone, ObjectSemantic::Waterside],
        material_slots: vec![
            ObjectMaterialBinding {
                slot: ObjectMaterialSlot::RockPrimary,
                material_family: ProceduralMaterialFamily::Stone,
                material_id: "dao/mat/stone/v1",
            },
            ObjectMaterialBinding {
                slot: ObjectMaterialSlot::RockStrata,
                material_family: ProceduralMaterialFamily::OldStone,
                material_id: "dao/mat/ruin-stone/v1",
            },
            ObjectMaterialBinding {
                slot: ObjectMaterialSlot::RockWet,
                material_family: ProceduralMaterialFamily::Water,
                material_id: "dao/mat/wetline/v1",
            },
            ObjectMaterialBinding {
                slot: ObjectMaterialSlot::RockMoss,
                material_family: ProceduralMaterialFamily::GroveLeaf,
                material_id: "dao/mat/moss/v1",
            },
            ObjectMaterialBinding {
                slot: ObjectMaterialSlot::RockShadow,
                material_family: ProceduralMaterialFamily::Shadow,
                material_id: "dao/mat/object-shadow/v1",
            },
        ],
        golden_seeds: GOLDEN_SEEDS.to_vec(),
    }
}

pub(crate) fn generate_asset(
    request: ObjectGenerationRequest,
    family: &ObjectFamilyDefinition,
) -> ObjectGeneratedAsset {
    let started_at = Instant::now();
    let profile = build_profile(&request);
    let mut parts = build_parts(&request, profile);
    let collision = build_collision(&request, profile);

    let vertex_estimate = parts
        .iter()
        .map(|part| match part.recipe {
            ObjectMeshRecipe::Sphere => 282_usize,
            ObjectMeshRecipe::Cuboid => 36_usize,
            ObjectMeshRecipe::Cylinder => 44_usize,
        })
        .sum();

    let stats = GeneratedObjectStats {
        part_count: parts.len(),
        mesh_count: parts.len(),
        vertex_estimate,
        collider_count: collision.trunk.iter().count() + collision.root_blockers.len(),
        generation_ms: started_at.elapsed().as_secs_f32() * 1000.0,
    };

    let stable_id = stable_object_id(
        request.kind,
        request.seed,
        family.profile_version,
        family.geometry_version,
    );
    let animation = ObjectAnimationRecipe {
        trunk_parts: 0,
        branch_parts: 0,
        leaf_parts: 0,
        uses_gust_response: false,
    };

    parts.shrink_to_fit();
    ObjectGeneratedAsset {
        kind: request.kind,
        seed: request.seed,
        stable_id,
        lod: request.lod,
        profile_version: family.profile_version,
        geometry_version: family.geometry_version,
        request,
        profile: GeneratedObjectProfile::Rock(profile),
        material_slots: family.material_slots.clone(),
        parts,
        collision,
        animation,
        stats,
    }
}

pub(crate) fn build_profile(request: &ObjectGenerationRequest) -> ProceduralRockProfile {
    let seed = request.seed;
    let size_scale = match request.lod {
        ObjectLod::Near => 1.0,
        ObjectLod::Mid => 0.82,
        ObjectLod::Far => 0.64,
    };
    let base_radius = (0.82 + seeded_unit(seed, 11) * 1.42) * size_scale;
    let height = (0.36 + seeded_unit(seed, 13) * 1.18)
        * size_scale
        * match request.biome {
            ObjectBiomeContext::Ridge => 1.08,
            ObjectBiomeContext::Wetland => 0.86,
            _ => 1.0,
        };
    let elongation = 0.82 + seeded_unit(seed, 17) * 0.74;
    let flatten = 0.34 + seeded_unit(seed, 19) * 0.42;
    let tilt = Vec2::new(seeded_signed(seed, 23), seeded_signed(seed, 29)) * 0.34;
    let strata_strength = (0.24 + seeded_unit(seed, 31) * 0.66).clamp(0.0, 1.0);
    let crack_density = (0.08 + seeded_unit(seed, 37) * 0.74).clamp(0.0, 1.0);
    let erosion = (0.16 + seeded_unit(seed, 41) * 0.72).clamp(0.0, 1.0);
    let wet_line = (0.08
        + seeded_unit(seed, 43) * 0.64
        + if matches!(request.weather, ObjectWeatherState::RainSoaked) {
            0.22
        } else {
            0.0
        })
    .clamp(0.0, 1.0);
    let moss_ratio = (0.04
        + seeded_unit(seed, 47) * 0.7
        + match request.biome {
            ObjectBiomeContext::Wetland => 0.18,
            ObjectBiomeContext::DesertWind => -0.2,
            _ => 0.0,
        })
    .clamp(0.0, 0.92);
    let shard_count = match request.lod {
        ObjectLod::Near => 2 + (seeded_unit(seed, 53) * 4.0).round() as usize,
        ObjectLod::Mid => 1 + (seeded_unit(seed, 53) * 2.0).round() as usize,
        ObjectLod::Far => 1,
    };
    let collider_radius = (base_radius * 0.72).max(0.3);
    let collider_height = (height * 1.1).max(0.45);
    ProceduralRockProfile {
        seed,
        biome: request.biome,
        base_radius,
        height,
        elongation,
        flatten,
        tilt,
        strata_strength,
        crack_density,
        erosion,
        wet_line,
        moss_ratio,
        shard_count,
        collider_radius,
        collider_height,
    }
}

fn build_parts(
    request: &ObjectGenerationRequest,
    profile: ProceduralRockProfile,
) -> Vec<GeneratedObjectPart> {
    let mut parts = Vec::new();

    parts.push(GeneratedObjectPart {
        name: "RockCore".to_string(),
        recipe: ObjectMeshRecipe::Sphere,
        slot: ObjectMaterialSlot::RockPrimary,
        local_transform: Transform::from_xyz(0.0, profile.height * 0.48, 0.0)
            .with_rotation(Quat::from_euler(
                EulerRot::XYZ,
                profile.tilt.x * 0.22,
                seeded_signed(profile.seed, 59) * 0.6,
                profile.tilt.y * 0.22,
            ))
            .with_scale(Vec3::new(
                profile.base_radius * profile.elongation,
                profile.height,
                profile.base_radius,
            )),
        wind_band: None,
    });

    if request.lod != ObjectLod::Far {
        parts.push(GeneratedObjectPart {
            name: "RockStrataBand".to_string(),
            recipe: ObjectMeshRecipe::Cuboid,
            slot: ObjectMaterialSlot::RockStrata,
            local_transform: Transform::from_xyz(0.0, profile.height * 0.42, 0.0)
                .with_rotation(Quat::from_rotation_y(seeded_signed(profile.seed, 61) * 0.6))
                .with_scale(Vec3::new(
                    profile.base_radius * 1.38,
                    (profile.height * 0.12).max(0.06),
                    profile.base_radius * 1.12,
                )),
            wind_band: None,
        });
    }

    if profile.wet_line > 0.14 {
        parts.push(GeneratedObjectPart {
            name: "RockWetLine".to_string(),
            recipe: ObjectMeshRecipe::Sphere,
            slot: ObjectMaterialSlot::RockWet,
            local_transform: Transform::from_xyz(0.0, profile.height * 0.22, 0.0).with_scale(
                Vec3::new(
                    profile.base_radius * 1.08,
                    (profile.height * 0.26).max(0.08),
                    profile.base_radius * 1.04,
                ),
            ),
            wind_band: None,
        });
    }

    if profile.moss_ratio > 0.16 && request.lod != ObjectLod::Far {
        parts.push(GeneratedObjectPart {
            name: "RockMossPatch".to_string(),
            recipe: ObjectMeshRecipe::Sphere,
            slot: ObjectMaterialSlot::RockMoss,
            local_transform: Transform::from_xyz(
                seeded_signed(profile.seed, 67) * profile.base_radius * 0.42,
                profile.height * 0.56,
                seeded_signed(profile.seed, 71) * profile.base_radius * 0.42,
            )
            .with_scale(Vec3::new(
                profile.base_radius * 0.44,
                profile.height * 0.22,
                profile.base_radius * 0.4,
            )),
            wind_band: None,
        });
    }

    let shard_count = profile.shard_count.max(1);
    for index in 0..shard_count {
        let seed = profile.seed.wrapping_add(79 + index as u64 * 43);
        let angle = index as f32 * (std::f32::consts::TAU / shard_count as f32)
            + seeded_signed(seed, 83) * 0.45;
        let radius = profile.base_radius * (0.42 + seeded_unit(seed, 89) * 0.22);
        let shard_height = profile.height * (0.18 + seeded_unit(seed, 97) * 0.22);
        parts.push(GeneratedObjectPart {
            name: format!("RockShard{index:02}"),
            recipe: if request.lod == ObjectLod::Near {
                ObjectMeshRecipe::Cuboid
            } else {
                ObjectMeshRecipe::Sphere
            },
            slot: ObjectMaterialSlot::RockPrimary,
            local_transform: Transform::from_xyz(
                angle.cos() * radius,
                shard_height,
                angle.sin() * radius,
            )
            .with_rotation(Quat::from_euler(
                EulerRot::XYZ,
                seeded_signed(seed, 101) * 0.34,
                angle,
                seeded_signed(seed, 103) * 0.34,
            ))
            .with_scale(Vec3::new(
                profile.base_radius * (0.18 + seeded_unit(seed, 107) * 0.2),
                profile.height * (0.24 + seeded_unit(seed, 109) * 0.28),
                profile.base_radius * (0.18 + seeded_unit(seed, 113) * 0.2),
            )),
            wind_band: None,
        });
    }

    parts.push(GeneratedObjectPart {
        name: "RockGroundShadow".to_string(),
        recipe: ObjectMeshRecipe::Sphere,
        slot: ObjectMaterialSlot::RockShadow,
        local_transform: Transform::from_xyz(0.0, 0.02, 0.0).with_scale(Vec3::new(
            profile.base_radius * 1.15,
            0.04,
            profile.base_radius * 0.95,
        )),
        wind_band: None,
    });

    parts
}

fn build_collision(
    request: &ObjectGenerationRequest,
    profile: ProceduralRockProfile,
) -> ObjectCollisionRecipe {
    if request.collision_mode == ObjectCollisionMode::VisualOnly || request.lod == ObjectLod::Far {
        return ObjectCollisionRecipe {
            trunk: None,
            root_blockers: Vec::new(),
            sensor: None,
        };
    }

    let trunk = Some(TreeTrunkColliderRecipe {
        radius: profile.collider_radius,
        height: profile.collider_height,
    });
    let root_blockers = if request.collision_mode == ObjectCollisionMode::TrunkOnly {
        Vec::new()
    } else {
        let blocker_count = if request.collision_mode == ObjectCollisionMode::Full {
            3
        } else {
            2
        };
        (0..blocker_count)
            .map(|index| {
                let seed = profile.seed.wrapping_add(index as u64 * 131);
                let angle = index as f32 * (std::f32::consts::TAU / blocker_count as f32)
                    + seeded_signed(seed, 127) * 0.3;
                TreeRootBlockerRecipe {
                    center: Vec3::new(
                        angle.cos() * profile.base_radius * 0.34,
                        profile.collider_height * 0.3,
                        angle.sin() * profile.base_radius * 0.34,
                    ),
                    half_extents: Vec3::new(
                        profile.collider_radius * 0.4,
                        profile.collider_height * 0.24,
                        profile.collider_radius * 0.36,
                    ),
                    yaw: angle,
                }
            })
            .collect()
    };
    ObjectCollisionRecipe {
        trunk,
        root_blockers,
        sensor: None,
    }
}
