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

mod families;

use crate::{
    core::performance::{FramePerformance, PerformancePhase},
    game::{
        assets::{ProceduralAssetMaterials, ProceduralMaterialFamily},
        environment::WindField,
        flow::{AppScreen, InGameState, SessionMode, in_session_mode},
        gallery::{
            AssetCodexSection, AssetCodexSlot, AssetCodexState, GalleryExportMode,
            GalleryExportQueue, GalleryExportStage, prepare_export_path,
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

impl ObjectSemantic {
    pub fn label(self) -> &'static str {
        match self {
            Self::Vegetation => "植被",
            Self::Stone => "石质",
            Self::Ruin => "遗迹",
            Self::Village => "村庄",
            Self::Waterside => "水岸",
            Self::Ecology => "生态",
            Self::Interaction => "交互",
            Self::Omen => "征兆",
        }
    }
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
            Self::TrunkOnly => "主体",
            Self::TrunkAndRoots => "主体+扩展",
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
    RockPrimary = 6,
    RockStrata = 7,
    RockWet = 8,
    RockMoss = 9,
    RockShadow = 10,
    RuinCore = 11,
    RuinEdge = 12,
    RuinDust = 13,
    RuinMoss = 14,
    RuinShadow = 15,
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
            Self::RockPrimary => "rock_primary",
            Self::RockStrata => "rock_strata",
            Self::RockWet => "rock_wet",
            Self::RockMoss => "rock_moss",
            Self::RockShadow => "rock_shadow",
            Self::RuinCore => "ruin_core",
            Self::RuinEdge => "ruin_edge",
            Self::RuinDust => "ruin_dust",
            Self::RuinMoss => "ruin_moss",
            Self::RuinShadow => "ruin_shadow",
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
    pub golden_seeds: Vec<u64>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum ObjectFamilyGeneratorKind {
    Tree,
    Rock,
    RuinFragment,
    Placeholder,
}

impl ObjectFamilyGeneratorKind {
    fn generate(
        self,
        request: ObjectGenerationRequest,
        family: &ObjectFamilyDefinition,
    ) -> ObjectGeneratedAsset {
        match self {
            Self::Tree => families::tree::generate_asset(request, family),
            Self::Rock => families::rock::generate_asset(request, family),
            Self::RuinFragment => families::ruin_fragment::generate_asset(request, family),
            Self::Placeholder => generate_placeholder_asset(request, family),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct RegisteredObjectFamily {
    definition: ObjectFamilyDefinition,
    generator: ObjectFamilyGeneratorKind,
}

impl RegisteredObjectFamily {
    fn new(definition: ObjectFamilyDefinition, generator: ObjectFamilyGeneratorKind) -> Self {
        Self {
            definition,
            generator,
        }
    }
}

#[derive(Debug, Resource, Clone, PartialEq)]
pub struct ProceduralObjectRegistry {
    families: Vec<RegisteredObjectFamily>,
}

impl Default for ProceduralObjectRegistry {
    fn default() -> Self {
        Self {
            families: vec![
                RegisteredObjectFamily::new(
                    families::tree::definition(),
                    ObjectFamilyGeneratorKind::Tree,
                ),
                RegisteredObjectFamily::new(
                    families::rock::definition(),
                    ObjectFamilyGeneratorKind::Rock,
                ),
                RegisteredObjectFamily::new(
                    families::ruin_fragment::definition(),
                    ObjectFamilyGeneratorKind::RuinFragment,
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
    pub fn families(&self) -> impl Iterator<Item = &ObjectFamilyDefinition> {
        self.families.iter().map(|family| &family.definition)
    }

    pub fn family(&self, kind: ObjectKind) -> Option<&ObjectFamilyDefinition> {
        self.families
            .iter()
            .find(|family| family.definition.kind == kind)
            .map(|family| &family.definition)
    }

    pub fn by_semantic(&self, semantic: ObjectSemantic) -> Vec<&ObjectFamilyDefinition> {
        self.families
            .iter()
            .filter(|family| family.definition.semantics.contains(&semantic))
            .map(|family| &family.definition)
            .collect()
    }

    pub fn generate(&self, request: ObjectGenerationRequest) -> Option<ObjectGeneratedAsset> {
        self.families
            .iter()
            .find(|family| family.definition.kind == request.kind)
            .map(|family| family.generator.generate(request, &family.definition))
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

    pub fn rock(seed: u64, lod: ObjectLod, transform: Transform) -> Self {
        Self {
            kind: ObjectKind::Rock,
            seed,
            lod,
            transform,
            biome: ObjectBiomeContext::Ridge,
            weather: ObjectWeatherState::Clear,
            material_variant: ObjectMaterialVariant::Default,
            collision_mode: ObjectCollisionMode::TrunkAndRoots,
            mode: ObjectGenerationMode::Gallery,
        }
    }

    pub fn ruin_fragment(seed: u64, lod: ObjectLod, transform: Transform) -> Self {
        Self {
            kind: ObjectKind::RuinFragment,
            seed,
            lod,
            transform,
            biome: ObjectBiomeContext::RuinEdge,
            weather: ObjectWeatherState::Clear,
            material_variant: ObjectMaterialVariant::Dusty,
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
pub struct ProceduralRockProfile {
    pub seed: u64,
    pub biome: ObjectBiomeContext,
    pub base_radius: f32,
    pub height: f32,
    pub elongation: f32,
    pub flatten: f32,
    pub tilt: Vec2,
    pub strata_strength: f32,
    pub crack_density: f32,
    pub erosion: f32,
    pub wet_line: f32,
    pub moss_ratio: f32,
    pub shard_count: usize,
    pub collider_radius: f32,
    pub collider_height: f32,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
pub struct ProceduralRuinFragmentProfile {
    pub seed: u64,
    pub biome: ObjectBiomeContext,
    pub width: f32,
    pub depth: f32,
    pub height: f32,
    pub tilt: Vec2,
    pub fracture: f32,
    pub erosion: f32,
    pub sand_cover: f32,
    pub moss_ratio: f32,
    pub relic_ratio: f32,
    pub column_count: usize,
    pub debris_count: usize,
    pub collider_radius: f32,
    pub collider_height: f32,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum GeneratedObjectProfile {
    Tree(ProceduralTreeProfile),
    Rock(ProceduralRockProfile),
    RuinFragment(ProceduralRuinFragmentProfile),
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
    family_index: usize,
    focus_index: usize,
}

impl Default for ObjectGalleryState {
    fn default() -> Self {
        Self {
            export_queue: GalleryExportQueue::new(
                "logs/object-gallery-manifest.json",
                "logs/object-gallery.png",
            ),
            family_index: 0,
            focus_index: 0,
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
struct ObjectSlotMaterials {
    bark_primary: Handle<StandardMaterial>,
    bark_worn_edge: Handle<StandardMaterial>,
    leaf_primary: Handle<StandardMaterial>,
    leaf_secondary: Handle<StandardMaterial>,
    leaf_dry: Handle<StandardMaterial>,
    root_shadow: Handle<StandardMaterial>,
    rock_primary: Handle<StandardMaterial>,
    rock_strata: Handle<StandardMaterial>,
    rock_wet: Handle<StandardMaterial>,
    rock_moss: Handle<StandardMaterial>,
    rock_shadow: Handle<StandardMaterial>,
    ruin_core: Handle<StandardMaterial>,
    ruin_edge: Handle<StandardMaterial>,
    ruin_dust: Handle<StandardMaterial>,
    ruin_moss: Handle<StandardMaterial>,
    ruin_shadow: Handle<StandardMaterial>,
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
    performance: Res<'w, FramePerformance>,
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
    Rock {
        base_radius: f32,
        height: f32,
        elongation: f32,
        flatten: f32,
        strata_strength: f32,
        crack_density: f32,
        erosion: f32,
        wet_line: f32,
        moss_ratio: f32,
        shard_count: usize,
    },
    RuinFragment {
        width: f32,
        depth: f32,
        height: f32,
        fracture: f32,
        erosion: f32,
        sand_cover: f32,
        moss_ratio: f32,
        relic_ratio: f32,
        column_count: usize,
        debris_count: usize,
    },
}

const TREE_SAMPLE_COUNT_PER_LOD: usize = 6;
const ROCK_SAMPLE_COUNT_PER_LOD: usize = 5;
const RUIN_SAMPLE_COUNT_PER_LOD: usize = 4;
const OBJECT_EXPORT_COOLDOWN_SECONDS: f32 = 0.55;

fn simple_family(kind: ObjectKind, semantics: Vec<ObjectSemantic>) -> RegisteredObjectFamily {
    RegisteredObjectFamily::new(
        ObjectFamilyDefinition {
            kind,
            profile_version: 1,
            geometry_version: 1,
            semantics,
            material_slots: Vec::new(),
            golden_seeds: default_family_golden_seeds(kind),
        },
        ObjectFamilyGeneratorKind::Placeholder,
    )
}

fn default_family_golden_seeds(kind: ObjectKind) -> Vec<u64> {
    (0..3_u64)
        .map(|index| {
            families::tree::GALLERY_BASE_SEED
                .wrapping_add((kind as u64 + 1) * 0x9E37_79B9)
                .wrapping_add(index * 0xBF58_476D)
        })
        .collect()
}

fn spawn_procedural_object_gallery(mut params: ObjectGallerySpawnParams) {
    let started_at = Instant::now();
    params.gallery_state.export_queue = GalleryExportQueue::new(
        "logs/object-gallery-manifest.json",
        "logs/object-gallery.png",
    );
    params.gallery_state.family_index = 0;
    params.gallery_state.focus_index = 0;
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
            .add(Mesh::from(Plane3d::new(Vec3::Y, Vec2::new(112.0, 30.0)))),
        cylinder: params.meshes.add(Mesh::from(Cylinder::new(1.0, 1.0))),
        sphere: params.meshes.add(Sphere::new(1.0).mesh().uv(20, 14)),
        cuboid: params.meshes.add(Mesh::from(Cuboid::new(1.0, 1.0, 1.0))),
    };
    let slot_materials = object_slot_materials(&params.asset_materials);

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
                Transform::from_xyz(50.0, -0.04, -15.0),
            ));
            for request in object_gallery_requests() {
                if params.registry.family(request.kind).is_some() {
                    let asset = generate_object_asset(request, &params.registry);
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
        samples = TREE_SAMPLE_COUNT_PER_LOD * 3
            + ROCK_SAMPLE_COUNT_PER_LOD * 3
            + RUIN_SAMPLE_COUNT_PER_LOD * 3,
        family_count = params.registry.families().count(),
        "procedural object gallery spawned with near/mid/far multi-family samples"
    );
    params
        .performance
        .record_phase_duration(PerformancePhase::Assets, started_at.elapsed());
}

fn object_gallery_requests() -> Vec<ObjectGenerationRequest> {
    let mut requests = Vec::with_capacity(
        TREE_SAMPLE_COUNT_PER_LOD * 3
            + ROCK_SAMPLE_COUNT_PER_LOD * 3
            + RUIN_SAMPLE_COUNT_PER_LOD * 3,
    );
    requests.extend(tree_gallery_requests());
    requests.extend(rock_gallery_requests());
    requests.extend(ruin_fragment_gallery_requests());
    requests
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
            let seed = families::tree::GALLERY_BASE_SEED
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

fn rock_gallery_requests() -> Vec<ObjectGenerationRequest> {
    let mut requests = Vec::with_capacity(ROCK_SAMPLE_COUNT_PER_LOD * 3);
    let biomes = [
        ObjectBiomeContext::Ridge,
        ObjectBiomeContext::Wetland,
        ObjectBiomeContext::RuinEdge,
        ObjectBiomeContext::DesertWind,
        ObjectBiomeContext::Meadow,
        ObjectBiomeContext::VillageCourtyard,
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
    ];
    let lod_rows = [ObjectLod::Near, ObjectLod::Mid, ObjectLod::Far];

    for (lod_row, lod) in lod_rows.into_iter().enumerate() {
        for column in 0..ROCK_SAMPLE_COUNT_PER_LOD {
            let seed = families::rock::GALLERY_BASE_SEED
                .wrapping_add((lod_row as u64 + 1) * 0x94D0_49BB)
                .wrapping_add(column as u64 * 0x9E37_79B9);
            let mut request = ObjectGenerationRequest {
                kind: ObjectKind::Rock,
                seed,
                lod,
                transform: Transform::from_xyz(
                    43.0 + column as f32 * 6.8,
                    0.0,
                    -6.0 - lod_row as f32 * 8.0,
                ),
                biome: biomes[(column + lod_row) % biomes.len()],
                weather: weather[(column * 2 + lod_row) % weather.len()],
                material_variant: material_variant[(column + lod_row) % material_variant.len()],
                collision_mode: match lod {
                    ObjectLod::Near => ObjectCollisionMode::Full,
                    ObjectLod::Mid => ObjectCollisionMode::TrunkAndRoots,
                    ObjectLod::Far => ObjectCollisionMode::VisualOnly,
                },
                mode: ObjectGenerationMode::Gallery,
            };
            if request.weather == ObjectWeatherState::DreamTint {
                request.material_variant = ObjectMaterialVariant::Mossy;
            }
            requests.push(request);
        }
    }

    requests
}

fn ruin_fragment_gallery_requests() -> Vec<ObjectGenerationRequest> {
    let mut requests = Vec::with_capacity(RUIN_SAMPLE_COUNT_PER_LOD * 3);
    let biomes = [
        ObjectBiomeContext::RuinEdge,
        ObjectBiomeContext::DesertWind,
        ObjectBiomeContext::Wetland,
        ObjectBiomeContext::Ridge,
    ];
    let weather = [
        ObjectWeatherState::Clear,
        ObjectWeatherState::DryWind,
        ObjectWeatherState::RainSoaked,
        ObjectWeatherState::DreamTint,
    ];
    let material_variant = [
        ObjectMaterialVariant::Dusty,
        ObjectMaterialVariant::Default,
        ObjectMaterialVariant::Mossy,
        ObjectMaterialVariant::Dream,
    ];
    let lod_rows = [ObjectLod::Near, ObjectLod::Mid, ObjectLod::Far];

    for (lod_row, lod) in lod_rows.into_iter().enumerate() {
        for column in 0..RUIN_SAMPLE_COUNT_PER_LOD {
            let seed = families::ruin_fragment::GALLERY_BASE_SEED
                .wrapping_add((lod_row as u64 + 1) * 0xD6E8_FD9D)
                .wrapping_add(column as u64 * 0x94D0_49BB);
            let mut request = ObjectGenerationRequest::ruin_fragment(
                seed,
                lod,
                Transform::from_xyz(78.0 + column as f32 * 6.6, 0.0, -6.0 - lod_row as f32 * 8.0),
            );
            request.biome = biomes[(column + lod_row) % biomes.len()];
            request.weather = weather[(column + lod_row * 2) % weather.len()];
            request.material_variant =
                material_variant[(column + lod_row) % material_variant.len()];
            request.collision_mode = match lod {
                ObjectLod::Near => ObjectCollisionMode::Full,
                ObjectLod::Mid => ObjectCollisionMode::TrunkAndRoots,
                ObjectLod::Far => ObjectCollisionMode::VisualOnly,
            };
            requests.push(request);
        }
    }

    requests
}

fn generate_object_asset(
    request: ObjectGenerationRequest,
    registry: &ProceduralObjectRegistry,
) -> ObjectGeneratedAsset {
    registry
        .generate(request)
        .expect("object family should be registered")
}

fn generate_placeholder_asset(
    request: ObjectGenerationRequest,
    family: &ObjectFamilyDefinition,
) -> ObjectGeneratedAsset {
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
        profile: GeneratedObjectProfile::Tree(families::tree::build_profile(
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

fn spawn_generated_object(
    parent: &mut ChildSpawnerCommands<'_>,
    meshes: &ObjectMeshHandles,
    slot_materials: &ObjectSlotMaterials,
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
                if let Some(profile) = tree_wind_profile(&asset.profile)
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
                spawn_object_colliders(tree, asset.kind, &asset.collision);
            }
        });

    tracing::info!(
        target: "dao_game::objects::gallery",
        kind = asset.kind.export_label(),
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

fn spawn_object_colliders(
    parent: &mut ChildSpawnerCommands<'_>,
    kind: ObjectKind,
    collision: &ObjectCollisionRecipe,
) {
    if let Some(trunk) = &collision.trunk {
        parent.spawn((
            Name::new(format!("{}MainCollider", kind.label())),
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
            Name::new(format!("{}Blocker{index}", kind.label())),
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
            Name::new(format!("{}ObservationSensor", kind.label())),
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

fn tree_wind_profile(profile: &GeneratedObjectProfile) -> Option<ProceduralTreeProfile> {
    match profile {
        GeneratedObjectProfile::Tree(profile) => Some(*profile),
        GeneratedObjectProfile::Rock(_) => None,
        GeneratedObjectProfile::RuinFragment(_) => None,
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
    materials: &ObjectSlotMaterials,
    slot: ObjectMaterialSlot,
) -> Handle<StandardMaterial> {
    match slot {
        ObjectMaterialSlot::BarkPrimary => materials.bark_primary.clone(),
        ObjectMaterialSlot::BarkWornEdge => materials.bark_worn_edge.clone(),
        ObjectMaterialSlot::LeafPrimary => materials.leaf_primary.clone(),
        ObjectMaterialSlot::LeafSecondary => materials.leaf_secondary.clone(),
        ObjectMaterialSlot::LeafDry => materials.leaf_dry.clone(),
        ObjectMaterialSlot::RootShadow => materials.root_shadow.clone(),
        ObjectMaterialSlot::RockPrimary => materials.rock_primary.clone(),
        ObjectMaterialSlot::RockStrata => materials.rock_strata.clone(),
        ObjectMaterialSlot::RockWet => materials.rock_wet.clone(),
        ObjectMaterialSlot::RockMoss => materials.rock_moss.clone(),
        ObjectMaterialSlot::RockShadow => materials.rock_shadow.clone(),
        ObjectMaterialSlot::RuinCore => materials.ruin_core.clone(),
        ObjectMaterialSlot::RuinEdge => materials.ruin_edge.clone(),
        ObjectMaterialSlot::RuinDust => materials.ruin_dust.clone(),
        ObjectMaterialSlot::RuinMoss => materials.ruin_moss.clone(),
        ObjectMaterialSlot::RuinShadow => materials.ruin_shadow.clone(),
    }
}

fn object_slot_materials(materials: &ProceduralAssetMaterials) -> ObjectSlotMaterials {
    ObjectSlotMaterials {
        bark_primary: materials.handle_for_family(ProceduralMaterialFamily::Wood),
        bark_worn_edge: materials.handle_for_family(ProceduralMaterialFamily::OldStone),
        leaf_primary: materials.handle_for_family(ProceduralMaterialFamily::GroveLeaf),
        leaf_secondary: materials.handle_for_family(ProceduralMaterialFamily::GroveLeaf),
        leaf_dry: materials.handle_for_family(ProceduralMaterialFamily::Sand),
        root_shadow: materials.handle_for_family(ProceduralMaterialFamily::Shadow),
        rock_primary: materials.handle_for_family(ProceduralMaterialFamily::Stone),
        rock_strata: materials.handle_for_family(ProceduralMaterialFamily::OldStone),
        rock_wet: materials.handle_for_family(ProceduralMaterialFamily::Water),
        rock_moss: materials.handle_for_family(ProceduralMaterialFamily::GroveLeaf),
        rock_shadow: materials.handle_for_family(ProceduralMaterialFamily::Shadow),
        ruin_core: materials.handle_for_family(ProceduralMaterialFamily::OldStone),
        ruin_edge: materials.handle_for_family(ProceduralMaterialFamily::Relic),
        ruin_dust: materials.handle_for_family(ProceduralMaterialFamily::Sand),
        ruin_moss: materials.handle_for_family(ProceduralMaterialFamily::GroveLeaf),
        ruin_shadow: materials.handle_for_family(ProceduralMaterialFamily::Shadow),
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
    performance: Res<FramePerformance>,
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
    if keys.just_pressed(KeyCode::Comma) {
        state.family_index = state.family_index.saturating_sub(1);
        state.focus_index = 0;
    } else if keys.just_pressed(KeyCode::Period) {
        state.family_index = state.family_index.saturating_add(1);
        state.focus_index = 0;
    } else if keys.just_pressed(KeyCode::Minus) {
        state.focus_index = state.focus_index.saturating_sub(1);
    } else if keys.just_pressed(KeyCode::Equal) {
        state.focus_index = state.focus_index.saturating_add(1);
    }

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
    performance: Res<FramePerformance>,
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
    mut gallery_state: ResMut<ObjectGalleryState>,
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
    let visible_kinds = visible_gallery_kinds(&instances);
    gallery_state.family_index = gallery_state
        .family_index
        .min(visible_kinds.len().saturating_sub(1));
    let current_kind = visible_kinds[gallery_state.family_index];
    let family_definitions: Vec<_> = registry.families().collect();
    let current_family_samples: Vec<_> = instances
        .iter()
        .copied()
        .filter(|instance| instance.kind == current_kind)
        .collect();
    gallery_state.focus_index = gallery_state
        .focus_index
        .min(current_family_samples.len().saturating_sub(1));
    let sample = current_family_samples[gallery_state.focus_index];
    let total_near_count = instances
        .iter()
        .filter(|instance| instance.lod == ObjectLod::Near)
        .count();
    let total_mid_count = instances
        .iter()
        .filter(|instance| instance.lod == ObjectLod::Mid)
        .count();
    let total_far_count = instances
        .iter()
        .filter(|instance| instance.lod == ObjectLod::Far)
        .count();
    let near_count = current_family_samples
        .iter()
        .filter(|instance| instance.lod == ObjectLod::Near)
        .count();
    let mid_count = current_family_samples
        .iter()
        .filter(|instance| instance.lod == ObjectLod::Mid)
        .count();
    let far_count = current_family_samples
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
    let (profile_version, geometry_version, slot_count, golden_seeds, semantics) = registry
        .family(sample.kind)
        .map(|family| {
            (
                family.profile_version,
                family.geometry_version,
                family.material_slots.len(),
                family.golden_seeds.clone(),
                family.semantics.clone(),
            )
        })
        .unwrap_or((
            sample.profile_version,
            sample.geometry_version,
            sample.material_slots.len(),
            Vec::new(),
            Vec::new(),
        ));

    let summary_lines = vec![
        format!(
            "家族：{}  家族页：{}/{}  家族样本：{}  总样本：{}",
            sample.kind.label(),
            gallery_state.family_index + 1,
            visible_kinds.len(),
            current_family_samples.len(),
            instances.len()
        ),
        format!(
            "当前家族近/中/远：{}/{}/{}  全展区近/中/远：{}/{}/{}  焦点：{}/{}",
            near_count,
            mid_count,
            far_count,
            total_near_count,
            total_mid_count,
            total_far_count,
            gallery_state.focus_index + 1,
            current_family_samples.len()
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
        format!(
            "版本：profile v{}  geometry v{}  语义：{}",
            profile_version,
            geometry_version,
            format_semantics(&semantics)
        ),
    ];

    let mut inspector_lines = Vec::new();
    match sample.profile {
        GeneratedObjectProfile::Tree(profile) => inspector_lines.push(format!(
            "树参数：年龄 {:.0}y  高度 {:.1}m  健康 {:.0}%  叶密 {:.0}%  根暴露 {:.0}%  风柔性 {:.0}%",
            profile.age_years,
            profile.height,
            profile.health * 100.0,
            profile.leaf_density * 100.0,
            profile.root_exposure * 100.0,
            profile.wind_flex * 100.0
        )),
        GeneratedObjectProfile::Rock(profile) => inspector_lines.push(format!(
            "岩石参数：半径 {:.2}m  高度 {:.2}m  层理 {:.0}%  裂纹 {:.0}%  侵蚀 {:.0}%  湿线 {:.0}%  苔藓 {:.0}%  碎块 {}",
            profile.base_radius,
            profile.height,
            profile.strata_strength * 100.0,
            profile.crack_density * 100.0,
            profile.erosion * 100.0,
            profile.wet_line * 100.0,
            profile.moss_ratio * 100.0,
            profile.shard_count
        )),
        GeneratedObjectProfile::RuinFragment(profile) => inspector_lines.push(format!(
            "遗迹参数：宽 {:.2}m  深 {:.2}m  高 {:.2}m  裂损 {:.0}%  侵蚀 {:.0}%  覆沙 {:.0}%  苔藓 {:.0}%  遗物 {:.0}%  立柱 {}  碎块 {}",
            profile.width,
            profile.depth,
            profile.height,
            profile.fracture * 100.0,
            profile.erosion * 100.0,
            profile.sand_cover * 100.0,
            profile.moss_ratio * 100.0,
            profile.relic_ratio * 100.0,
            profile.column_count,
            profile.debris_count
        )),
    }

    let approval_lines = vec![approval_distribution_line(&current_family_samples)];
    let browser_lines = build_seed_browser_lines(&current_family_samples);
    let family_lines = build_family_summary_lines(
        &family_definitions,
        &visible_kinds,
        sample.kind,
        &golden_seeds,
        gallery_state.family_index,
        current_family_samples.len(),
    );
    let slot_lines = build_slot_lines(&sample.material_slots);
    let export_lines = vec![
        format!("清单：{}", gallery_state.export_queue.export_path.display()),
        format!(
            "截图：{}",
            gallery_state.export_queue.screenshot_path.display()
        ),
        "操作：, / . 切换家族  - / = 切换样本  O 导出对象清单  Shift+O 导出对象清单+截图  E 导出材质清单"
            .to_string(),
    ];
    let overview_lines = summary_lines.clone();

    codex_state.visible = true;
    codex_state.title = "AssetCodex".to_string();
    codex_state.subtitle = "正式图鉴预览".to_string();
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
    codex_state.sections = vec![
        AssetCodexSection {
            title: "审查概览".to_string(),
            lines: overview_lines,
        },
        AssetCodexSection {
            title: "参数 Inspector".to_string(),
            lines: inspector_lines,
        },
        AssetCodexSection {
            title: "Seed 浏览".to_string(),
            lines: browser_lines,
        },
        AssetCodexSection {
            title: "家族注册".to_string(),
            lines: family_lines.into_iter().chain(approval_lines).collect(),
        },
        AssetCodexSection {
            title: "材质槽".to_string(),
            lines: slot_lines,
        },
        AssetCodexSection {
            title: "导出".to_string(),
            lines: export_lines,
        },
    ];
    codex_state.controls_hint =
        ", / . 切换家族  - / = 切换样本  O 导出对象清单  Shift+O 导出对象清单+截图  E 导出材质清单"
            .to_string();
    codex_state.export_manifest_path = gallery_state.export_queue.export_path.display().to_string();
    codex_state.screenshot_path = gallery_state
        .export_queue
        .screenshot_path
        .display()
        .to_string();
}

fn format_semantics(semantics: &[ObjectSemantic]) -> String {
    if semantics.is_empty() {
        return "未声明".to_string();
    }
    semantics
        .iter()
        .map(|semantic| semantic.label())
        .collect::<Vec<_>>()
        .join(" / ")
}

fn approval_distribution_line(instances: &[&ProceduralObjectInstance]) -> String {
    let mut satisfied = 0;
    let mut needs_revision = 0;
    let mut performance_risk = 0;
    let mut waiting_material = 0;
    let mut disabled = 0;
    for instance in instances {
        match instance.approval {
            ObjectApprovalState::Satisfied => satisfied += 1,
            ObjectApprovalState::NeedsRevision => needs_revision += 1,
            ObjectApprovalState::PerformanceRisk => performance_risk += 1,
            ObjectApprovalState::WaitingMaterial => waiting_material += 1,
            ObjectApprovalState::Disabled => disabled += 1,
        }
    }
    format!(
        "审核分布：满意 {}  需改 {}  性能风险 {}  待材质 {}  禁用 {}",
        satisfied, needs_revision, performance_risk, waiting_material, disabled
    )
}

fn visible_gallery_kinds(instances: &[&ProceduralObjectInstance]) -> Vec<ObjectKind> {
    let mut kinds = Vec::new();
    for instance in instances {
        if !kinds.contains(&instance.kind) {
            kinds.push(instance.kind);
        }
    }
    kinds.sort_by_key(|kind| *kind as u8);
    kinds
}

fn build_seed_browser_lines(instances: &[&ProceduralObjectInstance]) -> Vec<String> {
    if instances.is_empty() {
        return vec!["当前家族还没有可见样本".to_string()];
    }

    let mut lines = Vec::new();
    for lod in [ObjectLod::Near, ObjectLod::Mid, ObjectLod::Far] {
        let seeds: Vec<_> = instances
            .iter()
            .filter(|instance| instance.lod == lod)
            .take(4)
            .map(|instance| format!("{}#{:04x}", lod.export_label(), instance.seed & 0xFFFF))
            .collect();
        if !seeds.is_empty() {
            lines.push(seeds.join("  "));
        }
    }
    lines
}

fn build_family_summary_lines(
    families: &[&ObjectFamilyDefinition],
    visible_kinds: &[ObjectKind],
    current_kind: ObjectKind,
    golden_seeds: &[u64],
    family_index: usize,
    visible_sample_count: usize,
) -> Vec<String> {
    let current_family = families
        .iter()
        .copied()
        .find(|family| family.kind == current_kind);
    let mut lines = vec![format!(
        "已注册 {} 个家族  已生成 {} 个家族  当前可见样本 {}  当前语义 {}",
        families.len(),
        visible_kinds.len(),
        visible_sample_count,
        current_family
            .map(|family| format_semantics(&family.semantics))
            .unwrap_or_else(|| "未声明".to_string())
    )];
    if !golden_seeds.is_empty() {
        lines.push(format!(
            "Golden Seeds：{}",
            golden_seeds
                .iter()
                .take(4)
                .map(|seed| format!("{seed:#014x}"))
                .collect::<Vec<_>>()
                .join("  ")
        ));
    }
    lines.push(format!(
        "浏览：家族 {}/{}  样本切换使用 - / =",
        family_index + 1,
        visible_kinds.len()
    ));
    lines.push(format!(
        "家族索引：{}",
        families
            .iter()
            .map(|family| {
                let marker = if family.kind == current_kind {
                    ">"
                } else if visible_kinds.contains(&family.kind) {
                    "+"
                } else {
                    "-"
                };
                format!(
                    "{marker}{} v{}/{}",
                    family.kind.label(),
                    family.profile_version,
                    family.geometry_version
                )
            })
            .collect::<Vec<_>>()
            .join("  ")
    ));
    lines
}

fn build_slot_lines(slots: &[ObjectMaterialBinding]) -> Vec<String> {
    if slots.is_empty() {
        return vec!["当前家族没有材质槽声明".to_string()];
    }

    slots
        .iter()
        .map(|slot| {
            format!(
                "{} -> {} / {}",
                slot.slot.export_label(),
                material_family_label(slot.material_family),
                slot.material_id
            )
        })
        .collect()
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
        GeneratedObjectProfile::Rock(profile) => ObjectGalleryExportProfile::Rock {
            base_radius: profile.base_radius,
            height: profile.height,
            elongation: profile.elongation,
            flatten: profile.flatten,
            strata_strength: profile.strata_strength,
            crack_density: profile.crack_density,
            erosion: profile.erosion,
            wet_line: profile.wet_line,
            moss_ratio: profile.moss_ratio,
            shard_count: profile.shard_count,
        },
        GeneratedObjectProfile::RuinFragment(profile) => ObjectGalleryExportProfile::RuinFragment {
            width: profile.width,
            depth: profile.depth,
            height: profile.height,
            fracture: profile.fracture,
            erosion: profile.erosion,
            sand_cover: profile.sand_cover,
            moss_ratio: profile.moss_ratio,
            relic_ratio: profile.relic_ratio,
            column_count: profile.column_count,
            debris_count: profile.debris_count,
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
    match asset.profile {
        GeneratedObjectProfile::Tree(profile) => {
            if profile.health < 0.22 || profile.dead_branch_ratio > 0.74 {
                return ObjectApprovalState::NeedsRevision;
            }
        }
        GeneratedObjectProfile::Rock(profile) => {
            if profile.flatten < 0.28
                || profile.crack_density > 0.9
                || (matches!(asset.request.material_variant, ObjectMaterialVariant::Wet)
                    && profile.wet_line < 0.18)
            {
                return ObjectApprovalState::NeedsRevision;
            }
        }
        GeneratedObjectProfile::RuinFragment(profile) => {
            if profile.fracture > 0.94
                || profile.height < 0.9
                || (matches!(asset.request.material_variant, ObjectMaterialVariant::Dream)
                    && profile.relic_ratio < 0.22)
            {
                return ObjectApprovalState::NeedsRevision;
            }
        }
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
        ProceduralObjectRegistry, stable_object_id,
    };
    use crate::game::gallery::GalleryExportMode;
    use bevy::prelude::Transform;

    #[test]
    fn tree_profile_is_deterministic_for_same_request() {
        let request = ObjectGenerationRequest {
            kind: ObjectKind::Tree,
            seed: super::families::tree::GALLERY_BASE_SEED,
            lod: ObjectLod::Near,
            transform: Transform::default(),
            biome: ObjectBiomeContext::Meadow,
            weather: ObjectWeatherState::Clear,
            material_variant: super::ObjectMaterialVariant::Default,
            collision_mode: ObjectCollisionMode::Full,
            mode: super::ObjectGenerationMode::Gallery,
        };
        let first = super::families::tree::build_profile(&request);
        let second = super::families::tree::build_profile(&request);
        assert_eq!(first, second);
    }

    #[test]
    fn tree_profile_changes_with_seed_and_context() {
        let mut request = ObjectGenerationRequest::tree(
            super::families::tree::GALLERY_BASE_SEED,
            ObjectLod::Near,
            Transform::default(),
        );
        let first = super::families::tree::build_profile(&request);
        request.seed = super::families::tree::GALLERY_BASE_SEED + 97;
        request.biome = ObjectBiomeContext::Wetland;
        request.weather = ObjectWeatherState::DryWind;
        let second = super::families::tree::build_profile(&request);

        assert_ne!(first.height, second.height);
        assert_ne!(first.leaf_color, second.leaf_color);
        assert_ne!(first.branch_count, second.branch_count);
    }

    #[test]
    fn tree_profile_bounds_stay_reviewable() {
        for index in 0..48_u64 {
            let mut request = ObjectGenerationRequest::tree(
                super::families::tree::GALLERY_BASE_SEED + index,
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
            let profile = super::families::tree::build_profile(&request);

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
            super::families::tree::PROFILE_VERSION,
            super::families::tree::GEOMETRY_VERSION,
        );
        let b = stable_object_id(
            ObjectKind::Tree,
            42,
            super::families::tree::PROFILE_VERSION,
            super::families::tree::GEOMETRY_VERSION,
        );
        let c = stable_object_id(
            ObjectKind::Tree,
            43,
            super::families::tree::PROFILE_VERSION,
            super::families::tree::GEOMETRY_VERSION,
        );
        let d = stable_object_id(
            ObjectKind::Rock,
            42,
            super::families::tree::PROFILE_VERSION,
            super::families::tree::GEOMETRY_VERSION,
        );
        let e = stable_object_id(
            ObjectKind::Tree,
            42,
            super::families::tree::PROFILE_VERSION + 1,
            super::families::tree::GEOMETRY_VERSION,
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
        let family = super::families::tree::definition();
        let near_request =
            ObjectGenerationRequest::tree(0xABC, ObjectLod::Near, Transform::default());
        let far_request =
            ObjectGenerationRequest::tree(0xABC, ObjectLod::Far, Transform::default());

        let near = super::families::tree::generate_asset(near_request, &family);
        let far = super::families::tree::generate_asset(far_request, &family);

        assert!(near.stats.part_count > far.stats.part_count);
        assert!(near.stats.vertex_estimate > far.stats.vertex_estimate);
        assert!(near.stats.collider_count >= far.stats.collider_count);
        assert!(matches!(near.profile, GeneratedObjectProfile::Tree(_)));
    }

    #[test]
    fn tree_family_declares_golden_seeds_and_versions() {
        let family = super::families::tree::definition();
        assert_eq!(
            family.profile_version,
            super::families::tree::PROFILE_VERSION
        );
        assert_eq!(
            family.geometry_version,
            super::families::tree::GEOMETRY_VERSION
        );
        assert!(family.golden_seeds.len() >= 3);
    }

    #[test]
    fn rock_family_declares_golden_seeds_and_versions() {
        let family = super::families::rock::definition();
        assert_eq!(
            family.profile_version,
            super::families::rock::PROFILE_VERSION
        );
        assert_eq!(
            family.geometry_version,
            super::families::rock::GEOMETRY_VERSION
        );
        assert_eq!(family.kind, ObjectKind::Rock);
        assert!(family.golden_seeds.len() >= 3);
    }

    #[test]
    fn rock_generation_contains_collision_and_material_slots() {
        let family = super::families::rock::definition();
        let request = ObjectGenerationRequest {
            kind: ObjectKind::Rock,
            seed: super::families::rock::GALLERY_BASE_SEED,
            lod: ObjectLod::Near,
            transform: Transform::default(),
            biome: ObjectBiomeContext::Ridge,
            weather: ObjectWeatherState::RainSoaked,
            material_variant: super::ObjectMaterialVariant::Wet,
            collision_mode: ObjectCollisionMode::Full,
            mode: super::ObjectGenerationMode::Gallery,
        };

        let generated = super::families::rock::generate_asset(request, &family);
        assert!(matches!(generated.profile, GeneratedObjectProfile::Rock(_)));
        assert!(generated.stats.part_count >= 4);
        assert!(generated.collision.trunk.is_some());
        assert!(!generated.material_slots.is_empty());
    }

    #[test]
    fn ruin_fragment_family_declares_golden_seeds_and_versions() {
        let family = super::families::ruin_fragment::definition();
        assert_eq!(
            family.profile_version,
            super::families::ruin_fragment::PROFILE_VERSION
        );
        assert_eq!(
            family.geometry_version,
            super::families::ruin_fragment::GEOMETRY_VERSION
        );
        assert_eq!(family.kind, ObjectKind::RuinFragment);
        assert!(family.golden_seeds.len() >= 3);
    }

    #[test]
    fn ruin_fragment_generation_contains_parts_and_collision() {
        let family = super::families::ruin_fragment::definition();
        let mut request = ObjectGenerationRequest::ruin_fragment(
            super::families::ruin_fragment::GALLERY_BASE_SEED,
            ObjectLod::Near,
            Transform::default(),
        );
        request.biome = ObjectBiomeContext::RuinEdge;
        request.weather = ObjectWeatherState::DryWind;
        request.material_variant = super::ObjectMaterialVariant::Dusty;
        request.collision_mode = ObjectCollisionMode::Full;

        let generated = super::families::ruin_fragment::generate_asset(request, &family);
        assert!(matches!(
            generated.profile,
            GeneratedObjectProfile::RuinFragment(_)
        ));
        assert!(generated.stats.part_count >= 5);
        assert!(generated.collision.trunk.is_some());
        assert!(!generated.material_slots.is_empty());
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
