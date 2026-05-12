//! Material and color data used by render pipelines.
//!
//! Materials own the GPU uniform resources needed to shade a mesh. The current
//! material model is intentionally small: one base color multiplied by the
//! vertex tint in the static mesh shader.

use wgpu::util::DeviceExt;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct MaterialUniform {
    base_color: [f32; 4],
}

/// Simple material for static colored geometry.
#[derive(Debug)]
pub struct Material {
    base_color: [f32; 4],
    bind_group: wgpu::BindGroup,
}

impl Material {
    /// Creates a material with a single base color uniform.
    pub fn new(
        device: &wgpu::Device,
        layout: &wgpu::BindGroupLayout,
        label: &str,
        base_color: [f32; 4],
    ) -> Self {
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("{label} material buffer")),
            contents: bytemuck::bytes_of(&MaterialUniform { base_color }),
            usage: wgpu::BufferUsages::UNIFORM,
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some(&format!("{label} material bind group")),
            layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: buffer.as_entire_binding(),
            }],
        });

        Self {
            base_color,
            bind_group,
        }
    }

    /// Base color multiplied with per-vertex tint.
    #[allow(dead_code)]
    pub fn base_color(&self) -> [f32; 4] {
        self.base_color
    }

    /// GPU bind group consumed by the material shader bindings.
    pub fn bind_group(&self) -> &wgpu::BindGroup {
        &self.bind_group
    }
}
