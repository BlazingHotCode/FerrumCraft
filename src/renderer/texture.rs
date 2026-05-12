//! Block texture atlas — loads/creates PNG files and packs them into a GPU texture.
//!
//! On first run, 16×16 PNG textures are generated procedurally and saved to
//! `assets/<namespace>/textures/<path>.png`. Once written, they can be edited
//! or replaced by the user — the code will load them as-is on subsequent runs.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;

use wgpu::util::DeviceExt;

use crate::resource::{ResourceCategory, ResourceManager};

const TEX_SIZE: u32 = 16;
const ATLAS_COLS: u32 = 8;

pub struct TextureAtlas {
    pub texture: wgpu::Texture,
    pub sampler: wgpu::Sampler,
    pub view: wgpu::TextureView,
    uv_map: HashMap<String, [f32; 4]>,
}

impl TextureAtlas {
    pub fn load(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        resources: &ResourceManager,
        namespace: &str,
        texture_paths: &[String],
    ) -> Self {
        let unique: Vec<&str> = {
            let mut seen = HashSet::new();
            texture_paths
                .iter()
                .filter(|p| seen.insert(p.as_str()))
                .map(|s| s.as_str())
                .collect()
        };

        let tex_dir = resources.path(namespace, ResourceCategory::Texture, "");
        if let Some(parent) = tex_dir.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let count = unique.len() as u32;
        let atlas_h = ((count + ATLAS_COLS - 1) / ATLAS_COLS) * TEX_SIZE;
        let mut atlas_pixels = vec![0u8; (ATLAS_COLS * TEX_SIZE * atlas_h * 4) as usize];
        let mut uv_map = HashMap::new();

        for (i, path) in unique.iter().enumerate() {
            let png_path =
                resources.path(namespace, ResourceCategory::Texture, &format!("{path}.png"));

            if !png_path.exists() {
                if let Some(pixels) = procedural_texture(path) {
                    if let Err(e) = write_png(&png_path, &pixels) {
                        log::warn!(target: "textures", "Failed to write {}.png: {e}", path);
                    } else {
                        log::info!(target: "textures", "Generated {}.png", path);
                    }
                }
            }

            let pixels = match load_png(&png_path) {
                Ok(p) => p,
                Err(e) => {
                    log::warn!(target: "textures", "Failed to load {}.png: {e}", path);
                    vec![255u8; (TEX_SIZE * TEX_SIZE * 4) as usize]
                }
            };

            let col = i as u32 % ATLAS_COLS;
            let row = i as u32 / ATLAS_COLS;
            let ox = (col * TEX_SIZE) as usize;
            let oy = (row * TEX_SIZE) as usize;
            for py in 0..TEX_SIZE as usize {
                for px in 0..TEX_SIZE as usize {
                    let si = (py * TEX_SIZE as usize + px) * 4;
                    let di = ((oy + py) * (ATLAS_COLS * TEX_SIZE) as usize + (ox + px)) * 4;
                    atlas_pixels[di..di + 4].copy_from_slice(&pixels[si..si + 4]);
                }
            }

            let u0 = (col * TEX_SIZE) as f32 / (ATLAS_COLS * TEX_SIZE) as f32;
            let v0 = (row * TEX_SIZE) as f32 / atlas_h as f32;
            let u1 = ((col + 1) * TEX_SIZE) as f32 / (ATLAS_COLS * TEX_SIZE) as f32;
            let v1 = ((row + 1) * TEX_SIZE) as f32 / atlas_h as f32;
            uv_map.insert(path.to_string(), [u0, v0, u1, v1]);
        }

        let atlas_w = ATLAS_COLS * TEX_SIZE;
        let texture = device.create_texture_with_data(
            queue,
            &wgpu::TextureDescriptor {
                label: Some("Block texture atlas"),
                size: wgpu::Extent3d {
                    width: atlas_w,
                    height: atlas_h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: wgpu::TextureFormat::Rgba8UnormSrgb,
                usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
                view_formats: &[],
            },
            wgpu::util::TextureDataOrder::MipMajor,
            &atlas_pixels,
        );

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Block texture sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        log::info!(target: "textures", "Loaded {} textures ({}x{} atlas)", count, atlas_w, atlas_h);
        Self {
            texture,
            sampler,
            view,
            uv_map,
        }
    }

    pub fn uv(&self, path: &str) -> [f32; 4] {
        self.uv_map
            .get(path)
            .copied()
            .unwrap_or([0.0, 0.0, 0.0625, 0.0625])
    }

    pub fn bind_group_layout(device: &wgpu::Device) -> wgpu::BindGroupLayout {
        device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("Texture atlas bind group layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        })
    }

    pub fn bind_group(&self, device: &wgpu::Device) -> wgpu::BindGroup {
        let layout = Self::bind_group_layout(device);
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Texture atlas bind group"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(&self.view),
                },
            ],
        })
    }
}

// ── PNG I/O ─────────────────────────────────────────────────────────────────

fn write_png(path: &PathBuf, rgba: &[u8]) -> Result<(), String> {
    let file = fs::File::create(path).map_err(|e| format!("create: {e}"))?;
    let w = BufWriter::new(file);
    let mut encoder = png::Encoder::new(w, TEX_SIZE, TEX_SIZE);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| format!("header: {e}"))?;
    writer
        .write_image_data(rgba)
        .map_err(|e| format!("write: {e}"))?;
    writer.finish().map_err(|e| format!("finish: {e}"))?;
    Ok(())
}

fn load_png(path: &PathBuf) -> Result<Vec<u8>, String> {
    let file = fs::File::open(path).map_err(|e| format!("open: {e}"))?;
    let r = BufReader::new(file);
    let decoder = png::Decoder::new(r);
    let mut reader = decoder.read_info().map_err(|e| format!("read_info: {e}"))?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("frame: {e}"))?;

    if info.width != TEX_SIZE || info.height != TEX_SIZE {
        return Err(format!(
            "expected {}x{}, got {}x{}",
            TEX_SIZE, TEX_SIZE, info.width, info.height
        ));
    }

    let bytes = &buf[..info.buffer_size()];
    let size = (TEX_SIZE * TEX_SIZE) as usize;
    match info.color_type {
        png::ColorType::Rgba => Ok(bytes.to_vec()),
        png::ColorType::Rgb => {
            let mut rgba = vec![0u8; size * 4];
            for i in 0..size {
                rgba[i * 4] = bytes[i * 3];
                rgba[i * 4 + 1] = bytes[i * 3 + 1];
                rgba[i * 4 + 2] = bytes[i * 3 + 2];
                rgba[i * 4 + 3] = 255;
            }
            Ok(rgba)
        }
        png::ColorType::Grayscale => {
            let mut rgba = vec![0u8; size * 4];
            for i in 0..size {
                let g = bytes[i];
                rgba[i * 4] = g;
                rgba[i * 4 + 1] = g;
                rgba[i * 4 + 2] = g;
                rgba[i * 4 + 3] = 255;
            }
            Ok(rgba)
        }
        other => Err(format!("unsupported colour type {other:?}")),
    }
}

// ── Procedural texture generation (fallback) ──────────────────────────────

fn procedural_texture(path: &str) -> Option<Vec<u8>> {
    let name = path.rsplit('/').next().unwrap_or(path);
    match name {
        "stone" => Some(solid(140, 135, 130, 30)),
        "dirt" => Some(solid(115, 85, 55, 25)),
        "grass_block_top" => Some(grass_top()),
        "grass_block_side" => Some(grass_side()),
        "sand" => Some(solid(210, 195, 140, 20)),
        "water_still" => Some(water()),
        "oak_log" => Some(log_side()),
        "oak_log_top" => Some(log_top()),
        "leaves" | "oak_leaves" => Some(leaves()),
        "planks" | "oak_planks" => Some(planks()),
        "glass" => Some(glass()),
        _ => None,
    }
}

fn solid(r: u8, g: u8, b: u8, noise: u8) -> Vec<u8> {
    let mut d = vec![0u8; 256 * 4];
    for i in 0..256 {
        let nn = (i as u32)
            .wrapping_mul(0x9E3779B9)
            .wrapping_add(i as u32 * i as u32 * i as u32);
        let n = ((nn >> 16) as u8) % (noise + 1);
        d[i * 4] = r.saturating_sub(n);
        d[i * 4 + 1] = g.saturating_sub(n);
        d[i * 4 + 2] = b.saturating_sub(n);
        d[i * 4 + 3] = 255;
    }
    d
}

fn grass_top() -> Vec<u8> {
    let mut d = vec![0u8; 256 * 4];
    for y in 0..16 {
        for x in 0..16 {
            let i = (y * 16 + x) * 4;
            let s = (y as u32 * 16 + x as u32).wrapping_mul(3);
            let n = ((s.wrapping_mul(0x9E3779B9).wrapping_add(s * s * s) >> 16) as u8) % 21;
            if y < 5 {
                d[i] = 90 - n / 2;
                d[i + 1] = 160 - n / 2;
                d[i + 2] = 50 - n / 2;
            } else {
                d[i] = 115 - n / 2;
                d[i + 1] = 85 - n / 2;
                d[i + 2] = 55 - n / 2;
            }
            d[i + 3] = 255;
        }
    }
    d
}

fn water() -> Vec<u8> {
    let mut d = vec![0u8; 256 * 4];
    for i in 0..256 {
        let n = ((i as u32).wrapping_mul(5).wrapping_mul(0x9E3779B9) >> 16) as u8 % 11;
        d[i * 4] = 40 - n / 3;
        d[i * 4 + 1] = 60 - n / 3;
        d[i * 4 + 2] = 180 - n / 3;
        d[i * 4 + 3] = 180;
    }
    d
}

fn log_side() -> Vec<u8> {
    let mut d = vec![0u8; 256 * 4];
    for y in 0..16 {
        for x in 0..16 {
            let i = (y * 16 + x) * 4;
            let s = (y as u32 * 16 + x as u32).wrapping_mul(3);
            let n = ((s.wrapping_mul(0x9E3779B9) >> 16) as u8) % 16;
            let streak = (x % 3 == 1) as u8 * 10;
            d[i] = 100 - n + streak;
            d[i + 1] = 70 - n + streak;
            d[i + 2] = 35 - n + streak / 2;
            d[i + 3] = 255;
        }
    }
    d
}

fn grass_side() -> Vec<u8> {
    let mut d = vec![0u8; 256 * 4];
    for y in 0..16 {
        for x in 0..16 {
            let i = (y * 16 + x) * 4;
            let s = (y as u32 * 16 + x as u32).wrapping_mul(3);
            let n = ((s.wrapping_mul(0x9E3779B9).wrapping_add(s * s * s) >> 16) as u8) % 21;
            if y < 3 {
                d[i] = 60 - n / 2;
                d[i + 1] = 120 - n / 2;
                d[i + 2] = 40 - n / 2;
            } else {
                d[i] = 115 - n / 2;
                d[i + 1] = 85 - n / 2;
                d[i + 2] = 55 - n / 2;
            }
            d[i + 3] = 255;
        }
    }
    d
}

fn log_top() -> Vec<u8> {
    let mut d = vec![0u8; 256 * 4];
    for y in 0..16 {
        for x in 0..16 {
            let i = (y * 16 + x) * 4;
            let dist = (x as i32 - 8).abs().max((y as i32 - 8).abs()) as u8;
            let ring = (dist % 4 == 0) as u8 * 20;
            let s = (y as u32 * 16 + x as u32).wrapping_mul(3);
            let n = ((s.wrapping_mul(0x9E3779B9) >> 16) as u8) % 10;
            d[i] = 130 - n - ring;
            d[i + 1] = 95 - n - ring;
            d[i + 2] = 55 - n - ring;
            d[i + 3] = 255;
        }
    }
    d
}

fn leaves() -> Vec<u8> {
    let mut d = vec![0u8; 256 * 4];
    for i in 0..256 {
        let n = ((i as u32).wrapping_mul(7).wrapping_mul(0x9E3779B9) >> 16) as u8 % 31;
        d[i * 4] = 30 - n / 3;
        d[i * 4 + 1] = 120 - n / 3;
        d[i * 4 + 2] = 30 - n / 3;
        d[i * 4 + 3] = 200;
    }
    d
}

fn planks() -> Vec<u8> {
    let mut d = vec![0u8; 256 * 4];
    for y in 0..16 {
        for x in 0..16 {
            let i = (y * 16 + x) * 4;
            let s = (y as u32 * 16 + x as u32).wrapping_mul(3);
            let n = ((s.wrapping_mul(0x9E3779B9) >> 16) as u8) % 11;
            let line = (y % 4 == 0) as u8 * 15;
            d[i] = 160 - n - line;
            d[i + 1] = 120 - n - line;
            d[i + 2] = 70 - n - line;
            d[i + 3] = 255;
        }
    }
    d
}

fn glass() -> Vec<u8> {
    let mut d = vec![0u8; 256 * 4];
    for i in 0..256 {
        let n = ((i as u32).wrapping_mul(3).wrapping_mul(0x9E3779B9) >> 16) as u8 % 6;
        d[i * 4] = 200 - n;
        d[i * 4 + 1] = 220 - n;
        d[i * 4 + 2] = 240 - n;
        d[i * 4 + 3] = 100;
    }
    d
}
