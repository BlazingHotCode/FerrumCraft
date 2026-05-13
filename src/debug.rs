//! Runtime debug overlay state.
//!
//! The overlay mirrors Minecraft's F3-style diagnostics at a small scale. It is
//! intentionally independent of rendering so gameplay systems can add player and
//! chunk data here as those systems land.

use std::time::Duration;

use glam::Vec3;

/// User-toggleable debug data shown by the overlay.
#[derive(Debug, Default)]
pub struct DebugOverlay {
    visible: bool,
    frame_count: u32,
    fps_elapsed: Duration,
    fps: u32,
    frame_ms: f32,
    player_position: Vec3,
    facing: String,
    visible_meshes: usize,
    culled_meshes: usize,
    render_distance_chunks: i32,
    world_seed: u64,
    biome: String,
}

impl DebugOverlay {
    /// Toggles F3 diagnostics on or off.
    pub fn toggle(&mut self) {
        self.visible = !self.visible;
    }

    /// Records the completed render frame for FPS/frame-time reporting.
    pub fn record_frame(&mut self, frame_time: Duration) {
        self.frame_ms = frame_time.as_secs_f32() * 1000.0;
        self.frame_count += 1;
        self.fps_elapsed += frame_time;

        if self.fps_elapsed >= Duration::from_secs(1) {
            self.fps = self.frame_count;
            self.frame_count = 0;
            self.fps_elapsed = Duration::ZERO;
        }
    }

    /// Updates player/camera diagnostics shown by F3.
    pub fn set_player_position(&mut self, position: Vec3) {
        self.player_position = position;
    }

    /// Updates the facing direction string shown by F3.
    pub fn set_facing(&mut self, facing: String) {
        self.facing = facing;
    }

    /// Updates renderer culling diagnostics shown by F3.
    pub fn set_render_stats(&mut self, visible_meshes: usize, culled_meshes: usize) {
        self.visible_meshes = visible_meshes;
        self.culled_meshes = culled_meshes;
    }

    /// Updates the chunk render distance diagnostic shown by F3.
    pub fn set_render_distance(&mut self, chunks: i32) {
        self.render_distance_chunks = chunks;
    }

    /// Updates the deterministic world seed diagnostic shown by F3.
    pub fn set_world_seed(&mut self, seed: u64) {
        self.world_seed = seed;
    }

    /// Updates the current biome diagnostic shown by F3.
    pub fn set_biome(&mut self, biome: impl Into<String>) {
        self.biome = biome.into();
    }

    /// Text rendered by the overlay when visible.
    pub fn text(&self) -> Option<String> {
        self.visible.then(|| {
            format!(
                "F3 DEBUG\nFPS: {}\nFRAME: {:.2} MS\nSEED: {}\nPOSITION: {:.2} {:.2} {:.2}\nCHUNK: {} {}\nBIOME: {}\nFACING: {}\nRENDER DISTANCE: {} CHUNKS\nMESHES: {} VISIBLE, {} CULLED",
                self.fps,
                self.frame_ms,
                self.world_seed,
                self.player_position.x,
                self.player_position.y,
                self.player_position.z,
                chunk_coord(self.player_position.x),
                chunk_coord(self.player_position.z),
                self.biome,
                self.facing,
                self.render_distance_chunks,
                self.visible_meshes,
                self.culled_meshes,
            )
        })
    }
}

fn chunk_coord(block_coord: f32) -> i32 {
    ((block_coord + 8.5).floor() as i32).div_euclid(16)
}
