//! Resource manager and directory layout for data-driven content.
//!
//! Directory structure (relative to the game root):
//!
//! ```text
//! assets/<namespace>/
//!   lang/en_us.json
//!   textures/<path>.png
//!   models/block/<path>.json
//!   models/item/<path>.json
//!   blockstates/<path>.json
//!
//! data/<namespace>/
//!   tags/blocks/<id>.json
//!   tags/items/<id>.json
//!   recipes/<id>.json
//!   loot_tables/blocks/<id>.json
//!   worldgen/...
//! ```

use std::path::PathBuf;

/// Category of a resource (determines which subdirectory files live in).
#[derive(Clone, Copy, Debug)]
pub enum ResourceDomain {
    Assets,
    Data,
}

/// High-level resource category used to build file paths.
#[derive(Clone, Copy, Debug)]
#[allow(dead_code)]
pub enum ResourceCategory {
    Lang,
    Texture,
    BlockModel,
    ItemModel,
    Blockstate,
    BlockTag,
    ItemTag,
    Recipe,
    LootTable,
}

impl ResourceCategory {
    fn relative_path(&self) -> &'static str {
        match self {
            Self::Lang => "lang",
            Self::Texture => "textures",
            Self::BlockModel => "models/block",
            Self::ItemModel => "models/item",
            Self::Blockstate => "blockstates",
            Self::BlockTag => "tags/blocks",
            Self::ItemTag => "tags/items",
            Self::Recipe => "recipes",
            Self::LootTable => "loot_tables/blocks",
        }
    }

    fn domain(&self) -> ResourceDomain {
        match self {
            Self::Lang | Self::Texture | Self::BlockModel | Self::ItemModel | Self::Blockstate => {
                ResourceDomain::Assets
            }
            Self::BlockTag | Self::ItemTag | Self::Recipe | Self::LootTable => ResourceDomain::Data,
        }
    }
}

/// Loads game content from the asset and data directory trees.
#[derive(Debug)]
pub struct ResourceManager {
    root: PathBuf,
}

impl ResourceManager {
    /// Creates a resource manager rooted at the given directory.
    ///
    /// The root is typically the working directory or a dedicated game directory.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    /// Full filesystem path for a resource.
    pub fn path(&self, namespace: &str, category: ResourceCategory, filename: &str) -> PathBuf {
        let domain_dir = match category.domain() {
            ResourceDomain::Assets => "assets",
            ResourceDomain::Data => "data",
        };
        self.root
            .join(domain_dir)
            .join(namespace)
            .join(category.relative_path())
            .join(filename)
    }

    /// Reads the raw bytes of a resource.
    pub fn read_bytes(
        &self,
        namespace: &str,
        category: ResourceCategory,
        filename: &str,
    ) -> Result<Vec<u8>, ResourceError> {
        let path = self.path(namespace, category, filename);
        std::fs::read(&path).map_err(|source| ResourceError::Io {
            path: path.clone(),
            source,
        })
    }

    /// Reads a UTF-8 text file (e.g. lang, JSON source).
    pub fn read_text(
        &self,
        namespace: &str,
        category: ResourceCategory,
        filename: &str,
    ) -> Result<String, ResourceError> {
        let bytes = self.read_bytes(namespace, category, filename)?;
        String::from_utf8(bytes).map_err(|source| ResourceError::Utf8 {
            path: self.path(namespace, category, filename),
            source,
        })
    }

    /// Reads and deserializes a JSON resource file.
    pub fn read_json<T: serde::de::DeserializeOwned>(
        &self,
        namespace: &str,
        category: ResourceCategory,
        filename: &str,
    ) -> Result<T, ResourceError> {
        let text = self.read_text(namespace, category, filename)?;
        serde_json::from_str(&text).map_err(|source| ResourceError::Json {
            path: self.path(namespace, category, filename),
            source,
        })
    }

    /// Whether the resource root directory exists on disk.
    #[allow(dead_code)]
    pub fn root_exists(&self) -> bool {
        self.root.exists()
    }
}

/// Errors that can occur during resource loading.
#[derive(Debug)]
pub enum ResourceError {
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[allow(dead_code)]
    Utf8 {
        path: PathBuf,
        source: std::string::FromUtf8Error,
    },
    Json {
        path: PathBuf,
        source: serde_json::Error,
    },
}

impl std::fmt::Display for ResourceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => {
                write!(f, "I/O error reading {}: {}", path.display(), source)
            }
            Self::Utf8 { path, .. } => {
                write!(f, "file is not valid UTF-8: {}", path.display())
            }
            Self::Json { path, source } => {
                write!(f, "JSON error in {}: {}", path.display(), source)
            }
        }
    }
}

impl std::error::Error for ResourceError {}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// Creates a manager whose root is a new temporary directory.
    /// The directory is removed when the returned helper is dropped.
    struct TempManager {
        manager: ResourceManager,
        path: PathBuf,
    }

    impl TempManager {
        fn new() -> Self {
            // Use a counter so each test gets a unique directory.
            use std::sync::atomic::{AtomicU32, Ordering};
            static COUNTER: AtomicU32 = AtomicU32::new(0);
            let seq = COUNTER.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!("ferrumcraft_test_{seq}"));
            let _ = fs::remove_dir_all(&path);
            fs::create_dir_all(&path).unwrap();
            let manager = ResourceManager::new(&path);
            Self { manager, path }
        }

        fn write(
            &self,
            namespace: &str,
            category: ResourceCategory,
            filename: &str,
            content: &str,
        ) {
            let path = self.manager.path(namespace, category, filename);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, content).unwrap();
        }
    }

    impl Drop for TempManager {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn builds_asset_path() {
        let tmp = TempManager::new();
        let path = tmp
            .manager
            .path("ferrumcraft", ResourceCategory::Lang, "en_us.json");
        assert!(path.to_string_lossy().ends_with(if cfg!(windows) {
            "assets\\ferrumcraft\\lang\\en_us.json"
        } else {
            "assets/ferrumcraft/lang/en_us.json"
        }));
    }

    #[test]
    fn builds_data_path() {
        let tmp = TempManager::new();
        let path = tmp
            .manager
            .path("ferrumcraft", ResourceCategory::Recipe, "planks.json");
        assert!(path.to_string_lossy().ends_with(if cfg!(windows) {
            "data\\ferrumcraft\\recipes\\planks.json"
        } else {
            "data/ferrumcraft/recipes/planks.json"
        }));
    }

    #[test]
    fn reads_text_file() {
        let tmp = TempManager::new();
        tmp.write(
            "ferrumcraft",
            ResourceCategory::Lang,
            "en_us.json",
            r#"{"block.stone": "Stone"}"#,
        );

        let content = tmp
            .manager
            .read_text("ferrumcraft", ResourceCategory::Lang, "en_us.json");
        assert!(content.is_ok());
        assert_eq!(content.unwrap(), r#"{"block.stone": "Stone"}"#);
    }

    #[test]
    fn reads_json_file() {
        #[derive(serde::Deserialize, PartialEq, Debug)]
        struct LangFile {
            #[serde(rename = "block.stone")]
            block_stone: String,
        }

        let tmp = TempManager::new();
        tmp.write(
            "ferrumcraft",
            ResourceCategory::Lang,
            "en_us.json",
            r#"{"block.stone": "Stone"}"#,
        );

        let lang: LangFile = tmp
            .manager
            .read_json("ferrumcraft", ResourceCategory::Lang, "en_us.json")
            .unwrap();
        assert_eq!(lang.block_stone, "Stone");
    }

    #[test]
    fn returns_error_for_missing_file() {
        let tmp = TempManager::new();
        let result =
            tmp.manager
                .read_bytes("ferrumcraft", ResourceCategory::Lang, "nonexistent.json");
        assert!(result.is_err());
    }
}
