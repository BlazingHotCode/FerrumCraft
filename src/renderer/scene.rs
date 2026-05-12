//! Scene state prepared for rendering.
//!
//! A scene describes what should be rendered, independent of how pipelines draw
//! it or how the swapchain frame is acquired.

use super::mesh::Mesh;
use glam::{Mat4, Vec3};

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
    /// Builds the initial scene contents for the current drawable size.
    pub fn new(
        device: &wgpu::Device,
        material_layout: &wgpu::BindGroupLayout,
        width: u32,
        height: u32,
    ) -> Self {
        Self {
            clear_color: wgpu::Color {
                r: 0.53,
                g: 0.81,
                b: 0.92,
                a: 1.0,
            },
            view_projection: view_projection(width, height),
            meshes: vec![
                Mesh::plane(device, material_layout),
                Mesh::triangle(device, material_layout),
                Mesh::cube(device, material_layout),
            ],
        }
    }

    /// Updates aspect-ratio-sensitive camera projection after a surface resize.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.view_projection = view_projection(width, height);
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

fn view_projection(width: u32, height: u32) -> Mat4 {
    let aspect = width.max(1) as f32 / height.max(1) as f32;
    let projection = Mat4::perspective_rh(45.0_f32.to_radians(), aspect, 0.1, 100.0);
    let view = Mat4::look_at_rh(Vec3::new(2.0, 1.5, 3.0), Vec3::ZERO, Vec3::Y);
    projection * view
}
