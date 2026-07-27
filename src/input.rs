//! Input state collected from winit events.
//!
//! This module keeps raw keyboard, mouse button, and cursor state separate from
//! gameplay systems. Future camera/control code can query this state without
//! depending directly on winit event matching.

use std::collections::HashSet;

use winit::event::{DeviceEvent, ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};

/// Input state accumulated from window events.
#[derive(Debug, Default)]
pub struct InputState {
    pressed_keys: HashSet<KeyCode>,
    just_pressed_keys: HashSet<KeyCode>,
    pressed_mouse_buttons: HashSet<MouseButton>,
    just_pressed_mouse_buttons: HashSet<MouseButton>,
    pending_mouse_clicks: HashSet<MouseButton>,
    cursor_position: Option<(f64, f64)>,
    cursor_delta: (f64, f64),
    scroll_delta: f32,
    debug_overlay_toggle_requested: bool,
    f3_chord_pressed: bool,
}

impl InputState {
    /// Updates tracked input state from a single winit window event.
    pub fn handle_window_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };

                let f3_key =
                    matches!(event.logical_key, Key::Named(NamedKey::F3)) || code == KeyCode::F3;

                match event.state {
                    ElementState::Pressed => {
                        if !event.repeat && !f3_key && self.is_f3_pressed() {
                            self.f3_chord_pressed = true;
                        }
                        if !self.pressed_keys.contains(&code) {
                            self.just_pressed_keys.insert(code);
                        }
                        self.pressed_keys.insert(code);
                    }
                    ElementState::Released => {
                        if f3_key && !self.f3_chord_pressed {
                            self.debug_overlay_toggle_requested = true;
                        }
                        if f3_key {
                            self.f3_chord_pressed = false;
                        }
                        self.pressed_keys.remove(&code);
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => match state {
                ElementState::Pressed => {
                    if !self.pressed_mouse_buttons.contains(button) {
                        self.just_pressed_mouse_buttons.insert(*button);
                        self.pending_mouse_clicks.insert(*button);
                    }
                    self.pressed_mouse_buttons.insert(*button);
                }
                ElementState::Released => {
                    self.pressed_mouse_buttons.remove(button);
                }
            },
            WindowEvent::CursorMoved { position, .. } => {
                let next = (position.x, position.y);
                if let Some(previous) = self.cursor_position {
                    self.cursor_delta.0 += next.0 - previous.0;
                    self.cursor_delta.1 += next.1 - previous.1;
                }
                self.cursor_position = Some(next);
            }
            WindowEvent::MouseWheel { delta, .. } => {
                self.scroll_delta += match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    MouseScrollDelta::PixelDelta(position) => position.y as f32 / 32.0,
                };
            }
            WindowEvent::Focused(false) => self.clear(),
            _ => {}
        }
    }

    /// Updates tracked input state from device-level events.
    pub fn handle_device_event(&mut self, event: &DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = event {
            self.cursor_delta.0 += delta.0;
            self.cursor_delta.1 += delta.1;
        }
    }

    /// Returns whether a physical keyboard key is currently pressed.
    #[allow(dead_code)]
    pub fn is_key_pressed(&self, key: KeyCode) -> bool {
        self.pressed_keys.contains(&key)
    }

    /// Returns whether a physical key was pressed during this frame.
    #[allow(dead_code)]
    pub fn was_key_just_pressed(&self, key: KeyCode) -> bool {
        self.just_pressed_keys.contains(&key)
    }

    /// Returns and consumes whether a physical key was pressed this frame.
    pub fn take_key_press(&mut self, key: KeyCode) -> bool {
        self.just_pressed_keys.remove(&key)
    }

    /// Returns whether F3 is currently held for debug key chords.
    pub fn is_f3_pressed(&self) -> bool {
        self.pressed_keys.contains(&KeyCode::F3)
    }

    /// Returns whether either Shift key is currently held.
    pub fn is_shift_pressed(&self) -> bool {
        self.pressed_keys.contains(&KeyCode::ShiftLeft)
            || self.pressed_keys.contains(&KeyCode::ShiftRight)
    }

    /// Consumes whether the F3 debug overlay toggle was requested.
    pub fn take_debug_overlay_toggle_requested(&mut self) -> bool {
        let requested = self.debug_overlay_toggle_requested;
        self.debug_overlay_toggle_requested = false;
        requested
    }

    /// Returns whether a mouse button is currently pressed.
    #[allow(dead_code)]
    pub fn is_mouse_button_pressed(&self, button: MouseButton) -> bool {
        self.pressed_mouse_buttons.contains(&button)
    }

    /// Returns whether a mouse button was pressed during this frame.
    pub fn was_mouse_button_just_pressed(&self, button: MouseButton) -> bool {
        self.just_pressed_mouse_buttons.contains(&button)
    }

    /// Consumes a mouse click request after gameplay has handled it.
    pub fn take_mouse_click(&mut self, button: MouseButton) -> bool {
        self.pending_mouse_clicks.remove(&button)
    }

    /// Cursor movement accumulated since the last frame boundary.
    #[allow(dead_code)]
    pub fn cursor_delta(&self) -> (f64, f64) {
        self.cursor_delta
    }

    /// Current cursor position in window coordinates, if known.
    pub fn cursor_position(&self) -> Option<(f64, f64)> {
        self.cursor_position
    }

    /// Sets the cursor position tracked by gameplay after programmatic cursor moves.
    pub fn set_cursor_position(&mut self, position: (f64, f64)) {
        self.cursor_position = Some(position);
    }

    /// Clears any queued mouse clicks that should not reach gameplay.
    pub fn clear_mouse_clicks(&mut self) {
        self.just_pressed_mouse_buttons.clear();
        self.pending_mouse_clicks.clear();
    }

    /// Returns and clears accumulated scroll wheel movement.
    pub fn take_scroll_delta(&mut self) -> f32 {
        let delta = self.scroll_delta;
        self.scroll_delta = 0.0;
        delta
    }

    /// Returns and clears accumulated cursor movement.
    pub fn take_cursor_delta(&mut self) -> (f64, f64) {
        let delta = self.cursor_delta;
        self.cursor_delta = (0.0, 0.0);
        delta
    }

    /// Resets frame-local input data while preserving held buttons and keys.
    pub fn end_frame(&mut self) {
        self.just_pressed_keys.clear();
        self.just_pressed_mouse_buttons.clear();
        self.debug_overlay_toggle_requested = false;
        self.scroll_delta = 0.0;
    }

    fn clear(&mut self) {
        self.pressed_keys.clear();
        self.just_pressed_keys.clear();
        self.just_pressed_mouse_buttons.clear();
        self.pending_mouse_clicks.clear();
        self.pressed_mouse_buttons.clear();
        self.debug_overlay_toggle_requested = false;
        self.f3_chord_pressed = false;
        self.cursor_delta = (0.0, 0.0);
        self.scroll_delta = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn key_press_is_consumed_once() {
        let mut input = InputState::default();
        input.just_pressed_keys.insert(KeyCode::Enter);

        assert!(input.take_key_press(KeyCode::Enter));
        assert!(!input.take_key_press(KeyCode::Enter));
    }
}
