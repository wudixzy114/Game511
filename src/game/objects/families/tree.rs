use std::time::Instant;

use bevy::prelude::*;

use crate::game::assets::ProceduralMaterialFamily;

use super::super::{
    GeneratedObjectPart, GeneratedObjectProfile, GeneratedObjectStats, ObjectAnimationRecipe,
    ObjectBiomeContext, ObjectCollisionMode, ObjectCollisionRecipe, ObjectFamilyDefinition,
    ObjectGeneratedAsset, ObjectGenerationRequest, ObjectKind, ObjectLod, ObjectMaterialBinding,
    ObjectMaterialSlot, ObjectMeshRecipe, ObjectSemantic, ObjectWeatherState,
    ProceduralTreeProfile, TreeRootBlockerRecipe, TreeSegment, TreeSegmentMetrics,
    TreeSensorRecipe, TreeTrunkColliderRecipe, TreeWindBand, lerp, seeded_signed, seeded_unit,
    stable_object_id, transform_for_segment,
};

pub(crate) const PROFILE_VERSION: u32 = 2;
pub(crate) const GEOMETRY_VERSION: u32 = 2;
pub(crate) const GALLERY_BASE_SEED: u64 = 0xA11C_EE05_13AA_700D;

const GOLDEN_SEEDS: [u64; 5] = [
    GALLERY_BASE_SEED,
    GALLERY_BASE_SEED + 97,
    GALLERY_BASE_SEED + 211,
    GALLERY_BASE_SEED + 977,
    GALLERY_BASE_SEED + 4_099,
];

pub(crate) fn definition() -> ObjectFamilyDefinition {
    ObjectFamilyDefinition {
        kind: ObjectKind::Tree,
        profile_version: PROFILE_VERSION,
        geometry_version: GEOMETRY_VERSION,
        semantics: vec![
            ObjectSemantic::Vegetation,
            ObjectSemantic::Ecology,
            ObjectSemantic::Waterside,
        ],
        material_slots: vec![
            ObjectMaterialBinding {
                slot: ObjectMaterialSlot::BarkPrimary,
                material_family: ProceduralMaterialFamily::Wood,
                material_id: "dao/mat/old-wood/v1",
            },
            ObjectMaterialBinding {
                slot: ObjectMaterialSlot::BarkWornEdge,
                material_family: ProceduralMaterialFamily::OldStone,
                material_id: "dao/mat/ruin-stone/v1",
            },
            ObjectMaterialBinding {
                slot: ObjectMaterialSlot::LeafPrimary,
                material_family: ProceduralMaterialFamily::GroveLeaf,
                material_id: "dao/mat/meadow/v1",
            },
            ObjectMaterialBinding {
                slot: ObjectMaterialSlot::LeafSecondary,
                material_family: ProceduralMaterialFamily::GroveLeaf,
                material_id: "dao/mat/reed/v1",
            },
            ObjectMaterialBinding {
                slot: ObjectMaterialSlot::LeafDry,
                material_family: ProceduralMaterialFamily::Sand,
                material_id: "dao/mat/dry-sand/v1",
            },
            ObjectMaterialBinding {
                slot: ObjectMaterialSlot::RootShadow,
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

    let trunk_parts = parts
        .iter()
        .filter(|part| part.wind_band == Some(TreeWindBand::Trunk))
        .count();
    let branch_parts = parts
        .iter()
        .filter(|part| part.wind_band == Some(TreeWindBand::Branch))
        .count();
    let leaf_parts = parts
        .iter()
        .filter(|part| part.wind_band == Some(TreeWindBand::Leaf))
        .count();
    let vertex_estimate = parts
        .iter()
        .map(|part| match part.recipe {
            ObjectMeshRecipe::Cylinder => 44_usize,
            ObjectMeshRecipe::Sphere => 282_usize,
            ObjectMeshRecipe::Cuboid => 36_usize,
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
        trunk_parts,
        branch_parts,
        leaf_parts,
        uses_gust_response: request.lod != ObjectLod::Far,
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
        profile: GeneratedObjectProfile::Tree(profile),
        material_slots: family.material_slots.clone(),
        parts,
        collision,
        animation,
        stats,
    }
}

pub(crate) fn build_profile(request: &ObjectGenerationRequest) -> ProceduralTreeProfile {
    let seed = request.seed;
    let age_years = lerp(16.0, 132.0, seeded_unit(seed, 11));
    let biome_height_boost = match request.biome {
        ObjectBiomeContext::Meadow => 1.0,
        ObjectBiomeContext::Wetland => 1.08,
        ObjectBiomeContext::Ridge => 0.9,
        ObjectBiomeContext::VillageCourtyard => 0.84,
        ObjectBiomeContext::RuinEdge => 0.78,
        ObjectBiomeContext::DesertWind => 0.72,
    };
    let weather_health_bias = match request.weather {
        ObjectWeatherState::Clear => 0.05,
        ObjectWeatherState::RainSoaked => 0.12,
        ObjectWeatherState::DryWind => -0.16,
        ObjectWeatherState::DreamTint => -0.04,
    };
    let height = lerp(5.2, 10.8, seeded_unit(seed, 13)) * biome_height_boost;
    let trunk_base_radius = lerp(0.22, 0.58, seeded_unit(seed, 17)) * (0.85 + age_years / 220.0);
    let lean_angle = seeded_signed(seed, 19) * 0.52;
    let lean_distance = lerp(0.08, 0.34, seeded_unit(seed, 23)) * height;
    let biome_wind_bias = match request.biome {
        ObjectBiomeContext::Ridge | ObjectBiomeContext::DesertWind => 1.25,
        ObjectBiomeContext::Wetland => 0.86,
        _ => 1.0,
    };
    let lean = Vec2::new(lean_angle.cos(), lean_angle.sin()) * lean_distance * biome_wind_bias;
    let branch_count = 6 + (seeded_unit(seed, 29) * 7.0).floor() as usize;
    let branch_tiers = if seeded_unit(seed, 31) > 0.56 { 3 } else { 2 };
    let canopy_radius = lerp(1.85, 3.55, seeded_unit(seed, 37))
        * (0.72 + seeded_unit(seed, 41) * 0.45)
        * match request.biome {
            ObjectBiomeContext::VillageCourtyard => 0.86,
            ObjectBiomeContext::Wetland => 1.14,
            ObjectBiomeContext::DesertWind => 0.74,
            _ => 1.0,
        };
    let canopy_eccentricity = Vec2::new(
        seeded_signed(seed, 43) * canopy_radius * 0.34,
        seeded_signed(seed, 47) * canopy_radius * 0.34,
    );
    let leaf_density = lerp(0.48, 1.34, seeded_unit(seed, 53))
        * match request.weather {
            ObjectWeatherState::RainSoaked => 1.12,
            ObjectWeatherState::DryWind => 0.76,
            _ => 1.0,
        };
    let leaf_cluster_count = (branch_count as f32
        * lerp(1.24, 2.05, seeded_unit(seed, 59))
        * leaf_density.clamp(0.55, 1.28))
    .round()
    .max(8.0) as usize;
    let health = (lerp(0.38, 0.96, seeded_unit(seed, 61)) + weather_health_bias).clamp(0.15, 0.98);
    let dead_branch_ratio = (lerp(0.04, 0.36, seeded_unit(seed, 67))
        + (1.0 - health) * 0.38
        + match request.weather {
            ObjectWeatherState::DryWind => 0.16,
            _ => 0.0,
        })
    .clamp(0.02, 0.76);
    let root_exposure = lerp(0.1, 0.74, seeded_unit(seed, 71))
        * match request.biome {
            ObjectBiomeContext::Ridge | ObjectBiomeContext::RuinEdge => 1.22,
            ObjectBiomeContext::Wetland => 0.62,
            _ => 1.0,
        };
    let moss_ratio = (lerp(0.02, 0.62, seeded_unit(seed, 73))
        + match request.biome {
            ObjectBiomeContext::Wetland => 0.24,
            ObjectBiomeContext::DesertWind => -0.28,
            _ => 0.0,
        })
    .clamp(0.0, 0.72);
    let dryness = match request.weather {
        ObjectWeatherState::DryWind => 0.86,
        ObjectWeatherState::RainSoaked => 0.22,
        ObjectWeatherState::DreamTint => 0.52,
        ObjectWeatherState::Clear => 0.42,
    };
    let dream_shift = if request.weather == ObjectWeatherState::DreamTint {
        0.08
    } else {
        0.0
    };
    let leaf_color = [
        (0.16 + seeded_unit(seed, 79) * 0.12 + dryness * 0.12 + dream_shift).clamp(0.08, 0.42),
        (0.38 + seeded_unit(seed, 83) * 0.28 - dryness * 0.15 - dream_shift * 0.4)
            .clamp(0.14, 0.72),
        (0.12 + seeded_unit(seed, 89) * 0.14 - dryness * 0.08 + dream_shift).clamp(0.05, 0.44),
    ];
    let bark_color = [
        (0.2 + seeded_unit(seed, 97) * 0.18 + moss_ratio * 0.08).clamp(0.14, 0.46),
        (0.14 + seeded_unit(seed, 101) * 0.11 + moss_ratio * 0.07).clamp(0.08, 0.34),
        (0.08 + seeded_unit(seed, 103) * 0.08 + moss_ratio * 0.04).clamp(0.04, 0.26),
    ];
    let wind_flex = (lerp(0.48, 1.35, seeded_unit(seed, 107))
        * match request.biome {
            ObjectBiomeContext::Ridge | ObjectBiomeContext::DesertWind => 1.24,
            ObjectBiomeContext::VillageCourtyard => 0.84,
            _ => 1.0,
        })
    .clamp(0.34, 1.7);

    ProceduralTreeProfile {
        seed,
        biome: request.biome,
        age_years,
        health,
        height,
        trunk_base_radius,
        lean,
        branch_count,
        branch_tiers,
        leaf_cluster_count,
        canopy_radius,
        canopy_eccentricity,
        leaf_density,
        dead_branch_ratio,
        root_exposure,
        moss_ratio,
        leaf_color,
        bark_color,
        wind_flex,
    }
}

fn build_parts(
    request: &ObjectGenerationRequest,
    profile: ProceduralTreeProfile,
) -> Vec<GeneratedObjectPart> {
    let mut parts = Vec::new();
    if request.lod == ObjectLod::Far {
        append_far_parts(&mut parts, profile);
        return parts;
    }

    let trunk_points = trunk_points(profile, request.lod);
    append_trunk_parts(&mut parts, profile, &trunk_points);
    let branch_tips = append_branch_parts(&mut parts, profile, request.lod, &trunk_points);
    append_leaf_parts(&mut parts, profile, request.lod, &branch_tips);
    append_root_parts(&mut parts, profile, request.lod);
    parts
}

fn append_far_parts(parts: &mut Vec<GeneratedObjectPart>, profile: ProceduralTreeProfile) {
    let trunk = TreeSegment {
        start: Vec3::new(0.0, profile.trunk_base_radius * 0.4, 0.0),
        end: Vec3::new(
            profile.lean.x * 0.6,
            profile.height * 0.62,
            profile.lean.y * 0.6,
        ),
        radius: profile.trunk_base_radius * 0.84,
    };
    parts.push(GeneratedObjectPart {
        name: "TreeFarTrunk".to_string(),
        recipe: ObjectMeshRecipe::Cylinder,
        slot: ObjectMaterialSlot::BarkPrimary,
        local_transform: transform_for_segment(trunk),
        wind_band: Some(TreeWindBand::Trunk),
    });
    parts.push(GeneratedObjectPart {
        name: "TreeFarCanopyMain".to_string(),
        recipe: ObjectMeshRecipe::Sphere,
        slot: ObjectMaterialSlot::LeafPrimary,
        local_transform: Transform::from_xyz(
            profile.lean.x * 0.58 + profile.canopy_eccentricity.x * 0.12,
            profile.height * 0.72,
            profile.lean.y * 0.58 + profile.canopy_eccentricity.y * 0.12,
        )
        .with_scale(Vec3::new(
            profile.canopy_radius * 1.1,
            profile.canopy_radius * 0.92,
            profile.canopy_radius * 1.06,
        )),
        wind_band: Some(TreeWindBand::Leaf),
    });
    parts.push(GeneratedObjectPart {
        name: "TreeFarCanopySecondary".to_string(),
        recipe: ObjectMeshRecipe::Sphere,
        slot: ObjectMaterialSlot::LeafSecondary,
        local_transform: Transform::from_xyz(
            profile.lean.x * 0.55 - profile.canopy_eccentricity.x * 0.09,
            profile.height * 0.66,
            profile.lean.y * 0.55 - profile.canopy_eccentricity.y * 0.09,
        )
        .with_scale(Vec3::new(
            profile.canopy_radius * 0.84,
            profile.canopy_radius * 0.72,
            profile.canopy_radius * 0.76,
        )),
        wind_band: Some(TreeWindBand::Leaf),
    });
}

fn trunk_points(profile: ProceduralTreeProfile, lod: ObjectLod) -> Vec<Vec3> {
    let segment_count = match lod {
        ObjectLod::Near => 7,
        ObjectLod::Mid => 4,
        ObjectLod::Far => 2,
    };
    let mut points = Vec::with_capacity(segment_count + 1);
    for index in 0..=segment_count {
        let t = index as f32 / segment_count as f32;
        let bend = Vec2::new(
            seeded_signed(profile.seed, 201 + index as u64 * 3),
            seeded_signed(profile.seed, 203 + index as u64 * 3),
        ) * profile.trunk_base_radius
            * lerp(0.05, 0.34, t)
            * (1.0 - t * 0.55);
        let point = Vec3::new(
            profile.lean.x * t + profile.canopy_eccentricity.x * t * t * 0.18 + bend.x,
            profile.height * t,
            profile.lean.y * t + profile.canopy_eccentricity.y * t * t * 0.18 + bend.y,
        );
        points.push(point);
    }
    points
}

fn append_trunk_parts(
    parts: &mut Vec<GeneratedObjectPart>,
    profile: ProceduralTreeProfile,
    trunk_points: &[Vec3],
) {
    for index in 0..trunk_points.len().saturating_sub(1) {
        let start = trunk_points[index];
        let end = trunk_points[index + 1];
        let t0 = index as f32 / (trunk_points.len() - 1) as f32;
        let t1 = (index + 1) as f32 / (trunk_points.len() - 1) as f32;
        let radius = profile.trunk_base_radius
            * lerp(1.04, 0.24, ((t0 + t1) * 0.5).powf(0.95))
            * (1.0 + seeded_signed(profile.seed, 251 + index as u64) * 0.08);
        let segment = TreeSegment {
            start,
            end,
            radius: radius.max(profile.trunk_base_radius * 0.18),
        };
        parts.push(GeneratedObjectPart {
            name: format!("TreeTrunkSegment{index:02}"),
            recipe: ObjectMeshRecipe::Cylinder,
            slot: if index % 2 == 0 && profile.moss_ratio > 0.32 {
                ObjectMaterialSlot::BarkWornEdge
            } else {
                ObjectMaterialSlot::BarkPrimary
            },
            local_transform: transform_for_segment(segment),
            wind_band: Some(TreeWindBand::Trunk),
        });
    }
}

fn append_branch_parts(
    parts: &mut Vec<GeneratedObjectPart>,
    profile: ProceduralTreeProfile,
    lod: ObjectLod,
    trunk_points: &[Vec3],
) -> Vec<Vec3> {
    let mut tips = vec![*trunk_points.last().unwrap_or(&Vec3::Y)];
    let primary_count = match lod {
        ObjectLod::Near => profile.branch_count,
        ObjectLod::Mid => (profile.branch_count as f32 * 0.64).round() as usize,
        ObjectLod::Far => 2,
    }
    .max(4);
    let tier_limit = match lod {
        ObjectLod::Near => profile.branch_tiers,
        ObjectLod::Mid => profile.branch_tiers.min(2),
        ObjectLod::Far => 1,
    };
    for branch_index in 0..primary_count {
        let primary = primary_branch_segment(profile, branch_index, trunk_points);
        tips.push(primary.end);
        parts.push(GeneratedObjectPart {
            name: format!("TreePrimaryBranch{branch_index:02}"),
            recipe: ObjectMeshRecipe::Cylinder,
            slot: if seeded_unit(profile.seed, 301 + branch_index as u64)
                < profile.dead_branch_ratio
            {
                ObjectMaterialSlot::BarkWornEdge
            } else {
                ObjectMaterialSlot::BarkPrimary
            },
            local_transform: transform_for_segment(primary),
            wind_band: Some(TreeWindBand::Branch),
        });

        if tier_limit < 2 {
            continue;
        }

        let secondary_count = if seeded_unit(profile.seed, 307 + branch_index as u64) > 0.56 {
            2
        } else {
            1
        };
        for secondary_index in 0..secondary_count {
            let secondary =
                secondary_branch_segment(profile, branch_index, secondary_index, primary);
            tips.push(secondary.end);
            parts.push(GeneratedObjectPart {
                name: format!("TreeSecondaryBranch{branch_index:02}_{secondary_index}"),
                recipe: ObjectMeshRecipe::Cylinder,
                slot: ObjectMaterialSlot::BarkPrimary,
                local_transform: transform_for_segment(secondary),
                wind_band: Some(TreeWindBand::Branch),
            });

            if tier_limit < 3 || seeded_unit(profile.seed, 313 + secondary_index as u64) < 0.42 {
                continue;
            }
            let tertiary =
                tertiary_branch_segment(profile, branch_index, secondary_index, secondary);
            tips.push(tertiary.end);
            parts.push(GeneratedObjectPart {
                name: format!("TreeTertiaryBranch{branch_index:02}_{secondary_index}"),
                recipe: ObjectMeshRecipe::Cylinder,
                slot: ObjectMaterialSlot::BarkWornEdge,
                local_transform: transform_for_segment(tertiary),
                wind_band: Some(TreeWindBand::Branch),
            });
        }
    }
    tips
}

fn primary_branch_segment(
    profile: ProceduralTreeProfile,
    branch_index: usize,
    trunk_points: &[Vec3],
) -> TreeSegment {
    let seed = profile.seed.wrapping_add(branch_index as u64 * 977);
    let t = lerp(0.28, 0.9, seeded_unit(seed, 401));
    let point_index = ((trunk_points.len() as f32 - 1.0) * t).floor() as usize;
    let start = trunk_points[point_index.min(trunk_points.len() - 1)];
    let angle = branch_index as f32 * 2.399_963 + seeded_signed(seed, 403) * 0.58;
    let radial = Vec3::new(angle.cos(), 0.0, angle.sin());
    let length = profile.canopy_radius * lerp(0.7, 1.2, seeded_unit(seed, 409));
    let rise = profile.height * lerp(0.06, 0.22, seeded_unit(seed, 419));
    let end = start
        + radial * length
        + Vec3::Y * rise
        + Vec3::new(
            profile.canopy_eccentricity.x,
            0.0,
            profile.canopy_eccentricity.y,
        ) * 0.12;
    TreeSegment {
        start,
        end,
        radius: profile.trunk_base_radius
            * lerp(0.24, 0.46, seeded_unit(seed, 421))
            * (1.0 - t * 0.32),
    }
}

fn secondary_branch_segment(
    profile: ProceduralTreeProfile,
    branch_index: usize,
    secondary_index: usize,
    primary: TreeSegment,
) -> TreeSegment {
    let seed = profile
        .seed
        .wrapping_add(branch_index as u64 * 379)
        .wrapping_add(secondary_index as u64 * 47);
    let primary_axis = (primary.end - primary.start).normalize_or_zero();
    let side = Vec3::new(-primary_axis.z, 0.0, primary_axis.x).normalize_or_zero();
    let start =
        primary.start + (primary.end - primary.start) * lerp(0.52, 0.84, seeded_unit(seed, 433));
    let direction = (primary_axis * 0.42
        + side * seeded_signed(seed, 439) * 0.78
        + Vec3::Y * lerp(0.42, 0.82, seeded_unit(seed, 443)))
    .normalize_or_zero();
    let length = primary.length() * lerp(0.32, 0.54, seeded_unit(seed, 449));
    let end = start + direction * length;
    TreeSegment {
        start,
        end,
        radius: (primary.radius * lerp(0.48, 0.7, seeded_unit(seed, 457))).max(0.04),
    }
}

fn tertiary_branch_segment(
    profile: ProceduralTreeProfile,
    branch_index: usize,
    secondary_index: usize,
    secondary: TreeSegment,
) -> TreeSegment {
    let seed = profile
        .seed
        .wrapping_add(branch_index as u64 * 887)
        .wrapping_add(secondary_index as u64 * 131);
    let axis = (secondary.end - secondary.start).normalize_or_zero();
    let side = Vec3::new(axis.z, 0.0, -axis.x).normalize_or_zero();
    let start = secondary.start
        + (secondary.end - secondary.start) * lerp(0.62, 0.9, seeded_unit(seed, 463));
    let end = start
        + (axis * 0.38
            + side * seeded_signed(seed, 467) * 0.52
            + Vec3::Y * lerp(0.32, 0.62, seeded_unit(seed, 479)))
        .normalize_or_zero()
            * secondary.length()
            * lerp(0.38, 0.58, seeded_unit(seed, 487));
    TreeSegment {
        start,
        end,
        radius: (secondary.radius * lerp(0.42, 0.66, seeded_unit(seed, 491))).max(0.025),
    }
}

fn append_leaf_parts(
    parts: &mut Vec<GeneratedObjectPart>,
    profile: ProceduralTreeProfile,
    lod: ObjectLod,
    branch_tips: &[Vec3],
) {
    let cluster_scale = match lod {
        ObjectLod::Near => 1.0,
        ObjectLod::Mid => 0.56,
        ObjectLod::Far => 0.3,
    };
    let cluster_count = ((profile.leaf_cluster_count as f32) * cluster_scale).round() as usize;
    let cluster_count = cluster_count.max(6);
    for cluster_index in 0..cluster_count {
        let seed = profile.seed.wrapping_add(cluster_index as u64 * 2_653);
        let anchor = branch_tips[cluster_index % branch_tips.len()];
        let angle = cluster_index as f32 * 1.713 + seeded_signed(seed, 501) * 0.72;
        let outward = Vec3::new(angle.cos(), seeded_signed(seed, 503) * 0.26, angle.sin());
        let offset = outward
            * profile.canopy_radius
            * lerp(0.1, 0.48, seeded_unit(seed, 509))
            * profile.leaf_density.clamp(0.62, 1.34);
        let center = anchor + offset;

        let lobe_count = match lod {
            ObjectLod::Near => {
                if seeded_unit(seed, 521) > 0.64 {
                    3
                } else {
                    2
                }
            }
            ObjectLod::Mid => 1,
            ObjectLod::Far => 1,
        };
        for lobe in 0..lobe_count {
            let lobe_seed = seed.wrapping_add(lobe as u64 * 37);
            let lobe_offset = Vec3::new(
                seeded_signed(lobe_seed, 523) * profile.canopy_radius * 0.24,
                seeded_signed(lobe_seed, 541) * profile.canopy_radius * 0.18,
                seeded_signed(lobe_seed, 547) * profile.canopy_radius * 0.24,
            );
            let scale = Vec3::new(
                profile.canopy_radius * lerp(0.26, 0.72, seeded_unit(lobe_seed, 557)),
                profile.canopy_radius * lerp(0.18, 0.48, seeded_unit(lobe_seed, 563)),
                profile.canopy_radius * lerp(0.24, 0.68, seeded_unit(lobe_seed, 569)),
            );
            let dryness = profile.dead_branch_ratio + seeded_unit(lobe_seed, 571) * 0.24;
            let slot = if dryness > 0.68 {
                ObjectMaterialSlot::LeafDry
            } else if lobe % 3 == 0 {
                ObjectMaterialSlot::LeafSecondary
            } else {
                ObjectMaterialSlot::LeafPrimary
            };
            parts.push(GeneratedObjectPart {
                name: format!("TreeLeafLobe{cluster_index:03}_{lobe}"),
                recipe: if lobe % 2 == 0 {
                    ObjectMeshRecipe::Sphere
                } else {
                    ObjectMeshRecipe::Cuboid
                },
                slot,
                local_transform: Transform::from_translation(center + lobe_offset)
                    .with_rotation(Quat::from_euler(
                        EulerRot::XYZ,
                        seeded_signed(lobe_seed, 577) * 0.4,
                        angle + seeded_signed(lobe_seed, 587) * 0.22,
                        seeded_signed(lobe_seed, 593) * 0.3,
                    ))
                    .with_scale(scale),
                wind_band: Some(TreeWindBand::Leaf),
            });
        }
    }
}

fn append_root_parts(
    parts: &mut Vec<GeneratedObjectPart>,
    profile: ProceduralTreeProfile,
    lod: ObjectLod,
) {
    let root_count = match lod {
        ObjectLod::Near => (3.0 + profile.root_exposure * 4.0).round() as usize,
        ObjectLod::Mid => 2,
        ObjectLod::Far => 1,
    }
    .max(1);
    for root_index in 0..root_count {
        let seed = profile.seed.wrapping_add(root_index as u64 * 631);
        let angle = root_index as f32 * (std::f32::consts::TAU / root_count as f32)
            + seeded_signed(seed, 601) * 0.38;
        let start = Vec3::new(
            angle.cos(),
            0.16 + profile.root_exposure * 0.14,
            angle.sin(),
        ) * profile.trunk_base_radius
            * 0.72;
        let end = Vec3::new(
            angle.cos() * profile.canopy_radius * lerp(0.16, 0.34, seeded_unit(seed, 607)),
            0.02,
            angle.sin() * profile.canopy_radius * lerp(0.16, 0.34, seeded_unit(seed, 613)),
        );
        let segment = TreeSegment {
            start,
            end,
            radius: profile.trunk_base_radius
                * lerp(0.22, 0.38, seeded_unit(seed, 617))
                * profile.root_exposure.clamp(0.35, 1.1),
        };
        parts.push(GeneratedObjectPart {
            name: format!("TreeRoot{root_index:02}"),
            recipe: ObjectMeshRecipe::Cylinder,
            slot: ObjectMaterialSlot::BarkWornEdge,
            local_transform: transform_for_segment(segment),
            wind_band: None,
        });
    }
    parts.push(GeneratedObjectPart {
        name: "TreeRootShadow".to_string(),
        recipe: ObjectMeshRecipe::Sphere,
        slot: ObjectMaterialSlot::RootShadow,
        local_transform: Transform::from_xyz(profile.lean.x * 0.14, 0.03, profile.lean.y * 0.14)
            .with_scale(Vec3::new(
                profile.canopy_radius * 0.94,
                0.05,
                profile.canopy_radius * 0.78,
            )),
        wind_band: None,
    });
}

fn build_collision(
    request: &ObjectGenerationRequest,
    profile: ProceduralTreeProfile,
) -> ObjectCollisionRecipe {
    if request.lod == ObjectLod::Far || request.collision_mode == ObjectCollisionMode::VisualOnly {
        return ObjectCollisionRecipe {
            trunk: None,
            root_blockers: Vec::new(),
            sensor: None,
        };
    }
    let trunk = Some(TreeTrunkColliderRecipe {
        radius: (profile.trunk_base_radius * 0.82).max(0.2),
        height: (profile.height * 0.82).max(2.4),
    });
    let root_blockers = if request.collision_mode == ObjectCollisionMode::TrunkOnly {
        Vec::new()
    } else {
        let root_count = if request.collision_mode == ObjectCollisionMode::Full {
            4
        } else {
            2
        };
        (0..root_count)
            .map(|index| {
                let angle = index as f32 * (std::f32::consts::TAU / root_count as f32)
                    + seeded_signed(profile.seed, 701 + index as u64) * 0.32;
                TreeRootBlockerRecipe {
                    center: Vec3::new(
                        angle.cos() * profile.canopy_radius * 0.24,
                        0.32,
                        angle.sin() * profile.canopy_radius * 0.24,
                    ),
                    half_extents: Vec3::new(
                        (profile.trunk_base_radius * 0.46).max(0.16),
                        0.34,
                        (profile.trunk_base_radius * 0.34).max(0.12),
                    ),
                    yaw: angle,
                }
            })
            .collect()
    };
    let sensor = if request.collision_mode == ObjectCollisionMode::Full {
        Some(TreeSensorRecipe {
            center: Vec3::new(
                profile.lean.x * 0.35,
                profile.height * 0.58,
                profile.lean.y * 0.35,
            ),
            half_extents: Vec3::new(
                profile.canopy_radius * 0.72,
                profile.height * 0.28,
                profile.canopy_radius * 0.72,
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
