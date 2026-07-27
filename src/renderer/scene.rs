//! Scene state prepared for rendering.
//!
//! A scene describes what should be rendered, independent of how pipelines draw
//! it or how the swapchain frame is acquired.

use super::mesh::Mesh;
use crate::world::ChunkPos;
use glam::{Mat4, Vec3};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub enum FogEnvironment {
    #[default]
    Air,
    Water,
    Lava,
}

/// Renderable world state for the current frame.
///
/// For now this stores a clear color, built-in debug meshes, and a perspective
/// camera. Lights, materials, and chunk meshes can be added here as the renderer
/// grows.
#[derive(Debug)]
pub struct Scene {
    clear_color: wgpu::Color,
    view_projection: Mat4,
    camera_position: Vec3,
    camera_forward: Vec3,
    fog_distance: f32,
    fog_environment: FogEnvironment,
    opaque_meshes: HashMap<ChunkPos, Mesh>,
    transparent_meshes: HashMap<ChunkPos, Mesh>,
    destroy_overlay_mesh: Option<Mesh>,
    classic_mob_mesh: Option<Mesh>,
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
                r: 0.92,
                g: 0.98,
                b: 1.0,
                a: 1.0,
            },
            view_projection,
            camera_position: Vec3::ZERO,
            camera_forward: Vec3::Z,
            fog_distance: 1024.0,
            fog_environment: FogEnvironment::Air,
            opaque_meshes: HashMap::new(),
            transparent_meshes: HashMap::new(),
            destroy_overlay_mesh: None,
            classic_mob_mesh: None,
        }
    }

    /// Updates the camera matrix used for rendering.
    pub fn set_view_projection(&mut self, view_projection: Mat4) {
        self.view_projection = view_projection;
    }

    pub fn set_camera(
        &mut self,
        view_projection: Mat4,
        position: Vec3,
        forward: Vec3,
        fog_distance: f32,
    ) {
        self.view_projection = view_projection;
        self.camera_position = position;
        self.camera_forward = forward;
        self.fog_distance = fog_distance;
    }

    pub fn set_fog_environment(&mut self, environment: FogEnvironment) {
        self.fog_environment = environment;
    }

    /// Background color used when beginning the color pass.
    pub fn clear_color(&self) -> wgpu::Color {
        self.clear_color
    }

    /// Combined camera view/projection matrix used by the static mesh shader.
    pub fn view_projection(&self) -> Mat4 {
        self.view_projection
    }

    pub fn camera_position(&self) -> Vec3 {
        self.camera_position
    }

    pub fn camera_forward(&self) -> Vec3 {
        self.camera_forward
    }

    pub fn fog_distance(&self) -> f32 {
        self.fog_distance
    }

    pub fn fog_environment(&self) -> FogEnvironment {
        self.fog_environment
    }

    /// Meshes submitted for rendering this frame.
    pub fn opaque_meshes(&self) -> impl Iterator<Item = &Mesh> {
        self.opaque_meshes
            .values()
            .chain(self.classic_mob_mesh.iter())
    }

    /// Transparent meshes submitted for rendering after opaque geometry.
    pub fn transparent_meshes(&self) -> impl Iterator<Item = &Mesh> {
        self.transparent_meshes
            .values()
            .chain(self.destroy_overlay_mesh.iter())
    }

    /// Inserts or removes the active block-breaking overlay mesh.
    pub fn set_destroy_overlay_mesh(&mut self, mesh: Option<Mesh>) {
        self.destroy_overlay_mesh = mesh;
    }

    pub fn set_classic_mob_mesh(&mut self, mesh: Option<Mesh>) {
        self.classic_mob_mesh = mesh;
    }

    pub fn classic_mob_mesh_mut(&mut self) -> Option<&mut Mesh> {
        self.classic_mob_mesh.as_mut()
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

    /// Returns whether either render layer has a mesh for this chunk.
    pub fn has_chunk_mesh(&self, pos: ChunkPos) -> bool {
        self.opaque_meshes.contains_key(&pos) || self.transparent_meshes.contains_key(&pos)
    }
}
