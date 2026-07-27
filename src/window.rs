//! Window construction helpers.
//!
//! This module wraps winit window setup so the application entry point does not
//! need to know title, initial size, or ownership details.

use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::error::OsError;
use winit::event_loop::ActiveEventLoop;
use winit::monitor::MonitorHandle;
use winit::window::CursorGrabMode;
use winit::window::Window as WinitWindow;
use winit::window::WindowId;

use std::sync::Arc;

/// Game window handle shared with systems that need direct winit access.
pub struct Window {
    /// Shared winit window used by the renderer to create a surface and by the
    /// event loop to request redraws.
    #[allow(dead_code)]
    pub inner: Arc<WinitWindow>,
}

impl Window {
    /// Creates the main game window with a sensible initial monitor-relative size.
    pub fn new(event_loop: &ActiveEventLoop) -> Result<Self, OsError> {
        let size = preferred_size(event_loop.primary_monitor().as_ref());
        let attributes = WinitWindow::default_attributes()
            .with_title("Minecraft 0.0.14a_08")
            .with_inner_size(size);

        let inner = Arc::new(event_loop.create_window(attributes)?);
        Ok(Self { inner })
    }

    /// Stable identifier used to ignore events for other platform windows.
    pub fn id(&self) -> WindowId {
        self.inner.id()
    }

    /// Queues a redraw request for the next event-loop cycle.
    pub fn request_redraw(&self) {
        self.inner.request_redraw();
    }

    /// Enables or disables Minecraft-style cursor capture.
    pub fn set_pointer_locked(&self, locked: bool) {
        if locked {
            if self.inner.set_cursor_grab(CursorGrabMode::Locked).is_err() {
                let _ = self.inner.set_cursor_grab(CursorGrabMode::Confined);
            }
        } else {
            let _ = self.inner.set_cursor_grab(CursorGrabMode::None);
        }

        self.inner.set_cursor_visible(!locked);
    }

    /// Moves the OS cursor in window coordinates when supported by the platform.
    pub fn set_cursor_position(&self, x: f64, y: f64) {
        let _ = self.inner.set_cursor_position(PhysicalPosition::new(x, y));
    }
}

/// Chooses an initial window size based on the primary monitor.
///
/// The fallback keeps window creation deterministic on platforms that do not
/// expose monitor dimensions or report invalid values.
fn preferred_size(monitor: Option<&MonitorHandle>) -> PhysicalSize<u32> {
    const DEFAULT: PhysicalSize<u32> = PhysicalSize::new(1280, 720);
    const FRACTION: f32 = 0.8;

    let Some(monitor) = monitor else {
        return DEFAULT;
    };
    let PhysicalSize { width, height } = monitor.size();
    if width == 0 || height == 0 {
        return DEFAULT;
    }

    let scale = FRACTION
        .min(1280.0 / width as f32)
        .min(720.0 / height as f32);
    let w = (width as f32 * scale) as u32;
    let h = (height as f32 * scale) as u32;
    PhysicalSize::new(w, h)
}
