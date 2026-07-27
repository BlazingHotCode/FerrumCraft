//! FerrumCraft application entry point.
//!
//! This module wires together the winit application lifecycle, window creation,
//! and renderer frame loop. Game state should move into dedicated modules as it
//! grows; this file should stay focused on top-level orchestration.

// Many data/resource types are defined but not yet consumed by the running app.
// Remove this once gameplay systems use the registries and resource manager.
#![allow(dead_code)]

mod block;
mod camera;
mod classic_worldgen;
mod debug;
mod id;
mod input;
mod lang;
mod logging;
mod mesher;
mod model;
mod registry;
mod renderer;
mod resource;
mod tag;
mod window;
mod world;
mod worldgen;

use std::collections::{HashMap, HashSet, VecDeque};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, mpsc};
use std::thread;
use std::time::{Duration, Instant};

use camera::FirstPersonCamera;
use debug::DebugOverlay;
use glam::Vec3;
use input::InputState;
use renderer::Font;
use winit::application::ApplicationHandler;
use winit::event::DeviceEvent;
use winit::event::MouseButton;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::event_loop::ControlFlow;
use winit::event_loop::EventLoop;
use winit::keyboard::KeyCode;

const GAME_TICK_RATE: u32 = 20;
const FIXED_TIMESTEP: Duration = Duration::from_nanos(1_000_000_000 / GAME_TICK_RATE as u64);
const MAX_FIXED_STEPS_PER_FRAME: u32 = 5;
const CLASSIC_GROUND_ACCELERATION: f32 = 0.1;
const CLASSIC_AIR_ACCELERATION: f32 = 0.02;
const CLASSIC_JUMP_VELOCITY: f32 = 0.42;
const CLASSIC_GRAVITY: f32 = 0.08;
const PLAYER_HEIGHT: f32 = 1.8;
const PLAYER_EYE_HEIGHT: f32 = 1.62;
const PLAYER_CROUCH_EYE_HEIGHT: f32 = 1.35;
const PLAYER_RADIUS: f32 = 0.3;
const BLOCK_REACH: f32 = 5.0;
const BLOCK_RAY_STEP: f32 = 0.05;
const HOTBAR_BLOCKS: [&str; 9] = [
    "stone",
    "dirt",
    "cobblestone",
    "oak_planks",
    "oak_sapling",
    "oak_log",
    "oak_leaves",
    "sand",
    "gravel",
];
const HOTBAR_SIZE: usize = 9;
const ITEM_TYPE_COUNT: usize = HOTBAR_BLOCKS.len();
const INVENTORY_SIZE: usize = 27;
const MAX_STACK_SIZE: u32 = 64;
const AUTOSAVE_INTERVAL: Duration = Duration::from_secs(30);
const MIN_RENDER_DISTANCE_CHUNKS: i32 = 0;
const MAX_RENDER_DISTANCE_CHUNKS: i32 = 16;
const DEFAULT_RENDER_DISTANCE_CHUNKS: i32 = 4;
const DEMO_WORLD_SEED: u64 = 12_345;
const DEMO_SPAWN_CHUNK_RADIUS: i32 = 1;
const CHUNKS_GENERATED_PER_TICK: usize = 1;
const MAX_PENDING_CHUNK_GENERATIONS: usize = 2;
const GENERATED_CHUNKS_INTEGRATED_PER_TICK: usize = 1;
const GENERATED_MESHES_INTEGRATED_PER_TICK: usize = 1;
const CHUNK_MESH_REBUILDS_PER_TICK: usize = 1;
const UNLOAD_MARGIN_CHUNKS: i32 = 2;
const CLASSIC_WORLD_SIZE: i32 = 512;
const CLASSIC_WORLD_CHUNKS: i32 = CLASSIC_WORLD_SIZE / world::CHUNK_SIZE_X as i32;
const CLASSIC_ACTION_INTERVAL: f32 = 0.25;
const CLASSIC_SAVE_VERSION: u32 = 6;
const CLASSIC_FAR_PLANES: [f32; 4] = [1024.0, 256.0, 64.0, 16.0];
const CLASSIC_CHUNK_RADII: [i32; 4] = [4, 4, 2, 1];
const WATER_LEVEL_PROPERTY: u8 = 0;
const WATER_SOURCE_LEVEL: u8 = 0;
const WATER_MAX_HORIZONTAL_LEVEL: u8 = 7;
const WATER_FALLING_LEVEL: u8 = 8;
const WATER_MAX_LEVEL: u8 = 15;
const WATER_SOURCE_SEARCH_LIMIT: usize = 7;
const WATER_SLOPE_FIND_DISTANCE: usize = 4;
const WATER_FLOW_DELAY_TICKS: u64 = 5;
const WATER_UPDATES_PER_TICK: usize = 128;

/// Top-level application state owned by the winit event loop.
///
/// Window and renderer creation is deferred until `resumed` because winit
/// requires platform window handles to be created from an active event loop.
struct App {
    window: Option<window::Window>,
    renderer: Option<renderer::Renderer>,
    camera: Option<FirstPersonCamera>,
    world: world::World,
    biomes: crate::registry::Registry<worldgen::Biome>,
    biome_source: worldgen::BiomeSource,
    noise_settings: worldgen::NoiseSettings,
    classic_terrain: Arc<classic_worldgen::ClassicTerrain>,
    chunk_generation_tx: Option<mpsc::Sender<world::ChunkPos>>,
    generated_chunk_rx: Option<mpsc::Receiver<GeneratedChunk>>,
    mesh_generation_tx: Option<mpsc::Sender<MeshJob>>,
    generated_mesh_rx: Option<mpsc::Receiver<GeneratedMesh>>,
    pending_chunk_generations: HashSet<world::ChunkPos>,
    pending_mesh_rebuilds: VecDeque<world::ChunkPos>,
    queued_mesh_rebuilds: HashSet<world::ChunkPos>,
    pending_mesh_generations: HashSet<world::ChunkPos>,
    font: Option<Font>,
    block_models: Option<crate::registry::Registry<model::BlockModel>>,
    input: InputState,
    debug_overlay: DebugOverlay,
    pointer_locked: bool,
    last_update: Instant,
    last_frame_update: Instant,
    fixed_update_accumulator: Duration,
    player_velocity: Vec3,
    player_grounded: bool,
    player_jump_latched: bool,
    player_crouching: bool,
    hotbar_selected: usize,
    hotbar_slots: [InventorySlot; HOTBAR_SIZE],
    inventory_slots: [InventorySlot; INVENTORY_SIZE],
    carried_slot: InventorySlot,
    inventory_open: bool,
    inventory_toggle_held: bool,
    mining_target: Option<world::BlockPos>,
    mining_progress: f32,
    classic_action_cooldown: f32,
    classic_spawn_position: Vec3,
    classic_view_distance: usize,
    saved_player_position: Option<Vec3>,
    player_overlap_recovery_pending: bool,
    last_save: Instant,
    render_distance_chunks: i32,
    mesh_center_chunk: world::ChunkPos,
    water_tick: u64,
    pending_water_updates: VecDeque<(world::BlockPos, u64)>,
    queued_water_updates: HashMap<world::BlockPos, u64>,
    classic_mobs: Vec<ClassicMob>,
    classic_mob_random: classic_worldgen::JavaRandom,
}

struct GeneratedChunk {
    pos: world::ChunkPos,
    chunk: world::Chunk,
    mesh: mesher::MeshData,
}

struct MeshJob {
    pos: world::ChunkPos,
    chunks: Vec<world::Chunk>,
}

struct GeneratedMesh {
    pos: world::ChunkPos,
    mesh: mesher::MeshData,
}

struct ClassicMob {
    position: Vec3,
    previous_position: Vec3,
    velocity: Vec3,
    heading: f32,
    turn_velocity: f32,
    time_offset: f32,
    grounded: bool,
}

#[derive(Clone, Copy, Debug)]
struct BlockTarget {
    block_pos: world::BlockPos,
    place_pos: world::BlockPos,
}

#[derive(Clone, Copy, Debug, Default, serde::Deserialize, serde::Serialize)]
struct InventorySlot {
    item: Option<usize>,
    count: u32,
}

impl InventorySlot {
    fn is_empty(&self) -> bool {
        self.item.is_none() || self.count == 0
    }
}

#[derive(serde::Deserialize, serde::Serialize)]
struct SaveGame {
    #[serde(default)]
    format_version: u32,
    seed: u64,
    player: SavePlayer,
    #[serde(default)]
    mobs: Vec<SaveMob>,
    chunks: Vec<SaveChunk>,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct SaveMob {
    position: [f32; 3],
    velocity: [f32; 3],
    heading: f32,
    turn_velocity: f32,
    time_offset: f32,
    grounded: bool,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct SavePlayer {
    position: [f32; 3],
    #[serde(default)]
    spawn_position: Option<[f32; 3]>,
    hotbar_selected: usize,
    #[serde(default)]
    hotbar_counts: Vec<u32>,
    #[serde(default)]
    hotbar_slots: Vec<InventorySlot>,
    #[serde(default)]
    inventory_slots: Vec<InventorySlot>,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct SaveChunk {
    pos: world::ChunkPos,
    runs: Vec<SaveBlockRun>,
    #[serde(default)]
    properties: Vec<SaveBlockProperty>,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct SaveBlockRun {
    block: block::BlockId,
    len: usize,
}

#[derive(serde::Deserialize, serde::Serialize)]
struct SaveBlockProperty {
    index: usize,
    prop: u8,
    value: u8,
}

enum InventorySlotTarget {
    Hotbar(usize),
    Inventory(usize),
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let w = window::Window::new(event_loop).expect("Failed to create window");
        let size = w.inner.inner_size();
        let mut camera = FirstPersonCamera::new(size.width, size.height);
        camera.set_position(
            self.saved_player_position
                .take()
                .unwrap_or(self.classic_spawn_position),
        );
        let font = self
            .font
            .take()
            .expect("Font must be loaded before renderer creation");

        // Recreate resource manager and collect texture paths from block models.
        let resources = resource::ResourceManager::new(".");
        let block_models = self.block_models.as_ref().expect("Block models not loaded");
        let texture_paths: Vec<String> = {
            let mut paths: Vec<String> = Vec::new();
            let mut seen = std::collections::HashSet::new();
            for (_, model) in block_models.iter() {
                for face in &model::ALL_FACES {
                    let tex = model.texture(*face);
                    if !tex.is_empty() && seen.insert(tex.to_string()) {
                        paths.push(tex.to_string());
                    }
                }
            }
            for stage in 0..10 {
                let path = format!("block/destroy_stage_{stage}");
                if seen.insert(path.clone()) {
                    paths.push(path);
                }
            }
            paths
        };

        let renderer = pollster::block_on(renderer::Renderer::new(
            w.inner.clone(),
            camera.view_projection(),
            font,
            &resources,
            "ferrumcraft",
            &texture_paths,
        ));
        let model_map: std::collections::HashMap<String, model::BlockModel> = block_models
            .iter()
            .map(|(id, model)| (id.path().to_string(), model.clone()))
            .collect();
        let atlas_uv = renderer.atlas.uv_map();
        let (chunk_generation_tx, generated_chunk_rx) = start_chunk_generation_worker(
            self.world.seed(),
            Arc::clone(&self.classic_terrain),
            self.noise_settings,
            self.biome_source,
            model_map.clone(),
            atlas_uv.clone(),
        );
        let (mesh_generation_tx, generated_mesh_rx) = start_mesh_generation_worker(
            self.world.seed(),
            self.noise_settings,
            self.biome_source,
            model_map,
            atlas_uv,
        );
        self.chunk_generation_tx = Some(chunk_generation_tx);
        self.generated_chunk_rx = Some(generated_chunk_rx);
        self.mesh_generation_tx = Some(mesh_generation_tx);
        self.generated_mesh_rx = Some(generated_mesh_rx);
        self.window = Some(w);
        self.renderer = Some(renderer);
        self.camera = Some(camera);
        self.debug_overlay.set_world_seed(self.world.seed());
        self.rebuild_chunk_meshes(true);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        let is_app_window = self
            .window
            .as_ref()
            .map(|window| window.id() == window_id)
            .unwrap_or(false);
        if !is_app_window {
            return;
        }

        self.input.handle_window_event(&event);
        self.update_hotbar_selection();
        self.update_hotbar_scroll();
        self.handle_classic_keys();

        match event {
            WindowEvent::CloseRequested => {
                self.save_game();
                event_loop.exit();
            }
            WindowEvent::Focused(false) => {
                self.set_pointer_locked(false);
            }
            WindowEvent::KeyboardInput { .. }
                if self
                    .input
                    .was_key_just_pressed(winit::keyboard::KeyCode::Escape) =>
            {
                self.set_pointer_locked(false);
            }
            WindowEvent::MouseInput {
                state: winit::event::ElementState::Pressed,
                button: MouseButton::Left | MouseButton::Right,
                ..
            } => {
                self.set_pointer_locked(true);
            }
            WindowEvent::Resized(size) => {
                if let Some(camera) = &mut self.camera {
                    camera.resize(size.width, size.height);
                }
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size.width, size.height);
                    if let Some(camera) = &self.camera {
                        renderer.set_camera(
                            camera.view_projection(),
                            camera.position(),
                            camera.forward(),
                            camera.far_plane(),
                        );
                    }
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                self.update_rendered_mobs();
                if let Some(renderer) = &mut self.renderer {
                    let frame_start = Instant::now();
                    let classic_text = self.debug_overlay.classic_text();
                    if let Some(camera) = &self.camera {
                        renderer.set_fog_environment(camera_fog_environment(
                            &self.world,
                            camera.position(),
                        ));
                    }
                    match renderer.render(&classic_text, self.hotbar_selected) {
                        Ok(stats) => {
                            self.debug_overlay
                                .set_render_stats(stats.visible_meshes, stats.culled_meshes);
                            self.debug_overlay.record_frame(frame_start.elapsed());
                            self.input.end_frame();
                        }
                        Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                            log::warn!(target: "renderer", "Surface lost, reconfiguring");
                            let Some(window) = &self.window else {
                                return;
                            };
                            let size = window.inner.inner_size();
                            renderer.resize(size.width, size.height);
                            window.request_redraw();
                        }
                        Err(wgpu::SurfaceError::OutOfMemory) => {
                            log::error!(target: "renderer", "Out of graphics memory, exiting");
                            event_loop.exit();
                        }
                        Err(wgpu::SurfaceError::Timeout) => {}
                        Err(wgpu::SurfaceError::Other) => {}
                    }
                }
            }
            _ => {}
        }
    }

    fn device_event(
        &mut self,
        _event_loop: &ActiveEventLoop,
        _device_id: winit::event::DeviceId,
        event: DeviceEvent,
    ) {
        if self.pointer_locked {
            self.input.handle_device_event(&event);
            let delta = self.input.take_cursor_delta();
            if let (Some(camera), Some(renderer)) = (&mut self.camera, &mut self.renderer) {
                camera.apply_mouse_delta(delta);
                renderer.set_camera(
                    camera.view_projection(),
                    camera.position(),
                    camera.forward(),
                    camera.far_plane(),
                );
                self.debug_overlay.set_player_position(camera.position());
            }
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        let now = Instant::now();
        let frame_dt = now
            .saturating_duration_since(self.last_frame_update)
            .min(Duration::from_millis(100));
        self.last_frame_update = now;

        self.handle_block_interaction(frame_dt);
        self.run_fixed_updates();
        if self.last_save.elapsed() >= AUTOSAVE_INTERVAL {
            self.save_game();
        }

        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }
}

impl App {
    fn run_fixed_updates(&mut self) {
        let now = Instant::now();
        self.fixed_update_accumulator += now.saturating_duration_since(self.last_update);
        self.last_update = now;

        let mut steps = 0;
        while self.fixed_update_accumulator >= FIXED_TIMESTEP && steps < MAX_FIXED_STEPS_PER_FRAME {
            self.fixed_update(FIXED_TIMESTEP);
            self.fixed_update_accumulator -= FIXED_TIMESTEP;
            steps += 1;
        }

        if steps == MAX_FIXED_STEPS_PER_FRAME {
            self.fixed_update_accumulator = Duration::ZERO;
        }
    }

    fn fixed_update(&mut self, dt: Duration) {
        // Match Minecraft's 20 ticks-per-second simulation rate while rendering
        // stays independent and can run at a higher frame rate.
        self.water_tick = self.water_tick.wrapping_add(1);
        if self.pointer_locked {
            self.update_player_movement(dt);
        }
        self.update_classic_mobs();
        let mut camera_position = None;
        if let (Some(camera), Some(renderer)) = (&mut self.camera, &mut self.renderer) {
            renderer.set_camera(
                camera.view_projection(),
                camera.position(),
                camera.forward(),
                camera.far_plane(),
            );
            let position = camera.position();
            camera_position = Some(position);
            self.debug_overlay.set_player_position(position);
            self.debug_overlay.set_facing(camera.facing_name());
            self.debug_overlay
                .set_render_distance(self.render_distance_chunks);
        }
        if let Some(position) = camera_position {
            self.update_debug_biome(position);
            let center_chunk = camera_chunk_pos(position);
            let water_changed = self.process_water_updates();
            let integrated = self.integrate_generated_chunks();
            let integrated_meshes = self.integrate_generated_meshes(center_chunk);
            let rebuilt = self.process_pending_mesh_rebuilds(center_chunk);
            let generated = self.generate_missing_chunks_around(center_chunk);
            let unloaded = self.unload_far_chunks(center_chunk);
            self.remove_chunk_meshes_outside_render_distance(center_chunk);
            if water_changed > 0
                || integrated > 0
                || integrated_meshes > 0
                || rebuilt > 0
                || generated > 0
                || unloaded > 0
            {
                return;
            }
        }
        self.mesh_center_chunk = camera_position
            .map(camera_chunk_pos)
            .unwrap_or(self.mesh_center_chunk);
    }

    fn generate_missing_chunks_around(&mut self, center_chunk: world::ChunkPos) -> usize {
        if self.pending_chunk_generations.len() >= MAX_PENDING_CHUNK_GENERATIONS {
            return 0;
        }
        let mut requested = 0;
        let radius = self.render_distance_chunks.max(0);

        'outer: for distance in 0..=radius {
            for chunk_x in center_chunk.0 - distance..=center_chunk.0 + distance {
                for chunk_z in center_chunk.1 - distance..=center_chunk.1 + distance {
                    if (chunk_x - center_chunk.0)
                        .abs()
                        .max((chunk_z - center_chunk.1).abs())
                        != distance
                    {
                        continue;
                    }

                    let chunk_pos = world::ChunkPos(chunk_x, chunk_z);
                    if self.world.is_chunk_loaded(chunk_pos) {
                        let needs_mesh = self
                            .renderer
                            .as_ref()
                            .is_some_and(|renderer| !renderer.has_chunk_mesh(chunk_pos));
                        if needs_mesh {
                            self.queue_chunk_mesh_rebuild(chunk_pos);
                            requested += 1;
                            if requested >= CHUNKS_GENERATED_PER_TICK {
                                break 'outer;
                            }
                        }
                        continue;
                    }
                    if self.pending_chunk_generations.contains(&chunk_pos) {
                        continue;
                    }

                    if self.world.is_chunk_cached(chunk_pos) {
                        self.world.load_chunk(chunk_pos);
                        self.queue_chunk_meshes_near(chunk_pos);
                    } else {
                        let Some(chunk_generation_tx) = &self.chunk_generation_tx else {
                            continue;
                        };
                        if chunk_generation_tx.send(chunk_pos).is_err() {
                            continue;
                        }
                        self.pending_chunk_generations.insert(chunk_pos);
                    }
                    requested += 1;
                    if requested >= CHUNKS_GENERATED_PER_TICK {
                        break 'outer;
                    }
                }
            }
        }

        requested
    }

    fn integrate_generated_chunks(&mut self) -> usize {
        let mut integrated = 0;
        while integrated < GENERATED_CHUNKS_INTEGRATED_PER_TICK {
            let generated = {
                let Some(generated_chunk_rx) = &self.generated_chunk_rx else {
                    return 0;
                };
                generated_chunk_rx.try_recv()
            };
            let Ok(generated) = generated else {
                break;
            };

            self.pending_chunk_generations.remove(&generated.pos);
            if self.world.is_chunk_loaded(generated.pos)
                || self.world.is_chunk_cached(generated.pos)
            {
                continue;
            }

            let pos = generated.pos;
            self.world.insert_generated_chunk(generated.chunk);
            self.set_chunk_mesh_from_data(pos, generated.mesh);
            self.queue_chunk_meshes_near(pos);
            integrated += 1;
        }

        integrated
    }

    fn process_pending_mesh_rebuilds(&mut self, center_chunk: world::ChunkPos) -> usize {
        let mut submitted = 0;
        while submitted < CHUNK_MESH_REBUILDS_PER_TICK {
            let Some(chunk_pos) = self.pending_mesh_rebuilds.pop_front() else {
                break;
            };
            self.queued_mesh_rebuilds.remove(&chunk_pos);

            let distance = (chunk_pos.0 - center_chunk.0)
                .abs()
                .max((chunk_pos.1 - center_chunk.1).abs());
            if distance > self.render_distance_chunks || !self.world.is_chunk_loaded(chunk_pos) {
                if let Some(renderer) = &mut self.renderer {
                    renderer.remove_chunk_mesh(chunk_pos);
                }
                continue;
            }

            let Some(job) = self.create_mesh_job(chunk_pos) else {
                continue;
            };
            let Some(mesh_generation_tx) = &self.mesh_generation_tx else {
                continue;
            };
            if mesh_generation_tx.send(job).is_err() {
                continue;
            }
            self.pending_mesh_generations.insert(chunk_pos);
            submitted += 1;
        }

        submitted
    }

    fn integrate_generated_meshes(&mut self, center_chunk: world::ChunkPos) -> usize {
        let mut integrated = 0;
        while integrated < GENERATED_MESHES_INTEGRATED_PER_TICK {
            let generated = {
                let Some(generated_mesh_rx) = &self.generated_mesh_rx else {
                    return 0;
                };
                generated_mesh_rx.try_recv()
            };
            let Ok(generated) = generated else {
                break;
            };

            self.pending_mesh_generations.remove(&generated.pos);
            if self.queued_mesh_rebuilds.contains(&generated.pos) {
                self.pending_mesh_rebuilds.push_front(generated.pos);
                continue;
            }
            let distance = (generated.pos.0 - center_chunk.0)
                .abs()
                .max((generated.pos.1 - center_chunk.1).abs());
            if distance > self.render_distance_chunks || !self.world.is_chunk_loaded(generated.pos)
            {
                if let Some(renderer) = &mut self.renderer {
                    renderer.remove_chunk_mesh(generated.pos);
                }
                continue;
            }

            self.set_chunk_mesh_from_data(generated.pos, generated.mesh);
            integrated += 1;
        }

        integrated
    }

    fn create_mesh_job(&self, pos: world::ChunkPos) -> Option<MeshJob> {
        let mut chunks = Vec::with_capacity(5);
        for chunk_pos in [
            pos,
            world::ChunkPos(pos.0 + 1, pos.1),
            world::ChunkPos(pos.0 - 1, pos.1),
            world::ChunkPos(pos.0, pos.1 + 1),
            world::ChunkPos(pos.0, pos.1 - 1),
        ] {
            if let Some(chunk) = self.world.chunk(chunk_pos) {
                chunks.push(chunk.clone());
            }
        }

        chunks
            .iter()
            .any(|chunk| chunk.pos() == pos)
            .then_some(MeshJob { pos, chunks })
    }

    fn unload_far_chunks(&mut self, center_chunk: world::ChunkPos) -> usize {
        let unload_distance = self.render_distance_chunks + UNLOAD_MARGIN_CHUNKS;
        let mut unloaded = 0;
        for chunk_pos in self.world.chunk_positions() {
            let distance = (chunk_pos.0 - center_chunk.0)
                .abs()
                .max((chunk_pos.1 - center_chunk.1).abs());
            if distance > unload_distance && self.world.unload_chunk(chunk_pos) {
                if let Some(renderer) = &mut self.renderer {
                    renderer.remove_chunk_mesh(chunk_pos);
                }
                self.queued_mesh_rebuilds.remove(&chunk_pos);
                unloaded += 1;
            }
        }
        unloaded
    }

    fn update_player_movement(&mut self, dt: Duration) {
        let Some(camera) = &self.camera else {
            return;
        };

        let _ = dt;
        let mut position = camera.position();
        if !aabb_chunks_loaded(&self.world, Aabb::player(position)) {
            return;
        }
        if self.player_overlap_recovery_pending {
            self.player_overlap_recovery_pending = false;
            let resolved_position = resolve_player_overlap(&self.world, position);
            if resolved_position != position {
                position = resolved_position;
                self.player_velocity = Vec3::ZERO;
                self.player_grounded = false;
            }
        }
        let desired_direction = player_move_direction(camera, &self.input);
        let jump_held = self.input.is_key_pressed(KeyCode::Space);
        if !jump_held {
            self.player_jump_latched = false;
        }
        let fluid = player_fluid(&self.world, position);
        if let Some(fluid) = fluid {
            accelerate_horizontal(
                &mut self.player_velocity,
                desired_direction,
                CLASSIC_AIR_ACCELERATION,
            );
            if jump_held {
                self.player_velocity.y += 0.04;
            }
            let old_y = position.y;
            let (next_position, next_velocity, grounded, horizontal_collision) =
                move_player_with_collisions(&self.world, position, self.player_velocity);
            position = next_position;
            self.player_velocity = next_velocity;
            self.player_velocity *= if fluid == "lava" { 0.5 } else { 0.8 };
            self.player_velocity.y -= 0.02;
            if horizontal_collision
                && player_is_free_offset(
                    &self.world,
                    position,
                    Vec3::new(
                        self.player_velocity.x,
                        self.player_velocity.y + 0.6 - position.y + old_y,
                        self.player_velocity.z,
                    ),
                )
            {
                self.player_velocity.y = 0.3;
            }
            self.player_grounded = grounded;
        } else {
            let acceleration = if self.player_grounded {
                CLASSIC_GROUND_ACCELERATION
            } else {
                CLASSIC_AIR_ACCELERATION
            };
            accelerate_horizontal(&mut self.player_velocity, desired_direction, acceleration);

            if self.player_grounded && jump_held && !self.player_jump_latched {
                self.player_velocity.y = CLASSIC_JUMP_VELOCITY;
                self.player_jump_latched = true;
                self.player_grounded = false;
            }
            let (next_position, next_velocity, grounded, _) =
                move_player_with_collisions(&self.world, position, self.player_velocity);
            position = next_position;
            self.player_velocity = next_velocity;
            self.player_velocity.x *= 0.91;
            self.player_velocity.z *= 0.91;
            self.player_velocity.y = self.player_velocity.y * 0.98 - CLASSIC_GRAVITY;
            if grounded {
                self.player_velocity.x *= 0.6;
                self.player_velocity.z *= 0.6;
            }
            self.player_grounded = grounded;
        }

        if let Some(camera) = &mut self.camera {
            camera.set_position(position);
        }
        if let Some(renderer) = &mut self.renderer {
            if let Some(camera) = &self.camera {
                renderer.set_camera(
                    camera.view_projection(),
                    camera.position(),
                    camera.forward(),
                    camera.far_plane(),
                );
            }
        }
        self.debug_overlay.set_player_position(position);
        self.update_debug_biome(position);
    }

    fn set_pointer_locked(&mut self, locked: bool) {
        self.pointer_locked = locked;
        if let Some(window) = &self.window {
            window.set_pointer_locked(locked);
        }
    }

    fn handle_classic_keys(&mut self) {
        if self.input.take_key_press(KeyCode::KeyF) {
            self.classic_view_distance =
                (self.classic_view_distance + 1) % CLASSIC_FAR_PLANES.len();
            self.render_distance_chunks = CLASSIC_CHUNK_RADII[self.classic_view_distance];
            if let Some(camera) = &mut self.camera {
                camera.set_far_plane(CLASSIC_FAR_PLANES[self.classic_view_distance]);
                if let Some(renderer) = &mut self.renderer {
                    renderer.set_camera(
                        camera.view_projection(),
                        camera.position(),
                        camera.forward(),
                        camera.far_plane(),
                    );
                }
            }
        }
        if self.input.take_key_press(KeyCode::KeyY)
            && let Some(camera) = &mut self.camera
        {
            camera.toggle_invert_mouse();
        }
        if self.input.take_key_press(KeyCode::KeyR) {
            self.respawn_player();
        }
        if self.input.take_key_press(KeyCode::Enter) {
            if let Some(camera) = &self.camera {
                self.classic_spawn_position = camera.position();
            }
            self.respawn_player();
        }
        if self.input.take_key_press(KeyCode::KeyG) && self.classic_mobs.len() < 256 {
            if let Some(camera) = &self.camera {
                // Zombie.setPos centers its initial 1.8-high box on the player's eye Y.
                let position = camera.position() - Vec3::Y * (PLAYER_HEIGHT * 0.5);
                let turn_velocity = (self.classic_mob_random.next_float() + 1.0) * 0.01;
                let time_offset = self.classic_mob_random.next_float() * 1_239_813.0;
                let heading = self.classic_mob_random.next_float() * std::f32::consts::TAU;
                self.classic_mobs.push(ClassicMob {
                    position,
                    previous_position: position,
                    velocity: Vec3::ZERO,
                    heading,
                    turn_velocity,
                    time_offset,
                    grounded: false,
                });
            }
        }
    }

    fn respawn_player(&mut self) {
        if let Some(camera) = &mut self.camera {
            camera.set_position(self.classic_spawn_position);
        }
        self.player_velocity = Vec3::ZERO;
        self.player_grounded = false;
        self.player_jump_latched = false;
        self.player_overlap_recovery_pending = true;
    }

    fn update_classic_mobs(&mut self) {
        for mob in &mut self.classic_mobs {
            if !aabb_chunks_loaded(&self.world, Aabb::feet(mob.position)) {
                continue;
            }
            mob.previous_position = mob.position;
            mob.heading += mob.turn_velocity;
            mob.turn_velocity *= 0.99;
            mob.turn_velocity += (self.classic_mob_random.next_float()
                - self.classic_mob_random.next_float())
                * self.classic_mob_random.next_float()
                * self.classic_mob_random.next_float()
                * 0.08;
            let acceleration = if mob.grounded { 0.1 } else { 0.02 };
            mob.velocity.x += mob.heading.sin() * acceleration;
            mob.velocity.z += mob.heading.cos() * acceleration;
            if mob.grounded && self.classic_mob_random.next_float() < 0.08 {
                mob.velocity.y = 0.5;
                mob.grounded = false;
            }

            mob.velocity.y -= CLASSIC_GRAVITY;
            let (next_position, next_velocity, grounded, _) =
                move_mob_with_collisions(&self.world, mob.position, mob.velocity);
            mob.position = next_position;
            mob.velocity = next_velocity;
            mob.velocity.x *= 0.91;
            mob.velocity.y *= 0.98;
            mob.velocity.z *= 0.91;
            if grounded {
                mob.velocity.x *= 0.7;
                mob.velocity.z *= 0.7;
            }
            mob.grounded = grounded;
        }
        self.classic_mobs.retain(|mob| mob.position.y >= -100.0);
    }

    fn update_rendered_mobs(&mut self) {
        let alpha = (self.fixed_update_accumulator.as_secs_f32() / FIXED_TIMESTEP.as_secs_f32())
            .clamp(0.0, 1.0);
        let rendered_mobs: Vec<_> = self
            .classic_mobs
            .iter()
            .filter(|mob| aabb_chunks_loaded(&self.world, Aabb::feet(mob.position)))
            .map(|mob| {
                let position = mob.previous_position.lerp(mob.position, alpha);
                renderer::ClassicMobRender {
                    position,
                    heading: mob.heading,
                    time_offset: mob.time_offset,
                    brightness: classic_entity_brightness(&self.world, position),
                }
            })
            .collect();
        if let Some(renderer) = &mut self.renderer {
            renderer.set_classic_mobs(&rendered_mobs, (self.water_tick as f32 + alpha) * 0.5);
        }
    }

    fn handle_block_interaction(&mut self, dt: Duration) {
        if !self.pointer_locked {
            self.classic_action_cooldown = 0.0;
            return;
        }

        let place_requested = self.input.take_mouse_click(MouseButton::Right);

        if self.input.take_mouse_click(MouseButton::Middle)
            && let Some(target) = self.targeted_block()
        {
            let picked = self.world.get_block(target.block_pos);
            let picked = if picked.0 == "grass_block" {
                block::BlockId("dirt".to_string())
            } else {
                picked
            };
            if let Some(slot) = hotbar_slot_for_block(&picked) {
                self.hotbar_selected = slot;
            }
        }

        if place_requested {
            if let Some(target) = self.targeted_block() {
                self.place_selected_block(target);
            }
            return;
        }

        if !self.input.is_mouse_button_pressed(MouseButton::Left) {
            self.classic_action_cooldown = 0.0;
            return;
        }

        self.classic_action_cooldown -= dt.as_secs_f32();
        if self.classic_action_cooldown > 0.0 {
            return;
        }
        self.classic_action_cooldown = CLASSIC_ACTION_INTERVAL;

        let Some(target) = self.targeted_block() else {
            return;
        };

        if self.world.get_block(target.block_pos).0 != "bedrock" {
            self.world.set_block(
                target.block_pos,
                classic_break_replacement(target.block_pos),
            );
            self.queue_block_update_meshes(target.block_pos);
            self.settle_falling_blocks_above(target.block_pos);
            self.queue_water_updates_near(target.block_pos);
        }
    }

    fn place_selected_block(&mut self, target: BlockTarget) {
        let previous = self.world.get_block(target.place_pos);
        if !matches!(previous.0.as_str(), "" | "water" | "lava") {
            return;
        }
        self.world.set_block(
            target.place_pos,
            block::BlockId(HOTBAR_BLOCKS[self.hotbar_selected].to_string()),
        );
        if HOTBAR_BLOCKS[self.hotbar_selected] != "oak_sapling"
            && (self
                .camera
                .as_ref()
                .is_some_and(|camera| player_collides(&self.world, camera.position(), false))
                || self
                    .classic_mobs
                    .iter()
                    .any(|mob| block_intersects_mob(target.place_pos, mob.position)))
        {
            self.world.set_block(target.place_pos, previous);
            return;
        }
        self.queue_block_update_meshes(target.place_pos);
        self.settle_falling_blocks_above(target.place_pos);
        self.queue_water_updates_near(target.place_pos);
    }

    fn settle_falling_blocks_above(&mut self, changed_pos: world::BlockPos) {
        for (source, destination) in settle_falling_column(&mut self.world, changed_pos) {
            self.queue_block_update_meshes(source);
            self.queue_block_update_meshes(destination);
        }
    }

    fn reset_mining(&mut self) {
        self.mining_target = None;
        self.mining_progress = 0.0;
    }

    fn targeted_block(&self) -> Option<BlockTarget> {
        let camera = self.camera.as_ref()?;
        raycast_block(&self.world, camera.position(), camera.forward())
    }

    fn queue_block_update_meshes(&mut self, block_pos: world::BlockPos) {
        for chunk_pos in block_update_chunk_positions(block_pos) {
            self.queue_chunk_mesh_rebuild_front(chunk_pos);
        }
    }

    fn queue_water_update(&mut self, pos: world::BlockPos) {
        self.queue_water_update_after(pos, WATER_FLOW_DELAY_TICKS);
    }

    fn queue_water_update_after(&mut self, pos: world::BlockPos, delay_ticks: u64) {
        if !(0..world::CHUNK_SIZE_Y as i32).contains(&pos.1)
            || !self.world.is_chunk_loaded(pos.chunk_pos())
        {
            return;
        }
        let due_tick = self.water_tick.saturating_add(delay_ticks);
        if self
            .queued_water_updates
            .get(&pos)
            .map_or(true, |queued_tick| due_tick < *queued_tick)
        {
            self.queued_water_updates.insert(pos, due_tick);
            self.pending_water_updates.push_back((pos, due_tick));
        }
    }

    fn queue_water_updates_near(&mut self, pos: world::BlockPos) {
        for update_pos in [
            pos,
            world::BlockPos(pos.0, pos.1 + 1, pos.2),
            world::BlockPos(pos.0, pos.1 - 1, pos.2),
            world::BlockPos(pos.0 + 1, pos.1, pos.2),
            world::BlockPos(pos.0 - 1, pos.1, pos.2),
            world::BlockPos(pos.0, pos.1, pos.2 + 1),
            world::BlockPos(pos.0, pos.1, pos.2 - 1),
            world::BlockPos(pos.0 + 2, pos.1, pos.2),
            world::BlockPos(pos.0 - 2, pos.1, pos.2),
            world::BlockPos(pos.0, pos.1, pos.2 + 2),
            world::BlockPos(pos.0, pos.1, pos.2 - 2),
        ] {
            self.queue_water_update(update_pos);
        }
    }

    fn process_water_updates(&mut self) -> usize {
        let mut changed = 0;
        let mut processed = 0;
        let pending_count = self.pending_water_updates.len();
        let mut checked = 0;
        while processed < WATER_UPDATES_PER_TICK && checked < pending_count {
            checked += 1;
            let Some((pos, due_tick)) = self.pending_water_updates.pop_front() else {
                break;
            };
            if due_tick > self.water_tick {
                self.pending_water_updates.push_back((pos, due_tick));
                continue;
            }
            if self.queued_water_updates.get(&pos).copied() != Some(due_tick) {
                continue;
            }
            self.queued_water_updates.remove(&pos);
            processed += 1;
            if self.update_water_at(pos) {
                changed += 1;
            }
        }
        changed
    }

    fn update_water_at(&mut self, pos: world::BlockPos) -> bool {
        let fluid = self.world.get_block(pos);
        if !matches!(fluid.0.as_str(), "water" | "lava") {
            return false;
        }
        let opposite = if fluid.0 == "water" { "lava" } else { "water" };
        if horizontal_neighbors(pos)
            .into_iter()
            .chain((pos.1 > 0).then_some(world::BlockPos(pos.0, pos.1 - 1, pos.2)))
            .any(|neighbor| self.world.get_block(neighbor).0 == opposite)
        {
            self.world
                .set_block(pos, block::BlockId("stone".to_string()));
            self.queue_block_update_meshes(pos);
            return true;
        }

        let mut changed = false;
        let mut moved_down = false;
        let mut below = world::BlockPos(pos.0, pos.1 - 1, pos.2);
        while below.1 >= 0 && self.world.get_block(below) == block::BlockId::AIR {
            self.world.set_block(below, fluid.clone());
            self.world
                .set_block_property(below, WATER_LEVEL_PROPERTY, 1);
            self.queue_block_update_meshes(below);
            self.queue_water_update(below);
            changed = true;
            moved_down = true;
            if fluid.0 == "lava" {
                break;
            }
            below.1 -= 1;
        }

        if fluid.0 == "water" || !moved_down {
            for neighbor in horizontal_neighbors(pos) {
                let target = self.world.get_block(neighbor);
                if target.0 == opposite {
                    self.world
                        .set_block(neighbor, block::BlockId("stone".to_string()));
                    self.queue_block_update_meshes(neighbor);
                    changed = true;
                } else if target == block::BlockId::AIR {
                    self.world.set_block(neighbor, fluid.clone());
                    self.world
                        .set_block_property(neighbor, WATER_LEVEL_PROPERTY, 1);
                    self.queue_block_update_meshes(neighbor);
                    self.queue_water_update(neighbor);
                    changed = true;
                }
            }
        }

        self.world
            .set_block_property(pos, WATER_LEVEL_PROPERTY, if changed { 1 } else { 0 });
        changed
    }

    fn spread_water_from(&mut self, pos: world::BlockPos) -> bool {
        let level = self.water_level(pos);
        let below = world::BlockPos(pos.0, pos.1 - 1, pos.2);
        if pos.1 > 0 && self.can_water_replace(below, WATER_FALLING_LEVEL) {
            self.set_water_level(below, WATER_FALLING_LEVEL);
            self.queue_block_update_meshes(below);
            self.queue_water_updates_near(below);
            return true;
        }
        if has_falling_water_below(&self.world, pos) {
            return false;
        }

        let spread_level = water_spread_level(level);
        if spread_level >= WATER_MAX_HORIZONTAL_LEVEL {
            return false;
        }

        let next_level = spread_level + 1;
        let mut changed = false;
        for target in self.water_spread_targets(pos, next_level) {
            self.set_water_level(target, next_level);
            self.queue_block_update_meshes(target);
            self.queue_water_updates_near(target);
            changed = true;
        }
        changed
    }

    fn water_spread_targets(
        &self,
        source: world::BlockPos,
        next_level: u8,
    ) -> Vec<world::BlockPos> {
        water_spread_targets(&self.world, source, next_level)
    }

    fn recomputed_water_level(&self, pos: world::BlockPos) -> Option<u8> {
        if self.can_generate_water_source_at(pos) {
            return Some(WATER_SOURCE_LEVEL);
        }

        if self.has_water_above(pos) {
            return Some(WATER_FALLING_LEVEL);
        }

        let mut best = None;
        for neighbor in horizontal_neighbors(pos) {
            if self.world.get_block(neighbor).0 != "water" {
                continue;
            }
            if water_flows_downward(&self.world, neighbor) {
                continue;
            }
            if !self.water_connected_to_source(neighbor) {
                continue;
            }
            let level = self.water_level(neighbor);
            let spread_level = water_spread_level(level);
            if spread_level < WATER_MAX_HORIZONTAL_LEVEL {
                best = Some(best.map_or(spread_level + 1, |current: u8| {
                    current.min(spread_level + 1)
                }));
            }
        }

        best
    }

    fn can_generate_water_source_at(&self, pos: world::BlockPos) -> bool {
        can_generate_water_source(&self.world, pos)
    }

    fn water_connected_to_source(&self, pos: world::BlockPos) -> bool {
        if self.world.get_block(pos).0 != "water" {
            return false;
        }
        if self.water_level(pos) == WATER_SOURCE_LEVEL || self.has_water_above(pos) {
            return true;
        }

        let mut queue = VecDeque::from([(pos, 0usize)]);
        let mut visited = HashSet::from([pos]);
        while let Some((current, distance)) = queue.pop_front() {
            if distance >= WATER_SOURCE_SEARCH_LIMIT {
                continue;
            }

            for neighbor in horizontal_neighbors(current) {
                if !visited.insert(neighbor) || self.world.get_block(neighbor).0 != "water" {
                    continue;
                }
                if self.water_level(neighbor) == WATER_SOURCE_LEVEL
                    || self.has_water_above(neighbor)
                {
                    return true;
                }
                queue.push_back((neighbor, distance + 1));
            }
        }

        false
    }

    fn has_water_above(&self, pos: world::BlockPos) -> bool {
        pos.1 + 1 < world::CHUNK_SIZE_Y as i32
            && self
                .world
                .get_block(world::BlockPos(pos.0, pos.1 + 1, pos.2))
                .0
                == "water"
    }

    fn can_water_replace(&self, pos: world::BlockPos, new_level: u8) -> bool {
        can_water_replace(&self.world, pos, new_level)
    }

    fn set_water_level(&mut self, pos: world::BlockPos, level: u8) {
        self.world
            .set_block(pos, block::BlockId("water".to_string()));
        self.world
            .set_block_property(pos, WATER_LEVEL_PROPERTY, level.min(WATER_MAX_LEVEL));
    }

    fn water_level(&self, pos: world::BlockPos) -> u8 {
        water_level_at(&self.world, pos)
    }

    fn update_hotbar_selection(&mut self) {
        for (index, key) in [
            KeyCode::Digit1,
            KeyCode::Digit2,
            KeyCode::Digit3,
            KeyCode::Digit4,
            KeyCode::Digit5,
            KeyCode::Digit6,
            KeyCode::Digit7,
            KeyCode::Digit8,
            KeyCode::Digit9,
        ]
        .into_iter()
        .enumerate()
        {
            if self.input.take_key_press(key) {
                self.hotbar_selected = index;
            }
        }
    }

    fn update_hotbar_scroll(&mut self) {
        let scroll = self.input.take_scroll_delta();
        if scroll > 0.0 {
            self.hotbar_selected = (self.hotbar_selected + HOTBAR_SIZE - 1) % HOTBAR_SIZE;
        } else if scroll < 0.0 {
            self.hotbar_selected = (self.hotbar_selected + 1) % HOTBAR_SIZE;
        }
    }

    fn toggle_inventory(&mut self) {
        self.inventory_open = !self.inventory_open;
        self.set_pointer_locked(!self.inventory_open);
        self.input.clear_mouse_clicks();
        if self.inventory_open {
            if let Some(window) = &self.window {
                let size = window.inner.inner_size();
                let center = (size.width as f64 * 0.5, size.height as f64 * 0.5);
                window.set_cursor_position(center.0, center.1);
                self.input.set_cursor_position(center);
            }
        }
        if !self.inventory_open && !self.carried_slot.is_empty() {
            add_stack_to_inventory(
                &mut self.hotbar_slots,
                &mut self.inventory_slots,
                self.carried_slot,
            );
            self.carried_slot = InventorySlot::default();
        }
    }

    fn handle_inventory_click(&mut self) {
        self.input.take_mouse_click(MouseButton::Left);
        let Some((cursor_x, cursor_y)) = self.input.cursor_position() else {
            return;
        };
        let Some(window) = &self.window else {
            return;
        };
        let size = window.inner.inner_size();
        let Some(target) =
            inventory_slot_at(cursor_x as f32, cursor_y as f32, size.width, size.height)
        else {
            return;
        };

        match target {
            InventorySlotTarget::Hotbar(index) => {
                click_inventory_slot(&mut self.hotbar_slots[index], &mut self.carried_slot)
            }
            InventorySlotTarget::Inventory(index) => {
                click_inventory_slot(&mut self.inventory_slots[index], &mut self.carried_slot)
            }
        }
    }

    fn save_game(&mut self) {
        let Some(camera) = &self.camera else {
            return;
        };

        match save_game(
            &self.world,
            camera.position(),
            self.hotbar_selected,
            self.hotbar_slots,
            self.inventory_slots,
            self.classic_spawn_position,
            &self.classic_mobs,
        ) {
            Ok(()) => {
                self.last_save = Instant::now();
                log::info!(target: "save", "Saved world to {:?}", save_path());
            }
            Err(error) => {
                log::warn!(target: "save", "Failed to save world: {error}");
            }
        }
    }

    fn adjust_render_distance(&mut self, delta: i32) {
        let previous = self.render_distance_chunks;
        let next = (self.render_distance_chunks + delta)
            .clamp(MIN_RENDER_DISTANCE_CHUNKS, MAX_RENDER_DISTANCE_CHUNKS);
        if next == self.render_distance_chunks {
            return;
        }

        self.render_distance_chunks = next;
        self.debug_overlay.set_render_distance(next);

        if let Some(center_chunk) = self
            .camera
            .as_ref()
            .map(|camera| camera_chunk_pos(camera.position()))
        {
            self.unload_far_chunks(center_chunk);
            self.remove_chunk_meshes_outside_render_distance(center_chunk);
            if next > previous {
                self.queue_loaded_chunk_meshes_in_range(center_chunk, previous + 1, next);
            }
        }

        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn rebuild_chunk_meshes(&mut self, force: bool) {
        let Some(camera) = &self.camera else {
            return;
        };
        let center_chunk = camera_chunk_pos(camera.position());
        if !force && center_chunk == self.mesh_center_chunk {
            return;
        }

        let (Some(renderer), Some(block_models)) = (&mut self.renderer, self.block_models.as_ref())
        else {
            return;
        };

        build_chunk_meshes(
            renderer,
            block_models,
            &self.world,
            &self.biome_source,
            &self.noise_settings,
            center_chunk,
            self.render_distance_chunks,
        );
        self.mesh_center_chunk = center_chunk;
    }

    fn queue_chunk_meshes_near(&mut self, center: world::ChunkPos) {
        let positions = [
            center,
            world::ChunkPos(center.0 + 1, center.1),
            world::ChunkPos(center.0 - 1, center.1),
            world::ChunkPos(center.0, center.1 + 1),
            world::ChunkPos(center.0, center.1 - 1),
        ];

        for pos in positions {
            self.queue_chunk_mesh_rebuild(pos);
        }
    }

    fn queue_chunk_mesh_rebuild(&mut self, pos: world::ChunkPos) {
        if self.pending_mesh_generations.contains(&pos) {
            self.queued_mesh_rebuilds.insert(pos);
            return;
        }
        if self.queued_mesh_rebuilds.insert(pos) {
            self.pending_mesh_rebuilds.push_back(pos);
        }
    }

    fn queue_chunk_mesh_rebuild_front(&mut self, pos: world::ChunkPos) {
        if self.pending_mesh_generations.contains(&pos) {
            self.queued_mesh_rebuilds.insert(pos);
            return;
        }
        if self.queued_mesh_rebuilds.insert(pos) {
            self.pending_mesh_rebuilds.push_front(pos);
        } else {
            self.pending_mesh_rebuilds.retain(|queued| *queued != pos);
            self.pending_mesh_rebuilds.push_front(pos);
        }
    }

    fn set_chunk_mesh_from_data(&mut self, pos: world::ChunkPos, data: mesher::MeshData) {
        let Some(renderer) = &mut self.renderer else {
            return;
        };

        set_chunk_mesh_from_data(renderer, pos, data);
    }

    fn remove_chunk_meshes_outside_render_distance(&mut self, center_chunk: world::ChunkPos) {
        let Some(renderer) = &mut self.renderer else {
            return;
        };

        for chunk_pos in self.world.chunk_positions() {
            let distance = (chunk_pos.0 - center_chunk.0)
                .abs()
                .max((chunk_pos.1 - center_chunk.1).abs());
            if distance > self.render_distance_chunks {
                renderer.remove_chunk_mesh(chunk_pos);
            }
        }
    }

    fn queue_loaded_chunk_meshes_in_range(
        &mut self,
        center_chunk: world::ChunkPos,
        min_distance: i32,
        max_distance: i32,
    ) {
        for chunk_pos in self.world.chunk_positions() {
            let distance = (chunk_pos.0 - center_chunk.0)
                .abs()
                .max((chunk_pos.1 - center_chunk.1).abs());
            if distance < min_distance || distance > max_distance {
                continue;
            }

            self.queue_chunk_mesh_rebuild(chunk_pos);
        }
    }

    fn update_debug_biome(&mut self, position: Vec3) {
        let block_pos = camera_block_pos(position);
        let biome_name = self
            .biome_source
            .sample_biome(
                &self.world,
                &self.noise_settings,
                &self.biomes,
                block_pos.0,
                block_pos.2,
            )
            .map(|(id, biome)| {
                format!(
                    "{} ({}, T {:.1}, H {:.1})",
                    biome.name(),
                    id,
                    biome.temperature(),
                    biome.humidity()
                )
            })
            .unwrap_or_else(|| "Unknown".to_string());
        self.debug_overlay.set_biome(biome_name);
    }
}

/// Creates a demo world, builds chunk meshes, and sets them on the renderer.
fn build_chunk_meshes(
    renderer: &mut renderer::Renderer,
    block_models: &crate::registry::Registry<model::BlockModel>,
    world: &world::World,
    biome_source: &worldgen::BiomeSource,
    noise_settings: &worldgen::NoiseSettings,
    center_chunk: world::ChunkPos,
    render_distance_chunks: i32,
) {
    // Build a path → model lookup map.
    use std::collections::HashMap;
    let model_map: HashMap<String, model::BlockModel> = block_models
        .iter()
        .map(|(id, m)| (id.path().to_string(), m.clone()))
        .collect();
    let atlas_uv = renderer.atlas.uv_map();

    // Mesh each chunk and build GPU meshes.
    let material_layout = renderer.material_layout();
    let device = &renderer.device;
    let mut opaque_meshes = HashMap::new();
    let mut transparent_meshes = HashMap::new();

    for chunk in world.chunks() {
        let pos = chunk.pos();
        let distance = (pos.0 - center_chunk.0)
            .abs()
            .max((pos.1 - center_chunk.1).abs());
        if distance > render_distance_chunks {
            continue;
        }

        let data = mesher::mesh_chunk(
            chunk,
            world,
            biome_source,
            noise_settings,
            &model_map,
            &atlas_uv,
        );
        if !data.opaque.vertices.is_empty() {
            opaque_meshes.insert(
                pos,
                renderer::Mesh::from_vertices(
                    device,
                    material_layout,
                    &format!("chunk_{}_{}_opaque", pos.0, pos.1),
                    [0.8, 0.85, 0.75, 1.0],
                    &data.opaque.vertices,
                    &data.opaque.indices,
                ),
            );
        }
        if !data.transparent.vertices.is_empty() {
            transparent_meshes.insert(
                pos,
                renderer::Mesh::from_vertices(
                    device,
                    material_layout,
                    &format!("chunk_{}_{}_transparent", pos.0, pos.1),
                    [0.8, 0.85, 0.75, 1.0],
                    &data.transparent.vertices,
                    &data.transparent.indices,
                ),
            );
        }
    }

    log::debug!(target: "mesher", "Built {} opaque and {} transparent chunk meshes within render distance {render_distance_chunks}", opaque_meshes.len(), transparent_meshes.len());
    renderer.set_chunk_meshes(opaque_meshes, transparent_meshes);
}

fn set_chunk_mesh_from_data(
    renderer: &mut renderer::Renderer,
    chunk_pos: world::ChunkPos,
    data: mesher::MeshData,
) {
    let opaque_mesh = (!data.opaque.vertices.is_empty()).then(|| {
        renderer::Mesh::from_vertices(
            &renderer.device,
            renderer.material_layout(),
            &format!("chunk_{}_{}_opaque", chunk_pos.0, chunk_pos.1),
            [0.8, 0.85, 0.75, 1.0],
            &data.opaque.vertices,
            &data.opaque.indices,
        )
    });
    let transparent_mesh = (!data.transparent.vertices.is_empty()).then(|| {
        renderer::Mesh::from_vertices(
            &renderer.device,
            renderer.material_layout(),
            &format!("chunk_{}_{}_transparent", chunk_pos.0, chunk_pos.1),
            [0.8, 0.85, 0.75, 1.0],
            &data.transparent.vertices,
            &data.transparent.indices,
        )
    });

    renderer.set_opaque_chunk_mesh(chunk_pos, opaque_mesh);
    renderer.set_transparent_chunk_mesh(chunk_pos, transparent_mesh);
}

fn camera_chunk_pos(position: Vec3) -> world::ChunkPos {
    let block_pos = camera_block_pos(position);
    world::ChunkPos(
        block_pos.0.div_euclid(world::CHUNK_SIZE_X as i32),
        block_pos.2.div_euclid(world::CHUNK_SIZE_Z as i32),
    )
}

fn block_update_chunk_positions(block_pos: world::BlockPos) -> Vec<world::ChunkPos> {
    let chunk_pos = block_pos.chunk_pos();
    let (local_x, _, local_z) = block_pos.local();
    let mut positions = vec![chunk_pos];
    if local_x == 0 {
        positions.push(world::ChunkPos(chunk_pos.0 - 1, chunk_pos.1));
    } else if local_x + 1 == world::CHUNK_SIZE_X {
        positions.push(world::ChunkPos(chunk_pos.0 + 1, chunk_pos.1));
    }
    if local_z == 0 {
        positions.push(world::ChunkPos(chunk_pos.0, chunk_pos.1 - 1));
    } else if local_z + 1 == world::CHUNK_SIZE_Z {
        positions.push(world::ChunkPos(chunk_pos.0, chunk_pos.1 + 1));
    }
    positions
}

fn camera_block_pos(position: Vec3) -> world::BlockPos {
    world::BlockPos(
        position.x.floor() as i32,
        position.y.floor() as i32,
        position.z.floor() as i32,
    )
}

fn horizontal_neighbors(pos: world::BlockPos) -> [world::BlockPos; 4] {
    horizontal_directions(pos).map(|(_, neighbor)| neighbor)
}

fn horizontal_directions(pos: world::BlockPos) -> [((i32, i32), world::BlockPos); 4] {
    [
        ((1, 0), world::BlockPos(pos.0 + 1, pos.1, pos.2)),
        ((-1, 0), world::BlockPos(pos.0 - 1, pos.1, pos.2)),
        ((0, 1), world::BlockPos(pos.0, pos.1, pos.2 + 1)),
        ((0, -1), world::BlockPos(pos.0, pos.1, pos.2 - 1)),
    ]
}

fn water_level_at(world: &world::World, pos: world::BlockPos) -> u8 {
    world
        .get_block_property(pos, WATER_LEVEL_PROPERTY)
        .min(WATER_MAX_LEVEL)
}

fn has_two_adjacent_water_sources(world: &world::World, pos: world::BlockPos) -> bool {
    horizontal_neighbors(pos)
        .into_iter()
        .filter(|neighbor| {
            world.get_block(*neighbor).0 == "water"
                && water_level_at(world, *neighbor) == WATER_SOURCE_LEVEL
        })
        .take(2)
        .count()
        >= 2
}

fn can_generate_water_source(world: &world::World, pos: world::BlockPos) -> bool {
    let block = world.get_block(pos);
    let below_pos = world::BlockPos(pos.0, pos.1 - 1, pos.2);
    let below = world.get_block(below_pos);
    let supported = pos.1 == 0
        || (below.0 != ""
            && (below.0 != "water" || water_level_at(world, below_pos) == WATER_SOURCE_LEVEL));
    matches!(block.0.as_str(), "" | "water")
        && !(block.0 == "water" && water_level_at(world, pos) == WATER_SOURCE_LEVEL)
        && has_two_adjacent_water_sources(world, pos)
        && supported
}

fn has_falling_water_below(world: &world::World, pos: world::BlockPos) -> bool {
    if pos.1 == 0 {
        return false;
    }
    let below = world::BlockPos(pos.0, pos.1 - 1, pos.2);
    world.get_block(below).0 == "water" && water_level_at(world, below) >= WATER_FALLING_LEVEL
}

fn water_flows_downward(world: &world::World, pos: world::BlockPos) -> bool {
    if pos.1 == 0 {
        return false;
    }
    let below = world::BlockPos(pos.0, pos.1 - 1, pos.2);
    can_water_replace(world, below, WATER_FALLING_LEVEL) || has_falling_water_below(world, pos)
}

fn can_water_replace(world: &world::World, pos: world::BlockPos, new_level: u8) -> bool {
    if !(0..world::CHUNK_SIZE_Y as i32).contains(&pos.1) || !world.is_chunk_loaded(pos.chunk_pos())
    {
        return false;
    }
    match world.get_block(pos).0.as_str() {
        "" => true,
        "water" => world.get_block_property(pos, WATER_LEVEL_PROPERTY) > new_level,
        _ => false,
    }
}

fn water_can_fall_from(world: &world::World, pos: world::BlockPos) -> bool {
    pos.1 > 0
        && can_water_replace(
            world,
            world::BlockPos(pos.0, pos.1 - 1, pos.2),
            WATER_FALLING_LEVEL,
        )
}

fn water_distance_to_drop(
    world: &world::World,
    start: world::BlockPos,
    next_level: u8,
    initial_direction: (i32, i32),
) -> Option<usize> {
    if water_can_fall_from(world, start) {
        return Some(0);
    }

    water_slope_distance(
        world,
        start,
        next_level,
        1,
        (-initial_direction.0, -initial_direction.1),
    )
}

fn water_slope_distance(
    world: &world::World,
    pos: world::BlockPos,
    next_level: u8,
    distance: usize,
    blocked_direction: (i32, i32),
) -> Option<usize> {
    let mut best = None;
    for (direction, neighbor) in horizontal_directions(pos) {
        if direction == blocked_direction || !can_water_replace(world, neighbor, next_level) {
            continue;
        }
        if water_can_fall_from(world, neighbor) {
            return Some(distance);
        }
        if distance < WATER_SLOPE_FIND_DISTANCE
            && let Some(found) = water_slope_distance(
                world,
                neighbor,
                next_level,
                distance + 1,
                (-direction.0, -direction.1),
            )
        {
            best = Some(best.map_or(found, |current: usize| current.min(found)));
        }
    }
    best
}

fn water_spread_targets(
    world: &world::World,
    source: world::BlockPos,
    next_level: u8,
) -> Vec<world::BlockPos> {
    let mut best_distance = usize::MAX;
    let mut targets = Vec::new();
    for (direction, candidate) in horizontal_directions(source) {
        if !can_water_replace(world, candidate, next_level) {
            continue;
        }
        let distance =
            water_distance_to_drop(world, candidate, next_level, direction).unwrap_or(usize::MAX);
        if distance < best_distance {
            best_distance = distance;
            targets.clear();
        }
        if distance == best_distance {
            targets.push(candidate);
        }
    }
    targets
}

fn water_spread_level(level: u8) -> u8 {
    if level >= WATER_FALLING_LEVEL {
        WATER_SOURCE_LEVEL
    } else {
        level
    }
}

#[cfg(test)]
mod water_tests {
    use super::*;

    fn world_with_dirt_floor() -> world::World {
        let mut world = world::World::new();
        for x in 0..world::CHUNK_SIZE_X as i32 {
            for z in 0..world::CHUNK_SIZE_Z as i32 {
                world.set_block(world::BlockPos(x, 9, z), block::BlockId("dirt".to_string()));
            }
        }
        world
    }

    #[test]
    fn l_shaped_sources_generate_source_over_dirt() {
        let mut world = world::World::new();
        let target = world::BlockPos(8, 10, 8);
        world.set_block(
            world::BlockPos(target.0, target.1 - 1, target.2),
            block::BlockId("dirt".to_string()),
        );
        world.set_block(
            world::BlockPos(target.0 - 1, target.1, target.2),
            block::BlockId("water".to_string()),
        );
        world.set_block(
            world::BlockPos(target.0, target.1, target.2 - 1),
            block::BlockId("water".to_string()),
        );

        assert!(can_generate_water_source(&world, target));

        world.set_block(target, block::BlockId("water".to_string()));
        assert!(!can_generate_water_source(&world, target));
    }

    #[test]
    fn water_paths_toward_nearest_drop() {
        let mut world = world_with_dirt_floor();
        let source = world::BlockPos(8, 10, 8);
        let east = world::BlockPos(9, 10, 8);
        world.set_block(source, block::BlockId("water".to_string()));
        world.set_block(world::BlockPos(10, 9, 8), block::BlockId::AIR.clone());

        assert_eq!(water_spread_targets(&world, source, 1), vec![east]);
    }

    #[test]
    fn water_uses_all_equally_short_drop_routes() {
        let mut world = world_with_dirt_floor();
        let source = world::BlockPos(8, 10, 8);
        let east = world::BlockPos(9, 10, 8);
        let west = world::BlockPos(7, 10, 8);
        world.set_block(source, block::BlockId("water".to_string()));
        world.set_block(world::BlockPos(10, 9, 8), block::BlockId::AIR.clone());
        world.set_block(world::BlockPos(6, 9, 8), block::BlockId::AIR.clone());

        assert_eq!(water_spread_targets(&world, source, 1), vec![east, west]);
    }

    #[test]
    fn water_ignores_drops_beyond_slope_search_distance() {
        let mut world = world_with_dirt_floor();
        let source = world::BlockPos(8, 10, 8);
        world.set_block(source, block::BlockId("water".to_string()));
        world.set_block(world::BlockPos(14, 9, 8), block::BlockId::AIR.clone());

        assert_eq!(water_spread_targets(&world, source, 1).len(), 4);
    }

    #[test]
    fn falling_column_suppresses_horizontal_spread() {
        let mut world = world::World::new();
        let source = world::BlockPos(8, 10, 8);
        let below = world::BlockPos(8, 9, 8);
        world.set_block(source, block::BlockId("water".to_string()));

        assert!(water_flows_downward(&world, source));

        world.set_block(below, block::BlockId("water".to_string()));
        world.set_block_property(below, WATER_LEVEL_PROPERTY, WATER_FALLING_LEVEL);

        assert!(has_falling_water_below(&world, source));
        assert!(water_flows_downward(&world, source));

        world.set_block(below, block::BlockId("dirt".to_string()));
        assert!(!water_flows_downward(&world, source));
    }

    #[test]
    fn player_immersion_respects_water_surface_height() {
        let mut world = world::World::new();
        world.set_block(
            world::BlockPos(8, 10, 8),
            block::BlockId("water".to_string()),
        );
        let center = Vec3::new(8.5, 10.5, 8.5);

        assert!(water_block_at_point(&world, center).is_some());
        assert!(water_block_at_point(&world, Vec3::new(8.5, 10.9, 8.5)).is_none());
        assert!(player_in_water(
            &world,
            Vec3::new(8.5, 10.0 + PLAYER_EYE_HEIGHT, 8.5),
            false,
        ));
    }

    #[test]
    fn classic_player_fluid_detects_water_and_lava() {
        let mut world = world::World::new();
        let eye = Vec3::new(8.5, 10.0 + PLAYER_EYE_HEIGHT, 8.5);
        world.set_block(
            world::BlockPos(8, 10, 8),
            block::BlockId("water".to_string()),
        );
        assert_eq!(player_fluid(&world, eye), Some("water"));
        world.set_block(
            world::BlockPos(8, 10, 8),
            block::BlockId("lava".to_string()),
        );
        assert_eq!(player_fluid(&world, eye), Some("lava"));
    }
}

#[cfg(test)]
mod early_classic_tests {
    use super::*;

    #[test]
    fn sand_and_gravel_fall_instantly_to_lowest_air_block() {
        for block_id in ["sand", "gravel"] {
            let mut world = world::World::new();
            world.set_block(
                world::BlockPos(8, 1, 8),
                block::BlockId("stone".to_string()),
            );
            world.set_block(
                world::BlockPos(8, 8, 8),
                block::BlockId(block_id.to_string()),
            );

            let moved = settle_falling_column(&mut world, world::BlockPos(8, 8, 8));
            assert_eq!(
                moved,
                vec![(world::BlockPos(8, 8, 8), world::BlockPos(8, 2, 8))]
            );
            assert_eq!(world.get_block(world::BlockPos(8, 2, 8)).0, block_id);
        }
    }

    #[test]
    fn bedrock_is_unbreakable_and_not_in_item_catalog() {
        assert!(block_break_seconds(&block::BlockId("bedrock".to_string())).is_infinite());
        assert!(hotbar_slot_for_block(&block::BlockId("bedrock".to_string())).is_none());
    }

    #[test]
    fn creative_palette_matches_archived_client_order() {
        assert_eq!(
            HOTBAR_BLOCKS,
            [
                "stone",
                "dirt",
                "cobblestone",
                "oak_planks",
                "oak_sapling",
                "oak_log",
                "oak_leaves",
                "sand",
                "gravel",
            ]
        );
    }

    #[test]
    fn huge_world_edges_are_solid_without_a_ceiling() {
        let world = world::World::new();
        let eye_y = 40.0 + PLAYER_EYE_HEIGHT;
        assert!(player_collides(&world, Vec3::new(0.1, eye_y, 256.0), false));
        assert!(player_collides(
            &world,
            Vec3::new(CLASSIC_WORLD_SIZE as f32 - 0.1, eye_y, 256.0),
            false
        ));
        assert!(!player_collides(
            &world,
            Vec3::new(256.0, world::CHUNK_SIZE_Y as f32 + PLAYER_EYE_HEIGHT, 256.0),
            false
        ));
    }

    #[test]
    fn breaking_ocean_level_edges_refills_them_with_water() {
        assert_eq!(
            classic_break_replacement(world::BlockPos(0, 30, 20)).0,
            "water"
        );
        assert_eq!(
            classic_break_replacement(world::BlockPos(511, 31, 20)).0,
            "water"
        );
        assert_eq!(
            classic_break_replacement(world::BlockPos(0, 32, 20)),
            block::BlockId::AIR
        );
        assert_eq!(
            classic_break_replacement(world::BlockPos(1, 30, 20)),
            block::BlockId::AIR
        );
    }

    #[test]
    fn block_updates_only_remesh_chunks_sharing_the_changed_face() {
        assert_eq!(
            block_update_chunk_positions(world::BlockPos(8, 20, 8)),
            vec![world::ChunkPos(0, 0)]
        );
        assert_eq!(
            block_update_chunk_positions(world::BlockPos(15, 20, 8)),
            vec![world::ChunkPos(0, 0), world::ChunkPos(1, 0)]
        );
        assert_eq!(
            block_update_chunk_positions(world::BlockPos(16, 20, 16)),
            vec![
                world::ChunkPos(1, 1),
                world::ChunkPos(0, 1),
                world::ChunkPos(1, 0),
            ]
        );
    }

    #[test]
    fn placement_target_is_the_air_cell_before_the_hit_block() {
        let mut world = world::World::new();
        world.set_block(
            world::BlockPos(8, 20, 9),
            block::BlockId("water".to_string()),
        );
        world.set_block(
            world::BlockPos(8, 20, 10),
            block::BlockId("stone".to_string()),
        );
        let target = raycast_block(&world, Vec3::new(8.5, 20.5, 8.5), Vec3::Z)
            .expect("stone should be in Classic reach");

        assert_eq!(target.block_pos, world::BlockPos(8, 20, 10));
        assert_eq!(target.place_pos, world::BlockPos(8, 20, 9));
    }

    #[test]
    fn swept_classic_collision_clips_fast_movement_and_allows_face_contact() {
        let mut world = world::World::new();
        world.set_block(
            world::BlockPos(1, 0, 0),
            block::BlockId("stone".to_string()),
        );
        let start = Vec3::new(0.5, PLAYER_EYE_HEIGHT, 0.5);
        let (position, velocity, _, horizontal_collision) =
            move_player_with_collisions(&world, start, Vec3::new(2.0, 0.0, 0.0));

        assert!((position.x - 0.7).abs() < 1.0e-6);
        assert_eq!(velocity.x, 0.0);
        assert!(horizontal_collision);
        assert!(!player_collides(
            &world,
            Vec3::new(0.7, PLAYER_EYE_HEIGHT, 0.5),
            false,
        ));
    }

    #[test]
    fn classic_fluid_detection_uses_the_player_aabb() {
        let mut world = world::World::new();
        world.set_block(
            world::BlockPos(1, 0, 0),
            block::BlockId("water".to_string()),
        );

        assert_eq!(
            player_fluid(&world, Vec3::new(0.75, PLAYER_EYE_HEIGHT, 0.5)),
            Some("water")
        );
    }

    #[test]
    fn classic_collision_supplies_a_floor_below_the_level() {
        let world = world::World::new();
        let start = Vec3::new(8.5, PLAYER_EYE_HEIGHT, 8.5);
        let (position, velocity, grounded, _) =
            move_player_with_collisions(&world, start, Vec3::new(0.0, -1.0, 0.0));

        assert_eq!(position, start);
        assert_eq!(velocity.y, 0.0);
        assert!(grounded);
    }

    #[test]
    fn player_physics_waits_for_terrain_and_recovers_from_overlap() {
        let mut world = world::World::new();
        let eye = Vec3::new(8.5, PLAYER_EYE_HEIGHT, 8.5);
        assert!(!aabb_chunks_loaded(&world, Aabb::player(eye)));

        world.set_block(
            world::BlockPos(8, 0, 8),
            block::BlockId("stone".to_string()),
        );
        assert!(aabb_chunks_loaded(&world, Aabb::player(eye)));
        assert_eq!(resolve_player_overlap(&world, eye), eye + Vec3::Y * 1.001);
    }

    #[test]
    fn setting_spawn_preserves_the_non_overlapping_eye_position() {
        let mut world = world::World::new();
        world.set_block(
            world::BlockPos(8, 0, 8),
            block::BlockId("stone".to_string()),
        );
        let grounded_eye = Vec3::new(8.5, 1.0 + PLAYER_EYE_HEIGHT, 8.5);

        assert!(!player_collides(&world, grounded_eye, false));
        assert!(player_collides(&world, grounded_eye.floor(), false));
        assert!(valid_player_eye_position(grounded_eye));
        let (landed_eye, _, grounded, _) =
            move_player_with_collisions(&world, grounded_eye, Vec3::new(0.0, -0.08, 0.0));
        assert_eq!(landed_eye, grounded_eye);
        assert!(grounded);
    }

    #[test]
    fn save_version_is_read_from_header_without_parsing_chunks() {
        let header = r#"{
            "format_version": 6,
            "seed": 12345,
            "chunks": ["#;
        assert_eq!(saved_format_version(header), Some(6));
        assert_eq!(saved_format_version("{}"), None);
    }
}

fn start_chunk_generation_worker(
    seed: u64,
    classic_terrain: Arc<classic_worldgen::ClassicTerrain>,
    noise_settings: worldgen::NoiseSettings,
    biome_source: worldgen::BiomeSource,
    model_map: std::collections::HashMap<String, model::BlockModel>,
    atlas_uv: std::collections::HashMap<String, [f32; 4]>,
) -> (
    mpsc::Sender<world::ChunkPos>,
    mpsc::Receiver<GeneratedChunk>,
) {
    let (request_tx, request_rx) = mpsc::channel::<world::ChunkPos>();
    let (result_tx, result_rx) = mpsc::channel::<GeneratedChunk>();

    let request_rx = Arc::new(Mutex::new(request_rx));
    for worker_index in 0..1 {
        let request_rx = Arc::clone(&request_rx);
        let result_tx = result_tx.clone();
        let classic_terrain = Arc::clone(&classic_terrain);
        let model_map = model_map.clone();
        let atlas_uv = atlas_uv.clone();
        thread::Builder::new()
            .name(format!("chunk-generation-{worker_index}"))
            .spawn(move || {
                loop {
                    let chunk_pos = {
                        let request_rx =
                            request_rx.lock().expect("chunk generation queue poisoned");
                        request_rx.recv()
                    };
                    let Ok(chunk_pos) = chunk_pos else {
                        break;
                    };

                    let mut generated_world = world::World::with_seed(seed);
                    generate_worldgen_chunk(
                        &mut generated_world,
                        &classic_terrain,
                        &noise_settings,
                        chunk_pos,
                    );
                    for neighbor in [
                        world::ChunkPos(chunk_pos.0 + 1, chunk_pos.1),
                        world::ChunkPos(chunk_pos.0 - 1, chunk_pos.1),
                        world::ChunkPos(chunk_pos.0, chunk_pos.1 + 1),
                        world::ChunkPos(chunk_pos.0, chunk_pos.1 - 1),
                    ] {
                        if let Some(chunk) = classic_terrain.chunk(neighbor) {
                            generated_world.insert_generated_chunk(chunk);
                        }
                    }

                    if let Some(chunk) = generated_world.chunk(chunk_pos) {
                        let mesh = mesher::mesh_chunk(
                            chunk,
                            &generated_world,
                            &biome_source,
                            &noise_settings,
                            &model_map,
                            &atlas_uv,
                        );
                        let Some(chunk) = generated_world.take_chunk(chunk_pos) else {
                            continue;
                        };
                        if result_tx
                            .send(GeneratedChunk {
                                pos: chunk_pos,
                                chunk,
                                mesh,
                            })
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            })
            .expect("failed to start chunk generation worker");
    }

    (request_tx, result_rx)
}

fn start_mesh_generation_worker(
    seed: u64,
    noise_settings: worldgen::NoiseSettings,
    biome_source: worldgen::BiomeSource,
    model_map: std::collections::HashMap<String, model::BlockModel>,
    atlas_uv: std::collections::HashMap<String, [f32; 4]>,
) -> (mpsc::Sender<MeshJob>, mpsc::Receiver<GeneratedMesh>) {
    let (request_tx, request_rx) = mpsc::channel::<MeshJob>();
    let (result_tx, result_rx) = mpsc::channel::<GeneratedMesh>();

    let request_rx = Arc::new(Mutex::new(request_rx));
    for worker_index in 0..worker_count().min(2) {
        let request_rx = Arc::clone(&request_rx);
        let result_tx = result_tx.clone();
        let model_map = model_map.clone();
        let atlas_uv = atlas_uv.clone();
        thread::Builder::new()
            .name(format!("chunk-meshing-{worker_index}"))
            .spawn(move || {
                loop {
                    let job = {
                        let request_rx = request_rx.lock().expect("chunk meshing queue poisoned");
                        request_rx.recv()
                    };
                    let Ok(job) = job else {
                        break;
                    };

                    let mut snapshot = world::World::with_seed(seed);
                    for chunk in job.chunks {
                        snapshot.insert_generated_chunk(chunk);
                    }

                    if let Some(chunk) = snapshot.chunk(job.pos) {
                        let mesh = mesher::mesh_chunk(
                            chunk,
                            &snapshot,
                            &biome_source,
                            &noise_settings,
                            &model_map,
                            &atlas_uv,
                        );
                        if result_tx
                            .send(GeneratedMesh { pos: job.pos, mesh })
                            .is_err()
                        {
                            break;
                        }
                    }
                }
            })
            .expect("failed to start chunk meshing worker");
    }

    (request_tx, result_rx)
}

fn worker_count() -> usize {
    thread::available_parallelism()
        .map(|count| count.get().saturating_sub(1).clamp(1, 4))
        .unwrap_or(2)
}

fn generate_worldgen_chunk(
    world: &mut world::World,
    classic_terrain: &classic_worldgen::ClassicTerrain,
    noise_settings: &worldgen::NoiseSettings,
    chunk_pos: world::ChunkPos,
) {
    let id = |s: &str| block::BlockId(s.to_string());

    if !(0..CLASSIC_WORLD_CHUNKS).contains(&chunk_pos.0)
        || !(0..CLASSIC_WORLD_CHUNKS).contains(&chunk_pos.1)
    {
        worldgen::generate_surrounding_ocean(
            world,
            chunk_pos,
            noise_settings,
            id("bedrock"),
            id("stone"),
            id("water"),
        );
        return;
    }

    if let Some(chunk) = classic_terrain.chunk(chunk_pos) {
        world.insert_generated_chunk(chunk);
    }
}

fn create_demo_world(
    classic_terrain: &classic_worldgen::ClassicTerrain,
    noise_settings: &worldgen::NoiseSettings,
) -> world::World {
    let mut world = world::World::with_seed(DEMO_WORLD_SEED);

    let [spawn_x, _, spawn_z] = classic_terrain.spawn();
    let center = world::BlockPos(spawn_x, 0, spawn_z).chunk_pos();
    let spawn_chunks = worldgen::spawn_area_chunks(center, DEMO_SPAWN_CHUNK_RADIUS);

    for &chunk_pos in &spawn_chunks {
        generate_worldgen_chunk(&mut world, classic_terrain, noise_settings, chunk_pos);
    }
    world.clear_persistent_chunks();

    world
}

fn save_path() -> PathBuf {
    PathBuf::from("saves").join("world.json")
}

fn save_game(
    world: &world::World,
    player_position: Vec3,
    hotbar_selected: usize,
    hotbar_slots: [InventorySlot; HOTBAR_SIZE],
    inventory_slots: [InventorySlot; INVENTORY_SIZE],
    spawn_position: Vec3,
    mobs: &[ClassicMob],
) -> Result<(), Box<dyn std::error::Error>> {
    let save = SaveGame {
        format_version: CLASSIC_SAVE_VERSION,
        seed: world.seed(),
        player: SavePlayer {
            position: [player_position.x, player_position.y, player_position.z],
            spawn_position: Some(spawn_position.to_array()),
            hotbar_selected,
            hotbar_counts: hotbar_counts_from_slots(&hotbar_slots).to_vec(),
            hotbar_slots: hotbar_slots.to_vec(),
            inventory_slots: inventory_slots.to_vec(),
        },
        mobs: mobs
            .iter()
            .map(|mob| SaveMob {
                position: mob.position.to_array(),
                velocity: mob.velocity.to_array(),
                heading: mob.heading,
                turn_velocity: mob.turn_velocity,
                time_offset: mob.time_offset,
                grounded: mob.grounded,
            })
            .collect(),
        chunks: world
            .persistent_chunks()
            .map(|chunk| SaveChunk {
                pos: chunk.pos(),
                runs: encode_block_runs(chunk.blocks()),
                properties: encode_block_properties(chunk),
            })
            .collect(),
    };

    let path = save_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    write_save_atomically(&path, &serde_json::to_string_pretty(&save)?)?;
    Ok(())
}

fn write_save_atomically(path: &Path, contents: &str) -> Result<(), Box<dyn std::error::Error>> {
    let temporary = path.with_extension("json.tmp");
    let backup = path.with_extension("json.bak");
    let mut file = std::fs::File::create(&temporary)?;
    file.write_all(contents.as_bytes())?;
    file.sync_all()?;

    if path.exists() {
        if backup.exists() {
            std::fs::remove_file(&backup)?;
        }
        std::fs::rename(path, &backup)?;
    }
    if let Err(error) = std::fs::rename(&temporary, path) {
        if backup.exists() {
            let _ = std::fs::rename(&backup, path);
        }
        return Err(error.into());
    }
    if backup.exists()
        && let Err(error) = std::fs::remove_file(&backup)
    {
        log::warn!(target: "save", "Failed to remove save backup {:?}: {error}", backup);
    }
    Ok(())
}

fn load_saved_game(world: &mut world::World) -> Option<(SavePlayer, Vec<ClassicMob>)> {
    let primary_path = save_path();
    let backup_path = primary_path.with_extension("json.bak");
    let path = if primary_path.exists() {
        primary_path
    } else {
        backup_path
    };
    let mut header = String::new();
    let mut file = match std::fs::File::open(&path) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            log::warn!(target: "save", "Failed to read save {:?}: {error}", path);
            return None;
        }
    };
    if let Err(error) = (&mut file).take(1024).read_to_string(&mut header) {
        log::warn!(target: "save", "Failed to read save header {:?}: {error}", path);
        return None;
    }
    if saved_format_version(&header) != Some(CLASSIC_SAVE_VERSION) {
        log::info!(target: "save", "Ignoring incompatible save format");
        return None;
    }

    let text = match std::fs::read_to_string(&path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return None,
        Err(error) => {
            log::warn!(target: "save", "Failed to read save {:?}: {error}", path);
            return None;
        }
    };

    let save = match serde_json::from_str::<SaveGame>(&text) {
        Ok(save) => save,
        Err(error) => {
            log::warn!(target: "save", "Failed to parse save {:?}: {error}", path);
            return None;
        }
    };

    if save.seed != world.seed() {
        log::warn!(target: "save", "Ignoring save with seed {} for world seed {}", save.seed, world.seed());
        return None;
    }
    for saved_chunk in save.chunks {
        let Some(blocks) = decode_block_runs(&saved_chunk.runs) else {
            log::warn!(target: "save", "Skipping malformed chunk {:?}", saved_chunk.pos);
            continue;
        };

        let mut chunk = world::Chunk::new(saved_chunk.pos);
        for y in 0..world::CHUNK_SIZE_Y {
            for z in 0..world::CHUNK_SIZE_Z {
                for x in 0..world::CHUNK_SIZE_X {
                    let index =
                        y * world::CHUNK_SIZE_Z * world::CHUNK_SIZE_X + z * world::CHUNK_SIZE_X + x;
                    chunk.set_block(x, y, z, blocks[index].clone());
                }
            }
        }
        for property in saved_chunk.properties {
            if property.index >= world::CHUNK_VOLUME {
                continue;
            }
            let x = property.index % world::CHUNK_SIZE_X;
            let z = (property.index / world::CHUNK_SIZE_X) % world::CHUNK_SIZE_Z;
            let y = property.index / (world::CHUNK_SIZE_X * world::CHUNK_SIZE_Z);
            chunk.set_property(x, y, z, property.prop, property.value);
        }
        world.insert_chunk(chunk);
    }

    log::info!(target: "save", "Loaded world from {:?}", path);
    let mobs = save
        .mobs
        .into_iter()
        .filter(|mob| {
            mob.position.iter().all(|value| value.is_finite())
                && mob.velocity.iter().all(|value| value.is_finite())
                && mob.heading.is_finite()
                && mob.turn_velocity.is_finite()
                && mob.time_offset.is_finite()
        })
        .take(256)
        .map(|mob| {
            let position = Vec3::from_array(mob.position);
            ClassicMob {
                position,
                previous_position: position,
                velocity: Vec3::from_array(mob.velocity),
                heading: mob.heading,
                turn_velocity: mob.turn_velocity,
                time_offset: mob.time_offset,
                grounded: mob.grounded,
            }
        })
        .collect();
    Some((save.player, mobs))
}

fn saved_format_version(header: &str) -> Option<u32> {
    let marker = "\"format_version\"";
    let value = header.split_once(marker)?.1.split_once(':')?.1.trim_start();
    let digits = value
        .chars()
        .take_while(char::is_ascii_digit)
        .collect::<String>();
    digits.parse().ok()
}

fn encode_block_runs(blocks: &[block::BlockId; world::CHUNK_VOLUME]) -> Vec<SaveBlockRun> {
    let mut runs = Vec::new();
    let Some(first) = blocks.first() else {
        return runs;
    };

    let mut current = first.clone();
    let mut len = 0;
    for block in blocks {
        if *block == current {
            len += 1;
        } else {
            runs.push(SaveBlockRun {
                block: current,
                len,
            });
            current = block.clone();
            len = 1;
        }
    }
    runs.push(SaveBlockRun {
        block: current,
        len,
    });
    runs
}

fn encode_block_properties(chunk: &world::Chunk) -> Vec<SaveBlockProperty> {
    let mut properties = Vec::new();
    for (index, props) in chunk.property_overrides() {
        for &(prop, value) in props {
            properties.push(SaveBlockProperty { index, prop, value });
        }
    }
    properties
}

fn decode_block_runs(runs: &[SaveBlockRun]) -> Option<Vec<block::BlockId>> {
    let mut blocks = Vec::with_capacity(world::CHUNK_VOLUME);
    for run in runs {
        if run.len == 0 || blocks.len().saturating_add(run.len) > world::CHUNK_VOLUME {
            return None;
        }
        blocks.extend(std::iter::repeat_n(run.block.clone(), run.len));
    }

    (blocks.len() == world::CHUNK_VOLUME).then_some(blocks)
}

fn hotbar_counts_from_slots(slots: &[InventorySlot; HOTBAR_SIZE]) -> [u32; HOTBAR_SIZE] {
    std::array::from_fn(|i| slots[i].count)
}

fn hotbar_render_slots(slots: &[InventorySlot; HOTBAR_SIZE]) -> [Option<usize>; HOTBAR_SIZE] {
    std::array::from_fn(|i| slots[i].item)
}

fn hotbar_render_counts(slots: &[InventorySlot; HOTBAR_SIZE]) -> [u32; HOTBAR_SIZE] {
    std::array::from_fn(|i| slots[i].count)
}

fn inventory_render_slots(
    slots: &[InventorySlot; INVENTORY_SIZE],
) -> [Option<usize>; INVENTORY_SIZE] {
    std::array::from_fn(|i| slots[i].item)
}

fn inventory_render_counts(slots: &[InventorySlot; INVENTORY_SIZE]) -> [u32; INVENTORY_SIZE] {
    std::array::from_fn(|i| slots[i].count)
}

fn inventory_from_saved_player(
    player: &SavePlayer,
) -> (
    [InventorySlot; HOTBAR_SIZE],
    [InventorySlot; INVENTORY_SIZE],
) {
    let mut hotbar_slots = [InventorySlot::default(); HOTBAR_SIZE];
    for (index, slot) in player
        .hotbar_slots
        .iter()
        .copied()
        .take(HOTBAR_SIZE)
        .enumerate()
    {
        hotbar_slots[index] = sanitize_inventory_slot(slot);
    }
    if hotbar_slots.iter().all(InventorySlot::is_empty) {
        for (index, count) in player.hotbar_counts.iter().copied().enumerate() {
            if index < HOTBAR_SIZE && count > 0 {
                hotbar_slots[index] = InventorySlot {
                    item: (index < ITEM_TYPE_COUNT).then_some(index),
                    count: count.min(MAX_STACK_SIZE),
                };
            }
        }
    }

    let mut inventory_slots = [InventorySlot::default(); INVENTORY_SIZE];
    for (index, slot) in player
        .inventory_slots
        .iter()
        .copied()
        .take(INVENTORY_SIZE)
        .enumerate()
    {
        inventory_slots[index] = sanitize_inventory_slot(slot);
    }

    (hotbar_slots, inventory_slots)
}

fn sanitize_inventory_slot(slot: InventorySlot) -> InventorySlot {
    match slot.item {
        Some(item) if item < ITEM_TYPE_COUNT && slot.count > 0 => InventorySlot {
            item: Some(item),
            count: slot.count.min(MAX_STACK_SIZE),
        },
        _ => InventorySlot::default(),
    }
}

fn add_item_to_inventory(
    hotbar: &mut [InventorySlot; HOTBAR_SIZE],
    inventory: &mut [InventorySlot; INVENTORY_SIZE],
    item: usize,
    count: u32,
) {
    add_stack_to_inventory(
        hotbar,
        inventory,
        InventorySlot {
            item: Some(item),
            count,
        },
    );
}

fn add_stack_to_inventory(
    hotbar: &mut [InventorySlot; HOTBAR_SIZE],
    inventory: &mut [InventorySlot; INVENTORY_SIZE],
    mut stack: InventorySlot,
) {
    if stack.is_empty() {
        return;
    }

    for slot in hotbar.iter_mut().chain(inventory.iter_mut()) {
        if slot.item == stack.item && slot.count < MAX_STACK_SIZE {
            let moved = (MAX_STACK_SIZE - slot.count).min(stack.count);
            slot.count += moved;
            stack.count -= moved;
            if stack.count == 0 {
                return;
            }
        }
    }

    for slot in hotbar.iter_mut().chain(inventory.iter_mut()) {
        if slot.is_empty() {
            let moved = MAX_STACK_SIZE.min(stack.count);
            *slot = InventorySlot {
                item: stack.item,
                count: moved,
            };
            stack.count -= moved;
            if stack.count == 0 {
                return;
            }
        }
    }
}

fn remove_one_from_slot(slot: &mut InventorySlot) {
    slot.count = slot.count.saturating_sub(1);
    if slot.count == 0 {
        slot.item = None;
    }
}

fn click_inventory_slot(slot: &mut InventorySlot, carried: &mut InventorySlot) {
    if carried.is_empty() {
        std::mem::swap(slot, carried);
    } else if slot.is_empty() {
        std::mem::swap(slot, carried);
    } else if slot.item == carried.item && slot.count < MAX_STACK_SIZE {
        let moved = (MAX_STACK_SIZE - slot.count).min(carried.count);
        slot.count += moved;
        carried.count -= moved;
        if carried.count == 0 {
            carried.item = None;
        }
    } else {
        std::mem::swap(slot, carried);
    }
}

fn inventory_slot_at(x: f32, y: f32, width: u32, height: u32) -> Option<InventorySlotTarget> {
    let width = width.max(1) as f32;
    let height = height.max(1) as f32;
    let slot = 42.0;
    let gap = 4.0;
    let total_width = slot * 9.0 + gap * 8.0;
    let left = width * 0.5 - total_width * 0.5;
    let top = height * 0.5 - 112.0;

    for row in 0..3 {
        for col in 0..9 {
            let sx = left + col as f32 * (slot + gap);
            let sy = top + row as f32 * (slot + gap);
            if x >= sx && x <= sx + slot && y >= sy && y <= sy + slot {
                return Some(InventorySlotTarget::Inventory(row * 9 + col));
            }
        }
    }

    let hotbar_left = width * 0.5 - (slot * 5.0 + gap * 4.0) * 0.5;
    let hotbar_top = top + 3.0 * (slot + gap) + 20.0;
    for col in 0..HOTBAR_SIZE {
        let sx = hotbar_left + col as f32 * (slot + gap);
        if x >= sx && x <= sx + slot && y >= hotbar_top && y <= hotbar_top + slot {
            return Some(InventorySlotTarget::Hotbar(col));
        }
    }

    None
}

fn camera_fog_environment(world: &world::World, position: Vec3) -> renderer::FogEnvironment {
    let block = world.get_block(world::BlockPos(
        position.x as i32,
        (position.y + 0.12) as i32,
        position.z as i32,
    ));
    match block.0.as_str() {
        "water" => renderer::FogEnvironment::Water,
        "lava" => renderer::FogEnvironment::Lava,
        _ => renderer::FogEnvironment::Air,
    }
}

fn water_surface_height(world: &world::World, pos: world::BlockPos) -> f32 {
    if world.get_block(world::BlockPos(pos.0, pos.1 + 1, pos.2)).0 == "water" {
        return 1.0;
    }
    match water_level_at(world, pos) {
        WATER_SOURCE_LEVEL => 14.0 / 16.0,
        level if level >= WATER_FALLING_LEVEL => 1.0,
        level => ((8 - level.min(WATER_MAX_HORIZONTAL_LEVEL)) as f32 / 8.0).max(1.0 / 8.0),
    }
}

fn water_block_at_point(world: &world::World, point: Vec3) -> Option<world::BlockPos> {
    let pos = camera_block_pos(point);
    if world.get_block(pos).0 != "water" {
        return None;
    }
    let local_y = point.y - point.y.floor();
    (local_y < water_surface_height(world, pos)).then_some(pos)
}

fn player_water_block(
    world: &world::World,
    eye_position: Vec3,
    crouching: bool,
) -> Option<world::BlockPos> {
    let eye_height = if crouching {
        PLAYER_CROUCH_EYE_HEIGHT
    } else {
        PLAYER_EYE_HEIGHT
    };
    let height = if crouching { 1.5 } else { PLAYER_HEIGHT };
    let feet_y = eye_position.y - eye_height;
    [feet_y + 0.1, feet_y + height * 0.5, eye_position.y - 0.05]
        .into_iter()
        .find_map(|y| water_block_at_point(world, Vec3::new(eye_position.x, y, eye_position.z)))
}

fn player_in_water(world: &world::World, eye_position: Vec3, crouching: bool) -> bool {
    player_water_block(world, eye_position, crouching).is_some()
}

fn water_current_at(world: &world::World, eye_position: Vec3, crouching: bool) -> Vec3 {
    let Some(pos) = player_water_block(world, eye_position, crouching) else {
        return Vec3::ZERO;
    };
    let level = water_level_at(world, pos);
    let depth = water_flow_depth(level);
    let mut current = Vec3::ZERO;
    for ((dx, dz), neighbor) in horizontal_directions(pos) {
        let neighbor_block = world.get_block(neighbor);
        let difference = if neighbor_block.0 == "water" {
            depth - water_flow_depth(water_level_at(world, neighbor))
        } else if neighbor_block.0.is_empty() {
            depth
        } else {
            continue;
        };
        current += Vec3::new(dx as f32, 0.0, dz as f32) * difference;
    }
    if level >= WATER_FALLING_LEVEL {
        current.y -= 1.0;
    }
    current.try_normalize().unwrap_or(Vec3::ZERO)
}

fn water_flow_depth(level: u8) -> f32 {
    if level == WATER_SOURCE_LEVEL || level >= WATER_FALLING_LEVEL {
        8.0
    } else {
        (8 - level.min(WATER_MAX_HORIZONTAL_LEVEL)) as f32
    }
}

fn spawn_eye_position(world: &world::World) -> Vec3 {
    let block_x = CLASSIC_WORLD_SIZE / 2;
    let block_z = CLASSIC_WORLD_SIZE / 2;
    let ground_y = (0..world::CHUNK_SIZE_Y as i32)
        .rev()
        .find(|y| is_player_solid_block(&world.get_block(world::BlockPos(block_x, *y, block_z))))
        .unwrap_or(16);

    Vec3::new(
        block_x as f32 + 0.5,
        ground_y as f32 + 1.0 + PLAYER_EYE_HEIGHT,
        block_z as f32 + 0.5,
    )
}

fn raycast_block(world: &world::World, origin: Vec3, direction: Vec3) -> Option<BlockTarget> {
    let direction = direction.try_normalize()?;
    let mut previous = camera_block_pos(origin);
    let mut distance = 0.0;

    while distance <= BLOCK_REACH {
        let sample = origin + direction * distance;
        let block_pos = camera_block_pos(sample);
        let block = world.get_block(block_pos);
        if is_targetable_block(&block) && classic_pick_volume_contains(origin, block_pos) {
            return Some(BlockTarget {
                block_pos,
                place_pos: previous,
            });
        }
        if block_pos != previous {
            previous = block_pos;
        }
        distance += BLOCK_RAY_STEP;
    }

    None
}

fn player_move_direction(camera: &FirstPersonCamera, input: &InputState) -> Vec3 {
    let mut movement = Vec3::ZERO;

    if input.is_key_pressed(KeyCode::KeyW) || input.is_key_pressed(KeyCode::ArrowUp) {
        movement += camera.yaw_forward();
    }
    if input.is_key_pressed(KeyCode::KeyS) || input.is_key_pressed(KeyCode::ArrowDown) {
        movement -= camera.yaw_forward();
    }
    if input.is_key_pressed(KeyCode::KeyA) || input.is_key_pressed(KeyCode::ArrowLeft) {
        movement -= camera.yaw_right();
    }
    if input.is_key_pressed(KeyCode::KeyD) || input.is_key_pressed(KeyCode::ArrowRight) {
        movement += camera.yaw_right();
    }

    movement.try_normalize().unwrap_or(Vec3::ZERO)
}

fn accelerate_horizontal(velocity: &mut Vec3, direction: Vec3, acceleration: f32) {
    velocity.x += direction.x * acceleration;
    velocity.z += direction.z * acceleration;
}

fn player_fluid(world: &world::World, eye_position: Vec3) -> Option<&'static str> {
    let bounds = Aabb::player(eye_position);
    let water_bounds = Aabb {
        min: bounds.min + Vec3::Y * 0.4,
        max: bounds.max - Vec3::Y * 0.4,
    };
    if aabb_contains_fluid(world, water_bounds, "water") {
        Some("water")
    } else if aabb_contains_fluid(world, bounds, "lava") {
        Some("lava")
    } else {
        None
    }
}

fn aabb_contains_fluid(world: &world::World, bounds: Aabb, fluid: &str) -> bool {
    block_positions(bounds)
        .any(|pos| world.get_block(pos).0 == fluid && bounds.intersects(Aabb::block(pos)))
}

fn move_player_with_collisions(
    world: &world::World,
    position: Vec3,
    velocity: Vec3,
) -> (Vec3, Vec3, bool, bool) {
    let (bounds, velocity, grounded, horizontal_collision) =
        move_entity_with_collisions(world, Aabb::player(position), velocity);
    (
        Vec3::new(
            (bounds.min.x + bounds.max.x) * 0.5,
            bounds.min.y + PLAYER_EYE_HEIGHT,
            (bounds.min.z + bounds.max.z) * 0.5,
        ),
        velocity,
        grounded,
        horizontal_collision,
    )
}

fn move_mob_with_collisions(
    world: &world::World,
    feet_position: Vec3,
    velocity: Vec3,
) -> (Vec3, Vec3, bool, bool) {
    let (bounds, velocity, grounded, horizontal_collision) =
        move_entity_with_collisions(world, Aabb::feet(feet_position), velocity);
    (
        Vec3::new(
            (bounds.min.x + bounds.max.x) * 0.5,
            bounds.min.y,
            (bounds.min.z + bounds.max.z) * 0.5,
        ),
        velocity,
        grounded,
        horizontal_collision,
    )
}

fn move_entity_with_collisions(
    world: &world::World,
    mut bounds: Aabb,
    mut velocity: Vec3,
) -> (Aabb, Vec3, bool, bool) {
    let requested = velocity;
    let colliders = entity_colliders(world, bounds.expand(requested));

    for collider in &colliders {
        velocity.y = collider.clip_y(bounds, velocity.y);
    }
    bounds = bounds.moved(Vec3::Y * velocity.y);
    for collider in &colliders {
        velocity.x = collider.clip_x(bounds, velocity.x);
    }
    bounds = bounds.moved(Vec3::X * velocity.x);
    for collider in &colliders {
        velocity.z = collider.clip_z(bounds, velocity.z);
    }
    bounds = bounds.moved(Vec3::Z * velocity.z);

    let horizontal_collision = requested.x != velocity.x || requested.z != velocity.z;
    let grounded = requested.y != velocity.y && requested.y < 0.0;
    if requested.x != velocity.x {
        velocity.x = 0.0;
    }
    if requested.y != velocity.y {
        velocity.y = 0.0;
    }
    if requested.z != velocity.z {
        velocity.z = 0.0;
    }
    (bounds, velocity, grounded, horizontal_collision)
}

fn player_is_free_offset(world: &world::World, eye_position: Vec3, offset: Vec3) -> bool {
    let bounds = Aabb::player(eye_position).moved(offset);
    let clear_of_blocks = !entity_colliders(world, bounds)
        .into_iter()
        .any(|collider| bounds.intersects(collider));
    clear_of_blocks
        && !aabb_contains_fluid(world, bounds, "water")
        && !aabb_contains_fluid(world, bounds, "lava")
}

fn player_collides(world: &world::World, eye_position: Vec3, _crouching: bool) -> bool {
    let bounds = Aabb::player(eye_position);
    entity_colliders(world, bounds)
        .into_iter()
        .any(|collider| bounds.intersects(collider))
}

fn resolve_player_overlap(world: &world::World, mut eye_position: Vec3) -> Vec3 {
    for _ in 0..=world::CHUNK_SIZE_Y {
        if !player_collides(world, eye_position, false) {
            break;
        }
        eye_position.y += 1.001;
    }
    eye_position
}

fn valid_player_eye_position(position: Vec3) -> bool {
    position.is_finite()
        && position.x >= PLAYER_RADIUS
        && position.x < CLASSIC_WORLD_SIZE as f32 - PLAYER_RADIUS
        && position.z >= PLAYER_RADIUS
        && position.z < CLASSIC_WORLD_SIZE as f32 - PLAYER_RADIUS
        && position.y >= PLAYER_EYE_HEIGHT
        && position.y < world::CHUNK_SIZE_Y as f32 + 64.0
}

fn is_player_solid_block(block: &block::BlockId) -> bool {
    !matches!(block.0.as_str(), "" | "water" | "lava" | "oak_sapling")
}

fn classic_entity_brightness(world: &world::World, position: Vec3) -> f32 {
    let x = position.x as i32;
    let y = position.y as i32;
    let z = position.z as i32;
    if x < 0
        || z < 0
        || x >= CLASSIC_WORLD_SIZE
        || z >= CLASSIC_WORLD_SIZE
        || !(0..world::CHUNK_SIZE_Y as i32).contains(&y)
    {
        return 1.0;
    }
    let light_depth = (1..world::CHUNK_SIZE_Y as i32)
        .rev()
        .find(|&scan_y| {
            !matches!(
                world.get_block(world::BlockPos(x, scan_y, z)).0.as_str(),
                "" | "oak_leaves" | "oak_sapling" | "water" | "lava" | "glass"
            )
        })
        .map_or(1, |blocker_y| blocker_y + 1);
    if y >= light_depth { 1.0 } else { 0.5 }
}

#[derive(Clone, Copy, Debug)]
struct Aabb {
    min: Vec3,
    max: Vec3,
}

impl Aabb {
    fn player(eye: Vec3) -> Self {
        let mut feet = eye - Vec3::Y * PLAYER_EYE_HEIGHT;
        let block_boundary = feet.y.round();
        if (feet.y - block_boundary).abs() < 1.0e-4 {
            feet.y = block_boundary;
        }
        Self::feet(feet)
    }

    fn feet(feet: Vec3) -> Self {
        Self {
            min: feet - Vec3::new(PLAYER_RADIUS, 0.0, PLAYER_RADIUS),
            max: feet + Vec3::new(PLAYER_RADIUS, PLAYER_HEIGHT, PLAYER_RADIUS),
        }
    }

    fn block(pos: world::BlockPos) -> Self {
        let min = Vec3::new(pos.0 as f32, pos.1 as f32, pos.2 as f32);
        Self {
            min,
            max: min + Vec3::ONE,
        }
    }

    fn expand(self, movement: Vec3) -> Self {
        Self {
            min: self.min + movement.min(Vec3::ZERO),
            max: self.max + movement.max(Vec3::ZERO),
        }
    }

    fn moved(self, movement: Vec3) -> Self {
        Self {
            min: self.min + movement,
            max: self.max + movement,
        }
    }

    fn intersects(self, other: Self) -> bool {
        self.max.x > other.min.x
            && self.min.x < other.max.x
            && self.max.y > other.min.y
            && self.min.y < other.max.y
            && self.max.z > other.min.z
            && self.min.z < other.max.z
    }

    fn clip_x(self, moving: Self, mut amount: f32) -> f32 {
        if moving.max.y <= self.min.y
            || moving.min.y >= self.max.y
            || moving.max.z <= self.min.z
            || moving.min.z >= self.max.z
        {
            return amount;
        }
        if amount > 0.0 && moving.max.x <= self.min.x {
            amount = amount.min(self.min.x - moving.max.x);
        } else if amount < 0.0 && moving.min.x >= self.max.x {
            amount = amount.max(self.max.x - moving.min.x);
        }
        amount
    }

    fn clip_y(self, moving: Self, mut amount: f32) -> f32 {
        if moving.max.x <= self.min.x
            || moving.min.x >= self.max.x
            || moving.max.z <= self.min.z
            || moving.min.z >= self.max.z
        {
            return amount;
        }
        if amount > 0.0 && moving.max.y <= self.min.y {
            amount = amount.min(self.min.y - moving.max.y);
        } else if amount < 0.0 && moving.min.y >= self.max.y {
            amount = amount.max(self.max.y - moving.min.y);
        }
        amount
    }

    fn clip_z(self, moving: Self, mut amount: f32) -> f32 {
        if moving.max.x <= self.min.x
            || moving.min.x >= self.max.x
            || moving.max.y <= self.min.y
            || moving.min.y >= self.max.y
        {
            return amount;
        }
        if amount > 0.0 && moving.max.z <= self.min.z {
            amount = amount.min(self.min.z - moving.max.z);
        } else if amount < 0.0 && moving.min.z >= self.max.z {
            amount = amount.max(self.max.z - moving.min.z);
        }
        amount
    }
}

fn block_positions(bounds: Aabb) -> impl Iterator<Item = world::BlockPos> {
    let min_x = bounds.min.x.floor() as i32;
    let max_x = bounds.max.x.floor() as i32 + 1;
    let min_y = bounds.min.y.floor() as i32;
    let max_y = bounds.max.y.floor() as i32 + 1;
    let min_z = bounds.min.z.floor() as i32;
    let max_z = bounds.max.z.floor() as i32 + 1;
    (min_x..max_x).flat_map(move |x| {
        (min_y..max_y).flat_map(move |y| (min_z..max_z).map(move |z| world::BlockPos(x, y, z)))
    })
}

fn entity_colliders(world: &world::World, bounds: Aabb) -> Vec<Aabb> {
    block_positions(bounds)
        .filter_map(|pos| {
            let outside_horizontal = pos.0 < 0
                || pos.2 < 0
                || pos.0 >= CLASSIC_WORLD_SIZE
                || pos.2 >= CLASSIC_WORLD_SIZE;
            if outside_horizontal {
                return Some(Aabb::block(pos));
            }
            if pos.1 < 0 {
                return Some(Aabb::block(pos));
            }
            if pos.1 >= world::CHUNK_SIZE_Y as i32 {
                return None;
            }
            is_player_solid_block(&world.get_block(pos)).then(|| Aabb::block(pos))
        })
        .collect()
}

fn aabb_chunks_loaded(world: &world::World, bounds: Aabb) -> bool {
    let min =
        world::BlockPos(bounds.min.x.floor() as i32, 0, bounds.min.z.floor() as i32).chunk_pos();
    let max = world::BlockPos(
        (bounds.max.x - 1.0e-4).floor() as i32,
        0,
        (bounds.max.z - 1.0e-4).floor() as i32,
    )
    .chunk_pos();
    (min.0..=max.0).all(|x| (min.1..=max.1).all(|z| world.is_chunk_loaded(world::ChunkPos(x, z))))
}

fn is_targetable_block(block: &block::BlockId) -> bool {
    !matches!(block.0.as_str(), "" | "water" | "lava")
}

fn classic_pick_volume_contains(eye: Vec3, block: world::BlockPos) -> bool {
    let feet_y = eye.y - PLAYER_EYE_HEIGHT;
    let min = Vec3::new(
        eye.x - PLAYER_RADIUS - 2.5,
        feet_y - 2.5,
        eye.z - PLAYER_RADIUS - 2.5,
    );
    let max = Vec3::new(
        eye.x + PLAYER_RADIUS + 2.5,
        feet_y + PLAYER_HEIGHT + 2.5,
        eye.z + PLAYER_RADIUS + 2.5,
    );
    let block_min = Vec3::new(block.0 as f32, block.1 as f32, block.2 as f32);
    let block_max = block_min + Vec3::ONE;
    block_min.cmplt(max).all() && block_max.cmpgt(min).all()
}

fn classic_break_replacement(pos: world::BlockPos) -> block::BlockId {
    let on_edge = pos.0 == 0
        || pos.2 == 0
        || pos.0 == CLASSIC_WORLD_SIZE - 1
        || pos.2 == CLASSIC_WORLD_SIZE - 1;
    if on_edge && (30..32).contains(&pos.1) {
        block::BlockId("water".to_string())
    } else {
        block::BlockId::AIR.clone()
    }
}

fn block_intersects_mob(block: world::BlockPos, mob_feet: Vec3) -> bool {
    let block_min = Vec3::new(block.0 as f32, block.1 as f32, block.2 as f32);
    let block_max = block_min + Vec3::ONE;
    let mob_min = mob_feet - Vec3::new(PLAYER_RADIUS, 0.0, PLAYER_RADIUS);
    let mob_max = mob_feet + Vec3::new(PLAYER_RADIUS, PLAYER_HEIGHT, PLAYER_RADIUS);
    block_min.x < mob_max.x
        && block_max.x > mob_min.x
        && block_min.y < mob_max.y
        && block_max.y > mob_min.y
        && block_min.z < mob_max.z
        && block_max.z > mob_min.z
}

fn settle_falling_column(
    world: &mut world::World,
    changed_pos: world::BlockPos,
) -> Vec<(world::BlockPos, world::BlockPos)> {
    let top = world::CHUNK_SIZE_Y as i32 - 1;
    let changed_block = world.get_block(changed_pos);
    let start_y = if matches!(changed_block.0.as_str(), "sand" | "gravel") {
        changed_pos.1
    } else {
        changed_pos.1 + 1
    };
    let mut moved = Vec::new();
    for y in start_y.max(1)..=top {
        let source = world::BlockPos(changed_pos.0, y, changed_pos.2);
        let block = world.get_block(source);
        if !matches!(block.0.as_str(), "sand" | "gravel") {
            continue;
        }

        let mut destination_y = y;
        while destination_y > 0
            && world.get_block(world::BlockPos(
                changed_pos.0,
                destination_y - 1,
                changed_pos.2,
            )) == block::BlockId::AIR
        {
            destination_y -= 1;
        }
        if destination_y == y {
            continue;
        }

        let destination = world::BlockPos(changed_pos.0, destination_y, changed_pos.2);
        world.set_block(source, block::BlockId::AIR.clone());
        world.set_block(destination, block);
        moved.push((source, destination));
    }
    moved
}

fn hotbar_slot_for_block(block: &block::BlockId) -> Option<usize> {
    HOTBAR_BLOCKS.iter().position(|id| block.0 == *id)
}

fn block_drop(block: &block::BlockId) -> block::BlockId {
    match block.0.as_str() {
        "grass_block" => block::BlockId("dirt".to_string()),
        _ => block.clone(),
    }
}

fn block_break_seconds(block: &block::BlockId) -> f32 {
    match block.0.as_str() {
        "" | "water" | "lava" => 0.0,
        "bedrock" => f32::INFINITY,
        "oak_leaves" => 0.2,
        "glass" => 0.3,
        "dirt" | "sand" => 0.5,
        "grass_block" | "gravel" => 0.6,
        "stone" => 1.5,
        "oak_log" | "oak_planks" => 2.0,
        "coal_ore" | "iron_ore" | "gold_ore" => 3.0,
        _ => 1.0,
    }
}

fn main() {
    logging::init().expect("Failed to initialize logger");
    log::info!(target: "startup", "FerrumCraft v{} initializing", env!("CARGO_PKG_VERSION"));
    // Set FERRUM_LOG=debug to see more detail.

    let _resources = resource::ResourceManager::new(".");
    let _lang_table = match lang::TranslationTable::load(&_resources, "ferrumcraft", "en_us") {
        Ok(table) => {
            log::info!(target: "lang", "Loaded {} translation entries", table.len());
            table
        }
        Err(e) => {
            log::warn!(target: "lang", "Failed to load translations: {e}");
            lang::TranslationTable::empty()
        }
    };

    let block_registry = block::register_core_blocks();
    log::info!(target: "blocks", "Registered {} core block types", block_registry.len());
    let biomes = worldgen::register_core_biomes();
    log::info!(target: "worldgen", "Registered {} biomes", biomes.len());
    let biome_source = worldgen::BiomeSource::demo();
    log::info!(target: "worldgen", "Biome source: {:?}", biome_source);
    let noise_settings = worldgen::NoiseSettings::demo();
    // Log block component summary.
    let flammable_count = block_registry
        .iter()
        .filter(|(_, b)| b.components.flammable.is_some())
        .count();
    let gravity_count = block_registry
        .iter()
        .filter(|(_, b)| b.components.gravity_affected)
        .count();
    let replaceable_count = block_registry
        .iter()
        .filter(|(_, b)| b.components.replaceable)
        .count();
    let tool_count = block_registry
        .iter()
        .filter(|(_, b)| b.components.required_tool_tier != block::ToolTier::None)
        .count();
    log::info!(target: "blocks", "Components: {} flammable, {} gravity-affected, {} replaceable, {} require tool",
        flammable_count, gravity_count, replaceable_count, tool_count);

    // Collect block IDs and load their models (skip air).
    let block_ids: Vec<_> = block_registry
        .iter()
        .map(|(id, _)| id.clone())
        .filter(|id| id.path() != "air")
        .collect();
    let block_models = model::load_block_models(&_resources, "ferrumcraft", &block_ids);
    log::info!(target: "models", "Loaded {} block models", block_models.len());

    // Load item models for each block (placeable items).
    let item_paths: Vec<String> = block_ids.iter().map(|id| id.path().to_string()).collect();
    let item_models = model::load_item_models(&_resources, "ferrumcraft", &item_paths);
    log::info!(target: "models", "Loaded {} item models", item_models.len());

    // Load blockstate definitions for each non-air block.
    let blockstates = model::load_blockstates(&_resources, "ferrumcraft", &block_ids);
    log::info!(target: "blockstates", "Loaded {} blockstate definitions", blockstates.len());

    // Load tags.
    match tag::load_tag(
        &_resources,
        "ferrumcraft",
        resource::ResourceCategory::BlockTag,
        "solid",
    ) {
        Ok(t) => log::info!(target: "tags", "Loaded 'solid' tag with {} entries", t.len()),
        Err(e) => log::warn!(target: "tags", "Failed to load 'solid' tag: {e}"),
    }

    // Data validation.
    validate_data(&block_registry, &block_models, &blockstates, &_resources);

    // Load font for the debug overlay.
    let font = match Font::load(&_resources, "ferrumcraft") {
        Ok(f) => {
            log::info!(target: "startup", "Loaded bitmap font with {} glyphs", f.glyph_count());
            f
        }
        Err(e) => {
            log::warn!(target: "startup", "Failed to load font: {e}");
            // Fallback: will render nothing if font unavailable
            Font::new_empty()
        }
    };

    // Create the initial generated world around spawn.
    let classic_terrain = Arc::new(classic_worldgen::ClassicTerrain::generate(
        DEMO_WORLD_SEED,
        CLASSIC_WORLD_SIZE as usize,
        CLASSIC_WORLD_SIZE as usize,
    ));
    let mut world = create_demo_world(&classic_terrain, &noise_settings);
    log::info!(target: "world", "Demo world seed: {}", world.seed());
    log::info!(target: "worldgen", "Demo noise settings: {:?}", noise_settings);
    log::info!(target: "world", "Directions: north={}, south={}, west={}, east={}, up={}, down={}",
        block::direction::NORTH,
        block::direction::SOUTH,
        block::direction::WEST,
        block::direction::EAST,
        block::direction::UP,
        block::direction::DOWN);
    log::info!(target: "world", "Facing property order: {:?}", &block::direction::ALL);
    log::info!(target: "world", "Demo world created: {} chunks, {} dirty, block at (0,0,0) = {:?}",
        world.chunk_count(),
        world.drain_dirty().len(),
        world.get_block(world::BlockPos(0, 0, 0)),
    );
    let (saved_player, saved_mobs) = load_saved_game(&mut world)
        .map(|(player, mobs)| (Some(player), mobs))
        .unwrap_or_default();
    let saved_player_position = saved_player
        .as_ref()
        .map(|player| Vec3::from_array(player.position))
        .filter(|position| valid_player_eye_position(*position));
    let player_overlap_recovery_pending = saved_player_position.is_some();
    let saved_hotbar_selected = saved_player
        .as_ref()
        .map(|player| player.hotbar_selected.min(HOTBAR_SIZE - 1))
        .unwrap_or(0);
    let (saved_hotbar_slots, saved_inventory_slots) = saved_player
        .as_ref()
        .map(inventory_from_saved_player)
        .unwrap_or_default();
    let [spawn_x, spawn_y, spawn_z] = classic_terrain.spawn();
    let generated_spawn_position = Vec3::new(
        spawn_x as f32 + 0.5,
        (spawn_y - 1) as f32 + PLAYER_EYE_HEIGHT,
        spawn_z as f32 + 0.5,
    );
    let classic_spawn_position = saved_player
        .as_ref()
        .and_then(|player| player.spawn_position.map(Vec3::from_array))
        .filter(|position| valid_player_eye_position(*position))
        .unwrap_or(generated_spawn_position);
    let spawn_chunk = world::BlockPos(spawn_x, 0, spawn_z).chunk_pos();

    let event_loop = EventLoop::new().expect("Failed to create event loop");
    event_loop.set_control_flow(ControlFlow::Poll);
    let mut app = App {
        window: None,
        renderer: None,
        camera: None,
        world,
        biomes,
        biome_source,
        noise_settings,
        classic_terrain,
        chunk_generation_tx: None,
        generated_chunk_rx: None,
        mesh_generation_tx: None,
        generated_mesh_rx: None,
        pending_chunk_generations: HashSet::new(),
        pending_mesh_rebuilds: VecDeque::new(),
        queued_mesh_rebuilds: HashSet::new(),
        pending_mesh_generations: HashSet::new(),
        font: Some(font),
        block_models: Some(block_models),
        input: InputState::default(),
        debug_overlay: DebugOverlay::default(),
        pointer_locked: false,
        last_update: Instant::now(),
        last_frame_update: Instant::now(),
        fixed_update_accumulator: Duration::ZERO,
        player_velocity: Vec3::ZERO,
        player_grounded: false,
        player_jump_latched: false,
        player_crouching: false,
        hotbar_selected: saved_hotbar_selected,
        hotbar_slots: saved_hotbar_slots,
        inventory_slots: saved_inventory_slots,
        carried_slot: InventorySlot::default(),
        inventory_open: false,
        inventory_toggle_held: false,
        mining_target: None,
        mining_progress: 0.0,
        classic_action_cooldown: 0.0,
        classic_spawn_position,
        classic_view_distance: 0,
        saved_player_position,
        player_overlap_recovery_pending,
        last_save: Instant::now(),
        render_distance_chunks: DEFAULT_RENDER_DISTANCE_CHUNKS,
        mesh_center_chunk: spawn_chunk,
        water_tick: 0,
        pending_water_updates: VecDeque::new(),
        queued_water_updates: HashMap::new(),
        classic_mobs: saved_mobs,
        classic_mob_random: classic_worldgen::JavaRandom::new(DEMO_WORLD_SEED ^ 0xC1A5_51C0),
    };
    event_loop.run_app(&mut app).expect("Event loop error");
}

/// Runs startup validation checks on loaded data.
fn validate_data(
    block_registry: &crate::registry::Registry<block::BlockDefinition>,
    block_models: &crate::registry::Registry<model::BlockModel>,
    blockstates: &crate::registry::Registry<model::BlockState>,
    _resources: &resource::ResourceManager,
) {
    // Validate that each block has a corresponding model.
    for (id, _def) in block_registry.iter() {
        if id.path() == "air" {
            continue;
        }
        if !block_models.contains(id) {
            log::warn!(target: "validation", "Block {id} has no block model");
        }
        let _ = blockstates.get(id).map(|state| {
            if !block_models.iter().any(|(mid, _)| mid.path() == state.model.strip_prefix("block/").unwrap_or("")) {
                log::warn!(target: "validation", "Blockstate for {id} references unknown model '{}'", state.model);
            }
        });
    }

    // Collect unique texture paths from all block models.
    let mut texture_paths: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (_id, model) in block_models.iter() {
        for face in &model::ALL_FACES {
            let tex = model.texture(*face);
            if !tex.is_empty() {
                texture_paths.insert(tex.to_string());
            }
        }
    }

    log::info!(target: "validation", "Collected {} unique texture paths from models", texture_paths.len());
    for path in &texture_paths {
        let full_path = format!("assets/ferrumcraft/textures/{}.png", path);
        if !std::path::Path::new(&full_path).exists() {
            log::warn!(target: "validation", "Missing texture: {full_path}");
        }
    }

    // Log built-in resource pack.
    log::info!(target: "startup", "Built-in resource pack 'ferrumcraft' loaded from ./assets and ./data");
}
