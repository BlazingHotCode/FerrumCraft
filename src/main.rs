//! FerrumCraft application entry point.
//!
//! This module wires together the winit application lifecycle, window creation,
//! and renderer frame loop. Game state should move into dedicated modules as it
//! grows; this file should stay focused on top-level orchestration.

mod renderer;
mod window;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::event_loop::EventLoop;

/// Top-level application state owned by the winit event loop.
///
/// Window and renderer creation is deferred until `resumed` because winit
/// requires platform window handles to be created from an active event loop.
struct App {
    window: Option<window::Window>,
    renderer: Option<renderer::Renderer>,
}

impl ApplicationHandler for App {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_some() {
            return;
        }

        let w = window::Window::new(event_loop).expect("Failed to create window");
        let renderer = pollster::block_on(renderer::Renderer::new(w.inner.clone()));
        self.window = Some(w);
        self.renderer = Some(renderer);
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: winit::window::WindowId,
        event: WindowEvent,
    ) {
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                if let Some(renderer) = &mut self.renderer {
                    renderer.resize(size.width, size.height);
                }
            }
            WindowEvent::RedrawRequested => {
                if let Some(renderer) = &self.renderer {
                    renderer.render();
                }
            }
            _ => {}
        }
    }

    fn about_to_wait(&mut self, _event_loop: &ActiveEventLoop) {
        // Drive a simple continuous redraw loop until a fixed tick/update model
        // is introduced.
        if let Some(window) = &self.window {
            window.inner.request_redraw();
        }
    }
}

fn main() {
    let event_loop = EventLoop::new().expect("Failed to create event loop");
    let mut app = App {
        window: None,
        renderer: None,
    };
    event_loop.run_app(&mut app).expect("Event loop error");
}
