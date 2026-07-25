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

use std::collections::{HashSet, VecDeque};
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
const WALK_SPEED: f32 = 4.3;
const SPRINT_SPEED: f32 = 5.6;
const CROUCH_SPEED: f32 = 1.3;
const GROUND_ACCELERATION: f32 = 24.0;
const AIR_ACCELERATION: f32 = 6.0;
const GRAVITY: f32 = 28.0;
const JUMP_SPEED: f32 = 8.4;
const PLAYER_HEIGHT: f32 = 1.8;
const PLAYER_EYE_HEIGHT: f32 = 1.62;
const PLAYER_CROUCH_EYE_HEIGHT: f32 = 1.35;
const PLAYER_RADIUS: f32 = 0.3;
const BLOCK_REACH: f32 = 5.0;
const BLOCK_RAY_STEP: f32 = 0.05;
const MIN_RENDER_DISTANCE_CHUNKS: i32 = 0;
const MAX_RENDER_DISTANCE_CHUNKS: i32 = 16;
const DEFAULT_RENDER_DISTANCE_CHUNKS: i32 = 4;
const DEMO_WORLD_SEED: u64 = 12_345;
const DEMO_SPAWN_CHUNK_RADIUS: i32 = 1;
const CHUNKS_GENERATED_PER_TICK: usize = 1;
const GENERATED_CHUNKS_INTEGRATED_PER_TICK: usize = 1;
const GENERATED_MESHES_INTEGRATED_PER_TICK: usize = 1;
const CHUNK_MESH_REBUILDS_PER_TICK: usize = 1;
const UNLOAD_MARGIN_CHUNKS: i32 = 2;
const WORLD_RENDER_OFFSET: f32 = 8.5;

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
    worldgen_feature_types: crate::registry::Registry<worldgen::WorldgenFeatureType>,
    structure_sets: crate::registry::Registry<worldgen::StructureSet>,
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
    render_distance_chunks: i32,
    mesh_center_chunk: world::ChunkPos,
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

#[derive(Clone, Copy, Debug)]
struct BlockTarget {
    block_pos: world::BlockPos,
    place_pos: world::BlockPos,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let w = window::Window::new(event_loop).expect("Failed to create window");
        let size = w.inner.inner_size();
        let mut camera = FirstPersonCamera::new(size.width, size.height);
        camera.set_position(spawn_eye_position(&self.world));
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
            if seen.insert("block/grass_block_side_overlay".to_string()) {
                paths.push("block/grass_block_side_overlay".to_string());
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
            self.worldgen_feature_types.clone(),
            self.structure_sets.clone(),
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
        if self.input.take_debug_overlay_toggle_requested() {
            self.debug_overlay.toggle();
        }
        if self.input.is_f3_pressed() && self.input.was_key_just_pressed(KeyCode::KeyF) {
            let delta = if self.input.is_shift_pressed() { -1 } else { 1 };
            self.adjust_render_distance(delta);
        }

        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
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
                button: MouseButton::Left,
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
                        renderer.set_view_projection(camera.view_projection());
                    }
                }
                if let Some(window) = &self.window {
                    window.request_redraw();
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(renderer) = &mut self.renderer {
                    let frame_start = Instant::now();
                    let debug_text = self.debug_overlay.text();
                    let screen_tint = self
                        .camera
                        .as_ref()
                        .and_then(|camera| camera_water_tint(&self.world, camera.position()));
                    match renderer.render(debug_text.as_deref(), screen_tint) {
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
                renderer.set_view_projection(camera.view_projection());
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

        self.update_player_movement(frame_dt);
        self.handle_block_interaction();
        self.run_fixed_updates();

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

    fn fixed_update(&mut self, _dt: Duration) {
        // Match Minecraft's 20 ticks-per-second simulation rate while rendering
        // stays independent and can run at a higher frame rate.
        let mut camera_position = None;
        if let (Some(camera), Some(renderer)) = (&mut self.camera, &mut self.renderer) {
            renderer.set_view_projection(camera.view_projection());
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
            let integrated = self.integrate_generated_chunks();
            let integrated_meshes = self.integrate_generated_meshes(center_chunk);
            let rebuilt = self.process_pending_mesh_rebuilds(center_chunk);
            let generated = self.generate_missing_chunks_around(center_chunk);
            let unloaded = self.unload_far_chunks(center_chunk);
            self.remove_chunk_meshes_outside_render_distance(center_chunk);
            if integrated > 0
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
                            self.queue_chunk_meshes_near(chunk_pos);
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
            if self.world.is_chunk_loaded(generated.pos) {
                continue;
            }

            let pos = generated.pos;
            self.world.insert_chunk(generated.chunk);
            self.set_chunk_mesh_from_data(pos, generated.mesh);
            self.queue_chunk_mesh_rebuild(world::ChunkPos(pos.0 + 1, pos.1));
            self.queue_chunk_mesh_rebuild(world::ChunkPos(pos.0 - 1, pos.1));
            self.queue_chunk_mesh_rebuild(world::ChunkPos(pos.0, pos.1 + 1));
            self.queue_chunk_mesh_rebuild(world::ChunkPos(pos.0, pos.1 - 1));
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
                self.pending_mesh_generations.remove(&chunk_pos);
                unloaded += 1;
            }
        }
        unloaded
    }

    fn update_player_movement(&mut self, dt: Duration) {
        let Some(camera) = &self.camera else {
            return;
        };

        let dt = dt.as_secs_f32();
        let mut position = camera.position();
        let desired_direction = player_move_direction(camera, &self.input);
        let speed = if self.input.is_shift_pressed() {
            CROUCH_SPEED
        } else if self.input.is_key_pressed(KeyCode::ControlLeft)
            || self.input.is_key_pressed(KeyCode::ControlRight)
        {
            SPRINT_SPEED
        } else {
            WALK_SPEED
        };
        let target_horizontal = desired_direction * speed;
        let acceleration = if self.player_grounded {
            GROUND_ACCELERATION
        } else {
            AIR_ACCELERATION
        };
        let smoothing = 1.0 - (-acceleration * dt).exp();
        self.player_velocity.x += (target_horizontal.x - self.player_velocity.x) * smoothing;
        self.player_velocity.z += (target_horizontal.z - self.player_velocity.z) * smoothing;

        if self.player_grounded && self.input.is_key_pressed(KeyCode::Space) {
            self.player_velocity.y = JUMP_SPEED;
            self.player_grounded = false;
        }
        self.player_velocity.y -= GRAVITY * dt;

        let (next_position, next_velocity, grounded) = move_player_with_collisions(
            &self.world,
            position,
            self.player_velocity,
            dt,
            self.input.is_shift_pressed(),
        );
        position = next_position;
        self.player_velocity = next_velocity;
        self.player_grounded = grounded;

        if let Some(camera) = &mut self.camera {
            camera.set_position(position);
        }
        if let Some(renderer) = &mut self.renderer {
            if let Some(camera) = &self.camera {
                renderer.set_view_projection(camera.view_projection());
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

    fn handle_block_interaction(&mut self) {
        if !self.pointer_locked {
            return;
        }

        let Some(target) = self.targeted_block() else {
            return;
        };

        if self.input.was_mouse_button_just_pressed(MouseButton::Left) {
            self.world
                .set_block(target.block_pos, block::BlockId::AIR.clone());
            self.queue_block_update_meshes(target.block_pos);
        }

        if self.input.was_mouse_button_just_pressed(MouseButton::Right) {
            let previous = self.world.get_block(target.place_pos);
            if matches!(previous.0.as_str(), "" | "water") {
                self.world
                    .set_block(target.place_pos, block::BlockId("dirt".to_string()));
                let collides = self.camera.as_ref().is_some_and(|camera| {
                    player_collides(
                        &self.world,
                        camera.position(),
                        self.input.is_shift_pressed(),
                    )
                });
                if collides {
                    self.world.set_block(target.place_pos, previous);
                } else {
                    self.queue_block_update_meshes(target.place_pos);
                }
            }
        }
    }

    fn targeted_block(&self) -> Option<BlockTarget> {
        let camera = self.camera.as_ref()?;
        raycast_block(&self.world, camera.position(), camera.forward())
    }

    fn queue_block_update_meshes(&mut self, block_pos: world::BlockPos) {
        let chunk_pos = block_pos.chunk_pos();
        self.queue_chunk_meshes_near(chunk_pos);
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
            return;
        }
        if self.queued_mesh_rebuilds.insert(pos) {
            self.pending_mesh_rebuilds.push_back(pos);
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

fn camera_block_pos(position: Vec3) -> world::BlockPos {
    world::BlockPos(
        (position.x + WORLD_RENDER_OFFSET).floor() as i32,
        position.y.floor() as i32,
        (position.z + WORLD_RENDER_OFFSET).floor() as i32,
    )
}

fn start_chunk_generation_worker(
    seed: u64,
    feature_types: crate::registry::Registry<worldgen::WorldgenFeatureType>,
    structure_sets: crate::registry::Registry<worldgen::StructureSet>,
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
    for worker_index in 0..worker_count() {
        let request_rx = Arc::clone(&request_rx);
        let result_tx = result_tx.clone();
        let feature_types = feature_types.clone();
        let structure_sets = structure_sets.clone();
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
                        &feature_types,
                        &structure_sets,
                        &noise_settings,
                        &biome_source,
                        chunk_pos,
                    );

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
    for worker_index in 0..worker_count() {
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
                        snapshot.insert_chunk(chunk);
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
    feature_types: &crate::registry::Registry<worldgen::WorldgenFeatureType>,
    structure_sets: &crate::registry::Registry<worldgen::StructureSet>,
    noise_settings: &worldgen::NoiseSettings,
    biome_source: &worldgen::BiomeSource,
    chunk_pos: world::ChunkPos,
) {
    let id = |s: &str| block::BlockId(s.to_string());

    worldgen::populate_chunk_noise(world, chunk_pos, noise_settings, id("stone"));
    worldgen::apply_surface_rules(world, chunk_pos, noise_settings, biome_source);
    worldgen::apply_carvers(world, chunk_pos, noise_settings);
    worldgen::apply_sea_level_water(world, chunk_pos, noise_settings, id("water"));

    let coal_ore = worldgen::PlacedFeature {
        configured: worldgen::ConfiguredFeature {
            feature_type: id::NamespacedId::ferrumcraft("ore").expect("valid feature type ID"),
            config: worldgen::FeatureConfig::Ore {
                ore: id("coal_ore"),
                replaceable: vec![id("stone")],
                size: 6,
            },
        },
        placement: worldgen::PlacementConfig {
            attempts_per_chunk: 2,
            chance: 3,
            salt: 5_005,
            height: worldgen::PlacementHeight::Range { min: 1, max: 6 },
            biome_filter: Vec::new(),
        },
    };
    worldgen::place_placed_feature_in_chunk(
        feature_types,
        &coal_ore,
        world,
        noise_settings,
        biome_source,
        chunk_pos,
    );

    let forest_tree = worldgen::PlacedFeature {
        configured: worldgen::ConfiguredFeature {
            feature_type: id::NamespacedId::ferrumcraft("tree").expect("valid feature type ID"),
            config: worldgen::FeatureConfig::SimpleTree {
                log: id("oak_log"),
                leaves: id("oak_leaves"),
                trunk_height: 3,
            },
        },
        placement: worldgen::PlacementConfig {
            attempts_per_chunk: 1,
            chance: 6,
            salt: 4_004,
            height: worldgen::PlacementHeight::Surface,
            biome_filter: vec![id::NamespacedId::ferrumcraft("forest").expect("valid biome ID")],
        },
    };
    worldgen::place_placed_feature_in_chunk(
        feature_types,
        &forest_tree,
        world,
        noise_settings,
        biome_source,
        chunk_pos,
    );

    worldgen::place_structures_in_chunk(structure_sets, world, noise_settings, chunk_pos);
}

fn create_demo_world(
    feature_types: &crate::registry::Registry<worldgen::WorldgenFeatureType>,
    structure_sets: &crate::registry::Registry<worldgen::StructureSet>,
    noise_settings: &worldgen::NoiseSettings,
    biome_source: &worldgen::BiomeSource,
) -> world::World {
    let mut world = world::World::with_seed(DEMO_WORLD_SEED);

    let id = |s: &str| block::BlockId(s.to_string());
    let spawn_chunks = worldgen::spawn_area_chunks(world::ChunkPos(0, 0), DEMO_SPAWN_CHUNK_RADIUS);

    for &chunk_pos in &spawn_chunks {
        generate_worldgen_chunk(
            &mut world,
            feature_types,
            structure_sets,
            noise_settings,
            biome_source,
            chunk_pos,
        );
    }
    let stone_column = worldgen::ConfiguredFeature {
        feature_type: id::NamespacedId::ferrumcraft("block_column").expect("valid feature type ID"),
        config: worldgen::FeatureConfig::BlockColumn {
            block: id("stone"),
            min_height: 5,
            max_height: 8,
            height_salt: 1,
        },
    };
    worldgen::place_configured_feature(
        feature_types,
        &stone_column,
        &mut world,
        world::BlockPos(38, 2, 8),
    );
    worldgen::place_configured_feature(
        feature_types,
        &stone_column,
        &mut world,
        world::BlockPos(2, 2, 2),
    );
    place_ao_test_structure(&mut world, world::BlockPos(12, 2, 2));
    for x in 8..12 {
        let z = world.seeded_range(x, 6, 2, 3, 7);
        let height = world.seeded_range(x, z, 3, 2, 4);
        for y in 2..=height {
            world.set_block(world::BlockPos(x, y, z), id("stone"));
        }
    }
    for x in 10..14 {
        for z in 10..14 {
            world.set_block(world::BlockPos(x, 1, z), id("sand"));
        }
    }
    for x in 10..14 {
        for z in 4..8 {
            world.set_block(world::BlockPos(x, 1, z), id("glass"));
        }
    }
    for x in 4..8 {
        for z in 10..14 {
            world.set_block(world::BlockPos(x, 1, z), id("oak_planks"));
        }
    }
    for x in 1..5 {
        for z in 10..14 {
            world.set_block(world::BlockPos(x, 1, z), id("water"));
        }
    }
    let oak_tree = worldgen::ConfiguredFeature {
        feature_type: id::NamespacedId::ferrumcraft("tree").expect("valid feature type ID"),
        config: worldgen::FeatureConfig::SimpleTree {
            log: id("oak_log"),
            leaves: id("oak_leaves"),
            trunk_height: 2,
        },
    };
    worldgen::place_configured_feature(
        feature_types,
        &oak_tree,
        &mut world,
        world::BlockPos(5, 2, 5),
    );

    world
}

fn place_ao_test_structure(world: &mut world::World, origin: world::BlockPos) {
    let stone = block::BlockId("stone".to_string());
    for x in 0..3 {
        world.set_block(
            world::BlockPos(origin.0 + x, origin.1, origin.2),
            stone.clone(),
        );
        world.set_block(
            world::BlockPos(origin.0 + x, origin.1 + 1, origin.2),
            stone.clone(),
        );
    }
    for z in 0..3 {
        world.set_block(
            world::BlockPos(origin.0, origin.1, origin.2 + z),
            stone.clone(),
        );
        world.set_block(
            world::BlockPos(origin.0, origin.1 + 1, origin.2 + z),
            stone.clone(),
        );
    }
    for y in 0..3 {
        world.set_block(
            world::BlockPos(origin.0 + 2, origin.1 + y, origin.2 + 2),
            stone.clone(),
        );
        world.set_block(
            world::BlockPos(origin.0 + 1, origin.1 + y, origin.2 + 2),
            stone.clone(),
        );
        world.set_block(
            world::BlockPos(origin.0 + 2, origin.1 + y, origin.2 + 1),
            stone.clone(),
        );
    }
    world.set_block(
        world::BlockPos(origin.0 + 1, origin.1 + 2, origin.2),
        stone.clone(),
    );
    world.set_block(world::BlockPos(origin.0, origin.1 + 2, origin.2 + 1), stone);
}

fn camera_water_tint(world: &world::World, position: Vec3) -> Option<[f32; 4]> {
    let block_pos = world::BlockPos(
        camera_block_pos(position).0,
        camera_block_pos(position).1,
        camera_block_pos(position).2,
    );
    if world.get_block(block_pos).0 != "water" {
        return None;
    }

    let local_y = position.y - position.y.floor();
    let has_water_above = world
        .get_block(world::BlockPos(block_pos.0, block_pos.1 + 1, block_pos.2))
        .0
        == "water";
    if has_water_above || local_y < 14.0 / 16.0 {
        Some([0.0196, 0.0196, 0.2, 0.65])
    } else {
        None
    }
}

fn spawn_eye_position(world: &world::World) -> Vec3 {
    let block_x = 8;
    let block_z = 8;
    let ground_y = (0..world::CHUNK_SIZE_Y as i32)
        .rev()
        .find(|y| is_player_solid_block(&world.get_block(world::BlockPos(block_x, *y, block_z))))
        .unwrap_or(16);

    Vec3::new(
        block_x as f32 + 0.5 - WORLD_RENDER_OFFSET,
        ground_y as f32 + 1.0 + PLAYER_EYE_HEIGHT,
        block_z as f32 + 0.5 - WORLD_RENDER_OFFSET,
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
        if is_targetable_block(&block) {
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

    if input.is_key_pressed(KeyCode::KeyW) {
        movement += camera.yaw_forward();
    }
    if input.is_key_pressed(KeyCode::KeyS) {
        movement -= camera.yaw_forward();
    }
    if input.is_key_pressed(KeyCode::KeyA) {
        movement -= camera.yaw_right();
    }
    if input.is_key_pressed(KeyCode::KeyD) {
        movement += camera.yaw_right();
    }

    movement.try_normalize().unwrap_or(Vec3::ZERO)
}

fn move_player_with_collisions(
    world: &world::World,
    mut position: Vec3,
    mut velocity: Vec3,
    dt: f32,
    crouching: bool,
) -> (Vec3, Vec3, bool) {
    let mut grounded = false;

    position.x += velocity.x * dt;
    if player_collides(world, position, crouching) {
        position.x -= velocity.x * dt;
        velocity.x = 0.0;
    }

    position.z += velocity.z * dt;
    if player_collides(world, position, crouching) {
        position.z -= velocity.z * dt;
        velocity.z = 0.0;
    }

    position.y += velocity.y * dt;
    if player_collides(world, position, crouching) {
        position.y -= velocity.y * dt;
        if velocity.y < 0.0 {
            grounded = true;
        }
        velocity.y = 0.0;
    }

    if !grounded {
        let probe = position - Vec3::Y * 0.03;
        grounded = player_collides(world, probe, crouching);
    }

    (position, velocity, grounded)
}

fn player_collides(world: &world::World, eye_position: Vec3, crouching: bool) -> bool {
    let eye_height = if crouching {
        PLAYER_CROUCH_EYE_HEIGHT
    } else {
        PLAYER_EYE_HEIGHT
    };
    let height = if crouching { 1.5 } else { PLAYER_HEIGHT };
    let feet_y = eye_position.y - eye_height;
    let min_x = eye_position.x + WORLD_RENDER_OFFSET - PLAYER_RADIUS;
    let max_x = eye_position.x + WORLD_RENDER_OFFSET + PLAYER_RADIUS;
    let min_y = feet_y;
    let max_y = feet_y + height;
    let min_z = eye_position.z + WORLD_RENDER_OFFSET - PLAYER_RADIUS;
    let max_z = eye_position.z + WORLD_RENDER_OFFSET + PLAYER_RADIUS;

    for y in min_y.floor() as i32..=max_y.floor() as i32 {
        if !(0..world::CHUNK_SIZE_Y as i32).contains(&y) {
            return y < 0;
        }
        for z in min_z.floor() as i32..=max_z.floor() as i32 {
            for x in min_x.floor() as i32..=max_x.floor() as i32 {
                if is_player_solid_block(&world.get_block(world::BlockPos(x, y, z))) {
                    return true;
                }
            }
        }
    }

    false
}

fn is_player_solid_block(block: &block::BlockId) -> bool {
    !matches!(block.0.as_str(), "" | "water" | "oak_leaves")
}

fn is_targetable_block(block: &block::BlockId) -> bool {
    !matches!(block.0.as_str(), "" | "water")
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
    let worldgen_feature_types = worldgen::register_core_feature_types();
    log::info!(target: "worldgen", "Registered {} worldgen feature types", worldgen_feature_types.len());
    for (id, feature_type) in worldgen_feature_types.iter() {
        log::debug!(target: "worldgen", "Feature type {id}: {}", feature_type.name());
    }
    let structure_sets = worldgen::register_core_structure_sets();
    log::info!(target: "worldgen", "Registered {} structure sets", structure_sets.len());
    for (id, structure_set) in structure_sets.iter() {
        log::debug!(target: "worldgen", "Structure set {id}: {}", structure_set.name());
    }

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

    // Create a demo world and place some blocks.
    let mut world = create_demo_world(
        &worldgen_feature_types,
        &structure_sets,
        &noise_settings,
        &biome_source,
    );
    log::info!(target: "world", "Demo world seed: {}", world.seed());
    log::info!(target: "worldgen", "Demo noise settings: {:?}", noise_settings);
    if let Some(def) = block_registry
        .iter()
        .find(|(id, _)| id.path() == "oak_log")
        .map(|(_, d)| d)
    {
        world.set_block_property(world::BlockPos(5, 2, 5), 0, 1);
        log::info!(target: "world", "Log axis = {}, properties: {:?}",
            def.properties[0].values[world.get_block_property(world::BlockPos(5, 2, 5), 0) as usize],
            def.properties);
    }
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
        worldgen_feature_types,
        structure_sets,
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
        render_distance_chunks: DEFAULT_RENDER_DISTANCE_CHUNKS,
        mesh_center_chunk: world::ChunkPos(0, 0),
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
