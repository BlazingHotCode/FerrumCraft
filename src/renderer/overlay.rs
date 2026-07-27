//! Debug overlay renderer with a 5x7 bitmap font loaded from a JSON resource.
//!
//! The font file is at `assets/<namespace>/font/5x7.json` and contains glyph
//! definitions for printable ASCII characters. Each glyph is 7 rows of 5 bits.

use std::collections::HashMap;

use wgpu::util::DeviceExt;

use crate::resource::{ResourceCategory, ResourceManager};

/// A 5x7 bitmap font loaded from a JSON resource.
#[derive(Clone, Debug)]
pub struct Font {
    glyphs: HashMap<char, [String; 7]>,
}

impl Font {
    /// Loads a font from a JSON resource file.
    pub fn load(resources: &ResourceManager, namespace: &str) -> Result<Self, FontError> {
        let raw: HashMap<String, Vec<String>> = resources
            .read_json(namespace, ResourceCategory::Font, "5x7.json")
            .map_err(|e| FontError::Load(e.to_string()))?;

        let mut glyphs = HashMap::new();
        for (key, rows) in raw {
            if rows.len() != 7 {
                continue;
            }
            // The key is a JSON string. If it's a single character, use it directly.
            // Some chars might be encoded as unicode escapes - they're already decoded by serde.
            let chars: Vec<char> = key.chars().collect();
            if chars.len() == 1 {
                let arr: [String; 7] = [
                    rows[0].clone(),
                    rows[1].clone(),
                    rows[2].clone(),
                    rows[3].clone(),
                    rows[4].clone(),
                    rows[5].clone(),
                    rows[6].clone(),
                ];
                glyphs.insert(chars[0], arr);
            }
        }

        Ok(Self { glyphs })
    }

    /// Creates an empty font (no glyphs will render).
    pub fn new_empty() -> Self {
        Self {
            glyphs: HashMap::new(),
        }
    }

    /// Number of loaded glyphs.
    pub fn glyph_count(&self) -> usize {
        self.glyphs.len()
    }

    /// Returns the 7-row bitmap for a character, or a fallback for missing chars.
    pub fn glyph(&self, ch: char) -> &[String; 7] {
        use std::sync::OnceLock;
        static FALLBACK: OnceLock<[String; 7]> = OnceLock::new();
        self.glyphs
            .get(&ch)
            .or_else(|| {
                let upper = ch.to_ascii_uppercase();
                if upper != ch {
                    self.glyphs.get(&upper)
                } else {
                    None
                }
            })
            .unwrap_or_else(|| FALLBACK.get_or_init(|| std::array::from_fn(|_| String::new())))
    }
}

/// Errors that can occur during font loading.
#[derive(Debug)]
pub enum FontError {
    Load(String),
}

impl std::fmt::Display for FontError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Load(s) => write!(f, "font loading error: {s}"),
        }
    }
}

impl std::error::Error for FontError {}

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct OverlayVertex {
    position: [f32; 2],
    color: [f32; 4],
}

impl OverlayVertex {
    fn layout() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Self>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

/// Renders text diagnostics over the 3D scene.
#[derive(Debug)]
pub struct OverlayRenderer {
    pipeline: wgpu::RenderPipeline,
    font: Font,
    classic_hud_buffer: wgpu::Buffer,
}

const MAX_CLASSIC_HUD_VERTICES: usize = 32_768;

impl OverlayRenderer {
    /// Creates the overlay pipeline for the swapchain format.
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat, font: Font) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Debug overlay shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("overlay.wgsl").into()),
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Debug overlay pipeline"),
            layout: None,
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[OverlayVertex::layout()],
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
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let classic_hud_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Classic HUD vertex buffer"),
            size: (MAX_CLASSIC_HUD_VERTICES * std::mem::size_of::<OverlayVertex>())
                as wgpu::BufferAddress,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        Self {
            pipeline,
            font,
            classic_hud_buffer,
        }
    }

    pub fn encode_classic_hud(
        &self,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
        text: &str,
        selected: usize,
    ) {
        let width = width.max(1) as f32;
        let height = height.max(1) as f32;
        let mut vertices = classic_text_vertices(&self.font, text, width, height);
        push_classic_crosshair(&mut vertices, width, height);
        push_selected_block(&mut vertices, width, height, selected);
        debug_assert!(vertices.len() <= MAX_CLASSIC_HUD_VERTICES);
        vertices.truncate(MAX_CLASSIC_HUD_VERTICES);
        queue.write_buffer(&self.classic_hud_buffer, 0, bytemuck::cast_slice(&vertices));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Classic HUD pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, self.classic_hud_buffer.slice(..));
        pass.draw(0..vertices.len() as u32, 0..1);
    }

    /// Draws overlay text into the current frame after the 3D pass.
    pub fn encode(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
        text: &str,
    ) {
        let vertices = text_vertices(&self.font, text, width.max(1) as f32, height.max(1) as f32);
        if vertices.is_empty() {
            return;
        }

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Debug overlay vertex buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Debug overlay pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..vertices.len() as u32, 0..1);
    }

    pub fn encode_classic_text(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
        text: &str,
    ) {
        let vertices =
            classic_text_vertices(&self.font, text, width.max(1) as f32, height.max(1) as f32);
        self.encode_vertices(device, encoder, view, "Classic HUD text", &vertices);
    }

    pub fn encode_selected_block(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
        selected: usize,
    ) {
        let width = width.max(1) as f32;
        let height = height.max(1) as f32;
        let cx = width - 30.0;
        let cy = 30.0;
        let color = classic_block_color(selected);
        let mut vertices = Vec::with_capacity(18);
        push_polygon(
            &mut vertices,
            [
                [cx, cy - 13.0],
                [cx + 16.0, cy - 5.0],
                [cx, cy + 3.0],
                [cx - 16.0, cy - 5.0],
            ],
            width,
            height,
            lighten(color, 1.18),
        );
        push_polygon(
            &mut vertices,
            [
                [cx - 16.0, cy - 5.0],
                [cx, cy + 3.0],
                [cx, cy + 20.0],
                [cx - 16.0, cy + 12.0],
            ],
            width,
            height,
            darken(color, 0.62),
        );
        push_polygon(
            &mut vertices,
            [
                [cx, cy + 3.0],
                [cx + 16.0, cy - 5.0],
                [cx + 16.0, cy + 12.0],
                [cx, cy + 20.0],
            ],
            width,
            height,
            darken(color, 0.82),
        );
        self.encode_vertices(device, encoder, view, "Classic selected block", &vertices);
    }

    fn encode_vertices(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        label: &str,
        vertices: &[OverlayVertex],
    ) {
        if vertices.is_empty() {
            return;
        }
        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some(label),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some(label),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..vertices.len() as u32, 0..1);
    }

    /// Draws a full-screen translucent color over the current frame.
    pub fn encode_tint(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        color: [f32; 4],
    ) {
        let vertices = [
            OverlayVertex {
                position: [-1.0, -1.0],
                color,
            },
            OverlayVertex {
                position: [1.0, -1.0],
                color,
            },
            OverlayVertex {
                position: [1.0, 1.0],
                color,
            },
            OverlayVertex {
                position: [-1.0, -1.0],
                color,
            },
            OverlayVertex {
                position: [1.0, 1.0],
                color,
            },
            OverlayVertex {
                position: [-1.0, 1.0],
                color,
            },
        ];

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Screen tint vertex buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Screen tint pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..vertices.len() as u32, 0..1);
    }

    /// Draws a small Minecraft-style center crosshair.
    pub fn encode_crosshair(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
        mining_progress: f32,
    ) {
        let width = width.max(1) as f32;
        let height = height.max(1) as f32;
        let cx = width * 0.5;
        let cy = height * 0.5;
        let color = [0.95, 0.95, 0.95, 0.9];
        let shadow = [0.0, 0.0, 0.0, 0.55];
        let mut vertices = Vec::with_capacity(24);
        push_quad(
            &mut vertices,
            cx - 8.0,
            cy,
            16.0,
            2.0,
            width,
            height,
            shadow,
        );
        push_quad(
            &mut vertices,
            cx,
            cy - 8.0,
            2.0,
            16.0,
            width,
            height,
            shadow,
        );
        push_quad(
            &mut vertices,
            cx - 8.0,
            cy - 1.0,
            16.0,
            1.0,
            width,
            height,
            color,
        );
        push_quad(
            &mut vertices,
            cx - 1.0,
            cy - 8.0,
            1.0,
            16.0,
            width,
            height,
            color,
        );
        if mining_progress > 0.0 {
            let progress = mining_progress.clamp(0.0, 1.0);
            push_quad(
                &mut vertices,
                cx - 18.0,
                cy + 14.0,
                36.0,
                4.0,
                width,
                height,
                [0.0, 0.0, 0.0, 0.75],
            );
            push_quad(
                &mut vertices,
                cx - 17.0,
                cy + 15.0,
                34.0 * progress,
                2.0,
                width,
                height,
                [0.72, 0.72, 0.72, 0.95],
            );
        }

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Crosshair vertex buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Crosshair overlay pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..vertices.len() as u32, 0..1);
    }

    /// Draws a Minecraft-style nine-slot hotbar at the bottom center.
    pub fn encode_hotbar(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
        selected: usize,
        items: [Option<usize>; 9],
        counts: [u32; 9],
    ) {
        let width = width.max(1) as f32;
        let height = height.max(1) as f32;
        let slot = 42.0;
        let gap = 4.0;
        let total_width = slot * 9.0 + gap * 8.0;
        let left = width * 0.5 - total_width * 0.5;
        let top = height - slot - 18.0;
        let mut vertices = Vec::with_capacity(9 * 18);

        for i in 0..9 {
            let x = left + i as f32 * (slot + gap);
            let border = if i == selected {
                [1.0, 1.0, 1.0, 0.95]
            } else {
                [0.15, 0.15, 0.15, 0.85]
            };
            push_quad(&mut vertices, x, top, slot, slot, width, height, border);
            push_quad(
                &mut vertices,
                x + 3.0,
                top + 3.0,
                slot - 6.0,
                slot - 6.0,
                width,
                height,
                [0.08, 0.08, 0.08, 0.72],
            );
            draw_slot_stack(
                &mut vertices,
                &self.font,
                x,
                top,
                slot,
                width,
                height,
                items[i],
                counts[i],
            );
        }

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Hotbar vertex buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Hotbar overlay pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..vertices.len() as u32, 0..1);
    }

    /// Draws a Minecraft-style inventory grid above the hotbar.
    pub fn encode_inventory(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
        hotbar_items: [Option<usize>; 9],
        hotbar_counts: [u32; 9],
        inventory_items: [Option<usize>; 27],
        inventory_counts: [u32; 27],
        carried_item: Option<usize>,
        carried_count: u32,
    ) {
        let width = width.max(1) as f32;
        let height = height.max(1) as f32;
        let slot = 42.0;
        let gap = 4.0;
        let total_width = slot * 9.0 + gap * 8.0;
        let left = width * 0.5 - total_width * 0.5;
        let top = height * 0.5 - 112.0;
        let mut vertices = Vec::new();

        push_quad(
            &mut vertices,
            left - 16.0,
            top - 20.0,
            total_width + 32.0,
            230.0,
            width,
            height,
            [0.06, 0.06, 0.06, 0.88],
        );
        for row in 0..3 {
            for col in 0..9 {
                let index = row * 9 + col;
                let x = left + col as f32 * (slot + gap);
                let y = top + row as f32 * (slot + gap);
                draw_inventory_slot(&mut vertices, x, y, slot, width, height);
                draw_slot_stack(
                    &mut vertices,
                    &self.font,
                    x,
                    y,
                    slot,
                    width,
                    height,
                    inventory_items[index],
                    inventory_counts[index],
                );
            }
        }

        let hotbar_left = width * 0.5 - (slot * 9.0 + gap * 8.0) * 0.5;
        let hotbar_top = top + 3.0 * (slot + gap) + 20.0;
        for col in 0..9 {
            let x = hotbar_left + col as f32 * (slot + gap);
            draw_inventory_slot(&mut vertices, x, hotbar_top, slot, width, height);
            draw_slot_stack(
                &mut vertices,
                &self.font,
                x,
                hotbar_top,
                slot,
                width,
                height,
                hotbar_items[col],
                hotbar_counts[col],
            );
        }

        if carried_count > 0 {
            let x = width * 0.5 + total_width * 0.5 + 28.0;
            let y = top;
            draw_inventory_slot(&mut vertices, x, y, slot, width, height);
            draw_slot_stack(
                &mut vertices,
                &self.font,
                x,
                y,
                slot,
                width,
                height,
                carried_item,
                carried_count,
            );
        }

        let vertex_buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Inventory vertex buffer"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Inventory overlay pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_vertex_buffer(0, vertex_buffer.slice(..));
        pass.draw(0..vertices.len() as u32, 0..1);
    }
}

fn hotbar_item_color(index: usize) -> [f32; 4] {
    match index {
        0 => [0.45, 0.28, 0.16, 1.0],
        1 => [0.42, 0.42, 0.42, 1.0],
        2 => [0.43, 0.27, 0.1, 1.0],
        3 => [0.65, 0.45, 0.23, 1.0],
        _ => [0.65, 0.85, 0.92, 0.8],
    }
}

fn classic_block_color(index: usize) -> [f32; 4] {
    match index {
        0 => [0.48, 0.48, 0.48, 1.0],
        1 => [0.45, 0.30, 0.18, 1.0],
        2 => [0.38, 0.38, 0.38, 1.0],
        3 => [0.66, 0.49, 0.28, 1.0],
        4 => [0.30, 0.62, 0.20, 1.0],
        5 => [0.39, 0.25, 0.12, 1.0],
        6 => [0.28, 0.55, 0.20, 1.0],
        7 => [0.76, 0.70, 0.48, 1.0],
        _ => [0.50, 0.48, 0.45, 1.0],
    }
}

fn push_classic_crosshair(vertices: &mut Vec<OverlayVertex>, width: f32, height: f32) {
    let cx = width * 0.5;
    let cy = height * 0.5;
    let shadow = [0.0, 0.0, 0.0, 0.55];
    let color = [0.95, 0.95, 0.95, 0.9];
    push_quad(vertices, cx - 8.0, cy, 16.0, 2.0, width, height, shadow);
    push_quad(vertices, cx, cy - 8.0, 2.0, 16.0, width, height, shadow);
    push_quad(
        vertices,
        cx - 8.0,
        cy - 1.0,
        16.0,
        1.0,
        width,
        height,
        color,
    );
    push_quad(
        vertices,
        cx - 1.0,
        cy - 8.0,
        1.0,
        16.0,
        width,
        height,
        color,
    );
}

fn push_selected_block(
    vertices: &mut Vec<OverlayVertex>,
    width: f32,
    height: f32,
    selected: usize,
) {
    let cx = width - 30.0;
    let cy = 30.0;
    let color = classic_block_color(selected);
    push_polygon(
        vertices,
        [
            [cx, cy - 13.0],
            [cx + 16.0, cy - 5.0],
            [cx, cy + 3.0],
            [cx - 16.0, cy - 5.0],
        ],
        width,
        height,
        lighten(color, 1.18),
    );
    push_polygon(
        vertices,
        [
            [cx - 16.0, cy - 5.0],
            [cx, cy + 3.0],
            [cx, cy + 20.0],
            [cx - 16.0, cy + 12.0],
        ],
        width,
        height,
        darken(color, 0.62),
    );
    push_polygon(
        vertices,
        [
            [cx, cy + 3.0],
            [cx + 16.0, cy - 5.0],
            [cx + 16.0, cy + 12.0],
            [cx, cy + 20.0],
        ],
        width,
        height,
        darken(color, 0.82),
    );
}

fn lighten(mut color: [f32; 4], amount: f32) -> [f32; 4] {
    for channel in &mut color[..3] {
        *channel = (*channel * amount).min(1.0);
    }
    color
}

fn darken(mut color: [f32; 4], amount: f32) -> [f32; 4] {
    for channel in &mut color[..3] {
        *channel *= amount;
    }
    color
}

fn push_polygon(
    vertices: &mut Vec<OverlayVertex>,
    points: [[f32; 2]; 4],
    width: f32,
    height: f32,
    color: [f32; 4],
) {
    for index in [0, 1, 2, 0, 2, 3] {
        let [x, y] = points[index];
        vertices.push(OverlayVertex {
            position: [x / width * 2.0 - 1.0, 1.0 - y / height * 2.0],
            color,
        });
    }
}

fn classic_text_vertices(font: &Font, text: &str, width: f32, height: f32) -> Vec<OverlayVertex> {
    let mut vertices = Vec::new();
    let mut x = 4.0;
    let mut y = 4.0;
    for ch in text.chars() {
        if ch == '\n' {
            x = 4.0;
            y += 18.0;
            continue;
        }
        push_glyph(
            &mut vertices,
            font,
            ch,
            x + 2.0,
            y + 2.0,
            2.0,
            width,
            height,
            [0.0, 0.0, 0.0, 0.65],
        );
        push_glyph(
            &mut vertices,
            font,
            ch,
            x,
            y,
            2.0,
            width,
            height,
            [1.0, 1.0, 1.0, 1.0],
        );
        x += 12.0;
    }
    vertices
}

fn draw_inventory_slot(
    vertices: &mut Vec<OverlayVertex>,
    x: f32,
    y: f32,
    slot: f32,
    width: f32,
    height: f32,
) {
    push_quad(
        vertices,
        x,
        y,
        slot,
        slot,
        width,
        height,
        [0.18, 0.18, 0.18, 0.95],
    );
    push_quad(
        vertices,
        x + 3.0,
        y + 3.0,
        slot - 6.0,
        slot - 6.0,
        width,
        height,
        [0.08, 0.08, 0.08, 0.9],
    );
}

fn draw_slot_stack(
    vertices: &mut Vec<OverlayVertex>,
    font: &Font,
    x: f32,
    y: f32,
    slot: f32,
    width: f32,
    height: f32,
    item: Option<usize>,
    count: u32,
) {
    let Some(item) = item else {
        return;
    };
    if count == 0 {
        return;
    }

    push_quad(
        vertices,
        x + 13.0,
        y + 13.0,
        slot - 26.0,
        slot - 26.0,
        width,
        height,
        hotbar_item_color(item),
    );
    let text = count.min(999).to_string();
    let text_x = x + slot - 6.0 - text.len() as f32 * 12.0;
    let text_y = y + slot - 14.0;
    for (offset, ch) in text.chars().enumerate() {
        push_glyph(
            vertices,
            font,
            ch,
            text_x + offset as f32 * 12.0 + 1.0,
            text_y + 1.0,
            2.0,
            width,
            height,
            [0.0, 0.0, 0.0, 0.85],
        );
        push_glyph(
            vertices,
            font,
            ch,
            text_x + offset as f32 * 12.0,
            text_y,
            2.0,
            width,
            height,
            [1.0, 1.0, 1.0, 1.0],
        );
    }
}

fn text_vertices(font: &Font, text: &str, width: f32, height: f32) -> Vec<OverlayVertex> {
    const SCALE: f32 = 3.0;
    const GLYPH_W: f32 = 5.0;
    const GLYPH_H: f32 = 7.0;
    const GAP: f32 = 1.0;
    const LEFT: f32 = 12.0;
    const TOP: f32 = 12.0;
    const COLOR: [f32; 4] = [0.92, 0.98, 0.92, 1.0];
    const SHADOW: [f32; 4] = [0.0, 0.0, 0.0, 0.75];

    let mut vertices = Vec::new();
    let mut x = LEFT;
    let mut y = TOP;

    for ch in text.chars() {
        if ch == '\n' {
            x = LEFT;
            y += (GLYPH_H + 2.0) * SCALE;
            continue;
        }

        push_glyph(
            &mut vertices,
            font,
            ch,
            x + SCALE,
            y + SCALE,
            SCALE,
            width,
            height,
            SHADOW,
        );
        push_glyph(&mut vertices, font, ch, x, y, SCALE, width, height, COLOR);
        x += (GLYPH_W + GAP) * SCALE;
    }

    vertices
}

fn push_glyph(
    vertices: &mut Vec<OverlayVertex>,
    font: &Font,
    ch: char,
    x: f32,
    y: f32,
    scale: f32,
    width: f32,
    height: f32,
    color: [f32; 4],
) {
    for (row, pattern) in font.glyph(ch).iter().enumerate() {
        for (col, pixel) in pattern.bytes().enumerate() {
            if pixel == b'1' {
                push_quad(
                    vertices,
                    x + col as f32 * scale,
                    y + row as f32 * scale,
                    scale,
                    scale,
                    width,
                    height,
                    color,
                );
            }
        }
    }
}

fn push_quad(
    vertices: &mut Vec<OverlayVertex>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    width: f32,
    height: f32,
    color: [f32; 4],
) {
    let x0 = x / width * 2.0 - 1.0;
    let x1 = (x + w) / width * 2.0 - 1.0;
    let y0 = 1.0 - y / height * 2.0;
    let y1 = 1.0 - (y + h) / height * 2.0;
    vertices.extend_from_slice(&[
        OverlayVertex {
            position: [x0, y0],
            color,
        },
        OverlayVertex {
            position: [x1, y0],
            color,
        },
        OverlayVertex {
            position: [x1, y1],
            color,
        },
        OverlayVertex {
            position: [x0, y0],
            color,
        },
        OverlayVertex {
            position: [x1, y1],
            color,
        },
        OverlayVertex {
            position: [x0, y1],
            color,
        },
    ]);
}
