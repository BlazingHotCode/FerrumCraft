//! Chunk mesher — produces vertex/index data from chunks using block model
//! per-face textures.

use std::collections::HashMap;

use crate::block::BlockId;
use crate::model::{BlockModel, Face};
use crate::renderer::TextureAtlas;
use crate::renderer::Vertex;
use crate::world::Chunk;

const SX: usize = 16;
const SY: usize = 64;
const SZ: usize = 16;
const SLICE: usize = SX * SZ;

/// Output of the mesher.
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
}

/// Meshes a single chunk using per-face texture UVs from block models.
/// `model_map` maps BlockId.0 → BlockModel.
pub fn mesh_chunk(
    chunk: &Chunk,
    model_map: &HashMap<u16, BlockModel>,
    atlas: &TextureAtlas,
) -> MeshData {
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

                let model = model_map.get(&block.0);
                let fx = x as f32;
                let fy = y as f32;
                let fz = z as f32;

                // Right (+X)
                if x + 1 >= SX || blocks[idx + 1] == BlockId::AIR || blocks[idx + 1].0 == 5 {
                    let uv = model.map_or([0.0; 4], |m| atlas.uv(&m.texture(Face::Right)));
                    let q = quad(0, fx, fy, fz, uv);
                    verts.extend_from_slice(&q.vertices);
                    inds.extend_from_slice(&[off, off + 1, off + 2, off, off + 2, off + 3]);
                    off += 4;
                }
                // Left (-X)
                if x == 0 || blocks[idx - 1] == BlockId::AIR || blocks[idx - 1].0 == 5 {
                    let uv = model.map_or([0.0; 4], |m| atlas.uv(&m.texture(Face::Left)));
                    let q = quad(1, fx, fy, fz, uv);
                    verts.extend_from_slice(&q.vertices);
                    inds.extend_from_slice(&[off, off + 1, off + 2, off, off + 2, off + 3]);
                    off += 4;
                }
                // Top (+Y)
                if y + 1 >= SY || blocks[idx + SLICE] == BlockId::AIR || blocks[idx + SLICE].0 == 5
                {
                    let uv = model.map_or([0.0; 4], |m| atlas.uv(&m.texture(Face::Top)));
                    let q = quad(2, fx, fy, fz, uv);
                    verts.extend_from_slice(&q.vertices);
                    inds.extend_from_slice(&[off, off + 1, off + 2, off, off + 2, off + 3]);
                    off += 4;
                }
                // Bottom (-Y)
                if y == 0 || blocks[idx - SLICE] == BlockId::AIR || blocks[idx - SLICE].0 == 5 {
                    let uv = model.map_or([0.0; 4], |m| atlas.uv(&m.texture(Face::Bottom)));
                    let q = quad(3, fx, fy, fz, uv);
                    verts.extend_from_slice(&q.vertices);
                    inds.extend_from_slice(&[off, off + 1, off + 2, off, off + 2, off + 3]);
                    off += 4;
                }
                // Front (+Z)
                if z + 1 >= SZ || blocks[idx + SX] == BlockId::AIR || blocks[idx + SX].0 == 5 {
                    let uv = model.map_or([0.0; 4], |m| atlas.uv(&m.texture(Face::Front)));
                    let q = quad(4, fx, fy, fz, uv);
                    verts.extend_from_slice(&q.vertices);
                    inds.extend_from_slice(&[off, off + 1, off + 2, off, off + 2, off + 3]);
                    off += 4;
                }
                // Back (-Z)
                if z == 0 || blocks[idx - SX] == BlockId::AIR || blocks[idx - SX].0 == 5 {
                    let uv = model.map_or([0.0; 4], |m| atlas.uv(&m.texture(Face::Back)));
                    let q = quad(5, fx, fy, fz, uv);
                    verts.extend_from_slice(&q.vertices);
                    inds.extend_from_slice(&[off, off + 1, off + 2, off, off + 2, off + 3]);
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

fn quad(dir: u8, ox: f32, oy: f32, oz: f32, uv: [f32; 4]) -> Quad {
    let [u0, v0, u1, v1] = uv;
    let off = |v: [f32; 3]| [v[0] + ox - 8.0, v[1] + oy, v[2] + oz - 8.0];
    // Each face: (a,b,c,d) where triangles are a→b→c and a→c→d (CCW outside).
    // UV coords: bottom-left→[u0,v1], top-left→[u0,v0], top-right→[u1,v0], bottom-right→[u1,v1]
    let (verts, uvs): ([_; 4], [[f32; 2]; 4]) = match dir {
        // Right (+X)
        0 => (
            [
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [1.0, 1.0, 1.0],
                [1.0, 0.0, 1.0],
            ],
            [[u0, v1], [u0, v0], [u1, v0], [u1, v1]],
        ),
        // Left (-X)
        1 => (
            [
                [0.0, 0.0, 1.0],
                [0.0, 1.0, 1.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 0.0],
            ],
            [[u0, v1], [u0, v0], [u1, v0], [u1, v1]],
        ),
        // Top (+Y)
        2 => (
            [
                [0.0, 1.0, 0.0],
                [0.0, 1.0, 1.0],
                [1.0, 1.0, 1.0],
                [1.0, 1.0, 0.0],
            ],
            [[u0, v1], [u0, v0], [u1, v0], [u1, v1]],
        ),
        // Bottom (-Y)
        3 => (
            [
                [0.0, 0.0, 1.0],
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 0.0, 1.0],
            ],
            [[u0, v1], [u0, v0], [u1, v0], [u1, v1]],
        ),
        // Front (+Z): a=bottom-left, b=bottom-right, c=top-right, d=top-left
        4 => (
            [
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 1.0],
                [1.0, 1.0, 1.0],
                [0.0, 1.0, 1.0],
            ],
            [[u0, v1], [u1, v1], [u1, v0], [u0, v0]],
        ),
        // Back (-Z): a=bottom-left (X=1), b=bottom-right (X=0), c=top-right, d=top-left
        5 => (
            [
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
                [0.0, 1.0, 0.0],
                [1.0, 1.0, 0.0],
            ],
            [[u0, v1], [u1, v1], [u1, v0], [u0, v0]],
        ),
        _ => unreachable!(),
    };

    Quad {
        vertices: [
            Vertex {
                position: off(verts[0]),
                uv: uvs[0],
            },
            Vertex {
                position: off(verts[1]),
                uv: uvs[1],
            },
            Vertex {
                position: off(verts[2]),
                uv: uvs[2],
            },
            Vertex {
                position: off(verts[3]),
                uv: uvs[3],
            },
        ],
    }
}
