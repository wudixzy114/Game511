use std::collections::HashSet;

use bevy::{
    asset::RenderAssetUsages,
    color::LinearRgba,
    math::primitives::{Capsule3d, Cuboid, Cylinder, Sphere},
    mesh::{Indices, PrimitiveTopology},
    pbr::MeshMaterial3d,
    prelude::*,
};

use crate::core::{
    config::{AppConfig, AssetConfig},
    performance::{FramePerformance, PerformancePhase},
};
use crate::game::flow::{AppScreen, InGameState};

pub struct ProceduralAssetPlugin;

impl Plugin for ProceduralAssetPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ProceduralAssetRegistry::default());
        app.insert_resource(ProceduralAssetRuntimeStats::default());
        if app
            .world()
            .get_resource::<ProceduralAssetMaterials>()
            .is_none()
        {
            let material_set = {
                let config = app.world().resource::<AppConfig>().assets.clone();
                let mut materials = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
                ProceduralAssetMaterials::new(&mut materials, &config)
            };
            app.insert_resource(material_set);
        }
        app.add_systems(
            Update,
            report_procedural_asset_runtime_stats.run_if(in_state(InGameState::Running)),
        );
        app.add_systems(OnExit(AppScreen::InGame), cleanup_asset_runtime_stats);
    }
}

#[derive(Debug, Component, Clone, PartialEq)]
pub struct ProceduralAsset {
    pub spec: ProceduralAssetSpec,
}

impl ProceduralAsset {
    pub fn new(spec: ProceduralAssetSpec) -> Self {
        Self { spec }
    }
}

#[derive(Debug, Resource, Clone, PartialEq)]
pub struct ProceduralAssetRegistry {
    specs: Vec<ProceduralAssetSpec>,
}

#[derive(Debug, Resource, Default, Clone, PartialEq, Eq)]
struct ProceduralAssetRuntimeStats {
    last_asset_count: usize,
    last_kind_count: usize,
    last_semantic_count: usize,
}

impl Default for ProceduralAssetRegistry {
    fn default() -> Self {
        Self {
            specs: core_asset_specs(),
        }
    }
}

impl ProceduralAssetRegistry {
    pub fn specs(&self) -> &[ProceduralAssetSpec] {
        &self.specs
    }

    pub fn spec(&self, kind: ProceduralAssetKind) -> Option<&ProceduralAssetSpec> {
        self.specs.iter().find(|spec| spec.kind == kind)
    }

    pub fn by_semantic(&self, semantic: ProceduralSemantic) -> Vec<&ProceduralAssetSpec> {
        self.specs
            .iter()
            .filter(|spec| spec.semantics.contains(&semantic))
            .collect()
    }
}

#[derive(Debug, Resource, Clone)]
pub struct ProceduralAssetMaterials {
    mud_wall: Handle<StandardMaterial>,
    dark_roof: Handle<StandardMaterial>,
    wood: Handle<StandardMaterial>,
    stone: Handle<StandardMaterial>,
    cloth: Handle<StandardMaterial>,
    water: Handle<StandardMaterial>,
    sand: Handle<StandardMaterial>,
    wool: Handle<StandardMaterial>,
    npc_cloth: Handle<StandardMaterial>,
    bird_feather: Handle<StandardMaterial>,
    fish_scale: Handle<StandardMaterial>,
    old_stone: Handle<StandardMaterial>,
    relic: Handle<StandardMaterial>,
    desert_stone: Handle<StandardMaterial>,
    warm_light: Handle<StandardMaterial>,
    shadow: Handle<StandardMaterial>,
}

impl ProceduralAssetMaterials {
    pub fn new(materials: &mut Assets<StandardMaterial>, config: &AssetConfig) -> Self {
        let saturation = config.color_saturation.max(0.0);
        let water_alpha = config.water_alpha.clamp(0.05, 1.0);
        let shadow_alpha = config.shadow_alpha.clamp(0.05, 1.0);
        let warm_light_intensity = config.warm_light_intensity.max(0.0);
        Self {
            mud_wall: materials.add(StandardMaterial {
                base_color: toned_color(0.57, 0.49, 0.37, saturation),
                perceptual_roughness: 0.92,
                ..Default::default()
            }),
            dark_roof: materials.add(StandardMaterial {
                base_color: toned_color(0.33, 0.23, 0.17, saturation),
                perceptual_roughness: 0.96,
                ..Default::default()
            }),
            wood: materials.add(StandardMaterial {
                base_color: toned_color(0.33, 0.24, 0.15, saturation),
                perceptual_roughness: 0.92,
                ..Default::default()
            }),
            stone: materials.add(StandardMaterial {
                base_color: toned_color(0.48, 0.47, 0.42, saturation),
                perceptual_roughness: 0.98,
                ..Default::default()
            }),
            cloth: materials.add(StandardMaterial {
                base_color: toned_color(0.58, 0.28, 0.22, saturation),
                perceptual_roughness: 0.86,
                ..Default::default()
            }),
            water: materials.add(StandardMaterial {
                base_color: Color::srgba(0.18, 0.48, 0.58, water_alpha),
                alpha_mode: AlphaMode::Blend,
                metallic: 0.02,
                perceptual_roughness: 0.2,
                emissive: LinearRgba::rgb(0.015, 0.04, 0.055),
                ..Default::default()
            }),
            sand: materials.add(StandardMaterial {
                base_color: toned_color(0.66, 0.58, 0.4, saturation),
                perceptual_roughness: 1.0,
                ..Default::default()
            }),
            wool: materials.add(StandardMaterial {
                base_color: toned_color(0.86, 0.82, 0.72, saturation),
                perceptual_roughness: 0.99,
                ..Default::default()
            }),
            npc_cloth: materials.add(StandardMaterial {
                base_color: toned_color(0.58, 0.49, 0.36, saturation),
                perceptual_roughness: 0.91,
                ..Default::default()
            }),
            bird_feather: materials.add(StandardMaterial {
                base_color: toned_color(0.16, 0.16, 0.15, saturation),
                perceptual_roughness: 0.82,
                ..Default::default()
            }),
            fish_scale: materials.add(StandardMaterial {
                base_color: toned_color(0.34, 0.57, 0.63, saturation),
                perceptual_roughness: 0.42,
                metallic: 0.08,
                ..Default::default()
            }),
            old_stone: materials.add(StandardMaterial {
                base_color: toned_color(0.47, 0.42, 0.34, saturation),
                perceptual_roughness: 0.98,
                ..Default::default()
            }),
            relic: materials.add(StandardMaterial {
                base_color: toned_color(0.72, 0.64, 0.46, saturation),
                emissive: LinearRgba::rgb(
                    0.18 * warm_light_intensity,
                    0.12 * warm_light_intensity,
                    0.04 * warm_light_intensity,
                ),
                perceptual_roughness: 0.88,
                ..Default::default()
            }),
            desert_stone: materials.add(StandardMaterial {
                base_color: toned_color(0.72, 0.58, 0.36, saturation),
                perceptual_roughness: 0.96,
                reflectance: 0.06,
                ..Default::default()
            }),
            warm_light: materials.add(StandardMaterial {
                base_color: toned_color(1.0, 0.72, 0.34, saturation),
                emissive: LinearRgba::rgb(
                    1.1 * warm_light_intensity,
                    0.58 * warm_light_intensity,
                    0.18 * warm_light_intensity,
                ),
                perceptual_roughness: 0.5,
                ..Default::default()
            }),
            shadow: materials.add(StandardMaterial {
                base_color: Color::srgba(0.08, 0.06, 0.04, shadow_alpha),
                alpha_mode: AlphaMode::Blend,
                perceptual_roughness: 1.0,
                ..Default::default()
            }),
        }
    }

    fn family(&self, family: ProceduralMaterialFamily) -> Handle<StandardMaterial> {
        match family {
            ProceduralMaterialFamily::MudWall => self.mud_wall.clone(),
            ProceduralMaterialFamily::DarkRoof => self.dark_roof.clone(),
            ProceduralMaterialFamily::Wood => self.wood.clone(),
            ProceduralMaterialFamily::Stone => self.stone.clone(),
            ProceduralMaterialFamily::Cloth => self.cloth.clone(),
            ProceduralMaterialFamily::Water => self.water.clone(),
            ProceduralMaterialFamily::Sand => self.sand.clone(),
            ProceduralMaterialFamily::Wool => self.wool.clone(),
            ProceduralMaterialFamily::NpcCloth => self.npc_cloth.clone(),
            ProceduralMaterialFamily::BirdFeather => self.bird_feather.clone(),
            ProceduralMaterialFamily::FishScale => self.fish_scale.clone(),
            ProceduralMaterialFamily::OldStone => self.old_stone.clone(),
            ProceduralMaterialFamily::Relic => self.relic.clone(),
            ProceduralMaterialFamily::DesertStone => self.desert_stone.clone(),
            ProceduralMaterialFamily::WarmLight => self.warm_light(),
            ProceduralMaterialFamily::Shadow => self.shadow(),
        }
    }

    fn warm_light(&self) -> Handle<StandardMaterial> {
        self.warm_light.clone()
    }

    fn shadow(&self) -> Handle<StandardMaterial> {
        self.shadow.clone()
    }
}

fn toned_color(r: f32, g: f32, b: f32, saturation: f32) -> Color {
    let luminance = r * 0.2126 + g * 0.7152 + b * 0.0722;
    Color::srgb(
        (luminance + (r - luminance) * saturation).clamp(0.0, 1.0),
        (luminance + (g - luminance) * saturation).clamp(0.0, 1.0),
        (luminance + (b - luminance) * saturation).clamp(0.0, 1.0),
    )
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum ProceduralAssetLod {
    Near,
    Mid,
    Far,
}

#[derive(Debug, Clone)]
pub struct ProceduralSpawnRequest<'a> {
    pub kind: ProceduralAssetKind,
    pub seed_salt: u64,
    pub name: &'a str,
    pub transform: Transform,
    pub lod: ProceduralAssetLod,
}

impl<'a> ProceduralSpawnRequest<'a> {
    pub fn new(
        kind: ProceduralAssetKind,
        seed_salt: u64,
        name: &'a str,
        transform: Transform,
    ) -> Self {
        Self {
            kind,
            seed_salt,
            name,
            transform,
            lod: ProceduralAssetLod::Near,
        }
    }

    pub fn with_lod(mut self, lod: ProceduralAssetLod) -> Self {
        self.lod = lod;
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProceduralAssetBlueprint {
    pub kind: ProceduralAssetKind,
    pub lod: ProceduralAssetLod,
    pub parts: Vec<ProceduralPartBlueprint>,
}

impl ProceduralAssetBlueprint {
    pub fn part_count(&self) -> usize {
        self.parts.len()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProceduralPartBlueprint {
    pub name: &'static str,
    pub shape: ProceduralShape,
    pub material_family: ProceduralMaterialFamily,
    pub local_transform: Transform,
    pub animation_role: Option<ProceduralAnimationRole>,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProceduralShape {
    Cuboid(Vec3),
    Cylinder { radius: f32, depth: f32 },
    Capsule { radius: f32, depth: f32 },
    Sphere { radius: f32 },
    Pyramid { width: f32, height: f32 },
}

#[derive(Debug, Component, Clone, Copy, Eq, PartialEq, Hash)]
pub enum ProceduralAnimationRole {
    SheepHead,
    SheepLegFrontLeft,
    SheepLegFrontRight,
    SheepLegBackLeft,
    SheepLegBackRight,
    BirdLeftWing,
    BirdRightWing,
    FishTail,
    NpcHead,
    NpcHandLeft,
    NpcHandRight,
    ClothCanopy,
    Smoke,
    WaterRipple,
}

pub fn spawn_procedural_asset(
    parent: &mut ChildSpawnerCommands<'_>,
    meshes: &mut Assets<Mesh>,
    materials: &ProceduralAssetMaterials,
    request: ProceduralSpawnRequest<'_>,
) -> Entity {
    let spec = registered_spec(request.kind).instance(request.seed_salt);
    let blueprint = asset_blueprint(&spec, request.lod);
    let mut entity = parent.spawn((
        Name::new(request.name.to_string()),
        request.transform,
        ProceduralAsset::new(spec),
    ));
    let root = entity.id();
    entity.with_children(|part_parent| {
        spawn_blueprint_parts(part_parent, meshes, materials, &blueprint);
    });
    root
}

pub fn spawn_procedural_asset_entity(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &ProceduralAssetMaterials,
    request: ProceduralSpawnRequest<'_>,
) -> Entity {
    let spec = registered_spec(request.kind).instance(request.seed_salt);
    let blueprint = asset_blueprint(&spec, request.lod);
    let root = commands
        .spawn((
            Name::new(request.name.to_string()),
            DespawnOnExit(AppScreen::InGame),
            request.transform,
            ProceduralAsset::new(spec),
        ))
        .id();
    commands.entity(root).with_children(|part_parent| {
        spawn_blueprint_parts(part_parent, meshes, materials, &blueprint);
    });
    root
}

pub fn choose_lod(spec: &ProceduralAssetSpec, distance: f32) -> ProceduralAssetLod {
    match spec.lod {
        ProceduralLodStrategy::Fixed => ProceduralAssetLod::Near,
        ProceduralLodStrategy::LocalDetail {
            near_distance,
            mid_distance,
            ..
        } => {
            if distance <= near_distance {
                ProceduralAssetLod::Near
            } else if distance <= mid_distance {
                ProceduralAssetLod::Mid
            } else {
                ProceduralAssetLod::Far
            }
        }
        ProceduralLodStrategy::Character {
            visible_distance, ..
        } => {
            if distance <= visible_distance * 0.45 {
                ProceduralAssetLod::Near
            } else if distance <= visible_distance {
                ProceduralAssetLod::Mid
            } else {
                ProceduralAssetLod::Far
            }
        }
        ProceduralLodStrategy::Landmark {
            near_distance,
            silhouette_distance,
            ..
        } => {
            if distance <= near_distance {
                ProceduralAssetLod::Near
            } else if distance <= silhouette_distance {
                ProceduralAssetLod::Mid
            } else {
                ProceduralAssetLod::Far
            }
        }
    }
}

pub fn asset_blueprint(
    spec: &ProceduralAssetSpec,
    lod: ProceduralAssetLod,
) -> ProceduralAssetBlueprint {
    let parts = match spec.kind {
        ProceduralAssetKind::VillageHouse => house_parts(spec, lod),
        ProceduralAssetKind::VillageWell => well_parts(lod),
        ProceduralAssetKind::SheepPenRail => sheep_pen_rail_parts(spec),
        ProceduralAssetKind::MarketStall => market_stall_parts(lod),
        ProceduralAssetKind::VillageShore => shore_parts(lod),
        ProceduralAssetKind::PathStone => path_stone_parts(spec),
        ProceduralAssetKind::Sheep => sheep_parts(lod),
        ProceduralAssetKind::Shepherd => npc_parts(ProceduralAssetKind::Shepherd, lod),
        ProceduralAssetKind::Merchant => npc_parts(ProceduralAssetKind::Merchant, lod),
        ProceduralAssetKind::Bird => bird_parts(lod),
        ProceduralAssetKind::Fish => fish_parts(lod),
        ProceduralAssetKind::FortuneTeller => npc_parts(ProceduralAssetKind::FortuneTeller, lod),
        ProceduralAssetKind::DesertPyramid => pyramid_parts(spec, lod),
        ProceduralAssetKind::DesertOasis => oasis_parts(spec, lod),
        ProceduralAssetKind::PyramidRuinWall => ruin_wall_parts(spec, lod),
        ProceduralAssetKind::DesertRelic => relic_parts(lod),
        ProceduralAssetKind::MistRiver => mist_river_parts(spec, lod),
        ProceduralAssetKind::HeadlandMarker => headland_marker_parts(spec, lod),
    };

    if parts.is_empty() {
        tracing::error!(
            target: "dao_game::assets::factory",
            kind = spec.kind.label(),
            lod = ?lod,
            "procedural asset blueprint had no parts"
        );
    }

    ProceduralAssetBlueprint {
        kind: spec.kind,
        lod,
        parts,
    }
}

fn spawn_blueprint_parts(
    parent: &mut ChildSpawnerCommands<'_>,
    meshes: &mut Assets<Mesh>,
    materials: &ProceduralAssetMaterials,
    blueprint: &ProceduralAssetBlueprint,
) {
    for part in &blueprint.parts {
        let mut entity = parent.spawn((
            Name::new(part.name),
            Mesh3d(meshes.add(mesh_from_shape(part.shape))),
            MeshMaterial3d(materials.family(part.material_family)),
            part.local_transform,
        ));
        if let Some(role) = part.animation_role {
            entity.insert(role);
        }
    }
}

fn mesh_from_shape(shape: ProceduralShape) -> Mesh {
    match shape {
        ProceduralShape::Cuboid(size) => Mesh::from(Cuboid::new(size.x, size.y, size.z)),
        ProceduralShape::Cylinder { radius, depth } => Mesh::from(Cylinder::new(radius, depth)),
        ProceduralShape::Capsule { radius, depth } => Mesh::from(Capsule3d::new(radius, depth)),
        ProceduralShape::Sphere { radius } => Sphere::new(radius).mesh().uv(16, 10),
        ProceduralShape::Pyramid { width, height } => pyramid_mesh(width, height),
    }
}

fn part(
    name: &'static str,
    shape: ProceduralShape,
    material_family: ProceduralMaterialFamily,
    local_transform: Transform,
) -> ProceduralPartBlueprint {
    ProceduralPartBlueprint {
        name,
        shape,
        material_family,
        local_transform,
        animation_role: None,
    }
}

fn animated_part(
    name: &'static str,
    shape: ProceduralShape,
    material_family: ProceduralMaterialFamily,
    local_transform: Transform,
    animation_role: ProceduralAnimationRole,
) -> ProceduralPartBlueprint {
    ProceduralPartBlueprint {
        name,
        shape,
        material_family,
        local_transform,
        animation_role: Some(animation_role),
    }
}

fn seeded_unit(seed: u64, salt: u64) -> f32 {
    let value = stable_asset_seed(seed, salt);
    (value as f64 / u64::MAX as f64) as f32
}

fn house_parts(
    spec: &ProceduralAssetSpec,
    lod: ProceduralAssetLod,
) -> Vec<ProceduralPartBlueprint> {
    let body = Vec3::new(
        spec.base_size.x * (0.94 + seeded_unit(spec.seed, 31) * 0.12),
        spec.base_size.y * (0.96 + seeded_unit(spec.seed, 37) * 0.08),
        spec.base_size.z * (0.94 + seeded_unit(spec.seed, 41) * 0.12),
    );
    match lod {
        ProceduralAssetLod::Near => vec![
            part(
                "HouseWall",
                ProceduralShape::Cuboid(Vec3::new(body.x * 0.9, body.y * 0.75, body.z * 0.86)),
                ProceduralMaterialFamily::MudWall,
                Transform::from_translation(Vec3::Y * body.y * 0.38),
            ),
            part(
                "HouseRoof",
                ProceduralShape::Cuboid(Vec3::new(body.x, body.y * 0.24, body.z)),
                ProceduralMaterialFamily::DarkRoof,
                Transform::from_translation(Vec3::Y * body.y * 0.82),
            ),
            part(
                "HouseDoor",
                ProceduralShape::Cuboid(Vec3::new(0.76, 1.25, 0.08)),
                ProceduralMaterialFamily::Wood,
                Transform::from_translation(Vec3::new(0.0, 0.72, -body.z * 0.44)),
            ),
            part(
                "HouseFrontBeamLeft",
                ProceduralShape::Cuboid(Vec3::new(0.16, body.y * 0.76, 0.16)),
                ProceduralMaterialFamily::Wood,
                Transform::from_translation(Vec3::new(
                    -body.x * 0.45,
                    body.y * 0.39,
                    -body.z * 0.47,
                )),
            ),
            part(
                "HouseFrontBeamRight",
                ProceduralShape::Cuboid(Vec3::new(0.16, body.y * 0.76, 0.16)),
                ProceduralMaterialFamily::Wood,
                Transform::from_translation(Vec3::new(
                    body.x * 0.45,
                    body.y * 0.39,
                    -body.z * 0.47,
                )),
            ),
            part(
                "HouseWindowWarmLeft",
                ProceduralShape::Cuboid(Vec3::new(0.48, 0.38, 0.06)),
                ProceduralMaterialFamily::WarmLight,
                Transform::from_translation(Vec3::new(-body.x * 0.25, 1.28, -body.z * 0.45)),
            ),
            part(
                "HouseWindowWarmRight",
                ProceduralShape::Cuboid(Vec3::new(0.48, 0.38, 0.06)),
                ProceduralMaterialFamily::WarmLight,
                Transform::from_translation(Vec3::new(body.x * 0.25, 1.28, -body.z * 0.45)),
            ),
            part(
                "HouseChimney",
                ProceduralShape::Cuboid(Vec3::new(0.42, 0.82, 0.42)),
                ProceduralMaterialFamily::Stone,
                Transform::from_translation(Vec3::new(body.x * 0.22, body.y * 1.04, body.z * 0.18)),
            ),
            animated_part(
                "HouseSmokeWisp",
                ProceduralShape::Sphere { radius: 0.34 },
                ProceduralMaterialFamily::Shadow,
                Transform::from_translation(Vec3::new(body.x * 0.22, body.y * 1.32, body.z * 0.18))
                    .with_scale(Vec3::new(1.0, 1.45, 1.0)),
                ProceduralAnimationRole::Smoke,
            ),
            part(
                "HouseEaveShadow",
                ProceduralShape::Cuboid(Vec3::new(body.x * 0.94, 0.05, 0.08)),
                ProceduralMaterialFamily::Shadow,
                Transform::from_translation(Vec3::new(0.0, body.y * 0.7, -body.z * 0.5)),
            ),
            part(
                "HouseStep",
                ProceduralShape::Cuboid(Vec3::new(1.2, 0.18, 0.72)),
                ProceduralMaterialFamily::Stone,
                Transform::from_translation(Vec3::new(0.0, 0.12, -body.z * 0.58)),
            ),
        ],
        ProceduralAssetLod::Mid => vec![
            part(
                "HouseMidBody",
                ProceduralShape::Cuboid(Vec3::new(body.x * 0.9, body.y * 0.75, body.z * 0.86)),
                ProceduralMaterialFamily::MudWall,
                Transform::from_translation(Vec3::Y * body.y * 0.38),
            ),
            part(
                "HouseMidRoof",
                ProceduralShape::Cuboid(Vec3::new(body.x, body.y * 0.24, body.z)),
                ProceduralMaterialFamily::DarkRoof,
                Transform::from_translation(Vec3::Y * body.y * 0.82),
            ),
        ],
        ProceduralAssetLod::Far => vec![part(
            "HouseFarBlock",
            ProceduralShape::Cuboid(Vec3::new(body.x, body.y * 0.82, body.z)),
            ProceduralMaterialFamily::MudWall,
            Transform::from_translation(Vec3::Y * body.y * 0.42),
        )],
    }
}

fn well_parts(lod: ProceduralAssetLod) -> Vec<ProceduralPartBlueprint> {
    match lod {
        ProceduralAssetLod::Near => vec![
            part(
                "WellStoneRing",
                ProceduralShape::Cylinder {
                    radius: 1.1,
                    depth: 0.8,
                },
                ProceduralMaterialFamily::Stone,
                Transform::from_translation(Vec3::Y * 0.4),
            ),
            part(
                "WellWater",
                ProceduralShape::Cylinder {
                    radius: 0.82,
                    depth: 0.05,
                },
                ProceduralMaterialFamily::Water,
                Transform::from_translation(Vec3::Y * 0.84),
            ),
            part(
                "WellBeamLeft",
                ProceduralShape::Cuboid(Vec3::new(0.16, 1.55, 0.16)),
                ProceduralMaterialFamily::Wood,
                Transform::from_translation(Vec3::new(-0.92, 1.18, 0.0)),
            ),
            part(
                "WellBeamRight",
                ProceduralShape::Cuboid(Vec3::new(0.16, 1.55, 0.16)),
                ProceduralMaterialFamily::Wood,
                Transform::from_translation(Vec3::new(0.92, 1.18, 0.0)),
            ),
            part(
                "WellTopBeam",
                ProceduralShape::Cuboid(Vec3::new(2.1, 0.16, 0.16)),
                ProceduralMaterialFamily::Wood,
                Transform::from_translation(Vec3::new(0.0, 1.94, 0.0)),
            ),
            part(
                "WellBucket",
                ProceduralShape::Cuboid(Vec3::new(0.34, 0.34, 0.34)),
                ProceduralMaterialFamily::Wood,
                Transform::from_translation(Vec3::new(0.0, 1.18, 0.0)),
            ),
            part(
                "WellRope",
                ProceduralShape::Cuboid(Vec3::new(0.04, 0.84, 0.04)),
                ProceduralMaterialFamily::Wood,
                Transform::from_translation(Vec3::new(0.0, 1.52, 0.0)),
            ),
            animated_part(
                "WellWetGround",
                ProceduralShape::Cylinder {
                    radius: 1.9,
                    depth: 0.035,
                },
                ProceduralMaterialFamily::Water,
                Transform::from_translation(Vec3::Y * 0.03).with_scale(Vec3::new(1.2, 1.0, 0.74)),
                ProceduralAnimationRole::WaterRipple,
            ),
        ],
        ProceduralAssetLod::Mid => vec![
            part(
                "WellMidRing",
                ProceduralShape::Cylinder {
                    radius: 1.1,
                    depth: 0.8,
                },
                ProceduralMaterialFamily::Stone,
                Transform::from_translation(Vec3::Y * 0.4),
            ),
            part(
                "WellMidWater",
                ProceduralShape::Cylinder {
                    radius: 0.82,
                    depth: 0.05,
                },
                ProceduralMaterialFamily::Water,
                Transform::from_translation(Vec3::Y * 0.84),
            ),
        ],
        ProceduralAssetLod::Far => vec![part(
            "WellFarStone",
            ProceduralShape::Cylinder {
                radius: 1.0,
                depth: 0.45,
            },
            ProceduralMaterialFamily::Stone,
            Transform::from_translation(Vec3::Y * 0.25),
        )],
    }
}

fn sheep_pen_rail_parts(spec: &ProceduralAssetSpec) -> Vec<ProceduralPartBlueprint> {
    vec![
        part(
            "SheepPenPostLeft",
            ProceduralShape::Cuboid(Vec3::new(0.3, 1.25, 0.3)),
            ProceduralMaterialFamily::Wood,
            Transform::from_translation(Vec3::new(-spec.base_size.x * 0.5, -0.18, 0.0)),
        ),
        part(
            "SheepPenPostRight",
            ProceduralShape::Cuboid(Vec3::new(0.3, 1.25, 0.3)),
            ProceduralMaterialFamily::Wood,
            Transform::from_translation(Vec3::new(spec.base_size.x * 0.5, -0.18, 0.0)),
        ),
        part(
            "SheepPenRail",
            ProceduralShape::Cuboid(spec.base_size),
            ProceduralMaterialFamily::Wood,
            Transform::IDENTITY,
        ),
        part(
            "SheepPenRailLower",
            ProceduralShape::Cuboid(Vec3::new(
                spec.base_size.x,
                spec.base_size.y * 0.72,
                spec.base_size.z,
            )),
            ProceduralMaterialFamily::Wood,
            Transform::from_translation(Vec3::Y * -0.42),
        ),
        part(
            "SheepPenTrampledGround",
            ProceduralShape::Cuboid(Vec3::new(spec.base_size.x * 0.86, 0.035, 0.72)),
            ProceduralMaterialFamily::Sand,
            Transform::from_translation(Vec3::new(0.0, -0.86, -0.34)),
        ),
    ]
}

fn market_stall_parts(lod: ProceduralAssetLod) -> Vec<ProceduralPartBlueprint> {
    match lod {
        ProceduralAssetLod::Near => vec![
            part(
                "MarketCounter",
                ProceduralShape::Cuboid(Vec3::new(4.8, 0.7, 1.4)),
                ProceduralMaterialFamily::Wood,
                Transform::from_translation(Vec3::Y * 0.45),
            ),
            animated_part(
                "MarketClothCanopy",
                ProceduralShape::Cuboid(Vec3::new(5.4, 0.15, 3.0)),
                ProceduralMaterialFamily::Cloth,
                Transform::from_translation(Vec3::new(0.0, 2.1, -0.2)),
                ProceduralAnimationRole::ClothCanopy,
            ),
            part(
                "MarketPostLeft",
                ProceduralShape::Cuboid(Vec3::new(0.18, 2.1, 0.18)),
                ProceduralMaterialFamily::Wood,
                Transform::from_translation(Vec3::new(-2.25, 1.05, -1.15)),
            ),
            part(
                "MarketPostRight",
                ProceduralShape::Cuboid(Vec3::new(0.18, 2.1, 0.18)),
                ProceduralMaterialFamily::Wood,
                Transform::from_translation(Vec3::new(2.25, 1.05, -1.15)),
            ),
            part(
                "MarketCrateA",
                ProceduralShape::Cuboid(Vec3::new(0.82, 0.52, 0.72)),
                ProceduralMaterialFamily::Wood,
                Transform::from_translation(Vec3::new(-1.6, 0.32, 1.05)),
            ),
            part(
                "MarketCrateB",
                ProceduralShape::Cuboid(Vec3::new(0.64, 0.46, 0.64)),
                ProceduralMaterialFamily::Stone,
                Transform::from_translation(Vec3::new(1.55, 0.28, 0.95)),
            ),
            part(
                "MarketClayJar",
                ProceduralShape::Cylinder {
                    radius: 0.22,
                    depth: 0.52,
                },
                ProceduralMaterialFamily::Stone,
                Transform::from_translation(Vec3::new(0.95, 0.32, 1.05)),
            ),
            part(
                "MarketHangingScale",
                ProceduralShape::Cuboid(Vec3::new(0.86, 0.05, 0.05)),
                ProceduralMaterialFamily::WarmLight,
                Transform::from_translation(Vec3::new(0.0, 1.62, -1.22)),
            ),
            animated_part(
                "MarketCanopyEdge",
                ProceduralShape::Cuboid(Vec3::new(5.0, 0.08, 0.12)),
                ProceduralMaterialFamily::Cloth,
                Transform::from_translation(Vec3::new(0.0, 1.92, -1.65)),
                ProceduralAnimationRole::ClothCanopy,
            ),
        ],
        ProceduralAssetLod::Mid => vec![
            part(
                "MarketMidCounter",
                ProceduralShape::Cuboid(Vec3::new(4.8, 0.7, 1.4)),
                ProceduralMaterialFamily::Wood,
                Transform::from_translation(Vec3::Y * 0.45),
            ),
            part(
                "MarketMidCanopy",
                ProceduralShape::Cuboid(Vec3::new(5.4, 0.15, 3.0)),
                ProceduralMaterialFamily::Cloth,
                Transform::from_translation(Vec3::new(0.0, 2.1, -0.2)),
            ),
        ],
        ProceduralAssetLod::Far => vec![part(
            "MarketFarShape",
            ProceduralShape::Cuboid(Vec3::new(5.0, 1.5, 2.2)),
            ProceduralMaterialFamily::Cloth,
            Transform::from_translation(Vec3::Y * 0.9),
        )],
    }
}

fn shore_parts(lod: ProceduralAssetLod) -> Vec<ProceduralPartBlueprint> {
    let mut parts = vec![
        animated_part(
            "ShoreWater",
            ProceduralShape::Cylinder {
                radius: 18.0,
                depth: 0.04,
            },
            ProceduralMaterialFamily::Water,
            Transform::from_translation(Vec3::Y * 0.03).with_scale(Vec3::new(1.45, 1.0, 0.52)),
            ProceduralAnimationRole::WaterRipple,
        ),
        part(
            "ShoreWetSand",
            ProceduralShape::Cuboid(Vec3::new(28.0, 0.08, 5.0)),
            ProceduralMaterialFamily::Sand,
            Transform::from_translation(Vec3::new(0.0, 0.05, -5.5)),
        ),
    ];
    if lod == ProceduralAssetLod::Near {
        parts.extend([
            part(
                "ShoreOldPostA",
                ProceduralShape::Cylinder {
                    radius: 0.16,
                    depth: 1.3,
                },
                ProceduralMaterialFamily::Wood,
                Transform::from_translation(Vec3::new(-5.2, 0.65, -2.8))
                    .with_rotation(Quat::from_rotation_z(0.12)),
            ),
            part(
                "ShoreOldPostB",
                ProceduralShape::Cylinder {
                    radius: 0.14,
                    depth: 1.0,
                },
                ProceduralMaterialFamily::Wood,
                Transform::from_translation(Vec3::new(5.8, 0.5, -3.1))
                    .with_rotation(Quat::from_rotation_z(-0.18)),
            ),
            animated_part(
                "ShoreFoamLineA",
                ProceduralShape::Cuboid(Vec3::new(9.5, 0.025, 0.16)),
                ProceduralMaterialFamily::WarmLight,
                Transform::from_translation(Vec3::new(-4.2, 0.08, -4.25))
                    .with_rotation(Quat::from_rotation_y(0.08)),
                ProceduralAnimationRole::WaterRipple,
            ),
            animated_part(
                "ShoreFoamLineB",
                ProceduralShape::Cuboid(Vec3::new(7.4, 0.025, 0.12)),
                ProceduralMaterialFamily::WarmLight,
                Transform::from_translation(Vec3::new(5.0, 0.09, -4.85))
                    .with_rotation(Quat::from_rotation_y(-0.1)),
                ProceduralAnimationRole::WaterRipple,
            ),
        ]);
    }
    parts
}

fn path_stone_parts(spec: &ProceduralAssetSpec) -> Vec<ProceduralPartBlueprint> {
    vec![
        part(
            "PathStone",
            ProceduralShape::Cylinder {
                radius: spec.base_size.x * 0.5,
                depth: spec.base_size.y,
            },
            ProceduralMaterialFamily::Stone,
            Transform::from_translation(Vec3::Y * spec.base_size.y * 0.5),
        ),
        part(
            "PathDustPatch",
            ProceduralShape::Cuboid(Vec3::new(
                spec.base_size.x * 1.6,
                0.025,
                spec.base_size.z * 0.7,
            )),
            ProceduralMaterialFamily::Sand,
            Transform::from_translation(Vec3::Y * 0.015),
        ),
    ]
}

fn sheep_parts(lod: ProceduralAssetLod) -> Vec<ProceduralPartBlueprint> {
    match lod {
        ProceduralAssetLod::Near => vec![
            part(
                "SheepWoolBody",
                ProceduralShape::Capsule {
                    radius: 0.42,
                    depth: 0.72,
                },
                ProceduralMaterialFamily::Wool,
                Transform::from_scale(Vec3::new(1.2, 0.82, 0.82)),
            ),
            part(
                "SheepWoolShoulder",
                ProceduralShape::Sphere { radius: 0.18 },
                ProceduralMaterialFamily::Wool,
                Transform::from_translation(Vec3::new(-0.28, 0.04, -0.08))
                    .with_scale(Vec3::new(1.05, 0.78, 1.0)),
            ),
            part(
                "SheepWoolFlank",
                ProceduralShape::Sphere { radius: 0.2 },
                ProceduralMaterialFamily::Wool,
                Transform::from_translation(Vec3::new(0.28, -0.02, 0.18))
                    .with_scale(Vec3::new(1.0, 0.72, 1.05)),
            ),
            animated_part(
                "SheepHead",
                ProceduralShape::Sphere { radius: 0.24 },
                ProceduralMaterialFamily::Wool,
                Transform::from_translation(Vec3::new(0.0, 0.14, -0.58))
                    .with_scale(Vec3::new(0.85, 0.72, 1.0)),
                ProceduralAnimationRole::SheepHead,
            ),
            part(
                "SheepEarLeft",
                ProceduralShape::Cuboid(Vec3::new(0.08, 0.18, 0.04)),
                ProceduralMaterialFamily::Wool,
                Transform::from_translation(Vec3::new(-0.2, 0.2, -0.62))
                    .with_rotation(Quat::from_rotation_z(-0.45)),
            ),
            part(
                "SheepEarRight",
                ProceduralShape::Cuboid(Vec3::new(0.08, 0.18, 0.04)),
                ProceduralMaterialFamily::Wool,
                Transform::from_translation(Vec3::new(0.2, 0.2, -0.62))
                    .with_rotation(Quat::from_rotation_z(0.45)),
            ),
            animated_part(
                "SheepLegFrontLeft",
                ProceduralShape::Cuboid(Vec3::new(0.11, 0.48, 0.11)),
                ProceduralMaterialFamily::Wood,
                Transform::from_translation(Vec3::new(-0.24, -0.48, -0.2)),
                ProceduralAnimationRole::SheepLegFrontLeft,
            ),
            animated_part(
                "SheepLegFrontRight",
                ProceduralShape::Cuboid(Vec3::new(0.11, 0.48, 0.11)),
                ProceduralMaterialFamily::Wood,
                Transform::from_translation(Vec3::new(0.24, -0.48, -0.2)),
                ProceduralAnimationRole::SheepLegFrontRight,
            ),
            animated_part(
                "SheepLegBackLeft",
                ProceduralShape::Cuboid(Vec3::new(0.11, 0.48, 0.11)),
                ProceduralMaterialFamily::Wood,
                Transform::from_translation(Vec3::new(-0.24, -0.48, 0.28)),
                ProceduralAnimationRole::SheepLegBackLeft,
            ),
            animated_part(
                "SheepLegBackRight",
                ProceduralShape::Cuboid(Vec3::new(0.11, 0.48, 0.11)),
                ProceduralMaterialFamily::Wood,
                Transform::from_translation(Vec3::new(0.24, -0.48, 0.28)),
                ProceduralAnimationRole::SheepLegBackRight,
            ),
        ],
        ProceduralAssetLod::Mid => vec![part(
            "SheepMidBody",
            ProceduralShape::Capsule {
                radius: 0.42,
                depth: 0.72,
            },
            ProceduralMaterialFamily::Wool,
            Transform::from_scale(Vec3::new(1.2, 0.82, 0.82)),
        )],
        ProceduralAssetLod::Far => vec![part(
            "SheepFarWool",
            ProceduralShape::Sphere { radius: 0.34 },
            ProceduralMaterialFamily::Wool,
            Transform::from_scale(Vec3::new(1.4, 0.82, 0.82)),
        )],
    }
}

fn bird_parts(lod: ProceduralAssetLod) -> Vec<ProceduralPartBlueprint> {
    match lod {
        ProceduralAssetLod::Near => vec![
            part(
                "BirdBody",
                ProceduralShape::Capsule {
                    radius: 0.1,
                    depth: 0.28,
                },
                ProceduralMaterialFamily::BirdFeather,
                Transform::from_scale(Vec3::new(1.2, 0.5, 0.7)),
            ),
            animated_part(
                "BirdLeftWing",
                ProceduralShape::Cuboid(Vec3::new(0.52, 0.025, 0.16)),
                ProceduralMaterialFamily::BirdFeather,
                Transform::from_translation(Vec3::new(-0.34, 0.0, 0.0))
                    .with_rotation(Quat::from_rotation_z(-0.18)),
                ProceduralAnimationRole::BirdLeftWing,
            ),
            animated_part(
                "BirdRightWing",
                ProceduralShape::Cuboid(Vec3::new(0.52, 0.025, 0.16)),
                ProceduralMaterialFamily::BirdFeather,
                Transform::from_translation(Vec3::new(0.34, 0.0, 0.0))
                    .with_rotation(Quat::from_rotation_z(0.18)),
                ProceduralAnimationRole::BirdRightWing,
            ),
            part(
                "BirdTailFork",
                ProceduralShape::Cuboid(Vec3::new(0.08, 0.025, 0.18)),
                ProceduralMaterialFamily::BirdFeather,
                Transform::from_translation(Vec3::new(0.0, 0.0, 0.22))
                    .with_rotation(Quat::from_rotation_x(0.18)),
            ),
        ],
        ProceduralAssetLod::Mid => vec![part(
            "BirdMidWingLine",
            ProceduralShape::Cuboid(Vec3::new(0.72, 0.025, 0.16)),
            ProceduralMaterialFamily::BirdFeather,
            Transform::IDENTITY,
        )],
        ProceduralAssetLod::Far => vec![part(
            "BirdFarSpeck",
            ProceduralShape::Sphere { radius: 0.08 },
            ProceduralMaterialFamily::BirdFeather,
            Transform::IDENTITY,
        )],
    }
}

fn fish_parts(lod: ProceduralAssetLod) -> Vec<ProceduralPartBlueprint> {
    let body = part(
        "FishBody",
        ProceduralShape::Sphere { radius: 0.22 },
        ProceduralMaterialFamily::FishScale,
        Transform::from_scale(Vec3::new(1.8, 0.42, 0.72)),
    );
    if lod == ProceduralAssetLod::Near {
        vec![
            body,
            animated_part(
                "FishTail",
                ProceduralShape::Cuboid(Vec3::new(0.08, 0.18, 0.28)),
                ProceduralMaterialFamily::FishScale,
                Transform::from_translation(Vec3::new(0.0, 0.0, 0.34))
                    .with_rotation(Quat::from_rotation_y(0.55)),
                ProceduralAnimationRole::FishTail,
            ),
            part(
                "FishFlashSide",
                ProceduralShape::Cuboid(Vec3::new(0.04, 0.12, 0.34)),
                ProceduralMaterialFamily::WarmLight,
                Transform::from_translation(Vec3::new(0.18, 0.02, -0.02)),
            ),
        ]
    } else {
        vec![body]
    }
}

fn npc_parts(kind: ProceduralAssetKind, lod: ProceduralAssetLod) -> Vec<ProceduralPartBlueprint> {
    let cloth = match kind {
        ProceduralAssetKind::FortuneTeller => ProceduralMaterialFamily::Shadow,
        _ => ProceduralMaterialFamily::NpcCloth,
    };
    let accent = match kind {
        ProceduralAssetKind::Shepherd => ProceduralMaterialFamily::Wood,
        ProceduralAssetKind::Merchant => ProceduralMaterialFamily::Cloth,
        ProceduralAssetKind::FortuneTeller => ProceduralMaterialFamily::WarmLight,
        _ => ProceduralMaterialFamily::NpcCloth,
    };
    let mut near_parts = vec![
        part(
            "NpcBody",
            ProceduralShape::Capsule {
                radius: 0.36,
                depth: 1.25,
            },
            cloth,
            Transform::from_translation(Vec3::Y * 0.02),
        ),
        animated_part(
            "NpcHead",
            ProceduralShape::Sphere { radius: 0.22 },
            ProceduralMaterialFamily::NpcCloth,
            Transform::from_translation(Vec3::Y * 0.86),
            ProceduralAnimationRole::NpcHead,
        ),
        part(
            "NpcSemanticAccent",
            ProceduralShape::Cuboid(Vec3::new(0.14, 0.82, 0.14)),
            accent,
            Transform::from_translation(Vec3::new(0.44, 0.24, -0.08))
                .with_rotation(Quat::from_rotation_z(0.12)),
        ),
        animated_part(
            "NpcHandLeft",
            ProceduralShape::Sphere { radius: 0.08 },
            accent,
            Transform::from_translation(Vec3::new(-0.42, 0.26, -0.06)),
            ProceduralAnimationRole::NpcHandLeft,
        ),
        animated_part(
            "NpcHandRight",
            ProceduralShape::Sphere { radius: 0.08 },
            accent,
            Transform::from_translation(Vec3::new(0.42, 0.26, -0.06)),
            ProceduralAnimationRole::NpcHandRight,
        ),
    ];
    match kind {
        ProceduralAssetKind::Shepherd => near_parts.push(part(
            "ShepherdCrook",
            ProceduralShape::Cuboid(Vec3::new(0.08, 1.45, 0.08)),
            ProceduralMaterialFamily::Wood,
            Transform::from_translation(Vec3::new(0.58, 0.28, -0.12))
                .with_rotation(Quat::from_rotation_z(0.14)),
        )),
        ProceduralAssetKind::Merchant => near_parts.extend([
            part(
                "MerchantPack",
                ProceduralShape::Cuboid(Vec3::new(0.48, 0.48, 0.22)),
                ProceduralMaterialFamily::Cloth,
                Transform::from_translation(Vec3::new(0.0, 0.2, 0.42)),
            ),
            part(
                "MerchantLedger",
                ProceduralShape::Cuboid(Vec3::new(0.34, 0.05, 0.24)),
                ProceduralMaterialFamily::Wood,
                Transform::from_translation(Vec3::new(-0.38, 0.42, -0.18)),
            ),
        ]),
        ProceduralAssetKind::FortuneTeller => near_parts.extend([
            part(
                "FortuneLamp",
                ProceduralShape::Sphere { radius: 0.15 },
                ProceduralMaterialFamily::WarmLight,
                Transform::from_translation(Vec3::new(0.52, 0.08, -0.22)),
            ),
            part(
                "FortuneStillVeil",
                ProceduralShape::Cuboid(Vec3::new(0.62, 0.06, 0.5)),
                ProceduralMaterialFamily::Shadow,
                Transform::from_translation(Vec3::new(0.0, 0.68, -0.04)),
            ),
        ]),
        _ => {}
    }
    match lod {
        ProceduralAssetLod::Near => near_parts,
        ProceduralAssetLod::Mid => vec![part(
            "NpcMidBody",
            ProceduralShape::Capsule {
                radius: 0.36,
                depth: 1.25,
            },
            cloth,
            Transform::IDENTITY,
        )],
        ProceduralAssetLod::Far => vec![part(
            "NpcFarColumn",
            ProceduralShape::Cuboid(Vec3::new(0.44, 1.42, 0.44)),
            cloth,
            Transform::from_translation(Vec3::Y * 0.04),
        )],
    }
}

fn pyramid_parts(
    spec: &ProceduralAssetSpec,
    lod: ProceduralAssetLod,
) -> Vec<ProceduralPartBlueprint> {
    let width = spec.base_size.x;
    let height = spec.base_size.y;
    match lod {
        ProceduralAssetLod::Near => {
            let mut parts = vec![
                part(
                    "PyramidCore",
                    ProceduralShape::Pyramid { width, height },
                    ProceduralMaterialFamily::DesertStone,
                    Transform::IDENTITY,
                ),
                part(
                    "PyramidBuriedBase",
                    ProceduralShape::Cuboid(Vec3::new(width * 1.08, height * 0.06, width * 1.08)),
                    ProceduralMaterialFamily::Sand,
                    Transform::from_translation(Vec3::Y * height * 0.03),
                ),
                part(
                    "PyramidEntranceShadow",
                    ProceduralShape::Cuboid(Vec3::new(width * 0.14, height * 0.12, 0.45)),
                    ProceduralMaterialFamily::Shadow,
                    Transform::from_translation(Vec3::new(0.0, height * 0.12, -width * 0.51)),
                ),
                part(
                    "PyramidLeftWornEdge",
                    ProceduralShape::Cuboid(Vec3::new(width * 0.04, height * 0.52, width * 0.035)),
                    ProceduralMaterialFamily::OldStone,
                    Transform::from_translation(Vec3::new(
                        -width * 0.33,
                        height * 0.27,
                        -width * 0.22,
                    ))
                    .with_rotation(Quat::from_rotation_z(-0.2)),
                ),
                part(
                    "PyramidRightWornEdge",
                    ProceduralShape::Cuboid(Vec3::new(width * 0.035, height * 0.42, width * 0.035)),
                    ProceduralMaterialFamily::OldStone,
                    Transform::from_translation(Vec3::new(
                        width * 0.28,
                        height * 0.22,
                        width * 0.18,
                    ))
                    .with_rotation(Quat::from_rotation_z(0.24)),
                ),
                part(
                    "PyramidFallenBlockA",
                    ProceduralShape::Cuboid(Vec3::new(width * 0.08, height * 0.035, width * 0.06)),
                    ProceduralMaterialFamily::OldStone,
                    Transform::from_translation(Vec3::new(
                        -width * 0.48,
                        height * 0.04,
                        -width * 0.48,
                    ))
                    .with_rotation(Quat::from_rotation_y(0.62)),
                ),
                part(
                    "PyramidFallenBlockB",
                    ProceduralShape::Cuboid(Vec3::new(width * 0.06, height * 0.03, width * 0.05)),
                    ProceduralMaterialFamily::OldStone,
                    Transform::from_translation(Vec3::new(
                        width * 0.45,
                        height * 0.035,
                        -width * 0.38,
                    ))
                    .with_rotation(Quat::from_rotation_y(-0.44)),
                ),
            ];
            for index in 0..7 {
                let level = index as f32 / 7.0;
                parts.push(part(
                    "PyramidStep",
                    ProceduralShape::Cuboid(Vec3::new(
                        width * (1.0 - level * 0.72),
                        0.28,
                        width * 0.035,
                    )),
                    ProceduralMaterialFamily::DesertStone,
                    Transform::from_translation(Vec3::new(
                        0.0,
                        height * (0.08 + level * 0.58),
                        -width * (0.48 - level * 0.34),
                    )),
                ));
            }
            parts
        }
        ProceduralAssetLod::Mid => vec![part(
            "PyramidMidSilhouette",
            ProceduralShape::Pyramid { width, height },
            ProceduralMaterialFamily::DesertStone,
            Transform::IDENTITY,
        )],
        ProceduralAssetLod::Far => vec![part(
            "PyramidFarSilhouette",
            ProceduralShape::Pyramid {
                width: width * 0.92,
                height: height * 0.82,
            },
            ProceduralMaterialFamily::DesertStone,
            Transform::IDENTITY,
        )],
    }
}

fn oasis_parts(
    spec: &ProceduralAssetSpec,
    lod: ProceduralAssetLod,
) -> Vec<ProceduralPartBlueprint> {
    let radius = spec.base_size.x * 0.5;
    let mut parts = vec![part(
        "OasisWater",
        ProceduralShape::Cylinder {
            radius,
            depth: spec.base_size.y,
        },
        ProceduralMaterialFamily::Water,
        Transform::from_scale(Vec3::new(1.8, 1.0, 0.72)),
    )];
    if lod == ProceduralAssetLod::Near {
        parts.extend([
            part(
                "OasisWetShore",
                ProceduralShape::Cylinder {
                    radius: radius * 1.12,
                    depth: 0.04,
                },
                ProceduralMaterialFamily::Sand,
                Transform::from_translation(Vec3::Y * -0.035).with_scale(Vec3::new(1.9, 1.0, 0.78)),
            ),
            part(
                "OasisPalmTrunk",
                ProceduralShape::Cylinder {
                    radius: 0.18,
                    depth: 3.2,
                },
                ProceduralMaterialFamily::Wood,
                Transform::from_translation(Vec3::new(-radius * 0.62, 1.55, radius * 0.18))
                    .with_rotation(Quat::from_rotation_z(0.16)),
            ),
            part(
                "OasisPalmCrown",
                ProceduralShape::Sphere { radius: 0.9 },
                ProceduralMaterialFamily::Cloth,
                Transform::from_translation(Vec3::new(-radius * 0.72, 3.24, radius * 0.18))
                    .with_scale(Vec3::new(1.4, 0.28, 1.0)),
            ),
        ]);
    }
    parts
}

fn ruin_wall_parts(
    spec: &ProceduralAssetSpec,
    lod: ProceduralAssetLod,
) -> Vec<ProceduralPartBlueprint> {
    let size = spec.base_size;
    let mut parts = vec![part(
        "RuinWallCore",
        ProceduralShape::Cuboid(size),
        ProceduralMaterialFamily::OldStone,
        Transform::from_translation(Vec3::Y * (size.y * 0.5)),
    )];
    if lod == ProceduralAssetLod::Near {
        parts.extend([
            part(
                "RuinWallBrokenCap",
                ProceduralShape::Cuboid(Vec3::new(size.x * 0.36, size.y * 0.2, size.z * 1.06)),
                ProceduralMaterialFamily::OldStone,
                Transform::from_translation(Vec3::new(-size.x * 0.28, size.y * 1.08, 0.0))
                    .with_rotation(Quat::from_rotation_z(0.08)),
            ),
            part(
                "RuinWallFallenStone",
                ProceduralShape::Cuboid(Vec3::new(size.z * 1.2, size.y * 0.22, size.z)),
                ProceduralMaterialFamily::OldStone,
                Transform::from_translation(Vec3::new(size.x * 0.32, 0.18, size.z * 1.24))
                    .with_rotation(Quat::from_rotation_y(0.58)),
            ),
        ]);
    }
    parts
}

fn relic_parts(lod: ProceduralAssetLod) -> Vec<ProceduralPartBlueprint> {
    let mut parts = vec![part(
        "RelicStone",
        ProceduralShape::Cuboid(Vec3::new(1.2, 2.2, 1.2)),
        ProceduralMaterialFamily::Relic,
        Transform::from_translation(Vec3::Y * 1.1),
    )];
    if lod == ProceduralAssetLod::Near {
        parts.push(part(
            "RelicGlowLine",
            ProceduralShape::Cuboid(Vec3::new(0.08, 1.4, 0.04)),
            ProceduralMaterialFamily::WarmLight,
            Transform::from_translation(Vec3::new(0.0, 1.2, -0.62)),
        ));
    }
    parts
}

fn mist_river_parts(
    spec: &ProceduralAssetSpec,
    lod: ProceduralAssetLod,
) -> Vec<ProceduralPartBlueprint> {
    let radius = spec.base_size.x * 0.5;
    let mut parts = vec![part(
        "MistRiverWater",
        ProceduralShape::Cylinder {
            radius,
            depth: spec.base_size.y,
        },
        ProceduralMaterialFamily::Water,
        Transform::from_scale(Vec3::new(1.8, 1.0, 0.38)),
    )];
    if lod == ProceduralAssetLod::Near {
        parts.push(part(
            "MistRiverPaleBank",
            ProceduralShape::Cylinder {
                radius: radius * 1.08,
                depth: 0.04,
            },
            ProceduralMaterialFamily::Sand,
            Transform::from_translation(Vec3::Y * -0.03).with_scale(Vec3::new(1.86, 1.0, 0.42)),
        ));
    }
    parts
}

fn headland_marker_parts(
    spec: &ProceduralAssetSpec,
    lod: ProceduralAssetLod,
) -> Vec<ProceduralPartBlueprint> {
    let height = spec.base_size.y;
    let mut parts = vec![part(
        "HeadlandMarkerColumn",
        ProceduralShape::Cylinder {
            radius: spec.base_size.x * 0.5,
            depth: height * 0.8,
        },
        ProceduralMaterialFamily::OldStone,
        Transform::from_translation(Vec3::Y * height * 0.4),
    )];
    if lod != ProceduralAssetLod::Far {
        parts.push(part(
            "HeadlandMarkerLight",
            ProceduralShape::Sphere { radius: 0.46 },
            ProceduralMaterialFamily::WarmLight,
            Transform::from_translation(Vec3::Y * height * 0.86),
        ));
    }
    parts
}

fn pyramid_mesh(width: f32, height: f32) -> Mesh {
    let half = width * 0.5;
    let positions = vec![
        [-half, 0.0, -half],
        [half, 0.0, -half],
        [half, 0.0, half],
        [-half, 0.0, half],
        [0.0, height, 0.0],
    ];
    let normals = vec![
        [0.0, -1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, -1.0, 0.0],
        [0.0, 1.0, 0.0],
    ];
    let uvs = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0], [0.5, 0.5]];
    let indices = vec![0, 2, 1, 0, 3, 2, 0, 1, 4, 1, 2, 4, 2, 3, 4, 3, 0, 4];
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    mesh
}

#[derive(Debug, Clone, PartialEq)]
pub struct ProceduralAssetSpec {
    pub kind: ProceduralAssetKind,
    pub stable_id: u64,
    pub seed: u64,
    pub base_size: Vec3,
    pub material_family: ProceduralMaterialFamily,
    pub semantics: Vec<ProceduralSemantic>,
    pub lod: ProceduralLodStrategy,
    pub collision: ProceduralCollision,
}

impl ProceduralAssetSpec {
    pub fn instance(&self, seed_salt: u64) -> Self {
        let mut spec = self.clone();
        spec.seed = stable_asset_seed(self.seed, seed_salt);
        spec.stable_id = stable_asset_id(spec.kind, spec.seed);
        spec
    }

    pub fn with_size(mut self, base_size: Vec3) -> Self {
        self.base_size = base_size;
        self
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum ProceduralAssetKind {
    VillageHouse,
    VillageWell,
    SheepPenRail,
    MarketStall,
    VillageShore,
    PathStone,
    Sheep,
    Shepherd,
    Merchant,
    Bird,
    Fish,
    FortuneTeller,
    DesertPyramid,
    DesertOasis,
    PyramidRuinWall,
    DesertRelic,
    MistRiver,
    HeadlandMarker,
}

impl ProceduralAssetKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::VillageHouse => "village_house",
            Self::VillageWell => "village_well",
            Self::SheepPenRail => "sheep_pen_rail",
            Self::MarketStall => "market_stall",
            Self::VillageShore => "village_shore",
            Self::PathStone => "path_stone",
            Self::Sheep => "sheep",
            Self::Shepherd => "shepherd",
            Self::Merchant => "merchant",
            Self::Bird => "bird",
            Self::Fish => "fish",
            Self::FortuneTeller => "fortune_teller",
            Self::DesertPyramid => "desert_pyramid",
            Self::DesertOasis => "desert_oasis",
            Self::PyramidRuinWall => "pyramid_ruin_wall",
            Self::DesertRelic => "desert_relic",
            Self::MistRiver => "mist_river",
            Self::HeadlandMarker => "headland_marker",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum ProceduralSemantic {
    Village,
    Home,
    Flock,
    Trade,
    Water,
    Shore,
    Path,
    Animal,
    Npc,
    Boundary,
    Dream,
    Desert,
    Pyramid,
    Oasis,
    Ruin,
    Relic,
    Omen,
    Memory,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum ProceduralMaterialFamily {
    MudWall,
    DarkRoof,
    Wood,
    Stone,
    Cloth,
    Water,
    Sand,
    Wool,
    NpcCloth,
    BirdFeather,
    FishScale,
    OldStone,
    Relic,
    DesertStone,
    WarmLight,
    Shadow,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProceduralLodStrategy {
    Fixed,
    LocalDetail {
        near_distance: f32,
        mid_distance: f32,
        far_distance: f32,
    },
    Character {
        visible_distance: f32,
        update_interval_seconds: f32,
    },
    Landmark {
        near_distance: f32,
        silhouette_distance: f32,
        impostor_distance: f32,
    },
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum ProceduralCollision {
    None,
    VisualOnly,
    SimpleBlocker,
    InteractionProxy,
}

pub fn registered_spec(kind: ProceduralAssetKind) -> ProceduralAssetSpec {
    core_asset_specs()
        .into_iter()
        .find(|spec| spec.kind == kind)
        .unwrap_or_else(|| fallback_spec(kind))
}

pub fn stable_asset_id(kind: ProceduralAssetKind, seed: u64) -> u64 {
    stable_asset_seed(seed, kind as u64 + 1)
}

pub fn stable_asset_seed(seed: u64, salt: u64) -> u64 {
    let mut value = seed
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(salt.wrapping_mul(0xBF58_476D_1CE4_E5B9));
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn report_procedural_asset_runtime_stats(
    mut stats: ResMut<ProceduralAssetRuntimeStats>,
    query: Query<&ProceduralAsset>,
    mut performance: ResMut<FramePerformance>,
) {
    let started_at = std::time::Instant::now();
    let asset_count = query.iter().count();
    if asset_count == 0 {
        performance.record_phase_duration(PerformancePhase::Assets, started_at.elapsed());
        return;
    }

    let mut kinds = HashSet::new();
    let mut semantics = HashSet::new();
    for asset in &query {
        kinds.insert(asset.spec.kind);
        semantics.extend(asset.spec.semantics.iter().copied());
    }

    if stats.last_asset_count == asset_count
        && stats.last_kind_count == kinds.len()
        && stats.last_semantic_count == semantics.len()
    {
        performance.record_phase_duration(PerformancePhase::Assets, started_at.elapsed());
        return;
    }

    stats.last_asset_count = asset_count;
    stats.last_kind_count = kinds.len();
    stats.last_semantic_count = semantics.len();

    tracing::info!(
        target: "dao_game::assets::runtime",
        asset_count,
        asset_kind_count = kinds.len(),
        semantic_count = semantics.len(),
        "procedural asset semantics attached"
    );
    performance.record_phase_duration(PerformancePhase::Assets, started_at.elapsed());
}

fn cleanup_asset_runtime_stats(mut commands: Commands) {
    commands.insert_resource(ProceduralAssetRuntimeStats::default());
}

fn core_asset_specs() -> Vec<ProceduralAssetSpec> {
    vec![
        spec(
            ProceduralAssetKind::VillageHouse,
            Vec3::new(5.4, 3.2, 4.6),
            ProceduralMaterialFamily::MudWall,
            vec![ProceduralSemantic::Village, ProceduralSemantic::Home],
            ProceduralLodStrategy::LocalDetail {
                near_distance: 36.0,
                mid_distance: 120.0,
                far_distance: 260.0,
            },
            ProceduralCollision::SimpleBlocker,
        ),
        spec(
            ProceduralAssetKind::VillageWell,
            Vec3::new(2.2, 0.9, 2.2),
            ProceduralMaterialFamily::Stone,
            vec![
                ProceduralSemantic::Village,
                ProceduralSemantic::Water,
                ProceduralSemantic::Memory,
            ],
            ProceduralLodStrategy::LocalDetail {
                near_distance: 24.0,
                mid_distance: 70.0,
                far_distance: 150.0,
            },
            ProceduralCollision::InteractionProxy,
        ),
        spec(
            ProceduralAssetKind::SheepPenRail,
            Vec3::new(15.5, 0.22, 0.24),
            ProceduralMaterialFamily::Wood,
            vec![ProceduralSemantic::Village, ProceduralSemantic::Flock],
            ProceduralLodStrategy::LocalDetail {
                near_distance: 30.0,
                mid_distance: 95.0,
                far_distance: 180.0,
            },
            ProceduralCollision::SimpleBlocker,
        ),
        spec(
            ProceduralAssetKind::MarketStall,
            Vec3::new(5.4, 2.25, 3.0),
            ProceduralMaterialFamily::Cloth,
            vec![ProceduralSemantic::Village, ProceduralSemantic::Trade],
            ProceduralLodStrategy::LocalDetail {
                near_distance: 32.0,
                mid_distance: 100.0,
                far_distance: 200.0,
            },
            ProceduralCollision::InteractionProxy,
        ),
        spec(
            ProceduralAssetKind::VillageShore,
            Vec3::new(28.0, 0.08, 18.0),
            ProceduralMaterialFamily::Water,
            vec![
                ProceduralSemantic::Village,
                ProceduralSemantic::Shore,
                ProceduralSemantic::Water,
            ],
            ProceduralLodStrategy::Fixed,
            ProceduralCollision::None,
        ),
        spec(
            ProceduralAssetKind::PathStone,
            Vec3::new(0.9, 0.16, 0.9),
            ProceduralMaterialFamily::Stone,
            vec![ProceduralSemantic::Path, ProceduralSemantic::Boundary],
            ProceduralLodStrategy::LocalDetail {
                near_distance: 22.0,
                mid_distance: 70.0,
                far_distance: 120.0,
            },
            ProceduralCollision::VisualOnly,
        ),
        spec(
            ProceduralAssetKind::Sheep,
            Vec3::new(1.0, 1.0, 0.7),
            ProceduralMaterialFamily::Wool,
            vec![
                ProceduralSemantic::Animal,
                ProceduralSemantic::Flock,
                ProceduralSemantic::Village,
            ],
            ProceduralLodStrategy::Character {
                visible_distance: 120.0,
                update_interval_seconds: 0.066,
            },
            ProceduralCollision::InteractionProxy,
        ),
        spec(
            ProceduralAssetKind::Shepherd,
            Vec3::new(0.72, 1.6, 0.72),
            ProceduralMaterialFamily::NpcCloth,
            vec![
                ProceduralSemantic::Npc,
                ProceduralSemantic::Flock,
                ProceduralSemantic::Village,
            ],
            ProceduralLodStrategy::Character {
                visible_distance: 150.0,
                update_interval_seconds: 0.1,
            },
            ProceduralCollision::InteractionProxy,
        ),
        spec(
            ProceduralAssetKind::Merchant,
            Vec3::new(0.72, 1.6, 0.72),
            ProceduralMaterialFamily::NpcCloth,
            vec![
                ProceduralSemantic::Npc,
                ProceduralSemantic::Trade,
                ProceduralSemantic::Village,
                ProceduralSemantic::Desert,
            ],
            ProceduralLodStrategy::Character {
                visible_distance: 150.0,
                update_interval_seconds: 0.1,
            },
            ProceduralCollision::InteractionProxy,
        ),
        spec(
            ProceduralAssetKind::Bird,
            Vec3::new(0.42, 0.12, 0.24),
            ProceduralMaterialFamily::BirdFeather,
            vec![ProceduralSemantic::Animal, ProceduralSemantic::Omen],
            ProceduralLodStrategy::Character {
                visible_distance: 240.0,
                update_interval_seconds: 0.066,
            },
            ProceduralCollision::None,
        ),
        spec(
            ProceduralAssetKind::Fish,
            Vec3::new(0.48, 0.18, 0.3),
            ProceduralMaterialFamily::FishScale,
            vec![ProceduralSemantic::Animal, ProceduralSemantic::Water],
            ProceduralLodStrategy::Character {
                visible_distance: 90.0,
                update_interval_seconds: 0.12,
            },
            ProceduralCollision::None,
        ),
        spec(
            ProceduralAssetKind::FortuneTeller,
            Vec3::new(0.76, 1.7, 0.76),
            ProceduralMaterialFamily::NpcCloth,
            vec![
                ProceduralSemantic::Npc,
                ProceduralSemantic::Dream,
                ProceduralSemantic::Omen,
            ],
            ProceduralLodStrategy::Character {
                visible_distance: 160.0,
                update_interval_seconds: 0.1,
            },
            ProceduralCollision::InteractionProxy,
        ),
        spec(
            ProceduralAssetKind::DesertPyramid,
            Vec3::new(74.0, 53.0, 74.0),
            ProceduralMaterialFamily::DesertStone,
            vec![
                ProceduralSemantic::Dream,
                ProceduralSemantic::Desert,
                ProceduralSemantic::Pyramid,
                ProceduralSemantic::Omen,
            ],
            ProceduralLodStrategy::Landmark {
                near_distance: 230.0,
                silhouette_distance: 1_250.0,
                impostor_distance: 2_600.0,
            },
            ProceduralCollision::SimpleBlocker,
        ),
        spec(
            ProceduralAssetKind::DesertOasis,
            Vec3::new(24.0, 0.08, 24.0),
            ProceduralMaterialFamily::Water,
            vec![
                ProceduralSemantic::Desert,
                ProceduralSemantic::Oasis,
                ProceduralSemantic::Water,
            ],
            ProceduralLodStrategy::LocalDetail {
                near_distance: 120.0,
                mid_distance: 360.0,
                far_distance: 800.0,
            },
            ProceduralCollision::None,
        ),
        spec(
            ProceduralAssetKind::PyramidRuinWall,
            Vec3::new(20.0, 2.4, 1.2),
            ProceduralMaterialFamily::OldStone,
            vec![
                ProceduralSemantic::Desert,
                ProceduralSemantic::Ruin,
                ProceduralSemantic::Memory,
            ],
            ProceduralLodStrategy::LocalDetail {
                near_distance: 110.0,
                mid_distance: 320.0,
                far_distance: 680.0,
            },
            ProceduralCollision::SimpleBlocker,
        ),
        spec(
            ProceduralAssetKind::DesertRelic,
            Vec3::new(1.2, 2.2, 1.2),
            ProceduralMaterialFamily::Relic,
            vec![
                ProceduralSemantic::Desert,
                ProceduralSemantic::Relic,
                ProceduralSemantic::Dream,
            ],
            ProceduralLodStrategy::LocalDetail {
                near_distance: 90.0,
                mid_distance: 220.0,
                far_distance: 480.0,
            },
            ProceduralCollision::InteractionProxy,
        ),
        spec(
            ProceduralAssetKind::MistRiver,
            Vec3::new(44.0, 0.08, 18.0),
            ProceduralMaterialFamily::Water,
            vec![
                ProceduralSemantic::Boundary,
                ProceduralSemantic::Water,
                ProceduralSemantic::Omen,
            ],
            ProceduralLodStrategy::Landmark {
                near_distance: 70.0,
                silhouette_distance: 160.0,
                impostor_distance: 320.0,
            },
            ProceduralCollision::None,
        ),
        spec(
            ProceduralAssetKind::HeadlandMarker,
            Vec3::new(2.0, 14.0, 2.0),
            ProceduralMaterialFamily::OldStone,
            vec![ProceduralSemantic::Boundary, ProceduralSemantic::Memory],
            ProceduralLodStrategy::Landmark {
                near_distance: 80.0,
                silhouette_distance: 260.0,
                impostor_distance: 520.0,
            },
            ProceduralCollision::VisualOnly,
        ),
    ]
}

fn spec(
    kind: ProceduralAssetKind,
    base_size: Vec3,
    material_family: ProceduralMaterialFamily,
    semantics: Vec<ProceduralSemantic>,
    lod: ProceduralLodStrategy,
    collision: ProceduralCollision,
) -> ProceduralAssetSpec {
    let seed = stable_asset_seed(511, kind as u64 + 17);
    ProceduralAssetSpec {
        kind,
        stable_id: stable_asset_id(kind, seed),
        seed,
        base_size,
        material_family,
        semantics,
        lod,
        collision,
    }
}

fn fallback_spec(kind: ProceduralAssetKind) -> ProceduralAssetSpec {
    spec(
        kind,
        Vec3::ONE,
        ProceduralMaterialFamily::Stone,
        vec![ProceduralSemantic::Memory],
        ProceduralLodStrategy::Fixed,
        ProceduralCollision::VisualOnly,
    )
}

#[cfg(test)]
mod tests {
    use super::{
        ProceduralAnimationRole, ProceduralAssetKind, ProceduralAssetLod, ProceduralAssetRegistry,
        ProceduralLodStrategy, ProceduralSemantic, asset_blueprint, choose_lod, registered_spec,
        stable_asset_id,
    };

    #[test]
    fn registry_contains_first_visual_asset_families() {
        let registry = ProceduralAssetRegistry::default();

        assert!(registry.spec(ProceduralAssetKind::VillageHouse).is_some());
        assert!(registry.spec(ProceduralAssetKind::DesertPyramid).is_some());
        assert!(registry.spec(ProceduralAssetKind::Bird).is_some());
        assert!(registry.spec(ProceduralAssetKind::FortuneTeller).is_some());
    }

    #[test]
    fn semantic_queries_find_cross_system_assets() {
        let registry = ProceduralAssetRegistry::default();
        let dream_assets = registry.by_semantic(ProceduralSemantic::Dream);

        assert!(
            dream_assets
                .iter()
                .any(|spec| spec.kind == ProceduralAssetKind::DesertPyramid)
        );
        assert!(
            dream_assets
                .iter()
                .any(|spec| spec.kind == ProceduralAssetKind::FortuneTeller)
        );
    }

    #[test]
    fn stable_ids_are_deterministic_and_kind_sensitive() {
        let pyramid = stable_asset_id(ProceduralAssetKind::DesertPyramid, 42);
        let pyramid_again = stable_asset_id(ProceduralAssetKind::DesertPyramid, 42);
        let oasis = stable_asset_id(ProceduralAssetKind::DesertOasis, 42);

        assert_eq!(pyramid, pyramid_again);
        assert_ne!(pyramid, oasis);
    }

    #[test]
    fn spec_instances_preserve_semantics_and_change_seed() {
        let base = registered_spec(ProceduralAssetKind::VillageHouse);
        let instance = base.instance(91);

        assert_eq!(base.kind, instance.kind);
        assert_eq!(base.semantics, instance.semantics);
        assert_ne!(base.seed, instance.seed);
        assert_ne!(base.stable_id, instance.stable_id);
    }

    #[test]
    fn landmark_assets_use_landmark_lod() {
        let pyramid = registered_spec(ProceduralAssetKind::DesertPyramid);

        assert!(matches!(
            pyramid.lod,
            ProceduralLodStrategy::Landmark {
                near_distance: 230.0,
                silhouette_distance: 1_250.0,
                impostor_distance: 2_600.0,
            }
        ));
    }

    #[test]
    fn every_registered_asset_has_spawnable_blueprints() {
        let registry = ProceduralAssetRegistry::default();

        for spec in registry.specs() {
            for lod in [
                ProceduralAssetLod::Near,
                ProceduralAssetLod::Mid,
                ProceduralAssetLod::Far,
            ] {
                assert!(
                    asset_blueprint(spec, lod).part_count() > 0,
                    "{} {lod:?} produced no parts",
                    spec.kind.label()
                );
            }
        }
    }

    #[test]
    fn village_house_and_pyramid_gain_near_detail() {
        let house = registered_spec(ProceduralAssetKind::VillageHouse);
        let pyramid = registered_spec(ProceduralAssetKind::DesertPyramid);

        assert!(
            asset_blueprint(&house, ProceduralAssetLod::Near).part_count()
                > asset_blueprint(&house, ProceduralAssetLod::Far).part_count()
        );
        assert!(
            asset_blueprint(&pyramid, ProceduralAssetLod::Near).part_count()
                > asset_blueprint(&pyramid, ProceduralAssetLod::Mid).part_count()
        );
    }

    #[test]
    fn animal_and_npc_blueprints_expose_animation_roles() {
        let sheep = registered_spec(ProceduralAssetKind::Sheep);
        let bird = registered_spec(ProceduralAssetKind::Bird);
        let fish = registered_spec(ProceduralAssetKind::Fish);
        let merchant = registered_spec(ProceduralAssetKind::Merchant);

        let sheep_roles: Vec<_> = asset_blueprint(&sheep, ProceduralAssetLod::Near)
            .parts
            .iter()
            .filter_map(|part| part.animation_role)
            .collect();
        assert!(sheep_roles.contains(&ProceduralAnimationRole::SheepHead));
        assert!(sheep_roles.contains(&ProceduralAnimationRole::SheepLegFrontLeft));
        assert!(
            asset_blueprint(&bird, ProceduralAssetLod::Near)
                .parts
                .iter()
                .any(|part| part.animation_role == Some(ProceduralAnimationRole::BirdLeftWing))
        );
        assert!(
            asset_blueprint(&fish, ProceduralAssetLod::Near)
                .parts
                .iter()
                .any(|part| part.animation_role == Some(ProceduralAnimationRole::FishTail))
        );
        assert!(
            asset_blueprint(&merchant, ProceduralAssetLod::Near)
                .parts
                .iter()
                .any(|part| part.name == "MerchantPack")
        );
    }

    #[test]
    fn lod_selection_respects_registered_distances() {
        let house = registered_spec(ProceduralAssetKind::VillageHouse);
        let pyramid = registered_spec(ProceduralAssetKind::DesertPyramid);

        assert_eq!(choose_lod(&house, 12.0), ProceduralAssetLod::Near);
        assert_eq!(choose_lod(&house, 80.0), ProceduralAssetLod::Mid);
        assert_eq!(choose_lod(&house, 220.0), ProceduralAssetLod::Far);
        assert_eq!(choose_lod(&pyramid, 120.0), ProceduralAssetLod::Near);
        assert_eq!(choose_lod(&pyramid, 900.0), ProceduralAssetLod::Mid);
        assert_eq!(choose_lod(&pyramid, 1_800.0), ProceduralAssetLod::Far);
    }
}
