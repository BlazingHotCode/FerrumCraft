//! Chunk mesher — produces vertex/index data for visible block faces.
//!
//! Uses direct flat-array access for fast neighbor checks instead of per-block
//! function calls. Each block visits its 6 neighbors via pre-computed index
//! offsets, skipping blocks that are fully surrounded.

use crate::block::BlockId;
use crate::renderer::Vertex;
use crate::world::Chunk;

const SX: usize = 16; // CHUNK_SIZE_X
const SY: usize = 64; // CHUNK_SIZE_Y
const SZ: usize = 16; // CHUNK_SIZE_Z
const SLICE: usize = SX * SZ; // 256, size of one y-slice

/// Colors for each block type (indexed by BlockId.0).
const BLOCK_COLORS: &[[f32; 3]] = &[
    [0.0, 0.0, 0.0], // 0: air
    [0.5, 0.5, 0.5], // 1: stone
    [0.2, 0.6, 0.1], // 2: grass_block
    [0.5, 0.3, 0.1], // 3: dirt
    [0.8, 0.7, 0.5], // 4: sand
    [0.2, 0.3, 0.8], // 5: water
    [0.4, 0.2, 0.0], // 6: log
    [0.1, 0.5, 0.1], // 7: leaves
    [0.7, 0.6, 0.4], // 8: planks
    [0.6, 0.8, 0.9], // 9: glass
];

fn color(id: BlockId) -> [f32; 3] {
    let idx = id.0 as usize;
    if idx < BLOCK_COLORS.len() {
        BLOCK_COLORS[idx]
    } else {
        [1.0, 0.0, 1.0]
    }
}

/// Output of the mesher.
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
}

/// Neighbor offset table: for each of 6 faces, the (index_offset, face_direction).
/// Index offsets are pre-computed for a 16×64×16 chunk layout.
const NEIGHBORS: [(i32, u8); 6] = [
    (1, 0),    // Right  (+X): index + 1
    (-1, 1),   // Left   (-X): index - 1
    (256, 2),  // Top    (+Y): index + 256 (16*16)
    (-256, 3), // Bottom (-Y): index - 256
    (16, 4),   // Front  (+Z): index + 16
    (-16, 5),  // Back   (-Z): index - 16
];

/// Meshes a single chunk using direct flat-array access.
pub fn mesh_chunk(chunk: &Chunk) -> MeshData {
    let blocks = chunk.blocks();
    let mut verts = Vec::new();
    let mut inds = Vec::new();
    let mut off: u16 = 0;

    for y in 0..SY {
        let base_y = y * SLICE;
        for z in 0..SZ {
            let base_zy = base_y + z * SX;
            for x in 0..SX {
                let idx = base_zy + x;
                let block = blocks[idx];
                if block == BlockId::AIR {
                    continue;
                }
                let c = color(block);
                let fx = x as f32;
                let fy = y as f32;
                let fz = z as f32;

                for &(delta, dir) in &NEIGHBORS {
                    let ni = idx as i32 + delta;
                    let visible = if ni < 0 || ni >= (SY * SLICE) as i32 {
                        true
                    } else {
                        let nb = blocks[ni as usize];
                        nb == BlockId::AIR || nb.0 == 5
                    };
                    if !visible {
                        continue;
                    }

                    let q = quad(dir, fx, fy, fz, c);
                    let base = off;
                    verts.extend_from_slice(&q.vertices);
                    inds.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
                    off += 4;
                }
            }
        }
    }

    MeshData {
        vertices: verts,
        indices: inds,
    }
}

#[derive(Clone, Copy)]
struct Quad {
    vertices: [Vertex; 4],
}

fn quad(dir: u8, ox: f32, oy: f32, oz: f32, color: [f32; 3]) -> Quad {
    let (a, b, c, d) = match dir {
        // Right (+X)
        0 => (
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, 1.0, 1.0],
            [1.0, 0.0, 1.0],
        ),
        // Left (-X)
        1 => (
            [0.0, 0.0, 1.0],
            [0.0, 1.0, 1.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0],
        ),
        // Top (+Y)
        2 => (
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 1.0],
            [1.0, 1.0, 1.0],
            [1.0, 1.0, 0.0],
        ),
        // Bottom (-Y)
        3 => (
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 1.0],
        ),
        // Front (+Z)
        4 => (
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ),
        // Back (-Z)
        5 => (
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
        ),
        _ => unreachable!(),
    };

    let off = |v: [f32; 3]| [v[0] + ox - 8.0, v[1] + oy, v[2] + oz - 8.0];
    Quad {
        vertices: [
            Vertex {
                position: off(a),
                tint: color,
            },
            Vertex {
                position: off(b),
                tint: color,
            },
            Vertex {
                position: off(c),
                tint: color,
            },
            Vertex {
                position: off(d),
                tint: color,
            },
        ],
    }
}
