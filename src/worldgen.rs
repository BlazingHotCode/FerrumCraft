//! World generation feature type registry.
//!
//! This mirrors Minecraft's split between feature *types* and configured
//! features: the registry stores reusable algorithms such as `tree` or
//! `block_column`, while configuration chooses blocks, sizes, and placement.

use crate::block::BlockId;
use crate::id::NamespacedId;
use crate::registry::Registry;
use crate::world::{BlockPos, World};

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
