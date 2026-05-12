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
const LOWERED_WATER_HEIGHT: f32 = 14.0 / 16.0;

fn is_transparent(b: &BlockId) -> bool {
    matches!(b.0.as_str(), "water" | "glass" | "oak_leaves")
}

fn face_visible(current: &BlockId, neighbor: &BlockId) -> bool {
    if neighbor.0.is_empty() {
        return true;
    }

    is_transparent(neighbor) && neighbor != current
}

fn push_quad(verts: &mut Vec<Vertex>, inds: &mut Vec<u16>, off: &mut u16, q: Quad) {
    verts.extend_from_slice(&q.vertices);
    let ao = q.vertices.map(|v| v.ao);
    if ao[0] + ao[2] > ao[1] + ao[3] {
        inds.extend_from_slice(&[*off, *off + 1, *off + 3, *off + 1, *off + 2, *off + 3]);
    } else {
        inds.extend_from_slice(&[*off, *off + 1, *off + 2, *off, *off + 2, *off + 3]);
    }
    *off += 4;
}

fn random_rotation(path: &str, x: usize, y: usize, z: usize, face: Face) -> u8 {
    if !matches!(path, "block/sand" | "block/dirt" | "block/grass_block_top") {
        return 0;
    }

    let mut h = x as u32;
    h = h.wrapping_mul(0x9E37_79B9) ^ (y as u32).wrapping_mul(0x85EB_CA6B);
    h ^= (z as u32).wrapping_mul(0xC2B2_AE35);
    h ^= face as u32;
    ((h ^ (h >> 16)) & 3) as u8
}

fn occludes(blocks: &[BlockId], x: usize, y: usize, z: usize, dx: i32, dy: i32, dz: i32) -> bool {
    let nx = x as i32 + dx;
    let ny = y as i32 + dy;
    let nz = z as i32 + dz;
    if nx < 0 || ny < 0 || nz < 0 || nx >= SX as i32 || ny >= SY as i32 || nz >= SZ as i32 {
        return false;
    }

    let block = &blocks[ny as usize * SLICE + nz as usize * SX + nx as usize];
    !block.0.is_empty() && !is_transparent(block)
}

fn vertex_ao(
    blocks: &[BlockId],
    x: usize,
    y: usize,
    z: usize,
    normal: [i32; 3],
    side_a: [i32; 3],
    side_b: [i32; 3],
) -> f32 {
    let side_1 = occludes(
        blocks,
        x,
        y,
        z,
        normal[0] + side_a[0],
        normal[1] + side_a[1],
        normal[2] + side_a[2],
    );
    let side_2 = occludes(
        blocks,
        x,
        y,
        z,
        normal[0] + side_b[0],
        normal[1] + side_b[1],
        normal[2] + side_b[2],
    );
    let corner = occludes(
        blocks,
        x,
        y,
        z,
        normal[0] + side_a[0] + side_b[0],
        normal[1] + side_a[1] + side_b[1],
        normal[2] + side_a[2] + side_b[2],
    );

    let level = if side_1 && side_2 {
        0
    } else {
        3 - side_1 as u8 - side_2 as u8 - corner as u8
    };
    [0.45, 0.62, 0.8, 1.0][level as usize]
}

fn face_ao(blocks: &[BlockId], x: usize, y: usize, z: usize, dir: u8) -> [f32; 4] {
    let (normal, sides): ([i32; 3], [[i32; 3]; 4]) = match dir {
        0 => ([1, 0, 0], [[0, -1, 0], [0, 1, 0], [0, 1, 0], [0, -1, 0]]),
        1 => ([-1, 0, 0], [[0, -1, 0], [0, 1, 0], [0, 1, 0], [0, -1, 0]]),
        2 => ([0, 1, 0], [[-1, 0, 0], [-1, 0, 0], [1, 0, 0], [1, 0, 0]]),
        3 => ([0, -1, 0], [[-1, 0, 0], [-1, 0, 0], [1, 0, 0], [1, 0, 0]]),
        4 => ([0, 0, 1], [[-1, 0, 0], [1, 0, 0], [1, 0, 0], [-1, 0, 0]]),
        5 => ([0, 0, -1], [[1, 0, 0], [-1, 0, 0], [-1, 0, 0], [1, 0, 0]]),
        _ => unreachable!(),
    };
    let other_sides: [[i32; 3]; 4] = match dir {
        0 | 1 => [[0, 0, -1], [0, 0, -1], [0, 0, 1], [0, 0, 1]],
        2 | 3 => [[0, 0, -1], [0, 0, 1], [0, 0, 1], [0, 0, -1]],
        4 | 5 => [[0, -1, 0], [0, -1, 0], [0, 1, 0], [0, 1, 0]],
        _ => unreachable!(),
    };

    [
        vertex_ao(blocks, x, y, z, normal, sides[0], other_sides[0]),
        vertex_ao(blocks, x, y, z, normal, sides[1], other_sides[1]),
        vertex_ao(blocks, x, y, z, normal, sides[2], other_sides[2]),
        vertex_ao(blocks, x, y, z, normal, sides[3], other_sides[3]),
    ]
}

fn face_uv(
    model: Option<&BlockModel>,
    atlas: &TextureAtlas,
    face: Face,
    x: usize,
    y: usize,
    z: usize,
) -> ([f32; 4], u8) {
    let Some(model) = model else {
        return ([0.0; 4], 0);
    };
    let texture = model.texture(face);
    (atlas.uv(texture), random_rotation(texture, x, y, z, face))
}

/// Output of the mesher.
pub struct MeshData {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u16>,
}

/// Meshes a single chunk using per-face texture UVs from block models.
/// `model_map` maps block path string → BlockModel.
pub fn mesh_chunk(
    chunk: &Chunk,
    model_map: &HashMap<String, BlockModel>,
    atlas: &TextureAtlas,
) -> MeshData {
    let blocks = chunk.blocks();
    let chunk_origin_x = chunk.pos().0 as f32 * SX as f32;
    let chunk_origin_z = chunk.pos().1 as f32 * SZ as f32;
    let mut opaque_verts = Vec::new();
    let mut opaque_inds = Vec::new();
    let mut opaque_off: u16 = 0;
    let mut transparent_verts = Vec::new();
    let mut transparent_inds = Vec::new();
    let mut transparent_off: u16 = 0;

    for y in 0..SY {
        let base_y = y * SLICE;
        for z in 0..SZ {
            let base_zy = base_y + z * SX;
            for x in 0..SX {
                let idx = base_zy + x;
                let block = &blocks[idx];
                if block.0.is_empty() {
                    continue;
                }

                let model = model_map.get(&block.0);
                let (verts, inds, off) = if is_transparent(block) {
                    (
                        &mut transparent_verts,
                        &mut transparent_inds,
                        &mut transparent_off,
                    )
                } else {
                    (&mut opaque_verts, &mut opaque_inds, &mut opaque_off)
                };

                let fx = chunk_origin_x + x as f32;
                let fy = y as f32;
                let fz = chunk_origin_z + z as f32;
                let top_height =
                    if block.0 == "water" && (y + 1 >= SY || blocks[idx + SLICE].0 != "water") {
                        LOWERED_WATER_HEIGHT
                    } else {
                        1.0
                    };

                // Right (+X)
                if x + 1 >= SX || face_visible(block, &blocks[idx + 1]) {
                    let (uv, rotation) = face_uv(model, atlas, Face::Right, x, y, z);
                    let q = quad(
                        0,
                        fx,
                        fy,
                        fz,
                        uv,
                        top_height,
                        rotation,
                        face_ao(blocks, x, y, z, 0),
                    );
                    push_quad(verts, inds, off, q);
                }
                // Left (-X)
                if x == 0 || face_visible(block, &blocks[idx - 1]) {
                    let (uv, rotation) = face_uv(model, atlas, Face::Left, x, y, z);
                    let q = quad(
                        1,
                        fx,
                        fy,
                        fz,
                        uv,
                        top_height,
                        rotation,
                        face_ao(blocks, x, y, z, 1),
                    );
                    push_quad(verts, inds, off, q);
                }
                // Top (+Y)
                if y + 1 >= SY || face_visible(block, &blocks[idx + SLICE]) {
                    let (uv, rotation) = face_uv(model, atlas, Face::Top, x, y, z);
                    let q = quad(
                        2,
                        fx,
                        fy,
                        fz,
                        uv,
                        top_height,
                        rotation,
                        face_ao(blocks, x, y, z, 2),
                    );
                    push_quad(verts, inds, off, q);
                }
                // Bottom (-Y)
                if y == 0 || face_visible(block, &blocks[idx - SLICE]) {
                    let (uv, rotation) = face_uv(model, atlas, Face::Bottom, x, y, z);
                    let q = quad(
                        3,
                        fx,
                        fy,
                        fz,
                        uv,
                        top_height,
                        rotation,
                        face_ao(blocks, x, y, z, 3),
                    );
                    push_quad(verts, inds, off, q);
                }
                // Front (+Z)
                if z + 1 >= SZ || face_visible(block, &blocks[idx + SX]) {
                    let (uv, rotation) = face_uv(model, atlas, Face::Front, x, y, z);
                    let q = quad(
                        4,
                        fx,
                        fy,
                        fz,
                        uv,
                        top_height,
                        rotation,
                        face_ao(blocks, x, y, z, 4),
                    );
                    push_quad(verts, inds, off, q);
                }
                // Back (-Z)
                if z == 0 || face_visible(block, &blocks[idx - SX]) {
                    let (uv, rotation) = face_uv(model, atlas, Face::Back, x, y, z);
                    let q = quad(
                        5,
                        fx,
                        fy,
                        fz,
                        uv,
                        top_height,
                        rotation,
                        face_ao(blocks, x, y, z, 5),
                    );
                    push_quad(verts, inds, off, q);
                }
            }
        }
    }

    let index_offset = opaque_verts.len() as u16;
    opaque_inds.extend(transparent_inds.into_iter().map(|i| i + index_offset));
    opaque_verts.extend(transparent_verts);

    MeshData {
        vertices: opaque_verts,
        indices: opaque_inds,
    }
}

#[derive(Clone, Copy)]
struct Quad {
    vertices: [Vertex; 4],
}

fn quad(
    dir: u8,
    ox: f32,
    oy: f32,
    oz: f32,
    uv: [f32; 4],
    top_height: f32,
    rotation: u8,
    ao: [f32; 4],
) -> Quad {
    let [u0, v0, u1, v1] = uv;
    let off = |v: [f32; 3]| [v[0] + ox - 8.5, v[1] + oy, v[2] + oz - 8.5];
    // Each face: (a,b,c,d) where triangles are a→b→c and a→c→d (CCW outside).
    // UV coords: bottom-left→[u0,v1], top-left→[u0,v0], top-right→[u1,v0], bottom-right→[u1,v1]
    let (verts, uvs): ([_; 4], [[f32; 2]; 4]) = match dir {
        // Right (+X)
        0 => (
            [
                [1.0, 0.0, 0.0],
                [1.0, top_height, 0.0],
                [1.0, top_height, 1.0],
                [1.0, 0.0, 1.0],
            ],
            [[u0, v1], [u0, v0], [u1, v0], [u1, v1]],
        ),
        // Left (-X)
        1 => (
            [
                [0.0, 0.0, 1.0],
                [0.0, top_height, 1.0],
                [0.0, top_height, 0.0],
                [0.0, 0.0, 0.0],
            ],
            [[u0, v1], [u0, v0], [u1, v0], [u1, v1]],
        ),
        // Top (+Y)
        2 => (
            [
                [0.0, top_height, 0.0],
                [0.0, top_height, 1.0],
                [1.0, top_height, 1.0],
                [1.0, top_height, 0.0],
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
                [1.0, top_height, 1.0],
                [0.0, top_height, 1.0],
            ],
            [[u0, v1], [u1, v1], [u1, v0], [u0, v0]],
        ),
        // Back (-Z): a=bottom-left (X=1), b=bottom-right (X=0), c=top-right, d=top-left
        5 => (
            [
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
                [0.0, top_height, 0.0],
                [1.0, top_height, 0.0],
            ],
            [[u0, v1], [u1, v1], [u1, v0], [u0, v0]],
        ),
        _ => unreachable!(),
    };

    let uvs = rotate_uvs(uvs, rotation);

    Quad {
        vertices: [
            Vertex {
                position: off(verts[0]),
                uv: uvs[0],
                ao: ao[0],
            },
            Vertex {
                position: off(verts[1]),
                uv: uvs[1],
                ao: ao[1],
            },
            Vertex {
                position: off(verts[2]),
                uv: uvs[2],
                ao: ao[2],
            },
            Vertex {
                position: off(verts[3]),
                uv: uvs[3],
                ao: ao[3],
            },
        ],
    }
}

fn rotate_uvs(mut uvs: [[f32; 2]; 4], rotation: u8) -> [[f32; 2]; 4] {
    for _ in 0..rotation {
        uvs = [uvs[3], uvs[0], uvs[1], uvs[2]];
    }
    uvs
}
