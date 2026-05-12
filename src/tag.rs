//! Tag system for grouping blocks, items, and other content by namespaced IDs.
//!
//! Tags are JSON files in `data/<namespace>/tags/<category>/<name>.json` and
//! contain lists of namespaced IDs. They allow grouping content without
//! hardcoding in the engine — e.g., a `blocks/solid` tag lists all solid blocks.
//!
//! In actual use: Systems query tags instead of checking individual block IDs.
//! Worldgen asks "is this block in the `blocks/solid` tag?" instead of matching
//! against a hardcoded list of block IDs.

use std::collections::HashSet;

use serde::Deserialize;

use crate::id::NamespacedId;
use crate::resource::{ResourceCategory, ResourceManager};

/// A tag containing a set of namespaced IDs.
#[derive(Clone, Debug)]
pub struct Tag {
    /// The entries in this tag.
    pub values: HashSet<NamespacedId>,
}

impl Tag {
    /// Returns `true` if the given ID is in this tag.
    pub fn contains(&self, id: &NamespacedId) -> bool {
        self.values.contains(id)
    }

    /// Number of entries in this tag.
    pub fn len(&self) -> usize {
        self.values.len()
    }
}

/// Loads a single tag from a JSON file.
pub fn load_tag(
    resources: &ResourceManager,
    namespace: &str,
    category: ResourceCategory,
    name: &str,
) -> Result<Tag, TagError> {
    let filename = format!("{name}.json");
    let file: TagFile = resources
        .read_json(namespace, category, &filename)
        .map_err(|e| TagError::Load {
            tag: format!("{namespace}:{name}"),
            detail: e.to_string(),
        })?;

    let mut values = HashSet::new();
    for raw in &file.values {
        match raw.parse::<NamespacedId>() {
            Ok(id) => {
                values.insert(id);
            }
            Err(e) => {
                log::warn!(target: "tags", "Invalid ID '{raw}' in tag {name}: {e}");
            }
        }
    }

    Ok(Tag { values })
}

/// Raw JSON structure of a tag file.
#[derive(Debug, Deserialize)]
struct TagFile {
    values: Vec<String>,
}

/// Errors that can occur during tag loading.
#[derive(Debug)]
pub enum TagError {
    Load { tag: String, detail: String },
}

impl std::fmt::Display for TagError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Load { tag, detail } => write!(f, "error loading tag {tag}: {detail}"),
        }
    }
}

impl std::error::Error for TagError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_tag_contains_nothing() {
        let tag = Tag {
            values: HashSet::new(),
        };
        let id: NamespacedId = "ferrumcraft:stone".parse().unwrap();
        assert!(!tag.contains(&id));
    }

    #[test]
    fn tag_contains_registered_ids() {
        let mut values = HashSet::new();
        values.insert("ferrumcraft:stone".parse().unwrap());
        values.insert("ferrumcraft:dirt".parse().unwrap());
        let tag = Tag { values };
        assert!(tag.contains(&"ferrumcraft:stone".parse().unwrap()));
        assert!(tag.contains(&"ferrumcraft:dirt".parse().unwrap()));
        assert!(!tag.contains(&"ferrumcraft:air".parse().unwrap()));
    }
}
