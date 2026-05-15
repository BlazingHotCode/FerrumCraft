//! World generation feature type registry.
//!
//! This mirrors Minecraft's split between feature *types* and configured
//! features: the registry stores reusable algorithms such as `tree` or
//! `block_column`, while configuration chooses blocks, sizes, and placement.

use crate::block::BlockId;
use crate::id::NamespacedId;
use crate::registry::Registry;
use crate::world::{BlockPos, CHUNK_SIZE_X, CHUNK_SIZE_Y, CHUNK_SIZE_Z, ChunkPos, World};

/// Basic biome definition used by the biome source.
#[derive(Clone, Copy, Debug)]
pub struct Biome {
    name: &'static str,
    temperature: f32,
    humidity: f32,
    continentalness: f32,
    erosion: f32,
    depth: f32,
    weirdness: f32,
}

impl Biome {
    pub fn new(
        name: &'static str,
        temperature: f32,
        humidity: f32,
        continentalness: f32,
        erosion: f32,
        depth: f32,
        weirdness: f32,
    ) -> Self {
        Self {
            name,
            temperature,
            humidity,
            continentalness,
            erosion,
            depth,
            weirdness,
        }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    pub fn temperature(&self) -> f32 {
        self.temperature
    }

    pub fn humidity(&self) -> f32 {
        self.humidity
    }

    fn climate_distance(&self, sample: ClimateSample) -> f32 {
        weighted_distance(self.temperature, sample.temperature, 1.35)
            + weighted_distance(self.humidity, sample.humidity, 1.25)
            + weighted_distance(self.continentalness, sample.continentalness, 0.7)
            + weighted_distance(self.erosion, sample.erosion, 0.8)
            + weighted_distance(self.depth, sample.depth, 0.6)
            + weighted_distance(self.weirdness, sample.weirdness, 0.9)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct ClimateSample {
    pub temperature: f32,
    pub humidity: f32,
    pub continentalness: f32,
    pub erosion: f32,
    pub depth: f32,
    pub weirdness: f32,
}

/// Seeded biome source that maps world columns to registered biomes.
#[derive(Clone, Copy, Debug)]
pub struct BiomeSource {
    temperature_scale: f32,
    humidity_scale: f32,
    weirdness_scale: f32,
}

impl BiomeSource {
    pub fn demo() -> Self {
        Self {
            temperature_scale: 0.035,
            humidity_scale: 0.04,
            weirdness_scale: 0.045,
        }
    }

    pub fn sample_climate(
        &self,
        world: &World,
        noise: &NoiseSettings,
        x: i32,
        z: i32,
    ) -> ClimateSample {
        // Modern Java samples biome climate at quart resolution: one climate
        // sample covers a 4x4 block-column area before nearest-biome lookup.
        let qx = x.div_euclid(4);
        let qz = z.div_euclid(4);
        let terrain = noise.sample(world, qx, qz);
        ClimateSample {
            temperature: fbm(world, qx, qz, 404, self.temperature_scale, 4),
            humidity: fbm(world, qx, qz, 505, self.humidity_scale, 4),
            continentalness: terrain.continentalness,
            erosion: terrain.erosion,
            depth: 0.0,
            weirdness: fbm(world, qx, qz, 606, self.weirdness_scale, 3) * 0.65
                + terrain.peaks_valleys * 0.35,
        }
    }

    pub fn sample_biome_id(
        &self,
        world: &World,
        noise: &NoiseSettings,
        x: i32,
        z: i32,
    ) -> NamespacedId {
        let sample = self.sample_climate(world, noise, x, z);
        let path = builtin_biome_targets()
            .into_iter()
            .min_by(|(_, a), (_, b)| {
                a.climate_distance(sample)
                    .total_cmp(&b.climate_distance(sample))
            })
            .map(|(path, _)| path)
            .unwrap_or("plains");

        NamespacedId::ferrumcraft(path).expect("valid built-in biome ID")
    }

    pub fn sample_biome<'a>(
        &self,
        world: &World,
        noise: &NoiseSettings,
        biomes: &'a Registry<Biome>,
        x: i32,
        z: i32,
    ) -> Option<(&'a NamespacedId, &'a Biome)> {
        let sample = self.sample_climate(world, noise, x, z);
        biomes.iter().min_by(|(_, a), (_, b)| {
            a.climate_distance(sample)
                .total_cmp(&b.climate_distance(sample))
        })
    }
}

fn weighted_distance(target: f32, sample: f32, weight: f32) -> f32 {
    let d = target - sample;
    d * d * weight
}

/// Registers built-in biomes used by the current biome source.
pub fn register_core_biomes() -> Registry<Biome> {
    let mut reg = Registry::new();
    for (path, biome) in [
        (
            "plains",
            Biome::new("Plains", 0.1, 0.0, 0.05, 0.15, 0.0, 0.0),
        ),
        (
            "forest",
            Biome::new("Forest", 0.0, 0.55, 0.0, -0.15, 0.0, -0.05),
        ),
        (
            "desert",
            Biome::new("Desert", 0.75, -0.65, 0.0, 0.25, 0.0, 0.0),
        ),
        (
            "hills",
            Biome::new("Hills", -0.1, -0.05, 0.45, -0.45, 0.0, 0.55),
        ),
    ] {
        reg.register(
            NamespacedId::ferrumcraft(path).expect("valid built-in biome ID"),
            biome,
        );
    }

    reg
}

fn builtin_biome_targets() -> [(&'static str, Biome); 4] {
    [
        (
            "plains",
            Biome::new("Plains", 0.1, 0.0, 0.05, 0.15, 0.0, 0.0),
        ),
        (
            "forest",
            Biome::new("Forest", 0.0, 0.55, 0.0, -0.15, 0.0, -0.05),
        ),
        (
            "desert",
            Biome::new("Desert", 0.75, -0.65, 0.0, 0.25, 0.0, 0.0),
        ),
        (
            "hills",
            Biome::new("Hills", -0.1, -0.05, 0.45, -0.45, 0.0, 0.55),
        ),
    ]
}

/// Terrain-shape noise settings inspired by Minecraft's layered noise inputs.
#[derive(Clone, Copy, Debug)]
pub struct NoiseSettings {
    pub base_height: i32,
    pub height_scale: f32,
    pub sea_level: i32,
    pub continentalness_scale: f32,
    pub erosion_scale: f32,
    pub peaks_valleys_scale: f32,
}

/// Sampled terrain-shape signals at one column.
#[derive(Clone, Copy, Debug)]
pub struct TerrainNoiseSample {
    pub continentalness: f32,
    pub erosion: f32,
    pub peaks_valleys: f32,
    pub height: i32,
}

impl NoiseSettings {
    /// Small demo settings that create readable low hills in the current 64-high world.
    pub fn demo() -> Self {
        Self {
            base_height: 16,
            height_scale: 15.0,
            sea_level: 12,
            continentalness_scale: 0.015,
            erosion_scale: 0.035,
            peaks_valleys_scale: 0.055,
        }
    }

    /// Samples Minecraft-like terrain shaping signals for a world column.
    pub fn sample(&self, world: &World, x: i32, z: i32) -> TerrainNoiseSample {
        let warp_x = fbm_f32(world, x as f32, z as f32, 707, 0.01, 2) * 10.0;
        let warp_z = fbm_f32(world, x as f32, z as f32, 708, 0.01, 2) * 10.0;
        let wx = x as f32 + warp_x;
        let wz = z as f32 + warp_z;

        let continentalness = fbm_f32(world, wx, wz, 101, self.continentalness_scale, 5);
        let erosion = fbm_f32(world, wx, wz, 202, self.erosion_scale, 4);
        let peaks_valleys = fbm_f32(world, wx, wz, 303, self.peaks_valleys_scale, 4);
        let local_detail = fbm_f32(world, x as f32, z as f32, 304, 0.16, 2) * 0.04;

        let land = smoothstep(((continentalness + 1.0) * 0.5).clamp(0.0, 1.0));
        let ridge = 1.0 - peaks_valleys.abs();
        let ridge = ridge * ridge;
        let erosion_cut = (1.0 - erosion.abs() * 0.75).clamp(0.15, 1.0);
        let continental_lift = continentalness * 0.38;
        let ocean_depth = (0.38 - land).max(0.0) * -0.45;
        let plains = (land - 0.35).max(0.0) * 0.35;
        let hills = ridge * erosion_cut * land * 0.42;
        let mountains = ridge.powf(3.0) * land.powf(1.6) * 0.52;
        let valleys = (1.0 - ridge) * land * -0.18;
        let shape =
            continental_lift + ocean_depth + plains + hills + mountains + valleys + local_detail;
        let height = (self.base_height as f32 + shape * self.height_scale)
            .round()
            .clamp(4.0, 48.0) as i32;

        TerrainNoiseSample {
            continentalness,
            erosion,
            peaks_valleys,
            height,
        }
    }

    /// Signed terrain density for a block position. Solid terrain is density >= 0.
    pub fn density_at(&self, sample: TerrainNoiseSample, y: i32) -> f32 {
        sample.height as f32 - y as f32
    }
}

/// Populates one chunk with base stone terrain from seeded height and density fields.
pub fn populate_chunk_noise(
    world: &mut World,
    chunk_pos: ChunkPos,
    noise: &NoiseSettings,
    stone: BlockId,
) {
    world.load_chunk(chunk_pos);

    let min_x = chunk_pos.0 * CHUNK_SIZE_X as i32;
    let min_z = chunk_pos.1 * CHUNK_SIZE_Z as i32;
    for lx in 0..CHUNK_SIZE_X as i32 {
        for lz in 0..CHUNK_SIZE_Z as i32 {
            let x = min_x + lx;
            let z = min_z + lz;
            let sample = noise.sample(world, x, z);
            for y in 0..CHUNK_SIZE_Y as i32 {
                if noise.density_at(sample, y) >= 0.0 {
                    world.set_block(BlockPos(x, y, z), stone.clone());
                }
            }
        }
    }
}

/// Returns chunk positions in the square spawn area around a center chunk.
pub fn spawn_area_chunks(center: ChunkPos, radius: i32) -> Vec<ChunkPos> {
    let radius = radius.max(0);
    let mut chunks = Vec::new();
    for chunk_x in center.0 - radius..=center.0 + radius {
        for chunk_z in center.1 - radius..=center.1 + radius {
            chunks.push(ChunkPos(chunk_x, chunk_z));
        }
    }
    chunks
}

/// Applies biome-dependent top and filler blocks over generated base stone.
pub fn apply_surface_rules(
    world: &mut World,
    chunk_pos: ChunkPos,
    noise: &NoiseSettings,
    biome_source: &BiomeSource,
) {
    let min_x = chunk_pos.0 * CHUNK_SIZE_X as i32;
    let min_z = chunk_pos.1 * CHUNK_SIZE_Z as i32;
    for lx in 0..CHUNK_SIZE_X as i32 {
        for lz in 0..CHUNK_SIZE_Z as i32 {
            let x = min_x + lx;
            let z = min_z + lz;
            let sample = noise.sample(world, x, z);
            let biome_id = biome_source.sample_biome_id(world, noise, x, z);
            let surface = surface_blocks(biome_id.path(), sample.height);

            world.set_block(BlockPos(x, sample.height, z), surface.top);
            for y in (sample.height - 2).max(0)..sample.height {
                world.set_block(BlockPos(x, y, z), surface.filler.clone());
            }
        }
    }
}

/// Removes generated terrain for simple deterministic caves and ravine cuts.
pub fn apply_carvers(world: &mut World, chunk_pos: ChunkPos, noise: &NoiseSettings) {
    let min_x = chunk_pos.0 * CHUNK_SIZE_X as i32;
    let min_z = chunk_pos.1 * CHUNK_SIZE_Z as i32;
    for lx in 0..CHUNK_SIZE_X as i32 {
        for lz in 0..CHUNK_SIZE_Z as i32 {
            let x = min_x + lx;
            let z = min_z + lz;
            let sample = noise.sample(world, x, z);
            let ravine = fbm(world, x, z, 909, 0.025, 2).abs() < 0.03;

            for y in 1..=sample.height {
                let cave = fbm(world, x, z + y * 13, 808 + y as u64, 0.18, 3);
                let vertical = y as f32 / (sample.height.max(1) as f32);
                let cave_carves = y < sample.height - 1 && (cave + vertical * 0.35).abs() < 0.075;
                let ravine_carves = ravine && y <= sample.height;

                if cave_carves || ravine_carves {
                    world.set_block(BlockPos(x, y, z), BlockId::AIR);
                }
            }
        }
    }
}

/// Fills air above low terrain up to sea level, creating water bodies in basins.
pub fn apply_sea_level_water(
    world: &mut World,
    chunk_pos: ChunkPos,
    noise: &NoiseSettings,
    water: BlockId,
) {
    let min_x = chunk_pos.0 * CHUNK_SIZE_X as i32;
    let min_z = chunk_pos.1 * CHUNK_SIZE_Z as i32;
    for lx in 0..CHUNK_SIZE_X as i32 {
        for lz in 0..CHUNK_SIZE_Z as i32 {
            let x = min_x + lx;
            let z = min_z + lz;
            let sample = noise.sample(world, x, z);
            if sample.height >= noise.sea_level {
                continue;
            }

            for y in sample.height + 1..=noise.sea_level {
                let pos = BlockPos(x, y, z);
                if world.get_block(pos) == BlockId::AIR {
                    world.set_block(pos, water.clone());
                }
            }
        }
    }
}

struct SurfaceBlocks {
    top: BlockId,
    filler: BlockId,
}

fn surface_blocks(biome_path: &str, height: i32) -> SurfaceBlocks {
    let block = |path: &str| BlockId(path.to_string());
    match biome_path {
        "desert" => SurfaceBlocks {
            top: block("sand"),
            filler: block("sand"),
        },
        "hills" if height >= 7 => SurfaceBlocks {
            top: block("stone"),
            filler: block("stone"),
        },
        _ => SurfaceBlocks {
            top: block("grass_block"),
            filler: block("dirt"),
        },
    }
}

/// A registered worldgen feature algorithm.
#[derive(Clone, Copy, Debug)]
pub struct WorldgenFeatureType {
    name: &'static str,
    place: fn(&mut World, BlockPos, &FeatureConfig),
}

impl WorldgenFeatureType {
    pub fn new(name: &'static str, place: fn(&mut World, BlockPos, &FeatureConfig)) -> Self {
        Self { name, place }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }

    fn place(&self, world: &mut World, origin: BlockPos, config: &FeatureConfig) {
        (self.place)(world, origin, config);
    }
}

/// A concrete feature instance: feature type ID plus configuration.
#[derive(Clone, Debug)]
pub struct ConfiguredFeature {
    pub feature_type: NamespacedId,
    pub config: FeatureConfig,
}

/// A configured feature plus deterministic placement rules.
#[derive(Clone, Debug)]
pub struct PlacedFeature {
    pub configured: ConfiguredFeature,
    pub placement: PlacementConfig,
}

/// Basic placement rules for chunk-local worldgen features.
#[derive(Clone, Debug)]
pub struct PlacementConfig {
    pub attempts_per_chunk: u32,
    pub chance: u32,
    pub salt: u64,
    pub height: PlacementHeight,
    pub biome_filter: Vec<NamespacedId>,
}

/// Height provider used by placed features.
#[derive(Clone, Copy, Debug)]
pub enum PlacementHeight {
    Surface,
    Range { min: i32, max: i32 },
}

/// A simple structure set with deterministic chunk-level spacing.
#[derive(Clone, Copy, Debug)]
pub struct StructureSet {
    name: &'static str,
    spacing: i32,
    salt: u64,
    place: fn(&mut World, BlockPos),
}

impl StructureSet {
    pub fn new(
        name: &'static str,
        spacing: i32,
        salt: u64,
        place: fn(&mut World, BlockPos),
    ) -> Self {
        Self {
            name,
            spacing,
            salt,
            place,
        }
    }

    pub fn name(&self) -> &'static str {
        self.name
    }
}

/// Built-in feature configuration variants.
#[derive(Clone, Debug)]
pub enum FeatureConfig {
    BlockColumn {
        block: BlockId,
        min_height: i32,
        max_height: i32,
        height_salt: u64,
    },
    SimpleTree {
        log: BlockId,
        leaves: BlockId,
        trunk_height: i32,
    },
    Disk {
        block: BlockId,
        radius: i32,
        depth: i32,
    },
    Lake {
        fluid: BlockId,
        radius: i32,
        depth: i32,
    },
    Ore {
        ore: BlockId,
        replaceable: Vec<BlockId>,
        size: u32,
    },
}

/// Registers built-in feature types. Configured features select from this set.
pub fn register_core_feature_types() -> Registry<WorldgenFeatureType> {
    let mut reg = Registry::new();
    for (path, feature) in [
        (
            "block_column",
            WorldgenFeatureType::new("Block column", place_block_column),
        ),
        ("tree", WorldgenFeatureType::new("Tree", place_tree)),
        ("disk", WorldgenFeatureType::new("Disk", place_disk)),
        ("lake", WorldgenFeatureType::new("Lake", place_lake)),
        ("ore", WorldgenFeatureType::new("Ore", place_ore)),
    ] {
        reg.register(
            NamespacedId::ferrumcraft(path).expect("valid built-in worldgen feature type ID"),
            feature,
        );
    }

    reg
}

/// Registers built-in structure sets used by world generation hooks.
pub fn register_core_structure_sets() -> Registry<StructureSet> {
    let mut reg = Registry::new();
    reg.register(
        NamespacedId::ferrumcraft("stone_pile").expect("valid built-in structure set ID"),
        StructureSet::new("Stone pile", 5, 6_006, place_stone_pile),
    );
    reg
}

/// Runs simple structure placement hooks for one generated chunk.
pub fn place_structures_in_chunk(
    registry: &Registry<StructureSet>,
    world: &mut World,
    noise: &NoiseSettings,
    chunk_pos: ChunkPos,
) {
    for (_, set) in registry.iter() {
        let spacing = set.spacing.max(1);
        if chunk_pos.0.rem_euclid(spacing)
            != world.seeded_range(chunk_pos.0, chunk_pos.1, set.salt, 0, spacing - 1)
            || chunk_pos.1.rem_euclid(spacing)
                != world.seeded_range(chunk_pos.0, chunk_pos.1, set.salt + 1, 0, spacing - 1)
        {
            continue;
        }

        let x = chunk_pos.0 * CHUNK_SIZE_X as i32
            + world.seeded_range(chunk_pos.0, chunk_pos.1, set.salt + 2, 4, 11);
        let z = chunk_pos.1 * CHUNK_SIZE_Z as i32
            + world.seeded_range(chunk_pos.0, chunk_pos.1, set.salt + 3, 4, 11);
        let y = noise.sample(world, x, z).height + 1;
        (set.place)(world, BlockPos(x, y, z));
    }
}

/// Places a feature in one chunk using deterministic placement rules.
pub fn place_placed_feature_in_chunk(
    registry: &Registry<WorldgenFeatureType>,
    placed: &PlacedFeature,
    world: &mut World,
    noise: &NoiseSettings,
    biome_source: &BiomeSource,
    chunk_pos: ChunkPos,
) {
    let attempts = placed.placement.attempts_per_chunk.max(1);
    let chance = placed.placement.chance.max(1);
    let min_x = chunk_pos.0 * CHUNK_SIZE_X as i32;
    let min_z = chunk_pos.1 * CHUNK_SIZE_Z as i32;

    for attempt in 0..attempts {
        let salt = placed.placement.salt + attempt as u64 * 31;
        let x = min_x + world.seeded_range(chunk_pos.0, chunk_pos.1, salt, 2, 13);
        let z = min_z + world.seeded_range(chunk_pos.0, chunk_pos.1, salt + 1, 2, 13);
        if world.seeded_range(x, z, salt + 2, 0, chance as i32 - 1) != 0 {
            continue;
        }

        let biome_id = biome_source.sample_biome_id(world, noise, x, z);
        if !placed.placement.biome_filter.is_empty()
            && !placed
                .placement
                .biome_filter
                .iter()
                .any(|id| id == &biome_id)
        {
            continue;
        }

        let y = match placed.placement.height {
            PlacementHeight::Surface => noise.sample(world, x, z).height + 1,
            PlacementHeight::Range { min, max } => world.seeded_range(x, z, salt + 3, min, max),
        };
        place_configured_feature(registry, &placed.configured, world, BlockPos(x, y, z));
    }
}

/// Places a configured feature by resolving its registered feature type.
pub fn place_configured_feature(
    registry: &Registry<WorldgenFeatureType>,
    configured: &ConfiguredFeature,
    world: &mut World,
    origin: BlockPos,
) {
    if let Some(feature_type) = registry.get(&configured.feature_type) {
        feature_type.place(world, origin, &configured.config);
    } else {
        log::warn!(target: "worldgen", "Missing worldgen feature type: {}", configured.feature_type);
    }
}

fn place_block_column(world: &mut World, origin: BlockPos, config: &FeatureConfig) {
    let FeatureConfig::BlockColumn {
        block,
        min_height,
        max_height,
        height_salt,
    } = config
    else {
        return;
    };

    let height = world.seeded_range(origin.0, origin.2, *height_salt, *min_height, *max_height);
    for y in 0..height {
        world.set_block(BlockPos(origin.0, origin.1 + y, origin.2), block.clone());
    }
}

fn place_tree(world: &mut World, origin: BlockPos, config: &FeatureConfig) {
    let FeatureConfig::SimpleTree {
        log,
        leaves,
        trunk_height,
    } = config
    else {
        return;
    };

    for y in 0..*trunk_height {
        world.set_block(BlockPos(origin.0, origin.1 + y, origin.2), log.clone());
    }

    let canopy_y = origin.1 + trunk_height - 1;
    world.set_block(BlockPos(origin.0 - 1, canopy_y, origin.2), leaves.clone());
    world.set_block(BlockPos(origin.0 + 1, canopy_y, origin.2), leaves.clone());
    world.set_block(BlockPos(origin.0, canopy_y, origin.2 - 1), leaves.clone());
    world.set_block(BlockPos(origin.0, canopy_y, origin.2 + 1), leaves.clone());
    world.set_block(BlockPos(origin.0, canopy_y + 1, origin.2), leaves.clone());
}

fn place_disk(world: &mut World, origin: BlockPos, config: &FeatureConfig) {
    let FeatureConfig::Disk {
        block,
        radius,
        depth,
    } = config
    else {
        return;
    };

    let r2 = radius * radius;
    for dx in -*radius..=*radius {
        for dz in -*radius..=*radius {
            if dx * dx + dz * dz > r2 {
                continue;
            }
            for dy in 0..*depth {
                world.set_block(
                    BlockPos(origin.0 + dx, origin.1 - dy, origin.2 + dz),
                    block.clone(),
                );
            }
        }
    }
}

fn place_lake(world: &mut World, origin: BlockPos, config: &FeatureConfig) {
    let FeatureConfig::Lake {
        fluid,
        radius,
        depth,
    } = config
    else {
        return;
    };

    let r2 = radius * radius;
    for dx in -*radius..=*radius {
        for dz in -*radius..=*radius {
            if dx * dx + dz * dz > r2 {
                continue;
            }
            for dy in 0..*depth {
                world.set_block(
                    BlockPos(origin.0 + dx, origin.1 - dy, origin.2 + dz),
                    fluid.clone(),
                );
            }
        }
    }
}

fn place_ore(world: &mut World, origin: BlockPos, config: &FeatureConfig) {
    let FeatureConfig::Ore {
        ore,
        replaceable,
        size,
    } = config
    else {
        return;
    };

    for i in 0..(*size).max(1) {
        let salt = 1_100 + i as u64;
        let dx = world.seeded_range(origin.0 + i as i32, origin.2, salt, -1, 1);
        let dy = world.seeded_range(origin.0, origin.2 + i as i32, salt + 1, -1, 1);
        let dz = world.seeded_range(origin.0 - i as i32, origin.2, salt + 2, -1, 1);
        let pos = BlockPos(origin.0 + dx, origin.1 + dy, origin.2 + dz);
        let current = world.get_block(pos);
        if replaceable.iter().any(|target| target == &current) {
            world.set_block(pos, ore.clone());
        }
    }
}

fn place_stone_pile(world: &mut World, origin: BlockPos) {
    let stone = BlockId("stone".to_string());
    world.set_block(origin, stone.clone());
    world.set_block(BlockPos(origin.0 + 1, origin.1, origin.2), stone.clone());
    world.set_block(BlockPos(origin.0, origin.1, origin.2 + 1), stone.clone());
    world.set_block(BlockPos(origin.0, origin.1 + 1, origin.2), stone);
}

fn fbm(world: &World, x: i32, z: i32, salt: u64, scale: f32, octaves: u32) -> f32 {
    fbm_f32(world, x as f32, z as f32, salt, scale, octaves)
}

fn fbm_f32(world: &World, x: f32, z: f32, salt: u64, scale: f32, octaves: u32) -> f32 {
    let mut value = 0.0;
    let mut amplitude = 1.0;
    let mut frequency = scale;
    let mut max = 0.0;

    for octave in 0..octaves {
        value +=
            smooth_noise(world, x * frequency, z * frequency, salt + octave as u64) * amplitude;
        max += amplitude;
        amplitude *= 0.5;
        frequency *= 2.0;
    }

    if max > 0.0 { value / max } else { 0.0 }
}

fn smooth_noise(world: &World, x: f32, z: f32, salt: u64) -> f32 {
    let x0 = x.floor() as i32;
    let z0 = z.floor() as i32;
    let tx = smoothstep(x - x.floor());
    let tz = smoothstep(z - z.floor());

    let a = hash_noise(world, x0, z0, salt);
    let b = hash_noise(world, x0 + 1, z0, salt);
    let c = hash_noise(world, x0, z0 + 1, salt);
    let d = hash_noise(world, x0 + 1, z0 + 1, salt);

    lerp(lerp(a, b, tx), lerp(c, d, tx), tz)
}

fn hash_noise(world: &World, x: i32, z: i32, salt: u64) -> f32 {
    let value = world.seeded_u32(x, z, salt) as f32 / u32::MAX as f32;
    value * 2.0 - 1.0
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn biome_climate_samples_use_quart_coordinates() {
        let world = World::with_seed(12345);
        let noise = NoiseSettings::demo();
        let source = BiomeSource::demo();

        let a = source.sample_climate(&world, &noise, 0, 0);
        let b = source.sample_climate(&world, &noise, 3, 3);
        let c = source.sample_climate(&world, &noise, 4, 4);

        assert_eq!(a.temperature, b.temperature);
        assert_eq!(a.humidity, b.humidity);
        assert_eq!(a.continentalness, b.continentalness);
        assert_ne!(a.temperature, c.temperature);
    }

    #[test]
    fn populate_chunk_noise_fills_stone_to_density_surface() {
        let mut world = World::with_seed(12345);
        let noise = NoiseSettings::demo();
        let stone = BlockId("stone".to_string());
        let chunk_pos = ChunkPos(-1, 0);

        populate_chunk_noise(&mut world, chunk_pos, &noise, stone.clone());

        let x = -1;
        let z = 0;
        let sample = noise.sample(&world, x, z);
        assert_eq!(world.get_block(BlockPos(x, 0, z)), stone);
        assert_eq!(world.get_block(BlockPos(x, sample.height, z)).0, "stone");
        assert_eq!(
            world.get_block(BlockPos(x, sample.height + 1, z)),
            BlockId::AIR
        );
    }

    #[test]
    fn surface_rules_replace_top_and_filler_blocks() {
        let mut world = World::with_seed(12345);
        let noise = NoiseSettings::demo();
        let source = BiomeSource::demo();
        let chunk_pos = ChunkPos(0, 0);

        populate_chunk_noise(&mut world, chunk_pos, &noise, BlockId("stone".to_string()));
        apply_surface_rules(&mut world, chunk_pos, &noise, &source);

        let x = 0;
        let z = 0;
        let sample = noise.sample(&world, x, z);
        let biome_id = source.sample_biome_id(&world, &noise, x, z);
        let expected = surface_blocks(biome_id.path(), sample.height);

        assert_eq!(world.get_block(BlockPos(x, sample.height, z)), expected.top);
        assert_eq!(
            world.get_block(BlockPos(x, sample.height - 1, z)),
            expected.filler
        );
    }

    #[test]
    fn sea_level_water_fills_low_air_columns() {
        let mut world = World::with_seed(12345);
        let noise = NoiseSettings::demo();
        let source = BiomeSource::demo();
        let water = BlockId("water".to_string());
        let chunk_pos = (-12..=12)
            .flat_map(|cx| (-4..=4).map(move |cz| ChunkPos(cx, cz)))
            .find(|chunk_pos| {
                let min_x = chunk_pos.0 * CHUNK_SIZE_X as i32;
                let min_z = chunk_pos.1 * CHUNK_SIZE_Z as i32;
                (0..CHUNK_SIZE_X as i32).any(|lx| {
                    (0..CHUNK_SIZE_Z as i32).any(|lz| {
                        noise.sample(&world, min_x + lx, min_z + lz).height < noise.sea_level
                    })
                })
            })
            .expect("demo noise should create low terrain");

        populate_chunk_noise(&mut world, chunk_pos, &noise, BlockId("stone".to_string()));
        apply_surface_rules(&mut world, chunk_pos, &noise, &source);
        apply_sea_level_water(&mut world, chunk_pos, &noise, water.clone());

        let min_x = chunk_pos.0 * CHUNK_SIZE_X as i32;
        let min_z = chunk_pos.1 * CHUNK_SIZE_Z as i32;
        let (x, z, sample) = (0..CHUNK_SIZE_X as i32)
            .flat_map(|lx| (0..CHUNK_SIZE_Z as i32).map(move |lz| (min_x + lx, min_z + lz)))
            .map(|(x, z)| (x, z, noise.sample(&world, x, z)))
            .find(|(_, _, sample)| sample.height < noise.sea_level)
            .expect("selected chunk should contain low terrain");

        assert_eq!(world.get_block(BlockPos(x, noise.sea_level, z)), water);
        assert_ne!(world.get_block(BlockPos(x, sample.height, z)).0, "water");
    }

    #[test]
    fn carvers_remove_generated_terrain() {
        let mut world = World::with_seed(12345);
        let noise = NoiseSettings::demo();
        let source = BiomeSource::demo();
        let chunk_pos = ChunkPos(0, 0);

        populate_chunk_noise(&mut world, chunk_pos, &noise, BlockId("stone".to_string()));
        apply_surface_rules(&mut world, chunk_pos, &noise, &source);
        apply_carvers(&mut world, chunk_pos, &noise);

        let min_x = chunk_pos.0 * CHUNK_SIZE_X as i32;
        let min_z = chunk_pos.1 * CHUNK_SIZE_Z as i32;
        let carved = (0..CHUNK_SIZE_X as i32).any(|lx| {
            (0..CHUNK_SIZE_Z as i32).any(|lz| {
                let x = min_x + lx;
                let z = min_z + lz;
                let sample = noise.sample(&world, x, z);
                (1..=sample.height).any(|y| world.get_block(BlockPos(x, y, z)) == BlockId::AIR)
            })
        });

        assert!(carved);
    }

    #[test]
    fn core_feature_registry_contains_pipeline_feature_types() {
        let registry = register_core_feature_types();

        for path in ["block_column", "tree", "disk", "lake", "ore"] {
            let id = NamespacedId::ferrumcraft(path).expect("valid feature ID");
            assert!(registry.contains(&id));
        }
    }

    #[test]
    fn placed_feature_uses_deterministic_chunk_placement() {
        let registry = register_core_feature_types();
        let mut world = World::with_seed(12345);
        let noise = NoiseSettings::demo();
        let source = BiomeSource::demo();
        let chunk_pos = ChunkPos(1, -1);
        let block = BlockId("marker".to_string());
        let placed = PlacedFeature {
            configured: ConfiguredFeature {
                feature_type: NamespacedId::ferrumcraft("block_column").expect("valid feature ID"),
                config: FeatureConfig::BlockColumn {
                    block: block.clone(),
                    min_height: 1,
                    max_height: 1,
                    height_salt: 99,
                },
            },
            placement: PlacementConfig {
                attempts_per_chunk: 1,
                chance: 1,
                salt: 77,
                height: PlacementHeight::Surface,
                biome_filter: Vec::new(),
            },
        };

        let x = chunk_pos.0 * CHUNK_SIZE_X as i32
            + world.seeded_range(chunk_pos.0, chunk_pos.1, 77, 2, 13);
        let z = chunk_pos.1 * CHUNK_SIZE_Z as i32
            + world.seeded_range(chunk_pos.0, chunk_pos.1, 78, 2, 13);
        let y = noise.sample(&world, x, z).height + 1;

        place_placed_feature_in_chunk(&registry, &placed, &mut world, &noise, &source, chunk_pos);

        assert_eq!(world.get_block(BlockPos(x, y, z)), block);
    }

    #[test]
    fn ore_feature_replaces_only_target_blocks() {
        let registry = register_core_feature_types();
        let mut world = World::with_seed(12345);
        let ore = BlockId("coal_ore".to_string());
        let stone = BlockId("stone".to_string());
        let dirt = BlockId("dirt".to_string());
        let origin = BlockPos(0, 3, 0);
        for x in -1..=1 {
            for y in 2..=4 {
                for z in -1..=1 {
                    world.set_block(BlockPos(x, y, z), stone.clone());
                }
            }
        }
        world.set_block(origin, dirt.clone());

        let configured = ConfiguredFeature {
            feature_type: NamespacedId::ferrumcraft("ore").expect("valid feature ID"),
            config: FeatureConfig::Ore {
                ore: ore.clone(),
                replaceable: vec![stone],
                size: 12,
            },
        };
        place_configured_feature(&registry, &configured, &mut world, origin);

        assert_eq!(world.get_block(origin), dirt);
        let placed_ore = (-1..=1)
            .any(|x| (2..=4).any(|y| (-1..=1).any(|z| world.get_block(BlockPos(x, y, z)) == ore)));
        assert!(placed_ore);
    }

    #[test]
    fn placed_tree_respects_biome_filter() {
        let registry = register_core_feature_types();
        let mut world = World::with_seed(12345);
        let noise = NoiseSettings::demo();
        let source = BiomeSource::demo();
        let chunk_pos = ChunkPos(0, 0);
        let log = BlockId("oak_log".to_string());
        let placed = PlacedFeature {
            configured: ConfiguredFeature {
                feature_type: NamespacedId::ferrumcraft("tree").expect("valid feature ID"),
                config: FeatureConfig::SimpleTree {
                    log: log.clone(),
                    leaves: BlockId("oak_leaves".to_string()),
                    trunk_height: 3,
                },
            },
            placement: PlacementConfig {
                attempts_per_chunk: 1,
                chance: 1,
                salt: 88,
                height: PlacementHeight::Surface,
                biome_filter: vec![
                    NamespacedId::ferrumcraft("missing_biome").expect("valid biome ID"),
                ],
            },
        };

        place_placed_feature_in_chunk(&registry, &placed, &mut world, &noise, &source, chunk_pos);

        assert!(world.chunks().all(|chunk| !chunk.blocks().contains(&log)));
    }

    #[test]
    fn structure_set_registry_contains_stone_pile() {
        let registry = register_core_structure_sets();
        let id = NamespacedId::ferrumcraft("stone_pile").expect("valid structure set ID");

        assert!(registry.contains(&id));
    }

    #[test]
    fn structure_hooks_place_deterministic_structures() {
        let registry = register_core_structure_sets();
        let mut world = World::with_seed(12345);
        let noise = NoiseSettings::demo();
        let stone = BlockId("stone".to_string());
        let chunk_pos = (-10..=10)
            .flat_map(|cx| (-10..=10).map(move |cz| ChunkPos(cx, cz)))
            .find(|chunk_pos| {
                let spacing = 5;
                chunk_pos.0.rem_euclid(spacing)
                    == world.seeded_range(chunk_pos.0, chunk_pos.1, 6_006, 0, spacing - 1)
                    && chunk_pos.1.rem_euclid(spacing)
                        == world.seeded_range(chunk_pos.0, chunk_pos.1, 6_007, 0, spacing - 1)
            })
            .expect("test range should contain a structure chunk");

        place_structures_in_chunk(&registry, &mut world, &noise, chunk_pos);

        assert!(world.chunks().any(|chunk| chunk.blocks().contains(&stone)));
    }

    #[test]
    fn spawn_area_chunks_cover_square_radius() {
        let chunks = spawn_area_chunks(ChunkPos(2, -1), 1);

        assert_eq!(chunks.len(), 9);
        assert!(chunks.contains(&ChunkPos(1, -2)));
        assert!(chunks.contains(&ChunkPos(2, -1)));
        assert!(chunks.contains(&ChunkPos(3, 0)));
    }
}
