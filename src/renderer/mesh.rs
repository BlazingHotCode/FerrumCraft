//! Mesh data structures.
//!
//! This module owns CPU-side vertex/index definitions and the GPU buffers built
//! from them. Keeping it separate from pipelines prevents mesh layout decisions
//! from leaking into frame orchestration.

use wgpu::util::DeviceExt;

use super::material::Material;
use glam::{Mat4, Vec3, Vec4};

/// One vertex in static mesh geometry.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vertex {
    pub position: [f32; 3],
    pub uv: [f32; 2],
    pub ao: f32,
    pub tint: [f32; 3],
}

impl Vertex {
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
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 5]>() as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 6]>() as wgpu::BufferAddress,
                    shader_location: 3,
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
    bounds: Bounds,
}

#[derive(Clone, Copy, Debug)]
pub struct Bounds {
    min: Vec3,
    max: Vec3,
}

impl Bounds {
    fn from_vertices(vertices: &[Vertex]) -> Self {
        let mut min = Vec3::splat(f32::INFINITY);
        let mut max = Vec3::splat(f32::NEG_INFINITY);
        for vertex in vertices {
            let position = Vec3::from(vertex.position);
            min = min.min(position);
            max = max.max(position);
        }

        Self { min, max }
    }

    pub fn intersects_frustum(&self, frustum: &Frustum) -> bool {
        frustum.contains_bounds(*self)
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Frustum {
    planes: [Vec4; 6],
}

impl Frustum {
    pub fn from_view_projection(view_projection: Mat4) -> Self {
        let m = view_projection;
        let row = |i: usize| Vec4::new(m.x_axis[i], m.y_axis[i], m.z_axis[i], m.w_axis[i]);
        let planes = [
            normalize_plane(row(3) + row(0)),
            normalize_plane(row(3) - row(0)),
            normalize_plane(row(3) + row(1)),
            normalize_plane(row(3) - row(1)),
            normalize_plane(row(3) + row(2)),
            normalize_plane(row(3) - row(2)),
        ];

        Self { planes }
    }

    fn contains_bounds(&self, bounds: Bounds) -> bool {
        for plane in self.planes {
            let positive = Vec3::new(
                if plane.x >= 0.0 {
                    bounds.max.x
                } else {
                    bounds.min.x
                },
                if plane.y >= 0.0 {
                    bounds.max.y
                } else {
                    bounds.min.y
                },
                if plane.z >= 0.0 {
                    bounds.max.z
                } else {
                    bounds.min.z
                },
            );

            if plane.truncate().dot(positive) + plane.w < 0.0 {
                return false;
            }
        }

        true
    }
}

fn normalize_plane(plane: Vec4) -> Vec4 {
    let len = plane.truncate().length();
    if len > 0.0 { plane / len } else { plane }
}

impl Mesh {
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

        let bounds = Bounds::from_vertices(vertices);

        Self {
            vertex_buffer,
            index_buffer,
            index_count: indices.len() as u32,
            material: Material::new(device, material_layout, label, base_color),
            bounds,
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

    /// World-space bounds used for frustum culling.
    pub fn bounds(&self) -> Bounds {
        self.bounds
    }
}
