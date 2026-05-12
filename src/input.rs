//! Input state collected from winit events.
//!
//! This module keeps raw keyboard, mouse button, and cursor state separate from
//! gameplay systems. Future camera/control code can query this state without
//! depending directly on winit event matching.

use std::collections::HashSet;

use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::keyboard::{Key, KeyCode, NamedKey, PhysicalKey};

/// Input state accumulated from window events.
#[derive(Debug, Default)]
pub struct InputState {
    pressed_keys: HashSet<KeyCode>,
    just_pressed_keys: HashSet<KeyCode>,
    pressed_mouse_buttons: HashSet<MouseButton>,
    cursor_position: Option<(f64, f64)>,
    cursor_delta: (f64, f64),
    debug_overlay_toggle_requested: bool,
}

impl InputState {
    /// Updates tracked input state from a single winit window event.
    pub fn handle_window_event(&mut self, event: &WindowEvent) {
        match event {
            WindowEvent::KeyboardInput { event, .. } => {
                if event.state == ElementState::Pressed
                    && !event.repeat
                    && (matches!(event.logical_key, Key::Named(NamedKey::F3))
                        || matches!(event.physical_key, PhysicalKey::Code(KeyCode::F3)))
                {
                    self.debug_overlay_toggle_requested = true;
                }

                let PhysicalKey::Code(code) = event.physical_key else {
                    return;
                };

                match event.state {
                    ElementState::Pressed => {
                        if !self.pressed_keys.contains(&code) {
                            self.just_pressed_keys.insert(code);
                        }
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

    /// Returns whether a physical key was pressed during this frame.
    #[allow(dead_code)]
    pub fn was_key_just_pressed(&self, key: KeyCode) -> bool {
        self.just_pressed_keys.contains(&key)
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

    /// Cursor movement accumulated since the last frame boundary.
    #[allow(dead_code)]
    pub fn cursor_delta(&self) -> (f64, f64) {
        self.cursor_delta
    }

    /// Resets frame-local input data while preserving held buttons and keys.
    pub fn end_frame(&mut self) {
        self.just_pressed_keys.clear();
        self.debug_overlay_toggle_requested = false;
        self.cursor_delta = (0.0, 0.0);
    }

    fn clear(&mut self) {
        self.pressed_keys.clear();
        self.just_pressed_keys.clear();
        self.pressed_mouse_buttons.clear();
        self.debug_overlay_toggle_requested = false;
        self.cursor_delta = (0.0, 0.0);
    }
}
