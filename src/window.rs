use winit::dpi::PhysicalSize;
use winit::error::OsError;
use winit::event_loop::ActiveEventLoop;
use winit::monitor::MonitorHandle;
use winit::window::Window as WinitWindow;

pub struct Window {
    #[allow(dead_code)]
    pub inner: WinitWindow,
}

impl Window {
    pub fn new(event_loop: &ActiveEventLoop) -> Result<Self, OsError> {
        let size = preferred_size(event_loop.primary_monitor().as_ref());
        let attributes = WinitWindow::default_attributes()
            .with_title("FerrumCraft")
            .with_inner_size(size);

        let inner = event_loop.create_window(attributes)?;
        Ok(Self { inner })
    }
}

fn preferred_size(monitor: Option<&MonitorHandle>) -> PhysicalSize<u32> {
    const DEFAULT: PhysicalSize<u32> = PhysicalSize::new(1280, 720);
    const FRACTION: f32 = 0.8;

    let Some(monitor) = monitor else { return DEFAULT };
    let PhysicalSize { width, height } = monitor.size();
    if width == 0 || height == 0 {
        return DEFAULT;
    }

    let w = (width as f32 * FRACTION) as u32;
    let h = (height as f32 * FRACTION) as u32;
    PhysicalSize::new(w, h)
}
