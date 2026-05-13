use std::{fs, path::Path, time::Instant};

use avian3d::prelude::{Collider, CollisionEventsEnabled, RigidBody, Sensor};
use bevy::{
    asset::RenderAssetUsages,
    color::{ColorToComponents, LinearRgba},
    ecs::system::SystemParam,
    input::mouse::AccumulatedMouseMotion,
    math::primitives::{Cuboid, Plane3d, Sphere, Torus},
    mesh::{Indices, PrimitiveTopology},
    pbr::MeshMaterial3d,
    prelude::*,
    render::{
        render_resource::{Extent3d, TextureDimension, TextureFormat},
        view::screenshot::{Screenshot, save_to_disk},
    },
    window::{CursorGrabMode, CursorOptions, PrimaryWindow},
};
use serde::Serialize;

use crate::{
    core::performance::{FramePerformance, PerformancePhase},
    game::{
        flow::{AppScreen, InGameState, SessionMode, in_session_mode},
        gallery::{GalleryExportMode, GalleryExportQueue, GalleryExportStage, prepare_export_path},
        physics::{
            DaoCollider, DaoColliderRole, DaoColliderSource, DaoPhysicsLayer, DaoPhysicsSensor,
            DaoSensorKind, gallery_layers, stable_gallery_sensor_id,
        },
        world::{SunLight, WorldCamera},
    },
};

pub struct MaterialGalleryPlugin;

impl Plugin for MaterialGalleryPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ProceduralMaterialLibrary>();
        app.insert_resource(MaterialGalleryState::default());
        app.insert_resource(MaterialGalleryCameraState::default());
        app.insert_resource(MaterialGalleryExportState::default());
        app.add_systems(
            OnEnter(AppScreen::InGame),
            spawn_material_gallery.run_if(in_session_mode(SessionMode::MaterialGallery)),
        );
        app.add_systems(
            OnEnter(InGameState::Running),
            lock_material_gallery_cursor.run_if(in_session_mode(SessionMode::MaterialGallery)),
        );
        app.add_systems(
            Update,
            (
                advance_material_gallery_export_frame,
                handle_material_gallery_input,
                apply_material_gallery_lighting,
                focus_material_gallery_camera,
                move_material_gallery_camera,
                process_material_gallery_export_queue,
            )
                .chain()
                .run_if(in_state(InGameState::Running))
                .run_if(in_session_mode(SessionMode::MaterialGallery)),
        );
        app.add_systems(OnExit(AppScreen::InGame), cleanup_material_gallery);
    }
}

#[derive(Debug, Resource, Clone, PartialEq)]
pub struct ProceduralMaterialLibrary {
    pub materials: Vec<ProceduralMaterialDefinition>,
}

impl Default for ProceduralMaterialLibrary {
    fn default() -> Self {
        Self {
            materials: first_material_definitions(),
        }
    }
}

impl ProceduralMaterialLibrary {
    pub fn by_id(&self, id: &str) -> Option<&ProceduralMaterialDefinition> {
        self.materials.iter().find(|material| material.id == id)
    }

    pub fn by_category(&self, category: MaterialCategory) -> Vec<&ProceduralMaterialDefinition> {
        self.materials
            .iter()
            .filter(|material| material.category == category)
            .collect()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ProceduralMaterialDefinition {
    pub id: &'static str,
    pub name: &'static str,
    pub category: MaterialCategory,
    pub base_color: [f32; 3],
    pub accent_color: [f32; 3],
    pub roughness: f32,
    pub metallic: f32,
    pub normal_strength: f32,
    pub wetness: f32,
    pub dust: f32,
    pub wind_erosion: f32,
    pub edge_wear: f32,
    pub scene_usage: &'static str,
    pub pattern: ProceduralMaterialPattern,
}

impl ProceduralMaterialDefinition {
    pub fn stable_id(&self) -> u64 {
        fnv1a_64(self.id.as_bytes())
    }

    fn with_surface_state(&self, wetness: f32, dust: f32) -> Self {
        let mut definition = self.clone();
        definition.wetness = wetness.clamp(0.0, 1.0);
        definition.dust = dust.clamp(0.0, 1.0);
        definition.roughness = (self.roughness + (self.dust - definition.dust) * 0.12
            - definition.wetness * 0.24)
            .clamp(0.12, 1.0);
        definition.edge_wear = (self.edge_wear + definition.wetness * 0.12).clamp(0.0, 1.0);
        definition
    }

    fn base_color_value(&self) -> Color {
        Color::srgb(self.base_color[0], self.base_color[1], self.base_color[2])
    }

    fn accent_color_value(&self) -> Color {
        Color::srgb(
            self.accent_color[0],
            self.accent_color[1],
            self.accent_color[2],
        )
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize)]
pub enum MaterialCategory {
    Ground,
    Building,
    Wood,
    Cloth,
    Metal,
    Water,
    Biological,
    Ruin,
}

impl MaterialCategory {
    pub const ALL: [Self; 8] = [
        Self::Ground,
        Self::Building,
        Self::Wood,
        Self::Cloth,
        Self::Metal,
        Self::Water,
        Self::Biological,
        Self::Ruin,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::Ground => "地表",
            Self::Building => "建筑",
            Self::Wood => "木材",
            Self::Cloth => "布料",
            Self::Metal => "金属",
            Self::Water => "水体",
            Self::Biological => "生物",
            Self::Ruin => "遗迹",
        }
    }

    fn export_label(self) -> &'static str {
        match self {
            Self::Ground => "ground",
            Self::Building => "building",
            Self::Wood => "wood",
            Self::Cloth => "cloth",
            Self::Metal => "metal",
            Self::Water => "water",
            Self::Biological => "biological",
            Self::Ruin => "ruin",
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize)]
pub enum ProceduralMaterialPattern {
    PackedEarth,
    StoneBlock,
    WoodGrain,
    RoofShingle,
    WovenFiber,
    SandRipple,
    GrassBlade,
    MossPatch,
    ReedStalk,
    WoolFiber,
    CeramicCrackle,
    WaterCaustic,
    MetalOxide,
    RuinStrata,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize)]
pub enum MaterialApprovalState {
    Satisfied,
    NeedsRevision,
    Disabled,
}

impl MaterialApprovalState {
    pub fn label(self) -> &'static str {
        match self {
            Self::Satisfied => "满意",
            Self::NeedsRevision => "需要修改",
            Self::Disabled => "禁用",
        }
    }
}

#[derive(Debug, Resource, Clone, PartialEq)]
pub struct MaterialGalleryState {
    pub selected_category: Option<MaterialCategory>,
    pub lighting: GalleryLightingPreset,
}

impl Default for MaterialGalleryState {
    fn default() -> Self {
        Self {
            selected_category: None,
            lighting: GalleryLightingPreset::FixedStudio,
        }
    }
}

#[derive(Debug, Resource, Clone, PartialEq)]
struct MaterialGalleryExportState {
    queue: GalleryExportQueue,
}

impl Default for MaterialGalleryExportState {
    fn default() -> Self {
        Self {
            queue: GalleryExportQueue::new(
                "logs/material-gallery-manifest.json",
                "logs/material-gallery.png",
            ),
        }
    }
}

#[derive(Debug, Resource, Clone, Copy, PartialEq)]
struct MaterialGalleryCameraState {
    yaw: f32,
    pitch: f32,
    focused_category: Option<MaterialCategory>,
    initialized: bool,
}

impl Default for MaterialGalleryCameraState {
    fn default() -> Self {
        Self {
            yaw: -0.78,
            pitch: -0.42,
            focused_category: None,
            initialized: false,
        }
    }
}

impl MaterialGalleryCameraState {
    fn align_to_view(&mut self, eye: Vec3, target: Vec3) {
        let forward = (target - eye).normalize_or_zero();
        if forward.length_squared() <= f32::EPSILON {
            return;
        }
        self.yaw = -forward.x.atan2(-forward.z);
        self.pitch = forward.y.asin().clamp(-1.35, 1.35);
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash, Serialize)]
pub enum GalleryLightingPreset {
    FixedStudio,
    HardRake,
    OvercastDiffuse,
    NightWarm,
}

impl GalleryLightingPreset {
    pub const ALL: [Self; 4] = [
        Self::FixedStudio,
        Self::HardRake,
        Self::OvercastDiffuse,
        Self::NightWarm,
    ];

    pub fn label(self) -> &'static str {
        match self {
            Self::FixedStudio => "固定光照",
            Self::HardRake => "斜向强光",
            Self::OvercastDiffuse => "阴天漫射",
            Self::NightWarm => "夜间暖光",
        }
    }
}

#[derive(Debug, Component)]
struct MaterialGalleryRoot;

#[derive(Debug, Component, Clone, Copy, PartialEq, Eq, Hash)]
struct MaterialExhibit {
    stable_id: u64,
    category: MaterialCategory,
}

#[derive(Debug, Component)]
struct GalleryLight;

#[derive(Debug, Clone)]
struct UploadedProceduralMaterial {
    id: &'static str,
    handle: Handle<StandardMaterial>,
    dry_handle: Handle<StandardMaterial>,
    wet_handle: Handle<StandardMaterial>,
}

#[derive(Debug, Serialize)]
struct MaterialGalleryExport {
    generated_by: &'static str,
    lighting: GalleryLightingPreset,
    export_mode: &'static str,
    screenshot_path: String,
    materials: Vec<MaterialGalleryExportItem>,
}

#[derive(Debug, Serialize)]
struct MaterialGalleryExportItem {
    id: &'static str,
    stable_id: u64,
    name: &'static str,
    category: &'static str,
    roughness: f32,
    metallic: f32,
    normal_strength: f32,
    wetness: f32,
    dust: f32,
    wind_erosion: f32,
    edge_wear: f32,
    scene_usage: &'static str,
    approval: &'static str,
}

const MATERIAL_TEXTURE_SIZE: u32 = 96;
const EXHIBIT_COLUMNS: usize = 4;
const EXHIBIT_SPACING_X: f32 = 9.2;
const EXHIBIT_SPACING_Z: f32 = 7.8;
const MATERIAL_EXPORT_COOLDOWN_SECONDS: f32 = 0.55;

#[derive(SystemParam)]
struct MaterialGallerySpawnParams<'w, 's> {
    commands: Commands<'w, 's>,
    library: Res<'w, ProceduralMaterialLibrary>,
    state: ResMut<'w, MaterialGalleryState>,
    export_state: ResMut<'w, MaterialGalleryExportState>,
    camera_state: ResMut<'w, MaterialGalleryCameraState>,
    performance: Res<'w, FramePerformance>,
    meshes: ResMut<'w, Assets<Mesh>>,
    images: ResMut<'w, Assets<Image>>,
    materials: ResMut<'w, Assets<StandardMaterial>>,
    camera_query: Query<'w, 's, Entity, With<WorldCamera>>,
    sun_query: Query<'w, 's, Entity, With<SunLight>>,
}

fn spawn_material_gallery(mut params: MaterialGallerySpawnParams) {
    let gallery_started = Instant::now();
    params.state.selected_category = None;
    params.state.lighting = GalleryLightingPreset::FixedStudio;
    params.export_state.queue = GalleryExportQueue::new(
        "logs/material-gallery-manifest.json",
        "logs/material-gallery.png",
    );
    *params.camera_state = MaterialGalleryCameraState::default();

    for entity in &params.sun_query {
        params.commands.entity(entity).despawn();
    }

    let definitions = params.library.materials.clone();
    let generation_started = Instant::now();
    let uploaded = definitions
        .iter()
        .map(|definition| {
            upload_procedural_material(definition, &mut params.images, &mut params.materials)
        })
        .collect::<Vec<_>>();
    params.performance.record_phase_duration(
        PerformancePhase::MaterialGeneration,
        generation_started.elapsed(),
    );
    params.performance.record_phase_duration(
        PerformancePhase::MaterialUpload,
        generation_started.elapsed(),
    );

    let mut root = params.commands.spawn((
        Name::new("MaterialGallery"),
        DespawnOnExit(AppScreen::InGame),
        MaterialGalleryRoot,
        Transform::default(),
        Visibility::Visible,
    ));
    let root_id = root.id();
    root.with_children(|parent| {
        spawn_gallery_floor(parent, &mut params.meshes, &mut params.materials);
        for (index, definition) in definitions.iter().enumerate() {
            let uploaded = uploaded
                .iter()
                .find(|material| material.id == definition.id)
                .expect("uploaded material should exist");
            spawn_material_exhibit(parent, &mut params.meshes, definition, uploaded, index);
        }
    });

    params.commands.spawn((
        Name::new("MaterialGalleryKeyLight"),
        DespawnOnExit(AppScreen::InGame),
        DirectionalLight {
            shadows_enabled: true,
            illuminance: 38_000.0,
            color: Color::srgb(1.0, 0.92, 0.82),
            ..Default::default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.72, -0.82, 0.0)),
        GalleryLight,
    ));

    params.commands.spawn((
        Name::new("MaterialGalleryFillLight"),
        DespawnOnExit(AppScreen::InGame),
        PointLight {
            intensity: 18_000.0,
            range: 80.0,
            color: Color::srgb(0.65, 0.78, 1.0),
            shadows_enabled: false,
            ..Default::default()
        },
        Transform::from_xyz(-18.0, 12.0, 12.0),
        GalleryLight,
    ));

    if let Some(entity) = params.camera_query.iter().next() {
        let eye = Vec3::new(-9.0, 12.0, 25.0);
        let target = Vec3::new(13.0, 1.1, 8.0);
        params.camera_state.align_to_view(eye, target);
        params.camera_state.initialized = true;
        params
            .commands
            .entity(entity)
            .insert(Transform::from_translation(eye).looking_at(target, Vec3::Y));
    }

    tracing::info!(
        target: "dao_game::materials::gallery",
        root = ?root_id,
        material_count = definitions.len(),
        texture_resolution = MATERIAL_TEXTURE_SIZE,
        categories = MaterialCategory::ALL.len(),
        "material gallery spawned"
    );
    params
        .performance
        .record_phase_duration(PerformancePhase::MaterialGallery, gallery_started.elapsed());
}

fn spawn_gallery_floor(
    parent: &mut ChildSpawnerCommands<'_>,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
) {
    let floor_material = materials.add(StandardMaterial {
        base_color: Color::srgb(0.12, 0.13, 0.13),
        perceptual_roughness: 0.88,
        metallic: 0.0,
        ..Default::default()
    });
    parent.spawn((
        Name::new("MaterialGalleryFloor"),
        Mesh3d(meshes.add(Mesh::from(Plane3d::new(Vec3::Y, Vec2::new(23.0, 23.0))))),
        MeshMaterial3d(floor_material),
        Transform::from_xyz(12.0, -0.02, 11.0),
    ));
}

fn spawn_material_exhibit(
    parent: &mut ChildSpawnerCommands<'_>,
    meshes: &mut Assets<Mesh>,
    definition: &ProceduralMaterialDefinition,
    uploaded: &UploadedProceduralMaterial,
    index: usize,
) {
    let column = index % EXHIBIT_COLUMNS;
    let row = index / EXHIBIT_COLUMNS;
    let origin = Vec3::new(
        column as f32 * EXHIBIT_SPACING_X,
        0.0,
        row as f32 * EXHIBIT_SPACING_Z,
    );
    let stable_id = definition.stable_id();
    let exhibit = MaterialExhibit {
        stable_id,
        category: definition.category,
    };

    parent
        .spawn((
            Name::new(format!("MaterialExhibit::{}", definition.id)),
            Transform::from_translation(origin),
            Visibility::Visible,
            exhibit,
        ))
        .with_children(|parent| {
            let plinth_material = uploaded.handle.clone();
            parent.spawn((
                Name::new("MaterialSphereClose"),
                Mesh3d(meshes.add(Mesh::from(Sphere::new(0.95)))),
                MeshMaterial3d(plinth_material.clone()),
                Transform::from_xyz(-2.3, 1.25, 0.0),
            ));
            parent.spawn((
                Name::new("MaterialCubeMid"),
                Mesh3d(meshes.add(Mesh::from(Cuboid::new(1.65, 1.65, 1.65)))),
                MeshMaterial3d(plinth_material.clone()),
                Transform::from_xyz(0.1, 1.05, 0.0).with_rotation(Quat::from_euler(
                    EulerRot::XYZ,
                    0.0,
                    0.55,
                    0.0,
                )),
            ));
            parent.spawn((
                Name::new("MaterialRakedPlane"),
                Mesh3d(meshes.add(slant_test_mesh())),
                MeshMaterial3d(plinth_material.clone()),
                Transform::from_xyz(2.35, 0.72, 0.05),
            ));
            parent.spawn((
                Name::new("MaterialRoughPatch"),
                Mesh3d(meshes.add(rough_patch_mesh(definition))),
                MeshMaterial3d(plinth_material),
                Transform::from_xyz(0.0, 0.08, 2.15),
            ));
            parent.spawn((
                Name::new("MaterialDryComparison"),
                Mesh3d(meshes.add(Mesh::from(Torus::new(0.3, 0.62)))),
                MeshMaterial3d(uploaded.dry_handle.clone()),
                Transform::from_xyz(-2.85, 0.36, 2.05)
                    .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
            ));
            parent.spawn((
                Name::new("MaterialWetComparison"),
                Mesh3d(meshes.add(Mesh::from(Torus::new(0.38, 0.76)))),
                MeshMaterial3d(uploaded.wet_handle.clone()),
                Transform::from_xyz(-1.8, 0.36, 2.05)
                    .with_rotation(Quat::from_rotation_x(std::f32::consts::FRAC_PI_2)),
            ));
            parent.spawn((
                Name::new("MaterialGallerySensor"),
                Transform::from_xyz(0.0, 0.9, 0.9),
                RigidBody::Static,
                Collider::cuboid(6.4, 2.0, 4.9),
                Sensor,
                CollisionEventsEnabled,
                gallery_layers(),
                DaoPhysicsSensor {
                    kind: DaoSensorKind::GalleryExhibit,
                },
                DaoCollider {
                    layer: DaoPhysicsLayer::Gallery,
                    role: DaoColliderRole::InteractionSensor,
                    source: DaoColliderSource::MaterialGallery,
                },
            ));
        });

    let approval = approval_for_material(definition);
    tracing::info!(
        target: "dao_game::materials::exhibit",
        material_id = definition.id,
        stable_id,
        sensor_id = stable_gallery_sensor_id(definition.id),
        name = definition.name,
        category = definition.category.label(),
        roughness = definition.roughness,
        metallic = definition.metallic,
        normal_strength = definition.normal_strength,
        wetness = definition.wetness,
        dust = definition.dust,
        wind_erosion = definition.wind_erosion,
        edge_wear = definition.edge_wear,
        scene_usage = definition.scene_usage,
        approval = approval.label(),
        "material exhibit registered"
    );
}

fn advance_material_gallery_export_frame(mut export_state: ResMut<MaterialGalleryExportState>) {
    export_state.queue.advance_frame();
}

fn handle_material_gallery_input(
    keys: Res<ButtonInput<KeyCode>>,
    mut state: ResMut<MaterialGalleryState>,
    mut export_state: ResMut<MaterialGalleryExportState>,
) {
    if keys.just_pressed(KeyCode::Digit1) {
        state.lighting = GalleryLightingPreset::FixedStudio;
    } else if keys.just_pressed(KeyCode::Digit2) {
        state.lighting = GalleryLightingPreset::HardRake;
    } else if keys.just_pressed(KeyCode::Digit3) {
        state.lighting = GalleryLightingPreset::OvercastDiffuse;
    } else if keys.just_pressed(KeyCode::Digit4) {
        state.lighting = GalleryLightingPreset::NightWarm;
    }

    if keys.just_pressed(KeyCode::BracketRight) {
        state.selected_category = Some(next_category(state.selected_category, 1));
    } else if keys.just_pressed(KeyCode::BracketLeft) {
        state.selected_category = Some(next_category(state.selected_category, -1));
    } else if keys.just_pressed(KeyCode::Backslash) {
        state.selected_category = None;
    }

    if keys.just_pressed(KeyCode::KeyE) {
        let with_screenshot = keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight);
        let mode = if with_screenshot {
            GalleryExportMode::ManifestAndScreenshot
        } else {
            GalleryExportMode::ManifestOnly
        };
        match export_state
            .queue
            .queue_export(mode, MATERIAL_EXPORT_COOLDOWN_SECONDS)
        {
            Ok(()) => {
                tracing::info!(
                    target: "dao_game::materials::export",
                    mode = mode.export_label(),
                    "material gallery export queued"
                );
            }
            Err(cooldown_remaining_ms) => {
                tracing::warn!(
                    target: "dao_game::materials::export",
                    cooldown_remaining_ms,
                    "material gallery export ignored during cooldown"
                );
            }
        }
    }
}

fn process_material_gallery_export_queue(
    mut commands: Commands,
    library: Res<ProceduralMaterialLibrary>,
    state: Res<MaterialGalleryState>,
    mut export_state: ResMut<MaterialGalleryExportState>,
) {
    match export_state.queue.pending_stage.clone() {
        GalleryExportStage::Idle => {}
        GalleryExportStage::ManifestQueued { mode, queued_frame } => {
            if queued_frame == export_state.queue.frame_index {
                return;
            }
            if let Err(error) = export_material_gallery_manifest(
                &library,
                state.lighting,
                mode,
                &export_state.queue.export_path,
                &export_state.queue.screenshot_path,
            ) {
                tracing::error!(
                    target: "dao_game::materials::export",
                    path = %export_state.queue.export_path.display(),
                    error = %error,
                    "material gallery export failed"
                );
                export_state.queue.reset();
                return;
            }
            tracing::info!(
                target: "dao_game::materials::export",
                path = %export_state.queue.export_path.display(),
                mode = mode.export_label(),
                "material gallery manifest exported"
            );
            export_state.queue.mark_manifest_exported(mode);
        }
        GalleryExportStage::ScreenshotQueued { queued_frame } => {
            if queued_frame == export_state.queue.frame_index {
                return;
            }
            if let Err(error) = prepare_export_path(&export_state.queue.screenshot_path) {
                tracing::error!(
                    target: "dao_game::materials::export",
                    path = %export_state.queue.screenshot_path.display(),
                    error = %error,
                    "material gallery screenshot path preparation failed"
                );
            } else {
                commands
                    .spawn(Screenshot::primary_window())
                    .observe(save_to_disk(export_state.queue.screenshot_path.clone()));
                tracing::info!(
                    target: "dao_game::materials::export",
                    path = %export_state.queue.screenshot_path.display(),
                    "material gallery screenshot requested"
                );
            }
            export_state.queue.reset();
        }
    }
}

fn lock_material_gallery_cursor(mut cursor_query: Query<&mut CursorOptions, With<PrimaryWindow>>) {
    let Some(mut cursor_options) = cursor_query.iter_mut().next() else {
        return;
    };
    cursor_options.visible = false;
    cursor_options.grab_mode = CursorGrabMode::Locked;
}

fn apply_material_gallery_lighting(
    state: Res<MaterialGalleryState>,
    mut clear_color: ResMut<ClearColor>,
    mut lights: Query<(&mut DirectionalLight, &mut Transform), With<GalleryLight>>,
    mut exhibits: Query<(&MaterialExhibit, &mut Visibility)>,
) {
    if state.is_changed() {
        let preset = state.lighting;
        for (mut light, mut transform) in &mut lights {
            match preset {
                GalleryLightingPreset::FixedStudio => {
                    light.illuminance = 38_000.0;
                    light.color = Color::srgb(1.0, 0.92, 0.82);
                    *transform = Transform::from_rotation(Quat::from_euler(
                        EulerRot::XYZ,
                        -0.72,
                        -0.82,
                        0.0,
                    ));
                    clear_color.0 = Color::srgb(0.09, 0.1, 0.1);
                }
                GalleryLightingPreset::HardRake => {
                    light.illuminance = 72_000.0;
                    light.color = Color::srgb(1.0, 0.84, 0.58);
                    *transform = Transform::from_rotation(Quat::from_euler(
                        EulerRot::XYZ,
                        -0.28,
                        -1.22,
                        0.0,
                    ));
                    clear_color.0 = Color::srgb(0.075, 0.08, 0.085);
                }
                GalleryLightingPreset::OvercastDiffuse => {
                    light.illuminance = 19_000.0;
                    light.color = Color::srgb(0.82, 0.9, 1.0);
                    *transform =
                        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -1.12, -0.2, 0.0));
                    clear_color.0 = Color::srgb(0.18, 0.2, 0.21);
                }
                GalleryLightingPreset::NightWarm => {
                    light.illuminance = 10_500.0;
                    light.color = Color::srgb(1.0, 0.62, 0.34);
                    *transform =
                        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.54, 0.72, 0.0));
                    clear_color.0 = Color::srgb(0.035, 0.04, 0.055);
                }
            }
        }
        tracing::info!(
            target: "dao_game::materials::lighting",
            preset = state.lighting.label(),
            category = state.selected_category.map(MaterialCategory::label).unwrap_or("全部"),
            "material gallery view changed"
        );
    }

    for (exhibit, mut visibility) in &mut exhibits {
        *visibility = if state
            .selected_category
            .is_none_or(|category| category == exhibit.category)
        {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
    }
}

fn focus_material_gallery_camera(
    state: Res<MaterialGalleryState>,
    mut camera_state: ResMut<MaterialGalleryCameraState>,
    mut camera_query: Query<&mut Transform, With<WorldCamera>>,
) {
    if camera_state.initialized && camera_state.focused_category == state.selected_category {
        return;
    }
    let Some(mut transform) = camera_query.iter_mut().next() else {
        return;
    };
    let row_bias = state
        .selected_category
        .and_then(|category| {
            MaterialCategory::ALL
                .iter()
                .position(|candidate| *candidate == category)
        })
        .map(|index| index as f32 * 1.6)
        .unwrap_or(6.0);
    let eye = Vec3::new(-8.0, 11.5, 20.0 + row_bias);
    let target = Vec3::new(12.0, 1.1, 8.0 + row_bias * 0.35);
    camera_state.align_to_view(eye, target);
    camera_state.focused_category = state.selected_category;
    camera_state.initialized = true;
    *transform = Transform::from_translation(eye).looking_at(target, Vec3::Y);
}

fn move_material_gallery_camera(
    time: Res<Time>,
    keys: Res<ButtonInput<KeyCode>>,
    mouse_motion: Res<AccumulatedMouseMotion>,
    mut camera_state: ResMut<MaterialGalleryCameraState>,
    mut camera_query: Query<&mut Transform, With<WorldCamera>>,
) {
    let Some(mut transform) = camera_query.iter_mut().next() else {
        return;
    };

    let mouse_delta = mouse_motion.delta;
    if mouse_delta != Vec2::ZERO {
        const LOOK_SENSITIVITY: f32 = 0.0023;
        camera_state.yaw -= mouse_delta.x * LOOK_SENSITIVITY;
        camera_state.pitch =
            (camera_state.pitch - mouse_delta.y * LOOK_SENSITIVITY).clamp(-1.35, 1.2);
    }
    transform.rotation =
        Quat::from_rotation_y(camera_state.yaw) * Quat::from_rotation_x(camera_state.pitch);

    let forward = *transform.forward();
    let right = *transform.right();
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
    if keys.pressed(KeyCode::Space) {
        movement += Vec3::Y;
    }
    if keys.pressed(KeyCode::ControlLeft) || keys.pressed(KeyCode::ControlRight) {
        movement -= Vec3::Y;
    }

    if movement.length_squared() <= f32::EPSILON {
        return;
    }
    let speed = if keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight) {
        18.0
    } else {
        7.0
    };
    transform.translation += movement.normalize() * speed * time.delta_secs();
}

fn cleanup_material_gallery(
    mut commands: Commands,
    roots: Query<Entity, With<MaterialGalleryRoot>>,
    gallery_lights: Query<Entity, With<GalleryLight>>,
) {
    for entity in &roots {
        commands.entity(entity).despawn();
    }
    for entity in &gallery_lights {
        commands.entity(entity).despawn();
    }
}

fn upload_procedural_material(
    definition: &ProceduralMaterialDefinition,
    images: &mut Assets<Image>,
    materials: &mut Assets<StandardMaterial>,
) -> UploadedProceduralMaterial {
    let material = upload_material_variant(definition, images, materials);
    let dry = definition.with_surface_state(0.02, (definition.dust + 0.16).min(1.0));
    let wet =
        definition.with_surface_state((definition.wetness + 0.42).min(1.0), definition.dust * 0.32);
    let dry_material = upload_material_variant(&dry, images, materials);
    let wet_material = upload_material_variant(&wet, images, materials);

    UploadedProceduralMaterial {
        id: definition.id,
        handle: material,
        dry_handle: dry_material,
        wet_handle: wet_material,
    }
}

fn upload_material_variant(
    definition: &ProceduralMaterialDefinition,
    images: &mut Assets<Image>,
    materials: &mut Assets<StandardMaterial>,
) -> Handle<StandardMaterial> {
    let albedo = images.add(build_albedo_texture(definition, MATERIAL_TEXTURE_SIZE));
    let normal = images.add(build_normal_texture(definition, MATERIAL_TEXTURE_SIZE));
    let metallic_roughness = images.add(build_metallic_roughness_texture(
        definition,
        MATERIAL_TEXTURE_SIZE,
    ));
    materials.add(StandardMaterial {
        base_color: Color::WHITE,
        base_color_texture: Some(albedo),
        normal_map_texture: Some(normal),
        metallic_roughness_texture: Some(metallic_roughness),
        metallic: definition.metallic.clamp(0.0, 1.0).max(0.001),
        perceptual_roughness: definition.roughness.clamp(0.089, 1.0),
        reflectance: (0.18 + definition.edge_wear * 0.18 + definition.wetness * 0.12)
            .clamp(0.02, 0.75),
        clearcoat: (definition.wetness * 0.22).clamp(0.0, 0.5),
        clearcoat_perceptual_roughness: (definition.roughness * 0.7).clamp(0.1, 1.0),
        emissive: if definition.category == MaterialCategory::Water {
            LinearRgba::rgb(0.015, 0.045, 0.055)
        } else {
            LinearRgba::BLACK
        },
        ..Default::default()
    })
}

fn build_albedo_texture(definition: &ProceduralMaterialDefinition, resolution: u32) -> Image {
    let mut data = Vec::with_capacity((resolution * resolution * 4) as usize);
    for y in 0..resolution {
        for x in 0..resolution {
            let uv = Vec2::new(x as f32 / resolution as f32, y as f32 / resolution as f32);
            let color = procedural_color(definition, uv, false);
            let [r, g, b, a] = color.to_srgba().to_u8_array();
            data.extend_from_slice(&[r, g, b, a]);
        }
    }
    image_from_rgba(data, resolution, TextureFormat::Rgba8UnormSrgb)
}

fn build_normal_texture(definition: &ProceduralMaterialDefinition, resolution: u32) -> Image {
    let mut data = Vec::with_capacity((resolution * resolution * 4) as usize);
    let texel = 1.0 / resolution as f32;
    for y in 0..resolution {
        for x in 0..resolution {
            let uv = Vec2::new(x as f32 / resolution as f32, y as f32 / resolution as f32);
            let left = material_height(definition, uv - Vec2::X * texel);
            let right = material_height(definition, uv + Vec2::X * texel);
            let down = material_height(definition, uv - Vec2::Y * texel);
            let up = material_height(definition, uv + Vec2::Y * texel);
            let strength = definition.normal_strength.clamp(0.0, 2.2);
            let normal = Vec3::new((left - right) * strength, (down - up) * strength, 1.0)
                .normalize_or_zero();
            let encoded = ((normal * 0.5) + Vec3::splat(0.5)).clamp(Vec3::ZERO, Vec3::ONE);
            data.extend_from_slice(&[
                channel(encoded.x),
                channel(encoded.y),
                channel(encoded.z),
                255,
            ]);
        }
    }
    image_from_rgba(data, resolution, TextureFormat::Rgba8Unorm)
}

fn build_metallic_roughness_texture(
    definition: &ProceduralMaterialDefinition,
    resolution: u32,
) -> Image {
    let mut data = Vec::with_capacity((resolution * resolution * 4) as usize);
    for y in 0..resolution {
        for x in 0..resolution {
            let uv = Vec2::new(x as f32 / resolution as f32, y as f32 / resolution as f32);
            let variation = material_height(definition, uv);
            let roughness = (definition.roughness + variation * 0.14 - definition.wetness * 0.08)
                .clamp(0.089, 1.0);
            let metallic =
                (definition.metallic + definition.edge_wear * variation * 0.08).clamp(0.0, 1.0);
            data.extend_from_slice(&[0, channel(roughness), channel(metallic), 255]);
        }
    }
    image_from_rgba(data, resolution, TextureFormat::Rgba8Unorm)
}

fn image_from_rgba(data: Vec<u8>, resolution: u32, format: TextureFormat) -> Image {
    Image::new(
        Extent3d {
            width: resolution,
            height: resolution,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        format,
        RenderAssetUsages::default(),
    )
}

fn procedural_color(
    definition: &ProceduralMaterialDefinition,
    uv: Vec2,
    wet_variant: bool,
) -> Color {
    let base = definition.base_color_value().to_linear().to_vec3();
    let accent = definition.accent_color_value().to_linear().to_vec3();
    let h = material_height(definition, uv);
    let edge = edge_mask(uv) * definition.edge_wear;
    let dust = dust_pattern(uv, definition.dust, definition.wind_erosion);
    let wetness = if wet_variant {
        (definition.wetness + 0.35).clamp(0.0, 1.0)
    } else {
        definition.wetness
    };
    let mut color = base.lerp(accent, h * 0.52 + edge * 0.28);
    color *= 1.0 - dust * 0.22;
    color = color.lerp(
        color * Vec3::splat(0.52) + Vec3::new(0.015, 0.025, 0.035),
        wetness * 0.34,
    );
    Color::srgb(
        color.x.clamp(0.0, 1.0),
        color.y.clamp(0.0, 1.0),
        color.z.clamp(0.0, 1.0),
    )
}

fn material_height(definition: &ProceduralMaterialDefinition, uv: Vec2) -> f32 {
    let uv = uv.fract();
    let grain = hash_noise(uv * 41.0 + Vec2::splat(definition.stable_id() as f32 * 0.000_001));
    let fine = hash_noise(uv * 123.0 + Vec2::new(7.1, 19.3));
    let directional =
        ((uv.x * 12.0 + uv.y * definition.wind_erosion * 8.0).sin() * 0.5 + 0.5).clamp(0.0, 1.0);
    let pattern = match definition.pattern {
        ProceduralMaterialPattern::PackedEarth => grain * 0.62 + fine * 0.22,
        ProceduralMaterialPattern::StoneBlock => {
            mortar_grid(uv, 4.0, 3.0) * 0.6 + fractured_noise(uv) * 0.42
        }
        ProceduralMaterialPattern::WoodGrain => {
            let rings = ((uv.x * 18.0 + (uv.y * 8.0).sin() * 0.7).sin() * 0.5 + 0.5).powf(1.6);
            rings * 0.75 + grain * 0.25
        }
        ProceduralMaterialPattern::RoofShingle => {
            mortar_grid(uv, 7.0, 5.0) * 0.42 + directional * 0.32
        }
        ProceduralMaterialPattern::WovenFiber => {
            let warp = (uv.x * 42.0).sin().abs();
            let weft = (uv.y * 37.0).sin().abs();
            (warp * weft).sqrt() * 0.62 + fine * 0.2
        }
        ProceduralMaterialPattern::SandRipple => {
            ((uv.x * 24.0 + uv.y * 7.0).sin() * 0.5 + 0.5) * 0.55 + grain * 0.2
        }
        ProceduralMaterialPattern::GrassBlade => {
            ((uv.x * 64.0).sin().abs() * 0.55 + grain * 0.26 + uv.y * 0.18).clamp(0.0, 1.0)
        }
        ProceduralMaterialPattern::MossPatch => (grain * 0.72 + fine * 0.42).clamp(0.0, 1.0),
        ProceduralMaterialPattern::ReedStalk => {
            ((uv.x * 28.0).sin().abs() * 0.6 + uv.y * 0.32).clamp(0.0, 1.0)
        }
        ProceduralMaterialPattern::WoolFiber => {
            let curl = ((uv.x * 18.0).sin() + (uv.y * 21.0).cos()) * 0.25 + 0.5;
            curl * 0.7 + fine * 0.25
        }
        ProceduralMaterialPattern::CeramicCrackle => cracked_pattern(uv) * 0.65 + grain * 0.18,
        ProceduralMaterialPattern::WaterCaustic => {
            let wave_a = (uv.x * 19.0 + uv.y * 13.0).sin();
            let wave_b = (uv.x * -11.0 + uv.y * 23.0).cos();
            ((wave_a + wave_b) * 0.25 + 0.5).clamp(0.0, 1.0)
        }
        ProceduralMaterialPattern::MetalOxide => {
            (grain * 0.48 + cracked_pattern(uv) * 0.3 + edge_mask(uv) * 0.35).clamp(0.0, 1.0)
        }
        ProceduralMaterialPattern::RuinStrata => {
            let strata = ((uv.y * 18.0 + grain * 2.0).sin() * 0.5 + 0.5) * 0.52;
            strata + fractured_noise(uv) * 0.35
        }
    };
    pattern.clamp(0.0, 1.0)
}

fn slant_test_mesh() -> Mesh {
    let positions = vec![
        [-1.25, -0.55, -1.0],
        [1.25, -0.55, -1.0],
        [1.25, 0.55, 1.0],
        [-1.25, 0.55, 1.0],
    ];
    let normals = vec![[0.0, 0.86, -0.5]; 4];
    let uvs = vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]];
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(vec![0, 1, 2, 0, 2, 3]));
    let _ = mesh.generate_tangents();
    mesh
}

fn rough_patch_mesh(definition: &ProceduralMaterialDefinition) -> Mesh {
    let size = 2.8;
    let subdivisions = 10_u32;
    let mut positions = Vec::new();
    let mut normals = Vec::new();
    let mut uvs = Vec::new();
    for z in 0..=subdivisions {
        for x in 0..=subdivisions {
            let uv = Vec2::new(
                x as f32 / subdivisions as f32,
                z as f32 / subdivisions as f32,
            );
            let height =
                (material_height(definition, uv) - 0.5) * 0.16 * definition.normal_strength;
            positions.push([(uv.x - 0.5) * size, height, (uv.y - 0.5) * size]);
            normals.push([0.0, 1.0, 0.0]);
            uvs.push([uv.x, uv.y]);
        }
    }
    let stride = subdivisions + 1;
    let mut indices = Vec::new();
    for z in 0..subdivisions {
        for x in 0..subdivisions {
            let i = z * stride + x;
            indices.extend_from_slice(&[i, i + 1, i + stride + 1, i, i + stride + 1, i + stride]);
        }
    }
    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    let _ = mesh.generate_tangents();
    mesh
}

fn next_category(current: Option<MaterialCategory>, delta: i32) -> MaterialCategory {
    let index = current
        .and_then(|category| {
            MaterialCategory::ALL
                .iter()
                .position(|candidate| *candidate == category)
        })
        .unwrap_or(0) as i32;
    let next = (index + delta).rem_euclid(MaterialCategory::ALL.len() as i32) as usize;
    MaterialCategory::ALL[next]
}

fn export_material_gallery_manifest(
    library: &ProceduralMaterialLibrary,
    lighting: GalleryLightingPreset,
    mode: GalleryExportMode,
    export_path: &Path,
    screenshot_path: &Path,
) -> Result<(), String> {
    prepare_export_path(export_path)?;
    let export = MaterialGalleryExport {
        generated_by: "dao_game::materials::MaterialGallery",
        lighting,
        export_mode: mode.export_label(),
        screenshot_path: screenshot_path.display().to_string(),
        materials: library
            .materials
            .iter()
            .map(|material| MaterialGalleryExportItem {
                id: material.id,
                stable_id: material.stable_id(),
                name: material.name,
                category: material.category.export_label(),
                roughness: material.roughness,
                metallic: material.metallic,
                normal_strength: material.normal_strength,
                wetness: material.wetness,
                dust: material.dust,
                wind_erosion: material.wind_erosion,
                edge_wear: material.edge_wear,
                scene_usage: material.scene_usage,
                approval: approval_for_material(material).label(),
            })
            .collect(),
    };
    let raw = serde_json::to_string_pretty(&export)
        .map_err(|error| format!("failed to serialize gallery manifest: {error}"))?;
    fs::write(export_path, raw)
        .map_err(|error| format!("failed to write {}: {error}", export_path.display()))
}

fn approval_for_material(definition: &ProceduralMaterialDefinition) -> MaterialApprovalState {
    if definition.normal_strength < 0.22 || definition.roughness <= 0.1 {
        MaterialApprovalState::NeedsRevision
    } else if definition.dust > 0.92 && definition.wetness > 0.72 {
        MaterialApprovalState::Disabled
    } else {
        MaterialApprovalState::Satisfied
    }
}

macro_rules! material {
    (
        $id:expr,
        $name:expr,
        $category:expr,
        $base_color:expr,
        $accent_color:expr,
        $roughness:expr,
        $metallic:expr,
        $normal_strength:expr,
        $wetness:expr,
        $dust:expr,
        $wind_erosion:expr,
        $edge_wear:expr,
        $scene_usage:expr,
        $pattern:expr $(,)?
    ) => {
        ProceduralMaterialDefinition {
            id: $id,
            name: $name,
            category: $category,
            base_color: $base_color,
            accent_color: $accent_color,
            roughness: $roughness,
            metallic: $metallic,
            normal_strength: $normal_strength,
            wetness: $wetness,
            dust: $dust,
            wind_erosion: $wind_erosion,
            edge_wear: $edge_wear,
            scene_usage: $scene_usage,
            pattern: $pattern,
        }
    };
}

fn first_material_definitions() -> Vec<ProceduralMaterialDefinition> {
    use MaterialCategory as C;
    use ProceduralMaterialPattern as P;
    vec![
        material!(
            "dao/mat/mud-wall/v1",
            "夯土泥墙",
            C::Building,
            [0.55, 0.46, 0.34],
            [0.68, 0.58, 0.42],
            0.92,
            0.0,
            0.82,
            0.18,
            0.62,
            0.42,
            0.28,
            "村屋外墙、低矮围墙",
            P::PackedEarth,
        ),
        material!(
            "dao/mat/stone/v1",
            "海风灰石",
            C::Building,
            [0.43, 0.43, 0.4],
            [0.61, 0.6, 0.55],
            0.88,
            0.0,
            0.96,
            0.12,
            0.32,
            0.24,
            0.46,
            "井台、路径石、基座",
            P::StoneBlock,
        ),
        material!(
            "dao/mat/old-wood/v1",
            "旧木梁",
            C::Wood,
            [0.31, 0.22, 0.13],
            [0.57, 0.42, 0.26],
            0.86,
            0.0,
            1.1,
            0.1,
            0.48,
            0.36,
            0.58,
            "门框、栅栏、摊位骨架",
            P::WoodGrain,
        ),
        material!(
            "dao/mat/dark-roof/v1",
            "深色旧屋顶",
            C::Building,
            [0.19, 0.16, 0.13],
            [0.36, 0.29, 0.22],
            0.94,
            0.0,
            0.78,
            0.2,
            0.5,
            0.52,
            0.35,
            "村屋屋面、檐下阴影",
            P::RoofShingle,
        ),
        material!(
            "dao/mat/cloth/v1",
            "粗织布棚",
            C::Cloth,
            [0.58, 0.28, 0.22],
            [0.78, 0.52, 0.38],
            0.82,
            0.0,
            0.92,
            0.08,
            0.44,
            0.28,
            0.22,
            "集市布棚、NPC 衣料",
            P::WovenFiber,
        ),
        material!(
            "dao/mat/wet-sand/v1",
            "湿沙",
            C::Ground,
            [0.43, 0.38, 0.29],
            [0.65, 0.57, 0.42],
            0.48,
            0.0,
            0.72,
            0.82,
            0.16,
            0.3,
            0.12,
            "潮线、河滩、渡口浅处",
            P::SandRipple,
        ),
        material!(
            "dao/mat/dry-sand/v1",
            "干沙",
            C::Ground,
            [0.73, 0.62, 0.39],
            [0.86, 0.73, 0.47],
            0.96,
            0.0,
            0.84,
            0.05,
            0.78,
            0.86,
            0.2,
            "沙丘、远沙、风积地",
            P::SandRipple,
        ),
        material!(
            "dao/mat/meadow/v1",
            "草甸地表",
            C::Ground,
            [0.28, 0.43, 0.21],
            [0.62, 0.55, 0.3],
            0.94,
            0.0,
            1.05,
            0.28,
            0.34,
            0.24,
            0.16,
            "村外草地、放牧路线",
            P::GrassBlade,
        ),
        material!(
            "dao/mat/moss/v1",
            "湿苔藓",
            C::Ground,
            [0.12, 0.31, 0.18],
            [0.39, 0.55, 0.28],
            0.78,
            0.0,
            1.24,
            0.68,
            0.12,
            0.08,
            0.1,
            "石缝、水井边、阴湿林地",
            P::MossPatch,
        ),
        material!(
            "dao/mat/reed/v1",
            "芦苇叶茎",
            C::Ground,
            [0.34, 0.47, 0.28],
            [0.72, 0.65, 0.38],
            0.86,
            0.0,
            0.98,
            0.3,
            0.28,
            0.42,
            0.18,
            "河岸、湿地、海风边缘",
            P::ReedStalk,
        ),
        material!(
            "dao/mat/wool/v1",
            "羊毛",
            C::Biological,
            [0.78, 0.74, 0.64],
            [0.96, 0.9, 0.78],
            0.98,
            0.0,
            1.38,
            0.18,
            0.22,
            0.06,
            0.08,
            "羊群、毛毡、生活细节",
            P::WoolFiber,
        ),
        material!(
            "dao/mat/pottery/v1",
            "旧陶器",
            C::Building,
            [0.55, 0.29, 0.18],
            [0.84, 0.56, 0.34],
            0.72,
            0.0,
            0.74,
            0.12,
            0.38,
            0.18,
            0.4,
            "罐、碗、室内道具",
            P::CeramicCrackle,
        ),
        material!(
            "dao/mat/water/v1",
            "水面",
            C::Water,
            [0.11, 0.38, 0.48],
            [0.42, 0.72, 0.82],
            0.18,
            0.02,
            0.66,
            1.0,
            0.0,
            0.18,
            0.02,
            "海面、河面、井水",
            P::WaterCaustic,
        ),
        material!(
            "dao/mat/old-metal/v1",
            "旧金属",
            C::Metal,
            [0.24, 0.23, 0.21],
            [0.68, 0.53, 0.34],
            0.58,
            0.86,
            0.88,
            0.18,
            0.48,
            0.22,
            0.72,
            "扣件、遗物边框、工具",
            P::MetalOxide,
        ),
        material!(
            "dao/mat/ruin-stone/v1",
            "遗迹层石",
            C::Ruin,
            [0.44, 0.39, 0.31],
            [0.68, 0.6, 0.45],
            0.91,
            0.0,
            1.15,
            0.16,
            0.58,
            0.68,
            0.64,
            "残墙、石环、古老基座",
            P::RuinStrata,
        ),
        material!(
            "dao/mat/desert-stone/v1",
            "沙漠风蚀石",
            C::Ruin,
            [0.62, 0.48, 0.28],
            [0.86, 0.7, 0.42],
            0.94,
            0.0,
            1.08,
            0.06,
            0.84,
            0.92,
            0.52,
            "金字塔外层、戈壁岩块",
            P::RuinStrata,
        ),
    ]
}

fn dust_pattern(uv: Vec2, dust: f32, wind_erosion: f32) -> f32 {
    let streak = ((uv.x * 19.0 + uv.y * 4.0 * wind_erosion).sin() * 0.5 + 0.5).powf(2.0);
    (streak * dust + hash_noise(uv * 67.0) * dust * 0.35).clamp(0.0, 1.0)
}

fn mortar_grid(uv: Vec2, columns: f32, rows: f32) -> f32 {
    let cell = Vec2::new((uv.x * columns).fract(), (uv.y * rows).fract());
    let line = (cell.x.min(1.0 - cell.x).min(cell.y.min(1.0 - cell.y)) * 24.0).clamp(0.0, 1.0);
    1.0 - line
}

fn fractured_noise(uv: Vec2) -> f32 {
    let a = hash_noise(uv * 17.0);
    let b = hash_noise(uv * 43.0 + Vec2::new(4.0, 9.0));
    ((a - b).abs() * 1.8).clamp(0.0, 1.0)
}

fn cracked_pattern(uv: Vec2) -> f32 {
    let center = Vec2::new(
        hash_noise(uv.floor() + Vec2::X),
        hash_noise(uv.floor() + Vec2::Y),
    );
    let local = uv.fract();
    (1.0 - local.distance(center).min(1.0)).powf(8.0)
}

fn edge_mask(uv: Vec2) -> f32 {
    let edge = uv.x.min(1.0 - uv.x).min(uv.y.min(1.0 - uv.y));
    (1.0 - edge * 8.0).clamp(0.0, 1.0)
}

fn hash_noise(value: Vec2) -> f32 {
    let dot = value.dot(Vec2::new(127.1, 311.7));
    (dot.sin() * 43_758.547).fract().abs()
}

fn channel(value: f32) -> u8 {
    (value.clamp(0.0, 1.0) * 255.0).round() as u8
}

fn fnv1a_64(bytes: &[u8]) -> u64 {
    let mut value = 0xcbf2_9ce4_8422_2325_u64;
    for byte in bytes {
        value ^= u64::from(*byte);
        value = value.wrapping_mul(0x0000_0100_0000_01b3);
    }
    value
}

#[cfg(test)]
mod tests {
    use crate::game::materials::{
        GalleryLightingPreset, MaterialApprovalState, MaterialCategory, ProceduralMaterialLibrary,
        approval_for_material, next_category,
    };

    #[test]
    fn material_library_contains_required_first_families() {
        let library = ProceduralMaterialLibrary::default();

        assert_eq!(library.materials.len(), 16);
        for id in [
            "dao/mat/mud-wall/v1",
            "dao/mat/wet-sand/v1",
            "dao/mat/wool/v1",
            "dao/mat/water/v1",
            "dao/mat/desert-stone/v1",
        ] {
            assert!(library.by_id(id).is_some(), "{id} missing");
        }
    }

    #[test]
    fn material_ids_are_stable_and_unique() {
        let library = ProceduralMaterialLibrary::default();
        let mut ids = std::collections::HashSet::new();

        for material in &library.materials {
            assert!(ids.insert(material.stable_id()));
        }
    }

    #[test]
    fn every_category_has_gallery_entries_or_is_traceable() {
        let library = ProceduralMaterialLibrary::default();

        for category in MaterialCategory::ALL {
            assert!(
                !library.by_category(category).is_empty(),
                "{} has no material",
                category.label()
            );
        }
    }

    #[test]
    fn category_navigation_wraps() {
        assert_eq!(
            next_category(Some(MaterialCategory::Ground), -1),
            MaterialCategory::Ruin
        );
        assert_eq!(
            next_category(Some(MaterialCategory::Ruin), 1),
            MaterialCategory::Ground
        );
    }

    #[test]
    fn approval_state_is_recorded_for_manifest() {
        let library = ProceduralMaterialLibrary::default();
        let water = library.by_id("dao/mat/water/v1").unwrap();

        assert!(matches!(
            approval_for_material(water),
            MaterialApprovalState::Satisfied | MaterialApprovalState::NeedsRevision
        ));
    }

    #[test]
    fn lighting_presets_cover_four_gallery_modes() {
        assert_eq!(GalleryLightingPreset::ALL.len(), 4);
        assert!(
            GalleryLightingPreset::ALL
                .iter()
                .any(|preset| preset.label() == "斜向强光")
        );
    }
}
