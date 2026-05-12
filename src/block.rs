//! Core block definitions and properties.
//!
//! Each block has a set of properties that control its behavior: whether it is
//! solid, transparent, emits light, and so on. Blocks are registered in the
//! [`Registry`] at startup and looked up by [`NamespacedId`].
//!
//! # Direction conventions (Minecraft-compatible)
//!
//! | Direction | Axis | Sign |
//! |-----------|------|------|
//! | North     | Z    | `-Z` |
//! | South     | Z    | `+Z` |
//! | West      | X    | `-X` |
//! | East      | X    | `+X` |
//! | Up        | Y    | `+Y` |
//! | Down      | Y    | `-Y` |
//!
//! The `facing` property uses these values in the order:
//! `["north", "south", "west", "east", "up", "down"]`.
//! The `axis` property (for logs, pillars) uses:
//! `["y", "x", "z"]` (default y = vertical).

use crate::id::NamespacedId;
use crate::registry::Registry;

/// Compact runtime identifier for a block type (0 = air).
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub struct BlockId(pub u16);

impl BlockId {
    pub const AIR: BlockId = BlockId(0);
}

/// A single property schema defining allowed values for a block variant.
///
/// Example: `axis` → `["x", "y", "z"]` — the first value is the default.
#[derive(Debug, Clone)]
pub struct PropertySchema {
    pub name: &'static str,
    pub values: &'static [&'static str],
}

impl PropertySchema {
    pub const fn new(name: &'static str, values: &'static [&'static str]) -> Self {
        Self { name, values }
    }

    /// Index of the default (first) value.
    pub fn default_index(&self) -> u8 {
        0
    }

    /// Looks up the index for a value name, or returns the default.
    pub fn index_of(&self, value: &str) -> u8 {
        self.values.iter().position(|v| *v == value).unwrap_or(0) as u8
    }
}

/// Data-driven behavior components attached to a block definition.
///
/// Each field is optional — `None` means the block does not have that behavior.
/// Systems (worldgen, interaction, survival) check these components rather than
/// hardcoding block ID checks.
#[derive(Debug, Clone)]
pub struct BlockComponents {
    /// Block can catch fire and spread fire to neighbors.
    pub flammable: Option<FlammableComponent>,
    /// Block falls when the block below is removed (sand, gravel).
    pub gravity_affected: bool,
    /// Block can be replaced by placing another block (water, tall grass).
    pub replaceable: bool,
    /// Shape of the collision box.
    pub collision_shape: CollisionShape,
    /// Block acts as a container with the given number of slots.
    pub inventory_slots: Option<u32>,
    /// Block produces output when fueled (furnace-like).
    pub smelts_items: bool,
    /// Block can be smelted into the given item path.
    pub smelting_output: Option<&'static str>,
    /// Minimum tool tier required to drop items.
    pub required_tool_tier: ToolTier,
    /// Block drops itself when broken without the correct tool.
    pub drops_self: bool,
}

impl BlockComponents {
    /// A block with default/empty components (solid, no special behavior).
    pub const fn empty() -> Self {
        Self {
            flammable: None,
            gravity_affected: false,
            replaceable: false,
            collision_shape: CollisionShape::Full,
            inventory_slots: None,
            smelts_items: false,
            smelting_output: None,
            required_tool_tier: ToolTier::None,
            drops_self: true,
        }
    }
}

/// Flammability properties for a block.
#[derive(Debug, Clone)]
pub struct FlammableComponent {
    /// Chance (1 in N) for fire to spread to this block each tick.
    pub spread_chance: u32,
    /// Chance (1 in N) for this block to burn away each tick.
    pub burn_chance: u32,
}

impl FlammableComponent {
    pub const fn new(spread_chance: u32, burn_chance: u32) -> Self {
        Self {
            spread_chance,
            burn_chance,
        }
    }
}

/// Shape used for collision detection.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CollisionShape {
    /// Full 1×1×1 cube (most blocks).
    Full,
    /// No collision (air, water, torches).
    Empty,
    /// Custom AABB defined as [min_x, min_y, min_z, max_x, max_y, max_z].
    Partial([f32; 6]),
}

/// Required tool tier for efficient mining.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ToolTier {
    /// Any tool / hand works.
    None,
    /// Wooden tool or better.
    Wood,
    /// Stone tool or better.
    Stone,
    /// Iron tool or better.
    Iron,
    /// Diamond tool or better.
    Diamond,
}

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
    /// Data-driven behavior components.
    pub components: BlockComponents,
    /// Property schemas for blockstate variants (axis, facing, waterlogged, etc.).
    pub properties: &'static [PropertySchema],
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
        components: BlockComponents,
        properties: &'static [PropertySchema],
    ) -> Self {
        Self {
            lang_key,
            solid,
            opaque,
            transparent,
            liquid,
            hardness,
            light_emission,
            components,
            properties,
        }
    }
}

/// Direction names matching Minecraft conventions.
pub mod direction {
    pub const NORTH: &str = "north";
    pub const SOUTH: &str = "south";
    pub const WEST: &str = "west";
    pub const EAST: &str = "east";
    pub const UP: &str = "up";
    pub const DOWN: &str = "down";

    /// All six directions in the standard ordering used by the `facing` property.
    pub const ALL: [&str; 6] = [NORTH, SOUTH, WEST, EAST, UP, DOWN];
}

const AXIS_VALUES: &[&str] = &["y", "x", "z"];
const LOG_PROPERTIES: &[PropertySchema] = &[PropertySchema::new("axis", AXIS_VALUES)];

/// Facing property: `["north", "south", "west", "east", "up", "down"]`.
/// Index 0 = north (-Z), 1 = south (+Z), 2 = west (-X), 3 = east (+X), 4 = up (+Y), 5 = down (-Y).
const FACING_PROPERTY: PropertySchema = PropertySchema::new("facing", &direction::ALL);

/// Registers all built-in core blocks into the given registry.
pub fn register_core_blocks() -> Registry<BlockDefinition> {
    let mut reg = Registry::new();

    let blocks: &[(&str, BlockDefinition)] = &[
        (
            "air",
            BlockDefinition::block(
                "block.air",
                false,
                false,
                true,
                false,
                0.0,
                0,
                {
                    let mut c = BlockComponents::empty();
                    c.collision_shape = CollisionShape::Empty;
                    c.replaceable = true;
                    c.drops_self = false;
                    c
                },
                &[],
            ),
        ),
        (
            "stone",
            BlockDefinition::block(
                "block.stone",
                true,
                true,
                false,
                false,
                1.5,
                0,
                {
                    let mut c = BlockComponents::empty();
                    c.required_tool_tier = ToolTier::Wood;
                    c
                },
                &[],
            ),
        ),
        (
            "grass_block",
            BlockDefinition::block(
                "block.grass_block",
                true,
                true,
                false,
                false,
                0.6,
                0,
                BlockComponents::empty(),
                &[],
            ),
        ),
        (
            "dirt",
            BlockDefinition::block(
                "block.dirt",
                true,
                true,
                false,
                false,
                0.5,
                0,
                BlockComponents::empty(),
                &[],
            ),
        ),
        (
            "sand",
            BlockDefinition::block(
                "block.sand",
                true,
                true,
                false,
                false,
                0.5,
                0,
                {
                    let mut c = BlockComponents::empty();
                    c.gravity_affected = true;
                    c
                },
                &[],
            ),
        ),
        (
            "water",
            BlockDefinition::block(
                "block.water",
                false,
                false,
                true,
                true,
                0.0,
                1,
                {
                    let mut c = BlockComponents::empty();
                    c.collision_shape = CollisionShape::Empty;
                    c.replaceable = true;
                    c.drops_self = false;
                    c
                },
                &[],
            ),
        ),
        (
            "oak_log",
            BlockDefinition::block(
                "block.log",
                true,
                true,
                false,
                false,
                2.0,
                0,
                {
                    let mut c = BlockComponents::empty();
                    c.flammable = Some(FlammableComponent::new(5, 5));
                    c.required_tool_tier = ToolTier::Wood;
                    c
                },
                LOG_PROPERTIES,
            ),
        ),
        (
            "leaves",
            BlockDefinition::block(
                "block.leaves",
                true,
                false,
                true,
                false,
                0.2,
                0,
                {
                    let mut c = BlockComponents::empty();
                    c.flammable = Some(FlammableComponent::new(30, 60));
                    c.drops_self = false;
                    c
                },
                &[],
            ),
        ),
        (
            "planks",
            BlockDefinition::block(
                "block.planks",
                true,
                true,
                false,
                false,
                2.0,
                0,
                {
                    let mut c = BlockComponents::empty();
                    c.flammable = Some(FlammableComponent::new(5, 20));
                    c
                },
                &[],
            ),
        ),
        (
            "glass",
            BlockDefinition::block(
                "block.glass",
                true,
                false,
                true,
                false,
                0.3,
                0,
                {
                    let mut c = BlockComponents::empty();
                    c.required_tool_tier = ToolTier::None;
                    c
                },
                &[],
            ),
        ),
    ];

    for (path, def) in blocks {
        let id = NamespacedId::ferrumcraft(*path).expect("Invalid built-in block ID");
        reg.register(id, def.clone());
    }

    reg
}
