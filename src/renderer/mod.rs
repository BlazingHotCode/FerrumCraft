//! Rendering entry point.
//!
//! `Renderer` owns the GPU device, swapchain surface, scene state, and render
//! pipelines. It coordinates frame acquisition and submission, while the
//! submodules own scene data, mesh data, and render-pass encoding details.

mod mesh;
mod pipeline;
mod scene;

use std::sync::Arc;

use pipeline::RenderPipelines;
use scene::Scene;
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
    scene: Scene,
    pipelines: RenderPipelines,
}

impl Renderer {
    /// Creates the surface, selects an adapter, and initializes renderer state.
    pub async fn new(window: Arc<Window>) -> Self {
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

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .expect("Failed to request device");

        let config = surface
            .get_default_config(&adapter, size.width.max(1), size.height.max(1))
            .expect("Failed to get default surface config");
        surface.configure(&device, &config);

        Self {
            device,
            queue,
            surface,
            config,
            scene: Scene::default(),
            pipelines: RenderPipelines::default(),
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
    }

    /// Encodes and presents one frame.
    pub fn render(&self) {
        let frame = self
            .surface
            .get_current_texture()
            .expect("Failed to acquire swap chain texture");
        let view = frame
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor::default());

        self.pipelines.encode(&mut encoder, &view, &self.scene);

        self.queue.submit(std::iter::once(encoder.finish()));
        frame.present();
    }
}
