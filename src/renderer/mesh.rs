//! Mesh data structures.
//!
//! This module is the future home for CPU-side vertex/index data and GPU buffer
//! handles. Keeping it separate from pipelines prevents mesh layout decisions
//! from leaking into frame orchestration.

/// Renderable geometry owned by a [`Scene`](super::scene::Scene).
///
/// The type is currently empty because the renderer only clears the screen.
/// Vertex/index buffers will be added here when static 3D rendering lands.
#[derive(Debug, Default)]
pub struct Mesh;
