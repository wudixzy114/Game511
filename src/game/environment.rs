use std::f32::consts::{PI, TAU};

use bevy::{
    color::LinearRgba,
    light::NotShadowCaster,
    math::primitives::{Cuboid, Sphere},
    pbr::{DistanceFog, FogFalloff, MeshMaterial3d},
    prelude::*,
};

use crate::core::{
    config::AppConfig,
    performance::{FramePerformance, PerformancePhase},
};
use crate::game::{
    flow::{AppScreen, InGameState},
    journey::JourneyState,
    world::{SunLight, WorldCamera, WorldCycle, WorldPresentationControl},
};

pub struct EnvironmentPlugin;

impl Plugin for EnvironmentPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(WeatherState::default());
        app.insert_resource(WeatherTransition::default());
        app.insert_resource(EnvironmentSnapshot::default());
        app.insert_resource(WindField::default());
        app.insert_resource(EnvironmentTelemetry::default());
        app.add_systems(OnEnter(AppScreen::InGame), reset_environment_state);
        app.add_systems(
            First,
            initialize_environment_scene.run_if(in_state(AppScreen::InGame)),
        );
        app.add_systems(
            Update,
            (
                begin_environment_phase,
                advance_weather_state,
                update_environment_snapshot,
                sync_environment_anchors,
                update_atmosphere_and_fog,
                update_celestial_visuals,
                animate_weather_particles,
                end_environment_phase,
            )
                .chain()
                .run_if(in_state(InGameState::Running)),
        );
        app.add_systems(OnExit(AppScreen::InGame), cleanup_environment_session);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WeatherKind {
    Clear,
    Mist,
    Rain,
    Storm,
    Sandstorm,
    Snow,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct WeatherProfile {
    sky_day: Vec3,
    sky_night: Vec3,
    horizon_glow: Vec3,
    fog_day: Vec3,
    fog_night: Vec3,
    inscatter_day: Vec3,
    inscatter_night: Vec3,
    ambient_color: Vec3,
    visibility: f32,
    ambient_brightness: f32,
    sun_scale: f32,
    moon_scale: f32,
    star_scale: f32,
    cloud_cover: f32,
    precipitation_strength: f32,
    particle_speed: f32,
    particle_sway: f32,
    particle_alpha: f32,
    wind: Vec2,
    lightning_strength: f32,
}

#[derive(Debug, Resource, Clone, Copy, PartialEq, Eq)]
struct WeatherState {
    previous: WeatherKind,
    current: WeatherKind,
}

impl Default for WeatherState {
    fn default() -> Self {
        Self {
            previous: WeatherKind::Clear,
            current: WeatherKind::Clear,
        }
    }
}

#[derive(Debug, Resource, Clone, Copy, PartialEq)]
struct WeatherTransition {
    blend: f32,
}

impl Default for WeatherTransition {
    fn default() -> Self {
        Self { blend: 1.0 }
    }
}

#[derive(Debug, Resource, Clone)]
struct EnvironmentAssets {
    sun_material: Handle<StandardMaterial>,
    moon_material: Handle<StandardMaterial>,
    star_material: Handle<StandardMaterial>,
    particle_material: Handle<StandardMaterial>,
}

#[derive(Debug, Component)]
struct SkyAnchor;

#[derive(Debug, Component)]
struct WeatherAnchor;

#[derive(Debug, Component)]
struct SunDisc;

#[derive(Debug, Component)]
struct MoonDisc;

#[derive(Debug, Component)]
struct MoonLight;

#[derive(Debug, Component)]
struct LightningFlash;

#[derive(Debug, Component, Clone, Copy, PartialEq)]
struct Star {
    scale: f32,
}

#[derive(Debug, Component, Clone, Copy, PartialEq)]
struct WeatherParticle {
    index: u32,
    seed: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ParticleMode {
    None,
    Mist,
    Rain,
    Sand,
    Snow,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct CelestialFrame {
    sun_position: Vec3,
    moon_position: Vec3,
    sun_visibility: f32,
    moon_visibility: f32,
    night_factor: f32,
    horizon_factor: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct EnvironmentFrame {
    profile: WeatherProfile,
    dominant_kind: WeatherKind,
    celestial: CelestialFrame,
    flash: f32,
}

#[derive(Debug, Resource, Clone, Copy, PartialEq)]
pub struct EnvironmentSnapshot {
    pub weather: WeatherKind,
    pub daylight: f32,
    pub visibility: f32,
    pub humidity: f32,
    pub fog_density: f32,
    pub cloud_cover: f32,
    pub ambient_energy: f32,
    pub sea_mist: f32,
    pub storm_weight: f32,
    pub sandstorm_weight: f32,
    pub snow_weight: f32,
}

impl Default for EnvironmentSnapshot {
    fn default() -> Self {
        Self {
            weather: WeatherKind::Clear,
            daylight: 1.0,
            visibility: 170.0,
            humidity: 0.2,
            fog_density: 0.08,
            cloud_cover: 0.12,
            ambient_energy: 1.0,
            sea_mist: 0.08,
            storm_weight: 0.0,
            sandstorm_weight: 0.0,
            snow_weight: 0.0,
        }
    }
}

#[derive(Debug, Resource, Clone, Copy, PartialEq)]
pub struct WindField {
    pub direction: Vec2,
    pub raw_speed: f32,
    pub speed: f32,
    pub gust: f32,
    pub swirl: f32,
    pub omen_bias: f32,
}

impl Default for WindField {
    fn default() -> Self {
        Self {
            direction: Vec2::ZERO,
            raw_speed: 0.0,
            speed: 0.0,
            gust: 0.0,
            swirl: 0.0,
            omen_bias: 0.0,
        }
    }
}

#[derive(Debug, Resource, Clone, Copy, PartialEq)]
struct EnvironmentTelemetry {
    weather: WeatherKind,
    wind_bucket: i32,
    visibility_bucket: i32,
}

impl Default for EnvironmentTelemetry {
    fn default() -> Self {
        Self {
            weather: WeatherKind::Clear,
            wind_bucket: 0,
            visibility_bucket: 17,
        }
    }
}

const SKY_RADIUS: f32 = 360.0;
const SUN_DISC_SCALE: f32 = 11.0;
const MOON_DISC_SCALE: f32 = 8.2;
const STAR_COUNT: u32 = 220;
const PARTICLE_COUNT: u32 = 240;
const WEATHER_RADIUS: f32 = 20.0;
const WEATHER_TOP: f32 = 16.0;
const WEATHER_BOTTOM: f32 = -3.0;
const WEATHER_SEGMENT_SECONDS: f32 = 30.0;
const WEATHER_TRANSITION_SECONDS: f32 = 3.5;
const WEATHER_SEQUENCE: [WeatherKind; 7] = [
    WeatherKind::Clear,
    WeatherKind::Mist,
    WeatherKind::Rain,
    WeatherKind::Storm,
    WeatherKind::Sandstorm,
    WeatherKind::Clear,
    WeatherKind::Snow,
];

type EnvironmentSnapshotResources<'w> = (
    Res<'w, Time>,
    Res<'w, AppConfig>,
    Res<'w, WorldCycle>,
    Res<'w, WeatherState>,
    Res<'w, WeatherTransition>,
    Option<Res<'w, JourneyState>>,
);

fn begin_environment_phase(mut performance: ResMut<FramePerformance>) {
    performance.begin_phase(PerformancePhase::Environment);
}

fn end_environment_phase(mut performance: ResMut<FramePerformance>) {
    let _ = performance.end_phase(PerformancePhase::Environment);
}

fn reset_environment_state(
    mut weather_state: ResMut<WeatherState>,
    mut transition: ResMut<WeatherTransition>,
) {
    *weather_state = WeatherState::default();
    *transition = WeatherTransition::default();
}

fn initialize_environment_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    camera_query: Query<(Entity, &Transform), With<WorldCamera>>,
    assets: Option<Res<EnvironmentAssets>>,
) {
    if assets.is_some() {
        return;
    }

    let Some((camera_entity, camera_transform)) = camera_query.iter().next() else {
        return;
    };

    let sun_material = materials.add(StandardMaterial {
        base_color: Color::srgb(1.0, 0.86, 0.62),
        emissive: LinearRgba::rgb(12.0, 9.8, 6.0),
        unlit: true,
        cull_mode: None,
        ..Default::default()
    });
    let moon_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.9, 0.93, 1.0),
        emissive: LinearRgba::rgb(1.2, 1.3, 1.6),
        unlit: true,
        cull_mode: None,
        ..Default::default()
    });
    let star_material = materials.add(StandardMaterial {
        base_color: Color::BLACK,
        emissive: LinearRgba::rgb(4.2, 4.4, 5.2),
        unlit: true,
        cull_mode: None,
        ..Default::default()
    });
    let particle_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.75, 0.84, 0.94, 0.0),
        emissive: LinearRgba::BLACK,
        unlit: true,
        alpha_mode: AlphaMode::Blend,
        cull_mode: None,
        ..Default::default()
    });

    let disc_mesh = meshes.add(Sphere::new(1.0).mesh().uv(24, 14));
    let star_mesh = meshes.add(Sphere::new(0.55).mesh().uv(12, 8));
    let particle_mesh = meshes.add(Cuboid::new(1.0, 1.0, 1.0));

    let sky_anchor = commands
        .spawn((
            Name::new("SkyAnchor"),
            DespawnOnExit(AppScreen::InGame),
            Transform::from_translation(camera_transform.translation),
            SkyAnchor,
        ))
        .id();
    let weather_anchor = commands
        .spawn((
            Name::new("WeatherAnchor"),
            DespawnOnExit(AppScreen::InGame),
            Transform::from_translation(camera_transform.translation),
            WeatherAnchor,
        ))
        .id();

    commands.entity(sky_anchor).with_children(|parent| {
        parent.spawn((
            Name::new("SunDisc"),
            Mesh3d(disc_mesh.clone()),
            MeshMaterial3d(sun_material.clone()),
            Transform::from_scale(Vec3::splat(SUN_DISC_SCALE)),
            Visibility::Visible,
            NotShadowCaster,
            SunDisc,
        ));
        parent.spawn((
            Name::new("MoonDisc"),
            Mesh3d(disc_mesh.clone()),
            MeshMaterial3d(moon_material.clone()),
            Transform::from_scale(Vec3::splat(MOON_DISC_SCALE)),
            Visibility::Visible,
            NotShadowCaster,
            MoonDisc,
        ));

        for index in 0..STAR_COUNT {
            let direction = star_direction(index);
            let distance = SKY_RADIUS - 32.0 - hash_range(index, 91, 0.0, 42.0);
            let scale = 0.48 + hash_range(index, 17, 0.0, 1.25);
            parent.spawn((
                Name::new("Star"),
                Mesh3d(star_mesh.clone()),
                MeshMaterial3d(star_material.clone()),
                Transform::from_translation(direction * distance).with_scale(Vec3::splat(scale)),
                NotShadowCaster,
                Star { scale },
            ));
        }
    });

    commands.entity(weather_anchor).with_children(|parent| {
        for index in 0..PARTICLE_COUNT {
            parent.spawn((
                Name::new("WeatherParticle"),
                Mesh3d(particle_mesh.clone()),
                MeshMaterial3d(particle_material.clone()),
                Transform::from_xyz(0.0, -200.0, 0.0),
                Visibility::Hidden,
                NotShadowCaster,
                WeatherParticle {
                    index,
                    seed: index.wrapping_mul(73).wrapping_add(11),
                },
            ));
        }
    });

    commands.spawn((
        Name::new("MoonLight"),
        DespawnOnExit(AppScreen::InGame),
        DirectionalLight {
            illuminance: 0.0,
            shadows_enabled: false,
            color: Color::srgb(0.76, 0.82, 0.96),
            ..Default::default()
        },
        Transform::default(),
        MoonLight,
    ));
    commands.spawn((
        Name::new("LightningFlash"),
        DespawnOnExit(AppScreen::InGame),
        PointLight {
            intensity: 0.0,
            range: 85.0,
            radius: 0.4,
            color: Color::srgb(0.86, 0.9, 1.0),
            shadows_enabled: false,
            ..Default::default()
        },
        Transform::from_xyz(0.0, -80.0, 0.0),
        LightningFlash,
    ));

    commands.entity(camera_entity).insert(DistanceFog {
        color: Color::srgba(0.4, 0.5, 0.6, 1.0),
        directional_light_color: Color::srgba(0.98, 0.88, 0.72, 0.45),
        directional_light_exponent: 26.0,
        falloff: FogFalloff::from_visibility_colors(
            140.0,
            Color::srgb(0.72, 0.84, 0.94),
            Color::srgb(0.96, 0.9, 0.82),
        ),
    });

    commands.insert_resource(EnvironmentAssets {
        sun_material,
        moon_material,
        star_material,
        particle_material,
    });
}

fn advance_weather_state(
    time: Res<Time>,
    control: Option<Res<WorldPresentationControl>>,
    mut weather_state: ResMut<WeatherState>,
    mut transition: ResMut<WeatherTransition>,
) {
    let target = control
        .as_deref()
        .and_then(|control| control.weather_override)
        .unwrap_or_else(|| weather_for_elapsed(time.elapsed_secs()));

    if target != weather_state.current {
        weather_state.previous = weather_state.current;
        weather_state.current = target;
        transition.blend = 0.0;
        tracing::info!(
            target: "dao_game::environment::weather",
            previous = ?weather_state.previous,
            current = ?weather_state.current,
            "environment weather transitioned"
        );
    } else {
        transition.blend =
            (transition.blend + time.delta_secs() / WEATHER_TRANSITION_SECONDS).clamp(0.0, 1.0);
    }
}

fn update_environment_snapshot(
    resources: EnvironmentSnapshotResources<'_>,
    mut snapshot: ResMut<EnvironmentSnapshot>,
    mut wind_field: ResMut<WindField>,
    mut telemetry: ResMut<EnvironmentTelemetry>,
) {
    let (time, config, cycle, weather_state, transition, journey) = resources;
    let environment = build_environment_frame(&time, &config, &cycle, &weather_state, &transition);
    let response = journey
        .as_deref()
        .map(|journey| journey.response.intensity)
        .unwrap_or(0.0);
    let next_snapshot = environment_snapshot_from_profile(
        environment.dominant_kind,
        environment.profile,
        cycle.daylight,
    );
    let next_wind = wind_field_from_profile(
        time.elapsed_secs(),
        environment.dominant_kind,
        environment.profile,
        environment.flash,
        response,
    );
    let wind_bucket = (next_wind.speed * 10.0).round() as i32;
    let visibility_bucket = (next_snapshot.visibility / 10.0).round() as i32;

    if telemetry.weather != next_snapshot.weather
        || telemetry.wind_bucket != wind_bucket
        || telemetry.visibility_bucket != visibility_bucket
    {
        tracing::info!(
            target: "dao_game::environment::wind",
            weather = ?next_snapshot.weather,
            wind_dir_x = next_wind.direction.x,
            wind_dir_z = next_wind.direction.y,
            wind_speed = next_wind.speed,
            wind_gust = next_wind.gust,
            visibility = next_snapshot.visibility,
            humidity = next_snapshot.humidity,
            sea_mist = next_snapshot.sea_mist,
            "environment snapshot updated"
        );
        *telemetry = EnvironmentTelemetry {
            weather: next_snapshot.weather,
            wind_bucket,
            visibility_bucket,
        };
    }

    *snapshot = next_snapshot;
    *wind_field = next_wind;
}

#[allow(clippy::type_complexity)]
fn sync_environment_anchors(
    camera_query: Query<
        &Transform,
        (
            With<WorldCamera>,
            Without<SkyAnchor>,
            Without<WeatherAnchor>,
        ),
    >,
    mut sky_anchor_query: Query<&mut Transform, (With<SkyAnchor>, Without<WeatherAnchor>)>,
    mut weather_anchor_query: Query<&mut Transform, (With<WeatherAnchor>, Without<SkyAnchor>)>,
) {
    let Some(camera_transform) = camera_query.iter().next() else {
        return;
    };
    let translation = camera_transform.translation;

    if let Some(mut transform) = sky_anchor_query.iter_mut().next() {
        transform.translation = translation;
    }
    if let Some(mut transform) = weather_anchor_query.iter_mut().next() {
        transform.translation = translation;
    }
}

#[allow(clippy::type_complexity)]
fn update_atmosphere_and_fog(
    environment_state: (
        Res<Time>,
        Res<AppConfig>,
        Res<WorldCycle>,
        Res<WeatherState>,
        Res<WeatherTransition>,
    ),
    journey: Option<Res<JourneyState>>,
    mut clear_color: ResMut<ClearColor>,
    mut ambient_light: ResMut<GlobalAmbientLight>,
    mut fog_query: Query<&mut DistanceFog, With<WorldCamera>>,
) {
    let (time, config, cycle, weather_state, transition) = environment_state;
    let Some(mut fog) = fog_query.iter_mut().next() else {
        return;
    };

    let environment = build_environment_frame(&time, &config, &cycle, &weather_state, &transition);
    let profile = environment.profile;
    let frame = environment.celestial;
    let flash = environment.flash;
    let response = journey
        .as_deref()
        .map(|journey| journey.response.intensity)
        .unwrap_or(0.0);

    let sky_mix = cycle.daylight.powf(0.62);
    let mut sky_rgb = profile.sky_night.lerp(profile.sky_day, sky_mix);
    let horizon_boost =
        frame.horizon_factor * frame.sun_visibility * (1.0 - profile.cloud_cover * 0.42);
    sky_rgb = sky_rgb.lerp(profile.horizon_glow, horizon_boost * 0.6);
    sky_rgb += Vec3::splat(flash * 0.28 + response * 0.08);
    let sky_rgb = sky_rgb.clamp(Vec3::ZERO, Vec3::ONE);
    clear_color.0 = srgb_color(sky_rgb);

    ambient_light.color = srgb_color(
        profile
            .fog_night
            .lerp(profile.ambient_color, cycle.daylight.powf(0.8)),
    );
    ambient_light.brightness = profile.ambient_brightness
        * (0.16 + cycle.daylight * 0.84 + frame.moon_visibility * 0.12)
        + flash * 190.0
        + response * 120.0;

    let fog_rgb = profile
        .fog_night
        .lerp(profile.fog_day, cycle.daylight.powf(0.72))
        + Vec3::splat(flash * 0.18)
        + Vec3::new(0.08, 0.06, 0.03) * response;
    let fog_rgb = fog_rgb.clamp(Vec3::ZERO, Vec3::ONE);
    let inscatter_rgb = profile
        .inscatter_night
        .lerp(profile.inscatter_day, cycle.daylight.powf(0.7))
        .lerp(
            profile.horizon_glow,
            frame.horizon_factor * 0.35 * frame.sun_visibility,
        )
        + Vec3::splat(flash * 0.26);
    let inscatter_rgb = inscatter_rgb.clamp(Vec3::ZERO, Vec3::ONE);
    fog.color = Color::srgba(fog_rgb.x, fog_rgb.y, fog_rgb.z, 1.0);
    fog.directional_light_color = Color::srgba(
        inscatter_rgb.x,
        inscatter_rgb.y,
        inscatter_rgb.z,
        (0.2 + frame.sun_visibility * 0.6) * (1.0 - profile.cloud_cover * 0.55),
    );
    fog.directional_light_exponent = 18.0 + profile.cloud_cover * 12.0;
    fog.falloff = FogFalloff::from_visibility_colors(
        (profile.visibility - response * 18.0).max(42.0),
        srgb_color(fog_rgb),
        srgb_color(inscatter_rgb),
    );
}

#[allow(clippy::type_complexity)]
fn update_celestial_visuals(
    environment_state: (
        Res<Time>,
        Res<AppConfig>,
        Res<WorldCycle>,
        Res<WeatherState>,
        Res<WeatherTransition>,
    ),
    journey: Option<Res<JourneyState>>,
    environment_assets: Option<Res<EnvironmentAssets>>,
    camera_query: Query<
        &Transform,
        (
            With<WorldCamera>,
            Without<SunLight>,
            Without<MoonLight>,
            Without<LightningFlash>,
            Without<SunDisc>,
            Without<MoonDisc>,
            Without<Star>,
        ),
    >,
    mut materials: ResMut<Assets<StandardMaterial>>,
    mut transform_queries: ParamSet<(
        Query<
            (&mut DirectionalLight, &mut Transform),
            (
                With<SunLight>,
                Without<MoonLight>,
                Without<LightningFlash>,
                Without<SunDisc>,
                Without<MoonDisc>,
                Without<Star>,
            ),
        >,
        Query<
            (&mut DirectionalLight, &mut Transform),
            (
                With<MoonLight>,
                Without<SunLight>,
                Without<LightningFlash>,
                Without<SunDisc>,
                Without<MoonDisc>,
                Without<Star>,
            ),
        >,
        Query<
            (&mut PointLight, &mut Transform),
            (
                With<LightningFlash>,
                Without<SunLight>,
                Without<MoonLight>,
                Without<SunDisc>,
                Without<MoonDisc>,
                Without<Star>,
            ),
        >,
        Query<
            (&mut Transform, &mut Visibility),
            (
                With<SunDisc>,
                Without<MoonDisc>,
                Without<Star>,
                Without<SunLight>,
                Without<MoonLight>,
                Without<LightningFlash>,
            ),
        >,
        Query<
            (&mut Transform, &mut Visibility),
            (
                With<MoonDisc>,
                Without<SunDisc>,
                Without<Star>,
                Without<SunLight>,
                Without<MoonLight>,
                Without<LightningFlash>,
            ),
        >,
        Query<
            (&mut Transform, &Star),
            (
                With<Star>,
                Without<SunDisc>,
                Without<MoonDisc>,
                Without<SunLight>,
                Without<MoonLight>,
                Without<LightningFlash>,
            ),
        >,
    )>,
) {
    let Some(environment_assets) = environment_assets else {
        return;
    };
    let Some(camera_transform) = camera_query.iter().next() else {
        return;
    };
    let camera_translation = camera_transform.translation;
    let (time, config, cycle, weather_state, transition) = environment_state;
    let environment = build_environment_frame(&time, &config, &cycle, &weather_state, &transition);
    let profile = environment.profile;
    let dominant_kind = environment.dominant_kind;
    let frame = environment.celestial;
    let flash = environment.flash;
    let response = journey
        .as_deref()
        .map(|journey| journey.response.intensity)
        .unwrap_or(0.0);

    if let Some((mut light, mut transform)) = transform_queries.p0().iter_mut().next() {
        transform.look_at(-frame.sun_position, Vec3::Y);
        let sun_color =
            Vec3::new(1.0, 0.58, 0.34).lerp(Vec3::new(1.0, 0.95, 0.82), cycle.daylight.powf(0.42));
        light.color = srgb_color(sun_color);
        light.illuminance = (4_200.0 + cycle.daylight.powf(0.92) * 74_000.0)
            * frame.sun_visibility
            * profile.sun_scale
            * (1.0 - profile.cloud_cover * 0.46)
            + flash * 21_000.0
            + response * 14_000.0;
    }
    if let Some((mut light, mut transform)) = transform_queries.p1().iter_mut().next() {
        transform.look_at(-frame.moon_position, Vec3::Y);
        light.color = Color::srgb(0.74, 0.8, 0.96);
        light.illuminance = 1_650.0
            * frame.moon_visibility
            * profile.moon_scale
            * (1.0 - profile.cloud_cover * 0.38);
    }
    if let Some((mut light, mut transform)) = transform_queries.p2().iter_mut().next() {
        transform.translation = camera_translation + Vec3::new(12.0, 20.0, -10.0);
        light.intensity = flash * 420_000.0;
        light.range = 70.0 + flash * 30.0;
    }

    if let Some((mut transform, mut visibility)) = transform_queries.p3().iter_mut().next() {
        transform.translation = frame.sun_position;
        transform.scale = Vec3::splat(SUN_DISC_SCALE + frame.horizon_factor * 3.2);
        *visibility = if frame.sun_visibility > 0.01 {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
    if let Some((mut transform, mut visibility)) = transform_queries.p4().iter_mut().next() {
        transform.translation = frame.moon_position;
        transform.scale = Vec3::splat(MOON_DISC_SCALE + frame.moon_visibility * 1.5);
        *visibility = if frame.moon_visibility > 0.01 {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }

    let star_strength = profile.star_scale
        * frame.night_factor.powf(1.55)
        * (1.0 - profile.cloud_cover * 0.9)
        * (1.0 - frame.moon_visibility * 0.42);
    for (mut transform, star) in &mut transform_queries.p5() {
        let twinkle = 0.92
            + (transform.translation.x * 0.013 + time.elapsed_secs() * 0.6)
                .sin()
                .abs()
                * 0.18;
        transform.scale = Vec3::splat(star.scale * (0.92 + star_strength * 0.08 * twinkle));
    }

    if let Some(material) = materials.get_mut(&environment_assets.sun_material) {
        let sun_emission = 9.0 + frame.sun_visibility * 22.0;
        material.base_color = Color::srgb(1.0, 0.84, 0.6);
        material.emissive =
            LinearRgba::rgb(1.0 * sun_emission, 0.82 * sun_emission, 0.48 * sun_emission)
                + LinearRgba::rgb(response * 2.6, response * 1.8, response * 0.7);
    }
    if let Some(material) = materials.get_mut(&environment_assets.moon_material) {
        let moon_emission = 0.35 + frame.moon_visibility * profile.moon_scale * 2.2;
        material.base_color = Color::srgb(0.92, 0.95, 1.0);
        material.emissive = LinearRgba::rgb(
            0.58 * moon_emission,
            0.66 * moon_emission,
            0.88 * moon_emission,
        );
    }
    if let Some(material) = materials.get_mut(&environment_assets.star_material) {
        let emission = 1.4 + star_strength * 6.5;
        material.emissive = LinearRgba::rgb(0.86 * emission, 0.9 * emission, 1.0 * emission);
    }
    if let Some(material) = materials.get_mut(&environment_assets.particle_material) {
        let particle_mode = particle_mode_for_weather(dominant_kind);
        match particle_mode {
            ParticleMode::None => {
                material.base_color = Color::srgba(0.7, 0.8, 0.9, 0.0);
                material.emissive = LinearRgba::BLACK;
            }
            ParticleMode::Mist => {
                material.base_color = Color::srgba(0.84, 0.89, 0.95, profile.particle_alpha);
                material.emissive = LinearRgba::rgb(0.03, 0.03, 0.04);
            }
            ParticleMode::Rain => {
                material.base_color = Color::srgba(0.7, 0.8, 0.94, profile.particle_alpha);
                material.emissive = LinearRgba::rgb(0.08, 0.1, 0.14);
            }
            ParticleMode::Sand => {
                material.base_color = Color::srgba(0.78, 0.56, 0.32, profile.particle_alpha);
                material.emissive = LinearRgba::rgb(0.12, 0.08, 0.035);
            }
            ParticleMode::Snow => {
                material.base_color = Color::srgba(0.95, 0.97, 1.0, profile.particle_alpha);
                material.emissive = LinearRgba::rgb(0.05, 0.06, 0.08);
            }
        }
    }
}

fn build_environment_frame(
    time: &Time,
    config: &AppConfig,
    cycle: &WorldCycle,
    weather_state: &WeatherState,
    transition: &WeatherTransition,
) -> EnvironmentFrame {
    let profile = blended_profile(
        weather_profile(weather_state.previous, config),
        weather_profile(weather_state.current, config),
        smoothstep_unit(transition.blend),
    );
    let dominant_kind = dominant_weather_kind(weather_state, transition);
    let celestial = celestial_frame(cycle.normalized_time);
    let flash = if dominant_kind == WeatherKind::Storm {
        storm_flash(time.elapsed_secs()) * profile.lightning_strength
    } else {
        0.0
    };

    EnvironmentFrame {
        profile,
        dominant_kind,
        celestial,
        flash,
    }
}

fn environment_snapshot_from_profile(
    dominant_kind: WeatherKind,
    profile: WeatherProfile,
    daylight: f32,
) -> EnvironmentSnapshot {
    let fog_density = (1.0 - profile.visibility / 180.0).clamp(0.0, 1.0);
    let precipitation = profile.precipitation_strength.clamp(0.0, 1.0);
    let humidity = match dominant_kind {
        WeatherKind::Clear => (0.18 + profile.cloud_cover * 0.18).clamp(0.0, 1.0),
        WeatherKind::Mist => 0.82,
        WeatherKind::Rain => 0.9,
        WeatherKind::Storm => 1.0,
        WeatherKind::Sandstorm => 0.24,
        WeatherKind::Snow => 0.76,
    };
    let sea_mist = match dominant_kind {
        WeatherKind::Mist => 0.86,
        WeatherKind::Rain | WeatherKind::Storm => 0.44,
        WeatherKind::Sandstorm => 0.08,
        WeatherKind::Snow => 0.24,
        WeatherKind::Clear => 0.12 + fog_density * 0.42,
    }
    .clamp(0.0, 1.0);
    let storm_weight = match dominant_kind {
        WeatherKind::Storm => 1.0,
        WeatherKind::Rain => 0.62,
        _ => 0.0,
    };
    let sandstorm_weight = if dominant_kind == WeatherKind::Sandstorm {
        1.0
    } else {
        0.0
    };
    let snow_weight: f32 = if dominant_kind == WeatherKind::Snow {
        1.0
    } else {
        0.0
    };

    EnvironmentSnapshot {
        weather: dominant_kind,
        daylight: daylight.clamp(0.0, 1.0),
        visibility: profile.visibility,
        humidity,
        fog_density,
        cloud_cover: profile.cloud_cover.clamp(0.0, 1.0),
        ambient_energy: (profile.ambient_brightness / 360.0).clamp(0.18, 1.2),
        sea_mist,
        storm_weight,
        sandstorm_weight,
        snow_weight: snow_weight.max(precipitation * 0.55),
    }
}

fn wind_field_from_profile(
    elapsed: f32,
    dominant_kind: WeatherKind,
    profile: WeatherProfile,
    flash: f32,
    response: f32,
) -> WindField {
    let raw_speed = profile.wind.length();
    let direction = profile.wind.normalize_or_zero();
    let normalized_speed = (raw_speed / 4.8).clamp(0.0, 1.0);
    let gust_wave = 0.5 + 0.5 * (elapsed * (0.42 + normalized_speed * 0.58)).sin();
    let weather_bias = match dominant_kind {
        WeatherKind::Storm => 0.34,
        WeatherKind::Sandstorm => 0.42,
        WeatherKind::Mist => 0.18,
        WeatherKind::Snow => 0.16,
        WeatherKind::Rain => 0.22,
        WeatherKind::Clear => 0.08,
    };

    WindField {
        direction,
        raw_speed,
        speed: normalized_speed,
        gust: (normalized_speed * 0.45 + gust_wave * 0.28 + flash * 0.18).clamp(0.0, 1.0),
        swirl: (profile.particle_sway / 1.8).clamp(0.0, 1.0),
        omen_bias: (weather_bias + response * 0.52).clamp(0.0, 1.0),
    }
}

fn dominant_weather_kind(
    weather_state: &WeatherState,
    transition: &WeatherTransition,
) -> WeatherKind {
    if transition.blend < 0.5 {
        weather_state.previous
    } else {
        weather_state.current
    }
}

fn animate_weather_particles(
    time: Res<Time>,
    config: Res<AppConfig>,
    weather_state: Res<WeatherState>,
    transition: Res<WeatherTransition>,
    mut query: Query<(&WeatherParticle, &mut Transform, &mut Visibility)>,
) {
    let dominant_kind = if transition.blend < 0.5 {
        weather_state.previous
    } else {
        weather_state.current
    };
    let profile = weather_profile(dominant_kind, &config);
    let particle_mode = particle_mode_for_weather(dominant_kind);
    let active_count = (PARTICLE_COUNT as f32 * profile.precipitation_strength).round() as u32;

    for (particle, mut transform, mut visibility) in &mut query {
        if particle.index >= active_count || particle_mode == ParticleMode::None {
            *visibility = Visibility::Hidden;
            transform.translation = Vec3::new(0.0, -200.0, 0.0);
            continue;
        }

        *visibility = Visibility::Visible;

        let speed_variation = 0.84 + hash_range(particle.seed, 41, 0.0, 0.48);
        let fall_progress = fract(
            time.elapsed_secs() * profile.particle_speed * speed_variation
                + hash01(particle.seed, 3),
        );
        let base_x = hash_range(particle.seed, 11, -WEATHER_RADIUS, WEATHER_RADIUS);
        let base_z = hash_range(particle.seed, 23, -WEATHER_RADIUS, WEATHER_RADIUS);
        let swirl_phase = time.elapsed_secs() * (0.18 + hash_range(particle.seed, 31, 0.0, 0.34))
            + hash01(particle.seed, 37) * TAU;
        let swirl = Vec2::new(swirl_phase.sin(), (swirl_phase * 1.3).cos()) * profile.particle_sway;
        let wind_offset = profile.wind * (1.0 - fall_progress) * 4.5;
        let height = WEATHER_BOTTOM + (1.0 - fall_progress) * (WEATHER_TOP - WEATHER_BOTTOM);

        transform.translation = Vec3::new(
            base_x + swirl.x + wind_offset.x,
            height,
            base_z + swirl.y + wind_offset.y,
        );

        match particle_mode {
            ParticleMode::Mist => {
                transform.scale = Vec3::new(
                    0.18 + hash_range(particle.seed, 47, 0.0, 0.18),
                    0.08 + hash_range(particle.seed, 59, 0.0, 0.06),
                    0.18 + hash_range(particle.seed, 71, 0.0, 0.18),
                );
                transform.rotation = Quat::from_rotation_y(
                    hash01(particle.seed, 67) * TAU + time.elapsed_secs() * 0.08,
                );
            }
            ParticleMode::Rain => {
                let rain_length = if dominant_kind == WeatherKind::Storm {
                    1.4
                } else {
                    1.0
                };
                transform.scale = Vec3::new(0.035, rain_length, 0.035);
                let lean = profile.wind.normalize_or_zero();
                transform.rotation =
                    Quat::from_euler(EulerRot::XYZ, 0.22 + lean.y * 0.1, 0.0, -lean.x * 0.18);
            }
            ParticleMode::Sand => {
                transform.scale = Vec3::new(
                    0.22 + hash_range(particle.seed, 47, 0.0, 0.34),
                    0.08 + hash_range(particle.seed, 59, 0.0, 0.08),
                    0.22 + hash_range(particle.seed, 71, 0.0, 0.34),
                );
                let lean = profile.wind.normalize_or_zero();
                transform.rotation = Quat::from_euler(
                    EulerRot::XYZ,
                    0.05 + lean.y * 0.05,
                    swirl_phase + time.elapsed_secs() * 0.22,
                    -lean.x * 0.08,
                );
            }
            ParticleMode::Snow => {
                transform.scale = Vec3::splat(0.13 + hash_range(particle.seed, 83, 0.0, 0.11));
                transform.rotation = Quat::from_euler(
                    EulerRot::XYZ,
                    swirl_phase * 0.35,
                    swirl_phase * 0.48,
                    swirl_phase * 0.27,
                );
            }
            ParticleMode::None => {}
        }
    }
}

fn weather_for_elapsed(elapsed: f32) -> WeatherKind {
    let index = ((elapsed / WEATHER_SEGMENT_SECONDS).floor() as usize) % WEATHER_SEQUENCE.len();
    WEATHER_SEQUENCE[index]
}

fn weather_profile(kind: WeatherKind, config: &AppConfig) -> WeatherProfile {
    match kind {
        WeatherKind::Clear => WeatherProfile {
            sky_day: Vec3::new(0.42, 0.66, 0.92),
            sky_night: Vec3::new(0.015, 0.024, 0.072),
            horizon_glow: Vec3::new(0.98, 0.62, 0.34),
            fog_day: Vec3::new(0.72, 0.83, 0.94),
            fog_night: Vec3::new(0.06, 0.085, 0.13),
            inscatter_day: Vec3::new(0.98, 0.9, 0.74),
            inscatter_night: Vec3::new(0.22, 0.3, 0.42),
            ambient_color: Vec3::new(0.58, 0.64, 0.72),
            visibility: 170.0,
            ambient_brightness: 360.0,
            sun_scale: 1.0,
            moon_scale: 1.0,
            star_scale: 1.0,
            cloud_cover: 0.12,
            precipitation_strength: 0.0,
            particle_speed: 0.0,
            particle_sway: 0.0,
            particle_alpha: 0.0,
            wind: Vec2::new(0.4, 0.2),
            lightning_strength: 0.0,
        },
        WeatherKind::Mist => WeatherProfile {
            sky_day: Vec3::new(0.56, 0.64, 0.72),
            sky_night: Vec3::new(0.06, 0.08, 0.12),
            horizon_glow: Vec3::new(0.86, 0.7, 0.58),
            fog_day: Vec3::new(0.8, 0.84, 0.88),
            fog_night: Vec3::new(0.12, 0.14, 0.18),
            inscatter_day: Vec3::new(0.92, 0.9, 0.86),
            inscatter_night: Vec3::new(0.32, 0.36, 0.42),
            ambient_color: Vec3::new(0.64, 0.66, 0.7),
            visibility: 72.0,
            ambient_brightness: 240.0,
            sun_scale: 0.68,
            moon_scale: 0.45,
            star_scale: 0.25,
            cloud_cover: 0.52,
            precipitation_strength: 0.34,
            particle_speed: 0.18,
            particle_sway: 0.72,
            particle_alpha: 0.14,
            wind: Vec2::new(0.18, 0.12),
            lightning_strength: 0.0,
        },
        WeatherKind::Rain => WeatherProfile {
            sky_day: Vec3::new(0.34, 0.42, 0.54),
            sky_night: Vec3::new(0.035, 0.05, 0.082),
            horizon_glow: Vec3::new(0.68, 0.58, 0.5),
            fog_day: Vec3::new(0.58, 0.64, 0.72),
            fog_night: Vec3::new(0.08, 0.1, 0.13),
            inscatter_day: Vec3::new(0.74, 0.8, 0.88),
            inscatter_night: Vec3::new(0.18, 0.24, 0.32),
            ambient_color: Vec3::new(0.44, 0.5, 0.58),
            visibility: 92.0,
            ambient_brightness: 170.0,
            sun_scale: 0.52,
            moon_scale: 0.38,
            star_scale: 0.05,
            cloud_cover: 0.78,
            precipitation_strength: 0.82,
            particle_speed: 4.4,
            particle_sway: 0.22,
            particle_alpha: 0.28,
            wind: Vec2::new(1.8, -0.7),
            lightning_strength: 0.0,
        },
        WeatherKind::Storm => WeatherProfile {
            sky_day: Vec3::new(0.2, 0.25, 0.32),
            sky_night: Vec3::new(0.02, 0.028, 0.05),
            horizon_glow: Vec3::new(0.56, 0.52, 0.48),
            fog_day: Vec3::new(0.44, 0.5, 0.58),
            fog_night: Vec3::new(0.05, 0.065, 0.09),
            inscatter_day: Vec3::new(0.66, 0.72, 0.82),
            inscatter_night: Vec3::new(0.14, 0.18, 0.24),
            ambient_color: Vec3::new(0.3, 0.36, 0.42),
            visibility: 54.0,
            ambient_brightness: 118.0,
            sun_scale: 0.24,
            moon_scale: 0.18,
            star_scale: 0.0,
            cloud_cover: 0.95,
            precipitation_strength: 1.0,
            particle_speed: 5.6,
            particle_sway: 0.16,
            particle_alpha: 0.34,
            wind: Vec2::new(3.2, -1.15),
            lightning_strength: 1.0,
        },
        WeatherKind::Sandstorm => WeatherProfile {
            sky_day: Vec3::new(0.58, 0.43, 0.24),
            sky_night: Vec3::new(0.11, 0.07, 0.045),
            horizon_glow: Vec3::new(0.98, 0.64, 0.28),
            fog_day: Vec3::new(0.72, 0.52, 0.3),
            fog_night: Vec3::new(0.18, 0.12, 0.075),
            inscatter_day: Vec3::new(0.98, 0.72, 0.38),
            inscatter_night: Vec3::new(0.42, 0.24, 0.12),
            ambient_color: Vec3::new(0.68, 0.48, 0.28),
            visibility: config.desert.sandstorm_visibility.max(24.0),
            ambient_brightness: 230.0,
            sun_scale: 0.34,
            moon_scale: 0.18,
            star_scale: 0.04,
            cloud_cover: 0.9,
            precipitation_strength: config.desert.sandstorm_particle_strength.clamp(0.0, 1.0),
            particle_speed: config.desert.sandstorm_wind_speed.max(0.1) * 0.76,
            particle_sway: 1.8,
            particle_alpha: 0.42,
            wind: Vec2::new(config.desert.sandstorm_wind_speed.max(0.1), -1.2),
            lightning_strength: 0.0,
        },
        WeatherKind::Snow => WeatherProfile {
            sky_day: Vec3::new(0.68, 0.74, 0.82),
            sky_night: Vec3::new(0.045, 0.06, 0.1),
            horizon_glow: Vec3::new(0.9, 0.8, 0.72),
            fog_day: Vec3::new(0.84, 0.88, 0.92),
            fog_night: Vec3::new(0.14, 0.16, 0.2),
            inscatter_day: Vec3::new(0.96, 0.95, 0.92),
            inscatter_night: Vec3::new(0.26, 0.3, 0.38),
            ambient_color: Vec3::new(0.76, 0.78, 0.84),
            visibility: 80.0,
            ambient_brightness: 220.0,
            sun_scale: 0.64,
            moon_scale: 1.2,
            star_scale: 0.35,
            cloud_cover: 0.64,
            precipitation_strength: 0.68,
            particle_speed: 0.75,
            particle_sway: 1.1,
            particle_alpha: 0.82,
            wind: Vec2::new(0.7, 0.35),
            lightning_strength: 0.0,
        },
    }
}

fn blended_profile(a: WeatherProfile, b: WeatherProfile, blend: f32) -> WeatherProfile {
    WeatherProfile {
        sky_day: a.sky_day.lerp(b.sky_day, blend),
        sky_night: a.sky_night.lerp(b.sky_night, blend),
        horizon_glow: a.horizon_glow.lerp(b.horizon_glow, blend),
        fog_day: a.fog_day.lerp(b.fog_day, blend),
        fog_night: a.fog_night.lerp(b.fog_night, blend),
        inscatter_day: a.inscatter_day.lerp(b.inscatter_day, blend),
        inscatter_night: a.inscatter_night.lerp(b.inscatter_night, blend),
        ambient_color: a.ambient_color.lerp(b.ambient_color, blend),
        visibility: a.visibility + (b.visibility - a.visibility) * blend,
        ambient_brightness: a.ambient_brightness
            + (b.ambient_brightness - a.ambient_brightness) * blend,
        sun_scale: a.sun_scale + (b.sun_scale - a.sun_scale) * blend,
        moon_scale: a.moon_scale + (b.moon_scale - a.moon_scale) * blend,
        star_scale: a.star_scale + (b.star_scale - a.star_scale) * blend,
        cloud_cover: a.cloud_cover + (b.cloud_cover - a.cloud_cover) * blend,
        precipitation_strength: a.precipitation_strength
            + (b.precipitation_strength - a.precipitation_strength) * blend,
        particle_speed: a.particle_speed + (b.particle_speed - a.particle_speed) * blend,
        particle_sway: a.particle_sway + (b.particle_sway - a.particle_sway) * blend,
        particle_alpha: a.particle_alpha + (b.particle_alpha - a.particle_alpha) * blend,
        wind: a.wind.lerp(b.wind, blend),
        lightning_strength: a.lightning_strength
            + (b.lightning_strength - a.lightning_strength) * blend,
    }
}

fn particle_mode_for_weather(kind: WeatherKind) -> ParticleMode {
    match kind {
        WeatherKind::Clear => ParticleMode::None,
        WeatherKind::Mist => ParticleMode::Mist,
        WeatherKind::Rain | WeatherKind::Storm => ParticleMode::Rain,
        WeatherKind::Sandstorm => ParticleMode::Sand,
        WeatherKind::Snow => ParticleMode::Snow,
    }
}

fn celestial_frame(normalized_time: f32) -> CelestialFrame {
    let phase = normalized_time.rem_euclid(1.0) * TAU;
    let sun_height = phase.sin();
    let horizontal = -phase.cos();
    let sun_direction = Vec3::new(horizontal, sun_height, 0.24).normalize();
    let moon_direction = -sun_direction;
    let sun_visibility = smoothstep_edges(-0.08, 0.16, sun_height);
    let moon_visibility = smoothstep_edges(-0.12, 0.1, -sun_height) * (1.0 - sun_visibility * 0.78);
    let night_factor = smoothstep_edges(0.18, 0.9, -sun_height);
    let horizon_factor = (1.0 - sun_height.abs()).clamp(0.0, 1.0).powf(1.55);

    CelestialFrame {
        sun_position: sun_direction * SKY_RADIUS,
        moon_position: moon_direction * (SKY_RADIUS - 28.0),
        sun_visibility,
        moon_visibility,
        night_factor,
        horizon_factor,
    }
}

fn storm_flash(elapsed: f32) -> f32 {
    let primary = (elapsed * 0.91).sin().abs().powf(28.0);
    let secondary = (elapsed * 2.17 + PI * 0.35).sin().abs().powf(42.0);
    (primary + secondary * 0.75).clamp(0.0, 1.0)
}

fn star_direction(index: u32) -> Vec3 {
    let azimuth = hash01(index, 5) * TAU;
    let elevation = hash_range(index, 19, 0.12, 1.24);
    let horizontal = elevation.cos();
    Vec3::new(
        horizontal * azimuth.cos(),
        elevation.sin(),
        horizontal * azimuth.sin(),
    )
    .normalize()
}

fn hash01(seed: u32, salt: u32) -> f32 {
    let mut value = (seed as u64)
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add((salt as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9));
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^= value >> 31;
    (value as f64 / u64::MAX as f64) as f32
}

fn hash_range(seed: u32, salt: u32, min: f32, max: f32) -> f32 {
    min + (max - min) * hash01(seed, salt)
}

fn fract(value: f32) -> f32 {
    value - value.floor()
}

fn smoothstep_unit(value: f32) -> f32 {
    let t = value.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn smoothstep_edges(edge0: f32, edge1: f32, value: f32) -> f32 {
    let width = (edge1 - edge0).max(0.0001);
    smoothstep_unit((value - edge0) / width)
}

fn srgb_color(value: Vec3) -> Color {
    Color::srgb(value.x, value.y, value.z)
}

fn cleanup_environment_session(mut commands: Commands) {
    commands.remove_resource::<EnvironmentAssets>();
    commands.insert_resource(EnvironmentSnapshot::default());
    commands.insert_resource(WindField::default());
    commands.insert_resource(EnvironmentTelemetry::default());
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use crate::core::config::{
        AppConfig, AssetConfig, CameraConfig, DesertConfig, EcologyConfig, EnvironmentConfig,
        PlayerConfig, PresentationConfig, QualityConfig, SignConfig, WorldConfig,
    };

    use super::{
        CelestialFrame, WeatherKind, blended_profile, celestial_frame,
        environment_snapshot_from_profile, storm_flash, weather_for_elapsed, weather_profile,
        wind_field_from_profile,
    };

    #[test]
    fn weather_sequence_wraps_cleanly() {
        assert_eq!(weather_for_elapsed(0.0), WeatherKind::Clear);
        assert_eq!(weather_for_elapsed(31.0), WeatherKind::Mist);
        assert_eq!(weather_for_elapsed(121.0), WeatherKind::Sandstorm);
        assert_eq!(weather_for_elapsed(181.0), WeatherKind::Snow);
        assert_eq!(weather_for_elapsed(211.0), WeatherKind::Clear);
    }

    #[test]
    fn sunrise_and_sunset_swap_sides() {
        let sunrise: CelestialFrame = celestial_frame(0.0);
        let sunset: CelestialFrame = celestial_frame(0.5);

        assert!(sunrise.sun_position.x < 0.0);
        assert!(sunset.sun_position.x > 0.0);
        assert!(sunrise.sun_visibility < 0.4);
        assert!(sunset.sun_visibility < 0.4);
    }

    #[test]
    fn blended_profile_matches_input_at_full_blend() {
        let config = test_config();
        let clear = weather_profile(WeatherKind::Clear, &config);
        let snow = weather_profile(WeatherKind::Snow, &config);
        let blended = blended_profile(clear, snow, 1.0);

        assert_eq!(blended.sky_day, snow.sky_day);
        assert_eq!(blended.cloud_cover, snow.cloud_cover);
    }

    #[test]
    fn sandstorm_profile_uses_desert_config() {
        let mut config = test_config();
        config.desert.sandstorm_visibility = 61.0;
        config.desert.sandstorm_particle_strength = 0.55;
        let profile = weather_profile(WeatherKind::Sandstorm, &config);

        assert_eq!(profile.visibility, 61.0);
        assert_eq!(profile.precipitation_strength, 0.55);
    }

    #[test]
    fn storm_flash_stays_in_expected_range() {
        for sample in [0.0, 0.25, 0.5, 1.0, 3.4, 8.7] {
            let flash = storm_flash(sample);
            assert!((0.0..=1.0).contains(&flash));
        }
    }

    #[test]
    fn environment_snapshot_marks_sandstorm_as_dry_and_occluding() {
        let config = test_config();
        let profile = weather_profile(WeatherKind::Sandstorm, &config);
        let snapshot = environment_snapshot_from_profile(WeatherKind::Sandstorm, profile, 0.6);

        assert!(snapshot.sandstorm_weight > 0.9);
        assert!(snapshot.humidity < 0.3);
        assert!(snapshot.fog_density > 0.6);
    }

    #[test]
    fn wind_field_strengthens_under_storm_and_story_response() {
        let config = test_config();
        let profile = weather_profile(WeatherKind::Storm, &config);
        let wind = wind_field_from_profile(12.0, WeatherKind::Storm, profile, 0.4, 0.8);

        assert!(wind.speed > 0.5);
        assert!(wind.gust > 0.4);
        assert!(wind.omen_bias > 0.5);
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
