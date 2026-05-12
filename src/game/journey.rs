use std::time::Instant;

use bevy::prelude::*;

use crate::game::{
    flow::{AppScreen, InGameState},
    places::{MeaningfulPlace, MeaningfulPlaces, PlaceKind, choose_primary_place, planar_distance},
    signs::{OmenKind, SignState},
    world::{BiomeKind, WandererPrototype, WorldCamera, WorldGridCoord},
};

pub type JourneyPlaceKind = PlaceKind;

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
    pub interaction: JourneyInteractionState,
    pub response: JourneyResponseState,
    pub triggered_omens: Vec<JourneyOmenMemory>,
    pub memories: Vec<JourneyMemory>,
    pub session_elapsed: f32,
    pub stage_elapsed: f32,
    pub last_distance_to_target: Option<f32>,
    pub last_player_position: Option<Vec3>,
}

impl Default for JourneyState {
    fn default() -> Self {
        Self {
            stage: JourneyStage::FirstArrival,
            target: None,
            interaction: JourneyInteractionState::default(),
            response: JourneyResponseState::default(),
            triggered_omens: Vec::new(),
            memories: Vec::new(),
            session_elapsed: 0.0,
            stage_elapsed: 0.0,
            last_distance_to_target: None,
            last_player_position: None,
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

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JourneyTarget {
    pub id: u64,
    pub grid: WorldGridCoord,
    pub position: Vec3,
    pub kind: JourneyPlaceKind,
    pub biome: BiomeKind,
    pub arrival_radius: f32,
    pub interaction_radius: f32,
}

impl JourneyTarget {
    fn from_place(place: &MeaningfulPlace) -> Self {
        Self {
            id: place.id,
            grid: place.grid,
            position: place.position,
            kind: place.kind,
            biome: place.biome,
            arrival_radius: place.arrival_radius,
            interaction_radius: place.interaction_radius,
        }
    }
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
    pub interaction: Option<JourneyInteractionKind>,
    pub text: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum JourneyMemoryKind {
    Arrival,
    Response,
    Echo,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum JourneyInteractionKind {
    Stay,
    Gaze,
    Listen,
}

impl JourneyInteractionKind {
    pub fn label(self) -> &'static str {
        match self {
            Self::Stay => "停留",
            Self::Gaze => "注视",
            Self::Listen => "聆听",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JourneyInteractionState {
    pub near_target: bool,
    pub stay_seconds: f32,
    pub gaze_seconds: f32,
    pub listen_seconds: f32,
    pub completed: bool,
    pub completed_kind: Option<JourneyInteractionKind>,
}

impl Default for JourneyInteractionState {
    fn default() -> Self {
        Self {
            near_target: false,
            stay_seconds: 0.0,
            gaze_seconds: 0.0,
            listen_seconds: 0.0,
            completed: false,
            completed_kind: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct JourneyResponseState {
    pub active: bool,
    pub elapsed_seconds: f32,
    pub duration_seconds: f32,
    pub intensity: f32,
    pub place_id: Option<u64>,
    pub place_kind: Option<JourneyPlaceKind>,
}

impl Default for JourneyResponseState {
    fn default() -> Self {
        Self {
            active: false,
            elapsed_seconds: 0.0,
            duration_seconds: RESPONSE_DURATION_SECONDS,
            intensity: 0.0,
            place_id: None,
            place_kind: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum JourneyEvent {
    StageChanged {
        from: JourneyStage,
        to: JourneyStage,
        at_seconds: f32,
    },
    InteractionCompleted {
        kind: JourneyInteractionKind,
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
    pub target_arrival_radius: Option<f32>,
    pub target_interaction_radius: Option<f32>,
    pub look_alignment_to_target: Option<f32>,
    pub calm: f32,
    pub omen_triggered: bool,
    pub current_omen: Option<OmenKind>,
}

const DEFAULT_ARRIVAL_RADIUS: f32 = 13.5;
const DEFAULT_INTERACTION_RADIUS: f32 = 7.5;
const OMEN_FALLBACK_SECONDS: f32 = 4.0;
const STILL_SPEED_THRESHOLD: f32 = 0.42;
const STAY_INTERACTION_SECONDS: f32 = 1.25;
const GAZE_INTERACTION_SECONDS: f32 = 0.85;
const LISTEN_INTERACTION_SECONDS: f32 = 1.1;
const GAZE_ALIGNMENT_THRESHOLD: f32 = 0.82;
const LISTEN_CALM_THRESHOLD: f32 = 0.72;
const RESPONSE_DURATION_SECONDS: f32 = 7.5;
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
    places: Option<Res<MeaningfulPlaces>>,
    wanderer_query: Query<&Transform, With<WandererPrototype>>,
) {
    let Some(mut journey) = journey else {
        return;
    };
    if journey.target.is_some() {
        return;
    }

    let Some(places) = places else {
        return;
    };
    let Some(transform) = wanderer_query.iter().next() else {
        return;
    };

    let started_at = Instant::now();
    match choose_primary_place(&places, transform.translation).map(JourneyTarget::from_place) {
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
    camera_query: Query<&Transform, (With<WorldCamera>, Without<WandererPrototype>)>,
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
    let target_arrival_radius = journey.target.map(|target| target.arrival_radius);
    let target_interaction_radius = journey.target.map(|target| target.interaction_radius);
    let look_alignment_to_target = journey
        .target
        .map(|target| match camera_query.iter().next() {
            Some(camera_transform) => look_alignment(camera_transform, target.position),
            None => look_alignment(transform, target.position),
        });
    let sign_state = signs.as_deref();
    let events = advance_journey_state(
        &mut journey,
        JourneyAdvanceContext {
            delta_seconds: time.delta_secs(),
            player_position: transform.translation,
            distance_to_target,
            target_arrival_radius,
            target_interaction_radius,
            look_alignment_to_target,
            calm: sign_state.map_or(1.0, |signs| signs.calm),
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
    let player_speed = state
        .last_player_position
        .map(|last| planar_distance(last, context.player_position) / delta_seconds.max(0.001))
        .unwrap_or(0.0);
    state.last_player_position = Some(context.player_position);
    advance_response_state(&mut state.response, delta_seconds);

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
            let arrival_radius = context
                .target_arrival_radius
                .unwrap_or(DEFAULT_ARRIVAL_RADIUS);
            if context
                .distance_to_target
                .is_some_and(|distance| distance <= arrival_radius)
            {
                transition_stage(state, JourneyStage::PlaceReached, &mut events);
                record_journey_memory(state, JourneyMemoryKind::Arrival, context, &mut events);
            }
        }
        JourneyStage::PlaceReached => {
            if let Some(interaction) =
                advance_interaction_state(&mut state.interaction, context, player_speed)
            {
                events.push(JourneyEvent::InteractionCompleted {
                    kind: interaction,
                    at_seconds: state.session_elapsed,
                });
                transition_stage(state, JourneyStage::WorldResponded, &mut events);
                start_response_state(&mut state.response, state.target);
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
        interaction: state.interaction.completed_kind,
        text: memory_text(
            kind,
            state.target.map(|target| target.kind),
            state.interaction.completed_kind,
        ),
    };
    push_bounded(&mut state.memories, memory.clone(), MAX_JOURNEY_MEMORIES);
    events.push(JourneyEvent::MemoryRecorded(memory));
}

fn advance_interaction_state(
    interaction: &mut JourneyInteractionState,
    context: JourneyAdvanceContext,
    player_speed: f32,
) -> Option<JourneyInteractionKind> {
    let interaction_radius = context
        .target_interaction_radius
        .unwrap_or(DEFAULT_INTERACTION_RADIUS);
    let near_target = context
        .distance_to_target
        .is_some_and(|distance| distance <= interaction_radius);
    interaction.near_target = near_target;

    if !near_target {
        if !interaction.completed {
            interaction.stay_seconds = 0.0;
            interaction.gaze_seconds = 0.0;
            interaction.listen_seconds = 0.0;
        }
        return None;
    }
    if interaction.completed {
        return None;
    }

    let delta_seconds = context.delta_seconds.max(0.0);
    let is_still = player_speed <= STILL_SPEED_THRESHOLD;
    if is_still {
        interaction.stay_seconds += delta_seconds;
    } else {
        interaction.stay_seconds = 0.0;
    }

    if context
        .look_alignment_to_target
        .is_some_and(|alignment| alignment >= GAZE_ALIGNMENT_THRESHOLD)
    {
        interaction.gaze_seconds += delta_seconds;
    } else {
        interaction.gaze_seconds = 0.0;
    }

    if is_still && context.calm >= LISTEN_CALM_THRESHOLD {
        interaction.listen_seconds += delta_seconds;
    } else {
        interaction.listen_seconds = 0.0;
    }

    let completed = if interaction.gaze_seconds >= GAZE_INTERACTION_SECONDS {
        Some(JourneyInteractionKind::Gaze)
    } else if interaction.listen_seconds >= LISTEN_INTERACTION_SECONDS {
        Some(JourneyInteractionKind::Listen)
    } else if interaction.stay_seconds >= STAY_INTERACTION_SECONDS {
        Some(JourneyInteractionKind::Stay)
    } else {
        None
    };

    if let Some(kind) = completed {
        interaction.completed = true;
        interaction.completed_kind = Some(kind);
        return Some(kind);
    }
    None
}

fn start_response_state(response: &mut JourneyResponseState, target: Option<JourneyTarget>) {
    response.active = true;
    response.elapsed_seconds = 0.0;
    response.duration_seconds = RESPONSE_DURATION_SECONDS;
    response.intensity = 1.0;
    response.place_id = target.map(|target| target.id);
    response.place_kind = target.map(|target| target.kind);
}

fn advance_response_state(response: &mut JourneyResponseState, delta_seconds: f32) {
    if !response.active {
        return;
    }
    response.elapsed_seconds += delta_seconds;
    response.intensity =
        response_intensity(response.elapsed_seconds, response.duration_seconds).clamp(0.0, 1.0);
    if response.elapsed_seconds >= response.duration_seconds {
        response.active = false;
        response.intensity = 0.0;
    }
}

fn response_intensity(elapsed_seconds: f32, duration_seconds: f32) -> f32 {
    let duration = duration_seconds.max(0.001);
    let fade_in = smoothstep_unit((elapsed_seconds / 1.2).clamp(0.0, 1.0));
    let fade_out =
        1.0 - smoothstep_unit(((elapsed_seconds - (duration - 1.6)) / 1.6).clamp(0.0, 1.0));
    fade_in.min(fade_out).max(0.0)
}

fn should_record_omen(state: &JourneyState, omen: OmenKind) -> bool {
    state
        .triggered_omens
        .last()
        .is_none_or(|memory| memory.omen != omen)
}

fn memory_text(
    kind: JourneyMemoryKind,
    place_kind: Option<JourneyPlaceKind>,
    interaction: Option<JourneyInteractionKind>,
) -> String {
    let place = place_kind
        .map(JourneyPlaceKind::label)
        .unwrap_or("未名之地");
    match kind {
        JourneyMemoryKind::Arrival => format!("你抵达了{place}。"),
        JourneyMemoryKind::Response => match interaction {
            Some(interaction) => format!("{place}因你的{}回应。", interaction.label()),
            None => format!("{place}回应了你的停留。"),
        },
        JourneyMemoryKind::Echo => format!("{place}的回响沉入记忆。"),
    }
}

pub fn format_journey_memory_line(memory: &JourneyMemory) -> String {
    let total_seconds = memory.at_seconds.max(0.0).floor() as u32;
    let minutes = total_seconds / 60;
    let seconds = total_seconds % 60;
    format!("{minutes:02}:{seconds:02} {}", memory.text)
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
        JourneyEvent::InteractionCompleted { kind, at_seconds } => {
            tracing::info!(
                target: "dao_game::journey::interaction",
                interaction = kind.label(),
                at_seconds,
                "journey light interaction completed"
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

fn look_alignment(transform: &Transform, target_position: Vec3) -> f32 {
    let to_target = Vec3::new(
        target_position.x - transform.translation.x,
        0.0,
        target_position.z - transform.translation.z,
    )
    .normalize_or_zero();
    if to_target == Vec3::ZERO {
        return 1.0;
    }
    let forward = transform.forward();
    let forward = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    forward.dot(to_target).clamp(-1.0, 1.0)
}

fn smoothstep_unit(value: f32) -> f32 {
    let t = value.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
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
            JourneyAdvanceContext, JourneyEvent, JourneyInteractionKind, JourneyMemoryKind,
            JourneyPlaceKind, JourneyPlugin, JourneyStage, JourneyState, JourneyTarget,
            advance_journey_state, format_journey_memory_line,
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
            arrival_radius: 13.5,
            interaction_radius: 7.5,
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
            target_arrival_radius: Some(13.5),
            target_interaction_radius: Some(7.5),
            look_alignment_to_target: Some(0.9),
            calm: 0.8,
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
                context(0.9, Some(6.0), true),
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
    fn place_reached_waits_for_light_interaction_before_response() {
        let mut state = JourneyState {
            target: Some(sample_target()),
            ..Default::default()
        };
        advance_journey_state(&mut state, context(0.25, Some(84.0), false));
        advance_journey_state(&mut state, context(0.2, Some(68.0), true));
        advance_journey_state(&mut state, context(0.4, Some(8.0), true));

        let before_interaction = advance_journey_state(&mut state, context(0.3, Some(6.0), true));

        assert_eq!(state.stage, JourneyStage::PlaceReached);
        assert!(!state.interaction.completed);
        assert!(!state.response.active);
        assert!(before_interaction.iter().all(|event| {
            !matches!(
                event,
                JourneyEvent::StageChanged {
                    to: JourneyStage::WorldResponded,
                    ..
                }
            )
        }));
    }

    #[test]
    fn gaze_interaction_triggers_world_response_memory() {
        let mut state = JourneyState {
            target: Some(sample_target()),
            ..Default::default()
        };
        advance_journey_state(&mut state, context(0.25, Some(84.0), false));
        advance_journey_state(&mut state, context(0.2, Some(68.0), true));
        advance_journey_state(&mut state, context(0.4, Some(8.0), true));

        let events = advance_journey_state(&mut state, context(0.9, Some(6.0), true));

        assert_eq!(state.stage, JourneyStage::WorldResponded);
        assert!(state.response.active);
        assert_eq!(
            state.interaction.completed_kind,
            Some(JourneyInteractionKind::Gaze)
        );
        assert!(events.iter().any(|event| {
            matches!(
                event,
                JourneyEvent::InteractionCompleted {
                    kind: JourneyInteractionKind::Gaze,
                    ..
                }
            )
        }));
        assert!(state.memories.iter().any(
            |memory| memory.kind == JourneyMemoryKind::Response && memory.text.contains("注视")
        ));
    }

    #[test]
    fn journey_memory_line_formats_elapsed_time_without_task_language() {
        let memory = super::JourneyMemory {
            kind: JourneyMemoryKind::Response,
            stage: JourneyStage::WorldResponded,
            at_seconds: 75.4,
            position: Vec3::ZERO,
            place_kind: Some(JourneyPlaceKind::StoneRing),
            omen: Some(OmenKind::DawnLight),
            interaction: Some(JourneyInteractionKind::Stay),
            text: "石阵因你的停留回应。".to_string(),
        };

        assert_eq!(
            format_journey_memory_line(&memory),
            "01:15 石阵因你的停留回应。"
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
