use bevy::prelude::*;

use crate::{
    core::config::{AppConfig, SignConfig},
    game::world::{BiomeKind, TerrainTile, WandererPrototype, WorldCycle, WorldMap},
};

pub struct SignPlugin;

impl Plugin for SignPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SignState::default());
        app.insert_resource(WandererPresence::default());
        app.add_systems(Startup, spawn_omen_beacon);
        app.add_systems(
            Update,
            (
                capture_wanderer_presence,
                update_resonance,
                project_omen_feedback,
            )
                .chain(),
        );
    }
}

#[derive(Debug, Resource, Clone, Copy, PartialEq)]
pub struct SignState {
    pub resonance: f32,
    pub calm: f32,
    pub omen_triggered: bool,
    pub current_omen: Option<OmenKind>,
}

impl Default for SignState {
    fn default() -> Self {
        Self {
            resonance: 0.0,
            calm: 1.0,
            omen_triggered: false,
            current_omen: None,
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
struct SignUpdate {
    state: SignState,
    activated: bool,
}

#[derive(Debug, Component)]
struct OmenBeacon;

fn spawn_omen_beacon(mut commands: Commands) {
    commands.spawn((
        Name::new("OmenBeacon"),
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

fn update_resonance(
    time: Res<Time>,
    config: Res<AppConfig>,
    world_map: Res<WorldMap>,
    cycle: Res<WorldCycle>,
    presence: Res<WandererPresence>,
    mut signs: ResMut<SignState>,
) {
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

    let update = advance_sign_state(
        *signs,
        context,
        &config.signs,
        config.environment.wander_speed,
    );
    if update.activated {
        tracing::info!(
            target: "dao_game::signs::omen",
            resonance = update.state.resonance,
            calm = update.state.calm,
            omen = ?update.state.current_omen,
            biome = ?context.biome,
            "world omen triggered"
        );
    }
    *signs = update.state;
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

    if signs.omen_triggered {
        let color = omen_color(signs.current_omen);
        transform.translation =
            presence.position + Vec3::new(0.0, config.signs.omen_beacon_height, 0.0);
        light.color = color;
        light.intensity = match signs.current_omen {
            Some(OmenKind::DawnLight) => 120_000.0,
            Some(OmenKind::GroveWhisper) => 90_000.0,
            Some(OmenKind::SummitCall) => 150_000.0,
            Some(OmenKind::StillWater) => 70_000.0,
            None => 0.0,
        };
    } else {
        transform.translation.y = -50.0;
        light.intensity = 0.0;
    }
}

fn advance_sign_state(
    mut state: SignState,
    context: ResonanceContext,
    config: &SignConfig,
    expected_speed: f32,
) -> SignUpdate {
    let was_triggered = state.omen_triggered;
    let horizon = horizon_factor(context.normalized_time);
    let daylight = daylight_factor(context.normalized_time);
    let stillness = (1.0 - context.speed / expected_speed.max(0.05)).clamp(0.0, 1.0);
    let elevation = ((context.height - context.water_level) / 3.5).clamp(0.0, 1.0);
    let moisture_balance = (1.0 - (context.moisture - 0.58).abs() * 1.75).clamp(0.0, 1.0);
    let omen = choose_omen(context, horizon, elevation, stillness);
    let omen_bias = omen_resonance_bonus(omen);
    let target_resonance = (biome_affinity(context.biome) * 0.48
        + stillness * 0.18
        + elevation * 0.16
        + moisture_balance * 0.08
        + horizon * 0.1
        + omen_bias)
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

    let should_trigger = state.resonance >= config.resonance_threshold
        && state.calm >= config.calm_threshold
        && omen.is_some();

    if should_trigger {
        state.omen_triggered = true;
        state.current_omen = omen;
        state.calm = (state.calm - 0.18).clamp(0.0, 1.0);
    } else if state.resonance < config.resonance_threshold * 0.86 {
        state.omen_triggered = false;
        state.current_omen = None;
    }

    SignUpdate {
        state,
        activated: should_trigger && !was_triggered,
    }
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
    horizon: f32,
    elevation: f32,
    stillness: f32,
) -> Option<OmenKind> {
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
    use super::{BiomeKind, OmenKind, ResonanceContext, SignState, advance_sign_state};
    use crate::core::config::SignConfig;

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
            SignState {
                resonance: 0.72,
                calm: 0.7,
                omen_triggered: false,
                current_omen: None,
            },
            ResonanceContext {
                biome: BiomeKind::Ridge,
                height: 3.6,
                moisture: 0.3,
                water_level: -0.1,
                speed: 0.1,
                normalized_time: 0.01,
                delta_seconds: 1.0,
            },
            &SignConfig {
                resonance_threshold: 0.74,
                resonance_smoothing: 0.4,
                calm_recovery: 0.02,
                calm_threshold: 0.35,
                omen_beacon_height: 3.0,
            },
            0.75,
        );

        assert!(update.state.omen_triggered);
        assert_eq!(update.state.current_omen, Some(OmenKind::DawnLight));
        assert!(update.activated);
    }

    #[test]
    fn restless_motion_prevents_calm_recovery() {
        let update = advance_sign_state(
            SignState {
                resonance: 0.2,
                calm: 0.3,
                omen_triggered: false,
                current_omen: None,
            },
            ResonanceContext {
                biome: BiomeKind::Steppe,
                height: 1.2,
                moisture: 0.25,
                water_level: -0.1,
                speed: 1.8,
                normalized_time: 0.4,
                delta_seconds: 1.0,
            },
            &SignConfig {
                resonance_threshold: 0.72,
                resonance_smoothing: 0.2,
                calm_recovery: 0.02,
                calm_threshold: 0.35,
                omen_beacon_height: 3.0,
            },
            0.75,
        );

        assert!(update.state.calm < 0.3);
        assert!(!update.state.omen_triggered);
    }

    #[test]
    fn still_water_context_receives_bonus_toward_threshold() {
        let still_update = advance_sign_state(
            SignState {
                resonance: 0.62,
                calm: 0.8,
                omen_triggered: false,
                current_omen: None,
            },
            ResonanceContext {
                biome: BiomeKind::Water,
                height: -0.05,
                moisture: 0.62,
                water_level: -0.1,
                speed: 0.0,
                normalized_time: 0.72,
                delta_seconds: 1.0,
            },
            &SignConfig {
                resonance_threshold: 0.72,
                resonance_smoothing: 0.35,
                calm_recovery: 0.02,
                calm_threshold: 0.35,
                omen_beacon_height: 3.0,
            },
            0.75,
        );
        let restless_update = advance_sign_state(
            SignState {
                resonance: 0.62,
                calm: 0.8,
                omen_triggered: false,
                current_omen: None,
            },
            ResonanceContext {
                biome: BiomeKind::Water,
                height: -0.05,
                moisture: 0.62,
                water_level: -0.1,
                speed: 1.6,
                normalized_time: 0.72,
                delta_seconds: 1.0,
            },
            &SignConfig {
                resonance_threshold: 0.72,
                resonance_smoothing: 0.35,
                calm_recovery: 0.02,
                calm_threshold: 0.35,
                omen_beacon_height: 3.0,
            },
            0.75,
        );

        assert!(still_update.state.resonance > restless_update.state.resonance);
        assert!(still_update.state.calm > restless_update.state.calm);
    }
}
