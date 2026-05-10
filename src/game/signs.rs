use bevy::prelude::*;

use crate::core::config::AppConfig;

pub struct SignPlugin;

impl Plugin for SignPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(SignState::default());
        app.add_systems(Update, update_resonance);
    }
}

#[derive(Debug, Resource, Clone, Copy)]
pub struct SignState {
    pub resonance: f32,
    pub calm: f32,
    pub omen_triggered: bool,
}

impl Default for SignState {
    fn default() -> Self {
        Self {
            resonance: 0.0,
            calm: 1.0,
            omen_triggered: false,
        }
    }
}

fn update_resonance(time: Res<Time>, config: Res<AppConfig>, mut signs: ResMut<SignState>) {
    let t = time.elapsed_secs_wrapped();
    let pulse = (t * 0.35).sin() * 0.5 + 0.5;
    signs.resonance = (signs.resonance * 0.94 + pulse * 0.06).clamp(0.0, 1.0);
    signs.calm = (signs.calm + config.signs.calm_recovery * time.delta_secs()).clamp(0.0, 1.0);

    let threshold = config.signs.resonance_threshold;
    let should_trigger = signs.resonance >= threshold && signs.calm >= 0.35;
    if should_trigger && !signs.omen_triggered {
        signs.omen_triggered = true;
        signs.calm = 0.0;
        tracing::info!(
            target: "dao_game::signs::omen",
            resonance = signs.resonance,
            threshold = threshold,
            "world omen triggered"
        );
    } else if !should_trigger {
        signs.omen_triggered = false;
    }
}

#[cfg(test)]
mod tests {
    use super::SignState;

    #[test]
    fn default_sign_state_starts_calm() {
        let state = SignState::default();
        assert_eq!(state.resonance, 0.0);
        assert_eq!(state.calm, 1.0);
        assert!(!state.omen_triggered);
    }
}
