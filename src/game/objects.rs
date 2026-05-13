use std::{
    fs,
    path::Path,
    time::{Instant, SystemTime, UNIX_EPOCH},
};

use avian3d::prelude::{Collider, CollisionEventsEnabled, RigidBody, Sensor};
use bevy::{
    ecs::system::SystemParam,
    math::primitives::{Cuboid, Cylinder, Plane3d, Sphere},
    pbr::MeshMaterial3d,
    prelude::*,
    render::view::screenshot::{Screenshot, save_to_disk},
};
use serde::Serialize;

use crate::{
    core::performance::{FramePerformance, PerformancePhase},
    game::{
        assets::{ProceduralAssetMaterials, ProceduralMaterialFamily},
        environment::WindField,
        flow::{AppScreen, InGameState, SessionMode, in_session_mode},
        gallery::{
            AssetCodexSlot, AssetCodexState, GalleryExportMode, GalleryExportQueue,
            GalleryExportStage, prepare_export_path,
        },
        materials::MaterialGalleryState,
        physics::{
            DaoCollider, DaoColliderRole, DaoColliderSource, DaoPhysicsLayer, DaoPhysicsSensor,
            DaoSensorKind, gallery_layers,
        },
        world::WorldCamera,
    },
};

pub struct ProceduralObjectPlugin;

impl Plugin for ProceduralObjectPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ProceduralObjectRegistry::default());
        app.insert_resource(ObjectGalleryState::default());
        app.insert_resource(ObjectWindAnimationState::default());
        app.add_systems(
            OnEnter(AppScreen::InGame),
            spawn_procedural_object_gallery.run_if(in_session_mode(SessionMode::MaterialGallery)),
        );
        app.add_systems(
            Update,
            (
                advance_object_gallery_export_frame,
                handle_object_gallery_input,
                process_object_gallery_export_queue,
                animate_procedural_tree_wind,
                refresh_object_gallery_codex,
            )
                .chain()
                .run_if(in_state(InGameState::Running))
                .run_if(in_session_mode(SessionMode::MaterialGallery)),
        );
        app.add_systems(OnExit(AppScreen::InGame), cleanup_procedural_object_gallery);
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize)]
#[repr(u8)]
pub enum ObjectKind {
    Tree = 0,
    Rock = 1,
    RuinFragment = 2,
    VillageProp = 3,
    StructurePart = 4,
    WatersideProp = 5,
    EcologyProp = 6,
    Interactable = 7,
}

impl ObjectKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Tree => "Tree",
            Self::Rock => "Rock",
            Self::RuinFragment => "RuinFragment",
            Self::VillageProp => "VillageProp",
            Self::StructurePart => "StructurePart",
            Self::WatersideProp => "WatersideProp",
            Self::EcologyProp => "EcologyProp",
            Self::Interactable => "Interactable",
        }
    }

    pub fn export_label(self) -> &'static str {
        match self {
            Self::Tree => "tree",
            Self::Rock => "rock",
            Self::RuinFragment => "ruin_fragment",
            Self::VillageProp => "village_prop",
            Self::StructurePart => "structure_part",
            Self::WatersideProp => "waterside_prop",
            Self::EcologyProp => "ecology_prop",
            Self::Interactable => "interactable",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize)]
#[repr(u8)]
pub enum ObjectSemantic {
    Vegetation = 0,
    Stone = 1,
    Ruin = 2,
    Village = 3,
    Waterside = 4,
    Ecology = 5,
    Interaction = 6,
    Omen = 7,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize)]
#[repr(u8)]
pub enum ObjectLod {
    Near = 0,
    Mid = 1,
    Far = 2,
}

impl ObjectLod {
    pub fn label(self) -> &'static str {
        match self {
            Self::Near => "近景",
            Self::Mid => "中景",
            Self::Far => "远景",
        }
    }

    pub fn export_label(self) -> &'static str {
        match self {
            Self::Near => "near",
            Self::Mid => "mid",
            Self::Far => "far",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize)]
#[repr(u8)]
pub enum ObjectBiomeContext {
    Meadow = 0,
    Wetland = 1,
    Ridge = 2,
    VillageCourtyard = 3,
    RuinEdge = 4,
    DesertWind = 5,
}

impl ObjectBiomeContext {
    pub fn label(self) -> &'static str {
        match self {
            Self::Meadow => "草甸",
            Self::Wetland => "湿地",
            Self::Ridge => "山脊",
            Self::VillageCourtyard => "村院",
            Self::RuinEdge => "遗迹边缘",
            Self::DesertWind => "沙漠风口",
        }
    }

    pub fn export_label(self) -> &'static str {
        match self {
            Self::Meadow => "meadow",
            Self::Wetland => "wetland",
            Self::Ridge => "ridge",
            Self::VillageCourtyard => "village_courtyard",
            Self::RuinEdge => "ruin_edge",
            Self::DesertWind => "desert_wind",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize)]
#[repr(u8)]
pub enum ObjectWeatherState {
    Clear = 0,
    RainSoaked = 1,
    DryWind = 2,
    DreamTint = 3,
}

impl ObjectWeatherState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Clear => "晴朗",
            Self::RainSoaked => "雨浸",
            Self::DryWind => "干风",
            Self::DreamTint => "梦境偏色",
        }
    }

    pub fn export_label(self) -> &'static str {
        match self {
            Self::Clear => "clear",
            Self::RainSoaked => "rain_soaked",
            Self::DryWind => "dry_wind",
            Self::DreamTint => "dream_tint",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize)]
#[repr(u8)]
pub enum ObjectMaterialVariant {
    Default = 0,
    Wet = 1,
    Dusty = 2,
    Mossy = 3,
    Dream = 4,
}

impl ObjectMaterialVariant {
    pub fn label(self) -> &'static str {
        match self {
            Self::Default => "默认",
            Self::Wet => "湿润",
            Self::Dusty => "浮尘",
            Self::Mossy => "苔藓",
            Self::Dream => "梦境",
        }
    }

    pub fn export_label(self) -> &'static str {
        match self {
            Self::Default => "default",
            Self::Wet => "wet",
            Self::Dusty => "dusty",
            Self::Mossy => "mossy",
            Self::Dream => "dream",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize)]
#[repr(u8)]
pub enum ObjectCollisionMode {
    VisualOnly = 0,
    TrunkOnly = 1,
    TrunkAndRoots = 2,
    Full = 3,
}

impl ObjectCollisionMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::VisualOnly => "仅表现",
            Self::TrunkOnly => "树干",
            Self::TrunkAndRoots => "树干+根部",
            Self::Full => "完整",
        }
    }

    pub fn export_label(self) -> &'static str {
        match self {
            Self::VisualOnly => "visual_only",
            Self::TrunkOnly => "trunk_only",
            Self::TrunkAndRoots => "trunk_and_roots",
            Self::Full => "full",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize)]
#[repr(u8)]
pub enum ObjectGenerationMode {
    Gallery = 0,
    World = 1,
}

impl ObjectGenerationMode {
    pub fn export_label(self) -> &'static str {
        match self {
            Self::Gallery => "gallery",
            Self::World => "world",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize)]
#[repr(u8)]
pub enum ObjectMaterialSlot {
    BarkPrimary = 0,
    BarkWornEdge = 1,
    LeafPrimary = 2,
    LeafSecondary = 3,
    LeafDry = 4,
    RootShadow = 5,
}

impl ObjectMaterialSlot {
    pub fn export_label(self) -> &'static str {
        match self {
            Self::BarkPrimary => "bark_primary",
            Self::BarkWornEdge => "bark_worn_edge",
            Self::LeafPrimary => "leaf_primary",
            Self::LeafSecondary => "leaf_secondary",
            Self::LeafDry => "leaf_dry",
            Self::RootShadow => "root_shadow",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize)]
#[repr(u8)]
pub enum ObjectApprovalState {
    Satisfied = 0,
    NeedsRevision = 1,
    Disabled = 2,
    PerformanceRisk = 3,
    WaitingMaterial = 4,
}

impl ObjectApprovalState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Satisfied => "满意",
            Self::NeedsRevision => "需要修改",
            Self::Disabled => "禁用",
            Self::PerformanceRisk => "性能风险",
            Self::WaitingMaterial => "等待材质",
        }
    }

    pub fn export_label(self) -> &'static str {
        match self {
            Self::Satisfied => "satisfied",
            Self::NeedsRevision => "needs_revision",
            Self::Disabled => "disabled",
            Self::PerformanceRisk => "performance_risk",
            Self::WaitingMaterial => "waiting_material",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectMaterialBinding {
    pub slot: ObjectMaterialSlot,
    pub material_family: ProceduralMaterialFamily,
    pub material_id: &'static str,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectFamilyDefinition {
    pub kind: ObjectKind,
    pub profile_version: u32,
    pub geometry_version: u32,
    pub semantics: Vec<ObjectSemantic>,
    pub material_slots: Vec<ObjectMaterialBinding>,
}

#[derive(Debug, Resource, Clone, PartialEq)]
pub struct ProceduralObjectRegistry {
    families: Vec<ObjectFamilyDefinition>,
}

impl Default for ProceduralObjectRegistry {
    fn default() -> Self {
        Self {
            families: vec![
                tree_family_definition(),
                simple_family(
                    ObjectKind::Rock,
                    vec![ObjectSemantic::Stone, ObjectSemantic::Waterside],
                ),
                simple_family(
                    ObjectKind::RuinFragment,
                    vec![
                        ObjectSemantic::Stone,
                        ObjectSemantic::Ruin,
                        ObjectSemantic::Omen,
                    ],
                ),
                simple_family(
                    ObjectKind::VillageProp,
                    vec![ObjectSemantic::Village, ObjectSemantic::Interaction],
                ),
                simple_family(
                    ObjectKind::StructurePart,
                    vec![ObjectSemantic::Village, ObjectSemantic::Stone],
                ),
                simple_family(
                    ObjectKind::WatersideProp,
                    vec![ObjectSemantic::Waterside, ObjectSemantic::Ecology],
                ),
                simple_family(
                    ObjectKind::EcologyProp,
                    vec![ObjectSemantic::Ecology, ObjectSemantic::Vegetation],
                ),
                simple_family(
                    ObjectKind::Interactable,
                    vec![ObjectSemantic::Interaction, ObjectSemantic::Omen],
                ),
            ],
        }
    }
}

impl ProceduralObjectRegistry {
    pub fn families(&self) -> &[ObjectFamilyDefinition] {
        &self.families
    }

    pub fn family(&self, kind: ObjectKind) -> Option<&ObjectFamilyDefinition> {
        self.families.iter().find(|family| family.kind == kind)
    }

    pub fn by_semantic(&self, semantic: ObjectSemantic) -> Vec<&ObjectFamilyDefinition> {
        self.families
            .iter()
            .filter(|family| family.semantics.contains(&semantic))
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectGenerationRequest {
    pub kind: ObjectKind,
    pub seed: u64,
    pub lod: ObjectLod,
    pub transform: Transform,
    pub biome: ObjectBiomeContext,
    pub weather: ObjectWeatherState,
    pub material_variant: ObjectMaterialVariant,
    pub collision_mode: ObjectCollisionMode,
    pub mode: ObjectGenerationMode,
}

impl ObjectGenerationRequest {
    pub fn tree(seed: u64, lod: ObjectLod, transform: Transform) -> Self {
        Self {
            kind: ObjectKind::Tree,
            seed,
            lod,
            transform,
            biome: ObjectBiomeContext::Meadow,
            weather: ObjectWeatherState::Clear,
            material_variant: ObjectMaterialVariant::Default,
            collision_mode: ObjectCollisionMode::TrunkAndRoots,
            mode: ObjectGenerationMode::Gallery,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize)]
#[repr(u8)]
enum TreeWindBand {
    Trunk = 0,
    Branch = 1,
    Leaf = 2,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct ProceduralTreeProfile {
    pub seed: u64,
    pub biome: ObjectBiomeContext,
    pub age_years: f32,
    pub health: f32,
    pub height: f32,
    pub trunk_base_radius: f32,
    pub lean: Vec2,
    pub branch_count: usize,
    pub branch_tiers: usize,
    pub leaf_cluster_count: usize,
    pub canopy_radius: f32,
    pub canopy_eccentricity: Vec2,
    pub leaf_density: f32,
    pub dead_branch_ratio: f32,
    pub root_exposure: f32,
    pub moss_ratio: f32,
    pub leaf_color: [f32; 3],
    pub bark_color: [f32; 3],
    pub wind_flex: f32,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GeneratedObjectProfile {
    Tree(ProceduralTreeProfile),
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum ObjectMeshRecipe {
    Cylinder,
    Sphere,
    Cuboid,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeneratedObjectPart {
    pub name: String,
    recipe: ObjectMeshRecipe,
    pub slot: ObjectMaterialSlot,
    pub local_transform: Transform,
    wind_band: Option<TreeWindBand>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TreeTrunkColliderRecipe {
    pub radius: f32,
    pub height: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TreeRootBlockerRecipe {
    pub center: Vec3,
    pub half_extents: Vec3,
    pub yaw: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TreeSensorRecipe {
    pub center: Vec3,
    pub half_extents: Vec3,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectCollisionRecipe {
    pub trunk: Option<TreeTrunkColliderRecipe>,
    pub root_blockers: Vec<TreeRootBlockerRecipe>,
    pub sensor: Option<TreeSensorRecipe>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectAnimationRecipe {
    pub trunk_parts: usize,
    pub branch_parts: usize,
    pub leaf_parts: usize,
    pub uses_gust_response: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GeneratedObjectStats {
    pub part_count: usize,
    pub mesh_count: usize,
    pub vertex_estimate: usize,
    pub collider_count: usize,
    pub generation_ms: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectGeneratedAsset {
    pub kind: ObjectKind,
    pub seed: u64,
    pub stable_id: u64,
    pub lod: ObjectLod,
    pub profile_version: u32,
    pub geometry_version: u32,
    pub request: ObjectGenerationRequest,
    pub profile: GeneratedObjectProfile,
    pub material_slots: Vec<ObjectMaterialBinding>,
    pub parts: Vec<GeneratedObjectPart>,
    pub collision: ObjectCollisionRecipe,
    pub animation: ObjectAnimationRecipe,
    pub stats: GeneratedObjectStats,
}

#[derive(Debug, Component, Clone)]
struct ProceduralObjectInstance {
    kind: ObjectKind,
    seed: u64,
    stable_id: u64,
    lod: ObjectLod,
    profile_version: u32,
    geometry_version: u32,
    request: ObjectGenerationRequest,
    profile: GeneratedObjectProfile,
    material_slots: Vec<ObjectMaterialBinding>,
    collision: ObjectCollisionRecipe,
    stats: GeneratedObjectStats,
    approval: ObjectApprovalState,
}

#[derive(Debug, Component)]
struct ProceduralObjectGalleryRoot;

#[derive(Debug, Component, Clone, Copy)]
struct ProceduralTreeWindPart {
    base_transform: Transform,
    phase: f32,
    amplitude: f32,
    stiffness: f32,
    frequency: f32,
    gust_delay: f32,
    band: TreeWindBand,
    lod: ObjectLod,
}

#[derive(Debug, Resource, Clone, PartialEq)]
struct ObjectGalleryState {
    export_queue: GalleryExportQueue,
}

impl Default for ObjectGalleryState {
    fn default() -> Self {
        Self {
            export_queue: GalleryExportQueue::new(
                "logs/object-gallery-manifest.json",
                "logs/object-gallery.png",
            ),
        }
    }
}

#[derive(Debug, Resource, Clone, Copy, PartialEq)]
struct ObjectWindAnimationState {
    smoothed_direction: Vec2,
    smoothed_energy: f32,
    frame_index: u64,
}

impl Default for ObjectWindAnimationState {
    fn default() -> Self {
        Self {
            smoothed_direction: Vec2::new(0.72, 0.32).normalize(),
            smoothed_energy: 0.74,
            frame_index: 0,
        }
    }
}

#[derive(Debug)]
struct ObjectMeshHandles {
    floor: Handle<Mesh>,
    cylinder: Handle<Mesh>,
    sphere: Handle<Mesh>,
    cuboid: Handle<Mesh>,
}

#[derive(Debug, Clone)]
struct TreeSlotMaterials {
    bark_primary: Handle<StandardMaterial>,
    bark_worn_edge: Handle<StandardMaterial>,
    leaf_primary: Handle<StandardMaterial>,
    leaf_secondary: Handle<StandardMaterial>,
    leaf_dry: Handle<StandardMaterial>,
    root_shadow: Handle<StandardMaterial>,
}

#[derive(Debug, Clone, Copy)]
struct TreeSegment {
    start: Vec3,
    end: Vec3,
    radius: f32,
}

#[derive(SystemParam)]
struct ObjectGallerySpawnParams<'w, 's> {
    commands: Commands<'w, 's>,
    registry: Res<'w, ProceduralObjectRegistry>,
    asset_materials: Res<'w, ProceduralAssetMaterials>,
    gallery_state: ResMut<'w, ObjectGalleryState>,
    codex_state: ResMut<'w, AssetCodexState>,
    meshes: ResMut<'w, Assets<Mesh>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
    performance: ResMut<'w, FramePerformance>,
}

#[derive(Debug, Serialize)]
struct ObjectGalleryExport {
    generated_by: &'static str,
    exported_at_epoch_ms: u128,
    export_mode: &'static str,
    screenshot_path: String,
    lighting_preset: Option<String>,
    camera_position: Option<[f32; 3]>,
    camera_forward: Option<[f32; 3]>,
    samples: Vec<ObjectGalleryExportItem>,
}

#[derive(Debug, Serialize)]
struct ObjectGalleryExportItem {
    kind: &'static str,
    seed: u64,
    stable_id: u64,
    lod: &'static str,
    profile_version: u32,
    geometry_version: u32,
    biome: &'static str,
    weather: &'static str,
    material_variant: &'static str,
    collision_mode: &'static str,
    approval: &'static str,
    part_count: usize,
    mesh_count: usize,
    vertex_estimate: usize,
    collider_count: usize,
    trunk_collider: bool,
    root_blocker_count: usize,
    has_sensor: bool,
    material_slots: Vec<ObjectGalleryExportSlot>,
    profile: ObjectGalleryExportProfile,
}

#[derive(Debug, Serialize)]
struct ObjectGalleryExportSlot {
    slot: &'static str,
    material_family: &'static str,
    material_id: &'static str,
}

#[derive(Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum ObjectGalleryExportProfile {
    Tree {
        age_years: f32,
        health: f32,
        height: f32,
        trunk_base_radius: f32,
        branch_count: usize,
        branch_tiers: usize,
        leaf_cluster_count: usize,
        canopy_radius: f32,
        leaf_density: f32,
        dead_branch_ratio: f32,
        root_exposure: f32,
        moss_ratio: f32,
        wind_flex: f32,
    },
}

const TREE_PROFILE_VERSION: u32 = 2;
const TREE_GEOMETRY_VERSION: u32 = 2;
const TREE_SAMPLE_COUNT_PER_LOD: usize = 6;
const TREE_GALLERY_BASE_SEED: u64 = 0xA11C_EE05_13AA_700D;
const OBJECT_EXPORT_COOLDOWN_SECONDS: f32 = 0.55;

fn tree_family_definition() -> ObjectFamilyDefinition {
    ObjectFamilyDefinition {
        kind: ObjectKind::Tree,
        profile_version: TREE_PROFILE_VERSION,
        geometry_version: TREE_GEOMETRY_VERSION,
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
    }
}

fn simple_family(kind: ObjectKind, semantics: Vec<ObjectSemantic>) -> ObjectFamilyDefinition {
    ObjectFamilyDefinition {
        kind,
        profile_version: 1,
        geometry_version: 1,
        semantics,
        material_slots: Vec::new(),
    }
}

fn spawn_procedural_object_gallery(mut params: ObjectGallerySpawnParams) {
    let started_at = Instant::now();
    params.gallery_state.export_queue = GalleryExportQueue::new(
        "logs/object-gallery-manifest.json",
        "logs/object-gallery.png",
    );
    params.codex_state.reset();
    let floor_material = params.materials.add(StandardMaterial {
        base_color: Color::srgb(0.095, 0.115, 0.1),
        perceptual_roughness: 0.93,
        metallic: 0.0,
        ..Default::default()
    });
    let object_meshes = ObjectMeshHandles {
        floor: params
            .meshes
            .add(Mesh::from(Plane3d::new(Vec3::Y, Vec2::new(56.0, 24.0)))),
        cylinder: params.meshes.add(Mesh::from(Cylinder::new(1.0, 1.0))),
        sphere: params.meshes.add(Sphere::new(1.0).mesh().uv(20, 14)),
        cuboid: params.meshes.add(Mesh::from(Cuboid::new(1.0, 1.0, 1.0))),
    };
    let slot_materials = tree_slot_materials(&params.asset_materials);

    params
        .commands
        .spawn((
            Name::new("ProceduralObjectGallery"),
            DespawnOnExit(AppScreen::InGame),
            ProceduralObjectGalleryRoot,
            Transform::default(),
            Visibility::Visible,
        ))
        .with_children(|parent| {
            parent.spawn((
                Name::new("ProceduralObjectGalleryFloor"),
                Mesh3d(object_meshes.floor.clone()),
                MeshMaterial3d(floor_material),
                Transform::from_xyz(16.0, -0.04, -15.0),
            ));
            for request in tree_gallery_requests() {
                if let Some(family) = params.registry.family(request.kind) {
                    let asset = generate_object_asset(request, family);
                    spawn_generated_object(
                        parent,
                        &object_meshes,
                        &slot_materials,
                        asset,
                        ObjectGenerationMode::Gallery,
                    );
                }
            }
        });

    tracing::info!(
        target: "dao_game::objects::gallery",
        samples = TREE_SAMPLE_COUNT_PER_LOD * 3,
        profile_version = TREE_PROFILE_VERSION,
        geometry_version = TREE_GEOMETRY_VERSION,
        "procedural object gallery spawned with near/mid/far tree samples"
    );
    params
        .performance
        .record_phase_duration(PerformancePhase::Assets, started_at.elapsed());
}

fn tree_gallery_requests() -> Vec<ObjectGenerationRequest> {
    let mut requests = Vec::with_capacity(TREE_SAMPLE_COUNT_PER_LOD * 3);
    let biomes = [
        ObjectBiomeContext::Meadow,
        ObjectBiomeContext::Wetland,
        ObjectBiomeContext::Ridge,
        ObjectBiomeContext::VillageCourtyard,
        ObjectBiomeContext::RuinEdge,
        ObjectBiomeContext::DesertWind,
    ];
    let weather = [
        ObjectWeatherState::Clear,
        ObjectWeatherState::RainSoaked,
        ObjectWeatherState::DryWind,
        ObjectWeatherState::DreamTint,
    ];
    let material_variant = [
        ObjectMaterialVariant::Default,
        ObjectMaterialVariant::Wet,
        ObjectMaterialVariant::Dusty,
        ObjectMaterialVariant::Mossy,
        ObjectMaterialVariant::Dream,
    ];
    let lod_rows = [ObjectLod::Near, ObjectLod::Mid, ObjectLod::Far];

    for (lod_row, lod) in lod_rows.into_iter().enumerate() {
        for column in 0..TREE_SAMPLE_COUNT_PER_LOD {
            let seed = TREE_GALLERY_BASE_SEED
                .wrapping_add((lod_row as u64 + 1) * 0x9E37_79B9)
                .wrapping_add(column as u64 * 0xBF58_476D);
            let mut request = ObjectGenerationRequest::tree(
                seed,
                lod,
                Transform::from_xyz(column as f32 * 8.2 - 4.6, 0.0, -6.0 - lod_row as f32 * 8.0),
            );
            request.biome = biomes[(column + lod_row) % biomes.len()];
            request.weather = weather[(column * 2 + lod_row) % weather.len()];
            request.material_variant =
                material_variant[(column + lod_row * 2) % material_variant.len()];
            request.collision_mode = match lod {
                ObjectLod::Near => ObjectCollisionMode::Full,
                ObjectLod::Mid => ObjectCollisionMode::TrunkOnly,
                ObjectLod::Far => ObjectCollisionMode::VisualOnly,
            };
            requests.push(request);
        }
    }
    requests
}

fn generate_object_asset(
    request: ObjectGenerationRequest,
    family: &ObjectFamilyDefinition,
) -> ObjectGeneratedAsset {
    match request.kind {
        ObjectKind::Tree => generate_tree_asset(request, family),
        _ => {
            let stable_id = stable_object_id(
                request.kind,
                request.seed,
                family.profile_version,
                family.geometry_version,
            );
            ObjectGeneratedAsset {
                kind: request.kind,
                seed: request.seed,
                stable_id,
                lod: request.lod,
                profile_version: family.profile_version,
                geometry_version: family.geometry_version,
                request,
                profile: GeneratedObjectProfile::Tree(procedural_tree_profile(
                    &ObjectGenerationRequest::tree(0, ObjectLod::Far, Transform::default()),
                )),
                material_slots: family.material_slots.clone(),
                parts: Vec::new(),
                collision: ObjectCollisionRecipe {
                    trunk: None,
                    root_blockers: Vec::new(),
                    sensor: None,
                },
                animation: ObjectAnimationRecipe {
                    trunk_parts: 0,
                    branch_parts: 0,
                    leaf_parts: 0,
                    uses_gust_response: false,
                },
                stats: GeneratedObjectStats {
                    part_count: 0,
                    mesh_count: 0,
                    vertex_estimate: 0,
                    collider_count: 0,
                    generation_ms: 0.0,
                },
            }
        }
    }
}

fn generate_tree_asset(
    request: ObjectGenerationRequest,
    family: &ObjectFamilyDefinition,
) -> ObjectGeneratedAsset {
    let started_at = Instant::now();
    let profile = procedural_tree_profile(&request);
    let mut parts = generate_tree_parts(&request, profile);
    let collision = tree_collision_recipe(&request, profile);

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

fn procedural_tree_profile(request: &ObjectGenerationRequest) -> ProceduralTreeProfile {
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

fn generate_tree_parts(
    request: &ObjectGenerationRequest,
    profile: ProceduralTreeProfile,
) -> Vec<GeneratedObjectPart> {
    let mut parts = Vec::new();
    if request.lod == ObjectLod::Far {
        generate_far_tree_parts(&mut parts, profile);
        return parts;
    }

    let trunk_points = trunk_points(profile, request.lod);
    append_trunk_parts(&mut parts, profile, &trunk_points);
    let branch_tips = append_branch_parts(&mut parts, profile, request.lod, &trunk_points);
    append_leaf_parts(&mut parts, profile, request.lod, &branch_tips);
    append_root_parts(&mut parts, profile, request.lod);
    parts
}

fn generate_far_tree_parts(parts: &mut Vec<GeneratedObjectPart>, profile: ProceduralTreeProfile) {
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

fn tree_collision_recipe(
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

fn spawn_generated_object(
    parent: &mut ChildSpawnerCommands<'_>,
    meshes: &ObjectMeshHandles,
    slot_materials: &TreeSlotMaterials,
    asset: ObjectGeneratedAsset,
    mode: ObjectGenerationMode,
) {
    let approval = approval_for_object(&asset);
    let name = format!(
        "ProceduralObject::{}::{:016x}::{}",
        asset.kind.export_label(),
        asset.seed,
        asset.lod.export_label()
    );
    parent
        .spawn((
            Name::new(name),
            asset.request.transform,
            Visibility::Visible,
            ProceduralObjectInstance {
                kind: asset.kind,
                seed: asset.seed,
                stable_id: asset.stable_id,
                lod: asset.lod,
                profile_version: asset.profile_version,
                geometry_version: asset.geometry_version,
                request: asset.request.clone(),
                profile: asset.profile,
                material_slots: asset.material_slots.clone(),
                collision: asset.collision.clone(),
                stats: asset.stats.clone(),
                approval,
            },
        ))
        .with_children(|tree| {
            for (part_index, part) in asset.parts.iter().enumerate() {
                let mesh = mesh_for_recipe(meshes, part.recipe);
                let material = material_for_slot(slot_materials, part.slot);
                let mut entity = tree.spawn((
                    Name::new(format!("{}#{part_index}", part.name)),
                    Mesh3d(mesh),
                    MeshMaterial3d(material),
                    part.local_transform,
                ));
                if let Some(profile) = tree_profile_from_generated(&asset.profile)
                    && let Some(wind_band) = part.wind_band
                {
                    entity.insert(wind_part_for_tree(
                        profile,
                        part.local_transform,
                        wind_band,
                        asset.lod,
                        part_index,
                    ));
                }
            }
            if mode == ObjectGenerationMode::Gallery {
                spawn_tree_colliders(tree, &asset.collision);
            }
        });

    tracing::info!(
        target: "dao_game::objects::tree",
        stable_id = asset.stable_id,
        seed = asset.seed,
        lod = asset.lod.export_label(),
        part_count = asset.stats.part_count,
        vertex_estimate = asset.stats.vertex_estimate,
        collider_count = asset.stats.collider_count,
        approval = approval.export_label(),
        "procedural object generated"
    );
}

fn spawn_tree_colliders(parent: &mut ChildSpawnerCommands<'_>, collision: &ObjectCollisionRecipe) {
    if let Some(trunk) = &collision.trunk {
        parent.spawn((
            Name::new("TreeTrunkCollider"),
            Transform::from_xyz(0.0, trunk.height * 0.5, 0.0),
            RigidBody::Static,
            Collider::capsule(trunk.radius, (trunk.height - trunk.radius * 2.0).max(0.12)),
            gallery_layers(),
            CollisionEventsEnabled,
            DaoCollider {
                layer: DaoPhysicsLayer::Gallery,
                role: DaoColliderRole::StaticBlocker,
                source: DaoColliderSource::ObjectGallery,
            },
        ));
    }
    for (index, blocker) in collision.root_blockers.iter().enumerate() {
        parent.spawn((
            Name::new(format!("TreeRootBlocker{index}")),
            Transform::from_translation(blocker.center)
                .with_rotation(Quat::from_rotation_y(blocker.yaw)),
            RigidBody::Static,
            Collider::cuboid(
                blocker.half_extents.x * 2.0,
                blocker.half_extents.y * 2.0,
                blocker.half_extents.z * 2.0,
            ),
            gallery_layers(),
            CollisionEventsEnabled,
            DaoCollider {
                layer: DaoPhysicsLayer::Gallery,
                role: DaoColliderRole::StaticBlocker,
                source: DaoColliderSource::ObjectGallery,
            },
        ));
    }
    if let Some(sensor) = &collision.sensor {
        parent.spawn((
            Name::new("TreeObservationSensor"),
            Transform::from_translation(sensor.center),
            RigidBody::Static,
            Collider::cuboid(
                sensor.half_extents.x * 2.0,
                sensor.half_extents.y * 2.0,
                sensor.half_extents.z * 2.0,
            ),
            Sensor,
            gallery_layers(),
            CollisionEventsEnabled,
            DaoPhysicsSensor {
                kind: DaoSensorKind::GalleryExhibit,
            },
            DaoCollider {
                layer: DaoPhysicsLayer::Gallery,
                role: DaoColliderRole::InteractionSensor,
                source: DaoColliderSource::ObjectGallery,
            },
        ));
    }
}

fn tree_profile_from_generated(profile: &GeneratedObjectProfile) -> Option<ProceduralTreeProfile> {
    match profile {
        GeneratedObjectProfile::Tree(profile) => Some(*profile),
    }
}

fn mesh_for_recipe(meshes: &ObjectMeshHandles, recipe: ObjectMeshRecipe) -> Handle<Mesh> {
    match recipe {
        ObjectMeshRecipe::Cylinder => meshes.cylinder.clone(),
        ObjectMeshRecipe::Sphere => meshes.sphere.clone(),
        ObjectMeshRecipe::Cuboid => meshes.cuboid.clone(),
    }
}

fn material_for_slot(
    materials: &TreeSlotMaterials,
    slot: ObjectMaterialSlot,
) -> Handle<StandardMaterial> {
    match slot {
        ObjectMaterialSlot::BarkPrimary => materials.bark_primary.clone(),
        ObjectMaterialSlot::BarkWornEdge => materials.bark_worn_edge.clone(),
        ObjectMaterialSlot::LeafPrimary => materials.leaf_primary.clone(),
        ObjectMaterialSlot::LeafSecondary => materials.leaf_secondary.clone(),
        ObjectMaterialSlot::LeafDry => materials.leaf_dry.clone(),
        ObjectMaterialSlot::RootShadow => materials.root_shadow.clone(),
    }
}

fn tree_slot_materials(materials: &ProceduralAssetMaterials) -> TreeSlotMaterials {
    TreeSlotMaterials {
        bark_primary: materials.handle_for_family(ProceduralMaterialFamily::Wood),
        bark_worn_edge: materials.handle_for_family(ProceduralMaterialFamily::OldStone),
        leaf_primary: materials.handle_for_family(ProceduralMaterialFamily::GroveLeaf),
        leaf_secondary: materials.handle_for_family(ProceduralMaterialFamily::GroveLeaf),
        leaf_dry: materials.handle_for_family(ProceduralMaterialFamily::Sand),
        root_shadow: materials.handle_for_family(ProceduralMaterialFamily::Shadow),
    }
}

fn wind_part_for_tree(
    profile: ProceduralTreeProfile,
    base_transform: Transform,
    band: TreeWindBand,
    lod: ObjectLod,
    index: usize,
) -> ProceduralTreeWindPart {
    let (amplitude_scale, stiffness, frequency) = match band {
        TreeWindBand::Trunk => (0.26, 1.0, 0.74),
        TreeWindBand::Branch => (0.62, 0.66, 1.14),
        TreeWindBand::Leaf => (1.0, 0.38, 2.2),
    };
    let lod_scale = match lod {
        ObjectLod::Near => 1.0,
        ObjectLod::Mid => 0.72,
        ObjectLod::Far => 0.46,
    };
    ProceduralTreeWindPart {
        base_transform,
        phase: seeded_unit(profile.seed, 801 + index as u64) * std::f32::consts::TAU * 2.0,
        amplitude: 0.044 * profile.wind_flex * amplitude_scale * lod_scale,
        stiffness,
        frequency,
        gust_delay: seeded_unit(profile.seed, 809 + index as u64) * std::f32::consts::TAU,
        band,
        lod,
    }
}

fn animate_procedural_tree_wind(
    time: Res<Time>,
    wind: Option<Res<WindField>>,
    mut animation: ResMut<ObjectWindAnimationState>,
    mut performance: ResMut<FramePerformance>,
    mut query: Query<(&ProceduralTreeWindPart, &mut Transform)>,
) {
    let started_at = Instant::now();
    animation.frame_index = animation.frame_index.wrapping_add(1);
    let elapsed = time.elapsed_secs();
    let target_direction = wind
        .as_deref()
        .map(|wind| wind.direction.normalize_or_zero())
        .filter(|direction| direction.length_squared() > f32::EPSILON)
        .unwrap_or(Vec2::new(0.72, 0.32).normalize());
    let target_energy = wind
        .as_deref()
        .map(|wind| 0.62 + wind.speed * 0.72 + wind.gust * 0.58 + wind.swirl.abs() * 0.2)
        .unwrap_or(0.74)
        .clamp(0.24, 1.95);
    let smooth = (time.delta_secs() * 4.8).clamp(0.03, 0.3);
    animation.smoothed_direction = animation
        .smoothed_direction
        .lerp(target_direction, smooth)
        .normalize_or_zero();
    animation.smoothed_energy = lerp(animation.smoothed_energy, target_energy, smooth);
    let gust = wind
        .as_deref()
        .map(|wind| wind.gust)
        .unwrap_or(0.2)
        .clamp(0.0, 2.4);

    for (part, mut transform) in &mut query {
        if !should_update_wind_part(animation.frame_index, part.lod, part.band) {
            continue;
        }
        let base_wave =
            (elapsed * (part.frequency + animation.smoothed_energy * 0.6) + part.phase).sin();
        let gust_wave = (elapsed * 1.3 + part.gust_delay).sin().max(0.0) * gust;
        let sway = (base_wave * part.amplitude + gust_wave * part.amplitude * 0.62)
            * animation.smoothed_energy;
        let flutter = if part.band == TreeWindBand::Leaf {
            (elapsed * 2.4 + part.phase * 1.6).sin() * part.amplitude * 0.34
        } else {
            0.0
        };
        let bend = Quat::from_rotation_x(animation.smoothed_direction.y * sway * part.stiffness)
            * Quat::from_rotation_z(-animation.smoothed_direction.x * sway * part.stiffness);
        *transform = part.base_transform;
        transform.rotation = bend * part.base_transform.rotation;
        transform.translation += Vec3::new(
            animation.smoothed_direction.x,
            flutter.abs() * 0.11,
            animation.smoothed_direction.y,
        ) * sway.abs()
            * 0.3;
    }
    performance.record_phase_duration(PerformancePhase::Assets, started_at.elapsed());
}

fn should_update_wind_part(frame_index: u64, lod: ObjectLod, band: TreeWindBand) -> bool {
    let stride = match (lod, band) {
        (ObjectLod::Near, _) => 1,
        (ObjectLod::Mid, TreeWindBand::Leaf) => 2,
        (ObjectLod::Mid, _) => 3,
        (ObjectLod::Far, TreeWindBand::Leaf) => 4,
        (ObjectLod::Far, _) => 6,
    };
    frame_index.is_multiple_of(stride)
}

fn advance_object_gallery_export_frame(mut state: ResMut<ObjectGalleryState>) {
    state.export_queue.advance_frame();
}

fn handle_object_gallery_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<ObjectGalleryState>,
) {
    if !keys.just_pressed(KeyCode::KeyO) {
        return;
    }
    let with_screenshot = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
    let mode = if with_screenshot {
        GalleryExportMode::ManifestAndScreenshot
    } else {
        GalleryExportMode::ManifestOnly
    };
    match state
        .export_queue
        .queue_export(mode, OBJECT_EXPORT_COOLDOWN_SECONDS)
    {
        Ok(()) => {
            tracing::info!(
                target: "dao_game::objects::export",
                mode = mode.export_label(),
                "object gallery export queued"
            );
        }
        Err(cooldown_remaining_ms) => {
            tracing::warn!(
                target: "dao_game::objects::export",
                cooldown_remaining_ms,
                "object gallery export ignored during cooldown"
            );
        }
    }
}

fn process_object_gallery_export_queue(
    mut commands: Commands,
    mut state: ResMut<ObjectGalleryState>,
    material_gallery_state: Option<Res<MaterialGalleryState>>,
    camera_query: Query<&Transform, With<WorldCamera>>,
    objects: Query<&ProceduralObjectInstance>,
    mut performance: ResMut<FramePerformance>,
) {
    let started_at = Instant::now();
    match state.export_queue.pending_stage.clone() {
        GalleryExportStage::Idle => {}
        GalleryExportStage::ManifestQueued { mode, queued_frame } => {
            if queued_frame == state.export_queue.frame_index {
                performance.record_phase_duration(PerformancePhase::Assets, started_at.elapsed());
                return;
            }
            match export_object_gallery_manifest(
                &state.export_queue.export_path,
                &state.export_queue.screenshot_path,
                mode,
                material_gallery_state.as_deref(),
                camera_query.iter().next(),
                &objects,
            ) {
                Ok(count) => {
                    tracing::info!(
                        target: "dao_game::objects::export",
                        path = %state.export_queue.export_path.display(),
                        mode = mode.export_label(),
                        sample_count = count,
                        "object gallery manifest exported"
                    );
                    state.export_queue.mark_manifest_exported(mode);
                }
                Err(error) => {
                    tracing::error!(
                        target: "dao_game::objects::export",
                        path = %state.export_queue.export_path.display(),
                        error = %error,
                        "object gallery manifest export failed"
                    );
                    state.export_queue.reset();
                }
            }
        }
        GalleryExportStage::ScreenshotQueued { queued_frame } => {
            if queued_frame == state.export_queue.frame_index {
                performance.record_phase_duration(PerformancePhase::Assets, started_at.elapsed());
                return;
            }
            if let Err(error) = prepare_export_path(&state.export_queue.screenshot_path) {
                tracing::error!(
                    target: "dao_game::objects::export",
                    path = %state.export_queue.screenshot_path.display(),
                    error = %error,
                    "object gallery screenshot path preparation failed"
                );
            } else {
                commands
                    .spawn(Screenshot::primary_window())
                    .observe(save_to_disk(state.export_queue.screenshot_path.clone()));
                tracing::info!(
                    target: "dao_game::objects::export",
                    path = %state.export_queue.screenshot_path.display(),
                    "object gallery screenshot requested"
                );
            }
            state.export_queue.reset();
        }
    }
    performance.record_phase_duration(PerformancePhase::Assets, started_at.elapsed());
}

fn export_object_gallery_manifest(
    manifest_path: &Path,
    screenshot_path: &Path,
    mode: GalleryExportMode,
    material_gallery_state: Option<&MaterialGalleryState>,
    camera: Option<&Transform>,
    objects: &Query<&ProceduralObjectInstance>,
) -> Result<usize, String> {
    prepare_export_path(manifest_path)?;
    let mut samples: Vec<_> = objects
        .iter()
        .map(|instance| ObjectGalleryExportItem {
            kind: instance.kind.export_label(),
            seed: instance.seed,
            stable_id: instance.stable_id,
            lod: instance.lod.export_label(),
            profile_version: instance.profile_version,
            geometry_version: instance.geometry_version,
            biome: instance.request.biome.export_label(),
            weather: instance.request.weather.export_label(),
            material_variant: instance.request.material_variant.export_label(),
            collision_mode: instance.request.collision_mode.export_label(),
            approval: instance.approval.export_label(),
            part_count: instance.stats.part_count,
            mesh_count: instance.stats.mesh_count,
            vertex_estimate: instance.stats.vertex_estimate,
            collider_count: instance.stats.collider_count,
            trunk_collider: instance.collision.trunk.is_some(),
            root_blocker_count: instance.collision.root_blockers.len(),
            has_sensor: instance.collision.sensor.is_some(),
            material_slots: instance
                .material_slots
                .iter()
                .map(|slot| ObjectGalleryExportSlot {
                    slot: slot.slot.export_label(),
                    material_family: material_family_label(slot.material_family),
                    material_id: slot.material_id,
                })
                .collect(),
            profile: export_profile(&instance.profile),
        })
        .collect();
    samples.sort_by_key(|item| item.stable_id);
    let (camera_position, camera_forward) = camera
        .map(|camera| {
            (
                [
                    camera.translation.x,
                    camera.translation.y,
                    camera.translation.z,
                ],
                {
                    let forward = camera.forward();
                    [forward.x, forward.y, forward.z]
                },
            )
        })
        .unzip();
    let export = ObjectGalleryExport {
        generated_by: "dao_game::objects::ObjectGallery",
        exported_at_epoch_ms: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
        export_mode: mode.export_label(),
        screenshot_path: screenshot_path.display().to_string(),
        lighting_preset: material_gallery_state.map(|state| state.lighting.label().to_string()),
        camera_position,
        camera_forward,
        samples,
    };
    let raw = serde_json::to_string_pretty(&export)
        .map_err(|error| format!("failed to serialize object manifest: {error}"))?;
    fs::write(manifest_path, raw)
        .map_err(|error| format!("failed to write {}: {error}", manifest_path.display()))?;
    Ok(export.samples.len())
}

fn refresh_object_gallery_codex(
    registry: Res<ProceduralObjectRegistry>,
    gallery_state: Res<ObjectGalleryState>,
    material_gallery_state: Option<Res<MaterialGalleryState>>,
    objects: Query<&ProceduralObjectInstance>,
    mut codex_state: ResMut<AssetCodexState>,
) {
    let mut instances: Vec<_> = objects.iter().collect();
    if instances.is_empty() {
        codex_state.reset();
        return;
    }

    instances.sort_by_key(|instance| (instance.kind as u8, instance.stable_id));
    let sample = instances[0];
    let near_count = instances
        .iter()
        .filter(|instance| instance.lod == ObjectLod::Near)
        .count();
    let mid_count = instances
        .iter()
        .filter(|instance| instance.lod == ObjectLod::Mid)
        .count();
    let far_count = instances
        .iter()
        .filter(|instance| instance.lod == ObjectLod::Far)
        .count();
    let lighting_label = material_gallery_state
        .as_deref()
        .map(|state| state.lighting.label())
        .unwrap_or("未连接");
    let selected_category = material_gallery_state
        .as_deref()
        .and_then(|state| state.selected_category)
        .map(|category| category.label())
        .unwrap_or("全部");
    let (profile_version, geometry_version, slot_count) = registry
        .family(sample.kind)
        .map(|family| {
            (
                family.profile_version,
                family.geometry_version,
                family.material_slots.len(),
            )
        })
        .unwrap_or((
            sample.profile_version,
            sample.geometry_version,
            sample.material_slots.len(),
        ));

    let mut summary_lines = vec![
        format!(
            "家族：{}  样本：{}  近/中/远：{}/{}/{}",
            sample.kind.label(),
            instances.len(),
            near_count,
            mid_count,
            far_count
        ),
        format!(
            "焦点 Seed：{}  StableId：{}  审核：{}",
            sample.seed,
            sample.stable_id,
            sample.approval.label()
        ),
        format!(
            "LOD：{}  碰撞：{}  部件：{}  网格：{}  顶点≈{}",
            sample.lod.label(),
            sample.request.collision_mode.label(),
            sample.stats.part_count,
            sample.stats.mesh_count,
            sample.stats.vertex_estimate
        ),
        format!(
            "版本：profile v{}  geometry v{}  材质槽：{}",
            profile_version, geometry_version, slot_count
        ),
        format!(
            "环境：{} / {} / {}  光照：{}  材质筛选：{}",
            sample.request.biome.label(),
            sample.request.weather.label(),
            sample.request.material_variant.label(),
            lighting_label,
            selected_category
        ),
    ];

    match sample.profile {
        GeneratedObjectProfile::Tree(profile) => summary_lines.push(format!(
            "树参数：年龄 {:.0}y  高度 {:.1}m  健康 {:.0}%  叶密 {:.0}%  根暴露 {:.0}%  风柔性 {:.0}%",
            profile.age_years,
            profile.height,
            profile.health * 100.0,
            profile.leaf_density * 100.0,
            profile.root_exposure * 100.0,
            profile.wind_flex * 100.0
        )),
    }

    codex_state.visible = true;
    codex_state.title = "AssetCodex".to_string();
    codex_state.subtitle = "图鉴展列区骨架".to_string();
    codex_state.summary_lines = summary_lines;
    codex_state.slots = sample
        .material_slots
        .iter()
        .map(|slot| AssetCodexSlot {
            slot: slot.slot.export_label().to_string(),
            material_family: material_family_label(slot.material_family).to_string(),
            material_id: slot.material_id.to_string(),
        })
        .collect();
    codex_state.controls_hint =
        "O 导出对象清单  Shift+O 导出对象清单+截图  E 导出材质清单".to_string();
    codex_state.export_manifest_path = gallery_state.export_queue.export_path.display().to_string();
    codex_state.screenshot_path = gallery_state
        .export_queue
        .screenshot_path
        .display()
        .to_string();
}

fn export_profile(profile: &GeneratedObjectProfile) -> ObjectGalleryExportProfile {
    match profile {
        GeneratedObjectProfile::Tree(profile) => ObjectGalleryExportProfile::Tree {
            age_years: profile.age_years,
            health: profile.health,
            height: profile.height,
            trunk_base_radius: profile.trunk_base_radius,
            branch_count: profile.branch_count,
            branch_tiers: profile.branch_tiers,
            leaf_cluster_count: profile.leaf_cluster_count,
            canopy_radius: profile.canopy_radius,
            leaf_density: profile.leaf_density,
            dead_branch_ratio: profile.dead_branch_ratio,
            root_exposure: profile.root_exposure,
            moss_ratio: profile.moss_ratio,
            wind_flex: profile.wind_flex,
        },
    }
}

fn material_family_label(family: ProceduralMaterialFamily) -> &'static str {
    match family {
        ProceduralMaterialFamily::MudWall => "mud_wall",
        ProceduralMaterialFamily::DarkRoof => "dark_roof",
        ProceduralMaterialFamily::Wood => "wood",
        ProceduralMaterialFamily::GroveLeaf => "grove_leaf",
        ProceduralMaterialFamily::Stone => "stone",
        ProceduralMaterialFamily::Cloth => "cloth",
        ProceduralMaterialFamily::Water => "water",
        ProceduralMaterialFamily::Sand => "sand",
        ProceduralMaterialFamily::Wool => "wool",
        ProceduralMaterialFamily::NpcCloth => "npc_cloth",
        ProceduralMaterialFamily::BirdFeather => "bird_feather",
        ProceduralMaterialFamily::FishScale => "fish_scale",
        ProceduralMaterialFamily::OldStone => "old_stone",
        ProceduralMaterialFamily::Relic => "relic",
        ProceduralMaterialFamily::DesertStone => "desert_stone",
        ProceduralMaterialFamily::WarmLight => "warm_light",
        ProceduralMaterialFamily::Shadow => "shadow",
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

fn transform_for_segment(segment: TreeSegment) -> Transform {
    let axis = segment.end - segment.start;
    let length = axis.length().max(0.01);
    let midpoint = (segment.start + segment.end) * 0.5;
    Transform::from_translation(midpoint)
        .with_rotation(Quat::from_rotation_arc(Vec3::Y, axis / length))
        .with_scale(Vec3::new(
            segment.radius.max(0.01),
            length,
            segment.radius.max(0.01),
        ))
}

fn approval_for_object(asset: &ObjectGeneratedAsset) -> ObjectApprovalState {
    if matches!(asset.request.material_variant, ObjectMaterialVariant::Dream)
        && asset.material_slots.is_empty()
    {
        return ObjectApprovalState::WaitingMaterial;
    }
    if asset.stats.vertex_estimate > 11_500 && asset.lod == ObjectLod::Near {
        return ObjectApprovalState::PerformanceRisk;
    }
    let GeneratedObjectProfile::Tree(profile) = asset.profile;
    if profile.health < 0.22 || profile.dead_branch_ratio > 0.74 {
        return ObjectApprovalState::NeedsRevision;
    }
    ObjectApprovalState::Satisfied
}

pub fn stable_object_id(
    kind: ObjectKind,
    seed: u64,
    profile_version: u32,
    geometry_version: u32,
) -> u64 {
    let salt = ((profile_version as u64) << 32) ^ (geometry_version as u64);
    stable_hash64(
        seed.wrapping_add(salt),
        (kind as u64 + 1).wrapping_mul(0x9E37_79B9_7F4A_7C15),
    )
}

fn stable_hash64(seed: u64, salt: u64) -> u64 {
    let mut value = seed
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(salt.wrapping_mul(0xBF58_476D_1CE4_E5B9));
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn seeded_unit(seed: u64, salt: u64) -> f32 {
    let hash = stable_hash64(seed, salt);
    (hash as f64 / u64::MAX as f64) as f32
}

fn seeded_signed(seed: u64, salt: u64) -> f32 {
    seeded_unit(seed, salt) * 2.0 - 1.0
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t.clamp(0.0, 1.0)
}

trait TreeSegmentMetrics {
    fn length(self) -> f32;
}

impl TreeSegmentMetrics for TreeSegment {
    fn length(self) -> f32 {
        (self.end - self.start).length()
    }
}

#[cfg(test)]
mod tests {
    use super::{
        GeneratedObjectProfile, ObjectBiomeContext, ObjectCollisionMode, ObjectGalleryExport,
        ObjectGenerationRequest, ObjectKind, ObjectLod, ObjectSemantic, ObjectWeatherState,
        ProceduralObjectRegistry, TREE_GALLERY_BASE_SEED, TREE_GEOMETRY_VERSION,
        TREE_PROFILE_VERSION, generate_tree_asset, procedural_tree_profile, stable_object_id,
        tree_family_definition,
    };
    use crate::game::gallery::GalleryExportMode;
    use bevy::prelude::Transform;

    #[test]
    fn tree_profile_is_deterministic_for_same_request() {
        let request = ObjectGenerationRequest {
            kind: ObjectKind::Tree,
            seed: TREE_GALLERY_BASE_SEED,
            lod: ObjectLod::Near,
            transform: Transform::default(),
            biome: ObjectBiomeContext::Meadow,
            weather: ObjectWeatherState::Clear,
            material_variant: super::ObjectMaterialVariant::Default,
            collision_mode: ObjectCollisionMode::Full,
            mode: super::ObjectGenerationMode::Gallery,
        };
        let first = procedural_tree_profile(&request);
        let second = procedural_tree_profile(&request);
        assert_eq!(first, second);
    }

    #[test]
    fn tree_profile_changes_with_seed_and_context() {
        let mut request = ObjectGenerationRequest::tree(
            TREE_GALLERY_BASE_SEED,
            ObjectLod::Near,
            Transform::default(),
        );
        let first = procedural_tree_profile(&request);
        request.seed = TREE_GALLERY_BASE_SEED + 97;
        request.biome = ObjectBiomeContext::Wetland;
        request.weather = ObjectWeatherState::DryWind;
        let second = procedural_tree_profile(&request);

        assert_ne!(first.height, second.height);
        assert_ne!(first.leaf_color, second.leaf_color);
        assert_ne!(first.branch_count, second.branch_count);
    }

    #[test]
    fn tree_profile_bounds_stay_reviewable() {
        for index in 0..48_u64 {
            let mut request = ObjectGenerationRequest::tree(
                TREE_GALLERY_BASE_SEED + index,
                ObjectLod::Near,
                Transform::default(),
            );
            request.biome = match index % 6 {
                0 => ObjectBiomeContext::Meadow,
                1 => ObjectBiomeContext::Wetland,
                2 => ObjectBiomeContext::Ridge,
                3 => ObjectBiomeContext::VillageCourtyard,
                4 => ObjectBiomeContext::RuinEdge,
                _ => ObjectBiomeContext::DesertWind,
            };
            let profile = procedural_tree_profile(&request);

            assert!((4.0..=12.0).contains(&profile.height));
            assert!((0.18..=0.9).contains(&profile.trunk_base_radius));
            assert!((0.15..=1.5).contains(&profile.leaf_density));
            assert!((0.15..=0.98).contains(&profile.health));
            assert!((0.0..=0.72).contains(&profile.moss_ratio));
            assert!(profile.branch_tiers >= 2);
        }
    }

    #[test]
    fn stable_object_id_changes_with_kind_seed_and_versions() {
        let a = stable_object_id(
            ObjectKind::Tree,
            42,
            TREE_PROFILE_VERSION,
            TREE_GEOMETRY_VERSION,
        );
        let b = stable_object_id(
            ObjectKind::Tree,
            42,
            TREE_PROFILE_VERSION,
            TREE_GEOMETRY_VERSION,
        );
        let c = stable_object_id(
            ObjectKind::Tree,
            43,
            TREE_PROFILE_VERSION,
            TREE_GEOMETRY_VERSION,
        );
        let d = stable_object_id(
            ObjectKind::Rock,
            42,
            TREE_PROFILE_VERSION,
            TREE_GEOMETRY_VERSION,
        );
        let e = stable_object_id(
            ObjectKind::Tree,
            42,
            TREE_PROFILE_VERSION + 1,
            TREE_GEOMETRY_VERSION,
        );

        assert_eq!(a, b);
        assert_ne!(a, c);
        assert_ne!(a, d);
        assert_ne!(a, e);
    }

    #[test]
    fn registry_exposes_tree_family_and_semantic_queries() {
        let registry = ProceduralObjectRegistry::default();
        assert!(registry.family(ObjectKind::Tree).is_some());
        assert!(
            registry
                .by_semantic(ObjectSemantic::Vegetation)
                .iter()
                .any(|family| family.kind == ObjectKind::Tree)
        );
        assert!(
            registry
                .by_semantic(ObjectSemantic::Ruin)
                .iter()
                .any(|family| family.kind == ObjectKind::RuinFragment)
        );
    }

    #[test]
    fn near_tree_generation_contains_more_detail_than_far() {
        let family = tree_family_definition();
        let near_request =
            ObjectGenerationRequest::tree(0xABC, ObjectLod::Near, Transform::default());
        let far_request =
            ObjectGenerationRequest::tree(0xABC, ObjectLod::Far, Transform::default());

        let near = generate_tree_asset(near_request, &family);
        let far = generate_tree_asset(far_request, &family);

        assert!(near.stats.part_count > far.stats.part_count);
        assert!(near.stats.vertex_estimate > far.stats.vertex_estimate);
        assert!(near.stats.collider_count >= far.stats.collider_count);
        assert!(matches!(near.profile, GeneratedObjectProfile::Tree(_)));
    }

    #[test]
    fn object_manifest_payload_serializes_with_utf8_and_versions() {
        let manifest = ObjectGalleryExport {
            generated_by: "dao_game::objects::ObjectGallery",
            exported_at_epoch_ms: 1_234,
            export_mode: GalleryExportMode::ManifestOnly.export_label(),
            screenshot_path: "logs/object-gallery.png".to_string(),
            lighting_preset: Some("固定光照".to_string()),
            camera_position: Some([1.0, 2.0, 3.0]),
            camera_forward: Some([0.0, 0.0, -1.0]),
            samples: Vec::new(),
        };
        let json = serde_json::to_string(&manifest).expect("manifest should serialize");

        assert!(json.contains("manifest_only"));
        assert!(json.contains("固定光照"));
    }
}
