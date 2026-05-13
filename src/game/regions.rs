use std::time::Instant;

use bevy::{
    color::LinearRgba,
    math::primitives::{Cuboid, Cylinder, Plane3d},
    pbr::MeshMaterial3d,
    prelude::*,
};

use crate::{
    core::performance::{FramePerformance, PerformancePhase},
    game::{
        assets::{
            ProceduralAssetKind, ProceduralAssetLod, ProceduralAssetMaterials,
            ProceduralSpawnRequest, spawn_procedural_asset_entity,
        },
        environment::{EnvironmentSnapshot, WindField},
        flow::{AppScreen, InGameState},
        intent::{IntentKind, IntentState, PerceptionState},
        journey::{DreamPhase, JourneyState},
        notebook::{
            NotebookEntryKind, NotebookRecord, NotebookSource, NotebookState, NotebookTag,
            record_notebook_entry,
        },
        places::planar_distance,
        village::{VillageAreaKind, VillageState},
        world::{WandererPrototype, WorldMap, WorldShowcaseSpots},
    },
};

pub struct RegionPlugin;

impl Plugin for RegionPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                initialize_region_graph,
                update_transition_gate_state,
                update_transition_gate_visuals,
                update_region_outpost_visuals,
            )
                .chain()
                .run_if(in_state(InGameState::Running)),
        );
        app.add_systems(OnExit(AppScreen::InGame), cleanup_region_session);
    }
}

#[derive(Debug, Resource, Clone, PartialEq)]
pub struct RegionGraphState {
    pub regions: Vec<WorldRegion>,
    pub gates: Vec<TransitionGate>,
    pub current_region: RegionId,
    pub nearest_gate: Option<GateProximity>,
    pub discovered_gates: Vec<u64>,
    pub crossing: Option<GateCrossingState>,
    pub outpost: Option<RegionOutpostState>,
    pub milestones: RegionJourneyMilestones,
}

impl RegionGraphState {
    pub fn region(&self, id: RegionId) -> Option<&WorldRegion> {
        self.regions.iter().find(|region| region.id == id)
    }

    pub fn nearest_gate(&self) -> Option<&TransitionGate> {
        let id = self.nearest_gate?.gate_id;
        self.gates.iter().find(|gate| gate.id == id)
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct RegionId(pub u64);

#[derive(Debug, Clone, PartialEq)]
pub struct WorldRegion {
    pub id: RegionId,
    pub kind: RegionKind,
    pub seed: u64,
    pub center: Vec3,
    pub radius: f32,
    pub landmark: Option<RegionLandmarkKind>,
    pub boundary: RegionBoundaryKind,
    pub profile: RegionProfile,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum RegionKind {
    VillageCoast,
    GrasslandForest,
    MountainBoundary,
    Desert,
    FarSea,
}

impl RegionKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::VillageCoast => "村庄海岸",
            Self::GrasslandForest => "草原林地",
            Self::MountainBoundary => "山地边界",
            Self::Desert => "沙漠",
            Self::FarSea => "远海",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum RegionLandmarkKind {
    VillageHeadland,
    MistRiver,
    DesertPyramid,
    FarIslandLight,
}

impl RegionLandmarkKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::VillageHeadland => "村庄海岬",
            Self::MistRiver => "迷雾河",
            Self::DesertPyramid => "沙漠金字塔",
            Self::FarIslandLight => "远海灯火",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum RegionBoundaryKind {
    Shore,
    MistRiver,
    MountainPass,
    SandstormVeil,
    SeaHorizon,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RegionProfile {
    pub biome_bias: RegionBiomeBias,
    pub weather_bias: RegionWeatherBias,
    pub danger: f32,
    pub exploration_value: f32,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum RegionBiomeBias {
    CoastalMeadow,
    Ridge,
    DuneAndGobi,
    Ocean,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum RegionWeatherBias {
    ClearSeaMist,
    StrongWind,
    Sandstorm,
    OceanFog,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TransitionGate {
    pub id: u64,
    pub from: RegionId,
    pub to: RegionId,
    pub kind: TransitionGateKind,
    pub position: Vec3,
    pub radius: f32,
    pub condition: TransitionCondition,
    pub state: TransitionGateState,
    pub hint: &'static str,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum TransitionGateKind {
    MistRiverFord,
    MountainPass,
    Harbor,
}

impl TransitionGateKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::MistRiverFord => "迷雾旧渡口",
            Self::MountainPass => "山口",
            Self::Harbor => "港口",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum TransitionCondition {
    DreamAfterglowAndIntent,
    DiscoveredPass,
    SeaMethodKnown,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum TransitionGateState {
    Hidden,
    Hinted,
    Open,
    Crossed,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct GateProximity {
    pub gate_id: u64,
    pub distance: f32,
    pub open: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct GateCrossingState {
    pub gate_id: u64,
    pub gate_kind: TransitionGateKind,
    pub to_region: RegionId,
    pub elapsed_seconds: f32,
    pub duration_seconds: f32,
    pub start_position: Vec3,
    pub destination: Vec3,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RegionOutpostState {
    pub region: RegionId,
    pub center: Vec3,
    pub arrival_radius: f32,
    pub discovered: bool,
    pub recorded: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RegionJourneyMilestones {
    pub town_edge: RegionMilestoneState,
    pub loss_crossroad: RegionMilestoneState,
    pub desert_road: RegionMilestoneState,
}

#[derive(Debug, Clone, PartialEq)]
pub struct RegionMilestoneState {
    pub kind: RegionMilestoneKind,
    pub region: RegionId,
    pub center: Vec3,
    pub arrival_radius: f32,
    pub discovered: bool,
    pub recorded: bool,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum RegionMilestoneKind {
    TownEdge,
    LossCrossroad,
    DesertRoad,
}

impl RegionMilestoneKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::TownEdge => "集镇边缘",
            Self::LossCrossroad => "失物路口",
            Self::DesertRoad => "沙漠前路",
        }
    }

    fn notebook_title(self) -> &'static str {
        match self {
            Self::TownEdge => "听见集镇",
            Self::LossCrossroad => "失物路口",
            Self::DesertRoad => "沙漠前路",
        }
    }

    fn notebook_body(self) -> &'static str {
        match self {
            Self::TownEdge => {
                "摊棚后的路开始宽起来，车辙、盐袋和陌生口音混在风里。城镇还没露面，买卖和旅费已经有了重量。"
            }
            Self::LossCrossroad => {
                "路边有空箱、断绳和急促脚印。这里不像告诫，更像世界提前留下的一道裂缝。"
            }
            Self::DesertRoad => {
                "山风变干，地上的草线断在砂砾前。金字塔仍不可及，但沙漠已经进入脚下这条路。"
            }
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Self::TownEdge => "前方有摊声和车辙",
            Self::LossCrossroad => "路边空箱留下阴影",
            Self::DesertRoad => "干风带来沙的味道",
        }
    }
}

impl RegionJourneyMilestones {
    pub fn next_hint(
        &self,
        current_region: RegionId,
        outpost_discovered: bool,
    ) -> Option<&'static str> {
        if !outpost_discovered {
            return None;
        }
        [&self.town_edge, &self.loss_crossroad, &self.desert_road]
            .into_iter()
            .find(|milestone| milestone.region == current_region && !milestone.discovered)
            .map(|milestone| milestone.kind.hint())
    }
}

#[derive(Debug, Component)]
struct TransitionGateVisual {
    gate_id: u64,
    role: GateVisualRole,
    base_translation: Vec3,
    base_scale: Vec3,
}

#[derive(Debug, Component)]
struct RegionOutpostVisual {
    scope: RegionOutpostVisualScope,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
enum RegionOutpostVisualScope {
    Outpost,
    TownEdge,
    LossCrossroad,
    DesertRoad,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
enum GateVisualRole {
    MistBed,
    WaterRibbon,
    FordStone,
    Marker,
    SoftLight,
}

#[derive(Debug, Resource, Clone)]
struct RegionMaterials {
    mist: Handle<StandardMaterial>,
    stone: Handle<StandardMaterial>,
    water: Handle<StandardMaterial>,
    old_wood: Handle<StandardMaterial>,
}

const GATE_INTERACTION_RADIUS: f32 = 8.0;
const GATE_CROSSING_SECONDS: f32 = 1.8;

type GateUpdateResources<'w> = (
    Res<'w, ButtonInput<KeyCode>>,
    Option<ResMut<'w, RegionGraphState>>,
    Option<Res<'w, JourneyState>>,
    Option<Res<'w, IntentState>>,
    Option<Res<'w, PerceptionState>>,
    Option<ResMut<'w, NotebookState>>,
    Res<'w, Time>,
);

type RegionInitResources<'w> = (
    Option<Res<'w, WorldMap>>,
    Option<Res<'w, WorldShowcaseSpots>>,
    Option<Res<'w, VillageState>>,
    Option<Res<'w, RegionGraphState>>,
    Res<'w, ProceduralAssetMaterials>,
    Res<'w, crate::core::config::AppConfig>,
);

fn initialize_region_graph(
    mut commands: Commands,
    resources: RegionInitResources<'_>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    let (world_map, spots, village, existing, procedural_materials, config) = resources;
    if existing.is_some() {
        return;
    }
    let (Some(world_map), Some(spots), Some(village)) = (world_map, spots, village) else {
        return;
    };

    let graph = build_region_graph(&world_map, &spots, &village);
    let region_materials = RegionMaterials::new(&mut materials);
    spawn_gate_visuals(&mut commands, &mut meshes, &region_materials, &graph.gates);
    spawn_region_outpost(
        &mut commands,
        &mut meshes,
        &procedural_materials,
        &config.assets,
        &graph,
        &world_map,
    );

    tracing::info!(
        target: "dao_game::regions::graph",
        region_count = graph.regions.len(),
        gate_count = graph.gates.len(),
        current_region = graph
            .region(graph.current_region)
            .map(|region| region.kind.label()),
        "world region graph initialized"
    );

    commands.insert_resource(graph);
    commands.insert_resource(region_materials);
}

pub fn build_region_graph(
    world_map: &WorldMap,
    spots: &WorldShowcaseSpots,
    village: &VillageState,
) -> RegionGraphState {
    let village_region = RegionId(stable_region_id(
        world_map.seed_value(),
        RegionKind::VillageCoast,
    ));
    let grassland_region = RegionId(stable_region_id(
        world_map.seed_value(),
        RegionKind::GrasslandForest,
    ));
    let boundary_region = RegionId(stable_region_id(
        world_map.seed_value(),
        RegionKind::MountainBoundary,
    ));
    let desert_region = RegionId(stable_region_id(world_map.seed_value(), RegionKind::Desert));
    let sea_region = RegionId(stable_region_id(world_map.seed_value(), RegionKind::FarSea));

    let outer_path = village
        .area(VillageAreaKind::OuterPath)
        .map(|area| area.position)
        .unwrap_or(village.origin + Vec3::new(0.0, 0.0, -42.0));
    let shore = village
        .area(VillageAreaKind::Shore)
        .map(|area| area.position)
        .unwrap_or(village.origin + Vec3::new(0.0, 0.0, 38.0));
    let desert_center = ground_position(
        world_map,
        Vec3::new(
            spots.ridge.position.x + 220.0,
            0.0,
            spots.ridge.position.z - 280.0,
        ),
        0.0,
    );
    let mountain_center = ground_position(world_map, outer_path + Vec3::new(0.0, 0.0, -90.0), 0.0);
    let grassland_center = ground_position(world_map, spots.grove.position, 0.0);
    let sea_center = ground_position(world_map, shore + Vec3::new(0.0, 0.0, 140.0), 0.0);

    let regions = vec![
        WorldRegion {
            id: village_region,
            kind: RegionKind::VillageCoast,
            seed: derive_seed(world_map.seed_value(), 11),
            center: village.origin,
            radius: 160.0,
            landmark: Some(RegionLandmarkKind::VillageHeadland),
            boundary: RegionBoundaryKind::Shore,
            profile: RegionProfile {
                biome_bias: RegionBiomeBias::CoastalMeadow,
                weather_bias: RegionWeatherBias::ClearSeaMist,
                danger: 0.08,
                exploration_value: 0.35,
            },
        },
        WorldRegion {
            id: grassland_region,
            kind: RegionKind::GrasslandForest,
            seed: derive_seed(world_map.seed_value(), 17),
            center: grassland_center,
            radius: 190.0,
            landmark: None,
            boundary: RegionBoundaryKind::MountainPass,
            profile: RegionProfile {
                biome_bias: RegionBiomeBias::CoastalMeadow,
                weather_bias: RegionWeatherBias::ClearSeaMist,
                danger: 0.18,
                exploration_value: 0.48,
            },
        },
        WorldRegion {
            id: boundary_region,
            kind: RegionKind::MountainBoundary,
            seed: derive_seed(world_map.seed_value(), 23),
            center: mountain_center,
            radius: 190.0,
            landmark: Some(RegionLandmarkKind::MistRiver),
            boundary: RegionBoundaryKind::MistRiver,
            profile: RegionProfile {
                biome_bias: RegionBiomeBias::Ridge,
                weather_bias: RegionWeatherBias::StrongWind,
                danger: 0.36,
                exploration_value: 0.55,
            },
        },
        WorldRegion {
            id: desert_region,
            kind: RegionKind::Desert,
            seed: derive_seed(world_map.seed_value(), 37),
            center: desert_center,
            radius: 260.0,
            landmark: Some(RegionLandmarkKind::DesertPyramid),
            boundary: RegionBoundaryKind::SandstormVeil,
            profile: RegionProfile {
                biome_bias: RegionBiomeBias::DuneAndGobi,
                weather_bias: RegionWeatherBias::Sandstorm,
                danger: 0.72,
                exploration_value: 0.9,
            },
        },
        WorldRegion {
            id: sea_region,
            kind: RegionKind::FarSea,
            seed: derive_seed(world_map.seed_value(), 53),
            center: sea_center,
            radius: 220.0,
            landmark: Some(RegionLandmarkKind::FarIslandLight),
            boundary: RegionBoundaryKind::SeaHorizon,
            profile: RegionProfile {
                biome_bias: RegionBiomeBias::Ocean,
                weather_bias: RegionWeatherBias::OceanFog,
                danger: 0.48,
                exploration_value: 0.68,
            },
        },
    ];

    let mist_gate_position =
        ground_position(world_map, outer_path + Vec3::new(0.0, 0.0, -30.0), 0.1);
    let mountain_gate_position = ground_position(
        world_map,
        mountain_center + Vec3::new(34.0, 0.0, -22.0),
        0.1,
    );
    let harbor_position = ground_position(world_map, shore + Vec3::new(18.0, 0.0, 10.0), 0.1);
    let grove_pass_position =
        ground_position(world_map, village.origin + Vec3::new(42.0, 0.0, -28.0), 0.1);
    let gates = vec![
        TransitionGate {
            id: stable_gate_id(
                village_region,
                boundary_region,
                TransitionGateKind::MistRiverFord,
            ),
            from: village_region,
            to: boundary_region,
            kind: TransitionGateKind::MistRiverFord,
            position: mist_gate_position,
            radius: 22.0,
            condition: TransitionCondition::DreamAfterglowAndIntent,
            state: TransitionGateState::Hidden,
            hint: "雾里有一条旧水声，像是曾经有人从这里离开。",
        },
        TransitionGate {
            id: stable_gate_id(
                village_region,
                grassland_region,
                TransitionGateKind::MountainPass,
            ),
            from: village_region,
            to: grassland_region,
            kind: TransitionGateKind::MountainPass,
            position: grove_pass_position,
            radius: 18.0,
            condition: TransitionCondition::DiscoveredPass,
            state: TransitionGateState::Hidden,
            hint: "草坡后的林地没有路牌，只有风从两棵树之间穿过。",
        },
        TransitionGate {
            id: stable_gate_id(
                boundary_region,
                desert_region,
                TransitionGateKind::MountainPass,
            ),
            from: boundary_region,
            to: desert_region,
            kind: TransitionGateKind::MountainPass,
            position: mountain_gate_position,
            radius: 26.0,
            condition: TransitionCondition::DiscoveredPass,
            state: TransitionGateState::Hidden,
            hint: "山风从一道狭缝里吹来，带着干燥的沙味。",
        },
        TransitionGate {
            id: stable_gate_id(village_region, sea_region, TransitionGateKind::Harbor),
            from: village_region,
            to: sea_region,
            kind: TransitionGateKind::Harbor,
            position: harbor_position,
            radius: 20.0,
            condition: TransitionCondition::SeaMethodKnown,
            state: TransitionGateState::Hidden,
            hint: "海面远处有一点灯，只有雾淡时才像是真的。",
        },
    ];

    RegionGraphState {
        regions,
        gates,
        current_region: village_region,
        nearest_gate: None,
        discovered_gates: Vec::new(),
        crossing: None,
        outpost: build_region_outpost(world_map, boundary_region, mountain_center),
        milestones: build_region_milestones(
            world_map,
            boundary_region,
            desert_region,
            mountain_center,
        ),
    }
}

fn build_region_outpost(
    world_map: &WorldMap,
    boundary_region: RegionId,
    mountain_center: Vec3,
) -> Option<RegionOutpostState> {
    let center = ground_position(world_map, mountain_center + Vec3::new(18.0, 0.0, -8.0), 0.0);
    Some(RegionOutpostState {
        region: boundary_region,
        center,
        arrival_radius: 20.0,
        discovered: false,
        recorded: false,
    })
}

fn build_region_milestones(
    world_map: &WorldMap,
    boundary_region: RegionId,
    desert_region: RegionId,
    mountain_center: Vec3,
) -> RegionJourneyMilestones {
    RegionJourneyMilestones {
        town_edge: RegionMilestoneState {
            kind: RegionMilestoneKind::TownEdge,
            region: boundary_region,
            center: ground_position(
                world_map,
                mountain_center + Vec3::new(48.0, 0.0, -26.0),
                0.0,
            ),
            arrival_radius: 24.0,
            discovered: false,
            recorded: false,
        },
        loss_crossroad: RegionMilestoneState {
            kind: RegionMilestoneKind::LossCrossroad,
            region: boundary_region,
            center: ground_position(
                world_map,
                mountain_center + Vec3::new(92.0, 0.0, -66.0),
                0.0,
            ),
            arrival_radius: 22.0,
            discovered: false,
            recorded: false,
        },
        desert_road: RegionMilestoneState {
            kind: RegionMilestoneKind::DesertRoad,
            region: desert_region,
            center: ground_position(
                world_map,
                mountain_center + Vec3::new(138.0, 0.0, -118.0),
                0.0,
            ),
            arrival_radius: 28.0,
            discovered: false,
            recorded: false,
        },
    }
}

fn update_transition_gate_state(
    resources: GateUpdateResources<'_>,
    mut player_query: Query<&mut Transform, With<WandererPrototype>>,
    performance: Option<ResMut<FramePerformance>>,
) {
    let (keys, graph, journey, intent, perception, mut notebook, time) = resources;
    let Some(mut graph) = graph else {
        return;
    };
    let Some(player_transform) = player_query.iter().next() else {
        return;
    };
    let started_at = Instant::now();
    if let Some(crossing) = graph.crossing.as_mut() {
        crossing.elapsed_seconds += time.delta_secs();
        let t = (crossing.elapsed_seconds / crossing.duration_seconds.max(0.01)).clamp(0.0, 1.0);
        let eased = t * t * (3.0 - 2.0 * t);
        let lifted = crossing.start_position.lerp(crossing.destination, eased)
            + Vec3::Y * (0.6 * (1.0 - (2.0 * t - 1.0).abs()));
        if let Some(mut player_transform) = player_query.iter_mut().next() {
            player_transform.translation = lifted;
        }
        if t >= 1.0 {
            let gate_id = crossing.gate_id;
            let gate_kind = crossing.gate_kind;
            let to_region = crossing.to_region;
            graph.current_region = to_region;
            if let Some(outpost) = graph.outpost.as_mut()
                && outpost.region == to_region
            {
                outpost.discovered = true;
            }
            if let Some(gate) = graph.gates.iter_mut().find(|gate| gate.id == gate_id) {
                gate.state = TransitionGateState::Crossed;
            }
            let region_label = graph
                .region(graph.current_region)
                .map(|region| region.kind.label());
            let _ = record_notebook_entry(
                notebook.as_deref_mut(),
                NotebookRecord {
                    kind: NotebookEntryKind::Place,
                    at_seconds: time.elapsed_secs(),
                    location: region_label.map(str::to_string),
                    source: NotebookSource::PlaceArrival,
                    title: format!("穿过{}", gate_kind.label()),
                    body:
                        "你真正从旧边界走了过去。雾、水声和脚下的地面一起变了，村庄已经留在身后。"
                            .to_string(),
                    tags: vec![NotebookTag::Omen, NotebookTag::Memory],
                },
            );
            tracing::info!(
                target: "dao_game::regions::transition",
                gate_id,
                gate = gate_kind.label(),
                to_region = region_label,
                "natural boundary crossed"
            );
            graph.crossing = None;
        }
        if let Some(mut performance) = performance {
            performance.record_phase_duration(PerformancePhase::Regions, started_at.elapsed());
        }
        return;
    }

    let player_position = player_transform.translation;
    let current_region = graph.current_region;
    let should_record_outpost = graph.outpost.as_ref().is_some_and(|outpost| {
        outpost.discovered
            && !outpost.recorded
            && current_region == outpost.region
            && planar_distance(player_position, outpost.center) <= outpost.arrival_radius
    });
    if should_record_outpost {
        if let Some(outpost) = graph.outpost.as_mut() {
            outpost.recorded = true;
        }
        let _ = record_notebook_entry(
            notebook.as_deref_mut(),
            NotebookRecord {
                kind: NotebookEntryKind::Place,
                at_seconds: time.elapsed_secs(),
                location: Some("对岸前哨".to_string()),
                source: NotebookSource::PlaceArrival,
                title: "到达对岸前哨".to_string(),
                body: "雾后有几间临时屋舍、一处摊棚和压平的歇脚地。这里还不是城镇，但已经不再是出发时的村庄。".to_string(),
                tags: vec![NotebookTag::Memory, NotebookTag::Omen],
            },
        );
    }
    update_region_milestones(
        &mut graph,
        player_position,
        time.elapsed_secs(),
        notebook.as_deref_mut(),
    );
    let mut nearest = None;
    let mut changed_gate_ids = Vec::new();
    let current_region = graph.current_region;

    for gate in &mut graph.gates {
        if gate.from != current_region {
            continue;
        }
        let distance = planar_distance(player_position, gate.position);
        let opened =
            transition_condition_met(gate.condition, journey.as_deref(), intent.as_deref());
        let next_state = if distance <= gate.radius && opened {
            TransitionGateState::Open
        } else if distance <= gate.radius
            || (perception
                .as_deref()
                .is_some_and(|perception| perception.active)
                && distance <= gate.radius * 1.8)
        {
            TransitionGateState::Hinted
        } else {
            TransitionGateState::Hidden
        };
        if gate.state != next_state {
            tracing::info!(
                target: "dao_game::regions::gate",
                gate_id = gate.id,
                gate = gate.kind.label(),
                from = ?gate.from,
                to = ?gate.to,
                distance,
                state = ?next_state,
                "transition gate state changed"
            );
            gate.state = next_state;
            changed_gate_ids.push(gate.id);
        }
        if distance <= gate.radius * 1.8 {
            let candidate = GateProximity {
                gate_id: gate.id,
                distance,
                open: opened,
            };
            if nearest.is_none_or(|current: GateProximity| candidate.distance < current.distance) {
                nearest = Some(candidate);
            }
        }
    }

    for gate_id in changed_gate_ids {
        if graph.discovered_gates.contains(&gate_id) {
            continue;
        }
        let Some(gate) = graph.gates.iter().find(|gate| gate.id == gate_id) else {
            continue;
        };
        let should_record = matches!(
            gate.state,
            TransitionGateState::Hinted | TransitionGateState::Open
        );
        let gate_kind = gate.kind;
        let hint = gate.hint;
        if should_record {
            graph.discovered_gates.push(gate_id);
            let _ = record_notebook_entry(
                notebook.as_deref_mut(),
                NotebookRecord {
                    kind: NotebookEntryKind::Observation,
                    at_seconds: time.elapsed_secs(),
                    location: Some(gate_kind.label().to_string()),
                    source: NotebookSource::Observation,
                    title: gate_kind.label().to_string(),
                    body: hint.to_string(),
                    tags: vec![NotebookTag::Omen, NotebookTag::Memory],
                },
            );
        }
    }

    graph.nearest_gate = nearest;
    if keys.just_pressed(KeyCode::KeyG)
        && let Some(proximity) = nearest
        && proximity.open
        && proximity.distance <= GATE_INTERACTION_RADIUS
    {
        let Some(gate_index) = graph
            .gates
            .iter()
            .position(|gate| gate.id == proximity.gate_id)
        else {
            return;
        };
        let gate_id = graph.gates[gate_index].id;
        let gate_kind = graph.gates[gate_index].kind;
        let to_region = graph.gates[gate_index].to;
        let destination = graph
            .region(to_region)
            .map(|region| region.center + Vec3::Y * 1.2);
        if let Some(destination) = destination {
            graph.crossing = Some(GateCrossingState {
                gate_id,
                gate_kind,
                to_region,
                elapsed_seconds: 0.0,
                duration_seconds: GATE_CROSSING_SECONDS,
                start_position: player_position,
                destination,
            });
            let _ = record_notebook_entry(
                notebook.as_deref_mut(),
                NotebookRecord {
                    kind: NotebookEntryKind::Observation,
                    at_seconds: time.elapsed_secs(),
                    location: Some(gate_kind.label().to_string()),
                    source: NotebookSource::Observation,
                    title: format!("开始通过{}", gate_kind.label()),
                    body: "你没有消失在一扇门里，而是真的向着雾和旧水声走了过去。".to_string(),
                    tags: vec![NotebookTag::Omen, NotebookTag::Memory],
                },
            );
            tracing::info!(
                target: "dao_game::regions::transition",
                gate_id,
                gate = gate_kind.label(),
                "natural boundary crossing started"
            );
        }
    }
    if let Some(mut performance) = performance {
        performance.record_phase_duration(PerformancePhase::Regions, started_at.elapsed());
    }
}

fn update_region_milestones(
    graph: &mut RegionGraphState,
    player_position: Vec3,
    elapsed_seconds: f32,
    mut notebook: Option<&mut NotebookState>,
) {
    let current_region = graph.current_region;
    let mut reached = Vec::new();
    if mark_milestone_reached(
        &mut graph.milestones.town_edge,
        current_region,
        player_position,
    ) {
        reached.push(RegionMilestoneKind::TownEdge);
    }
    if mark_milestone_reached(
        &mut graph.milestones.loss_crossroad,
        current_region,
        player_position,
    ) {
        reached.push(RegionMilestoneKind::LossCrossroad);
    }
    if mark_milestone_reached(
        &mut graph.milestones.desert_road,
        current_region,
        player_position,
    ) {
        reached.push(RegionMilestoneKind::DesertRoad);
    }

    let region_label = graph
        .region(current_region)
        .map(|region| region.kind.label().to_string());
    for kind in reached {
        let _ = record_notebook_entry(
            notebook.as_deref_mut(),
            NotebookRecord {
                kind: match kind {
                    RegionMilestoneKind::TownEdge => NotebookEntryKind::Observation,
                    RegionMilestoneKind::LossCrossroad | RegionMilestoneKind::DesertRoad => {
                        NotebookEntryKind::JourneyEcho
                    }
                },
                at_seconds: elapsed_seconds,
                location: region_label
                    .clone()
                    .or_else(|| Some(kind.label().to_string())),
                source: NotebookSource::Journey,
                title: kind.notebook_title().to_string(),
                body: kind.notebook_body().to_string(),
                tags: milestone_notebook_tags(kind),
            },
        );
        tracing::info!(
            target: "dao_game::regions::milestone",
            milestone = kind.label(),
            region = region_label.as_deref(),
            "region journey milestone reached"
        );
    }
}

fn mark_milestone_reached(
    milestone: &mut RegionMilestoneState,
    current_region: RegionId,
    player_position: Vec3,
) -> bool {
    if milestone.recorded || milestone.region != current_region {
        return false;
    }
    if milestone.discovered
        || planar_distance(player_position, milestone.center) <= milestone.arrival_radius
    {
        milestone.discovered = true;
        milestone.recorded = true;
        return true;
    }
    false
}

fn milestone_notebook_tags(kind: RegionMilestoneKind) -> Vec<NotebookTag> {
    match kind {
        RegionMilestoneKind::TownEdge => vec![NotebookTag::Memory, NotebookTag::Merchant],
        RegionMilestoneKind::LossCrossroad => vec![NotebookTag::Memory, NotebookTag::Omen],
        RegionMilestoneKind::DesertRoad => {
            vec![NotebookTag::Memory, NotebookTag::Omen, NotebookTag::Desert]
        }
    }
}

fn spawn_region_outpost(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &ProceduralAssetMaterials,
    asset_config: &crate::core::config::AssetConfig,
    graph: &RegionGraphState,
    _world_map: &WorldMap,
) {
    let Some(outpost) = graph.outpost.as_ref() else {
        return;
    };
    let _root = commands
        .spawn((
            Name::new("BoundaryOutpost"),
            DespawnOnExit(AppScreen::InGame),
            Transform::from_translation(outpost.center),
            Visibility::Hidden,
            RegionOutpostVisual {
                scope: RegionOutpostVisualScope::Outpost,
            },
        ))
        .id();

    let house_offsets = [Vec3::new(-6.5, 0.0, 2.0), Vec3::new(3.4, 0.0, -1.8)];
    for (index, offset) in house_offsets.into_iter().enumerate() {
        let entity = spawn_procedural_asset_entity(
            commands,
            meshes,
            materials,
            asset_config,
            ProceduralSpawnRequest::new(
                ProceduralAssetKind::VillageHouse,
                8_000 + index as u64,
                "BoundaryOutpostHouse",
                Transform::from_translation(outpost.center + offset)
                    .with_scale(Vec3::new(0.78, 0.82, 0.74))
                    .with_rotation(Quat::from_rotation_y(index as f32 * 0.3)),
            )
            .with_lod(ProceduralAssetLod::Near),
        );
        commands.entity(entity).insert((
            Visibility::Hidden,
            RegionOutpostVisual {
                scope: RegionOutpostVisualScope::Outpost,
            },
        ));
    }

    let stall = spawn_procedural_asset_entity(
        commands,
        meshes,
        materials,
        asset_config,
        ProceduralSpawnRequest::new(
            ProceduralAssetKind::MarketStall,
            8_111,
            "BoundaryOutpostStall",
            Transform::from_translation(outpost.center + Vec3::new(-1.8, 0.0, 5.4))
                .with_scale(Vec3::splat(0.86))
                .with_rotation(Quat::from_rotation_y(-0.28)),
        )
        .with_lod(ProceduralAssetLod::Near),
    );
    commands.entity(stall).insert((
        Visibility::Hidden,
        RegionOutpostVisual {
            scope: RegionOutpostVisualScope::Outpost,
        },
    ));

    for index in 0..6 {
        let entity = spawn_procedural_asset_entity(
            commands,
            meshes,
            materials,
            asset_config,
            ProceduralSpawnRequest::new(
                ProceduralAssetKind::PathStone,
                8_200 + index as u64,
                "BoundaryOutpostPath",
                Transform::from_translation(
                    outpost.center
                        + Vec3::new((index as f32 - 2.5) * 1.6, 0.0, 10.0 + index as f32 * -1.4),
                )
                .with_scale(Vec3::splat(0.82)),
            )
            .with_lod(ProceduralAssetLod::Near),
        );
        commands.entity(entity).insert((
            Visibility::Hidden,
            RegionOutpostVisual {
                scope: RegionOutpostVisualScope::Outpost,
            },
        ));
    }

    let marker = spawn_procedural_asset_entity(
        commands,
        meshes,
        materials,
        asset_config,
        ProceduralSpawnRequest::new(
            ProceduralAssetKind::HeadlandMarker,
            8_301,
            "BoundaryOutpostMarker",
            Transform::from_translation(outpost.center + Vec3::new(8.0, 0.0, -6.5))
                .with_scale(Vec3::new(0.72, 0.88, 0.72)),
        )
        .with_lod(ProceduralAssetLod::Near),
    );
    commands.entity(marker).insert((
        Visibility::Hidden,
        RegionOutpostVisual {
            scope: RegionOutpostVisualScope::Outpost,
        },
    ));

    spawn_region_milestone_visuals(commands, meshes, materials, asset_config, graph);
}

fn spawn_region_milestone_visuals(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &ProceduralAssetMaterials,
    asset_config: &crate::core::config::AssetConfig,
    graph: &RegionGraphState,
) {
    let town = &graph.milestones.town_edge;
    let stall = spawn_procedural_asset_entity(
        commands,
        meshes,
        materials,
        asset_config,
        ProceduralSpawnRequest::new(
            ProceduralAssetKind::MarketStall,
            8_420,
            "TownEdgeTradeStall",
            Transform::from_translation(town.center + Vec3::new(-2.5, 0.0, 1.6))
                .with_scale(Vec3::splat(0.9))
                .with_rotation(Quat::from_rotation_y(0.42)),
        )
        .with_lod(ProceduralAssetLod::Near),
    );
    commands.entity(stall).insert((
        Visibility::Hidden,
        RegionOutpostVisual {
            scope: RegionOutpostVisualScope::TownEdge,
        },
    ));
    for index in 0..5 {
        let entity = spawn_procedural_asset_entity(
            commands,
            meshes,
            materials,
            asset_config,
            ProceduralSpawnRequest::new(
                ProceduralAssetKind::PathStone,
                8_430 + index as u64,
                "TownEdgeCartRuts",
                Transform::from_translation(
                    town.center + Vec3::new(index as f32 * 2.0 - 4.0, 0.0, -3.0),
                )
                .with_scale(Vec3::new(0.9, 0.5, 1.25)),
            )
            .with_lod(ProceduralAssetLod::Near),
        );
        commands.entity(entity).insert((
            Visibility::Hidden,
            RegionOutpostVisual {
                scope: RegionOutpostVisualScope::TownEdge,
            },
        ));
    }

    let loss = &graph.milestones.loss_crossroad;
    let marker = spawn_procedural_asset_entity(
        commands,
        meshes,
        materials,
        asset_config,
        ProceduralSpawnRequest::new(
            ProceduralAssetKind::HeadlandMarker,
            8_520,
            "LossCrossroadMarker",
            Transform::from_translation(loss.center + Vec3::new(1.5, 0.0, -1.8))
                .with_scale(Vec3::new(0.58, 0.7, 0.58))
                .with_rotation(Quat::from_rotation_y(-0.55)),
        )
        .with_lod(ProceduralAssetLod::Near),
    );
    commands.entity(marker).insert((
        Visibility::Hidden,
        RegionOutpostVisual {
            scope: RegionOutpostVisualScope::LossCrossroad,
        },
    ));
    for index in 0..3 {
        let entity = spawn_procedural_asset_entity(
            commands,
            meshes,
            materials,
            asset_config,
            ProceduralSpawnRequest::new(
                ProceduralAssetKind::PathStone,
                8_530 + index as u64,
                "LossCrossroadBrokenCrate",
                Transform::from_translation(
                    loss.center + Vec3::new(index as f32 * 1.7 - 1.7, 0.0, 2.4),
                )
                .with_scale(Vec3::new(0.7, 0.42, 0.58))
                .with_rotation(Quat::from_rotation_y(index as f32 * 0.7)),
            )
            .with_lod(ProceduralAssetLod::Near),
        );
        commands.entity(entity).insert((
            Visibility::Hidden,
            RegionOutpostVisual {
                scope: RegionOutpostVisualScope::LossCrossroad,
            },
        ));
    }

    let desert = &graph.milestones.desert_road;
    let relic = spawn_procedural_asset_entity(
        commands,
        meshes,
        materials,
        asset_config,
        ProceduralSpawnRequest::new(
            ProceduralAssetKind::DesertRelic,
            8_620,
            "DesertRoadRelic",
            Transform::from_translation(desert.center + Vec3::new(0.0, 0.0, -2.5))
                .with_scale(Vec3::new(0.74, 0.74, 0.74))
                .with_rotation(Quat::from_rotation_y(0.18)),
        )
        .with_lod(ProceduralAssetLod::Near),
    );
    commands.entity(relic).insert((
        Visibility::Hidden,
        RegionOutpostVisual {
            scope: RegionOutpostVisualScope::DesertRoad,
        },
    ));
}

fn update_transition_gate_visuals(
    graph: Option<Res<RegionGraphState>>,
    environment: Option<Res<EnvironmentSnapshot>>,
    wind: Option<Res<WindField>>,
    journey: Option<Res<JourneyState>>,
    mut query: Query<
        (&TransitionGateVisual, &mut Visibility, &mut Transform),
        Without<WandererPrototype>,
    >,
    performance: Option<ResMut<FramePerformance>>,
) {
    let Some(graph) = graph else {
        return;
    };
    let started_at = Instant::now();
    let fog_density = environment
        .as_deref()
        .map(|snapshot| snapshot.fog_density)
        .unwrap_or(0.0);
    let boundary_glow = environment
        .as_deref()
        .map(|snapshot| snapshot.boundary_glow)
        .unwrap_or(0.0);
    let horizon_tension = environment
        .as_deref()
        .map(|snapshot| snapshot.horizon_tension)
        .unwrap_or(0.0);
    let wind_gust = wind.as_deref().map(|field| field.gust).unwrap_or(0.0);
    let dream_bias = journey
        .as_deref()
        .map(|state| match state.story_stage {
            crate::game::journey::StoryArcStage::VillageAwakening => 0.0,
            crate::game::journey::StoryArcStage::VillageLife => 0.08,
            crate::game::journey::StoryArcStage::DreamApproaching => 0.16,
            crate::game::journey::StoryArcStage::Dreaming => 0.24,
            crate::game::journey::StoryArcStage::DreamAfterglow => 0.34,
            crate::game::journey::StoryArcStage::BoundaryCrossing => 0.78,
            crate::game::journey::StoryArcStage::FarBankOutpost => 0.92,
            crate::game::journey::StoryArcStage::TownPreparation => 0.72,
            crate::game::journey::StoryArcStage::FirstLoss => 0.72,
            crate::game::journey::StoryArcStage::DesertDeparture => 0.88,
        })
        .unwrap_or(0.0);
    for (visual, mut visibility, mut transform) in &mut query {
        let Some(gate) = graph.gates.iter().find(|gate| gate.id == visual.gate_id) else {
            continue;
        };
        let visible = matches!(
            gate.state,
            TransitionGateState::Hinted | TransitionGateState::Open | TransitionGateState::Crossed
        ) || graph
            .crossing
            .as_ref()
            .is_some_and(|crossing| crossing.gate_id == gate.id);
        *visibility = if visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        let pulse = match gate.state {
            TransitionGateState::Open => 1.0,
            TransitionGateState::Crossed => 0.75,
            TransitionGateState::Hinted => 0.42,
            TransitionGateState::Hidden => 0.0,
        };
        let crossing_boost = if graph
            .crossing
            .as_ref()
            .is_some_and(|crossing| crossing.gate_id == gate.id)
        {
            0.28
        } else {
            0.0
        };
        let atmosphere_gain =
            (fog_density * 0.2 + boundary_glow * 0.16 + wind_gust * 0.14).clamp(0.0, 0.28);
        let gate_scale = 0.85 + pulse * 0.35 + crossing_boost + atmosphere_gain + dream_bias * 0.08;
        transform.scale = visual.base_scale * gate_scale;
        if visual.role == GateVisualRole::WaterRibbon {
            transform.translation.y =
                visual.base_translation.y + 0.04 + horizon_tension * 0.06 + crossing_boost * 0.18;
        } else {
            transform.translation = visual.base_translation;
        }
    }
    if let Some(mut performance) = performance {
        performance.record_phase_duration(PerformancePhase::Regions, started_at.elapsed());
    }
}

fn update_region_outpost_visuals(
    graph: Option<Res<RegionGraphState>>,
    mut query: Query<(&RegionOutpostVisual, &mut Visibility), Without<WandererPrototype>>,
    performance: Option<ResMut<FramePerformance>>,
) {
    let Some(graph) = graph else {
        return;
    };
    let started_at = Instant::now();
    let outpost_discovered = graph
        .outpost
        .as_ref()
        .is_some_and(|outpost| outpost.discovered);
    let town_visible = graph.milestones.town_edge.discovered
        || graph.current_region == graph.milestones.town_edge.region;
    let loss_visible = graph.milestones.loss_crossroad.discovered
        || graph.current_region == graph.milestones.loss_crossroad.region;
    let desert_visible = graph.milestones.desert_road.discovered
        || graph.current_region == graph.milestones.desert_road.region;

    for (visual, mut visibility) in &mut query {
        let visible = match visual.scope {
            RegionOutpostVisualScope::Outpost => outpost_discovered,
            RegionOutpostVisualScope::TownEdge => outpost_discovered && town_visible,
            RegionOutpostVisualScope::LossCrossroad => outpost_discovered && loss_visible,
            RegionOutpostVisualScope::DesertRoad => outpost_discovered && desert_visible,
        };
        *visibility = if visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    if let Some(mut performance) = performance {
        performance.record_phase_duration(PerformancePhase::Regions, started_at.elapsed());
    }
}

fn spawn_gate_visuals(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &RegionMaterials,
    gates: &[TransitionGate],
) {
    for gate in gates {
        let material = match gate.kind {
            TransitionGateKind::MistRiverFord | TransitionGateKind::Harbor => {
                materials.mist.clone()
            }
            TransitionGateKind::MountainPass => materials.stone.clone(),
        };
        commands.spawn((
            Name::new(format!("TransitionGate:{}", gate.kind.label())),
            DespawnOnExit(AppScreen::InGame),
            Mesh3d(meshes.add(Mesh::from(Cylinder::new(gate.radius * 0.45, 0.12)))),
            MeshMaterial3d(material),
            Transform::from_translation(gate.position + Vec3::Y * 0.08),
            Visibility::Hidden,
            TransitionGateVisual {
                gate_id: gate.id,
                role: GateVisualRole::MistBed,
                base_translation: gate.position + Vec3::Y * 0.08,
                base_scale: Vec3::ONE,
            },
        ));
        if gate.kind == TransitionGateKind::MistRiverFord {
            spawn_mist_river_ford_visuals(commands, meshes, materials, gate);
        }
        if gate.kind == TransitionGateKind::MountainPass {
            for offset in [-3.8, 3.8] {
                commands.spawn((
                    Name::new("MountainPassMarker"),
                    DespawnOnExit(AppScreen::InGame),
                    Mesh3d(meshes.add(Mesh::from(Cuboid::new(1.1, 5.6, 1.1)))),
                    MeshMaterial3d(materials.stone.clone()),
                    Transform::from_translation(gate.position + Vec3::new(offset, 2.8, 0.0)),
                    Visibility::Hidden,
                    TransitionGateVisual {
                        gate_id: gate.id,
                        role: GateVisualRole::Marker,
                        base_translation: gate.position + Vec3::new(offset, 2.8, 0.0),
                        base_scale: Vec3::ONE,
                    },
                ));
            }
        } else {
            commands.spawn((
                Name::new("GateSoftLight"),
                DespawnOnExit(AppScreen::InGame),
                PointLight {
                    intensity: 42_000.0,
                    range: gate.radius * 1.15,
                    radius: 1.8,
                    shadows_enabled: false,
                    color: Color::srgb(0.72, 0.82, 0.86),
                    ..Default::default()
                },
                Transform::from_translation(gate.position + Vec3::Y * 1.6),
                Visibility::Hidden,
                TransitionGateVisual {
                    gate_id: gate.id,
                    role: GateVisualRole::SoftLight,
                    base_translation: gate.position + Vec3::Y * 1.6,
                    base_scale: Vec3::ONE,
                },
            ));
        }
    }
}

fn spawn_mist_river_ford_visuals(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &RegionMaterials,
    gate: &TransitionGate,
) {
    commands.spawn((
        Name::new("MistRiverWaterRibbon"),
        DespawnOnExit(AppScreen::InGame),
        Mesh3d(meshes.add(Mesh::from(Plane3d::default()))),
        MeshMaterial3d(materials.water.clone()),
        Transform::from_translation(gate.position + Vec3::new(0.0, 0.045, 0.0))
            .with_rotation(Quat::from_rotation_y(0.18))
            .with_scale(Vec3::new(gate.radius * 1.35, 1.0, gate.radius * 0.28)),
        Visibility::Hidden,
        TransitionGateVisual {
            gate_id: gate.id,
            role: GateVisualRole::WaterRibbon,
            base_translation: gate.position + Vec3::new(0.0, 0.045, 0.0),
            base_scale: Vec3::new(gate.radius * 1.35, 1.0, gate.radius * 0.28),
        },
    ));

    for (index, offset) in [
        Vec3::new(-5.6, 0.16, -3.8),
        Vec3::new(-2.1, 0.2, -1.4),
        Vec3::new(1.6, 0.18, 0.9),
        Vec3::new(5.0, 0.2, 3.1),
    ]
    .into_iter()
    .enumerate()
    {
        commands.spawn((
            Name::new("MistRiverFordStone"),
            DespawnOnExit(AppScreen::InGame),
            Mesh3d(meshes.add(Mesh::from(Cuboid::new(2.5, 0.28, 1.35)))),
            MeshMaterial3d(materials.stone.clone()),
            Transform::from_translation(gate.position + offset)
                .with_rotation(Quat::from_rotation_y(index as f32 * 0.34 - 0.42)),
            Visibility::Hidden,
            TransitionGateVisual {
                gate_id: gate.id,
                role: GateVisualRole::FordStone,
                base_translation: gate.position + offset,
                base_scale: Vec3::ONE,
            },
        ));
    }

    for (index, offset) in [Vec3::new(-7.2, 1.05, 4.6), Vec3::new(7.4, 1.0, -4.4)]
        .into_iter()
        .enumerate()
    {
        commands.spawn((
            Name::new("MistRiverOldPost"),
            DespawnOnExit(AppScreen::InGame),
            Mesh3d(meshes.add(Mesh::from(Cuboid::new(0.46, 2.1, 0.46)))),
            MeshMaterial3d(materials.old_wood.clone()),
            Transform::from_translation(gate.position + offset)
                .with_rotation(Quat::from_rotation_z(if index == 0 { 0.08 } else { -0.1 })),
            Visibility::Hidden,
            TransitionGateVisual {
                gate_id: gate.id,
                role: GateVisualRole::Marker,
                base_translation: gate.position + offset,
                base_scale: Vec3::ONE,
            },
        ));
    }
}

fn transition_condition_met(
    condition: TransitionCondition,
    journey: Option<&JourneyState>,
    intent: Option<&IntentState>,
) -> bool {
    match condition {
        TransitionCondition::DreamAfterglowAndIntent => {
            let dream_ready = journey.is_some_and(|journey| {
                journey.dream.phase == DreamPhase::Afterglow || journey.dream.echo_strength > 0.35
            });
            let intent_ready = intent.is_some_and(|intent| {
                intent.strength(IntentKind::BeyondVillage) >= 0.22
                    || intent.strength(IntentKind::DreamLandmark) >= 0.28
            });
            dream_ready && intent_ready
        }
        TransitionCondition::DiscoveredPass => journey.is_some_and(|journey| {
            journey.dream.phase == DreamPhase::Afterglow && journey.dream.echo_strength > 0.18
        }),
        TransitionCondition::SeaMethodKnown => intent.is_some_and(|intent| {
            intent.strength(IntentKind::Sea) >= 0.42
                && intent.strength(IntentKind::BeyondVillage) >= 0.18
        }),
    }
}

fn ground_position(world_map: &WorldMap, position: Vec3, y_offset: f32) -> Vec3 {
    let height = world_map
        .sample_height(position.x, position.z)
        .unwrap_or(position.y)
        .max(world_map.water_level() + 0.05);
    Vec3::new(position.x, height + y_offset, position.z)
}

impl RegionMaterials {
    fn new(materials: &mut Assets<StandardMaterial>) -> Self {
        Self {
            mist: materials.add(StandardMaterial {
                base_color: Color::srgba(0.78, 0.84, 0.88, 0.46),
                alpha_mode: AlphaMode::Blend,
                base_color_texture: None,
                emissive: LinearRgba::rgb(0.08, 0.09, 0.1),
                perceptual_roughness: 1.0,
                ..Default::default()
            }),
            stone: materials.add(StandardMaterial {
                base_color: Color::srgb(0.42, 0.42, 0.4),
                perceptual_roughness: 0.98,
                ..Default::default()
            }),
            water: materials.add(StandardMaterial {
                base_color: Color::srgba(0.2, 0.44, 0.54, 0.58),
                alpha_mode: AlphaMode::Blend,
                emissive: LinearRgba::rgb(0.015, 0.04, 0.052),
                perceptual_roughness: 0.28,
                metallic: 0.02,
                ..Default::default()
            }),
            old_wood: materials.add(StandardMaterial {
                base_color: Color::srgb(0.25, 0.19, 0.13),
                perceptual_roughness: 0.96,
                ..Default::default()
            }),
        }
    }
}

pub fn region_distance_score(region: &WorldRegion, position: Vec3) -> f32 {
    (1.0 - planar_distance(region.center, position) / region.radius.max(1.0)).clamp(0.0, 1.0)
}

fn stable_region_id(seed: u64, kind: RegionKind) -> u64 {
    derive_seed(seed, kind as u64 + 101)
}

fn stable_gate_id(from: RegionId, to: RegionId, kind: TransitionGateKind) -> u64 {
    derive_seed(from.0 ^ to.0, kind as u64 + 701)
}

fn derive_seed(seed: u64, salt: u64) -> u64 {
    let mut value = seed
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(salt.wrapping_mul(0xBF58_476D_1CE4_E5B9));
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn cleanup_region_session(mut commands: Commands) {
    commands.remove_resource::<RegionGraphState>();
    commands.remove_resource::<RegionMaterials>();
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use bevy::prelude::Vec3;

    use crate::{
        core::config::{
            AppConfig, AssetConfig, CameraConfig, DesertConfig, EcologyConfig, EnvironmentConfig,
            PlayerConfig, PresentationConfig, QualityConfig, SignConfig, WorldConfig,
        },
        game::{
            regions::{
                RegionKind, TransitionCondition, TransitionGateKind, build_region_graph,
                region_distance_score, transition_condition_met,
            },
            village::{VillageArea, VillageAreaKind, VillageState},
            world::{BiomeKind, ShowcaseSpot, WorldGridCoord, WorldMap, WorldShowcaseSpots},
        },
    };

    #[test]
    fn region_graph_contains_core_regions_and_gates() {
        let config = test_config();
        let world_map = WorldMap::new_for_testing(config.world.seed, &config);
        let graph = build_region_graph(&world_map, &spots(), &village());

        assert_eq!(graph.regions.len(), 5);
        assert!(
            graph
                .regions
                .iter()
                .any(|region| region.kind == RegionKind::GrasslandForest)
        );
        assert!(
            graph
                .regions
                .iter()
                .any(|region| region.kind == RegionKind::Desert)
        );
        assert!(
            graph
                .gates
                .iter()
                .any(|gate| gate.kind == TransitionGateKind::MistRiverFord)
        );
    }

    #[test]
    fn region_distance_score_clamps_to_region_radius() {
        let config = test_config();
        let world_map = WorldMap::new_for_testing(config.world.seed, &config);
        let graph = build_region_graph(&world_map, &spots(), &village());
        let region = graph.regions.first().expect("region");

        assert!(region_distance_score(region, region.center) > 0.99);
        assert_eq!(
            region_distance_score(
                region,
                region.center + Vec3::new(region.radius * 3.0, 0.0, 0.0)
            ),
            0.0
        );
    }

    #[test]
    fn dream_gate_requires_afterglow_and_matching_intent() {
        let mut journey = crate::game::journey::JourneyState::default();
        journey.dream.phase = crate::game::journey::DreamPhase::Afterglow;
        let mut intent = crate::game::intent::IntentState::default();
        crate::game::intent::advance_intent_state(
            &mut intent,
            0.0,
            0.0,
            [crate::game::intent::IntentSample {
                kind: crate::game::intent::IntentKind::BeyondVillage,
                source: crate::game::intent::IntentSource::Approaching,
                amount: 2.0,
            }],
        );

        assert!(transition_condition_met(
            TransitionCondition::DreamAfterglowAndIntent,
            Some(&journey),
            Some(&intent)
        ));
    }

    fn village() -> VillageState {
        VillageState {
            origin: Vec3::ZERO,
            spawn_point: Vec3::ZERO,
            areas: vec![
                VillageArea {
                    kind: VillageAreaKind::OuterPath,
                    position: Vec3::new(0.0, 0.0, -38.0),
                    radius: 14.0,
                },
                VillageArea {
                    kind: VillageAreaKind::Shore,
                    position: Vec3::new(0.0, 0.0, 34.0),
                    radius: 18.0,
                },
            ],
            houses: Vec::new(),
            actors: Vec::new(),
            nearest_actor: None,
            nearest_house: None,
            interaction_prompt: None,
            player_was_bootstrapped: true,
            herding: Default::default(),
        }
    }

    fn spots() -> WorldShowcaseSpots {
        WorldShowcaseSpots {
            ridge: spot(Vec3::new(120.0, 2.0, -80.0), BiomeKind::Ridge),
            grove: spot(Vec3::new(30.0, 1.0, 12.0), BiomeKind::Grove),
            water: spot(Vec3::new(-12.0, 0.0, 42.0), BiomeKind::Water),
            meadow: spot(Vec3::ZERO, BiomeKind::Meadow),
        }
    }

    fn spot(position: Vec3, biome: BiomeKind) -> ShowcaseSpot {
        ShowcaseSpot {
            coord: WorldGridCoord { x: 0, z: 0 },
            position,
            biome,
        }
    }

    fn test_config() -> AppConfig {
        AppConfig {
            window_title: "Dao".to_string(),
            log_directory: PathBuf::from("logs"),
            performance_log_name: "performance.log".to_string(),
            frame_log_interval: 60,
            performance_detail_interval: 1,
            presentation: PresentationConfig {
                enabled: false,
                scene_duration_seconds: 7.0,
                camera_blend_speed: 2.0,
            },
            world: WorldConfig {
                seed: 511,
                world_radius: 96,
                chunk_radius: 4,
                cell_size: 3.2,
                terrain_subdivisions: 6,
                terrain_scale: 18.0,
                height_variation: 6.0,
                water_level: -0.2,
                noise_octaves: 5,
                ridge_sharpness: 2.1,
                shoreline_blend: 0.2,
                river_frequency: 0.19,
                river_depth: 0.72,
                erosion_strength: 0.52,
                sediment_bias: 0.28,
                visible_chunk_radius: 2,
                high_detail_chunk_radius: 1,
                low_detail_chunk_radius: 2,
                preload_chunk_radius: 3,
                impostor_chunk_radius: 6,
                impostor_radial_bands: 3,
                impostor_angular_segments: 32,
                showcase_search_radius: 24,
                streaming_chunk_budget: 1,
                background_generation_budget: 2,
                streaming_cache_capacity: 32,
                collision_proxy_radius: 1,
                collision_subdivisions: 8,
                collision_chunk_budget: 1,
                collision_cache_capacity: 16,
                material_texture_resolution: 64,
                detail_density: 1.0,
            },
            environment: EnvironmentConfig {
                day_length_seconds: 180.0,
                wander_radius: 4.5,
                wander_speed: 0.7,
            },
            player: PlayerConfig {
                walk_speed: 7.0,
                sprint_multiplier: 1.6,
                mouse_sensitivity: 0.002,
                eye_height: 1.65,
                body_height: 1.2,
                capsule_radius: 0.4,
                max_slope_degrees: 45.0,
                step_height: 0.6,
                ground_snap_distance: 1.0,
                contact_substeps: 4,
                jump_velocity: 6.0,
                gravity: 18.0,
            },
            camera: CameraConfig {
                third_person_default_distance: 6.2,
                third_person_min_distance: 3.2,
                third_person_max_distance: 9.5,
                third_person_height: 2.25,
                third_person_side_offset: 0.42,
                third_person_smoothness: 12.0,
                third_person_ground_clearance: 0.55,
            },
            ecology: EcologyConfig {
                bird_count: 18,
                fish_count: 10,
                sheep_count: 9,
                state_update_interval_seconds: 0.2,
                visual_update_interval_seconds: 0.066,
                max_visible_bird_distance: 240.0,
                max_visible_fish_distance: 90.0,
                max_visible_sheep_distance: 120.0,
            },
            assets: AssetConfig {
                color_saturation: 1.0,
                warm_light_intensity: 1.0,
                water_alpha: 0.64,
                shadow_alpha: 0.58,
                foundation_proxy_mode: true,
                animate_placeholder_characters: false,
                animate_placeholder_ambience: false,
            },
            desert: DesertConfig {
                dune_height: 3.2,
                dune_frequency: 0.22,
                gobi_flatness: 0.48,
                oasis_radius: 38.0,
                oasis_moisture: 0.86,
                sandstorm_visibility: 46.0,
                sandstorm_particle_strength: 1.0,
                sandstorm_wind_speed: 4.2,
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
}
