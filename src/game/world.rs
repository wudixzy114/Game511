use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::{
        Mutex,
        mpsc::{self, Receiver, Sender, TryRecvError},
    },
    thread,
    time::Instant,
};

use bevy::{
    asset::RenderAssetUsages,
    math::primitives::{Capsule3d, Cylinder},
    mesh::{Indices, PrimitiveTopology, VertexAttributeValues},
    pbr::MeshMaterial3d,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

use crate::core::{
    config::{AppConfig, WorldConfig},
    performance::{FramePerformance, PerformancePhase},
};
use crate::game::{
    environment::WeatherKind,
    flow::{AppScreen, InGameState, SessionMode, in_session_mode},
    player::FirstPersonState,
};

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(WorldSeed(0));
        app.insert_resource(WorldCycle::default());
        app.add_systems(
            OnEnter(AppScreen::InGame),
            (
                reset_world_cycle,
                configure_world_seed,
                generate_world_map,
                cache_world_showcase_spots,
                create_terrain_material_texture,
            )
                .chain(),
        );
        app.add_systems(
            OnEnter(AppScreen::InGame),
            (spawn_camera, spawn_light, spawn_world).after(create_terrain_material_texture),
        );
        app.add_systems(
            Update,
            (
                advance_world_cycle,
                (
                    update_visible_chunks,
                    stream_terrain_chunks,
                    update_terrain_impostor,
                    update_collision_proxy.run_if(in_session_mode(SessionMode::Exploration)),
                )
                    .chain(),
                animate_wanderer,
            )
                .run_if(in_state(InGameState::Running)),
        );
        app.add_systems(OnExit(AppScreen::InGame), cleanup_world_session);
    }
}

#[derive(Debug, Resource, Clone, Copy, PartialEq, Eq)]
pub struct WorldSeed(pub u64);

#[derive(Debug, Resource, Clone, Copy, PartialEq)]
pub struct WorldCycle {
    pub normalized_time: f32,
    pub daylight: f32,
}

impl Default for WorldCycle {
    fn default() -> Self {
        Self {
            normalized_time: 0.12,
            daylight: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct TerrainSample {
    height: f32,
    moisture: f32,
    temperature: f32,
    erosion: f32,
    river: f32,
    sediment: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainTile {
    height: f32,
    moisture: f32,
    slope: f32,
    river: f32,
    erosion: f32,
    biome: BiomeKind,
}

impl TerrainTile {
    pub fn height(self) -> f32 {
        self.height
    }

    pub fn moisture(self) -> f32 {
        self.moisture
    }

    pub fn slope(self) -> f32 {
        self.slope
    }

    pub fn river(self) -> f32 {
        self.river
    }

    pub fn erosion(self) -> f32 {
        self.erosion
    }

    pub fn biome(self) -> BiomeKind {
        self.biome
    }
}

#[derive(Debug, Component)]
pub struct WandererPrototype;

#[derive(Debug, Component)]
pub struct WorldCamera;

#[derive(Debug, Component)]
pub(crate) struct SunLight;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BiomeKind {
    Water,
    Meadow,
    Grove,
    Steppe,
    Ridge,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldGridCoord {
    pub x: i32,
    pub z: i32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainVertexSample {
    pub world_x: f32,
    pub world_z: f32,
    pub height: f32,
    pub moisture: f32,
    pub temperature: f32,
    pub slope: f32,
    pub river: f32,
    pub erosion: f32,
    pub sediment: f32,
    pub biome: BiomeKind,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ShowcaseSpot {
    pub coord: WorldGridCoord,
    pub position: Vec3,
    pub biome: BiomeKind,
}

#[derive(Debug, Resource, Clone)]
pub struct WorldMap {
    seed: u64,
    radius: i32,
    showcase_search_radius: i32,
    chunk_radius: i32,
    cell_size: f32,
    subdivisions: u32,
    extent: f32,
    water_level: f32,
    terrain: WorldConfig,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct WorldChunkCoord {
    pub x: i32,
    pub z: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
enum TerrainLodLevel {
    High,
    Low,
    Preload,
}

impl TerrainLodLevel {
    fn subdivisions(self, base: u32) -> u32 {
        match self {
            Self::High => base.max(1),
            Self::Low => (base / 2).max(1),
            Self::Preload => (base / 4).max(1),
        }
    }

    fn scatter_enabled(self) -> bool {
        matches!(self, Self::High)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TerrainChunkKey {
    coord: WorldChunkCoord,
    lod: TerrainLodLevel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct TerrainChunkTarget {
    key: TerrainChunkKey,
    visible: bool,
}

#[derive(Debug, Clone)]
pub struct WorldChunk {
    pub coord: WorldChunkCoord,
    pub min: Vec2,
    pub max: Vec2,
    pub biome_counts: [u32; 5],
    pub average_height: f32,
    pub average_river: f32,
    pub average_erosion: f32,
    pub dominant_biome: BiomeKind,
    pub flow_exit: Option<WorldGridCoord>,
}

#[derive(Debug, Resource, Clone)]
struct TerrainRuntimeMaterial {
    handle: Handle<StandardMaterial>,
}

#[derive(Debug, Resource, Clone)]
struct TerrainImpostorMaterial {
    handle: Handle<StandardMaterial>,
}

#[derive(Debug, Resource, Clone)]
struct DetailMaterials {
    grove: Handle<StandardMaterial>,
    meadow: Handle<StandardMaterial>,
    steppe: Handle<StandardMaterial>,
    ridge: Handle<StandardMaterial>,
}

#[derive(Debug, Resource, Clone)]
struct DetailMeshBlueprints {
    grove: DetailMeshBlueprint,
    meadow: DetailMeshBlueprint,
    steppe: DetailMeshBlueprint,
    ridge: DetailMeshBlueprint,
}

#[derive(Debug, Clone)]
struct DetailMeshBlueprint {
    positions: Vec<[f32; 3]>,
    normals: Option<Vec<[f32; 3]>>,
    uvs: Option<Vec<[f32; 2]>>,
    indices: Vec<u32>,
    vertical_offset: f32,
}

#[derive(Debug, Resource, Clone, Copy)]
struct TerrainLodConfig {
    high_radius: i32,
    low_radius: i32,
    preload_radius: i32,
}

#[derive(Debug, Resource, Default)]
struct ChunkVisibilityState {
    active: Vec<TerrainChunkTarget>,
    center: Option<WorldChunkCoord>,
}

#[derive(Debug, Resource, Default)]
struct TerrainStreamingQueue {
    pending: VecDeque<TerrainChunkTarget>,
}

impl TerrainStreamingQueue {
    fn from_targets(targets: impl IntoIterator<Item = TerrainChunkTarget>) -> Self {
        let mut queue = Self::default();
        let existing = HashSet::new();
        queue.enqueue_missing(targets, &existing);
        queue
    }

    fn enqueue_missing(
        &mut self,
        targets: impl IntoIterator<Item = TerrainChunkTarget>,
        existing: &HashSet<TerrainChunkKey>,
    ) {
        let mut queued: HashSet<TerrainChunkKey> =
            self.pending.iter().map(|target| target.key).collect();
        for target in targets {
            if existing.contains(&target.key) || !queued.insert(target.key) {
                continue;
            }
            self.pending.push_back(target);
        }
    }

    fn retain_targets(&mut self, visible: &HashSet<TerrainChunkKey>) {
        self.pending.retain(|target| visible.contains(&target.key));
    }

    fn pop_next(&mut self) -> Option<TerrainChunkTarget> {
        self.pending.pop_front()
    }

    fn len(&self) -> usize {
        self.pending.len()
    }
}

#[derive(Debug, Resource, Clone, Copy)]
struct TerrainStreamingConfig {
    integrate_budget_per_frame: usize,
    background_budget_per_frame: usize,
    cache_capacity: usize,
}

#[derive(Debug, Resource, Clone, Copy)]
struct TerrainImpostorConfig {
    active_radius: i32,
    radial_bands: u32,
    angular_segments: u32,
}

#[derive(Debug, Resource, Default)]
struct TerrainImpostorState {
    anchor: Option<WorldChunkCoord>,
    mesh: Option<Handle<Mesh>>,
    entity: Option<Entity>,
}

impl TerrainImpostorState {
    fn should_refresh(&self, next_anchor: WorldChunkCoord, required_distance: i32) -> bool {
        let required_distance = required_distance.max(0);
        self.anchor.is_none_or(|anchor| {
            let dx = (anchor.x - next_anchor.x).abs();
            let dz = (anchor.z - next_anchor.z).abs();
            dx.max(dz) > required_distance
        })
    }
}

#[derive(Debug, Resource, Clone, Copy)]
struct TerrainCollisionConfig {
    active_radius: i32,
    subdivisions: u32,
    build_budget_per_frame: usize,
    cache_capacity: usize,
    integrate_budget_per_frame: usize,
}

#[derive(Debug, Clone)]
struct TerrainCollisionChunk {
    origin: Vec2,
    sample_step: f32,
    stride: usize,
    depth: usize,
    heights: Vec<f32>,
}

impl TerrainCollisionChunk {
    fn sample_height(&self, world_x: f32, world_z: f32) -> Option<f32> {
        if self.sample_step <= 0.0 || self.stride == 0 || self.depth == 0 {
            return None;
        }

        let max_x = (self.stride.saturating_sub(1)) as f32 * self.sample_step;
        let max_z = (self.depth.saturating_sub(1)) as f32 * self.sample_step;
        let local_x_world = world_x - self.origin.x;
        let local_z_world = world_z - self.origin.y;
        let epsilon = self.sample_step * 0.001;
        if local_x_world < -epsilon
            || local_z_world < -epsilon
            || local_x_world > max_x + epsilon
            || local_z_world > max_z + epsilon
        {
            return None;
        }

        let local_x = (local_x_world / self.sample_step).clamp(0.0, self.stride as f32 - 1.0);
        let local_z = (local_z_world / self.sample_step).clamp(0.0, self.depth as f32 - 1.0);
        let x0 = local_x.floor() as usize;
        let z0 = local_z.floor() as usize;
        let x1 = (x0 + 1).min(self.stride - 1);
        let z1 = (z0 + 1).min(self.depth - 1);
        let tx = local_x - x0 as f32;
        let tz = local_z - z0 as f32;

        let h00 = self.heights[z0 * self.stride + x0];
        let h10 = self.heights[z0 * self.stride + x1];
        let h01 = self.heights[z1 * self.stride + x0];
        let h11 = self.heights[z1 * self.stride + x1];
        let hx0 = h00 + (h10 - h00) * tx;
        let hx1 = h01 + (h11 - h01) * tx;
        Some(hx0 + (hx1 - hx0) * tz)
    }
}

#[derive(Debug, Clone)]
struct CachedTerrainCollisionChunk {
    chunk: TerrainCollisionChunk,
    last_used_tick: u64,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TerrainCollisionSample {
    pub height: f32,
    pub normal: Vec3,
    pub slope: f32,
}

#[derive(Debug, Resource, Default)]
pub struct TerrainCollisionProxy {
    active: Vec<WorldChunkCoord>,
    anchor: Option<WorldChunkCoord>,
    pending: VecDeque<WorldChunkCoord>,
    chunks: HashMap<WorldChunkCoord, CachedTerrainCollisionChunk>,
    tick: u64,
}

impl TerrainCollisionProxy {
    fn enqueue_missing(
        &mut self,
        targets: impl IntoIterator<Item = WorldChunkCoord>,
        existing: &HashSet<WorldChunkCoord>,
    ) {
        let mut queued: HashSet<WorldChunkCoord> = self.pending.iter().copied().collect();
        for target in targets {
            if existing.contains(&target) || !queued.insert(target) {
                continue;
            }
            self.pending.push_back(target);
        }
    }

    fn retain_active(&mut self, active: &HashSet<WorldChunkCoord>) {
        self.pending.retain(|coord| active.contains(coord));
    }

    fn pop_next(&mut self) -> Option<WorldChunkCoord> {
        self.pending.pop_front()
    }

    fn insert_chunk(&mut self, coord: WorldChunkCoord, chunk: TerrainCollisionChunk) {
        self.tick = self.tick.wrapping_add(1);
        self.chunks.insert(
            coord,
            CachedTerrainCollisionChunk {
                chunk,
                last_used_tick: self.tick,
            },
        );
    }

    fn get_chunk(&mut self, coord: WorldChunkCoord) -> Option<&TerrainCollisionChunk> {
        self.tick = self.tick.wrapping_add(1);
        let cached = self.chunks.get_mut(&coord)?;
        cached.last_used_tick = self.tick;
        Some(&cached.chunk)
    }

    pub fn sample_height(
        &mut self,
        world_map: &WorldMap,
        world_x: f32,
        world_z: f32,
    ) -> Option<f32> {
        let coord = world_map.chunk_coord_at(world_x, world_z)?;
        if let Some(chunk) = self.get_chunk(coord)
            && let Some(height) = chunk.sample_height(world_x, world_z)
        {
            return Some(height);
        }
        world_map.sample_height(world_x, world_z)
    }

    pub fn sample_ground(
        &mut self,
        world_map: &WorldMap,
        world_x: f32,
        world_z: f32,
    ) -> Option<TerrainCollisionSample> {
        let coord = world_map.chunk_coord_at(world_x, world_z)?;
        let height = self.sample_height(world_map, world_x, world_z)?;
        let sample_step = self
            .get_chunk(coord)
            .map(|chunk| chunk.sample_step)
            .unwrap_or_else(|| world_map.sample_spacing().max(0.001));
        let left = self
            .sample_height(world_map, world_x - sample_step, world_z)
            .unwrap_or(height);
        let right = self
            .sample_height(world_map, world_x + sample_step, world_z)
            .unwrap_or(height);
        let down = self
            .sample_height(world_map, world_x, world_z - sample_step)
            .unwrap_or(height);
        let up = self
            .sample_height(world_map, world_x, world_z + sample_step)
            .unwrap_or(height);

        Some(TerrainCollisionSample {
            height,
            normal: normal_from_neighbor_heights(left, right, down, up, sample_step),
            slope: ((right - left).hypot(up - down) / (sample_step * 2.0)).min(3.0),
        })
    }

    fn evict_inactive(&mut self, capacity: usize, active: &HashSet<WorldChunkCoord>) -> usize {
        let capacity = capacity.max(active.len());
        if self.chunks.len() <= capacity {
            return 0;
        }

        let mut candidates: Vec<(WorldChunkCoord, u64)> = self
            .chunks
            .iter()
            .filter(|(coord, _)| !active.contains(coord))
            .map(|(coord, cached)| (*coord, cached.last_used_tick))
            .collect();
        candidates.sort_by_key(|(_, last_used_tick)| *last_used_tick);

        let mut evicted = 0_usize;
        for (coord, _) in candidates {
            if self.chunks.len() <= capacity {
                break;
            }
            if self.chunks.remove(&coord).is_some() {
                evicted += 1;
            }
        }
        evicted
    }

    fn len(&self) -> usize {
        self.chunks.len()
    }
}

#[derive(Debug, Clone)]
struct CachedTerrainChunk {
    mesh: Handle<Mesh>,
    scatter_meshes: HashMap<BiomeKind, Handle<Mesh>>,
    last_used_tick: u64,
}

#[derive(Debug, Resource, Default)]
struct TerrainChunkCache {
    chunks: HashMap<TerrainChunkKey, CachedTerrainChunk>,
    tick: u64,
}

impl TerrainChunkCache {
    fn insert(&mut self, key: TerrainChunkKey, cached: CachedTerrainChunk) -> CachedTerrainChunk {
        self.tick = self.tick.wrapping_add(1);
        let mut cached = cached;
        cached.last_used_tick = self.tick;
        self.chunks.insert(key, cached.clone());
        cached
    }

    fn get(&mut self, key: TerrainChunkKey) -> Option<CachedTerrainChunk> {
        self.tick = self.tick.wrapping_add(1);
        let cached = self.chunks.get_mut(&key)?;
        cached.last_used_tick = self.tick;
        Some(cached.clone())
    }

    fn evict_inactive(
        &mut self,
        capacity: usize,
        active: &HashSet<TerrainChunkKey>,
        meshes: &mut Assets<Mesh>,
    ) -> usize {
        let capacity = capacity.max(active.len());
        if self.chunks.len() <= capacity {
            return 0;
        }

        let mut candidates: Vec<(TerrainChunkKey, u64)> = self
            .chunks
            .iter()
            .filter(|(coord, _)| !active.contains(coord))
            .map(|(coord, cached)| (*coord, cached.last_used_tick))
            .collect();
        candidates.sort_by_key(|(_, last_used_tick)| *last_used_tick);

        let mut evicted = 0_usize;
        for (coord, _) in candidates {
            if self.chunks.len() <= capacity {
                break;
            }
            if let Some(cached) = self.chunks.remove(&coord) {
                meshes.remove(cached.mesh.id());
                for handle in cached.scatter_meshes.into_values() {
                    meshes.remove(handle.id());
                }
                evicted += 1;
            }
        }
        evicted
    }

    fn len(&self) -> usize {
        self.chunks.len()
    }
}

#[derive(Debug)]
struct TerrainCollisionRequest {
    coord: WorldChunkCoord,
    world_map: WorldMap,
    subdivisions: u32,
}

#[derive(Debug)]
struct GeneratedTerrainCollisionChunk {
    coord: WorldChunkCoord,
    chunk: TerrainCollisionChunk,
}

#[derive(Resource)]
struct TerrainCollisionScheduler {
    sender: Sender<TerrainCollisionRequest>,
    receiver: Mutex<Receiver<GeneratedTerrainCollisionChunk>>,
    in_flight: HashSet<WorldChunkCoord>,
}

impl Default for TerrainCollisionScheduler {
    fn default() -> Self {
        create_collision_scheduler()
    }
}

type TerrainStreamResources<'w> = (
    Res<'w, AppConfig>,
    ResMut<'w, FramePerformance>,
    Res<'w, SessionMode>,
    Res<'w, WorldMap>,
    Res<'w, TerrainRuntimeMaterial>,
    Res<'w, DetailMaterials>,
    Res<'w, DetailMeshBlueprints>,
    Res<'w, TerrainStreamingConfig>,
);

type ChunkVisibilityQueries<'w, 's> = (
    Query<'w, 's, (Entity, &'static TerrainChunkEntity)>,
    Query<'w, 's, (Entity, &'static TerrainDetailEntity)>,
    Query<'w, 's, &'static Transform, With<WorldCamera>>,
);

type TerrainStreamState<'w> = (
    ResMut<'w, TerrainStreamingQueue>,
    ResMut<'w, TerrainChunkCache>,
    ResMut<'w, TerrainGenerationScheduler>,
);

type WorldSpawnResources<'w> = (
    Res<'w, WorldMap>,
    Res<'w, WorldSeed>,
    Res<'w, WorldShowcaseSpots>,
    Res<'w, TerrainRuntimeMaterial>,
    Res<'w, TerrainLodConfig>,
);

type WorldStreamingBootstrapResources<'w> = (
    Res<'w, TerrainImpostorMaterial>,
    Res<'w, TerrainImpostorConfig>,
    ResMut<'w, TerrainImpostorState>,
    ResMut<'w, TerrainCollisionProxy>,
    ResMut<'w, TerrainCollisionScheduler>,
    Res<'w, TerrainCollisionConfig>,
);

type TerrainImpostorResources<'w> = (
    Res<'w, WorldMap>,
    Res<'w, TerrainImpostorMaterial>,
    Res<'w, TerrainImpostorConfig>,
    Res<'w, TerrainLodConfig>,
    ResMut<'w, TerrainImpostorState>,
);

#[derive(Debug, Component, Clone, Copy, PartialEq, Eq)]
struct TerrainChunkEntity {
    key: TerrainChunkKey,
}

#[derive(Debug, Component, Clone, Copy, PartialEq, Eq)]
struct TerrainDetailEntity {
    key: TerrainChunkKey,
}

#[derive(Debug, Component)]
struct TerrainImpostorEntity;

impl WorldMap {
    fn new(seed: u64, config: &AppConfig) -> Self {
        let radius = config.world.world_radius.max(1);
        let chunk_radius = config.world.chunk_radius.max(1);
        let subdivisions = config.world.terrain_subdivisions.max(1);
        let extent = radius as f32 * config.world.cell_size;

        Self {
            seed,
            radius,
            showcase_search_radius: config.world.showcase_search_radius.max(1).min(radius),
            chunk_radius,
            cell_size: config.world.cell_size,
            subdivisions,
            extent,
            water_level: config.world.water_level,
            terrain: config.world.clone(),
        }
    }

    #[cfg(test)]
    pub fn new_for_testing(seed: u64, config: &AppConfig) -> Self {
        Self::new(seed, config)
    }

    pub fn radius(&self) -> i32 {
        self.radius
    }

    pub fn seed_value(&self) -> u64 {
        self.seed
    }

    pub fn showcase_search_radius(&self) -> i32 {
        self.showcase_search_radius
    }

    pub fn cell_size(&self) -> f32 {
        self.cell_size
    }

    pub fn chunk_radius(&self) -> i32 {
        self.chunk_radius
    }

    pub fn tile_size(&self) -> f32 {
        self.cell_size
    }

    pub fn subdivisions(&self) -> u32 {
        self.subdivisions
    }

    pub fn water_level(&self) -> f32 {
        self.water_level
    }

    pub fn extent(&self) -> f32 {
        self.extent
    }

    pub fn chunk_world_span(&self) -> f32 {
        self.cell_size * self.chunk_radius as f32
    }

    pub fn chunk_coord_at(&self, world_x: f32, world_z: f32) -> Option<WorldChunkCoord> {
        if !self.within_world_bounds(world_x, world_z) {
            return None;
        }

        let chunk_span = self.chunk_world_span().max(0.001);
        Some(WorldChunkCoord {
            x: (world_x / chunk_span).floor() as i32,
            z: (world_z / chunk_span).floor() as i32,
        })
    }

    pub fn find_chunk(&self, coord: WorldChunkCoord) -> Option<WorldChunk> {
        self.describe_chunk(coord)
    }

    pub fn describe_chunk(&self, coord: WorldChunkCoord) -> Option<WorldChunk> {
        let (tile_x_min, tile_x_max, tile_z_min, tile_z_max) =
            self.chunk_mesh_tile_bounds(coord)?;

        let mut biome_counts = [0_u32; 5];
        let mut height_sum = 0.0_f32;
        let mut river_sum = 0.0_f32;
        let mut erosion_sum = 0.0_f32;
        let mut count = 0.0_f32;
        let mut dominant_biome = BiomeKind::Meadow;
        let mut dominant_score = 0_u32;

        for tile_z in tile_z_min..=tile_z_max {
            for tile_x in tile_x_min..=tile_x_max {
                let Some(tile) = self.tile_at_grid(tile_x, tile_z) else {
                    continue;
                };
                biome_counts[biome_index(tile.biome())] += 1;
                height_sum += tile.height();
                river_sum += tile.river();
                erosion_sum += tile.erosion();
                count += 1.0;
            }
        }

        for (index, total) in biome_counts.iter().enumerate() {
            if *total > dominant_score {
                dominant_score = *total;
                dominant_biome = biome_from_index(index);
            }
        }

        Some(WorldChunk {
            coord,
            min: Vec2::new(
                tile_x_min as f32 * self.cell_size,
                tile_z_min as f32 * self.cell_size,
            ),
            max: Vec2::new(
                tile_x_max as f32 * self.cell_size,
                tile_z_max as f32 * self.cell_size,
            ),
            biome_counts,
            average_height: height_sum / count.max(1.0),
            average_river: river_sum / count.max(1.0),
            average_erosion: erosion_sum / count.max(1.0),
            dominant_biome,
            flow_exit: self.find_chunk_flow_exit(tile_x_min, tile_z_min, tile_x_max, tile_z_max),
        })
    }

    pub fn tile_at_grid(&self, x: i32, z: i32) -> Option<TerrainTile> {
        if x < -self.radius || x > self.radius || z < -self.radius || z > self.radius {
            return None;
        }

        let sample = self.sample_vertex(x as f32 * self.cell_size, z as f32 * self.cell_size)?;
        Some(TerrainTile {
            height: sample.height,
            moisture: sample.moisture,
            slope: sample.slope,
            river: sample.river,
            erosion: sample.erosion,
            biome: sample.biome,
        })
    }

    pub fn sample_world_position(&self, position: Vec3) -> Option<TerrainTile> {
        let height = self.sample_height(position.x, position.z)?;
        let moisture = self.sample_moisture(position.x, position.z)?;
        let slope = self.sample_slope(position.x, position.z)?;
        let biome = self.sample_biome(position.x, position.z)?;

        Some(TerrainTile {
            height,
            moisture,
            slope,
            river: self.sample_river(position.x, position.z)?,
            erosion: self.sample_erosion(position.x, position.z)?,
            biome,
        })
    }

    pub fn tile_translation(&self, x: i32, z: i32, height: f32) -> Vec3 {
        Vec3::new(x as f32 * self.cell_size, height, z as f32 * self.cell_size)
    }

    pub fn sample_height(&self, world_x: f32, world_z: f32) -> Option<f32> {
        self.sample_vertex_field(world_x, world_z, |sample| sample.height)
    }

    pub fn sample_moisture(&self, world_x: f32, world_z: f32) -> Option<f32> {
        self.sample_vertex_field(world_x, world_z, |sample| sample.moisture)
    }

    pub fn sample_slope(&self, world_x: f32, world_z: f32) -> Option<f32> {
        self.sample_vertex_field(world_x, world_z, |sample| sample.slope)
    }

    pub fn sample_river(&self, world_x: f32, world_z: f32) -> Option<f32> {
        self.sample_vertex_field(world_x, world_z, |sample| sample.river)
    }

    pub fn sample_erosion(&self, world_x: f32, world_z: f32) -> Option<f32> {
        self.sample_vertex_field(world_x, world_z, |sample| sample.erosion)
    }

    pub fn sample_biome(&self, world_x: f32, world_z: f32) -> Option<BiomeKind> {
        Some(self.sample_vertex(world_x, world_z)?.biome)
    }

    pub fn build_terrain_mesh(&self) -> Mesh {
        self.build_terrain_mesh_for_chunk(WorldChunkCoord { x: 0, z: 0 })
            .expect("origin terrain mesh should exist")
    }

    pub fn build_terrain_mesh_for_chunk(&self, coord: WorldChunkCoord) -> Option<Mesh> {
        self.build_terrain_mesh_for_chunk_with_subdivisions(coord, self.subdivisions)
    }

    pub fn build_terrain_mesh_for_chunk_with_subdivisions(
        &self,
        coord: WorldChunkCoord,
        subdivisions: u32,
    ) -> Option<Mesh> {
        let (tile_x_min, tile_x_max, tile_z_min, tile_z_max) =
            self.chunk_mesh_tile_bounds(coord)?;
        let subdivisions = subdivisions.max(1);
        let x_steps = ((tile_x_max - tile_x_min) as u32 * subdivisions) as usize;
        let z_steps = ((tile_z_max - tile_z_min) as u32 * subdivisions) as usize;
        let mesh_stride = x_steps + 1;
        let mesh_depth = z_steps + 1;

        if mesh_stride < 2 || mesh_depth < 2 {
            return None;
        }

        let vertex_count = mesh_stride * mesh_depth;
        let mut positions = Vec::with_capacity(vertex_count);
        let mut normals = Vec::with_capacity(vertex_count);
        let mut colors = Vec::with_capacity(mesh_stride * mesh_depth);
        let mut uvs = Vec::with_capacity(mesh_stride * mesh_depth);
        let uv_scale = self.chunk_world_span().max(0.001);
        let sample_step = (self.cell_size / subdivisions as f32).max(0.001);
        let mut raw_samples = Vec::with_capacity(vertex_count);
        let mut heights = Vec::with_capacity(vertex_count);

        for z_step in 0..=z_steps {
            for x_step in 0..=x_steps {
                let world_x =
                    (tile_x_min as f32 + x_step as f32 / subdivisions as f32) * self.cell_size;
                let world_z =
                    (tile_z_min as f32 + z_step as f32 / subdivisions as f32) * self.cell_size;
                if !self.within_world_bounds(world_x, world_z) {
                    return None;
                }
                let sample = sample_terrain(world_x, world_z, self.seed, &self.terrain);
                heights.push(sample.height);
                raw_samples.push(sample);
            }
        }

        for z_step in 0..=z_steps {
            for x_step in 0..=x_steps {
                let index = z_step * mesh_stride + x_step;
                let world_x =
                    (tile_x_min as f32 + x_step as f32 / subdivisions as f32) * self.cell_size;
                let world_z =
                    (tile_z_min as f32 + z_step as f32 / subdivisions as f32) * self.cell_size;
                let sample = raw_samples[index];
                let left = if x_step > 0 {
                    heights[index - 1]
                } else {
                    self.sample_height_clamped(world_x - sample_step, world_z)
                };
                let right = if x_step < x_steps {
                    heights[index + 1]
                } else {
                    self.sample_height_clamped(world_x + sample_step, world_z)
                };
                let down = if z_step > 0 {
                    heights[index - mesh_stride]
                } else {
                    self.sample_height_clamped(world_x, world_z - sample_step)
                };
                let up = if z_step < z_steps {
                    heights[index + mesh_stride]
                } else {
                    self.sample_height_clamped(world_x, world_z + sample_step)
                };
                let slope = self.slope_from_neighbors(sample.height, left, right, down, up);
                let biome = determine_biome(
                    sample,
                    slope,
                    self.water_level,
                    self.terrain.shoreline_blend,
                );
                let vertex = TerrainVertexSample {
                    world_x,
                    world_z,
                    height: sample.height,
                    moisture: sample.moisture,
                    temperature: sample.temperature,
                    slope,
                    river: sample.river,
                    erosion: sample.erosion,
                    sediment: sample.sediment,
                    biome,
                };
                positions.push([vertex.world_x, vertex.height, vertex.world_z]);
                normals.push(
                    normal_from_neighbor_heights(left, right, down, up, sample_step).to_array(),
                );
                colors.push(
                    vertex_color(&vertex, self.water_level)
                        .to_linear()
                        .to_f32_array(),
                );
                uvs.push([vertex.world_x / uv_scale, vertex.world_z / uv_scale]);
            }
        }

        let mut indices = Vec::with_capacity((mesh_stride - 1) * (mesh_depth - 1) * 6);
        for z in 0..(mesh_depth - 1) {
            for x in 0..(mesh_stride - 1) {
                let top_left = (z * mesh_stride + x) as u32;
                let top_right = (z * mesh_stride + x + 1) as u32;
                let bottom_left = ((z + 1) * mesh_stride + x) as u32;
                let bottom_right = ((z + 1) * mesh_stride + x + 1) as u32;

                indices.extend_from_slice(&[
                    top_left,
                    bottom_left,
                    top_right,
                    top_right,
                    bottom_left,
                    bottom_right,
                ]);
            }
        }

        let mut mesh = Mesh::new(
            PrimitiveTopology::TriangleList,
            RenderAssetUsages::default(),
        )
        .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
        .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
        .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
        .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
        mesh.insert_indices(Indices::U32(indices));
        Some(mesh)
    }

    fn chunk_mesh_tile_bounds(&self, coord: WorldChunkCoord) -> Option<(i32, i32, i32, i32)> {
        let raw_x_min = coord.x * self.chunk_radius;
        let raw_z_min = coord.z * self.chunk_radius;
        let raw_x_max = raw_x_min + self.chunk_radius;
        let raw_z_max = raw_z_min + self.chunk_radius;

        let tile_x_min = raw_x_min.max(-self.radius);
        let tile_z_min = raw_z_min.max(-self.radius);
        let tile_x_max = raw_x_max.min(self.radius);
        let tile_z_max = raw_z_max.min(self.radius);

        if tile_x_min >= tile_x_max || tile_z_min >= tile_z_max {
            return None;
        }

        Some((tile_x_min, tile_x_max, tile_z_min, tile_z_max))
    }

    fn find_chunk_flow_exit(
        &self,
        tile_x_min: i32,
        tile_z_min: i32,
        tile_x_max: i32,
        tile_z_max: i32,
    ) -> Option<WorldGridCoord> {
        let mut best: Option<(WorldGridCoord, f32)> = None;

        for tile_z in tile_z_min..=tile_z_max {
            for tile_x in tile_x_min..=tile_x_max {
                let Some(tile) = self.tile_at_grid(tile_x, tile_z) else {
                    continue;
                };
                let score =
                    tile.river() * 0.7 + (1.0 - tile.erosion()) * 0.15 - tile.height() * 0.05;
                let coord = WorldGridCoord {
                    x: tile_x,
                    z: tile_z,
                };
                match best {
                    None => best = Some((coord, score)),
                    Some((_, current)) if score > current => best = Some((coord, score)),
                    _ => {}
                }
            }
        }

        best.map(|(coord, _)| coord)
    }

    fn sample_spacing(&self) -> f32 {
        self.cell_size / self.subdivisions as f32
    }

    fn within_world_bounds(&self, world_x: f32, world_z: f32) -> bool {
        world_x.abs() <= self.extent && world_z.abs() <= self.extent
    }

    fn sample_vertex(&self, world_x: f32, world_z: f32) -> Option<TerrainVertexSample> {
        if !self.within_world_bounds(world_x, world_z) {
            return None;
        }

        let sample = sample_terrain(world_x, world_z, self.seed, &self.terrain);
        let slope = self.compute_slope_at(world_x, world_z, sample.height);
        let biome = determine_biome(
            sample,
            slope,
            self.water_level,
            self.terrain.shoreline_blend,
        );

        Some(TerrainVertexSample {
            world_x,
            world_z,
            height: sample.height,
            moisture: sample.moisture,
            temperature: sample.temperature,
            slope,
            river: sample.river,
            erosion: sample.erosion,
            sediment: sample.sediment,
            biome,
        })
    }

    fn sample_height_clamped(&self, world_x: f32, world_z: f32) -> f32 {
        let clamped_x = world_x.clamp(-self.extent, self.extent);
        let clamped_z = world_z.clamp(-self.extent, self.extent);
        sample_terrain(clamped_x, clamped_z, self.seed, &self.terrain).height
    }

    fn compute_slope_at(&self, world_x: f32, world_z: f32, center: f32) -> f32 {
        let step = self.sample_spacing().max(0.001);
        let left = self.sample_height_clamped(world_x - step, world_z);
        let right = self.sample_height_clamped(world_x + step, world_z);
        let down = self.sample_height_clamped(world_x, world_z - step);
        let up = self.sample_height_clamped(world_x, world_z + step);

        self.slope_from_neighbors(center, left, right, down, up)
    }

    fn slope_from_neighbors(&self, center: f32, left: f32, right: f32, down: f32, up: f32) -> f32 {
        let step = self.sample_spacing().max(0.001);
        let dx = (right - left) / (step * 2.0);
        let dz = (up - down) / (step * 2.0);
        ((dx * dx + dz * dz).sqrt() + (center - self.water_level).abs() * 0.02).min(3.0)
    }

    fn sample_vertex_field(
        &self,
        world_x: f32,
        world_z: f32,
        accessor: impl Fn(TerrainVertexSample) -> f32,
    ) -> Option<f32> {
        Some(accessor(self.sample_vertex(world_x, world_z)?))
    }
}

#[derive(Debug, Resource, Clone, Copy)]
pub struct WorldPresentationControl {
    pub time_override: Option<f32>,
    pub weather_override: Option<WeatherKind>,
    pub wander_target: Option<Vec3>,
    pub wander_speed_multiplier: f32,
}

impl Default for WorldPresentationControl {
    fn default() -> Self {
        Self {
            time_override: None,
            weather_override: None,
            wander_target: None,
            wander_speed_multiplier: 1.0,
        }
    }
}

#[derive(Debug, Resource, Clone, Copy)]
pub struct WorldShowcaseSpots {
    pub ridge: ShowcaseSpot,
    pub grove: ShowcaseSpot,
    pub water: ShowcaseSpot,
    pub meadow: ShowcaseSpot,
}

#[derive(Debug, Clone, Copy)]
struct ScatterPlacement {
    position: Vec3,
    biome: BiomeKind,
    scale: f32,
}

struct TerrainChunkSpawnContext<'a> {
    material: &'a Handle<StandardMaterial>,
    detail_materials: &'a DetailMaterials,
    build_context: TerrainChunkBuildContext<'a>,
}

#[derive(Clone, Copy)]
struct TerrainChunkBuildContext<'a> {
    world_map: &'a WorldMap,
    seed: u64,
}

#[derive(Debug, Clone)]
struct GeneratedTerrainChunk {
    key: TerrainChunkKey,
    mesh: Mesh,
    scatter_meshes: HashMap<BiomeKind, Mesh>,
}

#[derive(Debug)]
struct TerrainGenerationRequest {
    key: TerrainChunkKey,
    world_map: WorldMap,
    detail_mesh_blueprints: DetailMeshBlueprints,
}

#[derive(Resource)]
struct TerrainGenerationScheduler {
    sender: Sender<TerrainGenerationRequest>,
    receiver: Mutex<Receiver<GeneratedTerrainChunk>>,
    in_flight: HashSet<TerrainChunkKey>,
}

fn reset_world_cycle(mut cycle: ResMut<WorldCycle>) {
    *cycle = WorldCycle::default();
}

fn configure_world_seed(config: Res<AppConfig>, mut seed: ResMut<WorldSeed>) {
    seed.0 = config.world.seed;
    tracing::info!(
        target: "dao_game::world::bootstrap",
        seed = seed.0,
        "world seed configured"
    );
}

fn generate_world_map(mut commands: Commands, config: Res<AppConfig>, seed: Res<WorldSeed>) {
    let started_at = Instant::now();
    let world_map = WorldMap::new(seed.0, &config);
    let chunk_edge_vertices =
        (world_map.chunk_radius() as u32 * world_map.subdivisions()).saturating_add(1);
    let chunk_vertex_budget = chunk_edge_vertices * chunk_edge_vertices;
    commands.insert_resource(world_map);
    tracing::info!(
        target: "dao_game::world::generation",
        radius = config.world.world_radius,
        seed = seed.0,
        extent = config.world.world_radius as f32 * config.world.cell_size,
        chunk_radius = config.world.chunk_radius,
        chunk_vertex_budget = chunk_vertex_budget,
        subdivisions = config.world.terrain_subdivisions,
        showcase_search_radius = config.world.showcase_search_radius,
        generation_ms = started_at.elapsed().as_secs_f32() * 1000.0,
        "streaming world map rules generated"
    );
}

fn create_terrain_material_texture(
    mut commands: Commands,
    mut images: ResMut<Assets<Image>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    config: Res<AppConfig>,
    world_map: Res<WorldMap>,
) {
    let resolution = config.world.material_texture_resolution.max(32);
    let image = build_terrain_palette_texture(&world_map, resolution);
    let image_handle = images.add(image);
    let material_handle = materials.add(StandardMaterial {
        base_color_texture: Some(image_handle.clone()),
        perceptual_roughness: 0.96,
        reflectance: 0.18,
        ..Default::default()
    });
    let impostor_material_handle = materials.add(StandardMaterial {
        base_color_texture: Some(image_handle.clone()),
        base_color: Color::srgb(0.84, 0.87, 0.91),
        perceptual_roughness: 1.0,
        reflectance: 0.04,
        ..Default::default()
    });

    commands.insert_resource(TerrainRuntimeMaterial {
        handle: material_handle,
    });
    commands.insert_resource(TerrainImpostorMaterial {
        handle: impostor_material_handle,
    });
    commands.insert_resource(ChunkVisibilityState::default());
    commands.insert_resource(TerrainStreamingQueue::default());
    commands.insert_resource(TerrainLodConfig {
        high_radius: config.world.high_detail_chunk_radius.max(0),
        low_radius: config
            .world
            .low_detail_chunk_radius
            .max(config.world.high_detail_chunk_radius.max(0)),
        preload_radius: config
            .world
            .preload_chunk_radius
            .max(config.world.low_detail_chunk_radius.max(0)),
    });
    commands.insert_resource(TerrainStreamingConfig {
        integrate_budget_per_frame: config.world.streaming_chunk_budget.max(1) as usize,
        background_budget_per_frame: config.world.background_generation_budget.max(1) as usize,
        cache_capacity: config.world.streaming_cache_capacity.max(1),
    });
    commands.insert_resource(TerrainImpostorConfig {
        active_radius: config
            .world
            .impostor_chunk_radius
            .max(config.world.low_detail_chunk_radius.max(0) + 1),
        radial_bands: config.world.impostor_radial_bands.max(1),
        angular_segments: config.world.impostor_angular_segments.max(16),
    });
    commands.insert_resource(TerrainImpostorState::default());
    commands.insert_resource(TerrainCollisionConfig {
        active_radius: config.world.collision_proxy_radius.max(0),
        subdivisions: config
            .world
            .collision_subdivisions
            .max(1)
            .min(config.world.terrain_subdivisions.max(1)),
        build_budget_per_frame: config.world.collision_chunk_budget.max(1) as usize,
        cache_capacity: config.world.collision_cache_capacity.max(1),
        integrate_budget_per_frame: config.world.collision_chunk_budget.max(1) as usize,
    });
    commands.insert_resource(TerrainCollisionProxy::default());
    commands.insert_resource(create_generation_scheduler());
    commands.insert_resource(create_collision_scheduler());
}

fn create_generation_scheduler() -> TerrainGenerationScheduler {
    let (request_sender, request_receiver) = mpsc::channel::<TerrainGenerationRequest>();
    let (result_sender, result_receiver) = mpsc::channel::<GeneratedTerrainChunk>();

    thread::spawn(move || {
        while let Ok(request) = request_receiver.recv() {
            let build_context = TerrainChunkBuildContext {
                world_map: &request.world_map,
                seed: request.world_map.seed,
            };
            let Some(mesh) = build_chunk_mesh_for_lod(&build_context, request.key) else {
                continue;
            };
            let scatter_meshes = build_scatter_meshes_for_key(
                &build_context,
                request.key,
                &request.detail_mesh_blueprints,
            );
            let _ = result_sender.send(GeneratedTerrainChunk {
                key: request.key,
                mesh,
                scatter_meshes,
            });
        }
    });

    TerrainGenerationScheduler {
        sender: request_sender,
        receiver: Mutex::new(result_receiver),
        in_flight: HashSet::new(),
    }
}

fn create_collision_scheduler() -> TerrainCollisionScheduler {
    let (request_sender, request_receiver) = mpsc::channel::<TerrainCollisionRequest>();
    let (result_sender, result_receiver) = mpsc::channel::<GeneratedTerrainCollisionChunk>();

    thread::spawn(move || {
        while let Ok(request) = request_receiver.recv() {
            let Some(chunk) =
                build_collision_chunk(&request.world_map, request.coord, request.subdivisions)
            else {
                continue;
            };
            let _ = result_sender.send(GeneratedTerrainCollisionChunk {
                coord: request.coord,
                chunk,
            });
        }
    });

    TerrainCollisionScheduler {
        sender: request_sender,
        receiver: Mutex::new(result_receiver),
        in_flight: HashSet::new(),
    }
}

fn cache_world_showcase_spots(mut commands: Commands, world_map: Res<WorldMap>) {
    let ridge = find_showcase_spot(&world_map, BiomeKind::Ridge)
        .unwrap_or_else(|| fallback_showcase_spot(&world_map, BiomeKind::Ridge));
    let grove = find_showcase_spot(&world_map, BiomeKind::Grove)
        .unwrap_or_else(|| fallback_showcase_spot(&world_map, BiomeKind::Grove));
    let water = find_showcase_spot(&world_map, BiomeKind::Water)
        .unwrap_or_else(|| fallback_showcase_spot(&world_map, BiomeKind::Water));
    let meadow = find_showcase_spot(&world_map, BiomeKind::Meadow)
        .unwrap_or_else(|| fallback_showcase_spot(&world_map, BiomeKind::Meadow));

    commands.insert_resource(WorldShowcaseSpots {
        ridge,
        grove,
        water,
        meadow,
    });
}

fn spawn_camera(mut commands: Commands, spots: Res<WorldShowcaseSpots>) {
    commands.spawn((
        Name::new("WorldCamera"),
        DespawnOnExit(AppScreen::InGame),
        Camera3d::default(),
        Transform::from_xyz(
            spots.meadow.position.x - 18.0,
            spots.meadow.position.y + 10.0,
            spots.meadow.position.z + 18.0,
        )
        .looking_at(spots.meadow.position, Vec3::Y),
        WorldCamera,
    ));
}

fn spawn_light(mut commands: Commands) {
    commands.spawn((
        Name::new("SunLight"),
        DespawnOnExit(AppScreen::InGame),
        DirectionalLight {
            shadows_enabled: true,
            illuminance: 18_000.0,
            ..Default::default()
        },
        Transform::from_rotation(Quat::from_euler(EulerRot::XYZ, -0.9, 0.6, 0.0)),
        SunLight,
    ));
}

fn spawn_world(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    session_mode: Res<SessionMode>,
    runtime_resources: WorldStreamingBootstrapResources<'_>,
    resources: WorldSpawnResources<'_>,
) {
    let (
        impostor_material,
        impostor_config,
        mut impostor_state,
        mut collision_proxy,
        mut collision_scheduler,
        collision_config,
    ) = runtime_resources;
    let started_at = Instant::now();
    let (world_map, seed, spots, terrain_material, lod_config) = resources;
    let water_material = materials.add(StandardMaterial {
        base_color: Color::srgba(0.12, 0.29, 0.42, 0.75),
        alpha_mode: AlphaMode::Blend,
        metallic: 0.06,
        perceptual_roughness: 0.09,
        ..Default::default()
    });
    let detail_materials = DetailMaterials {
        grove: materials.add(StandardMaterial {
            base_color: Color::srgb(0.17, 0.4, 0.21),
            perceptual_roughness: 0.9,
            ..Default::default()
        }),
        meadow: materials.add(StandardMaterial {
            base_color: Color::srgb(0.41, 0.48, 0.27),
            perceptual_roughness: 0.95,
            ..Default::default()
        }),
        steppe: materials.add(StandardMaterial {
            base_color: Color::srgb(0.56, 0.47, 0.28),
            perceptual_roughness: 0.98,
            ..Default::default()
        }),
        ridge: materials.add(StandardMaterial {
            base_color: Color::srgb(0.46, 0.44, 0.43),
            perceptual_roughness: 0.99,
            ..Default::default()
        }),
    };
    let detail_mesh_blueprints = DetailMeshBlueprints {
        meadow: detail_mesh_blueprint(Mesh::from(Capsule3d::new(0.09, 0.6)), 0.35),
        grove: detail_mesh_blueprint(Mesh::from(Cylinder::new(0.18, 1.8)), 0.92),
        steppe: detail_mesh_blueprint(Mesh::from(Cylinder::new(0.28, 0.42)), 0.18),
        ridge: detail_mesh_blueprint(Mesh::from(Cylinder::new(0.16, 2.4)), 1.15),
    };
    let initial_center = world_map.chunk_coord_at(spots.meadow.position.x, spots.meadow.position.z);
    let initial_targets = chunk_targets_for_camera(&world_map, spots.meadow.position, *lod_config);
    let critical_key = world_map
        .chunk_coord_at(spots.meadow.position.x, spots.meadow.position.z)
        .map(|coord| TerrainChunkKey {
            coord,
            lod: TerrainLodLevel::High,
        });
    let spawn_context = TerrainChunkSpawnContext {
        material: &terrain_material.handle,
        detail_materials: &detail_materials,
        build_context: TerrainChunkBuildContext {
            world_map: &world_map,
            seed: seed.0,
        },
    };
    let mut chunk_cache = TerrainChunkCache::default();
    let mut queued_targets = initial_targets.clone();
    if let Some(critical_key) = critical_key {
        let boot_mesh = build_chunk_mesh_for_lod(&spawn_context.build_context, critical_key)
            .expect("critical boot chunk should build");
        let boot_scatter_meshes = build_scatter_meshes_for_key(
            &spawn_context.build_context,
            critical_key,
            &detail_mesh_blueprints,
        );
        let cached = chunk_cache.insert(
            critical_key,
            CachedTerrainChunk {
                mesh: meshes.add(boot_mesh),
                scatter_meshes: upload_scatter_meshes(&mut meshes, boot_scatter_meshes),
                last_used_tick: 0,
            },
        );
        spawn_cached_chunk(&mut commands, &spawn_context, critical_key, &cached, true);
        queued_targets.retain(|target| target.key != critical_key);
    }
    let queued_chunk_count = queued_targets.len();
    commands.insert_resource(ChunkVisibilityState {
        active: initial_targets.clone(),
        center: initial_center,
    });
    commands.insert_resource(TerrainStreamingQueue::from_targets(queued_targets));
    commands.insert_resource(chunk_cache);
    commands.insert_resource(detail_materials.clone());
    commands.insert_resource(detail_mesh_blueprints.clone());

    let initial_collision_coords = if *session_mode == SessionMode::Exploration {
        let coords = collision_chunk_coords_for_position(
            &world_map,
            spots.meadow.position,
            collision_config.active_radius,
        );
        collision_proxy.anchor = initial_center;
        collision_proxy.active = coords.clone();
        let existing_set: HashSet<WorldChunkCoord> =
            collision_proxy.chunks.keys().copied().collect();
        collision_proxy.pending.clear();
        collision_proxy.enqueue_missing(coords.iter().copied(), &existing_set);
        for coord in &coords {
            if collision_scheduler
                .sender
                .send(TerrainCollisionRequest {
                    coord: *coord,
                    world_map: world_map.clone(),
                    subdivisions: collision_config.subdivisions,
                })
                .is_ok()
            {
                collision_scheduler.in_flight.insert(*coord);
            }
        }
        coords
    } else {
        collision_proxy.anchor = None;
        collision_proxy.active.clear();
        collision_proxy.pending.clear();
        Vec::new()
    };

    if let Some(anchor) = world_map.chunk_coord_at(spots.meadow.position.x, spots.meadow.position.z)
        && let Some(mesh) =
            build_terrain_impostor_mesh(&world_map, anchor, lod_config.low_radius, *impostor_config)
    {
        let mesh_handle = meshes.add(mesh);
        let entity = commands
            .spawn((
                Name::new("TerrainImpostor"),
                DespawnOnExit(AppScreen::InGame),
                Mesh3d(mesh_handle.clone()),
                MeshMaterial3d(impostor_material.handle.clone()),
                Transform::default(),
                TerrainImpostorEntity,
            ))
            .id();
        impostor_state.anchor = Some(anchor);
        impostor_state.mesh = Some(mesh_handle);
        impostor_state.entity = Some(entity);
    }

    commands.spawn((
        Name::new("WaterPlane"),
        DespawnOnExit(AppScreen::InGame),
        Mesh3d(meshes.add(Mesh::from(Cylinder::new(world_map.extent() * 1.42, 0.03)))),
        MeshMaterial3d(water_material),
        Transform::from_xyz(0.0, world_map.water_level(), 0.0),
    ));

    spawn_showcase_markers(&mut commands, &mut meshes, &mut materials, &spots);

    commands.spawn((
        Name::new("WandererPrototype"),
        DespawnOnExit(AppScreen::InGame),
        Mesh3d(meshes.add(Mesh::from(Capsule3d::new(0.4, 1.3)))),
        MeshMaterial3d(materials.add(StandardMaterial {
            base_color: Color::srgb(0.82, 0.72, 0.6),
            ..Default::default()
        })),
        Transform::from_translation(spots.meadow.position + Vec3::Y * 1.2),
        WandererPrototype,
    ));

    tracing::info!(
        target: "dao_game::world::generation",
        seed = seed.0,
        extent = world_map.extent(),
        initial_chunk_count = initial_targets.len(),
        queued_chunk_count = queued_chunk_count,
        initial_impostor_anchor = ?impostor_state.anchor,
        initial_collision_chunk_count = initial_collision_coords.len(),
        generation_ms = started_at.elapsed().as_secs_f32() * 1000.0,
        "lod streaming terrain bootstrapped"
    );
}

fn update_visible_chunks(
    mut commands: Commands,
    config: (Res<WorldMap>, Res<TerrainLodConfig>),
    mut performance: ResMut<FramePerformance>,
    mut visibility_state: ResMut<ChunkVisibilityState>,
    mut queue: ResMut<TerrainStreamingQueue>,
    queries: ChunkVisibilityQueries<'_, '_>,
) {
    let started_at = Instant::now();
    let (world_map, lod_config) = config;
    let (chunk_query, detail_query, camera_query) = queries;
    let Some(camera_transform) = camera_query.iter().next() else {
        return;
    };
    let Some(center) = world_map.chunk_coord_at(
        camera_transform.translation.x,
        camera_transform.translation.z,
    ) else {
        return;
    };
    if visibility_state.center == Some(center) {
        return;
    }

    let targets = chunk_targets_for_center(&world_map, center, *lod_config);
    let visible_set: HashSet<TerrainChunkKey> = targets.iter().map(|target| target.key).collect();
    let desired_visible_by_coord: HashMap<WorldChunkCoord, TerrainChunkKey> = targets
        .iter()
        .filter(|target| target.visible)
        .map(|target| (target.key.coord, target.key))
        .collect();
    let spawned_set: HashSet<TerrainChunkKey> =
        chunk_query.iter().map(|(_, chunk)| chunk.key).collect();

    for (entity, chunk_component) in &chunk_query {
        if !should_keep_chunk_key(
            chunk_component.key,
            &visible_set,
            &desired_visible_by_coord,
            &spawned_set,
        ) {
            commands.entity(entity).despawn();
        }
    }

    for (entity, detail_component) in &detail_query {
        if !should_keep_chunk_key(
            detail_component.key,
            &visible_set,
            &desired_visible_by_coord,
            &spawned_set,
        ) {
            commands.entity(entity).despawn();
        }
    }

    let existing_set = spawned_set;
    queue.retain_targets(&visible_set);
    queue.enqueue_missing(targets.iter().copied(), &existing_set);

    visibility_state.active = targets;
    visibility_state.center = Some(center);
    performance.record_phase_duration(PerformancePhase::WorldVisibility, started_at.elapsed());
}

fn stream_terrain_chunks(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    stream_state: TerrainStreamState<'_>,
    resources: TerrainStreamResources<'_>,
    visibility_state: Res<ChunkVisibilityState>,
    existing_chunks: Query<(Entity, &TerrainChunkEntity)>,
    existing_details: Query<(Entity, &TerrainDetailEntity)>,
) {
    let (mut queue, mut cache, mut scheduler) = stream_state;
    let (
        config,
        mut performance,
        _session_mode,
        world_map,
        terrain_material,
        detail_materials,
        detail_mesh_blueprints,
        streaming_config,
    ) = resources;
    let mut existing_set: HashSet<TerrainChunkKey> =
        existing_chunks.iter().map(|(_, chunk)| chunk.key).collect();
    let spawn_context = TerrainChunkSpawnContext {
        material: &terrain_material.handle,
        detail_materials: &detail_materials,
        build_context: TerrainChunkBuildContext {
            world_map: &world_map,
            seed: world_map.seed,
        },
    };
    let started_at = Instant::now();
    let frame_budget_ms = config.quality.frame_time_budget_ms.max(1.0);
    let load_ratio = frame_load_ratio(&performance, frame_budget_ms);
    let mut integrated = 0_usize;
    let integrate_budget =
        adaptive_budget(streaming_config.integrate_budget_per_frame, 1, load_ratio);

    while integrated < integrate_budget {
        let result = {
            let receiver = scheduler
                .receiver
                .lock()
                .expect("terrain generation receiver should lock");
            match receiver.try_recv() {
                Ok(result) => result,
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        };
        scheduler.in_flight.remove(&result.key);
        if !visibility_state
            .active
            .iter()
            .any(|target| target.key == result.key)
        {
            continue;
        }
        if existing_set.contains(&result.key) {
            continue;
        }
        let cached = cache.insert(
            result.key,
            CachedTerrainChunk {
                mesh: meshes.add(result.mesh),
                scatter_meshes: upload_scatter_meshes(&mut meshes, result.scatter_meshes),
                last_used_tick: 0,
            },
        );
        spawn_cached_chunk(
            &mut commands,
            &spawn_context,
            result.key,
            &cached,
            result.key.lod != TerrainLodLevel::Preload,
        );
        despawn_obsolete_lod_entities(
            &mut commands,
            result.key,
            &visibility_state.active,
            &existing_chunks,
            &existing_details,
        );
        existing_set.insert(result.key);
        integrated += 1;
    }

    let mut scheduled = 0_usize;
    let background_budget =
        adaptive_budget(streaming_config.background_budget_per_frame, 1, load_ratio);
    let max_inflight = streaming_inflight_cap(background_budget);
    while scheduled < background_budget && scheduler.in_flight.len() < max_inflight {
        let Some(target) = queue.pop_next() else {
            break;
        };
        if existing_set.contains(&target.key) || scheduler.in_flight.contains(&target.key) {
            continue;
        }
        if cache.get(target.key).is_some() {
            if target.visible {
                let cached = cache
                    .get(target.key)
                    .expect("cached target should remain available");
                spawn_cached_chunk(&mut commands, &spawn_context, target.key, &cached, true);
                despawn_obsolete_lod_entities(
                    &mut commands,
                    target.key,
                    &visibility_state.active,
                    &existing_chunks,
                    &existing_details,
                );
                existing_set.insert(target.key);
                integrated += 1;
            }
            continue;
        }
        if scheduler
            .sender
            .send(TerrainGenerationRequest {
                key: target.key,
                world_map: world_map.clone(),
                detail_mesh_blueprints: detail_mesh_blueprints.clone(),
            })
            .is_ok()
        {
            scheduler.in_flight.insert(target.key);
            scheduled += 1;
        }
    }

    let active_set: HashSet<TerrainChunkKey> = visibility_state
        .active
        .iter()
        .map(|target| target.key)
        .collect();
    let evicted = cache.evict_inactive(streaming_config.cache_capacity, &active_set, &mut meshes);

    if integrated > 0 || scheduled > 0 {
        tracing::debug!(
            target: "dao_game::world::streaming",
            integrated_chunks = integrated,
            scheduled_chunks = scheduled,
            pending_targets = queue.len(),
            inflight_chunks = scheduler.in_flight.len(),
            cached_chunks = cache.len(),
            evicted_chunks = evicted,
            generation_ms = started_at.elapsed().as_secs_f32() * 1000.0,
            "terrain lod stream advanced"
        );
    }
    performance.record_phase_duration(PerformancePhase::WorldStreaming, started_at.elapsed());
}

fn update_terrain_impostor(
    config: Res<AppConfig>,
    mut performance: ResMut<FramePerformance>,
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    resources: TerrainImpostorResources<'_>,
    camera_query: Query<&Transform, With<WorldCamera>>,
) {
    let phase_started_at = Instant::now();
    let (world_map, impostor_material, impostor_config, lod_config, mut impostor_state) = resources;
    let Some(camera_transform) = camera_query.iter().next() else {
        return;
    };
    let Some(anchor) = world_map.chunk_coord_at(
        camera_transform.translation.x,
        camera_transform.translation.z,
    ) else {
        return;
    };
    let frame_budget_ms = config.quality.frame_time_budget_ms.max(1.0);
    let refresh_distance = if frame_load_ratio(&performance, frame_budget_ms) > 2.0 {
        2
    } else {
        1
    };
    if !impostor_state.should_refresh(anchor, refresh_distance) {
        return;
    }

    let started_at = Instant::now();
    let Some(mesh) =
        build_terrain_impostor_mesh(&world_map, anchor, lod_config.low_radius, *impostor_config)
    else {
        return;
    };
    let mesh_handle = meshes.add(mesh);
    let entity = commands
        .spawn((
            Name::new("TerrainImpostor"),
            DespawnOnExit(AppScreen::InGame),
            Mesh3d(mesh_handle.clone()),
            MeshMaterial3d(impostor_material.handle.clone()),
            Transform::default(),
            TerrainImpostorEntity,
        ))
        .id();
    if let Some(previous_entity) = impostor_state.entity.replace(entity) {
        commands.entity(previous_entity).despawn();
    }
    if let Some(previous_mesh) = impostor_state.mesh.replace(mesh_handle) {
        meshes.remove(previous_mesh.id());
    }
    impostor_state.anchor = Some(anchor);

    tracing::debug!(
        target: "dao_game::world::impostor",
        anchor_x = anchor.x,
        anchor_z = anchor.z,
        generation_ms = started_at.elapsed().as_secs_f32() * 1000.0,
        "terrain impostor refreshed"
    );
    performance.record_phase_duration(PerformancePhase::WorldImpostor, phase_started_at.elapsed());
}

fn update_collision_proxy(
    config: Res<AppConfig>,
    mut performance: ResMut<FramePerformance>,
    world_map: Res<WorldMap>,
    collision_config: Res<TerrainCollisionConfig>,
    mut collision_proxy: ResMut<TerrainCollisionProxy>,
    mut collision_scheduler: ResMut<TerrainCollisionScheduler>,
    anchors: Query<&Transform, With<WandererPrototype>>,
) {
    let phase_started_at = Instant::now();
    let Some(anchor_transform) = anchors.iter().next() else {
        return;
    };
    let Some(anchor) = world_map.chunk_coord_at(
        anchor_transform.translation.x,
        anchor_transform.translation.z,
    ) else {
        return;
    };

    if collision_proxy.anchor != Some(anchor) {
        let targets = collision_chunk_coords_for_position(
            &world_map,
            anchor_transform.translation,
            collision_config.active_radius,
        );
        let active_set: HashSet<WorldChunkCoord> = targets.iter().copied().collect();
        let existing_set: HashSet<WorldChunkCoord> =
            collision_proxy.chunks.keys().copied().collect();
        collision_proxy.retain_active(&active_set);
        collision_proxy.enqueue_missing(targets.iter().copied(), &existing_set);
        collision_proxy.anchor = Some(anchor);
        collision_proxy.active = targets;
    }

    let started_at = Instant::now();
    let frame_budget_ms = config.quality.frame_time_budget_ms.max(1.0);
    let integrate_budget = adaptive_budget(
        collision_config.integrate_budget_per_frame,
        1,
        frame_load_ratio(&performance, frame_budget_ms),
    );
    let build_budget = adaptive_budget(
        collision_config.build_budget_per_frame,
        1,
        frame_load_ratio(&performance, frame_budget_ms),
    );
    let mut integrated = 0_usize;
    while integrated < integrate_budget {
        let result = {
            let receiver = collision_scheduler
                .receiver
                .lock()
                .expect("terrain collision receiver should lock");
            match receiver.try_recv() {
                Ok(result) => result,
                Err(TryRecvError::Empty | TryRecvError::Disconnected) => break,
            }
        };
        collision_scheduler.in_flight.remove(&result.coord);
        if !collision_proxy.active.contains(&result.coord) {
            continue;
        }
        collision_proxy.insert_chunk(result.coord, result.chunk);
        integrated += 1;
    }

    let mut scheduled = 0_usize;
    while scheduled < build_budget {
        let Some(coord) = collision_proxy.pop_next() else {
            break;
        };
        if collision_proxy.chunks.contains_key(&coord)
            || collision_scheduler.in_flight.contains(&coord)
        {
            continue;
        }
        if collision_scheduler
            .sender
            .send(TerrainCollisionRequest {
                coord,
                world_map: world_map.clone(),
                subdivisions: collision_config.subdivisions,
            })
            .is_ok()
        {
            collision_scheduler.in_flight.insert(coord);
            scheduled += 1;
        }
    }

    let active_set: HashSet<WorldChunkCoord> = collision_proxy.active.iter().copied().collect();
    let evicted = collision_proxy.evict_inactive(collision_config.cache_capacity, &active_set);

    if integrated > 0 || scheduled > 0 || evicted > 0 {
        tracing::debug!(
            target: "dao_game::world::collision",
            integrated_chunks = integrated,
            scheduled_chunks = scheduled,
            pending_chunks = collision_proxy.pending.len(),
            inflight_chunks = collision_scheduler.in_flight.len(),
            cached_chunks = collision_proxy.len(),
            evicted_chunks = evicted,
            generation_ms = started_at.elapsed().as_secs_f32() * 1000.0,
            "terrain collision proxy advanced"
        );
    }
    performance.record_phase_duration(PerformancePhase::WorldCollision, phase_started_at.elapsed());
}

fn advance_world_cycle(
    time: Res<Time>,
    config: Res<AppConfig>,
    control: Option<Res<WorldPresentationControl>>,
    mut cycle: ResMut<WorldCycle>,
) {
    if let Some(time_override) = control.and_then(|control| control.time_override) {
        cycle.normalized_time = time_override.rem_euclid(1.0);
    } else {
        let cycle_length = config.environment.day_length_seconds.max(1.0);
        cycle.normalized_time =
            (cycle.normalized_time + time.delta_secs() / cycle_length).rem_euclid(1.0);
    }
    let sun_height = (cycle.normalized_time * std::f32::consts::TAU).sin();
    cycle.daylight = (sun_height * 0.5 + 0.5).clamp(0.0, 1.0);
}

fn animate_wanderer(
    time: Res<Time>,
    config: Res<AppConfig>,
    control: Option<Res<WorldPresentationControl>>,
    first_person: Option<Res<FirstPersonState>>,
    resources: (Res<WorldMap>, Res<WorldShowcaseSpots>),
    mut query: Query<&mut Transform, With<WandererPrototype>>,
) {
    let (world_map, spots) = resources;
    let Some(mut transform) = query.iter_mut().next() else {
        return;
    };

    if first_person.is_some() {
        return;
    }

    if let Some(control) = control
        .as_deref()
        .filter(|control| control.wander_target.is_some())
    {
        animate_controlled_wanderer(
            time.delta_secs(),
            config.environment.wander_speed,
            control,
            &world_map,
            &mut transform,
        );
        return;
    }

    let t = time.elapsed_secs() * config.environment.wander_speed.max(0.05);
    let orbit_center = (spots.grove.position + spots.meadow.position) * 0.5;
    let radius = config
        .environment
        .wander_radius
        .min(world_map.extent() * 0.55);
    let x = orbit_center.x + t.cos() * radius * 0.8 + (t * 0.28).sin() * radius * 0.22;
    let z = orbit_center.z + (t * 0.63).sin() * radius * 0.92;
    let next_x = orbit_center.x
        + (t + 0.25).cos() * radius * 0.8
        + ((t + 0.25) * 0.28).sin() * radius * 0.22;
    let next_z = orbit_center.z + ((t + 0.25) * 0.63).sin() * radius * 0.92;

    let Some(height) = world_map.sample_height(x, z) else {
        return;
    };
    let next_height = world_map.sample_height(next_x, next_z).unwrap_or(height);

    let target_position = Vec3::new(x, height + 1.2, z);
    let next_position = Vec3::new(next_x, next_height + 1.2, next_z);
    let smoothing = 1.0 - (-4.5 * time.delta_secs()).exp();
    transform.translation = transform.translation.lerp(target_position, smoothing);
    transform.look_at(next_position, Vec3::Y);
}

fn animate_controlled_wanderer(
    delta_secs: f32,
    base_speed: f32,
    control: &WorldPresentationControl,
    world_map: &WorldMap,
    transform: &mut Transform,
) {
    let Some(mut target_position) = control.wander_target else {
        return;
    };
    let Some(height) = world_map.sample_height(target_position.x, target_position.z) else {
        return;
    };
    target_position.y = height + 1.2;

    let direction = target_position - transform.translation;
    let distance = direction.length();
    if distance > 0.01 {
        let step = (base_speed * 4.7 * control.wander_speed_multiplier.max(0.1) * delta_secs)
            .min(distance);
        let movement = direction.normalize() * step;
        transform.translation += movement;
        transform.look_at(target_position + Vec3::new(0.0, 0.0, 0.2), Vec3::Y);
    } else {
        transform.translation = transform.translation.lerp(target_position, 0.18);
    }
}

fn sample_terrain(world_x: f32, world_z: f32, seed: u64, config: &WorldConfig) -> TerrainSample {
    let scale = config.terrain_scale.max(0.001);
    let xf = world_x / scale;
    let zf = world_z / scale;
    let seed_phase = (seed % 997) as f32 * 0.0017;

    let mut amplitude = 1.0_f32;
    let mut frequency = 0.55_f32;
    let mut height_accum = 0.0_f32;
    let mut moisture_accum = 0.0_f32;
    let mut temperature_accum = 0.0_f32;
    let mut amplitude_sum = 0.0_f32;

    for octave in 0..config.noise_octaves.max(1) {
        let octave_phase = seed_phase + octave as f32 * 0.73;
        let wave_a = ((xf * frequency + octave_phase).sin()
            + (zf * frequency * 1.11 - octave_phase).cos())
            * 0.5;
        let wave_b = ((xf * frequency * 0.63 - octave_phase * 0.4).cos()
            + (zf * frequency * 1.47 + octave_phase * 0.6).sin())
            * 0.5;
        let ridge = 1.0 - (wave_a.abs() * 0.74 + wave_b.abs() * 0.26).clamp(0.0, 1.0);
        let ridge = ridge.powf(config.ridge_sharpness.max(0.2));

        height_accum += (wave_a * 0.55 + wave_b * 0.25 + ridge * 0.45 - 0.15) * amplitude;
        moisture_accum += (((xf * frequency * 0.72 + octave_phase).cos()
            + (zf * frequency * 0.9 - octave_phase).sin())
            * 0.5
            + 0.5)
            * amplitude;
        temperature_accum += (((xf * frequency * 0.35 - octave_phase * 0.3).sin()
            - (zf * frequency * 0.28 + octave_phase * 0.7).cos())
            * 0.5
            + 0.5)
            * amplitude;

        amplitude_sum += amplitude;
        amplitude *= 0.5;
        frequency *= 1.92;
    }

    let normalized_height = height_accum / amplitude_sum.max(0.001);
    let moisture = (moisture_accum / amplitude_sum.max(0.001)).clamp(0.0, 1.0);
    let temperature = (temperature_accum / amplitude_sum.max(0.001)).clamp(0.0, 1.0);
    let basin =
        ((xf * 0.22 + seed_phase * 1.8).cos() + (zf * 0.27 - seed_phase * 1.2).sin()) * 0.45;
    let river_base = ((xf * config.river_frequency * 1.4 + seed_phase * 1.7).sin()
        - (zf * config.river_frequency * 1.1 - seed_phase * 0.8).cos())
    .abs();
    let river_mask = (1.0 - (river_base * 1.75).clamp(0.0, 1.0)).powf(4.0);
    let canyon_mask = ((xf * 0.11 + zf * 0.08 + seed_phase).sin() * 0.5 + 0.5).powf(2.4);
    let river = (river_mask * canyon_mask).clamp(0.0, 1.0);
    let erosion_noise = (((xf * 0.18).sin() * (zf * 0.16).cos()) * 0.5 + 0.5).clamp(0.0, 1.0);
    let erosion = (erosion_noise * config.erosion_strength + river * 0.65).clamp(0.0, 1.0);
    let sediment =
        ((river * 0.58 + moisture * 0.24 + (1.0 - slope_hint(normalized_height)) * 0.18)
            * config.sediment_bias.max(0.05))
        .clamp(0.0, 1.0);
    let river_cut = river * config.river_depth * (0.35 + erosion * 0.65);
    let height = normalized_height * config.height_variation + basin + (erosion_noise - 0.5) * 0.9
        - river_cut
        - erosion * 0.28
        + sediment * 0.14
        + 1.4;

    TerrainSample {
        height,
        moisture,
        temperature,
        erosion,
        river,
        sediment,
    }
}

fn determine_biome(
    sample: TerrainSample,
    slope: f32,
    water_level: f32,
    shoreline_blend: f32,
) -> BiomeKind {
    if sample.height <= water_level + shoreline_blend {
        BiomeKind::Water
    } else if sample.river > 0.38 && sample.moisture > 0.5 && slope < 0.95 {
        BiomeKind::Grove
    } else if slope > 0.92 || sample.height > water_level + 4.6 {
        BiomeKind::Ridge
    } else if sample.moisture > 0.68 && sample.temperature < 0.72 {
        BiomeKind::Grove
    } else if sample.moisture > 0.42 || sample.sediment > 0.18 {
        BiomeKind::Meadow
    } else {
        BiomeKind::Steppe
    }
}

fn build_chunk_mesh_for_lod(
    context: &TerrainChunkBuildContext<'_>,
    key: TerrainChunkKey,
) -> Option<Mesh> {
    context
        .world_map
        .build_terrain_mesh_for_chunk_with_subdivisions(
            key.coord,
            key.lod.subdivisions(context.world_map.subdivisions()),
        )
}

fn build_collision_chunk(
    world_map: &WorldMap,
    coord: WorldChunkCoord,
    subdivisions: u32,
) -> Option<TerrainCollisionChunk> {
    let (tile_x_min, tile_x_max, tile_z_min, tile_z_max) =
        world_map.chunk_mesh_tile_bounds(coord)?;
    let subdivisions = subdivisions.max(1);
    let x_steps = ((tile_x_max - tile_x_min) as u32 * subdivisions) as usize;
    let z_steps = ((tile_z_max - tile_z_min) as u32 * subdivisions) as usize;
    let stride = x_steps + 1;
    let depth = z_steps + 1;
    if stride < 2 || depth < 2 {
        return None;
    }

    let sample_step = (world_map.cell_size() / subdivisions as f32).max(0.001);
    let mut heights = Vec::with_capacity(stride * depth);
    for z_step in 0..=z_steps {
        for x_step in 0..=x_steps {
            let world_x =
                (tile_x_min as f32 + x_step as f32 / subdivisions as f32) * world_map.cell_size();
            let world_z =
                (tile_z_min as f32 + z_step as f32 / subdivisions as f32) * world_map.cell_size();
            if !world_map.within_world_bounds(world_x, world_z) {
                return None;
            }
            heights
                .push(sample_terrain(world_x, world_z, world_map.seed, &world_map.terrain).height);
        }
    }

    Some(TerrainCollisionChunk {
        origin: Vec2::new(
            tile_x_min as f32 * world_map.cell_size(),
            tile_z_min as f32 * world_map.cell_size(),
        ),
        sample_step,
        stride,
        depth,
        heights,
    })
}

fn build_terrain_impostor_mesh(
    world_map: &WorldMap,
    anchor: WorldChunkCoord,
    low_detail_radius: i32,
    config: TerrainImpostorConfig,
) -> Option<Mesh> {
    if config.active_radius <= low_detail_radius {
        return None;
    }

    let chunk_span = world_map.chunk_world_span();
    let inner_radius = (low_detail_radius.max(0) as f32 + 0.35) * chunk_span;
    let outer_radius = (config.active_radius as f32 + 0.85) * chunk_span;
    if outer_radius <= inner_radius + 0.01 {
        return None;
    }

    let radial_bands = config.radial_bands.max(1) as usize;
    let angular_segments = config.angular_segments.max(16) as usize;
    let center = chunk_world_center(world_map, anchor);
    let vertex_count = (radial_bands + 1) * (angular_segments + 1);
    let uv_scale = world_map.chunk_world_span().max(0.001);

    let mut positions = Vec::with_capacity(vertex_count);
    let mut colors = Vec::with_capacity(vertex_count);
    let mut uvs = Vec::with_capacity(vertex_count);
    for radial_index in 0..=radial_bands {
        let radial_t = radial_index as f32 / radial_bands as f32;
        let radius = inner_radius + (outer_radius - inner_radius) * radial_t;
        for angular_index in 0..=angular_segments {
            let angular_t = angular_index as f32 / angular_segments as f32;
            let angle = angular_t * std::f32::consts::TAU;
            let direction = Vec2::new(angle.cos(), angle.sin());
            let world_x = center.x + direction.x * radius;
            let world_z = center.y + direction.y * radius;
            let clamped_x = world_x.clamp(-world_map.extent(), world_map.extent());
            let clamped_z = world_z.clamp(-world_map.extent(), world_map.extent());
            let source = world_map
                .sample_vertex(clamped_x, clamped_z)
                .expect("clamped impostor sample should exist");
            let vertex = TerrainVertexSample {
                world_x,
                world_z,
                height: source.height,
                moisture: source.moisture,
                temperature: source.temperature,
                slope: source.slope,
                river: source.river,
                erosion: source.erosion,
                sediment: source.sediment,
                biome: source.biome,
            };
            positions.push([vertex.world_x, vertex.height, vertex.world_z]);
            colors.push(
                terrain_texture_color(&vertex, world_map.water_level())
                    .to_linear()
                    .to_f32_array(),
            );
            uvs.push([clamped_x / uv_scale, clamped_z / uv_scale]);
        }
    }

    let mut indices = Vec::with_capacity(radial_bands * angular_segments * 6);
    let stride = angular_segments + 1;
    for radial_index in 0..radial_bands {
        for angular_index in 0..angular_segments {
            let top_left = (radial_index * stride + angular_index) as u32;
            let top_right = top_left + 1;
            let bottom_left = ((radial_index + 1) * stride + angular_index) as u32;
            let bottom_right = bottom_left + 1;
            indices.extend_from_slice(&[
                top_left,
                bottom_left,
                top_right,
                top_right,
                bottom_left,
                bottom_right,
            ]);
        }
    }

    let mut normals = vec![[0.0, 0.0, 0.0]; positions.len()];
    accumulate_normals(&positions, &indices, &mut normals);

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_COLOR, colors)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    mesh.insert_indices(Indices::U32(indices));
    Some(mesh)
}

fn chunk_world_center(world_map: &WorldMap, coord: WorldChunkCoord) -> Vec2 {
    let span = world_map.chunk_world_span();
    Vec2::new(
        coord.x as f32 * span + span * 0.5,
        coord.z as f32 * span + span * 0.5,
    )
}

fn vertex_color(sample: &TerrainVertexSample, water_level: f32) -> Color {
    if sample.height <= water_level + 0.05 {
        return Color::srgb(0.29, 0.31, 0.26);
    }

    let base = match sample.biome {
        BiomeKind::Water => Color::srgb(0.37, 0.34, 0.27),
        BiomeKind::Meadow => Color::srgb(0.42, 0.48, 0.26),
        BiomeKind::Grove => Color::srgb(0.18, 0.36, 0.19),
        BiomeKind::Steppe => Color::srgb(0.58, 0.46, 0.28),
        BiomeKind::Ridge => Color::srgb(0.45, 0.43, 0.42),
    };

    let dryness = (1.0 - sample.moisture).clamp(0.0, 1.0);
    let height_tint = ((sample.height - water_level) / 5.0).clamp(0.0, 1.0);
    let slope_shadow = (sample.slope / 1.2).clamp(0.0, 1.0);
    let river_tint = sample.river.clamp(0.0, 1.0);
    let erosion_tint = sample.erosion.clamp(0.0, 1.0);
    let [r, g, b, _] = base.to_linear().to_f32_array();
    Color::linear_rgba(
        (r + height_tint * 0.08 - slope_shadow * 0.06 + sample.sediment * 0.09).clamp(0.0, 1.0),
        (g - dryness * 0.1 + height_tint * 0.05 + river_tint * 0.04 - erosion_tint * 0.04)
            .clamp(0.0, 1.0),
        (b - dryness * 0.06 + height_tint * 0.03 + river_tint * 0.09).clamp(0.0, 1.0),
        1.0,
    )
}

fn normal_from_neighbor_heights(left: f32, right: f32, down: f32, up: f32, step: f32) -> Vec3 {
    let normal = Vec3::new(left - right, step * 2.0, down - up).normalize_or_zero();
    if normal.y > 0.0 { normal } else { Vec3::Y }
}

fn accumulate_normals(positions: &[[f32; 3]], indices: &[u32], normals: &mut [[f32; 3]]) {
    for triangle in indices.chunks_exact(3) {
        let a = Vec3::from_array(positions[triangle[0] as usize]);
        let b = Vec3::from_array(positions[triangle[1] as usize]);
        let c = Vec3::from_array(positions[triangle[2] as usize]);
        let face_normal = (b - a).cross(c - a).normalize_or_zero();

        for index in triangle {
            let normal = &mut normals[*index as usize];
            let accumulated = Vec3::new(normal[0], normal[1], normal[2]) + face_normal;
            *normal = accumulated.to_array();
        }
    }

    for normal in normals.iter_mut() {
        let normalized = Vec3::new(normal[0], normal[1], normal[2]).normalize_or_zero();
        *normal = normalized.to_array();
    }
}

fn collect_scatter_placements_for_key(
    context: &TerrainChunkBuildContext<'_>,
    key: TerrainChunkKey,
) -> Vec<ScatterPlacement> {
    if !key.lod.scatter_enabled() {
        return Vec::new();
    }
    let Some((tile_x_min, tile_x_mesh_max, tile_z_min, tile_z_mesh_max)) =
        context.world_map.chunk_mesh_tile_bounds(key.coord)
    else {
        return Vec::new();
    };
    let tile_x_max = (tile_x_mesh_max - 1).max(tile_x_min);
    let tile_z_max = (tile_z_mesh_max - 1).max(tile_z_min);
    let mut placements = Vec::new();

    for tile_z in tile_z_min..=tile_z_max {
        for tile_x in tile_x_min..=tile_x_max {
            let Some(tile) = context.world_map.tile_at_grid(tile_x, tile_z) else {
                continue;
            };
            if tile.biome() == BiomeKind::Water {
                continue;
            }

            let detail_factor = scatter_noise(context.seed, tile_x, tile_z, 37);
            let should_spawn = match tile.biome() {
                BiomeKind::Meadow => detail_factor > (0.34 - tile.river() * 0.08),
                BiomeKind::Grove => detail_factor > 0.18,
                BiomeKind::Steppe => detail_factor > (0.52 + tile.erosion() * 0.08),
                BiomeKind::Ridge => detail_factor > (0.4 - tile.erosion() * 0.06),
                BiomeKind::Water => false,
            };
            if !should_spawn {
                continue;
            }

            let offset = scatter_offset(
                context.seed,
                tile_x,
                tile_z,
                context.world_map.cell_size() * 0.36,
            );
            let world_x = tile_x as f32 * context.world_map.cell_size() + offset.x;
            let world_z = tile_z as f32 * context.world_map.cell_size() + offset.y;
            let Some(height) = context.world_map.sample_height(world_x, world_z) else {
                continue;
            };
            let placement = ScatterPlacement {
                position: Vec3::new(world_x, height, world_z),
                biome: tile.biome(),
                scale: 0.72
                    + scatter_noise(context.seed, tile_x, tile_z, 89) * 0.8
                    + tile.river() * 0.18
                    - tile.erosion() * 0.1,
            };
            placements.push(placement);
        }
    }
    placements
}

fn detail_mesh_blueprint(mesh: Mesh, vertical_offset: f32) -> DetailMeshBlueprint {
    let positions = match mesh
        .attribute(Mesh::ATTRIBUTE_POSITION)
        .expect("detail mesh positions should exist")
    {
        VertexAttributeValues::Float32x3(values) => values.clone(),
        _ => panic!("detail mesh positions should be float32x3"),
    };
    let normals = mesh
        .attribute(Mesh::ATTRIBUTE_NORMAL)
        .map(|attribute| match attribute {
            VertexAttributeValues::Float32x3(values) => values.clone(),
            _ => panic!("detail mesh normals should be float32x3"),
        });
    let uvs = mesh
        .attribute(Mesh::ATTRIBUTE_UV_0)
        .map(|attribute| match attribute {
            VertexAttributeValues::Float32x2(values) => values.clone(),
            _ => panic!("detail mesh uvs should be float32x2"),
        });
    let indices = match mesh.indices() {
        Some(Indices::U16(values)) => values.iter().map(|index| *index as u32).collect(),
        Some(Indices::U32(values)) => values.clone(),
        None => (0..positions.len() as u32).collect(),
    };

    DetailMeshBlueprint {
        positions,
        normals,
        uvs,
        indices,
        vertical_offset,
    }
}

fn build_scatter_meshes_for_key(
    context: &TerrainChunkBuildContext<'_>,
    key: TerrainChunkKey,
    blueprints: &DetailMeshBlueprints,
) -> HashMap<BiomeKind, Mesh> {
    let placements = collect_scatter_placements_for_key(context, key);
    let mut grouped: HashMap<BiomeKind, Vec<ScatterPlacement>> = HashMap::new();
    for placement in placements {
        grouped.entry(placement.biome).or_default().push(placement);
    }

    let mut meshes = HashMap::new();
    for (biome, placements) in grouped {
        let blueprint = match biome {
            BiomeKind::Meadow => &blueprints.meadow,
            BiomeKind::Grove => &blueprints.grove,
            BiomeKind::Steppe => &blueprints.steppe,
            BiomeKind::Ridge => &blueprints.ridge,
            BiomeKind::Water => continue,
        };
        if let Some(mesh) = build_scatter_mesh_batch(blueprint, &placements) {
            meshes.insert(biome, mesh);
        }
    }
    meshes
}

fn build_scatter_mesh_batch(
    blueprint: &DetailMeshBlueprint,
    placements: &[ScatterPlacement],
) -> Option<Mesh> {
    if placements.is_empty() || blueprint.positions.is_empty() {
        return None;
    }

    let vertex_count = blueprint.positions.len() * placements.len();
    let index_count = blueprint.indices.len() * placements.len();
    let mut positions = Vec::with_capacity(vertex_count);
    let mut normals = blueprint
        .normals
        .as_ref()
        .map(|_| Vec::with_capacity(vertex_count));
    let mut uvs = blueprint
        .uvs
        .as_ref()
        .map(|_| Vec::with_capacity(vertex_count));
    let mut indices = Vec::with_capacity(index_count);

    for placement in placements {
        let transform = scatter_transform(
            placement.position,
            blueprint.vertical_offset,
            placement.scale,
        );
        let vertex_base = positions.len() as u32;
        let matrix = transform.to_matrix();
        let normal_matrix = Mat3::from_mat4(matrix).inverse().transpose();

        for position in &blueprint.positions {
            let transformed = matrix.transform_point3(Vec3::from_array(*position));
            positions.push(transformed.to_array());
        }
        if let Some(source_normals) = &blueprint.normals
            && let Some(target_normals) = normals.as_mut()
        {
            for normal in source_normals {
                let transformed = normal_matrix
                    .mul_vec3(Vec3::from_array(*normal))
                    .normalize_or_zero();
                target_normals.push(transformed.to_array());
            }
        }
        if let Some(source_uvs) = &blueprint.uvs
            && let Some(target_uvs) = uvs.as_mut()
        {
            target_uvs.extend(source_uvs.iter().copied());
        }
        indices.extend(blueprint.indices.iter().map(|index| index + vertex_base));
    }

    let mut mesh = Mesh::new(
        PrimitiveTopology::TriangleList,
        RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions);
    if let Some(normals) = normals {
        mesh.insert_attribute(Mesh::ATTRIBUTE_NORMAL, normals);
    }
    if let Some(uvs) = uvs {
        mesh.insert_attribute(Mesh::ATTRIBUTE_UV_0, uvs);
    }
    mesh.insert_indices(Indices::U32(indices));
    Some(mesh)
}

fn upload_scatter_meshes(
    meshes: &mut Assets<Mesh>,
    scatter_meshes: HashMap<BiomeKind, Mesh>,
) -> HashMap<BiomeKind, Handle<Mesh>> {
    scatter_meshes
        .into_iter()
        .map(|(biome, mesh)| (biome, meshes.add(mesh)))
        .collect()
}

fn spawn_cached_chunk(
    commands: &mut Commands,
    context: &TerrainChunkSpawnContext<'_>,
    key: TerrainChunkKey,
    cached: &CachedTerrainChunk,
    visible: bool,
) {
    if !visible {
        return;
    }
    commands.spawn((
        Name::new(format!(
            "TerrainChunk({:?}, {}, {})",
            key.lod, key.coord.x, key.coord.z
        )),
        DespawnOnExit(AppScreen::InGame),
        Mesh3d(cached.mesh.clone()),
        MeshMaterial3d((*context.material).clone()),
        Transform::default(),
        TerrainChunkEntity { key },
    ));
    for (biome, mesh) in &cached.scatter_meshes {
        let (name, material) = detail_visuals_for_biome(*biome, context.detail_materials);
        commands.spawn((
            Name::new(name),
            DespawnOnExit(AppScreen::InGame),
            Mesh3d(mesh.clone()),
            MeshMaterial3d(material),
            Transform::default(),
            TerrainDetailEntity { key },
        ));
    }
}

fn should_keep_chunk_key(
    existing_key: TerrainChunkKey,
    desired_keys: &HashSet<TerrainChunkKey>,
    desired_visible_by_coord: &HashMap<WorldChunkCoord, TerrainChunkKey>,
    spawned_keys: &HashSet<TerrainChunkKey>,
) -> bool {
    if desired_keys.contains(&existing_key) {
        return true;
    }

    let Some(replacement_key) = desired_visible_by_coord.get(&existing_key.coord).copied() else {
        return false;
    };

    if replacement_key == existing_key {
        return true;
    }

    !spawned_keys.contains(&replacement_key)
}

fn despawn_obsolete_lod_entities(
    commands: &mut Commands,
    spawned_key: TerrainChunkKey,
    active_targets: &[TerrainChunkTarget],
    existing_chunks: &Query<(Entity, &TerrainChunkEntity)>,
    existing_details: &Query<(Entity, &TerrainDetailEntity)>,
) {
    let desired_visible_by_coord: HashMap<WorldChunkCoord, TerrainChunkKey> = active_targets
        .iter()
        .filter(|target| target.visible)
        .map(|target| (target.key.coord, target.key))
        .collect();
    if desired_visible_by_coord.get(&spawned_key.coord).copied() != Some(spawned_key) {
        return;
    }

    for (entity, chunk) in existing_chunks.iter() {
        if chunk.key.coord == spawned_key.coord && chunk.key != spawned_key {
            commands.entity(entity).despawn();
        }
    }

    for (entity, detail) in existing_details.iter() {
        if detail.key.coord == spawned_key.coord && detail.key != spawned_key {
            commands.entity(entity).despawn();
        }
    }
}

fn slope_hint(normalized_height: f32) -> f32 {
    normalized_height.abs().clamp(0.0, 1.0)
}

fn biome_index(biome: BiomeKind) -> usize {
    match biome {
        BiomeKind::Water => 0,
        BiomeKind::Meadow => 1,
        BiomeKind::Grove => 2,
        BiomeKind::Steppe => 3,
        BiomeKind::Ridge => 4,
    }
}

fn biome_from_index(index: usize) -> BiomeKind {
    match index {
        0 => BiomeKind::Water,
        1 => BiomeKind::Meadow,
        2 => BiomeKind::Grove,
        3 => BiomeKind::Steppe,
        _ => BiomeKind::Ridge,
    }
}

fn chunk_targets_for_camera(
    world_map: &WorldMap,
    camera_position: Vec3,
    lod_config: TerrainLodConfig,
) -> Vec<TerrainChunkTarget> {
    let Some(center) = world_map.chunk_coord_at(camera_position.x, camera_position.z) else {
        return Vec::new();
    };
    chunk_targets_for_center(world_map, center, lod_config)
}

fn chunk_targets_for_center(
    world_map: &WorldMap,
    center: WorldChunkCoord,
    lod_config: TerrainLodConfig,
) -> Vec<TerrainChunkTarget> {
    let mut targets = Vec::new();
    for z in (center.z - lod_config.preload_radius)..=(center.z + lod_config.preload_radius) {
        for x in (center.x - lod_config.preload_radius)..=(center.x + lod_config.preload_radius) {
            let coord = WorldChunkCoord { x, z };
            if world_map.chunk_mesh_tile_bounds(coord).is_some() {
                let dx = (coord.x - center.x).abs();
                let dz = (coord.z - center.z).abs();
                let distance = dx.max(dz);
                let lod = if distance <= lod_config.high_radius {
                    TerrainLodLevel::High
                } else if distance <= lod_config.low_radius {
                    TerrainLodLevel::Low
                } else {
                    TerrainLodLevel::Preload
                };
                targets.push(TerrainChunkTarget {
                    key: TerrainChunkKey { coord, lod },
                    visible: lod != TerrainLodLevel::Preload,
                });
            }
        }
    }
    targets.sort_by_key(|target| {
        let dx = target.key.coord.x - center.x;
        let dz = target.key.coord.z - center.z;
        dx * dx + dz * dz
    });
    targets
}

fn frame_load_ratio(performance: &FramePerformance, frame_budget_ms: f32) -> f32 {
    if performance.frame_count() == 0 {
        return 1.0;
    }
    performance
        .last_frame_ms()
        .max(performance.moving_average_ms())
        / frame_budget_ms.max(0.1)
}

fn adaptive_budget(base_budget: usize, minimum_budget: usize, load_ratio: f32) -> usize {
    let base_budget = base_budget.max(minimum_budget);
    if load_ratio > 3.0 {
        minimum_budget
    } else if load_ratio > 1.75 {
        base_budget.div_ceil(2).max(minimum_budget)
    } else {
        base_budget
    }
}

fn streaming_inflight_cap(background_budget: usize) -> usize {
    (background_budget.max(1) * 3).max(6)
}

fn collision_chunk_coords_for_position(
    world_map: &WorldMap,
    position: Vec3,
    radius: i32,
) -> Vec<WorldChunkCoord> {
    let Some(center) = world_map.chunk_coord_at(position.x, position.z) else {
        return Vec::new();
    };

    let mut coords = Vec::new();
    for z in (center.z - radius)..=(center.z + radius) {
        for x in (center.x - radius)..=(center.x + radius) {
            let coord = WorldChunkCoord { x, z };
            if world_map.chunk_mesh_tile_bounds(coord).is_some() {
                coords.push(coord);
            }
        }
    }
    coords.sort_by_key(|coord| {
        let dx = coord.x - center.x;
        let dz = coord.z - center.z;
        dx * dx + dz * dz
    });
    coords
}

fn build_terrain_palette_texture(world_map: &WorldMap, resolution: u32) -> Image {
    let mut data = Vec::with_capacity((resolution * resolution * 4) as usize);
    let size = resolution as f32;

    for py in 0..resolution {
        for px in 0..resolution {
            let world_x =
                (px as f32 / (size - 1.0)) * world_map.extent() * 2.0 - world_map.extent();
            let world_z =
                (py as f32 / (size - 1.0)) * world_map.extent() * 2.0 - world_map.extent();
            let height = world_map
                .sample_height(world_x, world_z)
                .unwrap_or(world_map.water_level());
            let moisture = world_map.sample_moisture(world_x, world_z).unwrap_or(0.5);
            let slope = world_map.sample_slope(world_x, world_z).unwrap_or(0.0);
            let river = world_map.sample_river(world_x, world_z).unwrap_or(0.0);
            let erosion = world_map.sample_erosion(world_x, world_z).unwrap_or(0.0);
            let biome = world_map
                .sample_biome(world_x, world_z)
                .unwrap_or(BiomeKind::Meadow);
            let vertex = TerrainVertexSample {
                world_x,
                world_z,
                height,
                moisture,
                temperature: 0.5,
                slope,
                river,
                erosion,
                sediment: (river * 0.55 + moisture * 0.25).clamp(0.0, 1.0),
                biome,
            };
            let color = terrain_texture_color(&vertex, world_map.water_level());
            let [r, g, b, a] = color.to_srgba().to_u8_array();
            data.extend_from_slice(&[r, g, b, a]);
        }
    }

    Image::new(
        Extent3d {
            width: resolution,
            height: resolution,
            depth_or_array_layers: 1,
        },
        TextureDimension::D2,
        data,
        TextureFormat::Rgba8UnormSrgb,
        RenderAssetUsages::default(),
    )
}

fn terrain_texture_color(sample: &TerrainVertexSample, water_level: f32) -> Color {
    let base = vertex_color(sample, water_level);
    let [r, g, b, _] = base.to_linear().to_f32_array();
    let rocky = (sample.slope * 0.65 + sample.erosion * 0.45).clamp(0.0, 1.0);
    let wet = (sample.river * 0.75 + sample.moisture * 0.25).clamp(0.0, 1.0);
    let sediment = sample.sediment.clamp(0.0, 1.0);
    Color::linear_rgba(
        (r + sediment * 0.08 - rocky * 0.06).clamp(0.0, 1.0),
        (g + wet * 0.04 - rocky * 0.04).clamp(0.0, 1.0),
        (b + wet * 0.06 + rocky * 0.03).clamp(0.0, 1.0),
        1.0,
    )
}

fn detail_visuals_for_biome(
    biome: BiomeKind,
    materials: &DetailMaterials,
) -> (&'static str, Handle<StandardMaterial>) {
    match biome {
        BiomeKind::Meadow => ("MeadowTuftBatch", materials.meadow.clone()),
        BiomeKind::Grove => ("GroveTreeBatch", materials.grove.clone()),
        BiomeKind::Steppe => ("SteppeStoneBatch", materials.steppe.clone()),
        BiomeKind::Ridge => ("RidgeSpireBatch", materials.ridge.clone()),
        BiomeKind::Water => ("WaterDetailBatch", materials.meadow.clone()),
    }
}

fn scatter_transform(position: Vec3, vertical_offset: f32, scale: f32) -> Transform {
    Transform {
        translation: position + Vec3::Y * vertical_offset * scale,
        scale: Vec3::splat(scale),
        ..Default::default()
    }
}

fn spawn_showcase_markers(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    materials: &mut Assets<StandardMaterial>,
    spots: &WorldShowcaseSpots,
) {
    for (name, spot, color) in [
        ("RidgeMarker", spots.ridge, Color::srgb(0.82, 0.8, 0.92)),
        ("GroveMarker", spots.grove, Color::srgb(0.42, 0.82, 0.52)),
        ("WaterMarker", spots.water, Color::srgb(0.42, 0.72, 0.92)),
    ] {
        commands.spawn((
            Name::new(name),
            DespawnOnExit(AppScreen::InGame),
            Mesh3d(meshes.add(Mesh::from(Capsule3d::new(0.12, 0.75)))),
            MeshMaterial3d(materials.add(StandardMaterial {
                base_color: color,
                emissive: color.into(),
                ..Default::default()
            })),
            Transform::from_translation(spot.position + Vec3::Y * 1.2),
        ));
    }
}

fn find_showcase_spot(world_map: &WorldMap, biome: BiomeKind) -> Option<ShowcaseSpot> {
    let mut best: Option<(ShowcaseSpot, f32)> = None;

    let search_radius = world_map.showcase_search_radius();
    for tile_z in -search_radius..=search_radius {
        for tile_x in -search_radius..=search_radius {
            let Some(tile) = world_map.tile_at_grid(tile_x, tile_z) else {
                continue;
            };
            if tile.biome() != biome {
                continue;
            }
            let base_position = world_map.tile_translation(tile_x, tile_z, tile.height());
            let spot = ShowcaseSpot {
                coord: WorldGridCoord {
                    x: tile_x,
                    z: tile_z,
                },
                position: base_position,
                biome,
            };
            let score = match biome {
                BiomeKind::Water => -tile.height() + tile.moisture() * 0.3,
                BiomeKind::Meadow => tile.moisture() - tile.slope() * 0.35,
                BiomeKind::Grove => tile.moisture() + tile.height() * 0.05,
                BiomeKind::Steppe => (1.0 - tile.moisture()) + tile.height() * 0.04,
                BiomeKind::Ridge => tile.height() + tile.slope() * 0.65,
            };
            match best {
                None => best = Some((spot, score)),
                Some((_, current_score)) if score > current_score => best = Some((spot, score)),
                _ => {}
            }
        }
    }

    best.map(|(spot, _)| spot)
}

fn fallback_showcase_spot(world_map: &WorldMap, biome: BiomeKind) -> ShowcaseSpot {
    let center_tile = world_map.tile_at_grid(0, 0).unwrap_or(TerrainTile {
        height: 0.0,
        moisture: 0.5,
        slope: 0.0,
        river: 0.0,
        erosion: 0.0,
        biome,
    });
    ShowcaseSpot {
        coord: WorldGridCoord { x: 0, z: 0 },
        position: world_map.tile_translation(0, 0, center_tile.height()),
        biome,
    }
}

fn cleanup_world_session(mut commands: Commands, mut cycle: ResMut<WorldCycle>) {
    *cycle = WorldCycle::default();
    commands.remove_resource::<WorldMap>();
    commands.remove_resource::<WorldShowcaseSpots>();
    commands.remove_resource::<TerrainRuntimeMaterial>();
    commands.remove_resource::<TerrainImpostorMaterial>();
    commands.remove_resource::<ChunkVisibilityState>();
    commands.remove_resource::<TerrainStreamingQueue>();
    commands.remove_resource::<TerrainLodConfig>();
    commands.remove_resource::<TerrainStreamingConfig>();
    commands.remove_resource::<TerrainImpostorConfig>();
    commands.remove_resource::<TerrainImpostorState>();
    commands.remove_resource::<TerrainCollisionConfig>();
    commands.remove_resource::<TerrainCollisionProxy>();
    commands.remove_resource::<TerrainCollisionScheduler>();
    commands.remove_resource::<TerrainGenerationScheduler>();
    commands.remove_resource::<TerrainChunkCache>();
    commands.remove_resource::<DetailMaterials>();
    commands.remove_resource::<DetailMeshBlueprints>();
}

fn scatter_offset(seed: u64, x: i32, z: i32, radius: f32) -> Vec2 {
    let dx = scatter_noise(seed, x, z, 17) * radius * 2.0 - radius;
    let dz = scatter_noise(seed, x, z, 53) * radius * 2.0 - radius;
    Vec2::new(dx, dz)
}

fn scatter_noise(seed: u64, x: i32, z: i32, salt: u64) -> f32 {
    let mut value = seed
        .wrapping_add((x as i64 as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15))
        .wrapping_add((z as i64 as u64).wrapping_mul(0xBF58_476D_1CE4_E5B9))
        .wrapping_add(salt.wrapping_mul(0x94D0_49BB_1331_11EB));
    value ^= value >> 30;
    value = value.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    value ^= value >> 27;
    value = value.wrapping_mul(0x94D0_49BB_1331_11EB);
    value ^= value >> 31;
    (value as f64 / u64::MAX as f64) as f32
}

#[cfg(test)]
mod tests {
    use std::{
        collections::{HashMap, HashSet},
        path::PathBuf,
    };

    use bevy::{
        math::primitives::Capsule3d,
        prelude::{Assets, Mesh, Vec3},
    };

    use crate::core::config::{
        AppConfig, EnvironmentConfig, PlayerConfig, PresentationConfig, QualityConfig, SignConfig,
        WorldConfig,
    };

    use super::{
        BiomeKind, CachedTerrainChunk, TerrainChunkBuildContext, TerrainChunkCache,
        TerrainChunkKey, TerrainCollisionProxy, TerrainImpostorConfig, TerrainLodConfig,
        TerrainLodLevel, TerrainStreamingQueue, WorldChunkCoord, WorldMap, WorldSeed,
        accumulate_normals, adaptive_budget, build_chunk_mesh_for_lod, build_collision_chunk,
        build_scatter_mesh_batch, build_terrain_impostor_mesh, chunk_targets_for_camera,
        chunk_targets_for_center, collision_chunk_coords_for_position, detail_mesh_blueprint,
        determine_biome, sample_terrain, should_keep_chunk_key, streaming_inflight_cap,
    };

    fn test_config() -> AppConfig {
        AppConfig {
            window_title: "Dao".to_string(),
            log_directory: PathBuf::from("logs"),
            performance_log_name: "performance.log".to_string(),
            frame_log_interval: 60,
            performance_detail_interval: 1,
            presentation: PresentationConfig {
                enabled: true,
                scene_duration_seconds: 7.0,
                camera_blend_speed: 2.0,
            },
            world: WorldConfig {
                seed: 42,
                world_radius: 2,
                chunk_radius: 1,
                cell_size: 2.5,
                terrain_subdivisions: 4,
                terrain_scale: 8.0,
                height_variation: 3.5,
                water_level: -0.1,
                noise_octaves: 4,
                ridge_sharpness: 1.8,
                shoreline_blend: 0.16,
                river_frequency: 0.22,
                river_depth: 0.55,
                erosion_strength: 0.4,
                sediment_bias: 0.22,
                visible_chunk_radius: 1,
                high_detail_chunk_radius: 1,
                low_detail_chunk_radius: 1,
                preload_chunk_radius: 2,
                impostor_chunk_radius: 4,
                impostor_radial_bands: 3,
                impostor_angular_segments: 24,
                showcase_search_radius: 4,
                streaming_chunk_budget: 1,
                background_generation_budget: 2,
                streaming_cache_capacity: 16,
                collision_proxy_radius: 1,
                collision_subdivisions: 6,
                collision_chunk_budget: 1,
                collision_cache_capacity: 12,
                material_texture_resolution: 64,
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

    #[test]
    fn terrain_sampling_is_deterministic() {
        let config = test_config();
        let a = sample_terrain(2.0, -1.0, 42, &config.world);
        let b = sample_terrain(2.0, -1.0, 42, &config.world);

        assert_eq!(a, b);
    }

    #[test]
    fn determine_biome_marks_water_and_ridge() {
        let water = determine_biome(
            super::TerrainSample {
                height: -0.2,
                moisture: 0.6,
                temperature: 0.4,
                erosion: 0.2,
                river: 0.5,
                sediment: 0.2,
            },
            0.1,
            -0.1,
            0.16,
        );
        let ridge = determine_biome(
            super::TerrainSample {
                height: 4.9,
                moisture: 0.2,
                temperature: 0.3,
                erosion: 0.1,
                river: 0.0,
                sediment: 0.0,
            },
            1.1,
            -0.1,
            0.16,
        );

        assert_eq!(water, BiomeKind::Water);
        assert_eq!(ridge, BiomeKind::Ridge);
    }

    #[test]
    fn world_map_can_sample_center_tile() {
        let config = test_config();
        let world_map = WorldMap::new(42, &config);
        let center = world_map
            .sample_world_position(Vec3::new(0.1, 0.0, -0.2))
            .expect("center tile should exist");

        assert!(center.height() > -1.0);
    }

    #[test]
    fn world_map_builds_streamed_chunk_mesh_resolution() {
        let config = test_config();
        let world_map = WorldMap::new(42, &config);
        let mesh = build_chunk_mesh_for_lod(
            &TerrainChunkBuildContext {
                world_map: &world_map,
                seed: 42,
            },
            TerrainChunkKey {
                coord: WorldChunkCoord { x: 0, z: 0 },
                lod: TerrainLodLevel::High,
            },
        )
        .expect("origin chunk mesh should exist");
        let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION).expect("positions");
        let edge =
            config.world.chunk_radius as usize * config.world.terrain_subdivisions as usize + 1;
        assert_eq!(positions.len(), edge * edge);
    }

    #[test]
    fn world_map_samples_far_streaming_position_without_prebuilt_tiles() {
        let mut config = test_config();
        config.world.world_radius = 64;
        config.world.showcase_search_radius = 8;
        let world_map = WorldMap::new(42, &config);

        assert!(
            world_map
                .sample_world_position(Vec3::new(120.0, 0.0, -90.0))
                .is_some()
        );
        assert!(
            world_map
                .sample_world_position(Vec3::new(world_map.extent() + 1.0, 0.0, 0.0))
                .is_none()
        );
    }

    #[test]
    fn chunk_targets_follow_signed_world_position() {
        let config = test_config();
        let world_map = WorldMap::new(42, &config);
        let targets = chunk_targets_for_camera(
            &world_map,
            Vec3::new(-3.0, 0.0, 3.2),
            TerrainLodConfig {
                high_radius: 1,
                low_radius: 1,
                preload_radius: 2,
            },
        );

        assert!(
            targets
                .iter()
                .any(|target| target.key.coord == WorldChunkCoord { x: -1, z: 0 })
        );
        assert!(
            targets
                .iter()
                .all(|target| world_map.describe_chunk(target.key.coord).is_some())
        );
    }

    #[test]
    fn chunk_targets_prioritize_nearest_chunk() {
        let config = test_config();
        let world_map = WorldMap::new(42, &config);
        let targets = chunk_targets_for_camera(
            &world_map,
            Vec3::new(0.2, 0.0, 0.2),
            TerrainLodConfig {
                high_radius: 1,
                low_radius: 1,
                preload_radius: 2,
            },
        );

        assert_eq!(
            targets.first().map(|target| target.key.coord),
            Some(WorldChunkCoord { x: 0, z: 0 })
        );
    }

    #[test]
    fn chunk_targets_for_center_match_camera_sampling() {
        let config = test_config();
        let world_map = WorldMap::new(42, &config);
        let camera_position = Vec3::new(0.4, 0.0, -0.6);
        let center = world_map
            .chunk_coord_at(camera_position.x, camera_position.z)
            .expect("camera chunk should exist");

        assert_eq!(
            chunk_targets_for_camera(
                &world_map,
                camera_position,
                TerrainLodConfig {
                    high_radius: 1,
                    low_radius: 1,
                    preload_radius: 2,
                },
            ),
            chunk_targets_for_center(
                &world_map,
                center,
                TerrainLodConfig {
                    high_radius: 1,
                    low_radius: 1,
                    preload_radius: 2,
                },
            )
        );
    }

    #[test]
    fn collision_targets_follow_player_position() {
        let config = test_config();
        let world_map = WorldMap::new(42, &config);
        let targets = collision_chunk_coords_for_position(&world_map, Vec3::new(-1.8, 0.0, 0.4), 1);

        assert_eq!(
            targets.first().copied(),
            Some(WorldChunkCoord { x: -1, z: 0 })
        );
        assert!(
            targets
                .iter()
                .all(|coord| world_map.describe_chunk(*coord).is_some())
        );
    }

    #[test]
    fn collision_chunk_samples_height_close_to_world_height() {
        let config = test_config();
        let world_map = WorldMap::new(42, &config);
        let chunk = build_collision_chunk(&world_map, WorldChunkCoord { x: 0, z: 0 }, 8)
            .expect("collision chunk should build");
        let world_x = 0.93;
        let world_z = 1.11;
        let proxy_height = chunk
            .sample_height(world_x, world_z)
            .expect("cached collision sample should exist");
        let world_height = world_map
            .sample_height(world_x, world_z)
            .expect("world sample should exist");

        assert!((proxy_height - world_height).abs() < 0.2);
    }

    #[test]
    fn collision_proxy_falls_back_to_world_sampling() {
        let config = test_config();
        let world_map = WorldMap::new(42, &config);
        let mut proxy = TerrainCollisionProxy::default();
        let world_x = 0.75;
        let world_z = 0.5;

        assert_eq!(
            proxy.sample_height(&world_map, world_x, world_z),
            world_map.sample_height(world_x, world_z)
        );
    }

    #[test]
    fn collision_proxy_ground_sample_has_upward_normal() {
        let config = test_config();
        let world_map = WorldMap::new(42, &config);
        let mut proxy = TerrainCollisionProxy::default();
        let coord = world_map
            .chunk_coord_at(0.75, 0.5)
            .expect("sample chunk should exist");
        let chunk =
            build_collision_chunk(&world_map, coord, 8).expect("collision chunk should build");
        proxy.insert_chunk(coord, chunk);

        let sample = proxy
            .sample_ground(&world_map, 0.75, 0.5)
            .expect("ground sample should exist");

        assert!(sample.normal.y > 0.2);
        assert!(sample.slope >= 0.0);
    }

    #[test]
    fn terrain_impostor_mesh_builds_ring_geometry() {
        let config = test_config();
        let world_map = WorldMap::new(42, &config);
        let mesh = build_terrain_impostor_mesh(
            &world_map,
            WorldChunkCoord { x: 0, z: 0 },
            1,
            TerrainImpostorConfig {
                active_radius: 4,
                radial_bands: 3,
                angular_segments: 24,
            },
        )
        .expect("impostor mesh should build");
        let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION).expect("positions");
        let expected_vertices = (3_usize + 1) * (24_usize + 1);

        assert_eq!(positions.len(), expected_vertices);
    }

    #[test]
    fn streaming_queue_deduplicates_and_drops_invisible_chunks() {
        let high = TerrainLodLevel::High;
        let mut queue = TerrainStreamingQueue::from_targets([
            super::TerrainChunkTarget {
                key: TerrainChunkKey {
                    coord: WorldChunkCoord { x: 0, z: 0 },
                    lod: high,
                },
                visible: true,
            },
            super::TerrainChunkTarget {
                key: TerrainChunkKey {
                    coord: WorldChunkCoord { x: 0, z: 0 },
                    lod: high,
                },
                visible: true,
            },
            super::TerrainChunkTarget {
                key: TerrainChunkKey {
                    coord: WorldChunkCoord { x: 1, z: 0 },
                    lod: high,
                },
                visible: true,
            },
        ]);
        let visible = HashSet::from([TerrainChunkKey {
            coord: WorldChunkCoord { x: 1, z: 0 },
            lod: high,
        }]);
        queue.retain_targets(&visible);
        queue.enqueue_missing(
            [
                super::TerrainChunkTarget {
                    key: TerrainChunkKey {
                        coord: WorldChunkCoord { x: 1, z: 0 },
                        lod: high,
                    },
                    visible: true,
                },
                super::TerrainChunkTarget {
                    key: TerrainChunkKey {
                        coord: WorldChunkCoord { x: 2, z: 0 },
                        lod: high,
                    },
                    visible: true,
                },
            ],
            &HashSet::from([TerrainChunkKey {
                coord: WorldChunkCoord { x: 2, z: 0 },
                lod: high,
            }]),
        );

        assert_eq!(queue.len(), 1);
        assert_eq!(
            queue.pop_next().map(|target| target.key.coord),
            Some(WorldChunkCoord { x: 1, z: 0 })
        );
        assert_eq!(queue.len(), 0);
    }

    #[test]
    fn adaptive_budget_scales_down_under_heavy_load() {
        assert_eq!(adaptive_budget(4, 1, 1.2), 4);
        assert_eq!(adaptive_budget(4, 1, 2.1), 2);
        assert_eq!(adaptive_budget(4, 1, 3.4), 1);
    }

    #[test]
    fn inflight_cap_tracks_background_budget() {
        assert_eq!(streaming_inflight_cap(1), 6);
        assert_eq!(streaming_inflight_cap(3), 9);
    }

    #[test]
    fn terrain_chunk_cache_reuses_uploaded_mesh_handles() {
        let config = test_config();
        let world_map = WorldMap::new(42, &config);
        let mut cache = TerrainChunkCache::default();
        let mut meshes = Assets::<Mesh>::default();
        let context = TerrainChunkBuildContext {
            world_map: &world_map,
            seed: 42,
        };
        let key = TerrainChunkKey {
            coord: WorldChunkCoord { x: 0, z: 0 },
            lod: TerrainLodLevel::High,
        };
        let mesh = build_chunk_mesh_for_lod(&context, key).expect("chunk mesh should build");
        let first = cache.insert(
            key,
            CachedTerrainChunk {
                mesh: meshes.add(mesh),
                scatter_meshes: HashMap::new(),
                last_used_tick: 0,
            },
        );
        let second = cache.get(key).expect("chunk should be cached");

        assert_eq!(first.mesh.id(), second.mesh.id());
        assert_eq!(first.scatter_meshes.len(), second.scatter_meshes.len());
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn terrain_chunk_cache_evicts_inactive_oldest_entries() {
        let config = test_config();
        let world_map = WorldMap::new(42, &config);
        let mut cache = TerrainChunkCache::default();
        let mut meshes = Assets::<Mesh>::default();
        let context = TerrainChunkBuildContext {
            world_map: &world_map,
            seed: 42,
        };
        let active = WorldChunkCoord { x: 0, z: 0 };
        for key in [
            TerrainChunkKey {
                coord: active,
                lod: TerrainLodLevel::High,
            },
            TerrainChunkKey {
                coord: WorldChunkCoord { x: 1, z: 0 },
                lod: TerrainLodLevel::High,
            },
            TerrainChunkKey {
                coord: WorldChunkCoord { x: 0, z: 1 },
                lod: TerrainLodLevel::Low,
            },
        ] {
            let mesh = build_chunk_mesh_for_lod(&context, key).expect("chunk mesh should build");
            let _ = cache.insert(
                key,
                CachedTerrainChunk {
                    mesh: meshes.add(mesh),
                    scatter_meshes: HashMap::new(),
                    last_used_tick: 0,
                },
            );
        }

        let evicted = cache.evict_inactive(
            1,
            &HashSet::from([TerrainChunkKey {
                coord: active,
                lod: TerrainLodLevel::High,
            }]),
            &mut meshes,
        );

        assert_eq!(evicted, 2);
        assert_eq!(cache.len(), 1);
        assert!(cache.chunks.contains_key(&TerrainChunkKey {
            coord: active,
            lod: TerrainLodLevel::High,
        }));
    }

    #[test]
    fn scatter_mesh_batch_combines_multiple_instances() {
        let blueprint = detail_mesh_blueprint(Mesh::from(Capsule3d::new(0.09, 0.6)), 0.35);
        let placements = vec![
            super::ScatterPlacement {
                position: Vec3::new(0.0, 1.0, 0.0),
                biome: BiomeKind::Meadow,
                scale: 1.0,
            },
            super::ScatterPlacement {
                position: Vec3::new(2.0, 1.5, -1.0),
                biome: BiomeKind::Meadow,
                scale: 0.8,
            },
        ];

        let mesh = build_scatter_mesh_batch(&blueprint, &placements).expect("batch mesh");
        let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION).expect("positions");
        let expected_vertices = blueprint.positions.len() * placements.len();

        assert_eq!(positions.len(), expected_vertices);
    }

    #[test]
    fn keep_old_lod_visible_until_replacement_is_spawned() {
        let coord = WorldChunkCoord { x: 2, z: -1 };
        let low_key = TerrainChunkKey {
            coord,
            lod: TerrainLodLevel::Low,
        };
        let high_key = TerrainChunkKey {
            coord,
            lod: TerrainLodLevel::High,
        };

        assert!(should_keep_chunk_key(
            low_key,
            &HashSet::from([high_key]),
            &HashMap::from([(coord, high_key)]),
            &HashSet::from([low_key]),
        ));
    }

    #[test]
    fn old_lod_is_not_kept_after_replacement_has_spawned() {
        let coord = WorldChunkCoord { x: 2, z: -1 };
        let low_key = TerrainChunkKey {
            coord,
            lod: TerrainLodLevel::Low,
        };
        let high_key = TerrainChunkKey {
            coord,
            lod: TerrainLodLevel::High,
        };

        assert!(!should_keep_chunk_key(
            low_key,
            &HashSet::from([high_key]),
            &HashMap::from([(coord, high_key)]),
            &HashSet::from([low_key, high_key]),
        ));
    }

    #[test]
    fn normal_accumulation_produces_upward_component() {
        let positions = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 0.5, 1.0],
            [1.0, 0.2, 1.0],
        ];
        let indices = vec![0, 2, 1, 1, 2, 3];
        let mut normals = vec![[0.0, 0.0, 0.0]; positions.len()];
        accumulate_normals(&positions, &indices, &mut normals);

        assert!(normals.iter().all(|normal| normal[1] > 0.2));
    }

    #[test]
    fn world_seed_resource_wraps_seed_value() {
        assert_eq!(WorldSeed(511).0, 511);
    }
}
