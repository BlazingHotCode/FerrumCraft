//! Mesh data structures.
//!
//! This module owns CPU-side vertex/index definitions and the GPU buffers built
//! from them. Keeping it separate from pipelines prevents mesh layout decisions
//! from leaking into frame orchestration.

use wgpu::util::DeviceExt;

use super::material::Material;

/// One vertex in static mesh geometry.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub tint: [f32; 3],
}

impl Vertex {
    /// Describes the memory layout consumed by the static mesh shader.
    pub fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x3,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 3]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x3,
                },
            ],
        }
    }
}

/// Renderable indexed geometry owned by a [`Scene`](super::scene::Scene).
#[derive(Debug)]
pub struct Mesh {
    vertex_buffer: wgpu::Buffer,
    index_buffer: wgpu::Buffer,
    index_count: u32,
    material: Material,
}

impl Mesh {
    /// Creates a flat colored triangle debug primitive.
    pub fn triangle(device: &wgpu::Device, material_layout: &wgpu::BindGroupLayout) -> Self {
        let vertices = [
            Vertex {
                position: [-1.4, -0.25, 0.0],
                tint: [1.0, 1.0, 1.0],
            },
            Vertex {
                position: [-0.4, -0.25, 0.0],
                tint: [0.85, 0.85, 0.85],
            },
            Vertex {
                position: [-0.9, 0.75, 0.0],
                tint: [0.7, 0.7, 0.7],
            },
        ];
        let indices: &[u16] = &[0, 1, 2];

        Self::from_vertices(
            device,
            material_layout,
            "Triangle",
            [1.0, 0.25, 0.05, 1.0],
            &vertices,
            indices,
        )
    }

    /// Creates a simple colored cube debug primitive.
    pub fn cube(device: &wgpu::Device, material_layout: &wgpu::BindGroupLayout) -> Self {
        let vertices = [
            Vertex {
                position: [0.25, -0.5, 0.5],
                tint: [1.0, 1.0, 1.0],
            },
            Vertex {
                position: [1.25, -0.5, 0.5],
                tint: [0.95, 0.95, 0.95],
            },
            Vertex {
                position: [1.25, 0.5, 0.5],
                tint: [0.85, 0.85, 0.85],
            },
            Vertex {
                position: [0.25, 0.5, 0.5],
                tint: [0.75, 0.75, 0.75],
            },
            Vertex {
                position: [0.25, -0.5, -0.5],
                tint: [0.65, 0.65, 0.65],
            },
            Vertex {
                position: [1.25, -0.5, -0.5],
                tint: [0.55, 0.55, 0.55],
            },
            Vertex {
                position: [1.25, 0.5, -0.5],
                tint: [0.45, 0.45, 0.45],
            },
            Vertex {
                position: [0.25, 0.5, -0.5],
                tint: [0.35, 0.35, 0.35],
            },
        ];

        let indices: &[u16] = &[
            0, 1, 2, 0, 2, 3, 1, 5, 6, 1, 6, 2, 5, 4, 7, 5, 7, 6, 4, 0, 3, 4, 3, 7, 3, 2, 6, 3, 6,
            7, 4, 5, 1, 4, 1, 0,
        ];

        Self::from_vertices(
            device,
            material_layout,
            "Cube",
            [0.05, 0.95, 0.35, 1.0],
            &vertices,
            indices,
        )
    }

    /// Creates a flat colored plane debug primitive.
    pub fn plane(device: &wgpu::Device, material_layout: &wgpu::BindGroupLayout) -> Self {
        let vertices = [
            Vertex {
                position: [-1.5, -0.65, -1.0],
                tint: [1.0, 1.0, 1.0],
            },
            Vertex {
                position: [1.5, -0.65, -1.0],
                tint: [0.9, 0.9, 0.9],
            },
            Vertex {
                position: [1.5, -0.65, 1.0],
                tint: [0.75, 0.75, 0.75],
            },
            Vertex {
                position: [-1.5, -0.65, 1.0],
                tint: [0.85, 0.85, 0.85],
            },
        ];
        let indices: &[u16] = &[0, 2, 1, 0, 3, 2];

        Self::from_vertices(
            device,
            material_layout,
            "Plane",
            [0.25, 0.3, 1.0, 1.0],
            &vertices,
            indices,
        )
    }

    pub fn from_vertices(
        device: &wgpu::Device,
        material_layout: &wgpu::BindGroupLayout,
        label: &str,
        base_color: [f32; 4],
        vertices: &[Vertex],
        indices: &[u16],
    ) -> Self {
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("{label} vertex buffer")),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(&format!("{label} index buffer")),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            vertex_buffer,
            index_buffer,
            index_count: indices.len() as u32,
            material: Material::new(device, material_layout, label, base_color),
        }
    }

    /// GPU vertex buffer backing this mesh.
    pub fn vertex_buffer(&self) -> &wgpu::Buffer {
        &self.vertex_buffer
    }

    /// GPU index buffer backing this mesh.
    pub fn index_buffer(&self) -> &wgpu::Buffer {
        &self.index_buffer
    }

    /// Number of indices to draw.
    pub fn index_count(&self) -> u32 {
        self.index_count
    }

    /// Material used when drawing this mesh.
    pub fn material(&self) -> &Material {
        &self.material
    }
}
