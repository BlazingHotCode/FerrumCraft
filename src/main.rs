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
mod model;
mod registry;
mod renderer;
mod resource;
mod tag;
mod window;
mod world;

use std::time::{Duration, Instant};

use camera::FirstPersonCamera;
use debug::DebugOverlay;
use glam::Vec3;
use input::InputState;
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
const FREE_FLY_SPEED: f32 = 6.0;
const FREE_FLY_ACCELERATION: f32 = 18.0;

/// Top-level application state owned by the winit event loop.
///
/// Window and renderer creation is deferred until `resumed` because winit
/// requires platform window handles to be created from an active event loop.
struct App {
    window: Option<window::Window>,
    renderer: Option<renderer::Renderer>,
    camera: Option<FirstPersonCamera>,
    input: InputState,
    debug_overlay: DebugOverlay,
    pointer_locked: bool,
    last_update: Instant,
    last_frame_update: Instant,
    fixed_update_accumulator: Duration,
    free_fly_velocity: Vec3,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let w = window::Window::new(event_loop).expect("Failed to create window");
        let size = w.inner.inner_size();
        let camera = FirstPersonCamera::new(size.width, size.height);
        let renderer = pollster::block_on(renderer::Renderer::new(
            w.inner.clone(),
            camera.view_projection(),
        ));
        self.window = Some(w);
        self.renderer = Some(renderer);
        self.camera = Some(camera);
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
                    match renderer.render(debug_text.as_deref()) {
                        Ok(()) => {
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

        self.update_free_fly_movement(frame_dt);
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
        if let (Some(camera), Some(renderer)) = (&mut self.camera, &mut self.renderer) {
            renderer.set_view_projection(camera.view_projection());
            self.debug_overlay.set_player_position(camera.position());
            self.debug_overlay.set_facing(camera.facing_name());
        }
    }

    fn update_free_fly_movement(&mut self, dt: Duration) {
        let Some(camera) = &mut self.camera else {
            return;
        };

        let direction = free_fly_direction(camera, &self.input);
        let target_velocity = direction * FREE_FLY_SPEED;
        let smoothing = 1.0 - (-FREE_FLY_ACCELERATION * dt.as_secs_f32()).exp();
        self.free_fly_velocity = self.free_fly_velocity.lerp(target_velocity, smoothing);

        if self.free_fly_velocity.length_squared() > 0.0001 {
            camera.translate_world(self.free_fly_velocity * dt.as_secs_f32());
        }

        if let Some(renderer) = &mut self.renderer {
            renderer.set_view_projection(camera.view_projection());
        }
        self.debug_overlay.set_player_position(camera.position());
    }

    fn set_pointer_locked(&mut self, locked: bool) {
        self.pointer_locked = locked;
        if let Some(window) = &self.window {
            window.set_pointer_locked(locked);
        }
    }
}

fn free_fly_direction(camera: &FirstPersonCamera, input: &InputState) -> Vec3 {
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
    if input.is_key_pressed(KeyCode::Space) {
        movement.y += 1.0;
    }
    if input.is_key_pressed(KeyCode::ShiftLeft) || input.is_key_pressed(KeyCode::ShiftRight) {
        movement.y -= 1.0;
    }

    movement.try_normalize().unwrap_or(Vec3::ZERO)
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

    // Create a demo world and place some blocks.
    let mut world = world::World::new();
    world.load_chunk(world::ChunkPos(0, 0));
    world.set_block(world::BlockPos(0, 0, 0), block::BlockId(1));
    world.set_block(world::BlockPos(1, 0, 0), block::BlockId(2));
    world.set_block(world::BlockPos(2, 0, 0), block::BlockId(3));
    // Set axis property on log block at (3,0,0) to x (index 1).
    let log_id = block::BlockId(7);
    world.set_block(world::BlockPos(3, 0, 0), log_id);
    if let Some(def) = block_registry
        .iter()
        .find(|(id, _)| id.path() == "log")
        .map(|(_, d)| d)
    {
        world.set_block_property(world::BlockPos(3, 0, 0), 0, 1);
        log::info!(target: "world", "Log axis = {}, properties: {:?}",
            def.properties[0].values[world.get_block_property(world::BlockPos(3, 0, 0), 0) as usize],
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
        input: InputState::default(),
        debug_overlay: DebugOverlay::default(),
        pointer_locked: false,
        last_update: Instant::now(),
        last_frame_update: Instant::now(),
        fixed_update_accumulator: Duration::ZERO,
        free_fly_velocity: Vec3::ZERO,
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
