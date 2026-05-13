use std::time::Instant;

use bevy::prelude::*;

use crate::game::{
    director::{DirectorState, DirectorSuggestionKind, place_from_director_tags},
    flow::{AppScreen, InGameState},
    notebook::{
        NotebookEntryKind, NotebookRecord, NotebookSource, NotebookState, NotebookTag,
        dream_record, record_notebook_entry,
    },
    places::{MeaningfulPlace, MeaningfulPlaces, PlaceKind, choose_primary_place, planar_distance},
    signs::{OmenKind, SignState},
    village::{HerdingPhase, VillageAreaKind, VillageState},
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
    pub story_stage: StoryArcStage,
    pub story_elapsed: f32,
    pub village_day: u32,
    pub dream: DreamState,
    pub long_term_goal: Option<LongTermGoal>,
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
            story_stage: StoryArcStage::VillageAwakening,
            story_elapsed: 0.0,
            village_day: 1,
            dream: DreamState::default(),
            long_term_goal: None,
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

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum StoryArcStage {
    VillageAwakening,
    VillageLife,
    DreamApproaching,
    Dreaming,
    DreamAfterglow,
}

impl StoryArcStage {
    pub fn label(self) -> &'static str {
        match self {
            Self::VillageAwakening => "村庄醒来",
            Self::VillageLife => "村庄生活",
            Self::DreamApproaching => "梦境将至",
            Self::Dreaming => "梦中沙暴",
            Self::DreamAfterglow => "梦后回响",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct DreamState {
    pub phase: DreamPhase,
    pub phase_elapsed: f32,
    pub seen_pyramid: bool,
    pub echo_strength: f32,
}

impl Default for DreamState {
    fn default() -> Self {
        Self {
            phase: DreamPhase::Unseen,
            phase_elapsed: 0.0,
            seen_pyramid: false,
            echo_strength: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum DreamPhase {
    Unseen,
    Ready,
    InDream,
    Afterglow,
}

impl DreamPhase {
    pub fn label(self) -> &'static str {
        match self {
            Self::Unseen => "未入梦",
            Self::Ready => "梦将至",
            Self::InDream => "梦中",
            Self::Afterglow => "梦后",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum LongTermGoal {
    DesertPyramid,
}

impl LongTermGoal {
    pub fn label(self) -> &'static str {
        match self {
            Self::DesertPyramid => "沙漠金字塔",
        }
    }
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
    Dream,
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
    StoryStageChanged {
        from: StoryArcStage,
        to: StoryArcStage,
        at_seconds: f32,
    },
    DreamChanged {
        from: DreamPhase,
        to: DreamPhase,
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
    pub village_focus: bool,
    pub leaving_village: bool,
    pub herding_completed: bool,
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
const VILLAGE_LIFE_SECONDS: f32 = 9.0;
const DREAM_READY_SECONDS: f32 = 5.0;
const DREAM_DURATION_SECONDS: f32 = 5.8;
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
    director: Option<Res<DirectorState>>,
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
    match choose_journey_place(&places, transform.translation, director.as_deref())
        .map(JourneyTarget::from_place)
    {
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
                director_request_id = director.as_deref().and_then(|director| director.last_completed_request_id),
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
    village: Option<Res<VillageState>>,
    journey: Option<ResMut<JourneyState>>,
    mut notebook: Option<ResMut<NotebookState>>,
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
    let village_context = village_context(village.as_deref(), transform.translation);
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
            village_focus: village_context.0,
            leaving_village: village_context.1,
            herding_completed: village
                .as_deref()
                .is_some_and(|village| village.herding.first_task_completed),
        },
    );

    for event in &events {
        log_journey_event(event);
        if let Some(record) = notebook_record_for_event(event) {
            let _ = record_notebook_entry(notebook.as_deref_mut(), record);
        }
    }
}

fn choose_journey_place<'a>(
    places: &'a MeaningfulPlaces,
    player_position: Vec3,
    director: Option<&DirectorState>,
) -> Option<&'a MeaningfulPlace> {
    let directed_kind = director
        .and_then(|director| director.last_validation.as_ref())
        .and_then(|validation| {
            validation
                .accepted
                .iter()
                .filter(|suggestion| suggestion.kind == DirectorSuggestionKind::Place)
                .max_by(|left, right| left.strength.total_cmp(&right.strength))
        })
        .and_then(|suggestion| place_from_director_tags(&suggestion.semantic_tags));

    if let Some(kind) = directed_kind
        && let Some(place) = nearest_place_of_kind(places, player_position, kind)
    {
        return Some(place);
    }

    choose_primary_place(places, player_position)
}

fn nearest_place_of_kind(
    places: &MeaningfulPlaces,
    player_position: Vec3,
    kind: PlaceKind,
) -> Option<&MeaningfulPlace> {
    places
        .places
        .iter()
        .filter(|place| place.kind == kind)
        .min_by(|left, right| {
            planar_distance(player_position, left.position)
                .total_cmp(&planar_distance(player_position, right.position))
        })
}

pub fn advance_journey_state(
    state: &mut JourneyState,
    context: JourneyAdvanceContext,
) -> Vec<JourneyEvent> {
    let mut events = Vec::new();
    let delta_seconds = context.delta_seconds.max(0.0);
    state.session_elapsed += delta_seconds;
    state.stage_elapsed += delta_seconds;
    state.story_elapsed += delta_seconds;
    state.dream.phase_elapsed += delta_seconds;
    state.village_day = village_day_for_elapsed(state.session_elapsed);
    let player_speed = state
        .last_player_position
        .map(|last| planar_distance(last, context.player_position) / delta_seconds.max(0.001))
        .unwrap_or(0.0);
    state.last_player_position = Some(context.player_position);
    advance_response_state(&mut state.response, delta_seconds);
    advance_story_arc_state(state, context, &mut events);

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

fn transition_story_stage(
    state: &mut JourneyState,
    next: StoryArcStage,
    events: &mut Vec<JourneyEvent>,
) {
    if state.story_stage == next {
        return;
    }
    let previous = state.story_stage;
    state.story_stage = next;
    state.story_elapsed = 0.0;
    events.push(JourneyEvent::StoryStageChanged {
        from: previous,
        to: next,
        at_seconds: state.session_elapsed,
    });
}

fn transition_dream_phase(
    state: &mut JourneyState,
    next: DreamPhase,
    events: &mut Vec<JourneyEvent>,
) {
    if state.dream.phase == next {
        return;
    }
    let previous = state.dream.phase;
    state.dream.phase = next;
    state.dream.phase_elapsed = 0.0;
    if next == DreamPhase::InDream {
        state.dream.seen_pyramid = true;
    }
    if next == DreamPhase::Afterglow {
        state.long_term_goal = Some(LongTermGoal::DesertPyramid);
        state.dream.echo_strength = 1.0;
        record_dream_memory(state);
    }
    events.push(JourneyEvent::DreamChanged {
        from: previous,
        to: next,
        at_seconds: state.session_elapsed,
    });
}

fn advance_story_arc_state(
    state: &mut JourneyState,
    context: JourneyAdvanceContext,
    events: &mut Vec<JourneyEvent>,
) {
    state.dream.echo_strength = match state.dream.phase {
        DreamPhase::Afterglow => {
            let drift = if context.leaving_village {
                0.035
            } else if context.village_focus {
                -0.04
            } else {
                -0.022
            };
            (state.dream.echo_strength + drift * context.delta_seconds).clamp(0.0, 1.0)
        }
        DreamPhase::InDream => 1.0,
        DreamPhase::Ready => 0.45,
        DreamPhase::Unseen => 0.0,
    };

    match state.story_stage {
        StoryArcStage::VillageAwakening => {
            if state.story_elapsed >= 2.0 || context.village_focus {
                transition_story_stage(state, StoryArcStage::VillageLife, events);
            }
        }
        StoryArcStage::VillageLife => {
            if (context.herding_completed && state.story_elapsed >= 4.0)
                || state.story_elapsed >= VILLAGE_LIFE_SECONDS
                || state.village_day >= 2
            {
                transition_story_stage(state, StoryArcStage::DreamApproaching, events);
                transition_dream_phase(state, DreamPhase::Ready, events);
            }
        }
        StoryArcStage::DreamApproaching => {
            if state.dream.phase_elapsed >= DREAM_READY_SECONDS
                || (context.leaving_village && context.herding_completed)
            {
                transition_story_stage(state, StoryArcStage::Dreaming, events);
                transition_dream_phase(state, DreamPhase::InDream, events);
            }
        }
        StoryArcStage::Dreaming => {
            if state.dream.phase_elapsed >= DREAM_DURATION_SECONDS {
                transition_story_stage(state, StoryArcStage::DreamAfterglow, events);
                transition_dream_phase(state, DreamPhase::Afterglow, events);
            }
        }
        StoryArcStage::DreamAfterglow => {}
    }
}

fn record_dream_memory(state: &mut JourneyState) {
    let memory = JourneyMemory {
        kind: JourneyMemoryKind::Dream,
        stage: state.stage,
        at_seconds: state.session_elapsed,
        position: state.last_player_position.unwrap_or(Vec3::ZERO),
        place_kind: None,
        omen: Some(OmenKind::DawnLight),
        interaction: None,
        text: "梦里有沙暴、巨大金字塔，以及金字塔下的宝藏。".to_string(),
    };
    push_bounded(&mut state.memories, memory, MAX_JOURNEY_MEMORIES);
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
        JourneyMemoryKind::Dream => "梦里有沙暴、巨大金字塔，以及金字塔下的宝藏。".to_string(),
    }
}

fn notebook_record_for_event(event: &JourneyEvent) -> Option<NotebookRecord> {
    match event {
        JourneyEvent::DreamChanged {
            to: DreamPhase::Afterglow,
            at_seconds,
            ..
        } => Some(dream_record(*at_seconds)),
        JourneyEvent::OmenRecorded(memory) => Some(NotebookRecord {
            kind: NotebookEntryKind::Sign,
            at_seconds: memory.at_seconds,
            location: Some("途中".to_string()),
            source: NotebookSource::Sign,
            title: format!("{}曾经显现", omen_memory_label(memory.omen)),
            body: format!(
                "{}在光、风或水面里短暂出现，随后又回到世界本来的声音中。",
                omen_memory_label(memory.omen)
            ),
            tags: vec![NotebookTag::Omen, NotebookTag::Memory],
        }),
        JourneyEvent::MemoryRecorded(memory) => Some(notebook_record_for_journey_memory(memory)),
        _ => None,
    }
}

fn notebook_record_for_journey_memory(memory: &JourneyMemory) -> NotebookRecord {
    let place = memory
        .place_kind
        .map(JourneyPlaceKind::label)
        .unwrap_or("未名之地");
    let (kind, source, title) = match memory.kind {
        JourneyMemoryKind::Arrival => (
            NotebookEntryKind::Place,
            NotebookSource::PlaceArrival,
            format!("抵达{place}"),
        ),
        JourneyMemoryKind::Response => (
            NotebookEntryKind::JourneyEcho,
            NotebookSource::Journey,
            format!("{place}的回应"),
        ),
        JourneyMemoryKind::Echo => (
            NotebookEntryKind::JourneyEcho,
            NotebookSource::Journey,
            format!("{place}的回响"),
        ),
        JourneyMemoryKind::Dream => (
            NotebookEntryKind::Dream,
            NotebookSource::Dream,
            "沙暴中的金字塔".to_string(),
        ),
    };
    NotebookRecord {
        kind,
        at_seconds: memory.at_seconds,
        location: Some(place.to_string()),
        source,
        title,
        body: memory.text.clone(),
        tags: notebook_tags_for_memory(memory),
    }
}

fn notebook_tags_for_memory(memory: &JourneyMemory) -> Vec<NotebookTag> {
    let mut tags = vec![NotebookTag::Memory];
    if let Some(omen) = memory.omen {
        tags.push(NotebookTag::Omen);
        if omen == OmenKind::DawnLight {
            tags.push(NotebookTag::Dream);
        }
    }
    match memory.place_kind {
        Some(PlaceKind::SpringEye | PlaceKind::QuietBay) => tags.push(NotebookTag::Sea),
        Some(PlaceKind::StoneRing) => tags.push(NotebookTag::Dream),
        _ => {}
    }
    tags
}

fn omen_memory_label(omen: OmenKind) -> &'static str {
    match omen {
        OmenKind::DawnLight => "曙光",
        OmenKind::GroveWhisper => "林语",
        OmenKind::SummitCall => "山鸣",
        OmenKind::StillWater => "止水",
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
        JourneyEvent::StoryStageChanged {
            from,
            to,
            at_seconds,
        } => {
            tracing::info!(
                target: "dao_game::journey::story",
                from = from.label(),
                to = to.label(),
                at_seconds,
                "story arc stage advanced"
            );
        }
        JourneyEvent::DreamChanged {
            from,
            to,
            at_seconds,
        } => {
            tracing::info!(
                target: "dao_game::journey::dream",
                from = from.label(),
                to = to.label(),
                at_seconds,
                "dream phase changed"
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

fn village_day_for_elapsed(session_elapsed: f32) -> u32 {
    (session_elapsed.max(0.0) / 24.0).floor() as u32 + 1
}

fn village_context(village: Option<&VillageState>, player_position: Vec3) -> (bool, bool) {
    let Some(village) = village else {
        return (false, false);
    };
    let village_focus = village
        .areas
        .iter()
        .filter(|area| {
            matches!(
                area.kind,
                VillageAreaKind::Shore
                    | VillageAreaKind::SheepPen
                    | VillageAreaKind::Market
                    | VillageAreaKind::Well
            )
        })
        .any(|area| planar_distance(player_position, area.position) <= area.radius);
    let village_focus = village_focus
        || matches!(
            village.herding.phase,
            HerdingPhase::FollowingToGrass
                | HerdingPhase::GrazingAtPatch
                | HerdingPhase::ReturningToPen
        );
    let leaving_village = village
        .area(VillageAreaKind::OuterPath)
        .is_some_and(|area| planar_distance(player_position, area.position) <= area.radius);
    (village_focus, leaving_village)
}

#[cfg(test)]
mod tests {
    use bevy::{
        prelude::{App, AppExtStates, NextState, State, Time, Vec3},
        state::app::StatesPlugin,
    };

    use crate::game::{
        director::{DirectorState, DirectorSuggestion, DirectorSuggestionKind, DirectorValidation},
        flow::{AppScreen, InGameState},
        journey::{
            DreamPhase, JourneyAdvanceContext, JourneyEvent, JourneyInteractionKind,
            JourneyMemoryKind, JourneyOmenMemory, JourneyPlaceKind, JourneyPlugin, JourneyStage,
            JourneyState, JourneyTarget, StoryArcStage, advance_journey_state,
            choose_journey_place, format_journey_memory_line, notebook_record_for_event,
        },
        notebook::{NotebookEntryKind, NotebookSource},
        places::{MeaningfulPlace, MeaningfulPlaces, PlaceKind, PlaceTag},
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
            village_focus: false,
            leaving_village: false,
            herding_completed: false,
        }
    }

    #[test]
    fn default_journey_state_starts_at_first_arrival() {
        let state = JourneyState::default();

        assert_eq!(state.stage, JourneyStage::FirstArrival);
        assert_eq!(state.story_stage, StoryArcStage::VillageAwakening);
        assert_eq!(state.dream.phase, DreamPhase::Unseen);
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
    fn journey_memory_events_sync_to_notebook_records() {
        let event = JourneyEvent::MemoryRecorded(super::JourneyMemory {
            kind: JourneyMemoryKind::Arrival,
            stage: JourneyStage::PlaceReached,
            at_seconds: 31.0,
            position: Vec3::new(1.0, 0.0, 2.0),
            place_kind: Some(JourneyPlaceKind::QuietBay),
            omen: Some(OmenKind::StillWater),
            interaction: None,
            text: "你抵达了静水湾。".to_string(),
        });

        let record = notebook_record_for_event(&event).expect("notebook record");

        assert_eq!(record.kind, NotebookEntryKind::Place);
        assert_eq!(record.source, NotebookSource::PlaceArrival);
        assert_eq!(record.title, "抵达静水湾");
        assert!(!record.body.contains("任务"));
    }

    #[test]
    fn omen_events_sync_to_sign_notebook_records() {
        let event = JourneyEvent::OmenRecorded(JourneyOmenMemory {
            omen: OmenKind::SummitCall,
            at_seconds: 8.0,
            position: Vec3::ZERO,
        });

        let record = notebook_record_for_event(&event).expect("notebook record");

        assert_eq!(record.kind, NotebookEntryKind::Sign);
        assert_eq!(record.source, NotebookSource::Sign);
        assert!(record.title.contains("山鸣"));
    }

    #[test]
    fn director_place_suggestion_can_shape_journey_target_selection() {
        let places = MeaningfulPlaces {
            places: vec![
                MeaningfulPlace {
                    id: 1,
                    kind: PlaceKind::QuietBay,
                    grid: WorldGridCoord { x: 1, z: 1 },
                    position: Vec3::new(4.0, 0.0, 0.0),
                    biome: BiomeKind::Water,
                    tags: vec![PlaceTag::Water],
                    score: 0.9,
                    arrival_radius: 12.0,
                    interaction_radius: 6.0,
                },
                MeaningfulPlace {
                    id: 2,
                    kind: PlaceKind::StoneRing,
                    grid: WorldGridCoord { x: 10, z: 1 },
                    position: Vec3::new(80.0, 0.0, 0.0),
                    biome: BiomeKind::Meadow,
                    tags: vec![PlaceTag::Memory],
                    score: 0.7,
                    arrival_radius: 12.0,
                    interaction_radius: 6.0,
                },
            ],
            active_place_id: None,
            nearest_place_id: None,
            nearest_distance: None,
        };
        let mut director = DirectorState::default();
        director.last_validation = Some(DirectorValidation {
            accepted: vec![DirectorSuggestion {
                kind: DirectorSuggestionKind::Place,
                semantic_tags: vec!["memory".to_string()],
                strength: 0.9,
                duration_seconds: 4.0,
                precondition: crate::game::director::DirectorPrecondition::None,
                text: "记忆可以停在石阵附近。".to_string(),
            }],
            rejected: Vec::new(),
        });

        let place = choose_journey_place(&places, Vec3::ZERO, Some(&director)).expect("place");

        assert_eq!(place.kind, PlaceKind::StoneRing);
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
    fn story_arc_reaches_dream_after_village_life() {
        let mut state = JourneyState::default();

        let events = advance_journey_state(
            &mut state,
            JourneyAdvanceContext {
                delta_seconds: 2.1,
                village_focus: true,
                ..context(0.0, None, false)
            },
        );
        assert_eq!(state.story_stage, StoryArcStage::VillageLife);
        assert!(events.iter().any(|event| {
            matches!(
                event,
                JourneyEvent::StoryStageChanged {
                    to: StoryArcStage::VillageLife,
                    ..
                }
            )
        }));

        advance_journey_state(
            &mut state,
            JourneyAdvanceContext {
                delta_seconds: 9.2,
                ..context(0.0, None, false)
            },
        );
        assert_eq!(state.story_stage, StoryArcStage::DreamApproaching);
        assert_eq!(state.dream.phase, DreamPhase::Ready);

        advance_journey_state(
            &mut state,
            JourneyAdvanceContext {
                delta_seconds: 5.1,
                ..context(0.0, None, false)
            },
        );
        assert_eq!(state.story_stage, StoryArcStage::Dreaming);
        assert_eq!(state.dream.phase, DreamPhase::InDream);

        advance_journey_state(
            &mut state,
            JourneyAdvanceContext {
                delta_seconds: 6.0,
                ..context(0.0, None, false)
            },
        );
        assert_eq!(state.story_stage, StoryArcStage::DreamAfterglow);
        assert_eq!(state.dream.phase, DreamPhase::Afterglow);
        assert!(state.long_term_goal.is_some());
        assert!(
            state
                .memories
                .iter()
                .any(|memory| memory.kind == JourneyMemoryKind::Dream)
        );
    }

    #[test]
    fn dream_afterglow_recovers_when_player_tries_to_leave() {
        let mut state = JourneyState {
            story_stage: StoryArcStage::DreamAfterglow,
            dream: super::DreamState {
                phase: DreamPhase::Afterglow,
                echo_strength: 0.36,
                ..Default::default()
            },
            ..Default::default()
        };

        advance_journey_state(
            &mut state,
            JourneyAdvanceContext {
                delta_seconds: 4.0,
                leaving_village: true,
                ..context(0.0, None, false)
            },
        );

        assert!(state.dream.echo_strength > 0.36);
    }

    #[test]
    fn dream_afterglow_fades_when_player_only_stays_in_village() {
        let mut state = JourneyState {
            story_stage: StoryArcStage::DreamAfterglow,
            dream: super::DreamState {
                phase: DreamPhase::Afterglow,
                echo_strength: 0.6,
                ..Default::default()
            },
            ..Default::default()
        };

        advance_journey_state(
            &mut state,
            JourneyAdvanceContext {
                delta_seconds: 4.0,
                village_focus: true,
                ..context(0.0, None, false)
            },
        );

        assert!(state.dream.echo_strength < 0.6);
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
