use bevy::prelude::*;

use crate::{
    core::performance::{FramePerformance, PerformancePhase},
    game::{
        assets::{
            ProceduralAssetKind, ProceduralAssetLod, ProceduralAssetMaterials,
            ProceduralSpawnRequest, spawn_procedural_asset_entity,
        },
        flow::{AppScreen, InGameState},
        intent::PerceptionState,
        journey::{DreamPhase, JourneyState},
        notebook::{
            NotebookEntryKind, NotebookRecord, NotebookSource, NotebookState, NotebookTag,
            record_notebook_entry,
        },
        places::planar_distance,
        regions::{RegionGraphState, RegionKind, RegionLandmarkKind},
        signs::SignState,
        world::{WandererPrototype, WorldMap},
    },
};

pub struct LandmarkPlugin;

impl Plugin for LandmarkPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(
            Update,
            (initialize_landmarks, update_landmark_visibility)
                .chain()
                .run_if(in_state(InGameState::Running)),
        );
        app.add_systems(OnExit(AppScreen::InGame), cleanup_landmark_session);
    }
}

type LandmarkUpdateResources<'w> = (
    Option<ResMut<'w, LandmarkState>>,
    Option<Res<'w, JourneyState>>,
    Option<Res<'w, SignState>>,
    Option<Res<'w, PerceptionState>>,
    Option<ResMut<'w, NotebookState>>,
    Res<'w, Time>,
    ResMut<'w, FramePerformance>,
);

#[derive(Debug, Resource, Clone, PartialEq)]
pub struct LandmarkState {
    pub landmarks: Vec<Landmark>,
    pub pyramid_signal: PyramidSignal,
    pub recorded_near_pyramid: bool,
}

impl LandmarkState {
    pub fn desert_pyramid(&self) -> Option<&Landmark> {
        self.landmarks
            .iter()
            .find(|landmark| landmark.kind == RegionLandmarkKind::DesertPyramid)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Landmark {
    pub id: u64,
    pub kind: RegionLandmarkKind,
    pub region_kind: RegionKind,
    pub position: Vec3,
    pub scale: f32,
    pub reveal_distance: f32,
    pub semantic_tags: Vec<LandmarkTag>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub enum LandmarkTag {
    Dream,
    Desert,
    Pyramid,
    Boundary,
    Water,
    Memory,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PyramidSignal {
    pub visible: bool,
    pub distance: Option<f32>,
    pub sandstorm_strength: f32,
    pub silhouette_strength: f32,
}

impl Default for PyramidSignal {
    fn default() -> Self {
        Self {
            visible: false,
            distance: None,
            sandstorm_strength: 0.0,
            silhouette_strength: 0.0,
        }
    }
}

#[derive(Debug, Component)]
struct LandmarkVisual {
    landmark_id: u64,
    near_detail: bool,
    feature_kind: LandmarkFeatureKind,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
enum LandmarkFeatureKind {
    Silhouette,
    Ruin,
    Oasis,
    Relic,
    Boundary,
}

fn initialize_landmarks(
    mut commands: Commands,
    world_map: Option<Res<WorldMap>>,
    regions: Option<Res<RegionGraphState>>,
    existing: Option<Res<LandmarkState>>,
    mut meshes: ResMut<Assets<Mesh>>,
    materials: Res<ProceduralAssetMaterials>,
) {
    if existing.is_some() {
        return;
    }
    let (Some(world_map), Some(regions)) = (world_map, regions) else {
        return;
    };
    let landmarks = build_landmarks(&world_map, &regions);
    for landmark in &landmarks {
        spawn_landmark_visual(&mut commands, &mut meshes, &materials, landmark);
    }

    let pyramid = landmarks
        .iter()
        .find(|landmark| landmark.kind == RegionLandmarkKind::DesertPyramid);
    tracing::info!(
        target: "dao_game::landmarks",
        landmark_count = landmarks.len(),
        pyramid_x = pyramid.map(|landmark| landmark.position.x),
        pyramid_z = pyramid.map(|landmark| landmark.position.z),
        "region landmarks initialized"
    );

    commands.insert_resource(LandmarkState {
        landmarks,
        pyramid_signal: PyramidSignal::default(),
        recorded_near_pyramid: false,
    });
}

pub fn build_landmarks(world_map: &WorldMap, regions: &RegionGraphState) -> Vec<Landmark> {
    let mut landmarks = Vec::new();
    for region in &regions.regions {
        let Some(kind) = region.landmark else {
            continue;
        };
        let offset = match kind {
            RegionLandmarkKind::VillageHeadland => Vec3::new(0.0, 0.0, 54.0),
            RegionLandmarkKind::MistRiver => Vec3::new(-18.0, 0.0, 12.0),
            RegionLandmarkKind::DesertPyramid => Vec3::new(36.0, 0.0, -18.0),
            RegionLandmarkKind::FarIslandLight => Vec3::new(18.0, 0.0, 74.0),
        };
        let scale = match kind {
            RegionLandmarkKind::DesertPyramid => 74.0,
            RegionLandmarkKind::MistRiver => 22.0,
            RegionLandmarkKind::VillageHeadland => 18.0,
            RegionLandmarkKind::FarIslandLight => 14.0,
        };
        let reveal_distance = match kind {
            RegionLandmarkKind::DesertPyramid => 1_250.0,
            RegionLandmarkKind::MistRiver => 140.0,
            RegionLandmarkKind::VillageHeadland => 90.0,
            RegionLandmarkKind::FarIslandLight => 260.0,
        };
        let position = ground_position(world_map, region.center + offset, 0.05);
        landmarks.push(Landmark {
            id: stable_landmark_id(region.seed, kind),
            kind,
            region_kind: region.kind,
            position,
            scale,
            reveal_distance,
            semantic_tags: tags_for_landmark(kind),
        });
    }
    landmarks
}

fn update_landmark_visibility(
    resources: LandmarkUpdateResources<'_>,
    player_query: Query<&Transform, With<WandererPrototype>>,
    mut visual_query: Query<
        (&LandmarkVisual, &mut Visibility, &mut Transform),
        Without<WandererPrototype>,
    >,
) {
    let started_at = std::time::Instant::now();
    let (state, journey, signs, perception, mut notebook, time, mut performance) = resources;
    let Some(mut state) = state else {
        return;
    };
    let Some(player_transform) = player_query.iter().next() else {
        return;
    };
    let player_position = player_transform.translation;
    let perception_boost = perception
        .as_deref()
        .filter(|perception| perception.active)
        .map(|perception| perception.intensity)
        .unwrap_or(0.0);
    let dream_echo = journey
        .as_deref()
        .filter(|journey| journey.dream.phase == DreamPhase::Afterglow)
        .map(|journey| journey.dream.echo_strength)
        .unwrap_or(0.0);
    let omen_intensity = signs.as_deref().map_or(0.0, |signs| signs.omen_intensity);
    let mut pyramid_signal = PyramidSignal::default();

    for (visual, mut visibility, mut transform) in &mut visual_query {
        let Some(landmark) = state
            .landmarks
            .iter()
            .find(|landmark| landmark.id == visual.landmark_id)
        else {
            continue;
        };
        let distance = planar_distance(player_position, landmark.position);
        let reveal = landmark_reveal_strength(
            landmark,
            distance,
            dream_echo,
            perception_boost,
            omen_intensity,
        );
        let feature_reveal = match visual.feature_kind {
            LandmarkFeatureKind::Oasis => (reveal + 0.22).clamp(0.0, 1.0),
            LandmarkFeatureKind::Relic => (reveal + perception_boost * 0.2).clamp(0.0, 1.0),
            LandmarkFeatureKind::Ruin => reveal,
            LandmarkFeatureKind::Silhouette | LandmarkFeatureKind::Boundary => reveal,
        };
        let visible =
            feature_reveal > 0.05 && (!visual.near_detail || distance < landmark.scale * 3.1);
        *visibility = if visible {
            Visibility::Visible
        } else {
            Visibility::Hidden
        };
        if landmark.kind == RegionLandmarkKind::DesertPyramid {
            let sandstorm = sandstorm_strength_for_pyramid(distance, perception_boost, dream_echo);
            let silhouette =
                (feature_reveal + perception_boost * 0.34 + dream_echo * 0.18).clamp(0.0, 1.0);
            pyramid_signal = PyramidSignal {
                visible,
                distance: Some(distance),
                sandstorm_strength: sandstorm,
                silhouette_strength: silhouette,
            };
            let visual_scale = 0.74 + silhouette * 0.38;
            transform.scale = Vec3::splat(visual_scale);
        }
    }

    if !state.recorded_near_pyramid
        && let Some(pyramid) = state.desert_pyramid()
        && planar_distance(player_position, pyramid.position) < pyramid.scale * 2.2
    {
        state.recorded_near_pyramid = true;
        let _ = record_notebook_entry(
            notebook.as_deref_mut(),
            NotebookRecord {
                kind: NotebookEntryKind::Place,
                at_seconds: time.elapsed_secs(),
                location: Some("沙漠".to_string()),
                source: NotebookSource::PlaceArrival,
                title: "沙暴后的巨大斜面".to_string(),
                body: "风沙短暂散开时，你看见梦里那座巨大金字塔并不是一个念头。".to_string(),
                tags: vec![
                    NotebookTag::Desert,
                    NotebookTag::Pyramid,
                    NotebookTag::Dream,
                ],
            },
        );
    }

    state.pyramid_signal = pyramid_signal;
    performance.record_phase_duration(PerformancePhase::Landmarks, started_at.elapsed());
}

fn spawn_landmark_visual(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &ProceduralAssetMaterials,
    landmark: &Landmark,
) {
    match landmark.kind {
        RegionLandmarkKind::DesertPyramid => {
            let pyramid_entity = spawn_procedural_asset_entity(
                commands,
                meshes,
                materials,
                ProceduralSpawnRequest::new(
                    ProceduralAssetKind::DesertPyramid,
                    landmark.id,
                    "DesertPyramidSilhouette",
                    Transform::from_translation(landmark.position),
                )
                .with_lod(ProceduralAssetLod::Near),
            );
            commands.entity(pyramid_entity).insert((
                Visibility::Hidden,
                LandmarkVisual {
                    landmark_id: landmark.id,
                    near_detail: false,
                    feature_kind: LandmarkFeatureKind::Silhouette,
                },
            ));
            let oasis_entity = spawn_procedural_asset_entity(
                commands,
                meshes,
                materials,
                ProceduralSpawnRequest::new(
                    ProceduralAssetKind::DesertOasis,
                    landmark.id,
                    "DesertOasis",
                    Transform::from_translation(
                        landmark.position
                            + Vec3::new(-landmark.scale * 0.74, 0.08, landmark.scale * 0.36),
                    ),
                )
                .with_lod(ProceduralAssetLod::Near),
            );
            commands.entity(oasis_entity).insert((
                Visibility::Hidden,
                LandmarkVisual {
                    landmark_id: landmark.id,
                    near_detail: true,
                    feature_kind: LandmarkFeatureKind::Oasis,
                },
            ));
            let relic_entity = spawn_procedural_asset_entity(
                commands,
                meshes,
                materials,
                ProceduralSpawnRequest::new(
                    ProceduralAssetKind::DesertRelic,
                    landmark.id,
                    "DesertRelicPlaceholder",
                    Transform::from_translation(
                        landmark.position
                            + Vec3::new(landmark.scale * 0.48, 0.0, -landmark.scale * 0.62),
                    )
                    .with_rotation(Quat::from_rotation_y(0.7)),
                )
                .with_lod(ProceduralAssetLod::Near),
            );
            commands.entity(relic_entity).insert((
                Visibility::Hidden,
                LandmarkVisual {
                    landmark_id: landmark.id,
                    near_detail: true,
                    feature_kind: LandmarkFeatureKind::Relic,
                },
            ));
            for (index, offset) in [
                Vec3::new(-landmark.scale * 0.28, 0.0, landmark.scale * 0.42),
                Vec3::new(landmark.scale * 0.32, 0.0, landmark.scale * 0.34),
                Vec3::new(0.0, 0.0, -landmark.scale * 0.46),
            ]
            .into_iter()
            .enumerate()
            {
                let ruin_entity = spawn_procedural_asset_entity(
                    commands,
                    meshes,
                    materials,
                    ProceduralSpawnRequest::new(
                        ProceduralAssetKind::PyramidRuinWall,
                        landmark.id.wrapping_add(index as u64),
                        "PyramidRuinWall",
                        Transform::from_translation(landmark.position + offset)
                            .with_rotation(Quat::from_rotation_y(offset.x * 0.008)),
                    )
                    .with_lod(ProceduralAssetLod::Near),
                );
                commands.entity(ruin_entity).insert((
                    Visibility::Hidden,
                    LandmarkVisual {
                        landmark_id: landmark.id,
                        near_detail: true,
                        feature_kind: LandmarkFeatureKind::Ruin,
                    },
                ));
            }
            commands.spawn((
                Name::new("PyramidDreamGlow"),
                DespawnOnExit(AppScreen::InGame),
                PointLight {
                    intensity: 140_000.0,
                    range: landmark.scale * 1.2,
                    radius: 3.0,
                    shadows_enabled: false,
                    color: Color::srgb(0.9, 0.66, 0.38),
                    ..Default::default()
                },
                Transform::from_translation(landmark.position + Vec3::Y * (landmark.scale * 0.44)),
                Visibility::Hidden,
                LandmarkVisual {
                    landmark_id: landmark.id,
                    near_detail: false,
                    feature_kind: LandmarkFeatureKind::Silhouette,
                },
            ));
        }
        RegionLandmarkKind::MistRiver => {
            let entity = spawn_procedural_asset_entity(
                commands,
                meshes,
                materials,
                ProceduralSpawnRequest::new(
                    ProceduralAssetKind::MistRiver,
                    landmark.id,
                    "MistRiverLandmark",
                    Transform::from_translation(landmark.position),
                )
                .with_lod(ProceduralAssetLod::Near),
            );
            commands.entity(entity).insert((
                Visibility::Visible,
                LandmarkVisual {
                    landmark_id: landmark.id,
                    near_detail: false,
                    feature_kind: LandmarkFeatureKind::Boundary,
                },
            ));
        }
        RegionLandmarkKind::VillageHeadland | RegionLandmarkKind::FarIslandLight => {
            let entity = spawn_procedural_asset_entity(
                commands,
                meshes,
                materials,
                ProceduralSpawnRequest::new(
                    ProceduralAssetKind::HeadlandMarker,
                    landmark.id,
                    landmark.kind.label(),
                    Transform::from_translation(landmark.position),
                )
                .with_lod(ProceduralAssetLod::Near),
            );
            commands.entity(entity).insert((
                Visibility::Visible,
                LandmarkVisual {
                    landmark_id: landmark.id,
                    near_detail: false,
                    feature_kind: LandmarkFeatureKind::Boundary,
                },
            ));
        }
    }
}

pub fn landmark_reveal_strength(
    landmark: &Landmark,
    distance: f32,
    dream_echo: f32,
    perception_boost: f32,
    omen_intensity: f32,
) -> f32 {
    let distance_strength = (1.0 - distance / landmark.reveal_distance.max(1.0)).clamp(0.0, 1.0);
    let semantic = if landmark.kind == RegionLandmarkKind::DesertPyramid {
        dream_echo * 0.32 + perception_boost * 0.38 + omen_intensity * 0.18
    } else {
        perception_boost * 0.18
    };
    (distance_strength + semantic).clamp(0.0, 1.0)
}

pub fn sandstorm_strength_for_pyramid(
    distance: f32,
    perception_boost: f32,
    dream_echo: f32,
) -> f32 {
    let distance_haze = (distance / 900.0).clamp(0.18, 1.0);
    (distance_haze * 0.72 + dream_echo * 0.18 - perception_boost * 0.38).clamp(0.18, 1.0)
}

fn tags_for_landmark(kind: RegionLandmarkKind) -> Vec<LandmarkTag> {
    match kind {
        RegionLandmarkKind::VillageHeadland => vec![LandmarkTag::Water, LandmarkTag::Memory],
        RegionLandmarkKind::MistRiver => vec![LandmarkTag::Boundary, LandmarkTag::Water],
        RegionLandmarkKind::DesertPyramid => {
            vec![
                LandmarkTag::Dream,
                LandmarkTag::Desert,
                LandmarkTag::Pyramid,
            ]
        }
        RegionLandmarkKind::FarIslandLight => vec![LandmarkTag::Boundary, LandmarkTag::Water],
    }
}

fn ground_position(world_map: &WorldMap, position: Vec3, y_offset: f32) -> Vec3 {
    let height = world_map
        .sample_height(position.x, position.z)
        .unwrap_or(position.y)
        .max(world_map.water_level() + 0.05);
    Vec3::new(position.x, height + y_offset, position.z)
}

fn stable_landmark_id(seed: u64, kind: RegionLandmarkKind) -> u64 {
    let mut value = seed
        .wrapping_mul(0x9E37_79B9_7F4A_7C15)
        .wrapping_add((kind as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9));
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^ (value >> 31)
}

fn cleanup_landmark_session(mut commands: Commands) {
    commands.remove_resource::<LandmarkState>();
}

#[cfg(test)]
mod tests {
    use bevy::prelude::Vec3;

    use crate::game::{
        landmarks::{Landmark, landmark_reveal_strength, sandstorm_strength_for_pyramid},
        regions::{RegionKind, RegionLandmarkKind},
    };

    #[test]
    fn pyramid_reveal_responds_to_dream_and_perception() {
        let landmark = pyramid();
        let base = landmark_reveal_strength(&landmark, 900.0, 0.0, 0.0, 0.0);
        let boosted = landmark_reveal_strength(&landmark, 900.0, 0.8, 1.0, 0.5);

        assert!(boosted > base);
        assert!(boosted <= 1.0);
    }

    #[test]
    fn perception_clears_sandstorm_without_removing_it() {
        let baseline = sandstorm_strength_for_pyramid(800.0, 0.0, 0.5);
        let perceived = sandstorm_strength_for_pyramid(800.0, 1.0, 0.5);

        assert!(perceived < baseline);
        assert!(perceived >= 0.18);
    }

    fn pyramid() -> Landmark {
        Landmark {
            id: 1,
            kind: RegionLandmarkKind::DesertPyramid,
            region_kind: RegionKind::Desert,
            position: Vec3::ZERO,
            scale: 74.0,
            reveal_distance: 1_250.0,
            semantic_tags: Vec::new(),
        }
    }
}
