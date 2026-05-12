//! Minimal debug overlay renderer.
//!
//! This intentionally uses a tiny built-in 5x7 bitmap font so F3 diagnostics do
//! not depend on a UI or font asset pipeline yet.

use wgpu::util::DeviceExt;

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
}

impl OverlayRenderer {
    /// Creates the overlay pipeline for the swapchain format.
    pub fn new(device: &wgpu::Device, color_format: wgpu::TextureFormat) -> Self {
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

        Self { pipeline }
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
        let vertices = text_vertices(text, width.max(1) as f32, height.max(1) as f32);
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
}

fn text_vertices(text: &str, width: f32, height: f32) -> Vec<OverlayVertex> {
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
            ch,
            x + SCALE,
            y + SCALE,
            SCALE,
            width,
            height,
            SHADOW,
        );
        push_glyph(&mut vertices, ch, x, y, SCALE, width, height, COLOR);
        x += (GLYPH_W + GAP) * SCALE;
    }

    vertices
}

fn push_glyph(
    vertices: &mut Vec<OverlayVertex>,
    ch: char,
    x: f32,
    y: f32,
    scale: f32,
    width: f32,
    height: f32,
    color: [f32; 4],
) {
    for (row, pattern) in glyph(ch).iter().enumerate() {
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

fn glyph(ch: char) -> [&'static str; 7] {
    match ch {
        '0' => [
            "01110", "10001", "10011", "10101", "11001", "10001", "01110",
        ],
        '1' => [
            "00100", "01100", "00100", "00100", "00100", "00100", "01110",
        ],
        '2' => [
            "01110", "10001", "00001", "00010", "00100", "01000", "11111",
        ],
        '3' => [
            "11110", "00001", "00001", "01110", "00001", "00001", "11110",
        ],
        '4' => [
            "00010", "00110", "01010", "10010", "11111", "00010", "00010",
        ],
        '5' => [
            "11111", "10000", "10000", "11110", "00001", "00001", "11110",
        ],
        '6' => [
            "01110", "10000", "10000", "11110", "10001", "10001", "01110",
        ],
        '7' => [
            "11111", "00001", "00010", "00100", "01000", "01000", "01000",
        ],
        '8' => [
            "01110", "10001", "10001", "01110", "10001", "10001", "01110",
        ],
        '9' => [
            "01110", "10001", "10001", "01111", "00001", "00001", "01110",
        ],
        'A' => [
            "01110", "10001", "10001", "11111", "10001", "10001", "10001",
        ],
        'B' => [
            "11110", "10001", "10001", "11110", "10001", "10001", "11110",
        ],
        'C' => [
            "01111", "10000", "10000", "10000", "10000", "10000", "01111",
        ],
        'D' => [
            "11110", "10001", "10001", "10001", "10001", "10001", "11110",
        ],
        'E' => [
            "11111", "10000", "10000", "11110", "10000", "10000", "11111",
        ],
        'F' => [
            "11111", "10000", "10000", "11110", "10000", "10000", "10000",
        ],
        'G' => [
            "01111", "10000", "10000", "10011", "10001", "10001", "01111",
        ],
        'H' => [
            "10001", "10001", "10001", "11111", "10001", "10001", "10001",
        ],
        'K' => [
            "10001", "10010", "10100", "11000", "10100", "10010", "10001",
        ],
        'L' => [
            "10000", "10000", "10000", "10000", "10000", "10000", "11111",
        ],
        'M' => [
            "10001", "11011", "10101", "10101", "10001", "10001", "10001",
        ],
        'N' => [
            "10001", "11001", "10101", "10011", "10001", "10001", "10001",
        ],
        'P' => [
            "11110", "10001", "10001", "11110", "10000", "10000", "10000",
        ],
        'R' => [
            "11110", "10001", "10001", "11110", "10100", "10010", "10001",
        ],
        'S' => [
            "01111", "10000", "10000", "01110", "00001", "00001", "11110",
        ],
        'U' => [
            "10001", "10001", "10001", "10001", "10001", "10001", "01110",
        ],
        'Y' => [
            "10001", "10001", "01010", "00100", "00100", "00100", "00100",
        ],
        ':' => [
            "00000", "00100", "00100", "00000", "00100", "00100", "00000",
        ],
        '.' => [
            "00000", "00000", "00000", "00000", "00000", "01100", "01100",
        ],
        '/' => [
            "00001", "00010", "00010", "00100", "01000", "01000", "10000",
        ],
        ' ' => [
            "00000", "00000", "00000", "00000", "00000", "00000", "00000",
        ],
        _ => [
            "11111", "00001", "00010", "00100", "00000", "00100", "00100",
        ],
    }
}
