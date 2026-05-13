use bevy::{
    input::mouse::{AccumulatedMouseMotion, AccumulatedMouseScroll},
    prelude::*,
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};

use crate::{
    core::{
        config::AppConfig,
        performance::{FramePerformance, PerformancePhase},
    },
    game::{
        flow::{AppScreen, InGameState, SessionMode, in_session_mode},
        regions::RegionGraphState,
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
                handle_camera_mode_toggle,
                apply_mouse_look,
                move_player_body,
                sync_camera_to_player,
                update_player_body_visibility,
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
    pub camera_mode: CameraMode,
    pub third_person_distance: f32,
    pub animation_state: PlayerAnimationState,
}

impl Default for FirstPersonState {
    fn default() -> Self {
        Self {
            yaw: 0.0,
            pitch: 0.0,
            vertical_velocity: 0.0,
            grounded: true,
            cursor_locked: true,
            camera_mode: CameraMode::FirstPerson,
            third_person_distance: THIRD_PERSON_DEFAULT_DISTANCE,
            animation_state: PlayerAnimationState::Idle,
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum CameraMode {
    FirstPerson,
    ThirdPerson,
}

impl CameraMode {
    pub fn label(self) -> &'static str {
        match self {
            Self::FirstPerson => "第一人称",
            Self::ThirdPerson => "第三人称",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum PlayerAnimationState {
    Idle,
    Walk,
    Run,
    Jump,
}

impl PlayerAnimationState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Idle => "站立",
            Self::Walk => "行走",
            Self::Run => "奔跑",
            Self::Jump => "跳跃",
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
const THIRD_PERSON_DEFAULT_DISTANCE: f32 = 6.2;

type PlayerMoveResources<'w> = (
    Res<'w, Time>,
    Res<'w, ButtonInput<KeyCode>>,
    Res<'w, AppConfig>,
    Res<'w, WorldMap>,
    ResMut<'w, TerrainCollisionProxy>,
);

type CameraSyncResources<'w> = (
    Res<'w, Time>,
    Res<'w, AppConfig>,
    Option<Res<'w, FirstPersonState>>,
    Option<Res<'w, FirstPersonBootstrap>>,
    Option<Res<'w, WorldMap>>,
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
    state.camera_mode = CameraMode::FirstPerson;
    state.third_person_distance = config.camera.third_person_default_distance;
    state.animation_state = PlayerAnimationState::Idle;

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

fn handle_camera_mode_toggle(
    config: Res<AppConfig>,
    keys: Res<ButtonInput<KeyCode>>,
    scroll: Res<AccumulatedMouseScroll>,
    state: Option<ResMut<FirstPersonState>>,
) {
    let Some(mut state) = state else {
        return;
    };
    if keys.just_pressed(KeyCode::KeyV) {
        state.camera_mode = match state.camera_mode {
            CameraMode::FirstPerson => CameraMode::ThirdPerson,
            CameraMode::ThirdPerson => CameraMode::FirstPerson,
        };
        tracing::info!(
            target: "dao_game::player::camera",
            mode = state.camera_mode.label(),
            "camera mode changed"
        );
    }

    if state.camera_mode == CameraMode::ThirdPerson && scroll.delta.y.abs() > f32::EPSILON {
        state.third_person_distance = (state.third_person_distance - scroll.delta.y * 0.32).clamp(
            config.camera.third_person_min_distance,
            config.camera.third_person_max_distance,
        );
    }
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
    mut performance: ResMut<FramePerformance>,
    state: Option<ResMut<FirstPersonState>>,
    bootstrap: Option<Res<FirstPersonBootstrap>>,
    regions: Option<Res<RegionGraphState>>,
    mut player_query: Query<&mut Transform, With<WandererPrototype>>,
) {
    let started_at = std::time::Instant::now();
    let (time, keys, config, world_map, mut collision_proxy) = resources;
    let Some(bootstrap) = bootstrap else {
        return;
    };
    if bootstrap.pending {
        return;
    }
    if regions
        .as_deref()
        .is_some_and(|regions| regions.crossing.is_some())
    {
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
    state.animation_state =
        animation_state_for_movement(movement.length_squared(), sprint_multiplier, state.grounded);

    transform.rotation = Quat::from_rotation_y(state.yaw);
    performance.record_phase_duration(PerformancePhase::Player, started_at.elapsed());
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
    resources: CameraSyncResources<'_>,
    player_query: Query<&Transform, With<WandererPrototype>>,
    mut camera_query: Query<&mut Transform, (With<WorldCamera>, Without<WandererPrototype>)>,
) {
    let (time, config, state, bootstrap, world_map, mut collision_proxy) = resources;
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

    match state.camera_mode {
        CameraMode::FirstPerson => {
            camera_transform.translation =
                player_transform.translation + Vec3::Y * config.player.eye_height;
            camera_transform.rotation =
                Quat::from_euler(EulerRot::YXZ, state.yaw, state.pitch, 0.0);
        }
        CameraMode::ThirdPerson => {
            let desired = third_person_camera_position(
                player_transform.translation,
                state.yaw,
                state.pitch,
                state.third_person_distance,
                &config,
            );
            let adjusted = if let Some(world_map) = world_map.as_deref() {
                avoid_terrain_for_camera(desired, &mut collision_proxy, world_map, &config)
            } else {
                desired
            };
            let blend =
                1.0 - (-config.camera.third_person_smoothness.max(0.1) * time.delta_secs()).exp();
            camera_transform.translation = camera_transform.translation.lerp(adjusted, blend);
            camera_transform.look_at(
                player_transform.translation + Vec3::Y * (config.player.eye_height * 0.82),
                Vec3::Y,
            );
        }
    }
}

fn update_player_body_visibility(
    state: Option<Res<FirstPersonState>>,
    mut query: Query<&mut Visibility, With<WandererPrototype>>,
) {
    let Some(state) = state else {
        return;
    };
    for mut visibility in &mut query {
        *visibility = if state.camera_mode == CameraMode::ThirdPerson {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

pub fn third_person_camera_position(
    player_position: Vec3,
    yaw: f32,
    pitch: f32,
    distance: f32,
    config: &AppConfig,
) -> Vec3 {
    let yaw_rotation = Quat::from_rotation_y(yaw);
    let backward = yaw_rotation * Vec3::Z;
    let right = yaw_rotation * Vec3::X;
    let pitch_lift = (-pitch).clamp(-0.45, 0.65) * 1.2;
    player_position
        + Vec3::Y * (config.camera.third_person_height + pitch_lift)
        + backward
            * distance.clamp(
                config.camera.third_person_min_distance,
                config.camera.third_person_max_distance,
            )
        + right * config.camera.third_person_side_offset
}

fn avoid_terrain_for_camera(
    desired: Vec3,
    collision_proxy: &mut TerrainCollisionProxy,
    world_map: &WorldMap,
    config: &AppConfig,
) -> Vec3 {
    let Some(ground) = collision_proxy.sample_height(world_map, desired.x, desired.z) else {
        return desired;
    };
    let min_y = ground + config.camera.third_person_ground_clearance.max(0.1);
    if desired.y < min_y {
        Vec3::new(desired.x, min_y, desired.z)
    } else {
        desired
    }
}

fn animation_state_for_movement(
    movement_len_sq: f32,
    sprint_multiplier: f32,
    grounded: bool,
) -> PlayerAnimationState {
    if !grounded {
        PlayerAnimationState::Jump
    } else if movement_len_sq <= f32::EPSILON {
        PlayerAnimationState::Idle
    } else if sprint_multiplier > 1.05 {
        PlayerAnimationState::Run
    } else {
        PlayerAnimationState::Walk
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use bevy::prelude::{Vec2, Vec3};

    use crate::{
        core::config::{
            AppConfig, AssetConfig, CameraConfig, DesertConfig, EcologyConfig, EnvironmentConfig,
            PlayerConfig, PresentationConfig, QualityConfig, SignConfig, WorldConfig,
        },
        game::world::TerrainCollisionSample,
    };

    use super::{
        CameraMode, FirstPersonState, PlayerAnimationState, TerrainSupport, TerrainWalkerConfig,
        VerticalContactInput, animation_state_for_movement, resolve_horizontal_contact,
        resolve_vertical_contact, third_person_camera_position,
    };

    #[test]
    fn default_first_person_state_starts_grounded_and_locked() {
        let state = FirstPersonState::default();
        assert!(state.grounded);
        assert!(state.cursor_locked);
        assert_eq!(state.pitch, 0.0);
        assert_eq!(state.yaw, 0.0);
        assert_eq!(state.camera_mode, CameraMode::FirstPerson);
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

    #[test]
    fn third_person_camera_sits_behind_player() {
        let player = Vec3::new(0.0, 1.2, 0.0);
        let config = test_config();
        let camera = third_person_camera_position(player, 0.0, 0.0, 6.0, &config);

        assert!(camera.z > player.z);
        assert!(camera.y > player.y);
    }

    #[test]
    fn animation_state_tracks_ground_and_speed() {
        assert_eq!(
            animation_state_for_movement(0.0, 1.0, true),
            PlayerAnimationState::Idle
        );
        assert_eq!(
            animation_state_for_movement(1.0, 1.0, true),
            PlayerAnimationState::Walk
        );
        assert_eq!(
            animation_state_for_movement(1.0, 1.7, true),
            PlayerAnimationState::Run
        );
        assert_eq!(
            animation_state_for_movement(1.0, 1.0, false),
            PlayerAnimationState::Jump
        );
    }

    fn test_config() -> AppConfig {
        AppConfig {
            window_title: "Dao".to_string(),
            log_directory: PathBuf::from("logs"),
            performance_log_name: "performance.log".to_string(),
            frame_log_interval: 60,
            performance_detail_interval: 1,
            presentation: PresentationConfig {
                enabled: false,
                scene_duration_seconds: 7.0,
                camera_blend_speed: 2.0,
            },
            world: WorldConfig {
                seed: 42,
                world_radius: 64,
                chunk_radius: 4,
                cell_size: 3.2,
                terrain_subdivisions: 6,
                terrain_scale: 18.0,
                height_variation: 6.0,
                water_level: -0.2,
                noise_octaves: 5,
                ridge_sharpness: 2.1,
                shoreline_blend: 0.2,
                river_frequency: 0.19,
                river_depth: 0.72,
                erosion_strength: 0.52,
                sediment_bias: 0.28,
                visible_chunk_radius: 2,
                high_detail_chunk_radius: 1,
                low_detail_chunk_radius: 2,
                preload_chunk_radius: 3,
                impostor_chunk_radius: 6,
                impostor_radial_bands: 3,
                impostor_angular_segments: 32,
                showcase_search_radius: 24,
                streaming_chunk_budget: 1,
                background_generation_budget: 2,
                streaming_cache_capacity: 32,
                collision_proxy_radius: 1,
                collision_subdivisions: 8,
                collision_chunk_budget: 1,
                collision_cache_capacity: 16,
                material_texture_resolution: 64,
                detail_density: 1.0,
            },
            environment: EnvironmentConfig {
                day_length_seconds: 180.0,
                wander_radius: 4.5,
                wander_speed: 0.7,
            },
            player: PlayerConfig {
                walk_speed: 7.0,
                sprint_multiplier: 1.6,
                mouse_sensitivity: 0.002,
                eye_height: 1.65,
                body_height: 1.2,
                capsule_radius: 0.4,
                max_slope_degrees: 45.0,
                step_height: 0.6,
                ground_snap_distance: 1.0,
                contact_substeps: 4,
                jump_velocity: 6.0,
                gravity: 18.0,
            },
            camera: CameraConfig {
                third_person_default_distance: 6.2,
                third_person_min_distance: 3.2,
                third_person_max_distance: 9.5,
                third_person_height: 2.25,
                third_person_side_offset: 0.42,
                third_person_smoothness: 12.0,
                third_person_ground_clearance: 0.55,
            },
            ecology: EcologyConfig {
                bird_count: 18,
                fish_count: 10,
                sheep_count: 9,
                state_update_interval_seconds: 0.2,
                visual_update_interval_seconds: 0.066,
                max_visible_bird_distance: 240.0,
                max_visible_fish_distance: 90.0,
                max_visible_sheep_distance: 120.0,
            },
            assets: AssetConfig {
                color_saturation: 1.0,
                warm_light_intensity: 1.0,
                water_alpha: 0.64,
                shadow_alpha: 0.58,
                foundation_proxy_mode: true,
                animate_placeholder_characters: false,
                animate_placeholder_ambience: false,
            },
            desert: DesertConfig {
                dune_height: 3.2,
                dune_frequency: 0.22,
                gobi_flatness: 0.48,
                oasis_radius: 38.0,
                oasis_moisture: 0.86,
                sandstorm_visibility: 46.0,
                sandstorm_particle_strength: 1.0,
                sandstorm_wind_speed: 4.2,
            },
            signs: SignConfig {
                resonance_threshold: 0.7,
                resonance_smoothing: 0.12,
                calm_recovery: 0.01,
                calm_threshold: 0.35,
                omen_beacon_height: 3.0,
            },
            quality: QualityConfig {
                target_fps: 60.0,
                frame_time_budget_ms: 16.6,
            },
        }
    }
}
