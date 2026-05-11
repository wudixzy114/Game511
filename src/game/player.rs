use bevy::{
    input::mouse::AccumulatedMouseMotion,
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};

use crate::{
    core::config::AppConfig,
    game::{
        flow::{AppScreen, InGameState, SessionMode, in_session_mode},
        world::{
            TerrainCollisionProxy, TerrainCollisionSample, WandererPrototype, WorldCamera, WorldMap,
        },
    },
};

pub struct PlayerPlugin;

impl Plugin for PlayerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            OnEnter(AppScreen::InGame),
            start_player_session.run_if(in_session_mode(SessionMode::Exploration)),
        );
        app.add_systems(OnEnter(AppScreen::MainMenu), release_cursor_to_menu);
        app.add_systems(
            OnEnter(InGameState::Running),
            lock_cursor_for_running_session,
        );
        app.add_systems(OnEnter(InGameState::Paused), release_cursor_for_pause);
        app.add_systems(OnExit(AppScreen::InGame), end_player_session);
        app.add_systems(
            Update,
            (
                initialize_first_person_state,
                apply_mouse_look,
                move_player_body,
                sync_camera_to_player,
            )
                .chain()
                .run_if(in_state(InGameState::Running))
                .run_if(in_session_mode(SessionMode::Exploration)),
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

#[derive(Debug, Resource, Clone, Copy, Default)]
struct FirstPersonBootstrap {
    pending: bool,
}

const CAPSULE_SUPPORT_DIRECTIONS: [Vec2; 8] = [
    Vec2::new(1.0, 0.0),
    Vec2::new(-1.0, 0.0),
    Vec2::new(0.0, 1.0),
    Vec2::new(0.0, -1.0),
    Vec2::new(0.70710677, 0.70710677),
    Vec2::new(0.70710677, -0.70710677),
    Vec2::new(-0.70710677, 0.70710677),
    Vec2::new(-0.70710677, -0.70710677),
];

type PlayerMoveResources<'w> = (
    Res<'w, Time>,
    Res<'w, ButtonInput<KeyCode>>,
    Res<'w, AppConfig>,
    Res<'w, WorldMap>,
    ResMut<'w, TerrainCollisionProxy>,
);

#[derive(Debug, Clone, Copy, PartialEq)]
struct TerrainWalkerConfig {
    capsule_radius: f32,
    max_ground_normal_y: f32,
    step_height: f32,
    ground_snap_distance: f32,
    contact_substeps: usize,
}

impl TerrainWalkerConfig {
    fn from_app_config(config: &AppConfig) -> Self {
        Self {
            capsule_radius: config.player.capsule_radius.max(0.05),
            max_ground_normal_y: config
                .player
                .max_slope_degrees
                .clamp(1.0, 89.0)
                .to_radians()
                .cos(),
            step_height: config.player.step_height.max(0.0),
            ground_snap_distance: config.player.ground_snap_distance.max(0.0),
            contact_substeps: config.player.contact_substeps.max(1) as usize,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct TerrainSupport {
    height: f32,
    normal: Vec3,
}

impl TerrainSupport {
    fn is_walkable(self, max_ground_normal_y: f32) -> bool {
        self.normal.y >= max_ground_normal_y
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct HorizontalContactResult {
    position: Vec2,
    support: TerrainSupport,
    blocked: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct VerticalContactResult {
    y: f32,
    vertical_velocity: f32,
    grounded: bool,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct VerticalContactInput {
    current_y: f32,
    ground_y: f32,
    vertical_velocity: f32,
    grounded: bool,
    jump_requested: bool,
    delta_secs: f32,
    jump_velocity: f32,
    gravity: f32,
}

fn initialize_first_person_state(
    config: Res<AppConfig>,
    state: Option<ResMut<FirstPersonState>>,
    bootstrap: Option<ResMut<FirstPersonBootstrap>>,
    mut player_query: Query<&mut Transform, With<WandererPrototype>>,
    mut camera_query: Query<&mut Transform, (With<WorldCamera>, Without<WandererPrototype>)>,
) {
    let (Some(mut state), Some(mut bootstrap)) = (state, bootstrap) else {
        return;
    };
    if !bootstrap.pending {
        return;
    }
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
    bootstrap.pending = false;
}

fn start_player_session(mut commands: Commands) {
    commands.insert_resource(FirstPersonState::default());
    commands.insert_resource(FirstPersonBootstrap { pending: true });
}

fn lock_cursor_for_running_session(
    session_mode: Res<SessionMode>,
    state: Option<ResMut<FirstPersonState>>,
    mut cursor_query: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if *session_mode != SessionMode::Exploration {
        return;
    }

    if let Some(mut state) = state {
        state.cursor_locked = true;
    }
    let Some(mut cursor_options) = cursor_query.iter_mut().next() else {
        return;
    };
    cursor_options.visible = false;
    cursor_options.grab_mode = CursorGrabMode::Locked;
}

fn release_cursor_for_pause(
    state: Option<ResMut<FirstPersonState>>,
    mut cursor_query: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if let Some(mut state) = state {
        state.cursor_locked = false;
    }
    let Some(mut cursor_options) = cursor_query.iter_mut().next() else {
        return;
    };
    cursor_options.visible = true;
    cursor_options.grab_mode = CursorGrabMode::None;
}

fn release_cursor_to_menu(mut cursor_query: Query<&mut CursorOptions, With<PrimaryWindow>>) {
    let Some(mut cursor_options) = cursor_query.iter_mut().next() else {
        return;
    };
    cursor_options.visible = true;
    cursor_options.grab_mode = CursorGrabMode::None;
}

fn end_player_session(
    mut commands: Commands,
    mut cursor_query: Query<&mut CursorOptions, With<PrimaryWindow>>,
) {
    if let Some(mut cursor_options) = cursor_query.iter_mut().next() {
        cursor_options.visible = true;
        cursor_options.grab_mode = CursorGrabMode::None;
    }
    commands.remove_resource::<FirstPersonState>();
    commands.remove_resource::<FirstPersonBootstrap>();
}

fn apply_mouse_look(
    config: Res<AppConfig>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    state: Option<ResMut<FirstPersonState>>,
    bootstrap: Option<Res<FirstPersonBootstrap>>,
) {
    let Some(bootstrap) = bootstrap else {
        return;
    };
    if bootstrap.pending {
        return;
    }
    let Some(mut state) = state else {
        return;
    };
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
    resources: PlayerMoveResources<'_>,
    state: Option<ResMut<FirstPersonState>>,
    bootstrap: Option<Res<FirstPersonBootstrap>>,
    mut player_query: Query<&mut Transform, With<WandererPrototype>>,
) {
    let (time, keys, config, world_map, mut collision_proxy) = resources;
    let Some(bootstrap) = bootstrap else {
        return;
    };
    if bootstrap.pending {
        return;
    }
    let Some(mut state) = state else {
        return;
    };
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
    let walker = TerrainWalkerConfig::from_app_config(&config);
    let ground_sampler = &mut |x: f32, z: f32| collision_proxy.sample_ground(&world_map, x, z);
    let horizontal_contact = resolve_horizontal_contact(
        Vec2::new(transform.translation.x, transform.translation.z),
        Vec2::new(horizontal_delta.x, horizontal_delta.z),
        walker,
        ground_sampler,
    );
    let Some(horizontal_contact) = horizontal_contact else {
        return;
    };
    transform.translation.x = horizontal_contact.position.x;
    transform.translation.z = horizontal_contact.position.y;

    let vertical_contact = resolve_vertical_contact(
        VerticalContactInput {
            current_y: transform.translation.y,
            ground_y: horizontal_contact.support.height + config.player.body_height,
            vertical_velocity: state.vertical_velocity,
            grounded: state.grounded,
            jump_requested: keys.just_pressed(KeyCode::Space),
            delta_secs: time.delta_secs(),
            jump_velocity: config.player.jump_velocity,
            gravity: config.player.gravity,
        },
        walker,
    );
    transform.translation.y = vertical_contact.y;
    state.vertical_velocity = vertical_contact.vertical_velocity;
    state.grounded = vertical_contact.grounded;

    transform.rotation = Quat::from_rotation_y(state.yaw);
}

fn resolve_horizontal_contact(
    start: Vec2,
    desired_delta: Vec2,
    settings: TerrainWalkerConfig,
    mut sample_ground: impl FnMut(f32, f32) -> Option<TerrainCollisionSample>,
) -> Option<HorizontalContactResult> {
    let mut position = start;
    let mut support =
        sample_capsule_support(position, settings.capsule_radius, &mut sample_ground)?;
    if desired_delta.length_squared() <= f32::EPSILON {
        return Some(HorizontalContactResult {
            position,
            support,
            blocked: false,
        });
    }

    let distance_steps =
        (desired_delta.length() / settings.capsule_radius.max(0.05)).ceil() as usize;
    let step_count = settings.contact_substeps.max(distance_steps.max(1));
    let step_delta = desired_delta / step_count as f32;
    let mut blocked = false;

    for _ in 0..step_count {
        let Some((next_position, next_support)) =
            try_horizontal_step(position, support, step_delta, settings, &mut sample_ground)
        else {
            blocked = true;
            break;
        };
        position = next_position;
        support = next_support;
    }

    Some(HorizontalContactResult {
        position,
        support,
        blocked,
    })
}

fn try_horizontal_step(
    position: Vec2,
    current_support: TerrainSupport,
    step_delta: Vec2,
    settings: TerrainWalkerConfig,
    sample_ground: &mut impl FnMut(f32, f32) -> Option<TerrainCollisionSample>,
) -> Option<(Vec2, TerrainSupport)> {
    for axis_delta in [
        step_delta,
        Vec2::new(step_delta.x, 0.0),
        Vec2::new(0.0, step_delta.y),
    ] {
        if axis_delta.length_squared() <= f32::EPSILON {
            continue;
        }
        let candidate_position = position + axis_delta;
        let candidate_support =
            sample_capsule_support(candidate_position, settings.capsule_radius, sample_ground)?;
        if can_advance_on_support(current_support, candidate_support, settings) {
            return Some((candidate_position, candidate_support));
        }
    }
    None
}

fn can_advance_on_support(
    current_support: TerrainSupport,
    candidate_support: TerrainSupport,
    settings: TerrainWalkerConfig,
) -> bool {
    let rise = candidate_support.height - current_support.height;
    if rise <= 0.0 {
        return true;
    }

    rise <= settings.step_height && candidate_support.is_walkable(settings.max_ground_normal_y)
}

fn sample_capsule_support(
    center: Vec2,
    capsule_radius: f32,
    sample_ground: &mut impl FnMut(f32, f32) -> Option<TerrainCollisionSample>,
) -> Option<TerrainSupport> {
    let center_sample = sample_ground(center.x, center.y)?;
    let mut support = TerrainSupport {
        height: center_sample.height,
        normal: center_sample.normal,
    };

    for direction in CAPSULE_SUPPORT_DIRECTIONS {
        let offset = direction * capsule_radius;
        let Some(sample) = sample_ground(center.x + offset.x, center.y + offset.y) else {
            continue;
        };
        if sample.height > support.height {
            support.height = sample.height;
            support.normal = sample.normal;
        }
    }

    Some(support)
}

fn resolve_vertical_contact(
    input: VerticalContactInput,
    settings: TerrainWalkerConfig,
) -> VerticalContactResult {
    let mut next_grounded = input.grounded;
    let mut next_velocity = input.vertical_velocity;
    let mut next_y = input.current_y;

    if next_grounded && input.jump_requested {
        next_grounded = false;
        next_velocity = input.jump_velocity;
    }

    if next_grounded {
        if input.ground_y >= input.current_y {
            return VerticalContactResult {
                y: input.ground_y,
                vertical_velocity: 0.0,
                grounded: true,
            };
        }

        if input.current_y - input.ground_y <= settings.ground_snap_distance {
            return VerticalContactResult {
                y: input.ground_y,
                vertical_velocity: 0.0,
                grounded: true,
            };
        }

        next_velocity = 0.0;
    }

    next_velocity -= input.gravity * input.delta_secs;
    next_y += next_velocity * input.delta_secs;
    if next_y <= input.ground_y {
        return VerticalContactResult {
            y: input.ground_y,
            vertical_velocity: 0.0,
            grounded: true,
        };
    }

    VerticalContactResult {
        y: next_y,
        vertical_velocity: next_velocity,
        grounded: false,
    }
}

fn sync_camera_to_player(
    config: Res<AppConfig>,
    state: Option<Res<FirstPersonState>>,
    bootstrap: Option<Res<FirstPersonBootstrap>>,
    player_query: Query<&Transform, With<WandererPrototype>>,
    mut camera_query: Query<&mut Transform, (With<WorldCamera>, Without<WandererPrototype>)>,
) {
    let Some(bootstrap) = bootstrap else {
        return;
    };
    if bootstrap.pending {
        return;
    }
    let Some(state) = state else {
        return;
    };
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
    use bevy::prelude::{Vec2, Vec3};

    use crate::game::world::TerrainCollisionSample;

    use super::{
        FirstPersonState, TerrainSupport, TerrainWalkerConfig, VerticalContactInput,
        resolve_horizontal_contact, resolve_vertical_contact,
    };

    #[test]
    fn default_first_person_state_starts_grounded_and_locked() {
        let state = FirstPersonState::default();
        assert!(state.grounded);
        assert!(state.cursor_locked);
        assert_eq!(state.pitch, 0.0);
        assert_eq!(state.yaw, 0.0);
    }

    fn flat_sample(height: f32) -> TerrainCollisionSample {
        TerrainCollisionSample {
            height,
            normal: Vec3::Y,
            slope: 0.0,
        }
    }

    fn walker_config() -> TerrainWalkerConfig {
        TerrainWalkerConfig {
            capsule_radius: 0.4,
            max_ground_normal_y: 0.7,
            step_height: 0.75,
            ground_snap_distance: 1.0,
            contact_substeps: 4,
        }
    }

    #[test]
    fn capsule_contact_can_step_up_small_height_change() {
        let result =
            resolve_horizontal_contact(Vec2::ZERO, Vec2::new(1.2, 0.0), walker_config(), |x, _| {
                if x < 0.8 {
                    Some(flat_sample(0.0))
                } else {
                    Some(flat_sample(0.5))
                }
            })
            .expect("contact should resolve");

        assert!(result.position.x > 1.0);
        assert_eq!(
            result.support,
            TerrainSupport {
                height: 0.5,
                normal: Vec3::Y,
            }
        );
    }

    #[test]
    fn capsule_contact_blocks_steep_uphill_surface() {
        let result =
            resolve_horizontal_contact(Vec2::ZERO, Vec2::new(1.2, 0.0), walker_config(), |x, _| {
                if x < 0.7 {
                    Some(flat_sample(0.0))
                } else {
                    Some(TerrainCollisionSample {
                        height: 0.55,
                        normal: Vec3::new(0.0, 0.45, 0.89).normalize(),
                        slope: 1.3,
                    })
                }
            })
            .expect("contact should resolve");

        assert!(result.blocked);
        assert!(result.position.x < 0.8);
    }

    #[test]
    fn vertical_contact_snaps_small_drop_and_keeps_grounded() {
        let result = resolve_vertical_contact(
            VerticalContactInput {
                current_y: 2.0,
                ground_y: 1.35,
                vertical_velocity: 0.0,
                grounded: true,
                jump_requested: false,
                delta_secs: 0.016,
                jump_velocity: 6.0,
                gravity: 18.0,
            },
            walker_config(),
        );

        assert!(result.grounded);
        assert_eq!(result.y, 1.35);
        assert_eq!(result.vertical_velocity, 0.0);
    }

    #[test]
    fn vertical_contact_starts_fall_on_large_drop() {
        let result = resolve_vertical_contact(
            VerticalContactInput {
                current_y: 2.0,
                ground_y: 0.2,
                vertical_velocity: 0.0,
                grounded: true,
                jump_requested: false,
                delta_secs: 0.1,
                jump_velocity: 6.0,
                gravity: 18.0,
            },
            walker_config(),
        );

        assert!(!result.grounded);
        assert!(result.y < 2.0);
        assert!(result.vertical_velocity < 0.0);
    }
}
