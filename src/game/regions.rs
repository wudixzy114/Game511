use bevy::{
    color::LinearRgba,
    math::primitives::{Cuboid, Cylinder},
    pbr::MeshMaterial3d,
    prelude::*,
};

use crate::game::{
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

#[derive(Debug, Component)]
struct TransitionGateVisual {
    gate_id: u64,
}

#[derive(Debug, Resource, Clone)]
struct RegionMaterials {
    mist: Handle<StandardMaterial>,
    stone: Handle<StandardMaterial>,
}

const GATE_INTERACTION_RADIUS: f32 = 8.0;

type GateUpdateResources<'w> = (
    Res<'w, ButtonInput<KeyCode>>,
    Option<ResMut<'w, RegionGraphState>>,
    Option<Res<'w, JourneyState>>,
    Option<Res<'w, IntentState>>,
    Option<Res<'w, PerceptionState>>,
    Option<ResMut<'w, NotebookState>>,
    Res<'w, Time>,
);

fn initialize_region_graph(
    mut commands: Commands,
    world_map: Option<Res<WorldMap>>,
    spots: Option<Res<WorldShowcaseSpots>>,
    village: Option<Res<VillageState>>,
    existing: Option<Res<RegionGraphState>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if existing.is_some() {
        return;
    }
    let (Some(world_map), Some(spots), Some(village)) = (world_map, spots, village) else {
        return;
    };

    let graph = build_region_graph(&world_map, &spots, &village);
    let region_materials = RegionMaterials::new(&mut materials);
    spawn_gate_visuals(&mut commands, &mut meshes, &region_materials, &graph.gates);

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
    }
}

fn update_transition_gate_state(
    resources: GateUpdateResources<'_>,
    mut player_query: Query<&mut Transform, With<WandererPrototype>>,
) {
    let (keys, graph, journey, intent, perception, mut notebook, time) = resources;
    let Some(mut graph) = graph else {
        return;
    };
    let Some(player_transform) = player_query.iter().next() else {
        return;
    };
    let player_position = player_transform.translation;
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
        graph.gates[gate_index].state = TransitionGateState::Crossed;
        graph.current_region = to_region;
        let destination = graph
            .region(graph.current_region)
            .map(|region| region.center + Vec3::Y * 1.2);
        let region_label = graph
            .region(graph.current_region)
            .map(|region| region.kind.label());
        if let (Some(destination), Some(mut player_transform)) =
            (destination, player_query.iter_mut().next())
        {
            player_transform.translation = destination;
        }
        tracing::info!(
            target: "dao_game::regions::transition",
            gate_id,
            gate = gate_kind.label(),
            to_region = region_label,
            "natural boundary crossed"
        );
    }
}

fn update_transition_gate_visuals(
    graph: Option<Res<RegionGraphState>>,
    mut query: Query<
        (&TransitionGateVisual, &mut Visibility, &mut Transform),
        Without<WandererPrototype>,
    >,
) {
    let Some(graph) = graph else {
        return;
    };
    for (visual, mut visibility, mut transform) in &mut query {
        let Some(gate) = graph.gates.iter().find(|gate| gate.id == visual.gate_id) else {
            continue;
        };
        let visible = matches!(
            gate.state,
            TransitionGateState::Hinted | TransitionGateState::Open | TransitionGateState::Crossed
        );
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
        transform.scale = Vec3::splat(0.85 + pulse * 0.35);
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
            TransitionGateVisual { gate_id: gate.id },
        ));
        if gate.kind == TransitionGateKind::MountainPass {
            for offset in [-3.8, 3.8] {
                commands.spawn((
                    Name::new("MountainPassMarker"),
                    DespawnOnExit(AppScreen::InGame),
                    Mesh3d(meshes.add(Mesh::from(Cuboid::new(1.1, 5.6, 1.1)))),
                    MeshMaterial3d(materials.stone.clone()),
                    Transform::from_translation(gate.position + Vec3::new(offset, 2.8, 0.0)),
                    Visibility::Hidden,
                    TransitionGateVisual { gate_id: gate.id },
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
                TransitionGateVisual { gate_id: gate.id },
            ));
        }
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
            actors: Vec::new(),
            nearest_actor: None,
            interaction_prompt: None,
            player_was_bootstrapped: true,
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
                state_update_interval_seconds: 0.2,
                visual_update_interval_seconds: 0.066,
                max_visible_bird_distance: 240.0,
            },
            assets: AssetConfig {
                color_saturation: 1.0,
                warm_light_intensity: 1.0,
                water_alpha: 0.64,
                shadow_alpha: 0.58,
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
