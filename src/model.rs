//! Block model loading and resolution.
//!
//! Models are loaded from JSON files at `models/block/<id>.json` and define
//! which textures appear on each face of a block. Models can inherit from a
//! parent (e.g. `block/cube_all`) to share face layouts.
//!
//! In actual use: when the chunk meshing system needs geometry for a block, it
//! looks up the block's model to get the texture path for each face. Those
//! paths are resolved into atlas UV coordinates later.

use std::collections::HashMap;

use serde::Deserialize;

use crate::id::NamespacedId;
use crate::registry::Registry;
use crate::resource::{ResourceCategory, ResourceManager};

/// The six faces of a block in Minecraft standard order.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum Face {
    Right = 0,
    Left = 1,
    Top = 2,
    Bottom = 3,
    Front = 4,
    Back = 5,
}

pub const ALL_FACES: [Face; 6] = [
    Face::Right,
    Face::Left,
    Face::Top,
    Face::Bottom,
    Face::Front,
    Face::Back,
];

/// A resolved block model with a concrete texture path for each face.
#[derive(Clone, Debug)]
pub struct BlockModel {
    /// Texture path per face, e.g. `"block/stone"` (without namespace or extension).
    pub faces: [String; 6],
}

impl BlockModel {
    /// Returns the texture path for a given face.
    pub fn texture(&self, face: Face) -> &str {
        &self.faces[face as usize]
    }
}

/// Loads all block models for the given block IDs and returns a registry.
///
/// Each block's model file is loaded from `models/block/<path>.json`.
/// Falls back to a missing-texture model on error.
pub fn load_block_models(
    resources: &ResourceManager,
    namespace: &str,
    block_ids: &[NamespacedId],
) -> Registry<BlockModel> {
    let mut reg = Registry::new();

    for id in block_ids {
        let filename = format!("{}.json", id.path());
        match load_single_model(resources, namespace, &filename) {
            Ok(model) => {
                reg.register(id.clone(), model);
            }
            Err(e) => {
                log::warn!(target: "models", "Failed to load model for {id}: {e}");
                reg.register(
                    id.clone(),
                    BlockModel {
                        faces: std::array::from_fn(|_| String::new()),
                    },
                );
            }
        }
    }

    reg
}

fn load_single_model(
    resources: &ResourceManager,
    namespace: &str,
    filename: &str,
) -> Result<BlockModel, ModelError> {
    let file: ModelFile = resources
        .read_json(namespace, ResourceCategory::BlockModel, filename)
        .map_err(|e| ModelError::Load {
            file: filename.to_string(),
            detail: e.to_string(),
        })?;

    let faces = resolve_faces(&file, resources, namespace)?;
    Ok(BlockModel { faces })
}

/// Resolves a model file (possibly with parent) into a [String; 6] face map.
fn resolve_faces(
    file: &ModelFile,
    resources: &ResourceManager,
    namespace: &str,
) -> Result<[String; 6], ModelError> {
    // Collect all textures: start with parent's, override with this model's.
    let all_textures = if let Some(ref parent_path) = file.parent {
        let parent_textures = load_parent_textures(parent_path, resources, namespace)?;
        let mut merged = parent_textures;
        for (k, v) in &file.textures {
            merged.insert(k.clone(), v.clone());
        }
        merged
    } else {
        file.textures.clone()
    };

    // Determine the face layout from the parent name.
    let face_keys: [&str; 6] = match file.parent.as_deref() {
        Some("builtin/cube_all") | None => ["all"; 6],
        Some(other) => {
            return Err(ModelError::UnknownParent {
                parent: other.to_string(),
            });
        }
    };

    let mut faces = std::array::from_fn::<String, 6, _>(|_| String::new());
    for (i, key) in face_keys.iter().enumerate() {
        let tex = all_textures
            .get(*key)
            .ok_or_else(|| ModelError::MissingTexture {
                key: (*key).to_string(),
                file: "unknown".to_string(),
            })?;
        faces[i] = tex.clone();
    }

    Ok(faces)
}

/// Loads texture map from a parent model, supporting built-in parents directly.
fn load_parent_textures(
    parent_path: &str,
    resources: &ResourceManager,
    namespace: &str,
) -> Result<HashMap<String, String>, ModelError> {
    // Built-in parents don't need a file on disk.
    if parent_path == "builtin/cube_all" {
        return Ok(HashMap::new());
    }

    // Otherwise load from a JSON file.
    let parent_file: ModelFile = resources
        .read_json(namespace, ResourceCategory::BlockModel, parent_path)
        .map_err(|e| ModelError::Load {
            file: parent_path.to_string(),
            detail: e.to_string(),
        })?;
    Ok(parent_file.textures)
}

/// Raw JSON structure of a model file on disk.
#[derive(Clone, Debug, Deserialize)]
struct ModelFile {
    parent: Option<String>,
    #[serde(default)]
    textures: HashMap<String, String>,
}

/// Errors that can occur during model loading.
#[derive(Debug)]
pub enum ModelError {
    Load { file: String, detail: String },
    UnknownParent { parent: String },
    MissingTexture { key: String, file: String },
}

impl std::fmt::Display for ModelError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Load { file, detail } => write!(f, "error loading model {file}: {detail}"),
            Self::UnknownParent { parent } => write!(f, "unknown model parent: {parent}"),
            Self::MissingTexture { key, file } => {
                write!(f, "missing texture '{key}' in {file}")
            }
        }
    }
}

impl std::error::Error for ModelError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fallback_model_has_default_faces() {
        let model = BlockModel {
            faces: std::array::from_fn(|_| String::new()),
        };
        for face in &ALL_FACES {
            assert_eq!(model.texture(*face), "");
        }
    }

    #[test]
    fn face_enum_matches_array_order() {
        assert_eq!(Face::Right as usize, 0);
        assert_eq!(Face::Left as usize, 1);
        assert_eq!(Face::Top as usize, 2);
        assert_eq!(Face::Bottom as usize, 3);
        assert_eq!(Face::Front as usize, 4);
        assert_eq!(Face::Back as usize, 5);
    }
}
