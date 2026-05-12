//! Scene state prepared for rendering.
//!
//! A scene describes what should be rendered, independent of how pipelines draw
//! it or how the swapchain frame is acquired.

use super::mesh::Mesh;
use glam::Mat4;

/// Renderable world state for the current frame.
///
/// For now this stores a clear color, built-in debug meshes, and a perspective
/// camera. Lights, materials, and chunk meshes can be added here as the renderer
/// grows.
#[derive(Debug)]
pub struct Scene {
    clear_color: wgpu::Color,
    view_projection: Mat4,
    meshes: Vec<Mesh>,
}

impl Scene {
    /// Builds the initial scene contents for the current camera matrix.
    pub fn new(
        device: &wgpu::Device,
        material_layout: &wgpu::BindGroupLayout,
        view_projection: Mat4,
    ) -> Self {
        Self {
            clear_color: wgpu::Color {
                r: 0.53,
                g: 0.81,
                b: 0.92,
                a: 1.0,
            },
            view_projection,
            meshes: vec![
                Mesh::plane(device, material_layout),
                Mesh::triangle(device, material_layout),
                Mesh::cube(device, material_layout),
            ],
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
    pub fn meshes(&self) -> &[Mesh] {
        &self.meshes
    }
}
