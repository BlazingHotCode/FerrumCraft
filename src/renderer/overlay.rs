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
}

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

        Self { pipeline, font }
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

    /// Draws a compact five-slot hotbar at the bottom center.
    pub fn encode_hotbar(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        width: u32,
        height: u32,
        selected: usize,
        counts: [u32; 5],
    ) {
        let width = width.max(1) as f32;
        let height = height.max(1) as f32;
        let slot = 42.0;
        let gap = 4.0;
        let total_width = slot * 5.0 + gap * 4.0;
        let left = width * 0.5 - total_width * 0.5;
        let top = height - slot - 18.0;
        let mut vertices = Vec::with_capacity(5 * 18);

        for i in 0..5 {
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
            push_quad(
                &mut vertices,
                x + 13.0,
                top + 13.0,
                slot - 26.0,
                slot - 26.0,
                width,
                height,
                hotbar_item_color(i),
            );
            if counts[i] > 0 {
                let text = counts[i].min(999).to_string();
                let text_x = x + slot - 6.0 - text.len() as f32 * 12.0;
                let text_y = top + slot - 14.0;
                for (offset, ch) in text.chars().enumerate() {
                    push_glyph(
                        &mut vertices,
                        &self.font,
                        ch,
                        text_x + offset as f32 * 12.0 + 1.0,
                        text_y + 1.0,
                        2.0,
                        width,
                        height,
                        [0.0, 0.0, 0.0, 0.85],
                    );
                    push_glyph(
                        &mut vertices,
                        &self.font,
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
