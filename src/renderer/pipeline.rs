//! Render pipeline ownership and command encoding.
//!
//! Pipeline setup and render-pass code lives here so `Renderer` can focus on
//! frame lifecycle work and `Scene` can focus on renderable data.

use wgpu::util::DeviceExt;

use super::mesh::{Frustum, Vertex};
use super::scene::Scene;
use super::texture::TextureAtlas;

const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct CameraUniform {
    view_projection: [[f32; 4]; 4],
}

/// Collection of render pipelines used to draw a scene.
///
/// This currently contains one static mesh pipeline. The type provides the seam
/// where future material, opaque, and transparent pipelines can be added.
#[derive(Debug)]
pub struct RenderPipelines {
    opaque_mesh: wgpu::RenderPipeline,
    transparent_mesh: wgpu::RenderPipeline,
    camera_buffer: wgpu::Buffer,
    camera_bind_group: wgpu::BindGroup,
    material_bind_group_layout: wgpu::BindGroupLayout,
    texture_bind_group_layout: wgpu::BindGroupLayout,
    texture_bind_group: Option<wgpu::BindGroup>,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RenderStats {
    pub visible_meshes: usize,
    pub culled_meshes: usize,
}

impl RenderPipelines {
    /// Creates the static mesh pipeline and camera uniform resources.
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Static mesh shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("static_mesh.wgsl").into()),
        });

        let camera_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera uniform buffer"),
            contents: bytemuck::bytes_of(&CameraUniform {
                view_projection: glam::Mat4::IDENTITY.to_cols_array_2d(),
            }),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });

        let camera_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Camera bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let camera_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Camera bind group"),
            layout: &camera_bind_group_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: camera_buffer.as_entire_binding(),
            }],
        });

        let material_bind_group_layout =
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("Material bind group layout"),
                entries: &[wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                }],
            });

        let texture_bind_group_layout = TextureAtlas::bind_group_layout(device);

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Static mesh pipeline layout"),
            bind_group_layouts: &[
                &camera_bind_group_layout,
                &material_bind_group_layout,
                &texture_bind_group_layout,
            ],
            push_constant_ranges: &[],
        });

        let opaque_mesh =
            create_static_mesh_pipeline(device, color_format, &shader, &pipeline_layout, true);
        let transparent_mesh =
            create_static_mesh_pipeline(device, color_format, &shader, &pipeline_layout, false);

        Self {
            opaque_mesh,
            transparent_mesh,
            camera_buffer,
            camera_bind_group,
            material_bind_group_layout,
            texture_bind_group_layout,
            texture_bind_group: None,
        }
    }

    /// Bind group layout required by static mesh materials.
    pub fn material_layout(&self) -> &wgpu::BindGroupLayout {
        &self.material_bind_group_layout
    }

    /// Sets the texture atlas bind group (group 2).
    pub fn set_texture_bind_group(&mut self, bind_group: wgpu::BindGroup) {
        self.texture_bind_group = Some(bind_group);
    }

    /// Encodes the render passes needed to draw `scene` into `view`.
    pub fn encode(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        depth_view: &wgpu::TextureView,
        scene: &Scene,
    ) -> RenderStats {
        let mut stats = RenderStats::default();

        queue.write_buffer(
            &self.camera_buffer,
            0,
            bytemuck::bytes_of(&CameraUniform {
                view_projection: scene.view_projection().to_cols_array_2d(),
            }),
        );

        let frustum = Frustum::from_view_projection(scene.view_projection());

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Opaque mesh render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(scene.clear_color()),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            self.draw_meshes(
                &mut pass,
                &self.opaque_mesh,
                scene.opaque_meshes(),
                &frustum,
                &mut stats,
            );
        }

        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Transparent mesh render pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            self.draw_meshes(
                &mut pass,
                &self.transparent_mesh,
                scene.transparent_meshes(),
                &frustum,
                &mut stats,
            );
        }

        stats
    }

    fn draw_meshes<'a>(
        &'a self,
        pass: &mut wgpu::RenderPass<'a>,
        pipeline: &'a wgpu::RenderPipeline,
        meshes: impl Iterator<Item = &'a super::mesh::Mesh>,
        frustum: &Frustum,
        stats: &mut RenderStats,
    ) {
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &self.camera_bind_group, &[]);

        if let Some(ref tex_bg) = self.texture_bind_group {
            pass.set_bind_group(2, tex_bg, &[]);
        }

        for mesh in meshes {
            if !mesh.bounds().intersects_frustum(frustum) {
                stats.culled_meshes += 1;
                continue;
            }

            stats.visible_meshes += 1;
            pass.set_bind_group(1, mesh.material().bind_group(), &[]);
            pass.set_vertex_buffer(0, mesh.vertex_buffer().slice(..));
            pass.set_index_buffer(mesh.index_buffer().slice(..), wgpu::IndexFormat::Uint16);
            pass.draw_indexed(0..mesh.index_count(), 0, 0..1);
        }
    }
}

fn create_static_mesh_pipeline(
    device: &wgpu::Device,
    color_format: wgpu::TextureFormat,
    shader: &wgpu::ShaderModule,
    pipeline_layout: &wgpu::PipelineLayout,
    depth_write_enabled: bool,
) -> wgpu::RenderPipeline {
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Static mesh pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            buffers: &[Vertex::layout()],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: color_format,
                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        primitive: wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled,
            depth_compare: wgpu::CompareFunction::Less,
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        }),
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    })
}

/// Texture format used by the depth attachment.
pub fn depth_format() -> wgpu::TextureFormat {
    DEPTH_FORMAT
}
