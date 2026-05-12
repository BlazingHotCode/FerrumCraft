//! Mesh data structures.
//!
//! This module owns CPU-side vertex/index definitions and the GPU buffers built
//! from them. Keeping it separate from pipelines prevents mesh layout decisions
//! from leaking into frame orchestration.

use wgpu::util::DeviceExt;

/// One colored vertex in static mesh geometry.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    position: [f32; 3],
    color: [f32; 3],
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
}

impl Mesh {
    /// Creates a simple colored cube used to validate the static 3D render path.
    pub fn cube(device: &wgpu::Device) -> Self {
        let vertices = [
            Vertex {
                position: [-0.5, -0.5, 0.5],
                color: [1.0, 0.2, 0.2],
            },
            Vertex {
                position: [0.5, -0.5, 0.5],
                color: [0.2, 1.0, 0.2],
            },
            Vertex {
                position: [0.5, 0.5, 0.5],
                color: [0.2, 0.2, 1.0],
            },
            Vertex {
                position: [-0.5, 0.5, 0.5],
                color: [1.0, 1.0, 0.2],
            },
            Vertex {
                position: [-0.5, -0.5, -0.5],
                color: [1.0, 0.2, 1.0],
            },
            Vertex {
                position: [0.5, -0.5, -0.5],
                color: [0.2, 1.0, 1.0],
            },
            Vertex {
                position: [0.5, 0.5, -0.5],
                color: [1.0, 0.6, 0.2],
            },
            Vertex {
                position: [-0.5, 0.5, -0.5],
                color: [0.8, 0.8, 0.9],
            },
        ];

        let indices: &[u16] = &[
            0, 1, 2, 0, 2, 3, 1, 5, 6, 1, 6, 2, 5, 4, 7, 5, 7, 6, 4, 0, 3, 4, 3, 7, 3, 2, 6, 3, 6,
            7, 4, 5, 1, 4, 1, 0,
        ];

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Cube vertex buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let index_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Cube index buffer"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });

        Self {
            vertex_buffer,
            index_buffer,
            index_count: indices.len() as u32,
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
}
