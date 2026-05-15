//! Block texture atlas — loads/creates PNG files and packs them into a GPU texture.
//!
//! On first run, 16×16 PNG textures are generated procedurally and saved to
//! `assets/<namespace>/textures/<path>.png`. Once written, they can be edited
//! or replaced by the user — the code will load them as-is on subsequent runs.

use std::collections::{HashMap, HashSet};
use std::fs;
use std::io::{BufReader, BufWriter};
use std::path::PathBuf;
use std::time::{Duration, Instant};

use serde::Deserialize;
use wgpu::util::DeviceExt;

use crate::resource::{ResourceCategory, ResourceManager};

const TEX_SIZE: u32 = 16;
const ATLAS_COLS: u32 = 8;
const WATER_ALPHA: u8 = 210;

pub struct TextureAtlas {
    pub texture: wgpu::Texture,
    pub sampler: wgpu::Sampler,
    pub view: wgpu::TextureView,
    uv_map: HashMap<String, [f32; 4]>,
    animations: Vec<TextureAnimation>,
    last_animation_update: Instant,
}

struct TextureAnimation {
    atlas_x: u32,
    atlas_y: u32,
    frame_time: Duration,
    elapsed: Duration,
    frame_index: usize,
    frames: Vec<Vec<u8>>,
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
        let atlas_w = ATLAS_COLS * TEX_SIZE;
        let mut atlas_pixels = vec![0u8; (ATLAS_COLS * TEX_SIZE * atlas_h * 4) as usize];
        let mut uv_map = HashMap::new();
        let mut animations = Vec::new();

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

            let mut frames = match load_png_frames(&png_path) {
                Ok(f) => f,
                Err(e) => {
                    log::warn!(target: "textures", "Failed to load {}.png: {e}", path);
                    vec![vec![255u8; (TEX_SIZE * TEX_SIZE * 4) as usize]]
                }
            };

            load_texture_meta(resources, namespace, path);

            if *path == "block/water_still" {
                apply_water_overlay(resources, namespace, &mut frames);
            }

            let col = i as u32 % ATLAS_COLS;
            let row = i as u32 / ATLAS_COLS;
            let ox = (col * TEX_SIZE) as usize;
            let oy = (row * TEX_SIZE) as usize;
            let pixels = &frames[0];
            for py in 0..TEX_SIZE as usize {
                for px in 0..TEX_SIZE as usize {
                    let si = (py * TEX_SIZE as usize + px) * 4;
                    let di = ((oy + py) * atlas_w as usize + (ox + px)) * 4;
                    atlas_pixels[di..di + 4].copy_from_slice(&pixels[si..si + 4]);
                }
            }

            if frames.len() > 1 {
                let frame_time = load_animation_frame_time(resources, namespace, path)
                    .unwrap_or(Duration::from_millis(50));
                animations.push(TextureAnimation {
                    atlas_x: col * TEX_SIZE,
                    atlas_y: row * TEX_SIZE,
                    frame_time,
                    elapsed: Duration::ZERO,
                    frame_index: 0,
                    frames,
                });
            }

            let u0 = (col * TEX_SIZE) as f32 / (ATLAS_COLS * TEX_SIZE) as f32;
            let v0 = (row * TEX_SIZE) as f32 / atlas_h as f32;
            let u1 = ((col + 1) * TEX_SIZE) as f32 / (ATLAS_COLS * TEX_SIZE) as f32;
            let v1 = ((row + 1) * TEX_SIZE) as f32 / atlas_h as f32;
            uv_map.insert(path.to_string(), [u0, v0, u1, v1]);
        }

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
            animations,
            last_animation_update: Instant::now(),
        }
    }

    pub fn update_animations(&mut self, queue: &wgpu::Queue) {
        if self.animations.is_empty() {
            return;
        }

        let now = Instant::now();
        let dt = now.saturating_duration_since(self.last_animation_update);
        self.last_animation_update = now;

        for animation in &mut self.animations {
            animation.elapsed += dt;
            if animation.elapsed < animation.frame_time {
                continue;
            }

            while animation.elapsed >= animation.frame_time {
                animation.elapsed -= animation.frame_time;
                animation.frame_index = (animation.frame_index + 1) % animation.frames.len();
            }

            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &self.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d {
                        x: animation.atlas_x,
                        y: animation.atlas_y,
                        z: 0,
                    },
                    aspect: wgpu::TextureAspect::All,
                },
                &animation.frames[animation.frame_index],
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(TEX_SIZE * 4),
                    rows_per_image: Some(TEX_SIZE),
                },
                wgpu::Extent3d {
                    width: TEX_SIZE,
                    height: TEX_SIZE,
                    depth_or_array_layers: 1,
                },
            );
        }
    }

    pub fn uv(&self, path: &str) -> [f32; 4] {
        self.uv_map
            .get(path)
            .copied()
            .unwrap_or([0.0, 0.0, 0.0625, 0.0625])
    }

    pub fn uv_map(&self) -> HashMap<String, [f32; 4]> {
        self.uv_map.clone()
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

fn load_png_frames(path: &PathBuf) -> Result<Vec<Vec<u8>>, String> {
    let file = fs::File::open(path).map_err(|e| format!("open: {e}"))?;
    let r = BufReader::new(file);
    let mut decoder = png::Decoder::new(r);
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    let mut reader = decoder.read_info().map_err(|e| format!("read_info: {e}"))?;
    let mut buf = vec![0u8; reader.output_buffer_size()];
    let info = reader
        .next_frame(&mut buf)
        .map_err(|e| format!("frame: {e}"))?;

    if info.width != TEX_SIZE {
        return Err(format!(
            "expected {}px wide, got {}px",
            TEX_SIZE, info.width
        ));
    }
    if info.height < TEX_SIZE || info.height % TEX_SIZE != 0 {
        return Err(format!(
            "expected height to be a non-zero multiple of {}px, got {}px",
            TEX_SIZE, info.height
        ));
    }

    let bytes = &buf[..info.buffer_size().min(buf.len())];
    let image_size = (info.width * info.height) as usize;
    let mut rgba = match info.color_type {
        png::ColorType::Rgba => bytes.to_vec(),
        png::ColorType::Rgb => {
            let mut rgba = vec![0u8; image_size * 4];
            for i in 0..image_size {
                rgba[i * 4] = bytes[i * 3];
                rgba[i * 4 + 1] = bytes[i * 3 + 1];
                rgba[i * 4 + 2] = bytes[i * 3 + 2];
                rgba[i * 4 + 3] = 255;
            }
            rgba
        }
        png::ColorType::Grayscale => {
            let mut rgba = vec![0u8; image_size * 4];
            for i in 0..image_size {
                let g = bytes[i];
                rgba[i * 4] = g;
                rgba[i * 4 + 1] = g;
                rgba[i * 4 + 2] = g;
                rgba[i * 4 + 3] = 255;
            }
            rgba
        }
        png::ColorType::GrayscaleAlpha => {
            let mut rgba = vec![0u8; image_size * 4];
            for i in 0..image_size {
                let g = bytes[i * 2];
                rgba[i * 4] = g;
                rgba[i * 4 + 1] = g;
                rgba[i * 4 + 2] = g;
                rgba[i * 4 + 3] = bytes[i * 2 + 1];
            }
            rgba
        }
        png::ColorType::Indexed => {
            let palette = reader.info().palette.as_ref().ok_or("missing palette")?;
            let mut rgba = vec![0u8; image_size * 4];
            for i in 0..image_size {
                let idx = bytes[i] as usize;
                if idx * 3 + 2 < palette.len() {
                    rgba[i * 4] = palette[idx * 3];
                    rgba[i * 4 + 1] = palette[idx * 3 + 1];
                    rgba[i * 4 + 2] = palette[idx * 3 + 2];
                    rgba[i * 4 + 3] = 255;
                }
            }
            rgba
        }
    };

    let frame_count = (info.height / TEX_SIZE) as usize;
    if frame_count == 1 {
        return Ok(vec![rgba]);
    }

    let frame_len = (TEX_SIZE * TEX_SIZE * 4) as usize;
    let mut frames = Vec::with_capacity(frame_count);
    for frame in 0..frame_count {
        let start = frame * frame_len;
        let end = start + frame_len;
        frames.push(rgba[start..end].to_vec());
    }
    rgba.clear();
    Ok(frames)
}

#[derive(Deserialize)]
struct AnimationMetaFile {
    animation: Option<AnimationMeta>,
}

#[derive(Deserialize)]
struct AnimationMeta {
    frametime: Option<u64>,
}

#[derive(Deserialize)]
struct TextureMetaFile {
    texture: Option<TextureMeta>,
}

#[derive(Deserialize)]
struct TextureMeta {
    mipmap_strategy: Option<String>,
}

fn load_animation_frame_time(
    resources: &ResourceManager,
    namespace: &str,
    path: &str,
) -> Option<Duration> {
    let meta_path = resources.path(
        namespace,
        ResourceCategory::Texture,
        &format!("{path}.png.mcmeta"),
    );
    let text = fs::read_to_string(meta_path).ok()?;
    let meta: AnimationMetaFile = serde_json::from_str(&text).ok()?;
    let ticks = meta.animation?.frametime.unwrap_or(1).max(1);
    Some(Duration::from_millis(ticks * 50))
}

fn load_texture_meta(resources: &ResourceManager, namespace: &str, path: &str) {
    let meta_path = resources.path(
        namespace,
        ResourceCategory::Texture,
        &format!("{path}.png.mcmeta"),
    );
    let Ok(text) = fs::read_to_string(meta_path) else {
        return;
    };
    let Ok(meta) = serde_json::from_str::<TextureMetaFile>(&text) else {
        return;
    };

    if let Some(strategy) = meta.texture.and_then(|texture| texture.mipmap_strategy) {
        log::debug!(target: "textures", "Texture {path} uses mipmap strategy '{strategy}'");
    }
}

// ── Procedural texture generation (fallback) ──────────────────────────────
fn apply_water_overlay(resources: &ResourceManager, namespace: &str, water_frames: &mut [Vec<u8>]) {
    for frame in water_frames.iter_mut() {
        set_alpha(frame, WATER_ALPHA);
    }

    let overlay_path = resources.path(
        namespace,
        ResourceCategory::Texture,
        "block/water_overlay.png",
    );
    let mut overlay_frames = match load_png_frames(&overlay_path) {
        Ok(frames) => frames,
        Err(e) => {
            log::warn!(target: "textures", "Failed to load water overlay: {e}");
            return;
        }
    };

    let overlay = &mut overlay_frames[0];
    clamp_alpha(overlay, WATER_ALPHA);

    for frame in water_frames {
        alpha_composite(frame, overlay);
        set_alpha(frame, WATER_ALPHA);
    }
}

fn apply_grass_side_overlay(
    resources: &ResourceManager,
    namespace: &str,
    base_frames: &mut [Vec<u8>],
) {
    let overlay_path = resources.path(
        namespace,
        ResourceCategory::Texture,
        "block/grass_block_side_overlay.png",
    );
    let mut overlay_frames = match load_png_frames(&overlay_path) {
        Ok(frames) => frames,
        Err(e) => {
            log::warn!(target: "textures", "Failed to load grass block side overlay: {e}");
            return;
        }
    };

    let overlay = &mut overlay_frames[0];
    for base in base_frames {
        alpha_composite(base, overlay);
    }
}

fn alpha_composite(base: &mut [u8], overlay: &[u8]) {
    for i in 0..(TEX_SIZE * TEX_SIZE) as usize {
        let idx = i * 4;
        let alpha = overlay[idx + 3];
        if alpha == 0 {
            continue;
        }

        base[idx] = alpha_blend_channel(base[idx], overlay[idx], alpha);
        base[idx + 1] = alpha_blend_channel(base[idx + 1], overlay[idx + 1], alpha);
        base[idx + 2] = alpha_blend_channel(base[idx + 2], overlay[idx + 2], alpha);
        base[idx + 3] = base[idx + 3].max(alpha);
    }
}

fn alpha_blend_channel(base: u8, overlay: u8, alpha: u8) -> u8 {
    let alpha = alpha as u16;
    ((overlay as u16 * alpha + base as u16 * (255 - alpha)) / 255) as u8
}

fn set_alpha(rgba: &mut [u8], alpha: u8) {
    for i in 0..(TEX_SIZE * TEX_SIZE) as usize {
        rgba[i * 4 + 3] = alpha;
    }
}

fn clamp_alpha(rgba: &mut [u8], alpha: u8) {
    for i in 0..(TEX_SIZE * TEX_SIZE) as usize {
        let idx = i * 4 + 3;
        rgba[idx] = rgba[idx].min(alpha);
    }
}

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
