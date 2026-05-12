//! Window construction helpers.
//!
//! This module wraps winit window setup so the application entry point does not
//! need to know title, initial size, or ownership details.

use winit::dpi::PhysicalSize;
use winit::error::OsError;
use winit::event_loop::ActiveEventLoop;
use winit::monitor::MonitorHandle;
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
            .with_title("FerrumCraft")
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

    let w = (width as f32 * FRACTION) as u32;
    let h = (height as f32 * FRACTION) as u32;
    PhysicalSize::new(w, h)
}
