//! Input state collected from winit events.
//!
//! This module keeps raw keyboard, mouse button, and cursor state separate from
//! gameplay systems. Future camera/control code can query this state without
//! depending directly on winit event matching.

use std::collections::HashSet;

use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::keyboard::{KeyCode, PhysicalKey};

/// Input state accumulated from window events.
#[derive(Debug, Default)]
pub struct InputState {
    pressed_keys: HashSet<KeyCode>,
    pressed_mouse_buttons: HashSet<MouseButton>,
    cursor_position: Option<(f64, f64)>,
    cursor_delta: (f64, f64),
}

impl InputState {
    /// Updates tracked input state from a single winit window event.
    pub fn handle_window_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::KeyboardInput { event, .. } => {
                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };

                match event.state {
                    ElementState::Pressed => {
                        self.pressed_keys.insert(code);
                    }
                    ElementState::Released => {
                        self.pressed_keys.remove(&code);
                    }
                }
            }
            WindowEvent::MouseInput { state, button, .. } => match state {
                ElementState::Pressed => {
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
            WindowEvent::Focused(false) => self.clear(),
            _ => {}
        }
    }

    /// Returns whether a physical keyboard key is currently pressed.
    #[allow(dead_code)]
    pub fn is_key_pressed(&self, key: KeyCode) -> bool {
        self.pressed_keys.contains(&key)
    }

    /// Returns whether a mouse button is currently pressed.
    #[allow(dead_code)]
    pub fn is_mouse_button_pressed(&self, button: MouseButton) -> bool {
        self.pressed_mouse_buttons.contains(&button)
    }

    /// Cursor movement accumulated since the last frame boundary.
    #[allow(dead_code)]
    pub fn cursor_delta(&self) -> (f64, f64) {
        self.cursor_delta
    }

    /// Resets frame-local input data while preserving held buttons and keys.
    pub fn end_frame(&mut self) {
        self.cursor_delta = (0.0, 0.0);
    }

    fn clear(&mut self) {
        self.pressed_keys.clear();
        self.pressed_mouse_buttons.clear();
        self.cursor_delta = (0.0, 0.0);
    }
}
