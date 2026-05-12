//! World generation feature type registry.
//!
//! This mirrors Minecraft's split between feature *types* and configured
//! features: the registry stores reusable algorithms such as `tree` or
//! `block_column`, while configuration chooses blocks, sizes, and placement.

use crate::block::BlockId;
use crate::id::NamespacedId;
use crate::registry::Registry;
use crate::world::{BlockPos, World};

/// Terrain-shape noise settings inspired by Minecraft's layered noise inputs.
#[derive(Clone, Copy, Debug)]
pub struct NoiseSettings {
    pub base_height: i32,
    pub height_scale: f32,
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
            base_height: 2,
            height_scale: 3.5,
            continentalness_scale: 0.035,
            erosion_scale: 0.09,
            peaks_valleys_scale: 0.18,
        }
    }

    /// Samples Minecraft-like terrain shaping signals for a world column.
    pub fn sample(&self, world: &World, x: i32, z: i32) -> TerrainNoiseSample {
        let continentalness = fbm(world, x, z, 101, self.continentalness_scale, 4);
        let erosion = fbm(world, x, z, 202, self.erosion_scale, 3);
        let peaks_valleys = fbm(world, x, z, 303, self.peaks_valleys_scale, 2);
        let shape = continentalness * 0.65 - erosion.abs() * 0.35 + peaks_valleys * 0.25;
        let height = (self.base_height as f32 + shape * self.height_scale)
            .round()
            .clamp(1.0, 8.0) as i32;

        TerrainNoiseSample {
            continentalness,
            erosion,
            peaks_valleys,
            height,
        }
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
    ] {
        reg.register(
            NamespacedId::ferrumcraft(path).expect("valid built-in worldgen feature type ID"),
            feature,
        );
    }

    reg
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

fn fbm(world: &World, x: i32, z: i32, salt: u64, scale: f32, octaves: u32) -> f32 {
    let mut value = 0.0;
    let mut amplitude = 1.0;
    let mut frequency = scale;
    let mut max = 0.0;

    for octave in 0..octaves {
        value += smooth_noise(
            world,
            x as f32 * frequency,
            z as f32 * frequency,
            salt + octave as u64,
        ) * amplitude;
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
