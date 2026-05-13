use std::time::Instant;

use bevy::prelude::*;

use crate::game::assets::ProceduralMaterialFamily;

use super::super::{
    GeneratedObjectPart, GeneratedObjectProfile, GeneratedObjectStats, ObjectAnimationRecipe,
    ObjectBiomeContext, ObjectCollisionMode, ObjectCollisionRecipe, ObjectFamilyDefinition,
    ObjectGeneratedAsset, ObjectGenerationRequest, ObjectKind, ObjectLod, ObjectMaterialBinding,
    ObjectMaterialSlot, ObjectMeshRecipe, ObjectSemantic, ObjectWeatherState,
    ProceduralRuinFragmentProfile, TreeRootBlockerRecipe, TreeSensorRecipe,
    TreeTrunkColliderRecipe, seeded_signed, seeded_unit, stable_object_id,
};

pub(crate) const PROFILE_VERSION: u32 = 1;
pub(crate) const GEOMETRY_VERSION: u32 = 1;
pub(crate) const GALLERY_BASE_SEED: u64 = 0xA11C_EE05_13AA_7B11;

const GOLDEN_SEEDS: [u64; 5] = [
    GALLERY_BASE_SEED,
    GALLERY_BASE_SEED + 19,
    GALLERY_BASE_SEED + 71,
    GALLERY_BASE_SEED + 307,
    GALLERY_BASE_SEED + 1_127,
];

pub(crate) fn definition() -> ObjectFamilyDefinition {
    ObjectFamilyDefinition {
        kind: ObjectKind::RuinFragment,
        profile_version: PROFILE_VERSION,
        geometry_version: GEOMETRY_VERSION,
        semantics: vec![
            ObjectSemantic::Stone,
            ObjectSemantic::Ruin,
            ObjectSemantic::Omen,
        ],
        material_slots: vec![
            ObjectMaterialBinding {
                slot: ObjectMaterialSlot::RuinCore,
                material_family: ProceduralMaterialFamily::OldStone,
                material_id: "dao/mat/ruin-stone/v1",
            },
            ObjectMaterialBinding {
                slot: ObjectMaterialSlot::RuinEdge,
                material_family: ProceduralMaterialFamily::Relic,
                material_id: "dao/mat/relic-metal/v1",
            },
            ObjectMaterialBinding {
                slot: ObjectMaterialSlot::RuinDust,
                material_family: ProceduralMaterialFamily::Sand,
                material_id: "dao/mat/dry-sand/v1",
            },
            ObjectMaterialBinding {
                slot: ObjectMaterialSlot::RuinMoss,
                material_family: ProceduralMaterialFamily::GroveLeaf,
                material_id: "dao/mat/moss/v1",
            },
            ObjectMaterialBinding {
                slot: ObjectMaterialSlot::RuinShadow,
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
        collider_count: collision.trunk.iter().count()
            + collision.root_blockers.len()
            + collision.sensor.iter().count(),
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
        profile: GeneratedObjectProfile::RuinFragment(profile),
        material_slots: family.material_slots.clone(),
        parts,
        collision,
        animation,
        stats,
    }
}

pub(crate) fn build_profile(request: &ObjectGenerationRequest) -> ProceduralRuinFragmentProfile {
    let seed = request.seed;
    let size_scale = match request.lod {
        ObjectLod::Near => 1.0,
        ObjectLod::Mid => 0.82,
        ObjectLod::Far => 0.68,
    };
    let width = (1.6 + seeded_unit(seed, 11) * 2.4) * size_scale;
    let depth = (0.7 + seeded_unit(seed, 13) * 1.4) * size_scale;
    let height = (1.2 + seeded_unit(seed, 17) * 3.1)
        * size_scale
        * match request.biome {
            ObjectBiomeContext::RuinEdge => 1.08,
            ObjectBiomeContext::DesertWind => 0.92,
            _ => 1.0,
        };
    let tilt = Vec2::new(seeded_signed(seed, 19), seeded_signed(seed, 23)) * 0.22;
    let fracture = (0.18 + seeded_unit(seed, 29) * 0.74).clamp(0.0, 1.0);
    let erosion = (0.14
        + seeded_unit(seed, 31) * 0.78
        + if matches!(request.weather, ObjectWeatherState::DryWind) {
            0.08
        } else {
            0.0
        })
    .clamp(0.0, 1.0);
    let sand_cover = (0.06
        + seeded_unit(seed, 37) * 0.72
        + match request.biome {
            ObjectBiomeContext::DesertWind => 0.22,
            ObjectBiomeContext::Wetland => -0.12,
            _ => 0.0,
        })
    .clamp(0.0, 1.0);
    let moss_ratio = (0.04
        + seeded_unit(seed, 41) * 0.62
        + match request.biome {
            ObjectBiomeContext::Wetland => 0.22,
            ObjectBiomeContext::DesertWind => -0.2,
            _ => 0.0,
        })
    .clamp(0.0, 0.88);
    let relic_ratio = (0.08 + seeded_unit(seed, 43) * 0.7).clamp(0.0, 1.0);
    let column_count = match request.lod {
        ObjectLod::Near => 1 + (seeded_unit(seed, 47) * 2.0).round() as usize,
        ObjectLod::Mid => 1 + (seeded_unit(seed, 47) * 1.0).round() as usize,
        ObjectLod::Far => 1,
    };
    let debris_count = match request.lod {
        ObjectLod::Near => 3 + (seeded_unit(seed, 53) * 4.0).round() as usize,
        ObjectLod::Mid => 2 + (seeded_unit(seed, 53) * 2.0).round() as usize,
        ObjectLod::Far => 1,
    };
    let collider_radius = (width.max(depth) * 0.42).max(0.34);
    let collider_height = (height * 0.84).max(0.9);

    ProceduralRuinFragmentProfile {
        seed,
        biome: request.biome,
        width,
        depth,
        height,
        tilt,
        fracture,
        erosion,
        sand_cover,
        moss_ratio,
        relic_ratio,
        column_count,
        debris_count,
        collider_radius,
        collider_height,
    }
}

fn build_parts(
    request: &ObjectGenerationRequest,
    profile: ProceduralRuinFragmentProfile,
) -> Vec<GeneratedObjectPart> {
    let mut parts = Vec::new();

    parts.push(GeneratedObjectPart {
        name: "RuinWallCore".to_string(),
        recipe: ObjectMeshRecipe::Cuboid,
        slot: ObjectMaterialSlot::RuinCore,
        local_transform: Transform::from_xyz(0.0, profile.height * 0.46, 0.0)
            .with_rotation(Quat::from_euler(
                EulerRot::XYZ,
                profile.tilt.x,
                seeded_signed(profile.seed, 59) * 0.4,
                profile.tilt.y,
            ))
            .with_scale(Vec3::new(
                profile.width,
                profile.height,
                profile.depth.max(0.24),
            )),
        wind_band: None,
    });

    if request.lod != ObjectLod::Far {
        parts.push(GeneratedObjectPart {
            name: "RuinBrokenLedge".to_string(),
            recipe: ObjectMeshRecipe::Cuboid,
            slot: if profile.relic_ratio > 0.52 {
                ObjectMaterialSlot::RuinEdge
            } else {
                ObjectMaterialSlot::RuinCore
            },
            local_transform: Transform::from_xyz(
                profile.width * 0.12,
                profile.height * 0.76,
                seeded_signed(profile.seed, 61) * profile.depth * 0.24,
            )
            .with_rotation(Quat::from_euler(
                EulerRot::XYZ,
                seeded_signed(profile.seed, 67) * 0.34,
                seeded_signed(profile.seed, 71) * 0.48,
                seeded_signed(profile.seed, 73) * 0.28,
            ))
            .with_scale(Vec3::new(
                profile.width * 0.52,
                (profile.height * 0.12).max(0.08),
                profile.depth * 0.88,
            )),
            wind_band: None,
        });
    }

    for column_index in 0..profile.column_count.max(1) {
        let seed = profile.seed.wrapping_add(column_index as u64 * 79);
        let side = if column_index % 2 == 0 { -1.0 } else { 1.0 };
        parts.push(GeneratedObjectPart {
            name: format!("RuinColumn{column_index:02}"),
            recipe: ObjectMeshRecipe::Cylinder,
            slot: ObjectMaterialSlot::RuinCore,
            local_transform: Transform::from_xyz(
                side * profile.width * 0.42,
                profile.height * (0.28 + seeded_unit(seed, 83) * 0.18),
                seeded_signed(seed, 89) * profile.depth * 0.2,
            )
            .with_rotation(Quat::from_euler(
                EulerRot::XYZ,
                seeded_signed(seed, 97) * 0.12,
                seeded_signed(seed, 101) * 0.24,
                seeded_signed(seed, 103) * 0.12,
            ))
            .with_scale(Vec3::new(
                (profile.width * 0.12).max(0.08),
                profile.height * (0.4 + seeded_unit(seed, 107) * 0.18),
                (profile.width * 0.12).max(0.08),
            )),
            wind_band: None,
        });
    }

    for debris_index in 0..profile.debris_count.max(1) {
        let seed = profile.seed.wrapping_add(debris_index as u64 * 131);
        let angle = debris_index as f32
            * (std::f32::consts::TAU / profile.debris_count.max(1) as f32)
            + seeded_signed(seed, 109) * 0.45;
        parts.push(GeneratedObjectPart {
            name: format!("RuinDebris{debris_index:02}"),
            recipe: if request.lod == ObjectLod::Near {
                ObjectMeshRecipe::Cuboid
            } else {
                ObjectMeshRecipe::Sphere
            },
            slot: if debris_index % 3 == 0 && profile.relic_ratio > 0.48 {
                ObjectMaterialSlot::RuinEdge
            } else {
                ObjectMaterialSlot::RuinCore
            },
            local_transform: Transform::from_xyz(
                angle.cos() * profile.width * (0.4 + seeded_unit(seed, 113) * 0.22),
                profile.height * (0.06 + seeded_unit(seed, 127) * 0.1),
                angle.sin() * profile.width * (0.28 + seeded_unit(seed, 131) * 0.22),
            )
            .with_rotation(Quat::from_euler(
                EulerRot::XYZ,
                seeded_signed(seed, 137) * 0.44,
                seeded_signed(seed, 139) * 0.72,
                seeded_signed(seed, 149) * 0.44,
            ))
            .with_scale(Vec3::new(
                profile.width * (0.12 + seeded_unit(seed, 151) * 0.14),
                profile.height * (0.08 + seeded_unit(seed, 157) * 0.12),
                profile.depth * (0.24 + seeded_unit(seed, 163) * 0.24),
            )),
            wind_band: None,
        });
    }

    if profile.sand_cover > 0.12 {
        parts.push(GeneratedObjectPart {
            name: "RuinDustApron".to_string(),
            recipe: ObjectMeshRecipe::Cuboid,
            slot: ObjectMaterialSlot::RuinDust,
            local_transform: Transform::from_xyz(0.0, 0.05, 0.0).with_scale(Vec3::new(
                profile.width * 0.96,
                0.06,
                profile.depth * 1.3,
            )),
            wind_band: None,
        });
    }

    if profile.moss_ratio > 0.18 && request.lod != ObjectLod::Far {
        parts.push(GeneratedObjectPart {
            name: "RuinMossPatch".to_string(),
            recipe: ObjectMeshRecipe::Sphere,
            slot: ObjectMaterialSlot::RuinMoss,
            local_transform: Transform::from_xyz(
                seeded_signed(profile.seed, 173) * profile.width * 0.24,
                profile.height * 0.38,
                seeded_signed(profile.seed, 179) * profile.depth * 0.3,
            )
            .with_scale(Vec3::new(
                profile.width * 0.22,
                profile.height * 0.12,
                profile.depth * 0.28,
            )),
            wind_band: None,
        });
    }

    parts.push(GeneratedObjectPart {
        name: "RuinGroundShadow".to_string(),
        recipe: ObjectMeshRecipe::Sphere,
        slot: ObjectMaterialSlot::RuinShadow,
        local_transform: Transform::from_xyz(0.0, 0.02, 0.0).with_scale(Vec3::new(
            profile.width * 0.88,
            0.04,
            profile.width * 0.72,
        )),
        wind_band: None,
    });

    parts
}

fn build_collision(
    request: &ObjectGenerationRequest,
    profile: ProceduralRuinFragmentProfile,
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
        let blocker_count = profile.debris_count.clamp(1, 3);
        (0..blocker_count)
            .map(|index| {
                let seed = profile.seed.wrapping_add(index as u64 * 191);
                let angle = index as f32 * (std::f32::consts::TAU / blocker_count as f32)
                    + seeded_signed(seed, 181) * 0.28;
                TreeRootBlockerRecipe {
                    center: Vec3::new(
                        angle.cos() * profile.width * 0.28,
                        profile.collider_height * 0.18,
                        angle.sin() * profile.width * 0.22,
                    ),
                    half_extents: Vec3::new(
                        profile.collider_radius * 0.36,
                        profile.collider_height * 0.14,
                        profile.collider_radius * 0.28,
                    ),
                    yaw: angle,
                }
            })
            .collect()
    };
    let sensor =
        if request.collision_mode == ObjectCollisionMode::Full && profile.relic_ratio > 0.52 {
            Some(TreeSensorRecipe {
                center: Vec3::new(0.0, profile.height * 0.56, 0.0),
                half_extents: Vec3::new(
                    profile.width * 0.36,
                    profile.height * 0.16,
                    profile.depth.max(0.24) * 0.7,
                ),
            })
        } else {
            None
        };

    ObjectCollisionRecipe {
        trunk,
        root_blockers,
        sensor,
    }
}
