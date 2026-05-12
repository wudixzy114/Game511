use bevy::prelude::*;

use crate::game::{
    flow::{AppScreen, InGameState},
    journey::{DreamPhase, JourneyState, StoryArcStage},
    notebook::{
        NotebookEntryKind, NotebookRecord, NotebookSource, NotebookState, NotebookTag,
        record_notebook_entry,
    },
    places::{PlaceKind, planar_distance},
    signs::SignState,
    village::{VillageAreaKind, VillageState},
    world::{WandererPrototype, WorldCamera},
};

pub struct IntentPlugin;

impl Plugin for IntentPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(OnEnter(AppScreen::InGame), initialize_intent_session);
        app.add_systems(
            Update,
            (sample_intent_from_world, handle_perception_input)
                .chain()
                .run_if(in_state(InGameState::Running)),
        );
        app.add_systems(OnExit(AppScreen::InGame), cleanup_intent_session);
    }
}

#[derive(Debug, Resource, Clone, PartialEq)]
pub struct IntentState {
    pub channels: Vec<IntentChannelState>,
    pub last_dominant: Option<IntentKind>,
}

impl Default for IntentState {
    fn default() -> Self {
        Self {
            channels: IntentKind::ALL
                .into_iter()
                .map(|kind| IntentChannelState {
                    kind,
                    strength: 0.0,
                    last_source: None,
                    last_updated_seconds: 0.0,
                })
                .collect(),
            last_dominant: None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct IntentChannelState {
    pub kind: IntentKind,
    pub strength: f32,
    pub last_source: Option<IntentSource>,
    pub last_updated_seconds: f32,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum IntentKind {
    Sea,
    Mountain,
    BeyondVillage,
    People,
    Animals,
    DreamLandmark,
    Stillness,
}

impl IntentKind {
    pub const ALL: [Self; 7] = [
        Self::Sea,
        Self::Mountain,
        Self::BeyondVillage,
        Self::People,
        Self::Animals,
        Self::DreamLandmark,
        Self::Stillness,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Sea => "向海",
            Self::Mountain => "向山",
            Self::BeyondVillage => "向村外",
            Self::People => "向人群",
            Self::Animals => "向动物",
            Self::DreamLandmark => "向梦中地标",
            Self::Stillness => "向静处",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum IntentSource {
    Staying,
    Gazing,
    Approaching,
    Dialogue,
    Biome,
    DreamEcho,
}

#[derive(Debug, Resource, Clone, PartialEq)]
pub struct PerceptionState {
    pub active: bool,
    pub cooldown_seconds: f32,
    pub remaining_seconds: f32,
    pub intensity: f32,
    pub result: Option<PerceptionResult>,
    pub last_feedback: Option<PerceptionFeedback>,
}

impl Default for PerceptionState {
    fn default() -> Self {
        Self {
            active: false,
            cooldown_seconds: 0.0,
            remaining_seconds: 0.0,
            intensity: 0.0,
            result: None,
            last_feedback: None,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum PerceptionResult {
    ClarifiedOmen,
    DreamEcho,
    QuietFailure,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum PerceptionFeedback {
    WindClears,
    BirdCall,
    DreamImage,
    Silence,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct IntentSample {
    pub kind: IntentKind,
    pub source: IntentSource,
    pub amount: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PerceptionRequestContext {
    pub delta_seconds: f32,
    pub requested: bool,
    pub dominant_intent: Option<IntentKind>,
    pub dominant_strength: f32,
    pub has_current_omen: bool,
    pub calm: f32,
    pub dream_phase: DreamPhase,
}

const INTENT_DECAY_PER_SECOND: f32 = 0.025;
const INTENT_SIGNAL_SCALE: f32 = 0.14;
const PERCEPTION_DURATION_SECONDS: f32 = 4.2;
const PERCEPTION_COOLDOWN_SECONDS: f32 = 7.5;
const PERCEPTION_INTENT_THRESHOLD: f32 = 0.34;

impl IntentState {
    pub fn strength(&self, kind: IntentKind) -> f32 {
        self.channels
            .iter()
            .find(|channel| channel.kind == kind)
            .map(|channel| channel.strength)
            .unwrap_or(0.0)
    }

    pub fn dominant(&self) -> Option<IntentKind> {
        self.channels
            .iter()
            .filter(|channel| channel.strength > 0.08)
            .max_by(|a, b| a.strength.total_cmp(&b.strength))
            .map(|channel| channel.kind)
    }
}

pub fn advance_intent_state(
    state: &mut IntentState,
    delta_seconds: f32,
    elapsed_seconds: f32,
    samples: impl IntoIterator<Item = IntentSample>,
) -> Option<IntentKind> {
    let decay = INTENT_DECAY_PER_SECOND * delta_seconds.max(0.0);
    for channel in &mut state.channels {
        channel.strength = (channel.strength - decay).max(0.0);
    }

    for sample in samples {
        if let Some(channel) = state
            .channels
            .iter_mut()
            .find(|channel| channel.kind == sample.kind)
        {
            let previous = channel.strength;
            let amount = sample.amount.max(0.0) * INTENT_SIGNAL_SCALE;
            channel.strength = (channel.strength + amount).clamp(0.0, 1.0);
            channel.last_source = Some(sample.source);
            channel.last_updated_seconds = elapsed_seconds;
            if channel.strength - previous > 0.08 {
                tracing::debug!(
                    target: "dao_game::intent::state",
                    intent = channel.kind.label(),
                    source = ?sample.source,
                    strength = channel.strength,
                    "intent channel strengthened"
                );
            }
        }
    }

    let dominant = state.dominant();
    let changed = (dominant != state.last_dominant)
        .then_some(dominant)
        .flatten();
    state.last_dominant = dominant;
    changed
}

pub fn advance_perception_state(
    perception: &mut PerceptionState,
    context: PerceptionRequestContext,
) -> Option<PerceptionResult> {
    let delta_seconds = context.delta_seconds.max(0.0);
    perception.cooldown_seconds = (perception.cooldown_seconds - delta_seconds).max(0.0);
    if perception.active {
        perception.remaining_seconds = (perception.remaining_seconds - delta_seconds).max(0.0);
        perception.intensity =
            (perception.remaining_seconds / PERCEPTION_DURATION_SECONDS).clamp(0.0, 1.0);
        if perception.remaining_seconds <= 0.0 {
            perception.active = false;
            perception.result = None;
        }
    }

    if !context.requested {
        return None;
    }

    if perception.cooldown_seconds > 0.0 {
        perception.last_feedback = Some(PerceptionFeedback::Silence);
        return Some(PerceptionResult::QuietFailure);
    }

    let dream_relevant = context.dream_phase == DreamPhase::Afterglow
        && context.dominant_intent == Some(IntentKind::DreamLandmark)
        && context.dominant_strength >= PERCEPTION_INTENT_THRESHOLD * 0.82;
    let omen_relevant = context.has_current_omen
        && context.calm >= 0.34
        && context.dominant_strength >= PERCEPTION_INTENT_THRESHOLD
        && context.dominant_intent.is_some();

    let result = if dream_relevant {
        PerceptionResult::DreamEcho
    } else if omen_relevant {
        PerceptionResult::ClarifiedOmen
    } else {
        PerceptionResult::QuietFailure
    };

    perception.cooldown_seconds = PERCEPTION_COOLDOWN_SECONDS;
    perception.last_feedback = Some(match result {
        PerceptionResult::ClarifiedOmen => PerceptionFeedback::WindClears,
        PerceptionResult::DreamEcho => PerceptionFeedback::DreamImage,
        PerceptionResult::QuietFailure => PerceptionFeedback::Silence,
    });

    if result != PerceptionResult::QuietFailure {
        perception.active = true;
        perception.remaining_seconds = PERCEPTION_DURATION_SECONDS;
        perception.intensity = 1.0;
        perception.result = Some(result);
    }

    Some(result)
}

fn initialize_intent_session(mut commands: Commands) {
    commands.insert_resource(IntentState::default());
    commands.insert_resource(PerceptionState::default());
}

fn cleanup_intent_session(mut commands: Commands) {
    commands.remove_resource::<IntentState>();
    commands.remove_resource::<PerceptionState>();
}

fn sample_intent_from_world(
    time: Res<Time>,
    intent: Option<ResMut<IntentState>>,
    village: Option<Res<VillageState>>,
    journey: Option<Res<JourneyState>>,
    player_query: Query<&Transform, With<WandererPrototype>>,
    camera_query: Query<&Transform, (With<WorldCamera>, Without<WandererPrototype>)>,
) {
    let Some(mut intent) = intent else {
        return;
    };
    let Some(player_transform) = player_query.iter().next() else {
        return;
    };
    let samples = collect_intent_samples(
        player_transform,
        camera_query.iter().next(),
        village.as_deref(),
        journey.as_deref(),
    );
    let changed =
        advance_intent_state(&mut intent, time.delta_secs(), time.elapsed_secs(), samples);
    if let Some(kind) = changed {
        tracing::info!(
            target: "dao_game::intent::state",
            intent = kind.label(),
            strength = intent.strength(kind),
            "dominant intent changed"
        );
    }
}

fn collect_intent_samples(
    player_transform: &Transform,
    camera_transform: Option<&Transform>,
    village: Option<&VillageState>,
    journey: Option<&JourneyState>,
) -> Vec<IntentSample> {
    let mut samples = Vec::new();
    if let Some(village) = village {
        for area in &village.areas {
            let distance = planar_distance(player_transform.translation, area.position);
            let proximity = (1.0 - distance / area.radius.max(0.1)).clamp(0.0, 1.0);
            if proximity <= 0.0 {
                continue;
            }
            let kind = match area.kind {
                VillageAreaKind::Shore => IntentKind::Sea,
                VillageAreaKind::SheepPen => IntentKind::Animals,
                VillageAreaKind::Market => IntentKind::People,
                VillageAreaKind::OuterPath => IntentKind::BeyondVillage,
                VillageAreaKind::Well | VillageAreaKind::Houses => IntentKind::Stillness,
            };
            samples.push(IntentSample {
                kind,
                source: IntentSource::Approaching,
                amount: proximity,
            });
        }
    }

    if let Some(journey) = journey
        && journey.story_stage != StoryArcStage::VillageAwakening
    {
        let dream_bias = match journey.dream.phase {
            DreamPhase::Afterglow => 0.78,
            DreamPhase::Ready | DreamPhase::InDream => 0.35,
            DreamPhase::Unseen => 0.0,
        };
        if dream_bias > 0.0 {
            samples.push(IntentSample {
                kind: IntentKind::DreamLandmark,
                source: IntentSource::DreamEcho,
                amount: dream_bias,
            });
        }
    }

    if let Some(camera_transform) = camera_transform
        && let Some(village) = village
        && let Some(outer_path) = village.area(VillageAreaKind::OuterPath)
    {
        let alignment = look_alignment(camera_transform, outer_path.position);
        if alignment > 0.72 {
            samples.push(IntentSample {
                kind: IntentKind::BeyondVillage,
                source: IntentSource::Gazing,
                amount: (alignment - 0.72) / 0.28,
            });
        }
    }

    samples
}

fn handle_perception_input(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    perception: Option<ResMut<PerceptionState>>,
    intent: Option<Res<IntentState>>,
    signs: Option<Res<SignState>>,
    journey: Option<Res<JourneyState>>,
    mut notebook: Option<ResMut<NotebookState>>,
) {
    let Some(mut perception) = perception else {
        return;
    };
    let dominant = intent.as_deref().and_then(IntentState::dominant);
    let strength = dominant
        .and_then(|kind| intent.as_deref().map(|intent| intent.strength(kind)))
        .unwrap_or(0.0);
    let result = advance_perception_state(
        &mut perception,
        PerceptionRequestContext {
            delta_seconds: time.delta_secs(),
            requested: keys.just_pressed(KeyCode::KeyE),
            dominant_intent: dominant,
            dominant_strength: strength,
            has_current_omen: signs.as_deref().is_some_and(|signs| {
                signs.current_omen.is_some() || signs.response_intensity > 0.02
            }),
            calm: signs.as_deref().map_or(1.0, |signs| signs.calm),
            dream_phase: journey
                .as_deref()
                .map_or(DreamPhase::Unseen, |journey| journey.dream.phase),
        },
    );

    let Some(result) = result else {
        return;
    };
    tracing::info!(
        target: "dao_game::intent::perception",
        result = ?result,
        dominant_intent = dominant.map(IntentKind::label),
        strength,
        cooldown_seconds = perception.cooldown_seconds,
        "perception requested"
    );

    if matches!(
        result,
        PerceptionResult::ClarifiedOmen | PerceptionResult::DreamEcho
    ) {
        let title = match result {
            PerceptionResult::ClarifiedOmen => "风里显出更清楚的征兆",
            PerceptionResult::DreamEcho => "梦里的轮廓短暂重现",
            PerceptionResult::QuietFailure => "沉默",
        };
        let body = match result {
            PerceptionResult::ClarifiedOmen => {
                "你停下来感知，原本散乱的光、风或鸟声短暂聚成更清晰的方向。"
            }
            PerceptionResult::DreamEcho => "沙暴、金色斜面和地下阴影一闪而过，像梦醒后仍留在眼底。",
            PerceptionResult::QuietFailure => "周围安静下来，没有给出新的回答。",
        };
        let _ = record_notebook_entry(
            notebook.as_deref_mut(),
            NotebookRecord {
                kind: NotebookEntryKind::Sign,
                at_seconds: time.elapsed_secs(),
                location: Some("途中".to_string()),
                source: NotebookSource::Perception,
                title: title.to_string(),
                body: body.to_string(),
                tags: vec![NotebookTag::Omen, NotebookTag::Perception],
            },
        );
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

pub fn perception_label(perception: &PerceptionState) -> &'static str {
    if perception.active {
        return match perception.result {
            Some(PerceptionResult::ClarifiedOmen) => "风声清晰",
            Some(PerceptionResult::DreamEcho) => "梦影回返",
            _ => "正在感知",
        };
    }
    if perception.cooldown_seconds > 0.0 {
        "余波未散"
    } else {
        "可以感知"
    }
}

pub fn intent_debug_line(
    intent: Option<&IntentState>,
    perception: Option<&PerceptionState>,
) -> String {
    let intent_text = intent
        .and_then(IntentState::dominant)
        .map(|kind| {
            let strength = intent.map(|state| state.strength(kind)).unwrap_or(0.0);
            format!("{} {:.0}%", kind.label(), strength * 100.0)
        })
        .unwrap_or_else(|| "无明显意愿".to_string());
    let perception_text = perception.map(perception_label).unwrap_or("感知未初始化");
    format!("意愿：{intent_text}  感知：{perception_text}")
}

fn omen_for_place(kind: PlaceKind) -> IntentKind {
    match kind {
        PlaceKind::AncientTree => IntentKind::Stillness,
        PlaceKind::SpringEye | PlaceKind::QuietBay => IntentKind::Sea,
        PlaceKind::RidgeGate => IntentKind::Mountain,
        PlaceKind::StoneRing => IntentKind::DreamLandmark,
    }
}

pub fn sign_affinity_for_intent(place_kind: Option<PlaceKind>, intent: Option<IntentKind>) -> f32 {
    let (Some(place_kind), Some(intent)) = (place_kind, intent) else {
        return 0.0;
    };
    if omen_for_place(place_kind) == intent {
        0.22
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::{
        IntentKind, IntentSample, IntentSource, IntentState, PerceptionRequestContext,
        PerceptionResult, PerceptionState, advance_intent_state, advance_perception_state,
        sign_affinity_for_intent,
    };
    use crate::game::{journey::DreamPhase, places::PlaceKind};

    #[test]
    fn intent_accumulates_and_decays() {
        let mut state = IntentState::default();
        advance_intent_state(
            &mut state,
            0.0,
            0.0,
            [IntentSample {
                kind: IntentKind::Sea,
                source: IntentSource::Approaching,
                amount: 2.0,
            }],
        );

        assert!(state.strength(IntentKind::Sea) > 0.25);
        advance_intent_state(&mut state, 4.0, 4.0, []);
        assert!(state.strength(IntentKind::Sea) < 0.3);
    }

    #[test]
    fn dominant_intent_tracks_strongest_channel() {
        let mut state = IntentState::default();
        advance_intent_state(
            &mut state,
            0.0,
            0.0,
            [
                IntentSample {
                    kind: IntentKind::People,
                    source: IntentSource::Approaching,
                    amount: 0.5,
                },
                IntentSample {
                    kind: IntentKind::DreamLandmark,
                    source: IntentSource::DreamEcho,
                    amount: 1.5,
                },
            ],
        );

        assert_eq!(state.dominant(), Some(IntentKind::DreamLandmark));
    }

    #[test]
    fn perception_requires_intent_or_current_omen() {
        let mut state = PerceptionState::default();
        let result = advance_perception_state(
            &mut state,
            PerceptionRequestContext {
                delta_seconds: 0.0,
                requested: true,
                dominant_intent: Some(IntentKind::Sea),
                dominant_strength: 0.5,
                has_current_omen: true,
                calm: 0.8,
                dream_phase: DreamPhase::Unseen,
            },
        );

        assert_eq!(result, Some(PerceptionResult::ClarifiedOmen));
        assert!(state.active);
        assert!(state.cooldown_seconds > 0.0);
    }

    #[test]
    fn dream_afterglow_can_trigger_dream_echo() {
        let mut state = PerceptionState::default();
        let result = advance_perception_state(
            &mut state,
            PerceptionRequestContext {
                delta_seconds: 0.0,
                requested: true,
                dominant_intent: Some(IntentKind::DreamLandmark),
                dominant_strength: 0.3,
                has_current_omen: false,
                calm: 0.8,
                dream_phase: DreamPhase::Afterglow,
            },
        );

        assert_eq!(result, Some(PerceptionResult::DreamEcho));
    }

    #[test]
    fn sign_affinity_matches_semantic_place_intent() {
        assert!(
            sign_affinity_for_intent(Some(PlaceKind::RidgeGate), Some(IntentKind::Mountain)) > 0.0
        );
        assert_eq!(
            sign_affinity_for_intent(Some(PlaceKind::QuietBay), Some(IntentKind::People)),
            0.0
        );
    }
}
