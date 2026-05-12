//! Render pipeline ownership and command encoding.
//!
//! Pipeline setup and render-pass code lives here so `Renderer` can focus on
//! frame lifecycle work and `Scene` can focus on renderable data.

use super::scene::Scene;

/// Collection of render pipelines used to draw a scene.
///
/// This is currently a zero-sized placeholder because the clear-screen renderer
/// does not need shader pipelines yet. The type provides the seam where future
/// material, depth, opaque, and transparent pipelines can be added.
#[derive(Debug, Default)]
pub struct RenderPipelines;

impl RenderPipelines {
    /// Encodes the render passes needed to draw `scene` into `view`.
    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        scene: &Scene,
    ) {
        {
            let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: None,
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(scene.clear_color()),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Mesh drawing will be added here once meshes carry GPU buffers and
            // a concrete shader pipeline exists.
            for _mesh in scene.meshes() {}
        }
    }
}
