//! World data: chunk storage, block get/set, coordinate types.
//!
//! The world is divided into 16×64×16 chunks. Each chunk stores block IDs in
//! a flat array indexed by local coordinates. The `World` struct owns all loaded
//! chunks and provides safe block access across chunk boundaries.
//!
//! In actual use: Worldgen fills chunks with terrain, the player breaks/places
//! blocks through the world API, and the meshing system reads chunk data to
//! build render geometry. Dirty chunks are flagged for remeshing on change.

use std::collections::HashMap;

use crate::block::BlockId;

pub const CHUNK_SIZE_X: usize = 16;
pub const CHUNK_SIZE_Y: usize = 64;
pub const CHUNK_SIZE_Z: usize = 16;
pub const CHUNK_VOLUME: usize = CHUNK_SIZE_X * CHUNK_SIZE_Y * CHUNK_SIZE_Z;

/// A position in chunk coordinate space (x, z).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ChunkPos(pub i32, pub i32);

/// A position in absolute block coordinate space (x, y, z).
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BlockPos(pub i32, pub i32, pub i32);

impl BlockPos {
    /// Returns the chunk containing this block position.
    pub fn chunk_pos(&self) -> ChunkPos {
        ChunkPos(
            self.0.div_euclid(CHUNK_SIZE_X as i32),
            self.2.div_euclid(CHUNK_SIZE_Z as i32),
        )
    }

    /// Returns the local (in-chunk) coordinates [0, 16), [0, 64), [0, 16).
    pub fn local(&self) -> (usize, usize, usize) {
        (
            self.0.rem_euclid(CHUNK_SIZE_X as i32) as usize,
            self.1.rem_euclid(CHUNK_SIZE_Y as i32) as usize,
            self.2.rem_euclid(CHUNK_SIZE_Z as i32) as usize,
        )
    }
}

/// A 16×64×16 column of blocks identified by chunk position.
#[derive(Clone, Debug)]
pub struct Chunk {
    pos: ChunkPos,
    blocks: Box<[BlockId; CHUNK_VOLUME]>,
    dirty: bool,
    /// Per-block property overrides: map block index → (property_schema_index, value_index) pairs.
    properties: HashMap<usize, Vec<(u8, u8)>>,
}

impl Chunk {
    /// Creates a new chunk filled with air.
    pub fn new(pos: ChunkPos) -> Self {
        Self {
            pos,
            blocks: Box::new(std::array::from_fn(|_| BlockId::AIR.clone())),
            dirty: false,
            properties: HashMap::new(),
        }
    }

    /// Gets the block at local (in-chunk) coordinates.
    pub fn get_block(&self, x: usize, y: usize, z: usize) -> BlockId {
        self.blocks[Self::index(x, y, z)].clone()
    }

    /// Sets the block at local coordinates and marks the chunk dirty.
    pub fn set_block(&mut self, x: usize, y: usize, z: usize, id: BlockId) {
        self.blocks[Self::index(x, y, z)] = id;
        self.dirty = true;
    }

    /// Whether this chunk has been modified since last mesh.
    pub fn is_dirty(&self) -> bool {
        self.dirty
    }

    /// Clears the dirty flag (called after remeshing).
    pub fn clear_dirty(&mut self) {
        self.dirty = false;
    }

    /// Returns the chunk position.
    pub fn pos(&self) -> ChunkPos {
        self.pos
    }

    /// Returns a read-only view of all blocks.
    pub fn blocks(&self) -> &[BlockId; CHUNK_VOLUME] {
        &self.blocks
    }

    /// Computes the flat array index for local coordinates.
    fn index(x: usize, y: usize, z: usize) -> usize {
        y * CHUNK_SIZE_Z * CHUNK_SIZE_X + z * CHUNK_SIZE_X + x
    }

    /// Returns the value index for a block property at the given local position.
    ///
    /// Returns 0 (the default) if no override has been set.
    pub fn get_property(&self, x: usize, y: usize, z: usize, prop_idx: u8) -> u8 {
        let idx = Self::index(x, y, z);
        self.properties
            .get(&idx)
            .and_then(|props| props.iter().find(|(p, _)| *p == prop_idx).map(|(_, v)| *v))
            .unwrap_or(0)
    }

    /// Sets a block property value at the given local position.
    pub fn set_property(&mut self, x: usize, y: usize, z: usize, prop_idx: u8, value_idx: u8) {
        let idx = Self::index(x, y, z);
        self.properties
            .entry(idx)
            .or_default()
            .push((prop_idx, value_idx));
        self.dirty = true;
    }
}

/// The game world: a collection of loaded chunks keyed by position.
#[derive(Clone, Debug)]
pub struct World {
    chunks: HashMap<ChunkPos, Chunk>,
    dirty_chunks: Vec<ChunkPos>,
}

impl World {
    /// Creates an empty world with no chunks loaded.
    pub fn new() -> Self {
        Self {
            chunks: HashMap::new(),
            dirty_chunks: Vec::new(),
        }
    }

    /// Gets the block at an absolute world position.
    ///
    /// Returns `BlockId::AIR` if the chunk is not loaded.
    pub fn get_block(&self, pos: BlockPos) -> BlockId {
        let cp = pos.chunk_pos();
        let (lx, ly, lz) = pos.local();
        self.chunks
            .get(&cp)
            .map_or(BlockId::AIR, |c| c.get_block(lx, ly, lz))
    }

    /// Sets the block at an absolute world position.
    ///
    /// The chunk will be created with air if not already loaded.
    pub fn set_block(&mut self, pos: BlockPos, id: BlockId) {
        let cp = pos.chunk_pos();
        let (lx, ly, lz) = pos.local();
        let chunk = self.chunks.entry(cp).or_insert_with(|| {
            let mut c = Chunk::new(cp);
            c.clear_dirty(); // fresh chunk isn't "dirty" yet
            c
        });
        chunk.set_block(lx, ly, lz, id);

        if !self.dirty_chunks.contains(&cp) {
            self.dirty_chunks.push(cp);
        }
    }

    /// Returns `true` if the chunk at the given position is loaded.
    pub fn is_chunk_loaded(&self, pos: ChunkPos) -> bool {
        self.chunks.contains_key(&pos)
    }

    /// Ensures a chunk exists at the given position, filled with air.
    pub fn load_chunk(&mut self, pos: ChunkPos) {
        self.chunks.entry(pos).or_insert_with(|| Chunk::new(pos));
    }

    /// Returns an iterator over all loaded chunks.
    pub fn chunks(&self) -> impl Iterator<Item = &Chunk> {
        self.chunks.values()
    }

    /// Returns a mutable iterator over all loaded chunks.
    pub fn chunks_mut(&mut self) -> impl Iterator<Item = &mut Chunk> {
        self.chunks.values_mut()
    }

    /// Drains the dirty chunk list, returning all chunks that need remeshing.
    pub fn drain_dirty(&mut self) -> Vec<ChunkPos> {
        self.dirty_chunks.drain(..).collect()
    }

    /// Number of loaded chunks.
    pub fn chunk_count(&self) -> usize {
        self.chunks.len()
    }

    /// Gets a block property value at an absolute world position.
    ///
    /// Returns 0 (default) if the chunk is not loaded or no override exists.
    pub fn get_block_property(&self, pos: BlockPos, prop_idx: u8) -> u8 {
        let cp = pos.chunk_pos();
        let (lx, ly, lz) = pos.local();
        self.chunks
            .get(&cp)
            .map_or(0, |c| c.get_property(lx, ly, lz, prop_idx))
    }

    /// Sets a block property value at an absolute world position.
    pub fn set_block_property(&mut self, pos: BlockPos, prop_idx: u8, value_idx: u8) {
        let cp = pos.chunk_pos();
        let (lx, ly, lz) = pos.local();
        let chunk = self.chunks.entry(cp).or_insert_with(|| {
            let mut c = Chunk::new(cp);
            c.clear_dirty();
            c
        });
        chunk.set_property(lx, ly, lz, prop_idx, value_idx);

        if !self.dirty_chunks.contains(&cp) {
            self.dirty_chunks.push(cp);
        }
    }
}

impl Default for World {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_defaults_to_air() {
        let c = Box::new(Chunk::new(ChunkPos(0, 0)));
        // Check a few sample positions instead of all 16384.
        assert_eq!(c.get_block(0, 0, 0), BlockId::AIR);
        assert_eq!(c.get_block(15, 0, 15), BlockId::AIR);
        assert_eq!(c.get_block(0, 63, 0), BlockId::AIR);
        assert_eq!(c.get_block(8, 32, 8), BlockId::AIR);
    }

    #[test]
    fn chunk_set_and_get() {
        let mut c = Chunk::new(ChunkPos(0, 0));
        c.set_block(0, 0, 0, BlockId("stone".to_string()));
        assert_eq!(c.get_block(0, 0, 0), BlockId("stone".to_string()));
        assert!(c.is_dirty());
    }

    #[test]
    fn world_returns_air_for_unloaded() {
        let w = World::new();
        assert_eq!(w.get_block(BlockPos(0, 0, 0)), BlockId::AIR);
    }

    #[test]
    fn world_set_and_get() {
        let mut w = World::new();
        w.set_block(BlockPos(5, 10, 5), BlockId("custom".to_string()));
        assert_eq!(
            w.get_block(BlockPos(5, 10, 5)),
            BlockId("custom".to_string())
        );
    }

    #[test]
    fn world_tracks_dirty_chunks() {
        let mut w = World::new();
        w.set_block(BlockPos(0, 0, 0), BlockId("stone".to_string()));
        assert_eq!(w.drain_dirty().len(), 1);
        assert!(w.drain_dirty().is_empty());
    }

    #[test]
    fn block_pos_chunk_roundtrip() {
        let bp = BlockPos(16, 32, 48);
        let cp = bp.chunk_pos();
        assert_eq!(cp, ChunkPos(1, 3));
        let (lx, ly, lz) = bp.local();
        assert_eq!((lx, ly, lz), (0, 32, 0));
    }

    #[test]
    fn negative_block_positions() {
        let bp = BlockPos(-1, 0, -1);
        let cp = bp.chunk_pos();
        assert_eq!(cp, ChunkPos(-1, -1));
        let (lx, ly, lz) = bp.local();
        assert_eq!((lx, ly, lz), (15, 0, 15));
    }
}
