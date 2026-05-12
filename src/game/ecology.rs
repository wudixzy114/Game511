use bevy::{
    math::primitives::{Capsule3d, Sphere},
    pbr::MeshMaterial3d,
    prelude::*,
};

use crate::{
    core::performance::{FramePerformance, PerformancePhase},
    game::{
        flow::{AppScreen, InGameState},
        intent::{IntentKind, IntentState, PerceptionState},
        journey::{DreamPhase, JourneyState},
        notebook::{
            NotebookEntryKind, NotebookRecord, NotebookSource, NotebookState, NotebookTag,
            record_notebook_entry,
        },
        places::planar_distance,
        regions::{RegionGraphState, TransitionGateKind},
        signs::SignState,
        village::{VillageAreaKind, VillageState},
        world::{WandererPrototype, WorldCycle, WorldMap},
    },
};

pub struct EcologyPlugin;

impl Plugin for EcologyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (
                initialize_ecology,
                update_ecology_state,
                animate_ecology_entities,
                update_ecology_interactions,
            )
                .chain()
                .run_if(in_state(InGameState::Running)),
        );
        app.add_systems(OnExit(AppScreen::InGame), cleanup_ecology_session);
    }
}

#[derive(Debug, Resource, Clone, PartialEq)]
pub struct EcologyState {
    pub flocks: Vec<AnimalFlockState>,
    pub npcs: Vec<NpcScheduleState>,
    pub latest_signal: Option<EcologySignal>,
    pub fortune_teller_recorded: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AnimalFlockState {
    pub id: u64,
    pub kind: AnimalKind,
    pub center: Vec3,
    pub home: Vec3,
    pub radius: f32,
    pub behavior: AnimalBehavior,
    pub omen_alignment: f32,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum AnimalKind {
    SheepHerd,
    BirdFlock,
    ShoreFish,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum AnimalBehavior {
    Grazing,
    Scattering,
    Migrating,
    Circling,
    Sheltering,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NpcScheduleState {
    pub id: u64,
    pub kind: NpcKind,
    pub home: Vec3,
    pub target: Vec3,
    pub phase: NpcSchedulePhase,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum NpcKind {
    Shepherd,
    Merchant,
    FortuneTeller,
}

impl NpcKind {
    fn label(self) -> &'static str {
        match self {
            Self::Shepherd => "牧羊人",
            Self::Merchant => "商人",
            Self::FortuneTeller => "占卜人",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum NpcSchedulePhase {
    Working,
    Resting,
    WatchingSea,
    TellingRumor,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum EcologySignal {
    BirdsTowardBoundary,
    SheepUneasy,
    MerchantDesertRumor,
    FortuneTellerLamp,
}

#[derive(Debug, Component)]
struct EcologyActor {
    id: u64,
    kind: EcologyActorKind,
    index: u32,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
enum EcologyActorKind {
    Bird,
    Fish,
    FortuneTeller,
}

#[derive(Debug, Resource, Clone)]
struct EcologyMaterials {
    bird: Handle<StandardMaterial>,
    fish: Handle<StandardMaterial>,
    npc: Handle<StandardMaterial>,
}

const BIRD_COUNT: u32 = 18;
const FISH_COUNT: u32 = 10;
const ECOLOGY_INTERACTION_RADIUS: f32 = 4.8;

type EcologyStateResources<'w> = (
    Option<Res<'w, WorldCycle>>,
    Option<Res<'w, JourneyState>>,
    Option<Res<'w, IntentState>>,
    Option<Res<'w, SignState>>,
    Option<Res<'w, PerceptionState>>,
    Option<Res<'w, RegionGraphState>>,
);

fn initialize_ecology(
    mut commands: Commands,
    world_map: Option<Res<WorldMap>>,
    village: Option<Res<VillageState>>,
    regions: Option<Res<RegionGraphState>>,
    existing: Option<Res<EcologyState>>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
) {
    if existing.is_some() {
        return;
    }
    let (Some(world_map), Some(village), Some(regions)) = (world_map, village, regions) else {
        return;
    };
    let ecology_materials = EcologyMaterials::new(&mut materials);
    let ecology = build_ecology_state(&world_map, &village, &regions);
    spawn_ecology_visuals(
        &mut commands,
        &mut meshes,
        &ecology_materials,
        &ecology,
        &world_map,
    );

    tracing::info!(
        target: "dao_game::ecology",
        flock_count = ecology.flocks.len(),
        npc_count = ecology.npcs.len(),
        "ecology system initialized"
    );

    commands.insert_resource(ecology);
    commands.insert_resource(ecology_materials);
}

pub fn build_ecology_state(
    world_map: &WorldMap,
    village: &VillageState,
    regions: &RegionGraphState,
) -> EcologyState {
    let sheep_pen = village
        .area(VillageAreaKind::SheepPen)
        .map(|area| area.position)
        .unwrap_or(village.origin + Vec3::new(18.0, 0.0, -12.0));
    let shore = village
        .area(VillageAreaKind::Shore)
        .map(|area| area.position)
        .unwrap_or(village.origin + Vec3::new(0.0, 0.0, 34.0));
    let market = village
        .area(VillageAreaKind::Market)
        .map(|area| area.position)
        .unwrap_or(village.origin + Vec3::new(-16.0, 0.0, -8.0));
    let outer_gate = regions
        .gates
        .iter()
        .find(|gate| gate.kind == TransitionGateKind::MistRiverFord)
        .map(|gate| gate.position)
        .unwrap_or(village.origin + Vec3::new(0.0, 0.0, -70.0));

    EcologyState {
        flocks: vec![
            AnimalFlockState {
                id: stable_ecology_id(world_map.seed_value(), 11),
                kind: AnimalKind::SheepHerd,
                center: sheep_pen,
                home: sheep_pen,
                radius: 13.0,
                behavior: AnimalBehavior::Grazing,
                omen_alignment: 0.0,
            },
            AnimalFlockState {
                id: stable_ecology_id(world_map.seed_value(), 23),
                kind: AnimalKind::BirdFlock,
                center: shore + Vec3::Y * 14.0,
                home: shore + Vec3::Y * 14.0,
                radius: 24.0,
                behavior: AnimalBehavior::Circling,
                omen_alignment: 0.0,
            },
            AnimalFlockState {
                id: stable_ecology_id(world_map.seed_value(), 29),
                kind: AnimalKind::ShoreFish,
                center: shore,
                home: shore,
                radius: 16.0,
                behavior: AnimalBehavior::Sheltering,
                omen_alignment: 0.0,
            },
        ],
        npcs: vec![
            NpcScheduleState {
                id: stable_ecology_id(world_map.seed_value(), 41),
                kind: NpcKind::Shepherd,
                home: sheep_pen + Vec3::new(-4.0, 0.0, 3.0),
                target: sheep_pen,
                phase: NpcSchedulePhase::Working,
            },
            NpcScheduleState {
                id: stable_ecology_id(world_map.seed_value(), 43),
                kind: NpcKind::Merchant,
                home: market,
                target: market,
                phase: NpcSchedulePhase::Working,
            },
            NpcScheduleState {
                id: stable_ecology_id(world_map.seed_value(), 47),
                kind: NpcKind::FortuneTeller,
                home: outer_gate + Vec3::new(-6.0, 0.0, 5.0),
                target: outer_gate,
                phase: NpcSchedulePhase::Resting,
            },
        ],
        latest_signal: None,
        fortune_teller_recorded: false,
    }
}

fn update_ecology_state(
    time: Res<Time>,
    resources: EcologyStateResources<'_>,
    ecology: Option<ResMut<EcologyState>>,
    mut performance: ResMut<FramePerformance>,
) {
    let started_at = std::time::Instant::now();
    let Some(mut ecology) = ecology else {
        return;
    };
    let (cycle, journey, intent, signs, perception, regions) = resources;
    let daylight = cycle.as_deref().map_or(0.8, |cycle| cycle.daylight);
    let dream_afterglow = journey
        .as_deref()
        .is_some_and(|journey| journey.dream.phase == DreamPhase::Afterglow);
    let dream_intent = intent
        .as_deref()
        .map_or(0.0, |intent| intent.strength(IntentKind::DreamLandmark));
    let beyond_intent = intent
        .as_deref()
        .map_or(0.0, |intent| intent.strength(IntentKind::BeyondVillage));
    let perception_active = perception
        .as_deref()
        .is_some_and(|perception| perception.active);
    let omen_intensity = signs.as_deref().map_or(0.0, |signs| signs.omen_intensity);
    let boundary_direction = regions
        .as_deref()
        .and_then(|regions| regions.nearest_gate())
        .map(|gate| gate.position)
        .unwrap_or(Vec3::ZERO);

    let mut latest_signal = None;
    for flock in &mut ecology.flocks {
        let context = FlockContext {
            delta_seconds: time.delta_secs(),
            elapsed_seconds: time.elapsed_secs(),
            daylight,
            dream_afterglow,
            dream_intent,
            beyond_intent,
            perception_active,
            omen_intensity,
            boundary_position: boundary_direction,
        };
        let signal = advance_flock_state(flock, context);
        latest_signal = latest_signal.or(signal);
    }

    for npc in &mut ecology.npcs {
        let previous = npc.phase;
        npc.phase = schedule_phase_for(npc.kind, daylight, dream_afterglow, beyond_intent);
        npc.target = npc_target_for(npc, boundary_direction);
        if previous != npc.phase {
            tracing::info!(
                target: "dao_game::ecology::schedule",
                npc = npc.kind.label(),
                phase = ?npc.phase,
                "npc schedule changed"
            );
        }
        if npc.kind == NpcKind::Merchant && npc.phase == NpcSchedulePhase::TellingRumor {
            latest_signal = latest_signal.or(Some(EcologySignal::MerchantDesertRumor));
        }
        if npc.kind == NpcKind::FortuneTeller && npc.phase == NpcSchedulePhase::TellingRumor {
            latest_signal = latest_signal.or(Some(EcologySignal::FortuneTellerLamp));
        }
    }

    ecology.latest_signal = latest_signal;
    performance.record_phase_duration(PerformancePhase::Ecology, started_at.elapsed());
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FlockContext {
    pub delta_seconds: f32,
    pub elapsed_seconds: f32,
    pub daylight: f32,
    pub dream_afterglow: bool,
    pub dream_intent: f32,
    pub beyond_intent: f32,
    pub perception_active: bool,
    pub omen_intensity: f32,
    pub boundary_position: Vec3,
}

pub fn advance_flock_state(
    flock: &mut AnimalFlockState,
    context: FlockContext,
) -> Option<EcologySignal> {
    match flock.kind {
        AnimalKind::SheepHerd => {
            let unease = (context.omen_intensity * 0.45
                + context.dream_intent * 0.32
                + context.beyond_intent * 0.18)
                .clamp(0.0, 1.0);
            flock.behavior = if unease > 0.45 {
                AnimalBehavior::Scattering
            } else {
                AnimalBehavior::Grazing
            };
            flock.omen_alignment = unease;
            let phase = context.elapsed_seconds * if unease > 0.45 { 0.7 } else { 0.18 };
            let radius = flock.radius * if unease > 0.45 { 0.66 } else { 0.34 };
            flock.center = flock.home + Vec3::new(phase.cos() * radius, 0.0, phase.sin() * radius);
            (unease > 0.52).then_some(EcologySignal::SheepUneasy)
        }
        AnimalKind::BirdFlock => {
            let migration = (context.dream_intent + context.beyond_intent + context.omen_intensity)
                .clamp(0.0, 1.0);
            flock.behavior = if migration > 0.42 || context.perception_active {
                AnimalBehavior::Migrating
            } else {
                AnimalBehavior::Circling
            };
            flock.omen_alignment = migration;
            if flock.behavior == AnimalBehavior::Migrating
                && context.boundary_position != Vec3::ZERO
            {
                let direction = (context.boundary_position - flock.center).normalize_or_zero();
                flock.center += direction * (context.delta_seconds * (5.0 + migration * 4.0));
            } else {
                let phase = context.elapsed_seconds * 0.24;
                flock.center = flock.home
                    + Vec3::new(
                        phase.cos() * flock.radius,
                        0.0,
                        phase.sin() * flock.radius * 0.5,
                    );
            }
            (flock.behavior == AnimalBehavior::Migrating)
                .then_some(EcologySignal::BirdsTowardBoundary)
        }
        AnimalKind::ShoreFish => {
            flock.behavior = if context.daylight < 0.25 {
                AnimalBehavior::Sheltering
            } else {
                AnimalBehavior::Circling
            };
            let phase = context.elapsed_seconds * 0.36;
            flock.center = flock.home
                + Vec3::new(
                    phase.cos() * flock.radius * 0.62,
                    0.0,
                    phase.sin() * flock.radius * 0.22,
                );
            None
        }
    }
}

fn animate_ecology_entities(
    time: Res<Time>,
    ecology: Option<Res<EcologyState>>,
    world_map: Option<Res<WorldMap>>,
    player_query: Query<&Transform, With<WandererPrototype>>,
    mut actor_query: Query<
        (&EcologyActor, &mut Transform, &mut Visibility),
        Without<WandererPrototype>,
    >,
) {
    let (Some(ecology), Some(world_map)) = (ecology, world_map) else {
        return;
    };
    let player_position = player_query
        .iter()
        .next()
        .map(|transform| transform.translation)
        .unwrap_or(Vec3::ZERO);
    for (actor, mut transform, mut visibility) in &mut actor_query {
        match actor.kind {
            EcologyActorKind::Bird => {
                let Some(flock) = ecology
                    .flocks
                    .iter()
                    .find(|flock| flock.kind == AnimalKind::BirdFlock)
                else {
                    continue;
                };
                let phase = time.elapsed_secs() * 0.92 + actor.index as f32 * 0.72;
                let spread = 4.0 + actor.index as f32 % 5.0 + (actor.id % 7) as f32 * 0.08;
                transform.translation = flock.center
                    + Vec3::new(
                        phase.cos() * spread,
                        (phase * 1.7).sin() * 1.2,
                        phase.sin() * spread * 0.56,
                    );
                let look_from = transform.translation;
                transform.look_at(
                    look_from + Vec3::new(phase.cos(), 0.1, phase.sin()),
                    Vec3::Y,
                );
                *visibility = if planar_distance(player_position, transform.translation) < 240.0 {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                };
            }
            EcologyActorKind::Fish => {
                let Some(flock) = ecology
                    .flocks
                    .iter()
                    .find(|flock| flock.kind == AnimalKind::ShoreFish)
                else {
                    continue;
                };
                let phase = time.elapsed_secs() * 0.7 + actor.index as f32;
                let position = flock.center + Vec3::new(phase.cos() * 2.4, 0.0, phase.sin() * 1.1);
                let y = world_map.water_level() + 0.08;
                transform.translation = Vec3::new(position.x, y, position.z);
                transform.rotation = Quat::from_rotation_y(phase);
            }
            EcologyActorKind::FortuneTeller => {
                let Some(npc) = ecology
                    .npcs
                    .iter()
                    .find(|npc| npc.kind == NpcKind::FortuneTeller)
                else {
                    continue;
                };
                let target = ground_position(&world_map, npc.target, 1.05);
                transform.translation = transform.translation.lerp(target, 0.04);
                if planar_distance(transform.translation, player_position) < 14.0 {
                    let actor_y = transform.translation.y;
                    transform.look_at(
                        Vec3::new(player_position.x, actor_y, player_position.z),
                        Vec3::Y,
                    );
                }
                *visibility = if matches!(
                    npc.phase,
                    NpcSchedulePhase::TellingRumor | NpcSchedulePhase::WatchingSea
                ) {
                    Visibility::Visible
                } else {
                    Visibility::Hidden
                };
            }
        }
    }
}

fn update_ecology_interactions(
    keys: Res<ButtonInput<KeyCode>>,
    ecology: Option<ResMut<EcologyState>>,
    player_query: Query<&Transform, With<WandererPrototype>>,
    actor_query: Query<(&EcologyActor, &Transform)>,
    mut notebook: Option<ResMut<NotebookState>>,
    time: Res<Time>,
) {
    let Some(mut ecology) = ecology else {
        return;
    };
    if ecology.fortune_teller_recorded || !keys.just_pressed(KeyCode::KeyF) {
        return;
    }
    let Some(player_transform) = player_query.iter().next() else {
        return;
    };
    let near_fortune_teller = actor_query.iter().any(|(actor, transform)| {
        actor.kind == EcologyActorKind::FortuneTeller
            && planar_distance(player_transform.translation, transform.translation)
                <= ECOLOGY_INTERACTION_RADIUS
    });
    if !near_fortune_teller {
        return;
    }
    ecology.fortune_teller_recorded = true;
    let _ = record_notebook_entry(
        notebook.as_deref_mut(),
        NotebookRecord {
            kind: NotebookEntryKind::Person,
            at_seconds: time.elapsed_secs(),
            location: Some("村外小路".to_string()),
            source: NotebookSource::Dialogue,
            title: "占卜人的灯".to_string(),
            body: "占卜人说，梦不会替人走路，只会在你看向路口时把风点亮。".to_string(),
            tags: vec![NotebookTag::Village, NotebookTag::Dream, NotebookTag::Omen],
        },
    );
    tracing::info!(
        target: "dao_game::ecology::interaction",
        "fortune teller rumor recorded"
    );
}

fn spawn_ecology_visuals(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &EcologyMaterials,
    ecology: &EcologyState,
    world_map: &WorldMap,
) {
    let bird_mesh = meshes.add(Mesh::from(Capsule3d::new(0.12, 0.42)));
    for index in 0..BIRD_COUNT {
        commands.spawn((
            Name::new("BirdFlockMember"),
            DespawnOnExit(AppScreen::InGame),
            Mesh3d(bird_mesh.clone()),
            MeshMaterial3d(materials.bird.clone()),
            Transform::from_translation(Vec3::new(0.0, -120.0, 0.0))
                .with_scale(Vec3::new(1.6, 0.34, 0.62)),
            EcologyActor {
                id: stable_ecology_id(index as u64, 101),
                kind: EcologyActorKind::Bird,
                index,
            },
        ));
    }

    let fish_mesh = meshes.add(Sphere::new(0.22).mesh().uv(12, 8));
    for index in 0..FISH_COUNT {
        commands.spawn((
            Name::new("ShoreFish"),
            DespawnOnExit(AppScreen::InGame),
            Mesh3d(fish_mesh.clone()),
            MeshMaterial3d(materials.fish.clone()),
            Transform::from_translation(Vec3::new(0.0, world_map.water_level(), 0.0))
                .with_scale(Vec3::new(1.8, 0.42, 0.72)),
            EcologyActor {
                id: stable_ecology_id(index as u64, 203),
                kind: EcologyActorKind::Fish,
                index,
            },
        ));
    }

    if let Some(npc) = ecology
        .npcs
        .iter()
        .find(|npc| npc.kind == NpcKind::FortuneTeller)
    {
        commands.spawn((
            Name::new("FortuneTeller"),
            DespawnOnExit(AppScreen::InGame),
            Mesh3d(meshes.add(Mesh::from(Capsule3d::new(0.38, 1.42)))),
            MeshMaterial3d(materials.npc.clone()),
            Transform::from_translation(ground_position(world_map, npc.home, 1.05)),
            Visibility::Hidden,
            EcologyActor {
                id: npc.id,
                kind: EcologyActorKind::FortuneTeller,
                index: 0,
            },
        ));
    }
}

pub fn schedule_phase_for(
    kind: NpcKind,
    daylight: f32,
    dream_afterglow: bool,
    beyond_intent: f32,
) -> NpcSchedulePhase {
    match kind {
        NpcKind::Shepherd => {
            if daylight < 0.22 {
                NpcSchedulePhase::Resting
            } else if dream_afterglow && beyond_intent > 0.3 {
                NpcSchedulePhase::WatchingSea
            } else {
                NpcSchedulePhase::Working
            }
        }
        NpcKind::Merchant => {
            if dream_afterglow && beyond_intent > 0.24 {
                NpcSchedulePhase::TellingRumor
            } else if daylight < 0.18 {
                NpcSchedulePhase::Resting
            } else {
                NpcSchedulePhase::Working
            }
        }
        NpcKind::FortuneTeller => {
            if dream_afterglow && beyond_intent > 0.2 {
                NpcSchedulePhase::TellingRumor
            } else if daylight < 0.28 {
                NpcSchedulePhase::WatchingSea
            } else {
                NpcSchedulePhase::Resting
            }
        }
    }
}

fn npc_target_for(npc: &NpcScheduleState, boundary_position: Vec3) -> Vec3 {
    match npc.phase {
        NpcSchedulePhase::Working | NpcSchedulePhase::Resting => npc.home,
        NpcSchedulePhase::WatchingSea => npc.home + Vec3::new(0.0, 0.0, 8.0),
        NpcSchedulePhase::TellingRumor => {
            if boundary_position != Vec3::ZERO {
                boundary_position + Vec3::new(-6.0, 0.0, 5.0)
            } else {
                npc.home
            }
        }
    }
}

fn ground_position(world_map: &WorldMap, position: Vec3, y_offset: f32) -> Vec3 {
    let height = world_map
        .sample_height(position.x, position.z)
        .unwrap_or(position.y)
        .max(world_map.water_level() + 0.05);
    Vec3::new(position.x, height + y_offset, position.z)
}

impl EcologyMaterials {
    fn new(materials: &mut Assets<StandardMaterial>) -> Self {
        Self {
            bird: materials.add(StandardMaterial {
                base_color: Color::srgb(0.18, 0.18, 0.16),
                perceptual_roughness: 0.8,
                ..Default::default()
            }),
            fish: materials.add(StandardMaterial {
                base_color: Color::srgb(0.34, 0.56, 0.62),
                perceptual_roughness: 0.42,
                ..Default::default()
            }),
            npc: materials.add(StandardMaterial {
                base_color: Color::srgb(0.28, 0.22, 0.36),
                perceptual_roughness: 0.94,
                ..Default::default()
            }),
        }
    }
}

fn stable_ecology_id(seed: u64, salt: u64) -> u64 {
    let mut value = seed
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add(salt.wrapping_mul(0xBF58_476D_1CE4_E5B9));
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn cleanup_ecology_session(mut commands: Commands) {
    commands.remove_resource::<EcologyState>();
    commands.remove_resource::<EcologyMaterials>();
}

#[cfg(test)]
mod tests {
    use bevy::prelude::Vec3;

    use crate::game::ecology::{
        AnimalBehavior, AnimalFlockState, AnimalKind, EcologySignal, FlockContext, NpcKind,
        NpcSchedulePhase, advance_flock_state, schedule_phase_for,
    };

    #[test]
    fn bird_flock_migrates_when_intent_and_omen_align() {
        let mut flock = AnimalFlockState {
            id: 1,
            kind: AnimalKind::BirdFlock,
            center: Vec3::ZERO,
            home: Vec3::ZERO,
            radius: 10.0,
            behavior: AnimalBehavior::Circling,
            omen_alignment: 0.0,
        };
        let signal = advance_flock_state(
            &mut flock,
            FlockContext {
                delta_seconds: 1.0,
                elapsed_seconds: 2.0,
                daylight: 0.8,
                dream_afterglow: true,
                dream_intent: 0.5,
                beyond_intent: 0.4,
                perception_active: false,
                omen_intensity: 0.3,
                boundary_position: Vec3::new(20.0, 0.0, 0.0),
            },
        );

        assert_eq!(flock.behavior, AnimalBehavior::Migrating);
        assert_eq!(signal, Some(EcologySignal::BirdsTowardBoundary));
        assert!(flock.center.x > 0.0);
    }

    #[test]
    fn sheep_scatter_under_strong_dream_signal() {
        let mut flock = AnimalFlockState {
            id: 1,
            kind: AnimalKind::SheepHerd,
            center: Vec3::ZERO,
            home: Vec3::ZERO,
            radius: 10.0,
            behavior: AnimalBehavior::Grazing,
            omen_alignment: 0.0,
        };
        let signal = advance_flock_state(
            &mut flock,
            FlockContext {
                delta_seconds: 1.0,
                elapsed_seconds: 2.0,
                daylight: 0.8,
                dream_afterglow: true,
                dream_intent: 0.9,
                beyond_intent: 0.7,
                perception_active: false,
                omen_intensity: 0.7,
                boundary_position: Vec3::ZERO,
            },
        );

        assert_eq!(flock.behavior, AnimalBehavior::Scattering);
        assert_eq!(signal, Some(EcologySignal::SheepUneasy));
    }

    #[test]
    fn fortune_teller_appears_after_dream_and_beyond_intent() {
        assert_eq!(
            schedule_phase_for(NpcKind::FortuneTeller, 0.7, true, 0.35),
            NpcSchedulePhase::TellingRumor
        );
        assert_eq!(
            schedule_phase_for(NpcKind::FortuneTeller, 0.7, false, 0.35),
            NpcSchedulePhase::Resting
        );
    }
}
