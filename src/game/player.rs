use bevy::{
    input::mouse::AccumulatedMouseMotion,
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};

use crate::{
    core::config::AppConfig,
    game::world::{WandererPrototype, WorldCamera, WorldMap},
};

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(FirstPersonState::default());
        app.add_systems(
            PostStartup,
            (initialize_first_person_state, initialize_cursor_lock).chain(),
        );
        app.add_systems(
            Update,
            (
                toggle_cursor_lock,
                apply_mouse_look,
                move_player_body,
                sync_camera_to_player,
            )
                .chain(),
        );
    }
}

#[derive(Debug, Resource, Clone, Copy)]
pub struct FirstPersonState {
    pub yaw: f32,
    pub pitch: f32,
    pub vertical_velocity: f32,
    pub grounded: bool,
    pub cursor_locked: bool,
}

impl Default for FirstPersonState {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.0,
            vertical_velocity: 0.0,
            grounded: true,
            cursor_locked: true,
        }
    }
}

fn initialize_first_person_state(
    config: Res<AppConfig>,
    mut state: ResMut<FirstPersonState>,
    mut player_query: Query<&mut Transform, With<WandererPrototype>>,
    mut camera_query: Query<&mut Transform, (With<WorldCamera>, Without<WandererPrototype>)>,
) {
    let Some(mut player_transform) = player_query.iter_mut().next() else {
        return;
    };
    let Some(mut camera_transform) = camera_query.iter_mut().next() else {
        return;
    };

    state.yaw = 0.0;
    state.pitch = 0.0;
    state.vertical_velocity = 0.0;
    state.grounded = true;
    state.cursor_locked = true;

    player_transform.rotation = Quat::IDENTITY;
    camera_transform.translation =
        player_transform.translation + Vec3::Y * config.player.eye_height;
    camera_transform.rotation = Quat::IDENTITY;
}

fn initialize_cursor_lock(mut cursor_options: Single<&mut CursorOptions, With<PrimaryWindow>>) {
    cursor_options.visible = false;
    cursor_options.grab_mode = CursorGrabMode::Locked;
}

fn toggle_cursor_lock(
    keys: Res<ButtonInput<KeyCode>>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut state: ResMut<FirstPersonState>,
    mut cursor_options: Single<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if keys.just_pressed(KeyCode::Escape) {
        state.cursor_locked = false;
        cursor_options.visible = true;
        cursor_options.grab_mode = CursorGrabMode::None;
    }

    if mouse_buttons.just_pressed(MouseButton::Left) && !state.cursor_locked {
        state.cursor_locked = true;
        cursor_options.visible = false;
        cursor_options.grab_mode = CursorGrabMode::Locked;
    }
}

fn apply_mouse_look(
    config: Res<AppConfig>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    mut state: ResMut<FirstPersonState>,
) {
    if !state.cursor_locked {
        return;
    }

    let delta = mouse_motion.delta;
    if delta == Vec2::ZERO {
        return;
    }

    state.yaw -= delta.x * config.player.mouse_sensitivity;
    state.pitch = (state.pitch - delta.y * config.player.mouse_sensitivity).clamp(-1.54, 1.54);
}

fn move_player_body(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    config: Res<AppConfig>,
    world_map: Res<WorldMap>,
    mut state: ResMut<FirstPersonState>,
    mut player_query: Query<&mut Transform, With<WandererPrototype>>,
) {
    let Some(mut transform) = player_query.iter_mut().next() else {
        return;
    };

    let yaw_rotation = Quat::from_rotation_y(state.yaw);
    let forward = yaw_rotation * -Vec3::Z;
    let right = yaw_rotation * Vec3::X;

    let mut movement = Vec3::ZERO;
    if keys.pressed(KeyCode::KeyW) {
        movement += forward;
    }
    if keys.pressed(KeyCode::KeyS) {
        movement -= forward;
    }
    if keys.pressed(KeyCode::KeyD) {
        movement += right;
    }
    if keys.pressed(KeyCode::KeyA) {
        movement -= right;
    }
    movement.y = 0.0;
    if movement.length_squared() > 0.0 {
        movement = movement.normalize();
    }

    let sprint_multiplier = if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight)
    {
        config.player.sprint_multiplier
    } else {
        1.0
    };
    let horizontal_speed = config.player.walk_speed * sprint_multiplier;
    let horizontal_delta = movement * horizontal_speed * time.delta_secs();
    transform.translation += Vec3::new(horizontal_delta.x, 0.0, horizontal_delta.z);

    if let Some(ground_height) =
        world_map.sample_height(transform.translation.x, transform.translation.z)
    {
        let target_y = ground_height + config.player.body_height;
        if state.grounded && keys.just_pressed(KeyCode::Space) {
            state.vertical_velocity = config.player.jump_velocity;
            state.grounded = false;
        }

        if !state.grounded {
            state.vertical_velocity -= config.player.gravity * time.delta_secs();
            transform.translation.y += state.vertical_velocity * time.delta_secs();
        } else {
            transform.translation.y = transform.translation.y.lerp(target_y, 0.45);
        }

        if transform.translation.y <= target_y {
            transform.translation.y = target_y;
            state.vertical_velocity = 0.0;
            state.grounded = true;
        }
    }

    transform.rotation = Quat::from_rotation_y(state.yaw);
}

fn sync_camera_to_player(
    config: Res<AppConfig>,
    state: Res<FirstPersonState>,
    player_query: Query<&Transform, With<WandererPrototype>>,
    mut camera_query: Query<&mut Transform, (With<WorldCamera>, Without<WandererPrototype>)>,
) {
    let Some(player_transform) = player_query.iter().next() else {
        return;
    };
    let Some(mut camera_transform) = camera_query.iter_mut().next() else {
        return;
    };

    camera_transform.translation =
        player_transform.translation + Vec3::Y * config.player.eye_height;
    camera_transform.rotation = Quat::from_euler(EulerRot::YXZ, state.yaw, state.pitch, 0.0);
}

#[cfg(test)]
mod tests {
    use super::FirstPersonState;

    #[test]
    fn default_first_person_state_starts_grounded_and_locked() {
        let state = FirstPersonState::default();
        assert!(state.grounded);
        assert!(state.cursor_locked);
        assert_eq!(state.pitch, 0.0);
        assert_eq!(state.yaw, 0.0);
    }
}
