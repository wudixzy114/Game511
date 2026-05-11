use std::time::Instant;

use bevy::{
    asset::RenderAssetUsages,
    math::primitives::{Capsule3d, Cylinder},
    mesh::{Indices, PrimitiveTopology},
    pbr::MeshMaterial3d,
    prelude::*,
    render::render_resource::{Extent3d, TextureDimension, TextureFormat},
};

use crate::core::config::AppConfig;
use crate::game::player::FirstPersonState;

pub struct WorldPlugin;

impl Plugin for WorldPlugin {
    fn build(&self, app: &mut App) {
        app.insert_resource(WorldSeed(0));
        app.insert_resource(WorldCycle::default());
        app.add_systems(
            Startup,
            (
                configure_world_seed,
                generate_world_map,
                cache_world_showcase_spots,
                create_terrain_material_texture,
            )
                .chain(),
        );
        app.add_systems(
            Startup,
            (spawn_camera, spawn_light, spawn_world).after(create_terrain_material_texture),
        );
        app.add_systems(
            Update,
            (
                advance_world_cycle,
                update_visible_chunks,
                animate_wanderer,
                animate_sunlight,
            ),
        );
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
struct SunLight;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
    radius: i32,
    chunk_radius: i32,
    cell_size: f32,
    subdivisions: u32,
    stride: usize,
    extent: f32,
    water_level: f32,
    vertices: Vec<TerrainVertexSample>,
    tiles: Vec<TerrainTile>,
    chunks: Vec<WorldChunk>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorldChunkCoord {
    pub x: i32,
    pub z: i32,
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

#[derive(Debug, Resource, Default)]
struct ChunkVisibilityState {
    active: Vec<WorldChunkCoord>,
}

#[derive(Debug, Resource, Clone, Copy)]
struct VisibleChunkConfig {
    radius: i32,
}

type ChunkStreamingContext<'w, 's> = (
    ResMut<'w, ChunkVisibilityState>,
    Query<'w, 's, &'static Transform, With<WorldCamera>>,
    Query<'w, 's, (Entity, &'static TerrainChunkEntity)>,
);

#[derive(Debug, Component, Clone, Copy, PartialEq, Eq)]
struct TerrainChunkEntity {
    coord: WorldChunkCoord,
}

impl WorldMap {
    fn new(seed: u64, config: &AppConfig) -> Self {
        let radius = config.world.world_radius;
        let subdivisions = config.world.terrain_subdivisions.max(1);
        let stride = (radius as usize * 2 * subdivisions as usize) + 1;
        let extent = radius as f32 * config.world.cell_size;
        let mut vertices = Vec::with_capacity(stride * stride);

        for grid_z in 0..stride {
            for grid_x in 0..stride {
                let normalized_x = grid_x as f32 / subdivisions as f32 - radius as f32;
                let normalized_z = grid_z as f32 / subdivisions as f32 - radius as f32;
                let world_x = normalized_x * config.world.cell_size;
                let world_z = normalized_z * config.world.cell_size;
                let sample = sample_terrain(world_x, world_z, seed, config);
                vertices.push(TerrainVertexSample {
                    world_x,
                    world_z,
                    height: sample.height,
                    moisture: sample.moisture,
                    temperature: sample.temperature,
                    slope: 0.0,
                    river: sample.river,
                    erosion: sample.erosion,
                    sediment: sample.sediment,
                    biome: BiomeKind::Meadow,
                });
            }
        }

        let mut world_map = Self {
            radius,
            chunk_radius: config.world.chunk_radius.max(1),
            cell_size: config.world.cell_size,
            subdivisions,
            stride,
            extent,
            water_level: config.world.water_level,
            vertices,
            tiles: Vec::with_capacity(((radius * 2 + 1) * (radius * 2 + 1)) as usize),
            chunks: Vec::new(),
        };
        world_map.rebuild_derived_fields(config);
        world_map
    }

    fn rebuild_derived_fields(&mut self, config: &AppConfig) {
        for grid_z in 0..self.stride {
            for grid_x in 0..self.stride {
                let index = self.vertex_index(grid_x, grid_z);
                let slope = self.compute_vertex_slope(grid_x, grid_z);
                let biome = determine_biome(
                    TerrainSample {
                        height: self.vertices[index].height,
                        moisture: self.vertices[index].moisture,
                        temperature: self.vertices[index].temperature,
                        erosion: self.vertices[index].erosion,
                        river: self.vertices[index].river,
                        sediment: self.vertices[index].sediment,
                    },
                    slope,
                    config.world.water_level,
                    config.world.shoreline_blend,
                );
                self.vertices[index].slope = slope;
                self.vertices[index].biome = biome;
            }
        }

        self.tiles.clear();
        for tile_z in -self.radius..=self.radius {
            for tile_x in -self.radius..=self.radius {
                let tile = self.build_tile(tile_x, tile_z);
                self.tiles.push(tile);
            }
        }

        self.rebuild_chunks();
    }

    pub fn radius(&self) -> i32 {
        self.radius
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

    pub fn chunks(&self) -> &[WorldChunk] {
        &self.chunks
    }

    pub fn find_chunk(&self, coord: WorldChunkCoord) -> Option<&WorldChunk> {
        self.chunks.iter().find(|chunk| chunk.coord == coord)
    }

    pub fn tile_at_grid(&self, x: i32, z: i32) -> Option<TerrainTile> {
        if x < -self.radius || x > self.radius || z < -self.radius || z > self.radius {
            return None;
        }

        let diameter = self.radius * 2 + 1;
        let local_x = x + self.radius;
        let local_z = z + self.radius;
        let index = (local_z * diameter + local_x) as usize;
        self.tiles.get(index).copied()
    }

    pub fn sample_world_position(&self, position: Vec3) -> Option<TerrainTile> {
        if position.x.abs() > self.extent || position.z.abs() > self.extent {
            return None;
        }
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
        let (x0, z0, x1, z1, tx, tz) = self.sample_quad(world_x, world_z)?;
        let candidates = [
            self.vertex_at(x0, z0).biome,
            self.vertex_at(x1, z0).biome,
            self.vertex_at(x0, z1).biome,
            self.vertex_at(x1, z1).biome,
        ];
        let weights = [
            (1.0 - tx) * (1.0 - tz),
            tx * (1.0 - tz),
            (1.0 - tx) * tz,
            tx * tz,
        ];
        let mut best = candidates[0];
        let mut best_weight = 0.0;
        for (biome, weight) in candidates.into_iter().zip(weights) {
            if weight > best_weight {
                best = biome;
                best_weight = weight;
            }
        }
        Some(best)
    }

    pub fn build_terrain_mesh(&self) -> Mesh {
        self.build_terrain_mesh_filtered(None)
            .expect("full world terrain mesh should exist")
    }

    pub fn build_terrain_mesh_for_chunk(&self, coord: WorldChunkCoord) -> Option<Mesh> {
        self.build_terrain_mesh_filtered(Some(coord))
    }

    fn build_terrain_mesh_filtered(&self, filter: Option<WorldChunkCoord>) -> Option<Mesh> {
        let (x_start, x_end, z_start, z_end) = filter
            .map(|coord| self.chunk_vertex_bounds(coord))
            .unwrap_or((0, self.stride - 1, 0, self.stride - 1));

        let mesh_stride = x_end - x_start + 1;
        let mesh_depth = z_end - z_start + 1;
        if mesh_stride < 2 || mesh_depth < 2 {
            return None;
        }

        let mut positions = Vec::with_capacity(self.vertices.len());
        let mut normals = vec![[0.0_f32, 1.0, 0.0]; mesh_stride * mesh_depth];
        let mut colors = Vec::with_capacity(mesh_stride * mesh_depth);
        let mut uvs = Vec::with_capacity(mesh_stride * mesh_depth);

        for z in z_start..=z_end {
            for x in x_start..=x_end {
                let vertex = self.vertex_at(x, z);
                positions.push([vertex.world_x, vertex.height, vertex.world_z]);
                colors.push(
                    vertex_color(&vertex, self.water_level)
                        .to_linear()
                        .to_f32_array(),
                );
                let uv_extent = (self.extent * 2.0).max(0.001);
                uvs.push([
                    (vertex.world_x + self.extent) / uv_extent,
                    (vertex.world_z + self.extent) / uv_extent,
                ]);
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

    fn vertex_index(&self, grid_x: usize, grid_z: usize) -> usize {
        grid_z * self.stride + grid_x
    }

    fn vertex_at(&self, grid_x: usize, grid_z: usize) -> TerrainVertexSample {
        self.vertices[self.vertex_index(grid_x, grid_z)]
    }

    fn build_tile(&self, tile_x: i32, tile_z: i32) -> TerrainTile {
        let center_grid_x = (((tile_x + self.radius) as u32 * self.subdivisions)
            .min((self.stride - 1) as u32)) as usize;
        let center_grid_z = (((tile_z + self.radius) as u32 * self.subdivisions)
            .min((self.stride - 1) as u32)) as usize;
        let center = self.vertex_at(center_grid_x, center_grid_z);
        TerrainTile {
            height: center.height,
            moisture: center.moisture,
            slope: center.slope,
            river: center.river,
            erosion: center.erosion,
            biome: center.biome,
        }
    }

    fn rebuild_chunks(&mut self) {
        self.chunks.clear();
        let chunk_count = (self.radius * 2 + 1 + self.chunk_radius - 1) / self.chunk_radius;

        for chunk_z in 0..chunk_count {
            for chunk_x in 0..chunk_count {
                let tile_x_min = -self.radius + chunk_x * self.chunk_radius;
                let tile_z_min = -self.radius + chunk_z * self.chunk_radius;
                let tile_x_max = (tile_x_min + self.chunk_radius - 1).min(self.radius);
                let tile_z_max = (tile_z_min + self.chunk_radius - 1).min(self.radius);

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

                let min = Vec2::new(
                    tile_x_min as f32 * self.cell_size,
                    tile_z_min as f32 * self.cell_size,
                );
                let max = Vec2::new(
                    (tile_x_max + 1) as f32 * self.cell_size,
                    (tile_z_max + 1) as f32 * self.cell_size,
                );
                self.chunks.push(WorldChunk {
                    coord: WorldChunkCoord {
                        x: chunk_x,
                        z: chunk_z,
                    },
                    min,
                    max,
                    biome_counts,
                    average_height: height_sum / count.max(1.0),
                    average_river: river_sum / count.max(1.0),
                    average_erosion: erosion_sum / count.max(1.0),
                    dominant_biome,
                    flow_exit: self
                        .find_chunk_flow_exit(tile_x_min, tile_z_min, tile_x_max, tile_z_max),
                });
            }
        }
    }

    fn chunk_vertex_bounds(&self, coord: WorldChunkCoord) -> (usize, usize, usize, usize) {
        let chunk_tiles_x_min = coord.x * self.chunk_radius;
        let chunk_tiles_z_min = coord.z * self.chunk_radius;
        let chunk_tiles_x_max = (chunk_tiles_x_min + self.chunk_radius).min(self.radius * 2);
        let chunk_tiles_z_max = (chunk_tiles_z_min + self.chunk_radius).min(self.radius * 2);

        let x_start = (chunk_tiles_x_min as u32 * self.subdivisions) as usize;
        let z_start = (chunk_tiles_z_min as u32 * self.subdivisions) as usize;
        let x_end = ((chunk_tiles_x_max as u32 * self.subdivisions) as usize).min(self.stride - 1);
        let z_end = ((chunk_tiles_z_max as u32 * self.subdivisions) as usize).min(self.stride - 1);
        (x_start, x_end, z_start, z_end)
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

    fn compute_vertex_slope(&self, grid_x: usize, grid_z: usize) -> f32 {
        let center = self.vertex_at(grid_x, grid_z).height;
        let left = self.vertex_at(grid_x.saturating_sub(1), grid_z).height;
        let right = self
            .vertex_at((grid_x + 1).min(self.stride - 1), grid_z)
            .height;
        let down = self.vertex_at(grid_x, grid_z.saturating_sub(1)).height;
        let up = self
            .vertex_at(grid_x, (grid_z + 1).min(self.stride - 1))
            .height;

        let dx = (right - left) / (self.cell_size / self.subdivisions as f32 * 2.0);
        let dz = (up - down) / (self.cell_size / self.subdivisions as f32 * 2.0);
        ((dx * dx + dz * dz).sqrt() + (center - self.water_level).abs() * 0.02).min(3.0)
    }

    fn sample_vertex_field(
        &self,
        world_x: f32,
        world_z: f32,
        accessor: impl Fn(TerrainVertexSample) -> f32,
    ) -> Option<f32> {
        let (x0, z0, x1, z1, tx, tz) = self.sample_quad(world_x, world_z)?;
        let a = accessor(self.vertex_at(x0, z0));
        let b = accessor(self.vertex_at(x1, z0));
        let c = accessor(self.vertex_at(x0, z1));
        let d = accessor(self.vertex_at(x1, z1));
        let top = a + (b - a) * tx;
        let bottom = c + (d - c) * tx;
        Some(top + (bottom - top) * tz)
    }

    fn sample_quad(
        &self,
        world_x: f32,
        world_z: f32,
    ) -> Option<(usize, usize, usize, usize, f32, f32)> {
        if world_x.abs() > self.extent || world_z.abs() > self.extent {
            return None;
        }
        let scale = self.subdivisions as f32 / self.cell_size;
        let local_x = (world_x + self.extent) * scale;
        let local_z = (world_z + self.extent) * scale;

        let base_x = local_x.floor().clamp(0.0, (self.stride - 2) as f32) as usize;
        let base_z = local_z.floor().clamp(0.0, (self.stride - 2) as f32) as usize;
        let tx = (local_x - base_x as f32).clamp(0.0, 1.0);
        let tz = (local_z - base_z as f32).clamp(0.0, 1.0);
        Some((base_x, base_z, base_x + 1, base_z + 1, tx, tz))
    }
}

#[derive(Debug, Resource, Clone, Copy)]
pub struct WorldPresentationControl {
    pub time_override: Option<f32>,
    pub wander_target: Option<Vec3>,
    pub wander_speed_multiplier: f32,
}

impl Default for WorldPresentationControl {
    fn default() -> Self {
        Self {
            time_override: None,
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

#[derive(Debug, Clone)]
struct DetailMaterials {
    grove: Handle<StandardMaterial>,
    meadow: Handle<StandardMaterial>,
    steppe: Handle<StandardMaterial>,
    ridge: Handle<StandardMaterial>,
}

#[derive(Debug, Clone, Copy)]
struct ScatterPlacement {
    position: Vec3,
    biome: BiomeKind,
    scale: f32,
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
    let vertex_count = world_map.vertices.len();
    commands.insert_resource(world_map);
    tracing::info!(
        target: "dao_game::world::generation",
        radius = config.world.world_radius,
        seed = seed.0,
        vertex_count = vertex_count,
        subdivisions = config.world.terrain_subdivisions,
        generation_ms = started_at.elapsed().as_secs_f32() * 1000.0,
        "world map generated"
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

    commands.insert_resource(TerrainRuntimeMaterial {
        handle: material_handle,
    });
    commands.insert_resource(ChunkVisibilityState::default());
    commands.insert_resource(VisibleChunkConfig {
        radius: config.world.visible_chunk_radius.max(0),
    });
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
    world_map: Res<WorldMap>,
    seed: Res<WorldSeed>,
    spots: Res<WorldShowcaseSpots>,
    terrain_material: Res<TerrainRuntimeMaterial>,
) {
    let started_at = Instant::now();
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

    spawn_chunked_terrain_surface(
        &mut commands,
        &mut meshes,
        &world_map,
        terrain_material.handle.clone(),
        None,
    );

    commands.spawn((
        Name::new("WaterPlane"),
        Mesh3d(meshes.add(Mesh::from(Cylinder::new(world_map.extent() * 1.42, 0.03)))),
        MeshMaterial3d(water_material),
        Transform::from_xyz(0.0, world_map.water_level(), 0.0),
    ));

    scatter_biome_details(
        &mut commands,
        &mut meshes,
        seed.0,
        &world_map,
        &detail_materials,
    );
    spawn_showcase_markers(&mut commands, &mut meshes, &mut materials, &spots);

    commands.spawn((
        Name::new("WandererPrototype"),
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
        generation_ms = started_at.elapsed().as_secs_f32() * 1000.0,
        "continuous procedural terrain spawned"
    );
}

fn update_visible_chunks(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    world_map: Res<WorldMap>,
    terrain_context: (Res<TerrainRuntimeMaterial>, Res<VisibleChunkConfig>),
    streaming_context: ChunkStreamingContext<'_, '_>,
) {
    let (terrain_material, visible_config) = terrain_context;
    let (mut visibility_state, camera_query, existing_chunks) = streaming_context;

    let Some(camera_transform) = camera_query.iter().next() else {
        return;
    };

    let chunk_coords = visible_chunk_coords(
        &world_map,
        camera_transform.translation,
        visible_config.radius,
    );
    if chunk_coords == visibility_state.active {
        return;
    }

    for (entity, chunk_component) in &existing_chunks {
        if !chunk_coords.contains(&chunk_component.coord) {
            commands.entity(entity).despawn();
        }
    }

    let existing_set: Vec<WorldChunkCoord> = existing_chunks
        .iter()
        .map(|(_, chunk)| chunk.coord)
        .collect();
    for coord in &chunk_coords {
        if existing_set.contains(coord) {
            continue;
        }
        let single = Some(std::slice::from_ref(coord));
        spawn_chunked_terrain_surface(
            &mut commands,
            &mut meshes,
            &world_map,
            terrain_material.handle.clone(),
            single,
        );
    }

    visibility_state.active = chunk_coords;
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
    world_map: Res<WorldMap>,
    spots: Res<WorldShowcaseSpots>,
    mut query: Query<&mut Transform, With<WandererPrototype>>,
) {
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

fn animate_sunlight(
    cycle: Res<WorldCycle>,
    mut clear_color: ResMut<ClearColor>,
    mut lights: Query<(&mut DirectionalLight, &mut Transform), With<SunLight>>,
) {
    let Some((mut light, mut transform)) = lights.iter_mut().next() else {
        return;
    };

    let phase = cycle.normalized_time * std::f32::consts::TAU;
    let sun_height = phase.sin();
    let daylight = cycle.daylight;
    let pitch = -0.28 - sun_height * 1.08;
    let yaw = 0.42 + phase * 0.2;

    transform.rotation = Quat::from_euler(EulerRot::XYZ, pitch, yaw, 0.0);
    light.illuminance = 1_400.0 + daylight.powf(1.55) * 48_000.0;
    light.color = Color::srgb(1.0, 0.7 + daylight * 0.24, 0.58 + daylight * 0.34);
    clear_color.0 = Color::srgb(
        0.025 + daylight * 0.21,
        0.04 + daylight * 0.24,
        0.075 + daylight * 0.31,
    );
}

fn sample_terrain(world_x: f32, world_z: f32, seed: u64, config: &AppConfig) -> TerrainSample {
    let scale = config.world.terrain_scale.max(0.001);
    let xf = world_x / scale;
    let zf = world_z / scale;
    let seed_phase = (seed % 997) as f32 * 0.0017;

    let mut amplitude = 1.0_f32;
    let mut frequency = 0.55_f32;
    let mut height_accum = 0.0_f32;
    let mut moisture_accum = 0.0_f32;
    let mut temperature_accum = 0.0_f32;
    let mut amplitude_sum = 0.0_f32;

    for octave in 0..config.world.noise_octaves.max(1) {
        let octave_phase = seed_phase + octave as f32 * 0.73;
        let wave_a = ((xf * frequency + octave_phase).sin()
            + (zf * frequency * 1.11 - octave_phase).cos())
            * 0.5;
        let wave_b = ((xf * frequency * 0.63 - octave_phase * 0.4).cos()
            + (zf * frequency * 1.47 + octave_phase * 0.6).sin())
            * 0.5;
        let ridge = 1.0 - (wave_a.abs() * 0.74 + wave_b.abs() * 0.26).clamp(0.0, 1.0);
        let ridge = ridge.powf(config.world.ridge_sharpness.max(0.2));

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
    let river_base = ((xf * config.world.river_frequency * 1.4 + seed_phase * 1.7).sin()
        - (zf * config.world.river_frequency * 1.1 - seed_phase * 0.8).cos())
    .abs();
    let river_mask = (1.0 - (river_base * 1.75).clamp(0.0, 1.0)).powf(4.0);
    let canyon_mask = ((xf * 0.11 + zf * 0.08 + seed_phase).sin() * 0.5 + 0.5).powf(2.4);
    let river = (river_mask * canyon_mask).clamp(0.0, 1.0);
    let erosion_noise = (((xf * 0.18).sin() * (zf * 0.16).cos()) * 0.5 + 0.5).clamp(0.0, 1.0);
    let erosion = (erosion_noise * config.world.erosion_strength + river * 0.65).clamp(0.0, 1.0);
    let sediment =
        ((river * 0.58 + moisture * 0.24 + (1.0 - slope_hint(normalized_height)) * 0.18)
            * config.world.sediment_bias.max(0.05))
        .clamp(0.0, 1.0);
    let river_cut = river * config.world.river_depth * (0.35 + erosion * 0.65);
    let height =
        normalized_height * config.world.height_variation + basin + (erosion_noise - 0.5) * 0.9
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

fn scatter_biome_details(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    seed: u64,
    world_map: &WorldMap,
    materials: &DetailMaterials,
) {
    for tile_z in -world_map.radius()..=world_map.radius() {
        for tile_x in -world_map.radius()..=world_map.radius() {
            let Some(tile) = world_map.tile_at_grid(tile_x, tile_z) else {
                continue;
            };
            if tile.biome() == BiomeKind::Water {
                continue;
            }

            let detail_factor = scatter_noise(seed, tile_x, tile_z, 37);
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

            let offset = scatter_offset(seed, tile_x, tile_z, world_map.cell_size() * 0.36);
            let world_x = tile_x as f32 * world_map.cell_size() + offset.x;
            let world_z = tile_z as f32 * world_map.cell_size() + offset.y;
            let Some(height) = world_map.sample_height(world_x, world_z) else {
                continue;
            };
            let placement = ScatterPlacement {
                position: Vec3::new(world_x, height, world_z),
                biome: tile.biome(),
                scale: 0.72 + scatter_noise(seed, tile_x, tile_z, 89) * 0.8 + tile.river() * 0.18
                    - tile.erosion() * 0.1,
            };
            spawn_scatter(commands, meshes, placement, materials);
        }
    }
}

fn spawn_chunked_terrain_surface(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    world_map: &WorldMap,
    material: Handle<StandardMaterial>,
    filter: Option<&[WorldChunkCoord]>,
) {
    let coords: Vec<WorldChunkCoord> = filter
        .map(|coords| coords.to_vec())
        .unwrap_or_else(|| world_map.chunks().iter().map(|chunk| chunk.coord).collect());

    for coord in coords {
        let Some(chunk) = world_map.find_chunk(coord) else {
            continue;
        };
        let Some(mesh) = world_map.build_terrain_mesh_for_chunk(chunk.coord) else {
            continue;
        };
        commands.spawn((
            Name::new(format!(
                "TerrainChunk({}, {})",
                chunk.coord.x, chunk.coord.z
            )),
            Mesh3d(meshes.add(mesh)),
            MeshMaterial3d(material.clone()),
            Transform::default(),
            TerrainChunkEntity { coord: chunk.coord },
        ));
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

fn visible_chunk_coords(
    world_map: &WorldMap,
    camera_position: Vec3,
    visible_radius: i32,
) -> Vec<WorldChunkCoord> {
    let chunk_world_span = world_map.cell_size() * world_map.chunk_radius() as f32;
    let camera_chunk_x = ((camera_position.x + world_map.extent()) / chunk_world_span)
        .floor()
        .max(0.0) as i32;
    let camera_chunk_z = ((camera_position.z + world_map.extent()) / chunk_world_span)
        .floor()
        .max(0.0) as i32;
    let max_chunk_x = ((world_map.radius() * 2 + 1 + world_map.chunk_radius() - 1)
        / world_map.chunk_radius())
        - 1;
    let max_chunk_z = max_chunk_x;

    let mut coords = Vec::new();
    for z in (camera_chunk_z - visible_radius).max(0)
        ..=(camera_chunk_z + visible_radius).min(max_chunk_z)
    {
        for x in (camera_chunk_x - visible_radius).max(0)
            ..=(camera_chunk_x + visible_radius).min(max_chunk_x)
        {
            coords.push(WorldChunkCoord { x, z });
        }
    }
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

fn spawn_scatter(
    commands: &mut Commands,
    meshes: &mut Assets<Mesh>,
    placement: ScatterPlacement,
    materials: &DetailMaterials,
) {
    match placement.biome {
        BiomeKind::Meadow => {
            commands.spawn((
                Name::new("MeadowTuft"),
                Mesh3d(meshes.add(Mesh::from(Capsule3d::new(0.09, 0.6 * placement.scale)))),
                MeshMaterial3d(materials.meadow.clone()),
                Transform::from_translation(placement.position + Vec3::Y * 0.35 * placement.scale),
            ));
        }
        BiomeKind::Grove => {
            commands.spawn((
                Name::new("GroveTree"),
                Mesh3d(meshes.add(Mesh::from(Cylinder::new(
                    0.18 * placement.scale,
                    1.8 * placement.scale,
                )))),
                MeshMaterial3d(materials.grove.clone()),
                Transform::from_translation(placement.position + Vec3::Y * 0.92 * placement.scale),
            ));
        }
        BiomeKind::Steppe => {
            commands.spawn((
                Name::new("SteppeStone"),
                Mesh3d(meshes.add(Mesh::from(Cylinder::new(
                    0.28 * placement.scale,
                    0.42 * placement.scale,
                )))),
                MeshMaterial3d(materials.steppe.clone()),
                Transform::from_translation(placement.position + Vec3::Y * 0.18 * placement.scale),
            ));
        }
        BiomeKind::Ridge => {
            commands.spawn((
                Name::new("RidgeSpire"),
                Mesh3d(meshes.add(Mesh::from(Cylinder::new(
                    0.16 * placement.scale,
                    2.4 * placement.scale,
                )))),
                MeshMaterial3d(materials.ridge.clone()),
                Transform::from_translation(placement.position + Vec3::Y * 1.15 * placement.scale),
            ));
        }
        BiomeKind::Water => {}
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

    for tile_z in -world_map.radius()..=world_map.radius() {
        for tile_x in -world_map.radius()..=world_map.radius() {
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
    use std::path::PathBuf;

    use bevy::prelude::{Mesh, Vec3};

    use crate::core::config::{
        AppConfig, EnvironmentConfig, PlayerConfig, PresentationConfig, QualityConfig,
        SignConfig, WorldConfig,
    };

    use super::{
        BiomeKind, WorldMap, WorldSeed, accumulate_normals, determine_biome, sample_terrain,
    };

    fn test_config() -> AppConfig {
        AppConfig {
            window_title: "Dao".to_string(),
            log_directory: PathBuf::from("logs"),
            performance_log_name: "performance.log".to_string(),
            frame_log_interval: 60,
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
        let a = sample_terrain(2.0, -1.0, 42, &config);
        let b = sample_terrain(2.0, -1.0, 42, &config);

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
    fn world_map_builds_continuous_mesh_resolution() {
        let config = test_config();
        let world_map = WorldMap::new(42, &config);
        let mesh = world_map.build_terrain_mesh();
        let positions = mesh.attribute(Mesh::ATTRIBUTE_POSITION).expect("positions");
        assert_eq!(positions.len(), world_map.vertices.len());
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
