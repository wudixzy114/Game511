use std::time::Instant;

use bevy::prelude::*;

use crate::game::{
    flow::{AppScreen, InGameState},
    signs::{OmenKind, SignState},
    world::{BiomeKind, TerrainTile, WandererPrototype, WorldGridCoord, WorldMap},
};

pub struct JourneyPlugin;

impl Plugin for JourneyPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppScreen::InGame), initialize_journey_session);
        app.add_systems(
            Update,
            (select_journey_target, advance_journey_session)
                .chain()
                .run_if(in_state(InGameState::Running)),
        );
        app.add_systems(OnExit(AppScreen::InGame), cleanup_journey_session);
    }
}

#[derive(Debug, Resource, Clone, PartialEq)]
pub struct JourneyState {
    pub stage: JourneyStage,
    pub target: Option<JourneyTarget>,
    pub triggered_omens: Vec<JourneyOmenMemory>,
    pub memories: Vec<JourneyMemory>,
    pub session_elapsed: f32,
    pub stage_elapsed: f32,
    pub last_distance_to_target: Option<f32>,
}

impl Default for JourneyState {
    fn default() -> Self {
        Self {
            stage: JourneyStage::FirstArrival,
            target: None,
            triggered_omens: Vec::new(),
            memories: Vec::new(),
            session_elapsed: 0.0,
            stage_elapsed: 0.0,
            last_distance_to_target: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum JourneyStage {
    FirstArrival,
    OmenEmerging,
    BeingDrawn,
    PlaceReached,
    WorldResponded,
    EchoSettled,
}

impl JourneyStage {
    pub fn label(self) -> &'static str {
        match self {
            Self::FirstArrival => "初入世界",
            Self::OmenEmerging => "征兆浮现",
            Self::BeingDrawn => "被牵引",
            Self::PlaceReached => "抵达地点",
            Self::WorldResponded => "世界回应",
            Self::EchoSettled => "回响沉淀",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum JourneyPlaceKind {
    AncientTree,
    SpringEye,
    RidgeGate,
    QuietBay,
    StoneRing,
}

impl JourneyPlaceKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::AncientTree => "古树",
            Self::SpringEye => "泉眼",
            Self::RidgeGate => "山脊门",
            Self::QuietBay => "静水湾",
            Self::StoneRing => "石阵",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JourneyTarget {
    pub id: u64,
    pub grid: WorldGridCoord,
    pub position: Vec3,
    pub kind: JourneyPlaceKind,
    pub biome: BiomeKind,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JourneyOmenMemory {
    pub omen: OmenKind,
    pub at_seconds: f32,
    pub position: Vec3,
}

#[derive(Debug, Clone, PartialEq)]
pub struct JourneyMemory {
    pub kind: JourneyMemoryKind,
    pub stage: JourneyStage,
    pub at_seconds: f32,
    pub position: Vec3,
    pub place_kind: Option<JourneyPlaceKind>,
    pub omen: Option<OmenKind>,
    pub text: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum JourneyMemoryKind {
    Arrival,
    Response,
    Echo,
}

#[derive(Debug, Clone, PartialEq)]
pub enum JourneyEvent {
    StageChanged {
        from: JourneyStage,
        to: JourneyStage,
        at_seconds: f32,
    },
    OmenRecorded(JourneyOmenMemory),
    MemoryRecorded(JourneyMemory),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JourneyAdvanceContext {
    pub delta_seconds: f32,
    pub player_position: Vec3,
    pub distance_to_target: Option<f32>,
    pub omen_triggered: bool,
    pub current_omen: Option<OmenKind>,
}

const TARGET_SEARCH_RADIUS_TILES: i32 = 52;
const TARGET_SEARCH_STEP_TILES: usize = 2;
const MIN_TARGET_DISTANCE: f32 = 42.0;
const MAX_TARGET_DISTANCE: f32 = 180.0;
const IDEAL_TARGET_DISTANCE: f32 = 96.0;
const ARRIVAL_RADIUS: f32 = 13.5;
const OMEN_FALLBACK_SECONDS: f32 = 4.0;
const RESPONSE_DELAY_SECONDS: f32 = 1.35;
const ECHO_DELAY_SECONDS: f32 = 2.1;
const MAX_OMEN_MEMORIES: usize = 12;
const MAX_JOURNEY_MEMORIES: usize = 16;

fn initialize_journey_session(mut commands: Commands) {
    commands.insert_resource(JourneyState::default());
    tracing::info!(
        target: "dao_game::journey::lifecycle",
        "journey session initialized"
    );
}

fn cleanup_journey_session(mut commands: Commands) {
    commands.remove_resource::<JourneyState>();
}

fn select_journey_target(
    journey: Option<ResMut<JourneyState>>,
    world_map: Option<Res<WorldMap>>,
    wanderer_query: Query<&Transform, With<WandererPrototype>>,
) {
    let Some(mut journey) = journey else {
        return;
    };
    if journey.target.is_some() {
        return;
    }

    let Some(world_map) = world_map else {
        return;
    };
    let Some(transform) = wanderer_query.iter().next() else {
        return;
    };

    let started_at = Instant::now();
    match select_journey_target_from_world(&world_map, transform.translation) {
        Some(target) => {
            let distance = planar_distance(transform.translation, target.position);
            journey.target = Some(target);
            journey.last_distance_to_target = Some(distance);
            tracing::info!(
                target: "dao_game::journey::director",
                target_id = target.id,
                grid_x = target.grid.x,
                grid_z = target.grid.z,
                distance,
                place_kind = target.kind.label(),
                biome = ?target.biome,
                selection_ms = started_at.elapsed().as_secs_f32() * 1000.0,
                "journey target selected"
            );
        }
        None => {
            tracing::warn!(
                target: "dao_game::journey::director",
                player_x = transform.translation.x,
                player_z = transform.translation.z,
                selection_ms = started_at.elapsed().as_secs_f32() * 1000.0,
                "journey target selection found no candidate"
            );
        }
    }
}

fn advance_journey_session(
    time: Res<Time>,
    signs: Option<Res<SignState>>,
    journey: Option<ResMut<JourneyState>>,
    wanderer_query: Query<&Transform, With<WandererPrototype>>,
) {
    let Some(mut journey) = journey else {
        return;
    };
    let Some(transform) = wanderer_query.iter().next() else {
        return;
    };

    let distance_to_target = journey
        .target
        .map(|target| planar_distance(transform.translation, target.position));
    let sign_state = signs.as_deref();
    let events = advance_journey_state(
        &mut journey,
        JourneyAdvanceContext {
            delta_seconds: time.delta_secs(),
            player_position: transform.translation,
            distance_to_target,
            omen_triggered: sign_state.is_some_and(|signs| signs.omen_triggered),
            current_omen: sign_state.and_then(|signs| signs.current_omen),
        },
    );

    for event in &events {
        log_journey_event(event);
    }
}

pub fn advance_journey_state(
    state: &mut JourneyState,
    context: JourneyAdvanceContext,
) -> Vec<JourneyEvent> {
    let mut events = Vec::new();
    let delta_seconds = context.delta_seconds.max(0.0);
    state.session_elapsed += delta_seconds;
    state.stage_elapsed += delta_seconds;

    if let Some(distance) = context.distance_to_target {
        state.last_distance_to_target = Some(distance);
    }

    if context.omen_triggered
        && let Some(omen) = context.current_omen
        && should_record_omen(state, omen)
    {
        let memory = JourneyOmenMemory {
            omen,
            at_seconds: state.session_elapsed,
            position: context.player_position,
        };
        push_bounded(&mut state.triggered_omens, memory, MAX_OMEN_MEMORIES);
        events.push(JourneyEvent::OmenRecorded(memory));
    }

    match state.stage {
        JourneyStage::FirstArrival => {
            if state.target.is_some() {
                transition_stage(state, JourneyStage::OmenEmerging, &mut events);
            }
        }
        JourneyStage::OmenEmerging => {
            if state.target.is_some()
                && (context.omen_triggered || state.stage_elapsed >= OMEN_FALLBACK_SECONDS)
            {
                transition_stage(state, JourneyStage::BeingDrawn, &mut events);
            }
        }
        JourneyStage::BeingDrawn => {
            if context
                .distance_to_target
                .is_some_and(|distance| distance <= ARRIVAL_RADIUS)
            {
                transition_stage(state, JourneyStage::PlaceReached, &mut events);
                record_journey_memory(state, JourneyMemoryKind::Arrival, context, &mut events);
            }
        }
        JourneyStage::PlaceReached => {
            if state.stage_elapsed >= RESPONSE_DELAY_SECONDS {
                transition_stage(state, JourneyStage::WorldResponded, &mut events);
                record_journey_memory(state, JourneyMemoryKind::Response, context, &mut events);
            }
        }
        JourneyStage::WorldResponded => {
            if state.stage_elapsed >= ECHO_DELAY_SECONDS {
                transition_stage(state, JourneyStage::EchoSettled, &mut events);
                record_journey_memory(state, JourneyMemoryKind::Echo, context, &mut events);
            }
        }
        JourneyStage::EchoSettled => {}
    }

    events
}

fn transition_stage(state: &mut JourneyState, next: JourneyStage, events: &mut Vec<JourneyEvent>) {
    if state.stage == next {
        return;
    }
    let previous = state.stage;
    state.stage = next;
    state.stage_elapsed = 0.0;
    events.push(JourneyEvent::StageChanged {
        from: previous,
        to: next,
        at_seconds: state.session_elapsed,
    });
}

fn record_journey_memory(
    state: &mut JourneyState,
    kind: JourneyMemoryKind,
    context: JourneyAdvanceContext,
    events: &mut Vec<JourneyEvent>,
) {
    let memory = JourneyMemory {
        kind,
        stage: state.stage,
        at_seconds: state.session_elapsed,
        position: context.player_position,
        place_kind: state.target.map(|target| target.kind),
        omen: context.current_omen,
        text: memory_text(kind, state.target.map(|target| target.kind)),
    };
    push_bounded(&mut state.memories, memory.clone(), MAX_JOURNEY_MEMORIES);
    events.push(JourneyEvent::MemoryRecorded(memory));
}

fn should_record_omen(state: &JourneyState, omen: OmenKind) -> bool {
    state
        .triggered_omens
        .last()
        .is_none_or(|memory| memory.omen != omen)
}

fn memory_text(kind: JourneyMemoryKind, place_kind: Option<JourneyPlaceKind>) -> String {
    let place = place_kind
        .map(JourneyPlaceKind::label)
        .unwrap_or("未名之地");
    match kind {
        JourneyMemoryKind::Arrival => format!("你抵达了{place}。"),
        JourneyMemoryKind::Response => format!("{place}回应了你的停留。"),
        JourneyMemoryKind::Echo => format!("{place}的回响沉入记忆。"),
    }
}

fn push_bounded<T>(items: &mut Vec<T>, item: T, max_len: usize) {
    items.push(item);
    if items.len() > max_len {
        let overflow = items.len() - max_len;
        items.drain(0..overflow);
    }
}

fn log_journey_event(event: &JourneyEvent) {
    match event {
        JourneyEvent::StageChanged {
            from,
            to,
            at_seconds,
        } => {
            tracing::info!(
                target: "dao_game::journey::stage",
                from = from.label(),
                to = to.label(),
                at_seconds,
                "journey stage advanced"
            );
        }
        JourneyEvent::OmenRecorded(memory) => {
            tracing::info!(
                target: "dao_game::journey::memory",
                omen = ?memory.omen,
                at_seconds = memory.at_seconds,
                x = memory.position.x,
                z = memory.position.z,
                "journey omen recorded"
            );
        }
        JourneyEvent::MemoryRecorded(memory) => {
            tracing::info!(
                target: "dao_game::journey::memory",
                kind = ?memory.kind,
                stage = memory.stage.label(),
                at_seconds = memory.at_seconds,
                text = %memory.text,
                "journey memory recorded"
            );
        }
    }
}

fn select_journey_target_from_world(world_map: &WorldMap, origin: Vec3) -> Option<JourneyTarget> {
    let origin_x = (origin.x / world_map.cell_size()).round() as i32;
    let origin_z = (origin.z / world_map.cell_size()).round() as i32;
    let search_radius = TARGET_SEARCH_RADIUS_TILES.min(world_map.radius()).max(1);
    let mut best: Option<(f32, JourneyTarget)> = None;

    for z in (origin_z - search_radius..=origin_z + search_radius).step_by(TARGET_SEARCH_STEP_TILES)
    {
        for x in
            (origin_x - search_radius..=origin_x + search_radius).step_by(TARGET_SEARCH_STEP_TILES)
        {
            let Some(tile) = world_map.tile_at_grid(x, z) else {
                continue;
            };
            let position =
                world_map.tile_translation(x, z, tile.height().max(world_map.water_level()) + 0.1);
            let distance = planar_distance(origin, position);
            if !(MIN_TARGET_DISTANCE..=MAX_TARGET_DISTANCE).contains(&distance) {
                continue;
            }

            let kind = place_kind_for_tile(tile);
            let score = score_journey_target(tile, kind, distance, x, z);
            let candidate = JourneyTarget {
                id: journey_target_id(x, z, kind),
                grid: WorldGridCoord { x, z },
                position,
                kind,
                biome: tile.biome(),
            };

            if best
                .as_ref()
                .is_none_or(|(best_score, _)| score > *best_score)
            {
                best = Some((score, candidate));
            }
        }
    }

    best.map(|(_, target)| target)
}

fn score_journey_target(
    tile: TerrainTile,
    kind: JourneyPlaceKind,
    distance: f32,
    x: i32,
    z: i32,
) -> f32 {
    let distance_score = 1.0 - ((distance - IDEAL_TARGET_DISTANCE).abs() / IDEAL_TARGET_DISTANCE);
    let terrain_score = match kind {
        JourneyPlaceKind::AncientTree => tile.moisture() * 0.55 + (1.0 - tile.slope()) * 0.25,
        JourneyPlaceKind::SpringEye => tile.river() * 0.48 + tile.moisture() * 0.32,
        JourneyPlaceKind::RidgeGate => tile.height() * 0.08 + tile.slope() * 0.5,
        JourneyPlaceKind::QuietBay => {
            (1.0 - tile.slope()).clamp(0.0, 1.0) * 0.35 + tile.moisture() * 0.25
        }
        JourneyPlaceKind::StoneRing => {
            (1.0 - tile.moisture()).clamp(0.0, 1.0) * 0.34 + tile.erosion() * 0.2
        }
    };
    distance_score.clamp(0.0, 1.0) * 0.54 + terrain_score + hash_unit(x, z, kind as u64) * 0.015
}

fn place_kind_for_tile(tile: TerrainTile) -> JourneyPlaceKind {
    match tile.biome() {
        BiomeKind::Grove => JourneyPlaceKind::AncientTree,
        BiomeKind::Ridge => JourneyPlaceKind::RidgeGate,
        BiomeKind::Water if tile.river() > 0.46 => JourneyPlaceKind::SpringEye,
        BiomeKind::Water => JourneyPlaceKind::QuietBay,
        BiomeKind::Meadow if tile.moisture() > 0.68 || tile.river() > 0.42 => {
            JourneyPlaceKind::SpringEye
        }
        BiomeKind::Meadow | BiomeKind::Steppe => JourneyPlaceKind::StoneRing,
    }
}

fn journey_target_id(x: i32, z: i32, kind: JourneyPlaceKind) -> u64 {
    let mut value = (x as i64 as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add((z as i64 as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9))
        .wrapping_add((kind as u64).wrapping_mul(0x94D0_49BB_1331_11EB));
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn hash_unit(x: i32, z: i32, salt: u64) -> f32 {
    let value = journey_target_id(x, z, JourneyPlaceKind::StoneRing).wrapping_add(salt);
    (value as f64 / u64::MAX as f64) as f32
}

fn planar_distance(a: Vec3, b: Vec3) -> f32 {
    Vec2::new(a.x - b.x, a.z - b.z).length()
}

#[cfg(test)]
mod tests {
    use bevy::{
        prelude::{App, AppExtStates, NextState, State, Time, Vec3},
        state::app::StatesPlugin,
    };

    use crate::game::{
        flow::{AppScreen, InGameState},
        journey::{
            JourneyAdvanceContext, JourneyEvent, JourneyMemoryKind, JourneyPlaceKind,
            JourneyPlugin, JourneyStage, JourneyState, JourneyTarget, advance_journey_state,
        },
        signs::OmenKind,
        world::{BiomeKind, WorldGridCoord},
    };

    fn sample_target() -> JourneyTarget {
        JourneyTarget {
            id: 42,
            grid: WorldGridCoord { x: 12, z: -8 },
            position: Vec3::new(64.0, 2.0, -32.0),
            kind: JourneyPlaceKind::AncientTree,
            biome: BiomeKind::Grove,
        }
    }

    fn context(
        delta_seconds: f32,
        distance_to_target: Option<f32>,
        omen_triggered: bool,
    ) -> JourneyAdvanceContext {
        JourneyAdvanceContext {
            delta_seconds,
            player_position: Vec3::new(0.0, 2.0, 0.0),
            distance_to_target,
            omen_triggered,
            current_omen: omen_triggered.then_some(OmenKind::GroveWhisper),
        }
    }

    #[test]
    fn default_journey_state_starts_at_first_arrival() {
        let state = JourneyState::default();

        assert_eq!(state.stage, JourneyStage::FirstArrival);
        assert!(state.target.is_none());
        assert!(state.triggered_omens.is_empty());
        assert!(state.memories.is_empty());
    }

    #[test]
    fn target_selection_moves_journey_into_omen_emergence() {
        let mut state = JourneyState {
            target: Some(sample_target()),
            ..Default::default()
        };

        let events = advance_journey_state(&mut state, context(0.25, Some(78.0), false));

        assert_eq!(state.stage, JourneyStage::OmenEmerging);
        assert_eq!(state.last_distance_to_target, Some(78.0));
        assert!(matches!(
            events.as_slice(),
            [JourneyEvent::StageChanged {
                from: JourneyStage::FirstArrival,
                to: JourneyStage::OmenEmerging,
                ..
            }]
        ));
    }

    #[test]
    fn triggered_omen_is_recorded_and_begins_guidance() {
        let mut state = JourneyState {
            target: Some(sample_target()),
            ..Default::default()
        };
        advance_journey_state(&mut state, context(0.25, Some(78.0), false));

        let events = advance_journey_state(&mut state, context(0.2, Some(72.0), true));

        assert_eq!(state.stage, JourneyStage::BeingDrawn);
        assert_eq!(state.triggered_omens.len(), 1);
        assert_eq!(state.triggered_omens[0].omen, OmenKind::GroveWhisper);
        assert!(events.iter().any(|event| {
            matches!(
                event,
                JourneyEvent::StageChanged {
                    from: JourneyStage::OmenEmerging,
                    to: JourneyStage::BeingDrawn,
                    ..
                }
            )
        }));
    }

    #[test]
    fn stage_sequence_is_deterministic_for_same_inputs() {
        fn run_sequence() -> (JourneyStage, Vec<JourneyEvent>, Vec<JourneyMemoryKind>) {
            let mut state = JourneyState {
                target: Some(sample_target()),
                ..Default::default()
            };
            let mut all_events = Vec::new();

            all_events.extend(advance_journey_state(
                &mut state,
                context(0.25, Some(84.0), false),
            ));
            all_events.extend(advance_journey_state(
                &mut state,
                context(0.2, Some(68.0), true),
            ));
            all_events.extend(advance_journey_state(
                &mut state,
                context(0.4, Some(8.0), true),
            ));
            all_events.extend(advance_journey_state(
                &mut state,
                context(1.4, Some(6.0), true),
            ));
            all_events.extend(advance_journey_state(
                &mut state,
                context(2.2, Some(6.0), true),
            ));

            let memory_kinds = state.memories.iter().map(|memory| memory.kind).collect();
            (state.stage, all_events, memory_kinds)
        }

        let first = run_sequence();
        let second = run_sequence();

        assert_eq!(first, second);
        assert_eq!(first.0, JourneyStage::EchoSettled);
        assert_eq!(
            first.2,
            vec![
                JourneyMemoryKind::Arrival,
                JourneyMemoryKind::Response,
                JourneyMemoryKind::Echo
            ]
        );
    }

    #[test]
    fn journey_does_not_advance_without_target() {
        let mut state = JourneyState::default();

        let events = advance_journey_state(&mut state, context(10.0, None, true));

        assert_eq!(state.stage, JourneyStage::FirstArrival);
        assert!(
            events
                .iter()
                .all(|event| !matches!(event, JourneyEvent::StageChanged { .. }))
        );
    }

    #[test]
    fn journey_resource_follows_in_game_lifecycle() {
        let mut app = App::new();
        app.add_plugins(StatesPlugin);
        app.insert_resource(Time::<()>::default());
        app.init_state::<AppScreen>();
        app.add_sub_state::<InGameState>();
        app.add_plugins(JourneyPlugin);

        app.update();
        assert!(!app.world().contains_resource::<JourneyState>());

        app.world_mut()
            .resource_mut::<NextState<AppScreen>>()
            .set(AppScreen::InGame);
        app.update();
        assert_eq!(
            *app.world().resource::<State<AppScreen>>().get(),
            AppScreen::InGame
        );
        assert!(app.world().contains_resource::<JourneyState>());

        app.world_mut()
            .resource_mut::<NextState<AppScreen>>()
            .set(AppScreen::MainMenu);
        app.update();
        assert_eq!(
            *app.world().resource::<State<AppScreen>>().get(),
            AppScreen::MainMenu
        );
        assert!(!app.world().contains_resource::<JourneyState>());
    }
}
