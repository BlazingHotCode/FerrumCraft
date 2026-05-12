//! Naive chunk mesher — produces vertex/index data for visible block faces.
//!
//! In actual use: after terrain generation or block changes, the mesher scans
//! each chunk and emits quads for faces that neighbor air or transparent blocks.
//! The output is used to build GPU buffers that the renderer draws.

use crate::block::BlockId;
use crate::renderer::Vertex;
use crate::world::Chunk;

/// A single quad with position and color data.
#[derive(Clone, Copy, Debug)]
struct Quad {
    vertices: [Vertex; 4],
}

/// Position of a block face in the mesh (before adding quad vertices).
#[derive(Clone, Copy, Debug)]
enum FaceDir {
    Right,
    Left,
    Top,
    Bottom,
    Front,
    Back,
}

/// Colors for each block type (indexed by BlockId.0).
/// Index 0 (air) is unused.
const BLOCK_COLORS: &[[f32; 3]] = &[
    [0.0, 0.0, 0.0], // 0: air (unused)
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

fn block_color(id: BlockId) -> [f32; 3] {
    let idx = id.0 as usize;
    if idx < BLOCK_COLORS.len() {
        BLOCK_COLORS[idx]
    } else {
        [1.0, 0.0, 1.0] // magenta = missing
    }
}

/// Output of the mesher: vertex and index data plus a count.
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
}

/// Meshes a single chunk, returning vertex/index data for visible faces.
pub fn mesh_chunk(chunk: &Chunk) -> MeshData {
    let mut vertices = Vec::new();
    let mut indices = Vec::new();
    let mut vertex_offset: u16 = 0;

    for z in 0..crate::world::CHUNK_SIZE_Z {
        for y in 0..crate::world::CHUNK_SIZE_Y {
            for x in 0..crate::world::CHUNK_SIZE_X {
                let block_id = chunk.get_block(x, y, z);
                if block_id == BlockId::AIR {
                    continue;
                }

                let color = block_color(block_id);

                // Check each of the 6 faces.
                let checks: [(FaceDir, i32, i32, i32); 6] = [
                    (FaceDir::Right, 1, 0, 0),
                    (FaceDir::Left, -1, 0, 0),
                    (FaceDir::Top, 0, 1, 0),
                    (FaceDir::Bottom, 0, -1, 0),
                    (FaceDir::Front, 0, 0, 1),
                    (FaceDir::Back, 0, 0, -1),
                ];

                for (dir, dx, dy, dz) in &checks {
                    let nx = x as i32 + dx;
                    let ny = y as i32 + dy;
                    let nz = z as i32 + dz;

                    // Check if neighbor is air or outside chunk (treat as visible).
                    let visible = if nx < 0
                        || nx >= crate::world::CHUNK_SIZE_X as i32
                        || ny < 0
                        || ny >= crate::world::CHUNK_SIZE_Y as i32
                        || nz < 0
                        || nz >= crate::world::CHUNK_SIZE_Z as i32
                    {
                        true // chunk boundary — visible
                    } else {
                        let neighbor = chunk.get_block(nx as usize, ny as usize, nz as usize);
                        neighbor == BlockId::AIR || neighbor.0 == 5 // air or water
                    };

                    if !visible {
                        continue;
                    }

                    let quad = face_quad(*dir, x as f32, y as f32, z as f32, color);
                    let base = vertex_offset;
                    for v in &quad.vertices {
                        vertices.push(*v);
                    }
                    indices.extend_from_slice(&[
                        base,
                        base + 1,
                        base + 2,
                        base,
                        base + 2,
                        base + 3,
                    ]);
                    vertex_offset += 4;
                }
            }
        }
    }

    MeshData { vertices, indices }
}

/// Builds a quad for one face of a unit cube at (ox, oy, oz).
fn face_quad(dir: FaceDir, ox: f32, oy: f32, oz: f32, color: [f32; 3]) -> Quad {
    let (a, b, c, d) = match dir {
        FaceDir::Right => (
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [1.0, 1.0, 1.0],
            [1.0, 0.0, 1.0],
        ),
        FaceDir::Left => (
            [0.0, 0.0, 1.0],
            [0.0, 1.0, 1.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 0.0],
        ),
        FaceDir::Top => (
            [0.0, 1.0, 0.0],
            [0.0, 1.0, 1.0],
            [1.0, 1.0, 1.0],
            [1.0, 1.0, 0.0],
        ),
        FaceDir::Bottom => (
            [0.0, 0.0, 1.0],
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 0.0, 1.0],
        ),
        FaceDir::Front => (
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ),
        FaceDir::Back => (
            [1.0, 0.0, 0.0],
            [0.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
        ),
    };

    let offset = |v: [f32; 3]| -> [f32; 3] { [v[0] + ox - 8.0, v[1] + oy, v[2] + oz - 8.0] };

    Quad {
        vertices: [
            Vertex {
                position: offset(a),
                tint: color,
            },
            Vertex {
                position: offset(b),
                tint: color,
            },
            Vertex {
                position: offset(c),
                tint: color,
            },
            Vertex {
                position: offset(d),
                tint: color,
            },
        ],
    }
}
