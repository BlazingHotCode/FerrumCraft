//! Core block definitions and properties.
//!
//! Each block has a set of properties that control its behavior: whether it is
//! solid, transparent, emits light, and so on. Blocks are registered in the
//! [`Registry`] at startup and looked up by [`NamespacedId`].

use crate::id::NamespacedId;
use crate::registry::Registry;

/// A block type with its gameplay properties.
#[derive(Debug, Clone)]
pub struct BlockDefinition {
    /// Lang key for the display name, e.g. `block.stone`.
    pub lang_key: &'static str,
    /// Whether the block occupies its full cube volume for collision.
    pub solid: bool,
    /// Whether the block is fully opaque (blocks light).
    pub opaque: bool,
    /// Whether the block is transparent (water, glass, leaves).
    pub transparent: bool,
    /// Whether the block is a liquid.
    pub liquid: bool,
    /// Mining hardness in seconds at base tool speed.
    pub hardness: f32,
    /// Light level emitted (0–15).
    pub light_emission: u8,
}

impl BlockDefinition {
    const fn block(
        lang_key: &'static str,
        solid: bool,
        opaque: bool,
        transparent: bool,
        liquid: bool,
        hardness: f32,
        light_emission: u8,
    ) -> Self {
        Self {
            lang_key,
            solid,
            opaque,
            transparent,
            liquid,
            hardness,
            light_emission,
        }
    }
}

/// Registers all built-in core blocks into the given registry.
///
/// This is called once during startup. The registry is populated in a fixed
/// order that matches the bootstrap convention (blocks first).
pub fn register_core_blocks() -> Registry<BlockDefinition> {
    let mut reg = Registry::new();

    let blocks: &[(&str, BlockDefinition)] = &[
        (
            "air",
            BlockDefinition::block("block.air", false, false, true, false, 0.0, 0),
        ),
        (
            "stone",
            BlockDefinition::block("block.stone", true, true, false, false, 1.5, 0),
        ),
        (
            "grass_block",
            BlockDefinition::block("block.grass_block", true, true, false, false, 0.6, 0),
        ),
        (
            "dirt",
            BlockDefinition::block("block.dirt", true, true, false, false, 0.5, 0),
        ),
        (
            "sand",
            BlockDefinition::block("block.sand", true, true, false, false, 0.5, 0),
        ),
        (
            "water",
            BlockDefinition::block("block.water", false, false, true, true, 0.0, 1),
        ),
        (
            "log",
            BlockDefinition::block("block.log", true, true, false, false, 2.0, 0),
        ),
        (
            "leaves",
            BlockDefinition::block("block.leaves", true, false, true, false, 0.2, 0),
        ),
        (
            "planks",
            BlockDefinition::block("block.planks", true, true, false, false, 2.0, 0),
        ),
        (
            "glass",
            BlockDefinition::block("block.glass", true, false, true, false, 0.3, 0),
        ),
    ];

    for (path, def) in blocks {
        let id = NamespacedId::ferrumcraft(*path).expect("Invalid built-in block ID");
        reg.register(id, def.clone());
    }

    reg
}
