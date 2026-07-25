//! Rendering entry point.
//!
//! `Renderer` owns the GPU device, swapchain surface, scene state, and render
//! pipelines. It coordinates frame acquisition and submission, while the
//! submodules own scene data, mesh data, and render-pass encoding details.

pub use mesh::{Mesh, Vertex};
pub use overlay::Font;
pub use pipeline::RenderStats;
pub use texture::TextureAtlas;

mod material;
mod mesh;
mod overlay;
mod pipeline;
mod scene;
mod texture;

use std::sync::Arc;

use glam::Mat4;
use overlay::OverlayRenderer;
use pipeline::{RenderPipelines, depth_format};
use scene::Scene;
use std::collections::HashMap;
use winit::window::Window;

/// Coordinates GPU resources and per-frame rendering.
///
/// This type intentionally keeps window/surface orchestration separate from
/// scene contents and pipeline encoding so future renderer features can grow in
/// the relevant module instead of accumulating here.
pub struct Renderer {
    /// GPU device used to create buffers, textures, and pipeline resources.
    pub device: wgpu::Device,
    /// Submission queue for completed command buffers and future resource uploads.
    pub queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    depth_view: wgpu::TextureView,
    scene: Scene,
    pipelines: RenderPipelines,
    overlay: OverlayRenderer,
    /// Texture atlas loaded from block model texture references.
    pub atlas: TextureAtlas,
}

impl Renderer {
    /// Creates the surface, selects an adapter, and initializes renderer state.
    pub async fn new(
        window: Arc<Window>,
        view_projection: Mat4,
        font: Font,
        resources: &crate::resource::ResourceManager,
        namespace: &str,
        texture_paths: &[String],
    ) -> Self {
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let size = window.inner_size();
        let surface = instance
            .create_surface(window)
            .expect("Failed to create surface");

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("Failed to request adapter");

        let info = adapter.get_info();
        log::info!(target: "renderer", "Adapter: {} ({:?})", info.name, info.backend);

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .expect("Failed to request device");

        let config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .expect("Failed to get default surface config");
        log::info!(target: "renderer", "Surface config: {}x{} {:?}", config.width, config.height, config.format);
        surface.configure(&device, &config);

        let mut pipelines = RenderPipelines::new(&device, config.format);
        let atlas = TextureAtlas::load(&device, &queue, resources, namespace, texture_paths);
        let atlas_bg = atlas.bind_group(&device);
        pipelines.set_texture_bind_group(atlas_bg);
        log::info!(target: "renderer", "Texture atlas configured for rendering");
        let overlay = OverlayRenderer::new(&device, config.format, font);
        let depth_view = create_depth_view(&device, config.width, config.height);
        let scene = Scene::new(&device, pipelines.material_layout(), view_projection);

        Self {
            device,
            queue,
            surface,
            config,
            depth_view,
            scene,
            pipelines,
            overlay,
            atlas,
        }
    }

    /// Reconfigures the swapchain for the latest window size.
    ///
    /// WGPU surfaces cannot be configured with zero dimensions, so minimized
    /// windows are clamped to a 1x1 drawable surface.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.device, &self.config);
        self.depth_view = create_depth_view(&self.device, self.config.width, self.config.height);
    }

    /// Updates the camera matrix used by the scene shader uniforms.
    pub fn set_view_projection(&mut self, view_projection: Mat4) {
        self.scene.set_view_projection(view_projection);
    }

    /// Bind group layout required for chunk mesh materials.
    pub fn material_layout(&self) -> &wgpu::BindGroupLayout {
        self.pipelines.material_layout()
    }

    /// Replaces the scene meshes with chunk meshes built from the provided data.
    pub fn set_chunk_meshes(
        &mut self,
        opaque_meshes: HashMap<crate::world::ChunkPos, Mesh>,
        transparent_meshes: HashMap<crate::world::ChunkPos, Mesh>,
    ) {
        self.scene.set_meshes(opaque_meshes, transparent_meshes);
    }

    /// Inserts or removes one chunk's opaque mesh.
    pub fn set_opaque_chunk_mesh(&mut self, pos: crate::world::ChunkPos, mesh: Option<Mesh>) {
        self.scene.set_opaque_chunk_mesh(pos, mesh);
    }

    /// Inserts or removes one chunk's transparent mesh.
    pub fn set_transparent_chunk_mesh(&mut self, pos: crate::world::ChunkPos, mesh: Option<Mesh>) {
        self.scene.set_transparent_chunk_mesh(pos, mesh);
    }

    /// Removes one chunk mesh.
    pub fn remove_chunk_mesh(&mut self, pos: crate::world::ChunkPos) {
        self.scene.remove_chunk_mesh(pos);
    }

    /// Returns whether this chunk currently has renderable mesh data.
    pub fn has_chunk_mesh(&self, pos: crate::world::ChunkPos) -> bool {
        self.scene.has_chunk_mesh(pos)
    }

    /// Updates the Minecraft-style crack overlay drawn on the block being mined.
    pub fn set_destroy_overlay(&mut self, target: Option<crate::world::BlockPos>, progress: f32) {
        let Some(pos) = target else {
            self.scene.set_destroy_overlay_mesh(None);
            return;
        };

        if progress <= 0.0 {
            self.scene.set_destroy_overlay_mesh(None);
            return;
        }

        let stage = ((progress.clamp(0.0, 0.999) * 10.0).floor() as usize).min(9);
        let uv = self.atlas.uv(&format!("block/destroy_stage_{stage}"));
        let (vertices, indices) = destroy_overlay_geometry(pos, uv);
        let mesh = Mesh::from_vertices(
            &self.device,
            self.pipelines.material_layout(),
            "Destroy overlay mesh",
            [1.0, 1.0, 1.0, 0.75],
            &vertices,
            &indices,
        );
        self.scene.set_destroy_overlay_mesh(Some(mesh));
    }

    /// Encodes and presents one frame.
    pub fn render(
        &mut self,
        debug_text: Option<&str>,
        screen_tint: Option<[f32; 4]>,
        hotbar_selected: usize,
        hotbar_items: [Option<usize>; 9],
        hotbar_counts: [u32; 9],
        inventory_open: bool,
        inventory_items: [Option<usize>; 27],
        inventory_counts: [u32; 27],
        carried_item: Option<usize>,
        carried_count: u32,
        mining_progress: f32,
    ) -> Result<RenderStats, wgpu::SurfaceError> {
        self.atlas.update_animations(&self.queue);

        let frame = self.surface.get_current_texture()?;
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        let stats = self.pipelines.encode(
            &self.queue,
            &mut encoder,
            &view,
            &self.depth_view,
            &self.scene,
        );

        if let Some(color) = screen_tint {
            self.overlay
                .encode_tint(&self.device, &mut encoder, &view, color);
        }

        self.overlay.encode_crosshair(
            &self.device,
            &mut encoder,
            &view,
            self.config.width,
            self.config.height,
            mining_progress,
        );

        self.overlay.encode_hotbar(
            &self.device,
            &mut encoder,
            &view,
            self.config.width,
            self.config.height,
            hotbar_selected,
            hotbar_items,
            hotbar_counts,
        );

        if inventory_open {
            self.overlay.encode_inventory(
                &self.device,
                &mut encoder,
                &view,
                self.config.width,
                self.config.height,
                hotbar_items,
                hotbar_counts,
                inventory_items,
                inventory_counts,
                carried_item,
                carried_count,
            );
        }

        if let Some(text) = debug_text {
            self.overlay.encode(
                &self.device,
                &mut encoder,
                &view,
                self.config.width,
                self.config.height,
                text,
            );
        }

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
        Ok(stats)
    }
}

fn destroy_overlay_geometry(pos: crate::world::BlockPos, uv: [f32; 4]) -> (Vec<Vertex>, Vec<u16>) {
    let [u0, v0, u1, v1] = uv;
    let inflate = 0.002;
    let min = [
        pos.0 as f32 - 8.5 - inflate,
        pos.1 as f32 - inflate,
        pos.2 as f32 - 8.5 - inflate,
    ];
    let max = [
        pos.0 as f32 - 8.5 + 1.0 + inflate,
        pos.1 as f32 + 1.0 + inflate,
        pos.2 as f32 - 8.5 + 1.0 + inflate,
    ];
    let mut vertices = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    let mut push_face = |corners: [[f32; 3]; 4]| {
        let base = vertices.len() as u16;
        let uvs = [[u0, v1], [u0, v0], [u1, v0], [u1, v1]];
        for (position, uv) in corners.into_iter().zip(uvs) {
            vertices.push(Vertex {
                position,
                uv,
                ao: 1.0,
                tint: [1.0, 1.0, 1.0],
            });
        }
        indices.extend_from_slice(&[base, base + 1, base + 2, base, base + 2, base + 3]);
    };

    push_face([
        [max[0], min[1], min[2]],
        [max[0], max[1], min[2]],
        [max[0], max[1], max[2]],
        [max[0], min[1], max[2]],
    ]);
    push_face([
        [min[0], min[1], max[2]],
        [min[0], max[1], max[2]],
        [min[0], max[1], min[2]],
        [min[0], min[1], min[2]],
    ]);
    push_face([
        [min[0], max[1], min[2]],
        [min[0], max[1], max[2]],
        [max[0], max[1], max[2]],
        [max[0], max[1], min[2]],
    ]);
    push_face([
        [min[0], min[1], max[2]],
        [min[0], min[1], min[2]],
        [max[0], min[1], min[2]],
        [max[0], min[1], max[2]],
    ]);
    push_face([
        [min[0], min[1], max[2]],
        [max[0], min[1], max[2]],
        [max[0], max[1], max[2]],
        [min[0], max[1], max[2]],
    ]);
    push_face([
        [max[0], min[1], min[2]],
        [min[0], min[1], min[2]],
        [min[0], max[1], min[2]],
        [max[0], max[1], min[2]],
    ]);

    (vertices, indices)
}

fn create_depth_view(device: &wgpu::Device, width: u32, height: u32) -> wgpu::TextureView {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Depth texture"),
        size: wgpu::Extent3d {
            width: width.max(1),
            height: height.max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: depth_format(),
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });

    texture.create_view(&wgpu::TextureViewDescriptor::default())
}
