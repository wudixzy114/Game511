use std::collections::HashSet;

use bevy::prelude::*;

use crate::game::flow::{AppScreen, InGameState};

pub struct ProceduralAssetPlugin;

impl Plugin for ProceduralAssetPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(ProceduralAssetRegistry::default());
        app.insert_resource(ProceduralAssetRuntimeStats::default());
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
) {
    let asset_count = query.iter().count();
    if asset_count == 0 {
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
        ProceduralAssetKind, ProceduralAssetRegistry, ProceduralLodStrategy, ProceduralSemantic,
        registered_spec, stable_asset_id,
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
}
