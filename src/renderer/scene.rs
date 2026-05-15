//! Scene state prepared for rendering.
//!
//! A scene describes what should be rendered, independent of how pipelines draw
//! it or how the swapchain frame is acquired.

use super::mesh::Mesh;
use crate::world::ChunkPos;
use glam::Mat4;
use std::collections::HashMap;

/// Renderable world state for the current frame.
///
/// For now this stores a clear color, built-in debug meshes, and a perspective
/// camera. Lights, materials, and chunk meshes can be added here as the renderer
/// grows.
#[derive(Debug)]
pub struct Scene {
    clear_color: wgpu::Color,
    view_projection: Mat4,
    opaque_meshes: HashMap<ChunkPos, Mesh>,
    transparent_meshes: HashMap<ChunkPos, Mesh>,
}

impl Scene {
    /// Builds an empty scene with a clear colour and no meshes.
    pub fn new(
        device: &wgpu::Device,
        material_layout: &wgpu::BindGroupLayout,
        view_projection: Mat4,
    ) -> Self {
        let _ = (device, material_layout);
        Self {
            clear_color: wgpu::Color {
                r: 0.53,
                g: 0.81,
                b: 0.92,
                a: 1.0,
            },
            view_projection,
            opaque_meshes: HashMap::new(),
            transparent_meshes: HashMap::new(),
        }
    }

    /// Updates the camera matrix used for rendering.
    pub fn set_view_projection(&mut self, view_projection: Mat4) {
        self.view_projection = view_projection;
    }

    /// Background color used when beginning the color pass.
    pub fn clear_color(&self) -> wgpu::Color {
        self.clear_color
    }

    /// Combined camera view/projection matrix used by the static mesh shader.
    pub fn view_projection(&self) -> Mat4 {
        self.view_projection
    }

    /// Meshes submitted for rendering this frame.
    pub fn opaque_meshes(&self) -> impl Iterator<Item = &Mesh> {
        self.opaque_meshes.values()
    }

    /// Transparent meshes submitted for rendering after opaque geometry.
    pub fn transparent_meshes(&self) -> impl Iterator<Item = &Mesh> {
        self.transparent_meshes.values()
    }

    /// Replaces all scene meshes (e.g. when switching from debug shapes to chunks).
    pub fn set_meshes(
        &mut self,
        opaque_meshes: HashMap<ChunkPos, Mesh>,
        transparent_meshes: HashMap<ChunkPos, Mesh>,
    ) {
        self.opaque_meshes = opaque_meshes;
        self.transparent_meshes = transparent_meshes;
    }

    /// Inserts or removes one chunk's opaque mesh.
    pub fn set_opaque_chunk_mesh(&mut self, pos: ChunkPos, mesh: Option<Mesh>) {
        if let Some(mesh) = mesh {
            self.opaque_meshes.insert(pos, mesh);
        } else {
            self.opaque_meshes.remove(&pos);
        }
    }

    /// Inserts or removes one chunk's transparent mesh.
    pub fn set_transparent_chunk_mesh(&mut self, pos: ChunkPos, mesh: Option<Mesh>) {
        if let Some(mesh) = mesh {
            self.transparent_meshes.insert(pos, mesh);
        } else {
            self.transparent_meshes.remove(&pos);
        }
    }

    /// Removes one chunk mesh.
    pub fn remove_chunk_mesh(&mut self, pos: ChunkPos) {
        self.opaque_meshes.remove(&pos);
        self.transparent_meshes.remove(&pos);
    }
}
