use bevy::{
    prelude::*,
    tasks::{AsyncComputeTaskPool, Task, futures_lite::future},
};
use serde::{Deserialize, Serialize};

use crate::{
    core::performance::{FramePerformance, PerformancePhase},
    game::{
        ecology::{EcologySignal, EcologyState},
        flow::{AppScreen, InGameState},
        intent::{IntentKind, IntentState},
        journey::{DreamPhase, JourneyState, StoryArcStage},
        landmarks::LandmarkState,
        notebook::NotebookState,
        places::{MeaningfulPlaces, PlaceKind},
        regions::{RegionGraphState, RegionKind},
        signs::OmenKind,
        world::WandererPrototype,
    },
};

pub struct DirectorPlugin;

impl Plugin for DirectorPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppScreen::InGame), initialize_director);
        app.add_systems(
            Update,
            (poll_director_task, submit_director_task)
                .chain()
                .run_if(in_state(InGameState::Running)),
        );
        app.add_systems(OnExit(AppScreen::InGame), cleanup_director);
    }
}

#[derive(Debug, Resource, Clone, PartialEq)]
pub struct DirectorState {
    pub mode: DirectorMode,
    pub last_input: Option<DirectorInput>,
    pub last_output: Option<DirectorOutput>,
    pub last_validation: Option<DirectorValidation>,
    pub request_status: DirectorRequestStatus,
    pub last_request_id: Option<u64>,
    pub last_completed_request_id: Option<u64>,
    elapsed_since_last_run: f32,
}

impl Default for DirectorState {
    fn default() -> Self {
        Self {
            mode: DirectorMode::Deterministic,
            last_input: None,
            last_output: None,
            last_validation: None,
            request_status: DirectorRequestStatus::Idle,
            last_request_id: None,
            last_completed_request_id: None,
            elapsed_since_last_run: 0.0,
        }
    }
}

#[derive(Debug, Resource, Default)]
struct DirectorTaskState {
    next_request_id: u64,
    in_flight: Option<InFlightDirectorRequest>,
}

#[derive(Debug)]
struct InFlightDirectorRequest {
    request_id: u64,
    input: DirectorInput,
    elapsed_seconds: f32,
    task: Task<DirectorOutput>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum DirectorRequestStatus {
    Idle,
    InFlight,
    Completed,
    TimedOut,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum DirectorMode {
    Deterministic,
    AiDisabledFallback,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirectorInput {
    pub player_position: [f32; 3],
    pub current_region: Option<String>,
    pub story_stage: String,
    pub dream_phase: String,
    pub dominant_intent: Option<String>,
    pub known_places: Vec<String>,
    pub notebook_summary: Vec<String>,
    pub pyramid_visible: bool,
    pub ecology_signal: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirectorOutput {
    pub suggestions: Vec<DirectorSuggestion>,
    pub fallback_text: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirectorSuggestion {
    pub kind: DirectorSuggestionKind,
    pub semantic_tags: Vec<String>,
    pub strength: f32,
    pub duration_seconds: f32,
    pub precondition: DirectorPrecondition,
    pub text: String,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum DirectorSuggestionKind {
    Omen,
    Dream,
    Dialogue,
    Place,
    EnvironmentResponse,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize, Deserialize)]
pub enum DirectorPrecondition {
    None,
    DreamAfterglow,
    BoundaryNearby,
    RegionKnown,
    IntentAligned,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DirectorValidation {
    pub accepted: Vec<DirectorSuggestion>,
    pub rejected: Vec<DirectorRejection>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DirectorRejection {
    pub suggestion: DirectorSuggestion,
    pub reason: DirectorRejectionReason,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum DirectorRejectionReason {
    TaskLanguage,
    PreconditionMissing,
    UnknownRegion,
    NoWorldAnchor,
}

const DIRECTOR_INTERVAL_SECONDS: f32 = 2.0;
const DIRECTOR_REQUEST_TIMEOUT_SECONDS: f32 = 1.2;

type DirectorInputResources<'w> = (
    Option<Res<'w, JourneyState>>,
    Option<Res<'w, IntentState>>,
    Option<Res<'w, RegionGraphState>>,
    Option<Res<'w, MeaningfulPlaces>>,
    Option<Res<'w, NotebookState>>,
    Option<Res<'w, LandmarkState>>,
    Option<Res<'w, EcologyState>>,
);

type DirectorValidationResources<'w> = (
    Option<Res<'w, JourneyState>>,
    Option<Res<'w, RegionGraphState>>,
    Option<Res<'w, MeaningfulPlaces>>,
);

pub trait JourneyDirector {
    fn evaluate(&self, input: &DirectorInput) -> DirectorOutput;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct HardcodedJourneyDirector;

impl JourneyDirector for HardcodedJourneyDirector {
    fn evaluate(&self, input: &DirectorInput) -> DirectorOutput {
        deterministic_director(input)
    }
}

fn initialize_director(mut commands: Commands) {
    commands.insert_resource(DirectorState::default());
    commands.insert_resource(DirectorTaskState::default());
}

fn cleanup_director(mut commands: Commands) {
    commands.remove_resource::<DirectorState>();
    commands.remove_resource::<DirectorTaskState>();
}

fn poll_director_task(
    time: Res<Time>,
    director: Option<ResMut<DirectorState>>,
    task_state: Option<ResMut<DirectorTaskState>>,
    resources: DirectorValidationResources<'_>,
    mut performance: ResMut<FramePerformance>,
) {
    let started_at = std::time::Instant::now();
    let Some(mut director) = director else {
        return;
    };
    let Some(mut task_state) = task_state else {
        return;
    };
    let Some(request) = task_state.in_flight.as_mut() else {
        return;
    };

    request.elapsed_seconds += time.delta_secs();
    let result = if request.elapsed_seconds >= DIRECTOR_REQUEST_TIMEOUT_SECONDS {
        let output = HardcodedJourneyDirector.evaluate(&request.input);
        Some((output, DirectorRequestStatus::TimedOut))
    } else {
        future::block_on(future::poll_once(&mut request.task))
            .map(|output| (output, DirectorRequestStatus::Completed))
    };

    let Some((output, status)) = result else {
        performance.record_phase_duration(PerformancePhase::Director, started_at.elapsed());
        return;
    };

    let request = task_state
        .in_flight
        .take()
        .expect("in-flight request should exist after poll result");
    let (journey, regions, places) = resources;
    let validation = validate_director_output(
        &output,
        journey.as_deref(),
        regions.as_deref(),
        places.as_deref(),
    );
    complete_director_request(
        &mut director,
        request.request_id,
        request.input,
        output,
        validation,
        status,
    );
    performance.record_phase_duration(PerformancePhase::Director, started_at.elapsed());
}

fn submit_director_task(
    time: Res<Time>,
    director: Option<ResMut<DirectorState>>,
    task_state: Option<ResMut<DirectorTaskState>>,
    resources: DirectorInputResources<'_>,
    player_query: Query<&Transform, With<WandererPrototype>>,
    mut performance: ResMut<FramePerformance>,
) {
    let started_at = std::time::Instant::now();
    let Some(mut director) = director else {
        return;
    };
    let Some(mut task_state) = task_state else {
        return;
    };
    if task_state.in_flight.is_some() {
        performance.record_phase_duration(PerformancePhase::Director, started_at.elapsed());
        return;
    }
    director.elapsed_since_last_run += time.delta_secs();
    if director.elapsed_since_last_run < DIRECTOR_INTERVAL_SECONDS {
        return;
    }
    director.elapsed_since_last_run = 0.0;
    let Some(player_transform) = player_query.iter().next() else {
        return;
    };
    let (journey, intent, regions, places, notebook, landmarks, ecology) = resources;

    let input = build_director_input(
        player_transform.translation,
        journey.as_deref(),
        intent.as_deref(),
        regions.as_deref(),
        places.as_deref(),
        notebook.as_deref(),
        landmarks.as_deref(),
        ecology.as_deref(),
    );

    let request_id = task_state.next_request_id;
    task_state.next_request_id = task_state.next_request_id.wrapping_add(1).max(1);
    let task_input = input.clone();
    let task = AsyncComputeTaskPool::get().spawn(async move {
        let hardcoded = HardcodedJourneyDirector;
        hardcoded.evaluate(&task_input)
    });
    director.request_status = DirectorRequestStatus::InFlight;
    director.last_request_id = Some(request_id);
    task_state.in_flight = Some(InFlightDirectorRequest {
        request_id,
        input,
        elapsed_seconds: 0.0,
        task,
    });
    tracing::info!(
        target: "dao_game::director",
        request_id,
        mode = ?director.mode,
        "journey director request submitted"
    );
    performance.record_phase_duration(PerformancePhase::Director, started_at.elapsed());
}

fn complete_director_request(
    director: &mut DirectorState,
    request_id: u64,
    input: DirectorInput,
    output: DirectorOutput,
    validation: DirectorValidation,
    status: DirectorRequestStatus,
) {
    tracing::info!(
        target: "dao_game::director",
        request_id,
        mode = ?director.mode,
        status = ?status,
        input_region = input.current_region.as_deref(),
        input_intent = input.dominant_intent.as_deref(),
        suggestions = output.suggestions.len(),
        accepted = validation.accepted.len(),
        rejected = validation.rejected.len(),
        "journey director evaluated suggestions"
    );

    director.last_input = Some(input);
    director.last_output = Some(output);
    director.last_validation = Some(validation);
    director.request_status = status;
    director.last_completed_request_id = Some(request_id);
}

#[allow(clippy::too_many_arguments)]
pub fn build_director_input(
    player_position: Vec3,
    journey: Option<&JourneyState>,
    intent: Option<&IntentState>,
    regions: Option<&RegionGraphState>,
    places: Option<&MeaningfulPlaces>,
    notebook: Option<&NotebookState>,
    landmarks: Option<&LandmarkState>,
    ecology: Option<&EcologyState>,
) -> DirectorInput {
    let current_region = regions
        .and_then(|regions| regions.region(regions.current_region))
        .map(|region| region.kind.label().to_string());
    let known_places = places
        .map(|places| {
            places
                .places
                .iter()
                .take(8)
                .map(|place| place.kind.label().to_string())
                .collect()
        })
        .unwrap_or_default();
    let notebook_summary = notebook
        .map(|notebook| {
            notebook
                .entries
                .iter()
                .rev()
                .take(4)
                .map(|entry| entry.title.clone())
                .collect()
        })
        .unwrap_or_default();
    let story_stage = journey
        .map(|journey| journey.story_stage.label().to_string())
        .unwrap_or_else(|| StoryArcStage::VillageAwakening.label().to_string());
    let dream_phase = journey
        .map(|journey| journey.dream.phase.label().to_string())
        .unwrap_or_else(|| DreamPhase::Unseen.label().to_string());
    let dominant_intent = intent
        .and_then(IntentState::dominant)
        .map(IntentKind::label)
        .map(str::to_string);
    let pyramid_visible = landmarks.is_some_and(|landmarks| landmarks.pyramid_signal.visible);
    let ecology_signal = ecology
        .and_then(|ecology| ecology.latest_signal)
        .map(ecology_signal_label)
        .map(str::to_string);

    DirectorInput {
        player_position: [player_position.x, player_position.y, player_position.z],
        current_region,
        story_stage,
        dream_phase,
        dominant_intent,
        known_places,
        notebook_summary,
        pyramid_visible,
        ecology_signal,
    }
}

pub fn deterministic_director(input: &DirectorInput) -> DirectorOutput {
    let mut suggestions = Vec::new();
    if input.dream_phase == DreamPhase::Afterglow.label()
        && input
            .dominant_intent
            .as_deref()
            .is_some_and(|intent| intent == IntentKind::BeyondVillage.label())
    {
        suggestions.push(DirectorSuggestion {
            kind: DirectorSuggestionKind::Omen,
            semantic_tags: vec!["boundary".to_string(), "dream".to_string()],
            strength: 0.74,
            duration_seconds: 6.0,
            precondition: DirectorPrecondition::BoundaryNearby,
            text: "雾和鸟群可以短暂回应村外方向。".to_string(),
        });
    }
    if input.dream_phase == DreamPhase::Afterglow.label()
        && input
            .dominant_intent
            .as_deref()
            .is_some_and(|intent| intent == IntentKind::DreamLandmark.label())
    {
        suggestions.push(DirectorSuggestion {
            kind: DirectorSuggestionKind::EnvironmentResponse,
            semantic_tags: vec!["desert".to_string(), "pyramid".to_string()],
            strength: 0.82,
            duration_seconds: 4.0,
            precondition: DirectorPrecondition::DreamAfterglow,
            text: "风沙轮廓可以在感知时短暂显形。".to_string(),
        });
    }
    if input.ecology_signal.as_deref()
        == Some(ecology_signal_label(EcologySignal::MerchantDesertRumor))
    {
        suggestions.push(DirectorSuggestion {
            kind: DirectorSuggestionKind::Dialogue,
            semantic_tags: vec!["merchant".to_string(), "desert".to_string()],
            strength: 0.55,
            duration_seconds: 8.0,
            precondition: DirectorPrecondition::IntentAligned,
            text: "商人可以谈起风沙另一侧的旅人传闻。".to_string(),
        });
    }
    if input.story_stage == StoryArcStage::FarBankOutpost.label() {
        suggestions.push(DirectorSuggestion {
            kind: DirectorSuggestionKind::Dialogue,
            semantic_tags: vec![
                "town".to_string(),
                "trade".to_string(),
                "preparation".to_string(),
            ],
            strength: 0.62,
            duration_seconds: 8.0,
            precondition: DirectorPrecondition::RegionKnown,
            text: "对岸摊棚可以留下关于城镇买卖和旅费的闲谈。".to_string(),
        });
        suggestions.push(DirectorSuggestion {
            kind: DirectorSuggestionKind::EnvironmentResponse,
            semantic_tags: vec!["loss".to_string(), "trust".to_string()],
            strength: 0.48,
            duration_seconds: 5.0,
            precondition: DirectorPrecondition::RegionKnown,
            text: "路边空箱和凌乱脚印可以提前埋下失去的影子。".to_string(),
        });
    }
    if input.pyramid_visible {
        suggestions.push(DirectorSuggestion {
            kind: DirectorSuggestionKind::Place,
            semantic_tags: vec!["pyramid".to_string(), "memory".to_string()],
            strength: 0.9,
            duration_seconds: 10.0,
            precondition: DirectorPrecondition::RegionKnown,
            text: "记忆可以把金字塔作为已见过的地标保存。".to_string(),
        });
    }

    DirectorOutput {
        suggestions,
        fallback_text: "没有合适建议时，继续使用确定性旅程与世界规则。".to_string(),
    }
}

pub fn validate_director_output(
    output: &DirectorOutput,
    journey: Option<&JourneyState>,
    regions: Option<&RegionGraphState>,
    places: Option<&MeaningfulPlaces>,
) -> DirectorValidation {
    let mut accepted = Vec::new();
    let mut rejected = Vec::new();
    for suggestion in &output.suggestions {
        let rejection = validate_suggestion(suggestion, journey, regions, places);
        match rejection {
            None => accepted.push(suggestion.clone()),
            Some(reason) => rejected.push(DirectorRejection {
                suggestion: suggestion.clone(),
                reason,
            }),
        }
    }
    DirectorValidation { accepted, rejected }
}

fn validate_suggestion(
    suggestion: &DirectorSuggestion,
    journey: Option<&JourneyState>,
    regions: Option<&RegionGraphState>,
    places: Option<&MeaningfulPlaces>,
) -> Option<DirectorRejectionReason> {
    if contains_task_language(&suggestion.text) {
        return Some(DirectorRejectionReason::TaskLanguage);
    }
    match suggestion.precondition {
        DirectorPrecondition::None => {}
        DirectorPrecondition::DreamAfterglow => {
            if !journey.is_some_and(|journey| journey.dream.phase == DreamPhase::Afterglow) {
                return Some(DirectorRejectionReason::PreconditionMissing);
            }
        }
        DirectorPrecondition::BoundaryNearby => {
            if regions.is_none_or(|regions| regions.nearest_gate.is_none()) {
                return Some(DirectorRejectionReason::NoWorldAnchor);
            }
        }
        DirectorPrecondition::RegionKnown => {
            if !regions.is_some_and(|regions| {
                regions
                    .regions
                    .iter()
                    .any(|region| region.kind == RegionKind::Desert)
            }) {
                return Some(DirectorRejectionReason::UnknownRegion);
            }
        }
        DirectorPrecondition::IntentAligned => {
            if !journey.is_some_and(|journey| journey.dream.phase == DreamPhase::Afterglow) {
                return Some(DirectorRejectionReason::PreconditionMissing);
            }
        }
    }
    if suggestion.kind == DirectorSuggestionKind::Place
        && places.is_some_and(|places| places.places.is_empty())
        && !suggestion
            .semantic_tags
            .iter()
            .any(|tag| tag == "pyramid" || tag == "boundary")
    {
        return Some(DirectorRejectionReason::NoWorldAnchor);
    }
    None
}

fn contains_task_language(text: &str) -> bool {
    ["任务", "必须前往", "完成", "奖励", "目标："]
        .iter()
        .any(|word| text.contains(word))
}

fn ecology_signal_label(signal: EcologySignal) -> &'static str {
    match signal {
        EcologySignal::BirdsTowardBoundary => "鸟群朝向边界",
        EcologySignal::SheepUneasy => "羊群不安",
        EcologySignal::MerchantDesertRumor => "商人沙漠传闻",
        EcologySignal::FortuneTellerLamp => "占卜人的灯",
    }
}

pub fn omen_from_director_tags(tags: &[String]) -> Option<OmenKind> {
    if tags.iter().any(|tag| tag == "desert" || tag == "pyramid") {
        Some(OmenKind::DawnLight)
    } else if tags.iter().any(|tag| tag == "boundary") {
        Some(OmenKind::SummitCall)
    } else if tags.iter().any(|tag| tag == "water") {
        Some(OmenKind::StillWater)
    } else if tags.iter().any(|tag| tag == "grove") {
        Some(OmenKind::GroveWhisper)
    } else {
        None
    }
}

pub fn place_from_director_tags(tags: &[String]) -> Option<PlaceKind> {
    if tags.iter().any(|tag| tag == "water") {
        Some(PlaceKind::QuietBay)
    } else if tags.iter().any(|tag| tag == "boundary") {
        Some(PlaceKind::RidgeGate)
    } else if tags.iter().any(|tag| tag == "memory") {
        Some(PlaceKind::StoneRing)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use crate::game::{
        director::{
            DirectorInput, DirectorPrecondition, DirectorRejectionReason, DirectorSuggestion,
            DirectorSuggestionKind, deterministic_director, validate_director_output,
        },
        journey::{DreamPhase, JourneyState},
        signs::OmenKind,
    };

    #[test]
    fn deterministic_director_suggests_boundary_omen_for_afterglow_intent() {
        let input = DirectorInput {
            player_position: [0.0, 0.0, 0.0],
            current_region: Some("村庄海岸".to_string()),
            story_stage: "梦后回响".to_string(),
            dream_phase: DreamPhase::Afterglow.label().to_string(),
            dominant_intent: Some("向村外".to_string()),
            known_places: Vec::new(),
            notebook_summary: Vec::new(),
            pyramid_visible: false,
            ecology_signal: None,
        };
        let output = deterministic_director(&input);

        assert!(output.suggestions.iter().any(|suggestion| {
            suggestion.kind == DirectorSuggestionKind::Omen
                && suggestion.semantic_tags.iter().any(|tag| tag == "boundary")
        }));
    }

    #[test]
    fn validation_rejects_task_language() {
        let output = crate::game::director::DirectorOutput {
            fallback_text: String::new(),
            suggestions: vec![DirectorSuggestion {
                kind: DirectorSuggestionKind::Omen,
                semantic_tags: vec!["desert".to_string()],
                strength: 1.0,
                duration_seconds: 1.0,
                precondition: DirectorPrecondition::None,
                text: "任务：必须前往沙漠并完成目标。".to_string(),
            }],
        };
        let validation = validate_director_output(&output, None, None, None);

        assert_eq!(validation.accepted.len(), 0);
        assert_eq!(
            validation.rejected[0].reason,
            DirectorRejectionReason::TaskLanguage
        );
    }

    #[test]
    fn afterglow_precondition_is_checked() {
        let output = crate::game::director::DirectorOutput {
            fallback_text: String::new(),
            suggestions: vec![DirectorSuggestion {
                kind: DirectorSuggestionKind::EnvironmentResponse,
                semantic_tags: vec!["pyramid".to_string()],
                strength: 1.0,
                duration_seconds: 1.0,
                precondition: DirectorPrecondition::DreamAfterglow,
                text: "风沙轮廓短暂显形。".to_string(),
            }],
        };
        let mut journey = JourneyState::default();
        journey.dream.phase = DreamPhase::Unseen;
        let rejected = validate_director_output(&output, Some(&journey), None, None);
        journey.dream.phase = DreamPhase::Afterglow;
        let accepted = validate_director_output(&output, Some(&journey), None, None);

        assert_eq!(rejected.accepted.len(), 0);
        assert_eq!(accepted.accepted.len(), 1);
    }

    #[test]
    fn director_tags_can_map_to_existing_omen() {
        assert_eq!(
            crate::game::director::omen_from_director_tags(&["pyramid".to_string()]),
            Some(OmenKind::DawnLight)
        );
    }

    #[test]
    fn deterministic_director_reserves_far_bank_town_and_loss_hooks() {
        let input = DirectorInput {
            player_position: [0.0, 0.0, 0.0],
            current_region: Some("山地边界".to_string()),
            story_stage: crate::game::journey::StoryArcStage::FarBankOutpost
                .label()
                .to_string(),
            dream_phase: DreamPhase::Afterglow.label().to_string(),
            dominant_intent: None,
            known_places: Vec::new(),
            notebook_summary: Vec::new(),
            pyramid_visible: false,
            ecology_signal: None,
        };

        let output = deterministic_director(&input);

        assert!(output.suggestions.iter().any(|suggestion| {
            suggestion.semantic_tags.iter().any(|tag| tag == "trade")
                && !suggestion.text.contains("任务")
        }));
        assert!(output.suggestions.iter().any(|suggestion| {
            suggestion.semantic_tags.iter().any(|tag| tag == "loss")
                && !suggestion.text.contains("任务")
        }));
    }
}
