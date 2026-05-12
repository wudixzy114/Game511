use bevy::prelude::*;

use crate::{
    core::{
        config::{AppConfig, SignConfig},
        performance::{FramePerformance, PerformancePhase},
    },
    game::{
        flow::{AppScreen, InGameState},
        journey::JourneyState,
        places::{MeaningfulPlaces, PlaceKind, choose_primary_place, planar_distance},
        world::{BiomeKind, TerrainTile, WandererPrototype, WorldCycle, WorldMap},
    },
};

pub struct SignPlugin;

impl Plugin for SignPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SignState::default());
        app.insert_resource(WandererPresence::default());
        app.add_systems(
            OnEnter(AppScreen::InGame),
            (reset_sign_resources, spawn_omen_beacon),
        );
        app.add_systems(
            Update,
            (
                capture_wanderer_presence,
                update_resonance,
                project_omen_feedback,
            )
                .chain()
                .run_if(in_state(InGameState::Running)),
        );
    }
}

#[derive(Debug, Resource, Clone, Copy, PartialEq)]
pub struct SignState {
    pub resonance: f32,
    pub calm: f32,
    pub omen_triggered: bool,
    pub current_omen: Option<OmenKind>,
    pub omen_intensity: f32,
    pub omen_direction: Vec3,
    pub target_place_id: Option<u64>,
    pub target_place_kind: Option<PlaceKind>,
    pub target_position: Option<Vec3>,
    pub target_distance: Option<f32>,
    pub guidance_phase: OmenGuidancePhase,
    pub cooldown_seconds: f32,
    pub response_intensity: f32,
}

impl Default for SignState {
    fn default() -> Self {
        Self {
            resonance: 0.0,
            calm: 1.0,
            omen_triggered: false,
            current_omen: None,
            omen_intensity: 0.0,
            omen_direction: Vec3::ZERO,
            target_place_id: None,
            target_place_kind: None,
            target_position: None,
            target_distance: None,
            guidance_phase: OmenGuidancePhase::Dormant,
            cooldown_seconds: 0.0,
            response_intensity: 0.0,
        }
    }
}

#[derive(Debug, Resource, Clone, Copy, Default)]
struct WandererPresence {
    position: Vec3,
    speed: f32,
    tile: Option<TerrainTile>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OmenKind {
    DawnLight,
    GroveWhisper,
    SummitCall,
    StillWater,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum OmenGuidancePhase {
    #[default]
    Dormant,
    Far,
    DrawingNear,
    Arrived,
    Responding,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct ResonanceContext {
    biome: BiomeKind,
    height: f32,
    moisture: f32,
    water_level: f32,
    speed: f32,
    normalized_time: f32,
    delta_seconds: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct OmenGuidanceContext {
    target_place_id: Option<u64>,
    target_place_kind: Option<PlaceKind>,
    target_position: Option<Vec3>,
    target_distance: Option<f32>,
    direction_to_target: Vec3,
    proximity: f32,
    phase: OmenGuidancePhase,
    response_intensity: f32,
}

impl Default for OmenGuidanceContext {
    fn default() -> Self {
        Self {
            target_place_id: None,
            target_place_kind: None,
            target_position: None,
            target_distance: None,
            direction_to_target: Vec3::ZERO,
            proximity: 0.0,
            phase: OmenGuidancePhase::Dormant,
            response_intensity: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct SignUpdate {
    state: SignState,
    activated: bool,
}

#[derive(Debug, Component)]
struct OmenBeacon;

type SignUpdateResources<'w> = (
    Res<'w, Time>,
    Res<'w, AppConfig>,
    Res<'w, WorldMap>,
    Res<'w, WorldCycle>,
    Option<Res<'w, MeaningfulPlaces>>,
    Option<Res<'w, JourneyState>>,
    Res<'w, WandererPresence>,
    ResMut<'w, FramePerformance>,
);

fn reset_sign_resources(mut signs: ResMut<SignState>, mut presence: ResMut<WandererPresence>) {
    *signs = SignState::default();
    *presence = WandererPresence::default();
}

fn spawn_omen_beacon(mut commands: Commands) {
    commands.spawn((
        Name::new("OmenBeacon"),
        DespawnOnExit(AppScreen::InGame),
        PointLight {
            intensity: 0.0,
            range: 18.0,
            radius: 0.8,
            shadows_enabled: true,
            ..Default::default()
        },
        Transform::from_xyz(0.0, -50.0, 0.0),
        OmenBeacon,
    ));
}

fn capture_wanderer_presence(
    time: Res<Time>,
    world_map: Res<WorldMap>,
    mut presence: ResMut<WandererPresence>,
    query: Query<&Transform, With<WandererPrototype>>,
) {
    let Some(transform) = query.iter().next() else {
        return;
    };

    let speed = if time.delta_secs() > 0.0 {
        presence.position.distance(transform.translation) / time.delta_secs()
    } else {
        0.0
    };
    presence.position = transform.translation;
    presence.speed = speed;
    presence.tile = world_map.sample_world_position(transform.translation);
}

fn update_resonance(resources: SignUpdateResources<'_>, mut signs: ResMut<SignState>) {
    let (time, config, world_map, cycle, places, journey, presence, mut performance) = resources;
    let started_at = std::time::Instant::now();
    let Some(tile) = presence.tile else {
        return;
    };

    let context = ResonanceContext {
        biome: tile.biome(),
        height: tile.height(),
        moisture: tile.moisture(),
        water_level: world_map.water_level(),
        speed: presence.speed,
        normalized_time: cycle.normalized_time,
        delta_seconds: time.delta_secs(),
    };
    let guidance = build_guidance_context(places.as_deref(), journey.as_deref(), presence.position);
    let previous = *signs;

    let update = advance_sign_state(
        *signs,
        context,
        guidance,
        &config.signs,
        config.environment.wander_speed,
    );
    if update.activated || sign_guidance_changed(previous, update.state) {
        tracing::info!(
            target: "dao_game::signs::omen",
            resonance = update.state.resonance,
            calm = update.state.calm,
            omen = ?update.state.current_omen,
            intensity = update.state.omen_intensity,
            phase = ?update.state.guidance_phase,
            target_place_id = ?update.state.target_place_id,
            target_place_kind = update.state.target_place_kind.map(PlaceKind::label),
            target_distance = ?update.state.target_distance,
            biome = ?context.biome,
            cooldown_seconds = update.state.cooldown_seconds,
            "world omen guidance updated"
        );
    }
    *signs = update.state;
    performance.record_phase_duration(PerformancePhase::Signs, started_at.elapsed());
}

fn project_omen_feedback(
    config: Res<AppConfig>,
    presence: Res<WandererPresence>,
    signs: Res<SignState>,
    mut beacons: Query<(&mut PointLight, &mut Transform), With<OmenBeacon>>,
) {
    let Some((mut light, mut transform)) = beacons.iter_mut().next() else {
        return;
    };

    if signs.omen_intensity > 0.02 {
        let color = omen_color(signs.current_omen);
        transform.translation = omen_beacon_position(&presence, &signs, &config);
        light.color = color;
        light.intensity = match signs.current_omen {
            Some(OmenKind::DawnLight) => 120_000.0,
            Some(OmenKind::GroveWhisper) => 90_000.0,
            Some(OmenKind::SummitCall) => 150_000.0,
            Some(OmenKind::StillWater) => 70_000.0,
            None => 0.0,
        } * signs.omen_intensity.max(0.08)
            + signs.response_intensity * 95_000.0;
        light.range = 16.0 + signs.omen_intensity * 18.0 + signs.response_intensity * 14.0;
    } else {
        transform.translation.y = -50.0;
        light.intensity = 0.0;
    }
}

fn advance_sign_state(
    mut state: SignState,
    context: ResonanceContext,
    guidance: OmenGuidanceContext,
    config: &SignConfig,
    expected_speed: f32,
) -> SignUpdate {
    let was_triggered = state.omen_triggered;
    state.cooldown_seconds = (state.cooldown_seconds - context.delta_seconds).max(0.0);
    let horizon = horizon_factor(context.normalized_time);
    let daylight = daylight_factor(context.normalized_time);
    let stillness = (1.0 - context.speed / expected_speed.max(0.05)).clamp(0.0, 1.0);
    let elevation = ((context.height - context.water_level) / 3.5).clamp(0.0, 1.0);
    let moisture_balance = (1.0 - (context.moisture - 0.58).abs() * 1.75).clamp(0.0, 1.0);
    let omen = choose_omen(context, guidance, horizon, elevation, stillness);
    let omen_bias = omen_resonance_bonus(omen);
    let guidance_bonus =
        guidance.proximity * 0.26 + guidance.response_intensity * 0.42 + target_bias(guidance);
    let target_resonance = (biome_affinity(context.biome) * 0.48
        + stillness * 0.18
        + elevation * 0.16
        + moisture_balance * 0.08
        + horizon * 0.1
        + omen_bias
        + guidance_bonus)
        .clamp(0.0, 1.0);

    let smoothing = config.resonance_smoothing.clamp(0.0, 1.0);
    state.resonance =
        (state.resonance * (1.0 - smoothing) + target_resonance * smoothing).clamp(0.0, 1.0);

    let motion_pressure = (context.speed / expected_speed.max(0.05)).clamp(0.0, 1.5) * 0.08;
    let water_bonus = if context.biome == BiomeKind::Water {
        0.015
    } else {
        0.0
    };
    let night_penalty = if daylight < 0.2 { 0.01 } else { 0.0 };
    state.calm = (state.calm
        + (config.calm_recovery + water_bonus - motion_pressure - night_penalty)
            * context.delta_seconds)
        .clamp(0.0, 1.0);

    let threshold = config.resonance_threshold;
    let should_trigger =
        state.resonance >= threshold && state.calm >= config.calm_threshold && omen.is_some();
    let should_sustain = state.resonance >= threshold * 0.56
        && state.calm >= config.calm_threshold * 0.55
        && omen.is_some()
        && guidance.target_place_id.is_some();

    if should_trigger {
        state.omen_triggered = true;
        state.current_omen = omen;
        if !was_triggered && state.cooldown_seconds <= 0.0 {
            state.calm = (state.calm - 0.12).clamp(0.0, 1.0);
        }
    } else if should_sustain {
        state.omen_triggered = true;
        state.current_omen = omen;
    } else if state.resonance < threshold * 0.5 || guidance.target_place_id.is_none() {
        if was_triggered {
            state.cooldown_seconds = 1.8;
        }
        state.omen_triggered = false;
        state.current_omen = None;
    }

    state.omen_intensity = omen_intensity(state, guidance, threshold);
    state.omen_direction = guidance.direction_to_target;
    state.target_place_id = guidance.target_place_id;
    state.target_place_kind = guidance.target_place_kind;
    state.target_position = guidance.target_position;
    state.target_distance = guidance.target_distance;
    state.guidance_phase = guidance.phase;
    state.response_intensity = guidance.response_intensity;

    let activated = state.omen_triggered && !was_triggered;
    SignUpdate { state, activated }
}

fn build_guidance_context(
    places: Option<&MeaningfulPlaces>,
    journey: Option<&JourneyState>,
    player_position: Vec3,
) -> OmenGuidanceContext {
    let mut context = OmenGuidanceContext::default();
    let place = journey
        .and_then(|journey| {
            journey
                .target
                .and_then(|target| places?.place_by_id(target.id))
        })
        .or_else(|| places.and_then(|places| choose_primary_place(places, player_position)));
    let Some(place) = place else {
        return context;
    };

    let distance = planar_distance(player_position, place.position);
    let direction = Vec3::new(
        place.position.x - player_position.x,
        0.0,
        place.position.z - player_position.z,
    )
    .normalize_or_zero();
    let arrival_radius = place.arrival_radius.max(0.1);
    let proximity = (1.0 - ((distance - arrival_radius) / 120.0).clamp(0.0, 1.0))
        .max(if distance <= arrival_radius { 1.0 } else { 0.0 });
    let response_intensity = journey
        .map(|journey| journey.response.intensity)
        .unwrap_or(0.0);
    let phase = if response_intensity > 0.02 {
        OmenGuidancePhase::Responding
    } else if distance <= place.interaction_radius {
        OmenGuidancePhase::Arrived
    } else if distance <= arrival_radius * 2.6 {
        OmenGuidancePhase::DrawingNear
    } else {
        OmenGuidancePhase::Far
    };

    context.target_place_id = Some(place.id);
    context.target_place_kind = Some(place.kind);
    context.target_position = Some(place.position);
    context.target_distance = Some(distance);
    context.direction_to_target = direction;
    context.proximity = proximity;
    context.phase = phase;
    context.response_intensity = response_intensity;
    context
}

fn sign_guidance_changed(previous: SignState, current: SignState) -> bool {
    previous.target_place_id != current.target_place_id
        || previous.guidance_phase != current.guidance_phase
        || previous.current_omen != current.current_omen
        || (previous.omen_intensity - current.omen_intensity).abs() > 0.35
        || (previous.response_intensity <= 0.02 && current.response_intensity > 0.02)
}

fn target_bias(guidance: OmenGuidanceContext) -> f32 {
    match guidance.phase {
        OmenGuidancePhase::Dormant => 0.0,
        OmenGuidancePhase::Far => 0.06,
        OmenGuidancePhase::DrawingNear => 0.14,
        OmenGuidancePhase::Arrived => 0.22,
        OmenGuidancePhase::Responding => 0.34,
    }
}

fn omen_intensity(
    state: SignState,
    guidance: OmenGuidanceContext,
    resonance_threshold: f32,
) -> f32 {
    if state.current_omen.is_none() {
        return (guidance.response_intensity * 0.72).clamp(0.0, 1.0);
    }
    let resonance = (state.resonance / resonance_threshold.max(0.001)).clamp(0.0, 1.0);
    let phase = match guidance.phase {
        OmenGuidancePhase::Dormant => 0.0,
        OmenGuidancePhase::Far => 0.45,
        OmenGuidancePhase::DrawingNear => 0.72,
        OmenGuidancePhase::Arrived => 0.88,
        OmenGuidancePhase::Responding => 1.0,
    };
    (resonance * 0.48
        + guidance.proximity * 0.24
        + phase * 0.2
        + guidance.response_intensity * 0.28)
        .clamp(0.0, 1.0)
}

fn omen_beacon_position(
    presence: &WandererPresence,
    signs: &SignState,
    config: &AppConfig,
) -> Vec3 {
    let height = config.signs.omen_beacon_height;
    let base = presence.position + Vec3::Y * height;
    if signs.guidance_phase == OmenGuidancePhase::Responding
        && let Some(target) = signs.target_position
    {
        return target + Vec3::Y * (height + 1.0);
    }

    let distance = signs.target_distance.unwrap_or(0.0);
    let offset = match signs.guidance_phase {
        OmenGuidancePhase::Dormant => 0.0,
        OmenGuidancePhase::Far => distance.clamp(12.0, 28.0),
        OmenGuidancePhase::DrawingNear => distance.clamp(7.0, 18.0),
        OmenGuidancePhase::Arrived => 4.0,
        OmenGuidancePhase::Responding => 0.0,
    };
    base + signs.omen_direction.normalize_or_zero() * offset
}

fn omen_resonance_bonus(omen: Option<OmenKind>) -> f32 {
    match omen {
        Some(OmenKind::DawnLight) => 0.08,
        Some(OmenKind::GroveWhisper) => 0.12,
        Some(OmenKind::SummitCall) => 0.1,
        Some(OmenKind::StillWater) => 0.24,
        None => 0.0,
    }
}

fn biome_affinity(biome: BiomeKind) -> f32 {
    match biome {
        BiomeKind::Water => 0.42,
        BiomeKind::Meadow => 0.6,
        BiomeKind::Grove => 0.76,
        BiomeKind::Steppe => 0.48,
        BiomeKind::Ridge => 0.7,
    }
}

fn horizon_factor(normalized_time: f32) -> f32 {
    let phase = normalized_time * std::f32::consts::TAU;
    let sun_height = phase.sin().abs();
    (1.0 - sun_height).powf(1.7).clamp(0.0, 1.0)
}

fn daylight_factor(normalized_time: f32) -> f32 {
    ((normalized_time * std::f32::consts::TAU).sin() * 0.5 + 0.5).clamp(0.0, 1.0)
}

fn choose_omen(
    context: ResonanceContext,
    guidance: OmenGuidanceContext,
    horizon: f32,
    elevation: f32,
    stillness: f32,
) -> Option<OmenKind> {
    if guidance.response_intensity > 0.02 {
        return match guidance.target_place_kind {
            Some(PlaceKind::AncientTree) => Some(OmenKind::GroveWhisper),
            Some(PlaceKind::SpringEye | PlaceKind::QuietBay) => Some(OmenKind::StillWater),
            Some(PlaceKind::RidgeGate) => Some(OmenKind::SummitCall),
            Some(PlaceKind::StoneRing) => Some(OmenKind::DawnLight),
            None => None,
        };
    }

    if matches!(
        guidance.phase,
        OmenGuidancePhase::DrawingNear | OmenGuidancePhase::Arrived
    ) {
        match guidance.target_place_kind {
            Some(PlaceKind::AncientTree) if stillness > 0.16 => {
                return Some(OmenKind::GroveWhisper);
            }
            Some(PlaceKind::SpringEye | PlaceKind::QuietBay) if stillness > 0.28 => {
                return Some(OmenKind::StillWater);
            }
            Some(PlaceKind::RidgeGate) => return Some(OmenKind::SummitCall),
            Some(PlaceKind::StoneRing) if horizon > 0.48 => return Some(OmenKind::DawnLight),
            _ => {}
        }
    }

    if horizon > 0.82 && context.biome != BiomeKind::Water {
        Some(OmenKind::DawnLight)
    } else if context.biome == BiomeKind::Ridge && elevation > 0.62 {
        Some(OmenKind::SummitCall)
    } else if context.biome == BiomeKind::Grove && context.moisture > 0.68 && stillness > 0.22 {
        Some(OmenKind::GroveWhisper)
    } else if context.biome == BiomeKind::Water && stillness > 0.65 {
        Some(OmenKind::StillWater)
    } else {
        None
    }
}

fn omen_color(omen: Option<OmenKind>) -> Color {
    match omen {
        Some(OmenKind::DawnLight) => Color::srgb(0.95, 0.74, 0.46),
        Some(OmenKind::GroveWhisper) => Color::srgb(0.46, 0.85, 0.56),
        Some(OmenKind::SummitCall) => Color::srgb(0.78, 0.78, 0.92),
        Some(OmenKind::StillWater) => Color::srgb(0.42, 0.72, 0.92),
        None => Color::srgb(1.0, 1.0, 1.0),
    }
}

#[cfg(test)]
mod tests {
    use bevy::prelude::Vec3;

    use super::{
        BiomeKind, OmenGuidanceContext, OmenGuidancePhase, OmenKind, ResonanceContext, SignState,
        advance_sign_state,
    };
    use crate::core::config::SignConfig;
    use crate::game::places::PlaceKind;

    fn sign_state(resonance: f32, calm: f32) -> SignState {
        SignState {
            resonance,
            calm,
            ..Default::default()
        }
    }

    fn config(
        resonance_threshold: f32,
        resonance_smoothing: f32,
        calm_recovery: f32,
    ) -> SignConfig {
        SignConfig {
            resonance_threshold,
            resonance_smoothing,
            calm_recovery,
            calm_threshold: 0.35,
            omen_beacon_height: 3.0,
        }
    }

    fn guidance(kind: PlaceKind, distance: f32, phase: OmenGuidancePhase) -> OmenGuidanceContext {
        OmenGuidanceContext {
            target_place_id: Some(7),
            target_place_kind: Some(kind),
            target_position: Some(Vec3::new(24.0, 1.0, 0.0)),
            target_distance: Some(distance),
            direction_to_target: Vec3::X,
            proximity: (1.0 - distance / 120.0).clamp(0.0, 1.0),
            phase,
            response_intensity: 0.0,
        }
    }

    #[test]
    fn default_sign_state_starts_calm() {
        let state = SignState::default();
        assert_eq!(state.resonance, 0.0);
        assert_eq!(state.calm, 1.0);
        assert!(!state.omen_triggered);
        assert_eq!(state.current_omen, None);
    }

    #[test]
    fn ridge_at_horizon_can_trigger_omen() {
        let update = advance_sign_state(
            sign_state(0.72, 0.7),
            ResonanceContext {
                biome: BiomeKind::Ridge,
                height: 3.6,
                moisture: 0.3,
                water_level: -0.1,
                speed: 0.1,
                normalized_time: 0.01,
                delta_seconds: 1.0,
            },
            guidance(PlaceKind::RidgeGate, 64.0, OmenGuidancePhase::DrawingNear),
            &config(0.74, 0.4, 0.02),
            0.75,
        );

        assert!(update.state.omen_triggered);
        assert_eq!(update.state.current_omen, Some(OmenKind::SummitCall));
        assert!(update.activated);
    }

    #[test]
    fn restless_motion_prevents_calm_recovery() {
        let update = advance_sign_state(
            sign_state(0.2, 0.3),
            ResonanceContext {
                biome: BiomeKind::Steppe,
                height: 1.2,
                moisture: 0.25,
                water_level: -0.1,
                speed: 1.8,
                normalized_time: 0.4,
                delta_seconds: 1.0,
            },
            OmenGuidanceContext::default(),
            &config(0.72, 0.2, 0.02),
            0.75,
        );

        assert!(update.state.calm < 0.3);
        assert!(!update.state.omen_triggered);
    }

    #[test]
    fn still_water_context_receives_bonus_toward_threshold() {
        let still_update = advance_sign_state(
            sign_state(0.62, 0.8),
            ResonanceContext {
                biome: BiomeKind::Water,
                height: -0.05,
                moisture: 0.62,
                water_level: -0.1,
                speed: 0.0,
                normalized_time: 0.72,
                delta_seconds: 1.0,
            },
            guidance(PlaceKind::QuietBay, 34.0, OmenGuidancePhase::DrawingNear),
            &config(0.95, 0.35, 0.02),
            0.75,
        );
        let restless_update = advance_sign_state(
            sign_state(0.62, 0.8),
            ResonanceContext {
                biome: BiomeKind::Water,
                height: -0.05,
                moisture: 0.62,
                water_level: -0.1,
                speed: 1.6,
                normalized_time: 0.72,
                delta_seconds: 1.0,
            },
            guidance(PlaceKind::QuietBay, 34.0, OmenGuidancePhase::DrawingNear),
            &config(0.95, 0.35, 0.02),
            0.75,
        );

        assert!(still_update.state.resonance > restless_update.state.resonance);
        assert!(still_update.state.calm > restless_update.state.calm);
    }

    #[test]
    fn nearby_target_sustains_directional_omen_feedback() {
        let update = advance_sign_state(
            sign_state(0.44, 0.78),
            ResonanceContext {
                biome: BiomeKind::Meadow,
                height: 1.4,
                moisture: 0.48,
                water_level: -0.1,
                speed: 0.2,
                normalized_time: 0.03,
                delta_seconds: 1.0,
            },
            guidance(PlaceKind::StoneRing, 18.0, OmenGuidancePhase::DrawingNear),
            &config(0.72, 0.45, 0.02),
            0.75,
        );

        assert!(update.state.omen_triggered);
        assert_eq!(update.state.target_place_id, Some(7));
        assert_eq!(update.state.omen_direction, Vec3::X);
        assert!(update.state.omen_intensity > 0.45);
    }

    #[test]
    fn response_intensity_switches_omen_to_place_semantics() {
        let mut response_guidance =
            guidance(PlaceKind::SpringEye, 4.0, OmenGuidancePhase::Responding);
        response_guidance.response_intensity = 0.9;

        let update = advance_sign_state(
            sign_state(0.4, 0.82),
            ResonanceContext {
                biome: BiomeKind::Meadow,
                height: 0.4,
                moisture: 0.72,
                water_level: -0.1,
                speed: 0.0,
                normalized_time: 0.42,
                delta_seconds: 1.0,
            },
            response_guidance,
            &config(0.72, 0.35, 0.02),
            0.75,
        );

        assert_eq!(update.state.current_omen, Some(OmenKind::StillWater));
        assert_eq!(update.state.guidance_phase, OmenGuidancePhase::Responding);
        assert!(update.state.response_intensity > 0.8);
        assert!(update.state.omen_intensity > 0.7);
    }
}
