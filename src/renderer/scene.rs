//! Scene state prepared for rendering.
//!
//! A scene describes what should be rendered, independent of how pipelines draw
//! it or how the swapchain frame is acquired.

use super::mesh::Mesh;

/// Renderable world state for the current frame.
///
/// For now this stores only the clear color and an empty mesh list. Camera,
/// lights, materials, and chunk meshes can be added here as the renderer grows.
#[derive(Debug)]
pub struct Scene {
    clear_color: wgpu::Color,
    meshes: Vec<Mesh>,
}

impl Scene {
    /// Background color used when beginning the color pass.
    pub fn clear_color(&self) -> wgpu::Color {
        self.clear_color
    }

    /// Meshes submitted for rendering this frame.
    pub fn meshes(&self) -> &[Mesh] {
        &self.meshes
    }
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            clear_color: wgpu::Color {
                r: 0.53,
                g: 0.81,
                b: 0.92,
                a: 1.0,
            },
            meshes: Vec::new(),
        }
    }
}
