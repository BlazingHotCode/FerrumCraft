mod renderer;
mod window;

use winit::application::ApplicationHandler;
use winit::event::WindowEvent;
use winit::event_loop::ActiveEventLoop;
use winit::event_loop::EventLoop;

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
