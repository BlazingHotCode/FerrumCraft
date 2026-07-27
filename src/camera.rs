//! First-person camera state.
//!
//! The camera owns player-view position, yaw/pitch orientation, and projection
//! settings. Movement and mouse-look systems will mutate this state in later
//! roadmap items.

use glam::{Mat4, Vec3};

const MOUSE_SENSITIVITY: f32 = 0.15_f32.to_radians();
const MAX_PITCH: f32 = 90.0_f32.to_radians();

/// Camera used for Minecraft-style first-person rendering.
#[derive(Debug)]
pub struct FirstPersonCamera {
    position: Vec3,
    yaw: f32,
    pitch: f32,
    aspect: f32,
    vertical_fov: f32,
    near: f32,
    far: f32,
    invert_mouse: bool,
}

impl FirstPersonCamera {
    /// Creates a camera looking toward the debug shapes from player eye height.
    pub fn new(width: u32, height: u32) -> Self {
        log::debug!(target: "camera", "Spawning at (0.0, 1.62, 3.5) looking {:.1}° yaw, {:.1}° pitch", -90.0, -12.0);
        Self {
            position: Vec3::new(0.0, 1.62, 3.5),
            yaw: -90.0_f32.to_radians(),
            pitch: -12.0_f32.to_radians(),
            aspect: aspect(width, height),
            vertical_fov: 70.0_f32.to_radians(),
            near: 0.05,
            far: 1024.0,
            invert_mouse: false,
        }
    }

    /// Updates the projection aspect ratio after a window resize.
    pub fn resize(&mut self, width: u32, height: u32) {
        self.aspect = aspect(width, height);
    }

    /// Applies raw mouse movement to yaw/pitch orientation.
    pub fn apply_mouse_delta(&mut self, delta: (f64, f64)) {
        self.yaw += delta.0 as f32 * MOUSE_SENSITIVITY;
        let vertical = if self.invert_mouse { -delta.1 } else { delta.1 };
        self.pitch =
            (self.pitch - vertical as f32 * MOUSE_SENSITIVITY).clamp(-MAX_PITCH, MAX_PITCH);
    }

    pub fn toggle_invert_mouse(&mut self) {
        self.invert_mouse = !self.invert_mouse;
    }

    pub fn set_far_plane(&mut self, far: f32) {
        self.far = far;
    }

    pub fn far_plane(&self) -> f32 {
        self.far
    }

    /// Moves the camera by a world-space offset.
    pub fn translate_world(&mut self, offset: Vec3) {
        self.position += offset;
    }

    /// Places the camera at a world-space eye position.
    pub fn set_position(&mut self, position: Vec3) {
        self.position = position;
    }

    /// Horizontal forward direction from yaw only, ignoring pitch.
    pub fn yaw_forward(&self) -> Vec3 {
        let (yaw_sin, yaw_cos) = self.yaw.sin_cos();
        Vec3::new(yaw_cos, 0.0, yaw_sin).normalize()
    }

    /// Horizontal right direction from yaw only, ignoring pitch.
    pub fn yaw_right(&self) -> Vec3 {
        let forward = self.yaw_forward();
        Vec3::new(-forward.z, 0.0, forward.x)
    }

    /// Combined view/projection matrix consumed by the renderer.
    pub fn view_projection(&self) -> Mat4 {
        self.projection_matrix() * self.view_matrix()
    }

    /// Camera world position.
    pub fn position(&self) -> Vec3 {
        self.position
    }

    /// Human-readable facing direction for the F3 debug overlay.
    ///
    /// Returns a string like `"North (-Z)"` based on the horizontal yaw component.
    pub fn facing_name(&self) -> String {
        // Normalise yaw to 0–360 range.
        let yaw = self.yaw.to_degrees().rem_euclid(360.0);
        // 0 = South (+Z), 90 = West (-X), 180/–180 = North (–Z), –90 = East (+X)
        let dir = if yaw < 45.0 || yaw >= 315.0 {
            "SOUTH (+Z)"
        } else if yaw < 135.0 {
            "WEST (-X)"
        } else if yaw < 225.0 {
            "NORTH (-Z)"
        } else {
            "EAST (+X)"
        };
        dir.to_string()
    }

    fn view_matrix(&self) -> Mat4 {
        Mat4::look_to_rh(self.position, self.forward(), Vec3::Y)
    }

    fn projection_matrix(&self) -> Mat4 {
        Mat4::perspective_rh(self.vertical_fov, self.aspect, self.near, self.far)
    }

    pub fn forward(&self) -> Vec3 {
        let (yaw_sin, yaw_cos) = self.yaw.sin_cos();
        let (pitch_sin, pitch_cos) = self.pitch.sin_cos();
        Vec3::new(yaw_cos * pitch_cos, pitch_sin, yaw_sin * pitch_cos).normalize()
    }
}

fn aspect(width: u32, height: u32) -> f32 {
    width.max(1) as f32 / height.max(1) as f32
}
