//! Chunk mesher — produces vertex/index data from chunks using block model
//! per-face textures.

use std::collections::HashMap;

use crate::block::BlockId;
use crate::model::{BlockModel, Face};
use crate::renderer::Vertex;
use crate::world::{BlockPos, Chunk, World};
use crate::worldgen::{BiomeSource, NoiseSettings};

const SX: usize = 16;
const SY: usize = 64;
const SZ: usize = 16;
const SLICE: usize = SX * SZ;
const LOWERED_WATER_HEIGHT: f32 = 14.0 / 16.0;
const WATER_LEVEL_PROPERTY: u8 = 0;
const WATER_FALLING_LEVEL: u8 = 8;

fn is_transparent(b: &BlockId) -> bool {
    matches!(
        b.0.as_str(),
        "water" | "glass" | "oak_leaves" | "oak_sapling"
    )
}

fn face_visible(current: &BlockId, neighbor: Option<&BlockId>) -> bool {
    let Some(neighbor) = neighbor else {
        return true;
    };

    neighbor.0.is_empty() || (is_transparent(neighbor) && neighbor != current)
}

fn push_quad(verts: &mut Vec<Vertex>, inds: &mut Vec<u32>, off: &mut u32, q: Quad) {
    verts.extend_from_slice(&q.vertices);
    let ao = q.vertices.map(|v| v.ao);
    if ao[0] + ao[2] > ao[1] + ao[3] {
        inds.extend_from_slice(&[*off, *off + 1, *off + 3, *off + 1, *off + 2, *off + 3]);
    } else {
        inds.extend_from_slice(&[*off, *off + 1, *off + 2, *off, *off + 2, *off + 3]);
    }
    *off += 4;
}

fn biome_tint(path: &str, biome: &str) -> [f32; 3] {
    let rgb = match (path, biome) {
        ("block/water_still" | "block/water_flow", "desert") => [0x44, 0xaf, 0xd8],
        ("block/water_still" | "block/water_flow", _) => [0x3f, 0x76, 0xe4],
        ("block/grass_block_top" | "block/grass_block_side_overlay", "desert") => {
            [0xb7, 0xb7, 0x63]
        }
        ("block/grass_block_top" | "block/grass_block_side_overlay", "forest") => {
            [0x79, 0xc0, 0x5a]
        }
        ("block/grass_block_top" | "block/grass_block_side_overlay", "hills") => [0x8a, 0xb6, 0x89],
        ("block/grass_block_top" | "block/grass_block_side_overlay", _) => [0x91, 0xbd, 0x59],
        ("block/oak_leaves", "desert") => [0xa0, 0xa7, 0x55],
        ("block/oak_leaves", "forest") => [0x59, 0xae, 0x30],
        ("block/oak_leaves", "hills") => [0x71, 0x91, 0x5f],
        ("block/oak_leaves", _) => [0x77, 0xab, 0x2f],
        _ => return [1.0, 1.0, 1.0],
    };

    [
        rgb[0] as f32 / 255.0,
        rgb[1] as f32 / 255.0,
        rgb[2] as f32 / 255.0,
    ]
}

fn block_at_world(world: &World, x: i32, y: i32, z: i32) -> Option<&BlockId> {
    if !(0..SY as i32).contains(&y) {
        return None;
    }

    let pos = BlockPos(x, y, z);
    let (lx, ly, lz) = pos.local();
    world
        .chunk(pos.chunk_pos())
        .map(|chunk| &chunk.blocks()[ly * SZ * SX + lz * SX + lx])
}

fn occludes(world: &World, x: i32, y: i32, z: i32, dx: i32, dy: i32, dz: i32) -> bool {
    let ny = y + dy;
    if !(0..SY as i32).contains(&ny) {
        return false;
    }

    let block = block_at_world(world, x + dx, ny, z + dz);
    block.is_some_and(|block| !block.0.is_empty() && !is_transparent(block))
}

fn vertex_ao(
    world: &World,
    x: i32,
    y: i32,
    z: i32,
    normal: [i32; 3],
    side_a: [i32; 3],
    side_b: [i32; 3],
) -> f32 {
    let side_1 = occludes(
        world,
        x,
        y,
        z,
        normal[0] + side_a[0],
        normal[1] + side_a[1],
        normal[2] + side_a[2],
    );
    let side_2 = occludes(
        world,
        x,
        y,
        z,
        normal[0] + side_b[0],
        normal[1] + side_b[1],
        normal[2] + side_b[2],
    );
    let corner = occludes(
        world,
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

fn face_ao(world: &World, x: i32, y: i32, z: i32, dir: u8) -> [f32; 4] {
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
        vertex_ao(world, x, y, z, normal, sides[0], other_sides[0]),
        vertex_ao(world, x, y, z, normal, sides[1], other_sides[1]),
        vertex_ao(world, x, y, z, normal, sides[2], other_sides[2]),
        vertex_ao(world, x, y, z, normal, sides[3], other_sides[3]),
    ]
}

fn face_uv<'a>(
    model: Option<&'a BlockModel>,
    atlas_uv: &HashMap<String, [f32; 4]>,
    face: Face,
) -> ([f32; 4], &'a str) {
    let Some(model) = model else {
        return ([0.0; 4], "");
    };
    let texture = model.texture(face);
    (
        atlas_uv
            .get(texture)
            .copied()
            .unwrap_or([0.0, 0.0, 0.0625, 0.0625]),
        texture,
    )
}

fn grass_side_overlay_offset(dir: u8) -> (f32, f32) {
    let nudge = 0.001;
    match dir {
        0 => (nudge, 0.0),
        1 => (-nudge, 0.0),
        4 => (0.0, nudge),
        5 => (0.0, -nudge),
        _ => (0.0, 0.0),
    }
}

fn water_level(world: &World, pos: BlockPos) -> Option<u8> {
    (world.get_block(pos).0 == "water")
        .then(|| world.get_block_property(pos, WATER_LEVEL_PROPERTY).min(15))
}

fn water_sample_height(world: &World, pos: BlockPos) -> Option<f32> {
    let level = water_level(world, pos)?;
    if water_level(world, BlockPos(pos.0, pos.1 + 1, pos.2)).is_some() {
        return Some(1.0);
    }
    Some(if level == 0 {
        LOWERED_WATER_HEIGHT
    } else if level >= WATER_FALLING_LEVEL {
        1.0
    } else {
        ((8 - level) as f32 / 8.0).max(1.0 / 8.0)
    })
}

fn water_corner_height(world: &World, x: i32, y: i32, z: i32, samples: &[(i32, i32)]) -> f32 {
    let mut total = 0.0;
    let mut count = 0;
    for &(dx, dz) in samples {
        if let Some(height) = water_sample_height(world, BlockPos(x + dx, y, z + dz)) {
            if height >= 1.0 {
                return 1.0;
            }
            total += height;
            count += 1;
        }
    }

    if count == 0 {
        LOWERED_WATER_HEIGHT
    } else {
        total / count as f32
    }
}

fn water_top_heights(world: &World, x: i32, y: i32, z: i32) -> [f32; 4] {
    [
        water_corner_height(world, x, y, z, &[(0, 0), (-1, 0), (0, -1), (-1, -1)]),
        water_corner_height(world, x, y, z, &[(0, 0), (-1, 0), (0, 1), (-1, 1)]),
        water_corner_height(world, x, y, z, &[(0, 0), (1, 0), (0, 1), (1, 1)]),
        water_corner_height(world, x, y, z, &[(0, 0), (1, 0), (0, -1), (1, -1)]),
    ]
}

fn face_vertex_heights(dir: u8, top: [f32; 4]) -> [f32; 4] {
    match dir {
        0 => [0.0, top[3], top[2], 0.0],
        1 => [0.0, top[1], top[0], 0.0],
        2 => top,
        3 => [0.0; 4],
        4 => [0.0, 0.0, top[2], top[1]],
        5 => [0.0, 0.0, top[0], top[3]],
        _ => unreachable!(),
    }
}

/// Output of the mesher.
pub struct MeshData {
    pub opaque: MeshLayer,
    pub transparent: MeshLayer,
}

pub struct MeshLayer {
    pub vertices: Vec<Vertex>,
    pub indices: Vec<u32>,
}

/// Meshes a single chunk using per-face texture UVs from block models.
/// `model_map` maps block path string → BlockModel.
pub fn mesh_chunk(
    chunk: &Chunk,
    world: &World,
    _biome_source: &BiomeSource,
    _noise_settings: &NoiseSettings,
    model_map: &HashMap<String, BlockModel>,
    atlas_uv: &HashMap<String, [f32; 4]>,
) -> MeshData {
    let blocks = chunk.blocks();
    let chunk_origin_x = chunk.pos().0 as f32 * SX as f32;
    let chunk_origin_z = chunk.pos().1 as f32 * SZ as f32;
    let mut opaque_verts = Vec::new();
    let mut opaque_inds = Vec::new();
    let mut opaque_off: u32 = 0;
    let mut transparent_verts = Vec::new();
    let mut transparent_inds = Vec::new();
    let mut transparent_off: u32 = 0;
    let chunk_block_x = chunk.pos().0 * SX as i32;
    let chunk_block_z = chunk.pos().1 * SZ as i32;
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
                let world_x = chunk_block_x + x as i32;
                let world_y = y as i32;
                let world_z = chunk_block_z + z as i32;
                let biome = "plains";
                if block.0 == "oak_sapling" {
                    let (uv, _) = face_uv(model, atlas_uv, Face::Front);
                    push_crossed_sapling(verts, inds, off, fx, fy, fz, uv);
                    continue;
                }
                let top_heights =
                    if block.0 == "water" && (y + 1 >= SY || blocks[idx + SLICE].0 != "water") {
                        water_top_heights(world, world_x, world_y, world_z)
                    } else {
                        [1.0; 4]
                    };

                // Right (+X)
                if face_visible(block, block_at_world(world, world_x + 1, world_y, world_z)) {
                    let (uv, texture) = face_uv(model, atlas_uv, Face::Right);
                    let ao = face_ao(world, world_x, world_y, world_z, 0);
                    let q = quad(
                        0,
                        fx,
                        fy,
                        fz,
                        uv,
                        face_vertex_heights(0, top_heights),
                        ao,
                        biome_tint(texture, biome),
                    );
                    push_quad(verts, inds, off, q);
                    if texture == "block/grass_block_side" {
                        let (dx, dz) = grass_side_overlay_offset(0);
                        let q = quad(
                            0,
                            fx + dx,
                            fy,
                            fz + dz,
                            atlas_uv
                                .get("block/grass_block_side_overlay")
                                .copied()
                                .unwrap_or([0.0, 0.0, 0.0625, 0.0625]),
                            face_vertex_heights(0, top_heights),
                            ao,
                            biome_tint("block/grass_block_side_overlay", biome),
                        );
                        push_quad(verts, inds, off, q);
                    }
                }
                // Left (-X)
                if face_visible(block, block_at_world(world, world_x - 1, world_y, world_z)) {
                    let (uv, texture) = face_uv(model, atlas_uv, Face::Left);
                    let ao = face_ao(world, world_x, world_y, world_z, 1);
                    let q = quad(
                        1,
                        fx,
                        fy,
                        fz,
                        uv,
                        face_vertex_heights(1, top_heights),
                        ao,
                        biome_tint(texture, biome),
                    );
                    push_quad(verts, inds, off, q);
                    if texture == "block/grass_block_side" {
                        let (dx, dz) = grass_side_overlay_offset(1);
                        let q = quad(
                            1,
                            fx + dx,
                            fy,
                            fz + dz,
                            atlas_uv
                                .get("block/grass_block_side_overlay")
                                .copied()
                                .unwrap_or([0.0, 0.0, 0.0625, 0.0625]),
                            face_vertex_heights(1, top_heights),
                            ao,
                            biome_tint("block/grass_block_side_overlay", biome),
                        );
                        push_quad(verts, inds, off, q);
                    }
                }
                // Top (+Y)
                if face_visible(block, block_at_world(world, world_x, world_y + 1, world_z)) {
                    let (uv, texture) = face_uv(model, atlas_uv, Face::Top);
                    let q = quad(
                        2,
                        fx,
                        fy,
                        fz,
                        uv,
                        face_vertex_heights(2, top_heights),
                        face_ao(world, world_x, world_y, world_z, 2),
                        biome_tint(texture, biome),
                    );
                    push_quad(verts, inds, off, q);
                }
                // Bottom (-Y)
                if face_visible(block, block_at_world(world, world_x, world_y - 1, world_z)) {
                    let (uv, texture) = face_uv(model, atlas_uv, Face::Bottom);
                    let q = quad(
                        3,
                        fx,
                        fy,
                        fz,
                        uv,
                        face_vertex_heights(3, top_heights),
                        face_ao(world, world_x, world_y, world_z, 3),
                        biome_tint(texture, biome),
                    );
                    push_quad(verts, inds, off, q);
                }
                // Front (+Z)
                if face_visible(block, block_at_world(world, world_x, world_y, world_z + 1)) {
                    let (uv, texture) = face_uv(model, atlas_uv, Face::Front);
                    let ao = face_ao(world, world_x, world_y, world_z, 4);
                    let q = quad(
                        4,
                        fx,
                        fy,
                        fz,
                        uv,
                        face_vertex_heights(4, top_heights),
                        ao,
                        biome_tint(texture, biome),
                    );
                    push_quad(verts, inds, off, q);
                    if texture == "block/grass_block_side" {
                        let (dx, dz) = grass_side_overlay_offset(4);
                        let q = quad(
                            4,
                            fx + dx,
                            fy,
                            fz + dz,
                            atlas_uv
                                .get("block/grass_block_side_overlay")
                                .copied()
                                .unwrap_or([0.0, 0.0, 0.0625, 0.0625]),
                            face_vertex_heights(4, top_heights),
                            ao,
                            biome_tint("block/grass_block_side_overlay", biome),
                        );
                        push_quad(verts, inds, off, q);
                    }
                }
                // Back (-Z)
                if face_visible(block, block_at_world(world, world_x, world_y, world_z - 1)) {
                    let (uv, texture) = face_uv(model, atlas_uv, Face::Back);
                    let ao = face_ao(world, world_x, world_y, world_z, 5);
                    let q = quad(
                        5,
                        fx,
                        fy,
                        fz,
                        uv,
                        face_vertex_heights(5, top_heights),
                        ao,
                        biome_tint(texture, biome),
                    );
                    push_quad(verts, inds, off, q);
                    if texture == "block/grass_block_side" {
                        let (dx, dz) = grass_side_overlay_offset(5);
                        let q = quad(
                            5,
                            fx + dx,
                            fy,
                            fz + dz,
                            atlas_uv
                                .get("block/grass_block_side_overlay")
                                .copied()
                                .unwrap_or([0.0, 0.0, 0.0625, 0.0625]),
                            face_vertex_heights(5, top_heights),
                            ao,
                            biome_tint("block/grass_block_side_overlay", biome),
                        );
                        push_quad(verts, inds, off, q);
                    }
                }
            }
        }
    }

    MeshData {
        opaque: MeshLayer {
            vertices: opaque_verts,
            indices: opaque_inds,
        },
        transparent: MeshLayer {
            vertices: transparent_verts,
            indices: transparent_inds,
        },
    }
}

#[derive(Clone, Copy)]
struct Quad {
    vertices: [Vertex; 4],
}

fn push_crossed_sapling(
    verts: &mut Vec<Vertex>,
    inds: &mut Vec<u32>,
    off: &mut u32,
    x: f32,
    y: f32,
    z: f32,
    uv: [f32; 4],
) {
    let [u0, v0, u1, v1] = uv;
    let radius = 0.5 * std::f32::consts::SQRT_2;
    for (dx, dz) in [(radius, radius), (radius, -radius)] {
        push_quad(
            verts,
            inds,
            off,
            Quad {
                vertices: [
                    Vertex {
                        position: [x + 0.5 - dx, y, z + 0.5 - dz],
                        uv: [u0, v1],
                        ao: 1.0,
                        tint: [1.0; 3],
                    },
                    Vertex {
                        position: [x + 0.5 - dx, y + 1.0, z + 0.5 - dz],
                        uv: [u0, v0],
                        ao: 1.0,
                        tint: [1.0; 3],
                    },
                    Vertex {
                        position: [x + 0.5 + dx, y + 1.0, z + 0.5 + dz],
                        uv: [u1, v0],
                        ao: 1.0,
                        tint: [1.0; 3],
                    },
                    Vertex {
                        position: [x + 0.5 + dx, y, z + 0.5 + dz],
                        uv: [u1, v1],
                        ao: 1.0,
                        tint: [1.0; 3],
                    },
                ],
            },
        );
    }
}

fn quad(
    dir: u8,
    ox: f32,
    oy: f32,
    oz: f32,
    uv: [f32; 4],
    heights: [f32; 4],
    ao: [f32; 4],
    tint: [f32; 3],
) -> Quad {
    let [u0, v0, u1, v1] = uv;
    let side_v = |height: f32| v1 - (v1 - v0) * height.clamp(0.0, 1.0);
    let off = |v: [f32; 3]| [v[0] + ox, v[1] + oy, v[2] + oz];
    // Each face: (a,b,c,d) where triangles are a→b→c and a→c→d (CCW outside).
    // UV coords: bottom-left→[u0,v1], top-left→[u0,v0], top-right→[u1,v0], bottom-right→[u1,v1]
    let (verts, uvs): ([_; 4], [[f32; 2]; 4]) = match dir {
        // Right (+X)
        0 => (
            [
                [1.0, 0.0, 0.0],
                [1.0, heights[1], 0.0],
                [1.0, heights[2], 1.0],
                [1.0, 0.0, 1.0],
            ],
            [
                [u0, v1],
                [u0, side_v(heights[1])],
                [u1, side_v(heights[2])],
                [u1, v1],
            ],
        ),
        // Left (-X)
        1 => (
            [
                [0.0, 0.0, 1.0],
                [0.0, heights[1], 1.0],
                [0.0, heights[2], 0.0],
                [0.0, 0.0, 0.0],
            ],
            [
                [u0, v1],
                [u0, side_v(heights[1])],
                [u1, side_v(heights[2])],
                [u1, v1],
            ],
        ),
        // Top (+Y)
        2 => (
            [
                [0.0, heights[0], 0.0],
                [0.0, heights[1], 1.0],
                [1.0, heights[2], 1.0],
                [1.0, heights[3], 0.0],
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
                [1.0, heights[2], 1.0],
                [0.0, heights[3], 1.0],
            ],
            [
                [u0, v1],
                [u1, v1],
                [u1, side_v(heights[2])],
                [u0, side_v(heights[3])],
            ],
        ),
        // Back (-Z): a=bottom-left (X=1), b=bottom-right (X=0), c=top-right, d=top-left
        5 => (
            [
                [1.0, 0.0, 0.0],
                [0.0, 0.0, 0.0],
                [0.0, heights[2], 0.0],
                [1.0, heights[3], 0.0],
            ],
            [
                [u0, v1],
                [u1, v1],
                [u1, side_v(heights[2])],
                [u0, side_v(heights[3])],
            ],
        ),
        _ => unreachable!(),
    };

    Quad {
        vertices: [
            Vertex {
                position: off(verts[0]),
                uv: uvs[0],
                ao: ao[0],
                tint,
            },
            Vertex {
                position: off(verts[1]),
                uv: uvs[1],
                ao: ao[1],
                tint,
            },
            Vertex {
                position: off(verts[2]),
                uv: uvs[2],
                ao: ao[2],
                tint,
            },
            Vertex {
                position: off(verts[3]),
                uv: uvs[3],
                ao: ao[3],
                tint,
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dense_edited_chunks_use_indices_beyond_u16() {
        let mut world = World::with_seed(12345);
        for y in 0..SY as i32 {
            for z in 0..SZ as i32 {
                for x in 0..SX as i32 {
                    if (x + y + z) % 2 == 0 {
                        world.set_block(BlockPos(x, y, z), BlockId("stone".to_string()));
                    }
                }
            }
        }
        let model = BlockModel {
            faces: std::array::from_fn(|_| "block/stone".to_string()),
        };
        let models = HashMap::from([("stone".to_string(), model)]);
        let atlas = HashMap::from([("block/stone".to_string(), [0.0, 0.0, 1.0, 1.0])]);
        let chunk = world.chunk(crate::world::ChunkPos(0, 0)).unwrap();
        let mesh = mesh_chunk(
            chunk,
            &world,
            &BiomeSource::demo(),
            &NoiseSettings::demo(),
            &models,
            &atlas,
        );

        assert!(mesh.opaque.vertices.len() > u16::MAX as usize);
        assert!(
            mesh.opaque
                .indices
                .iter()
                .any(|index| *index > u16::MAX as u32)
        );
    }
}
