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
    meshes: HashMap<ChunkPos, Mesh>,
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
            meshes: HashMap::new(),
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
    pub fn meshes(&self) -> impl Iterator<Item = &Mesh> {
        self.meshes.values()
    }

    /// Replaces all scene meshes (e.g. when switching from debug shapes to chunks).
    pub fn set_meshes(&mut self, meshes: HashMap<ChunkPos, Mesh>) {
        self.meshes = meshes;
    }

    /// Inserts or replaces one chunk mesh.
    pub fn set_chunk_mesh(&mut self, pos: ChunkPos, mesh: Mesh) {
        self.meshes.insert(pos, mesh);
    }

    /// Removes one chunk mesh.
    pub fn remove_chunk_mesh(&mut self, pos: ChunkPos) {
        self.meshes.remove(&pos);
    }
}
