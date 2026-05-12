//! Namespaced string identifiers for data-driven game content.
//!
//! FerrumCraft stores content references as stable strings like
//! `ferrumcraft:stone` instead of runtime numeric IDs. Runtime registries can
//! still map these IDs to compact indices internally, but saves and data files
//! should use [`NamespacedId`] so content can be extended safely.

use std::fmt;
use std::str::FromStr;

const DEFAULT_NAMESPACE: &str = "ferrumcraft";

/// Stable identifier for registry-backed content.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct NamespacedId {
    namespace: String,
    path: String,
}

impl NamespacedId {
    /// Creates a validated namespaced ID.
    pub fn new(namespace: impl Into<String>, path: impl Into<String>) -> Result<Self, IdError> {
        let namespace = namespace.into();
        let path = path.into();
        validate_part(&namespace, IdPart::Namespace)?;
        validate_part(&path, IdPart::Path)?;
        Ok(Self { namespace, path })
    }

    /// Creates an ID in the built-in `ferrumcraft` namespace.
    pub fn ferrumcraft(path: impl Into<String>) -> Result<Self, IdError> {
        Self::new(DEFAULT_NAMESPACE, path)
    }

    /// Namespace segment, e.g. `ferrumcraft`.
    pub fn namespace(&self) -> &str {
        &self.namespace
    }

    /// Path segment, e.g. `blocks/stone` or `stone`.
    pub fn path(&self) -> &str {
        &self.path
    }
}

impl fmt::Display for NamespacedId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}", self.namespace, self.path)
    }
}

impl serde::Serialize for NamespacedId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> serde::Deserialize<'de> for NamespacedId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let s = String::deserialize(deserializer)?;
        s.parse().map_err(serde::de::Error::custom)
    }
}

impl FromStr for NamespacedId {
    type Err = IdError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let Some((namespace, path)) = value.split_once(':') else {
            return Self::new(DEFAULT_NAMESPACE, value);
        };
        if path.contains(':') {
            return Err(IdError::TooManySeparators);
        }
        Self::new(namespace, path)
    }
}

/// Validation failure for a namespaced ID.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum IdError {
    EmptyNamespace,
    EmptyPath,
    InvalidNamespaceCharacter(char),
    InvalidPathCharacter(char),
    TooManySeparators,
}

impl fmt::Display for IdError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyNamespace => write!(f, "namespace cannot be empty"),
            Self::EmptyPath => write!(f, "path cannot be empty"),
            Self::InvalidNamespaceCharacter(ch) => {
                write!(f, "invalid namespace character '{ch}'")
            }
            Self::InvalidPathCharacter(ch) => write!(f, "invalid path character '{ch}'"),
            Self::TooManySeparators => write!(f, "ID can contain only one ':' separator"),
        }
    }
}

impl std::error::Error for IdError {}

#[derive(Clone, Copy)]
enum IdPart {
    Namespace,
    Path,
}

fn validate_part(value: &str, part: IdPart) -> Result<(), IdError> {
    if value.is_empty() {
        return Err(match part {
            IdPart::Namespace => IdError::EmptyNamespace,
            IdPart::Path => IdError::EmptyPath,
        });
    }

    for ch in value.chars() {
        let valid = match part {
            IdPart::Namespace => {
                ch.is_ascii_lowercase()
                    || ch.is_ascii_digit()
                    || ch == '_'
                    || ch == '-'
                    || ch == '.'
            }
            IdPart::Path => {
                ch.is_ascii_lowercase()
                    || ch.is_ascii_digit()
                    || ch == '_'
                    || ch == '-'
                    || ch == '.'
                    || ch == '/'
            }
        };

        if !valid {
            return Err(match part {
                IdPart::Namespace => IdError::InvalidNamespaceCharacter(ch),
                IdPart::Path => IdError::InvalidPathCharacter(ch),
            });
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_explicit_namespace() {
        let id: NamespacedId = "ferrumcraft:stone".parse().unwrap();
        assert_eq!(id.namespace(), "ferrumcraft");
        assert_eq!(id.path(), "stone");
        assert_eq!(id.to_string(), "ferrumcraft:stone");
    }

    #[test]
    fn defaults_missing_namespace_to_ferrumcraft() {
        let id: NamespacedId = "dirt".parse().unwrap();
        assert_eq!(id.to_string(), "ferrumcraft:dirt");
    }

    #[test]
    fn creates_builtin_namespace_ids() {
        let id = NamespacedId::ferrumcraft("grass_block").unwrap();
        assert_eq!(id.to_string(), "ferrumcraft:grass_block");
    }

    #[test]
    fn accepts_resource_paths() {
        let id: NamespacedId = "ferrumcraft:models/block/cube_all".parse().unwrap();
        assert_eq!(id.path(), "models/block/cube_all");
    }

    #[test]
    fn rejects_uppercase_ids() {
        let error = "FerrumCraft:Stone".parse::<NamespacedId>().unwrap_err();
        assert_eq!(error, IdError::InvalidNamespaceCharacter('F'));
    }

    #[test]
    fn rejects_extra_separator() {
        let error = "a:b:c".parse::<NamespacedId>().unwrap_err();
        assert_eq!(error, IdError::TooManySeparators);
    }

    #[test]
    fn serde_roundtrip() {
        let id: NamespacedId = "ferrumcraft:stone".parse().unwrap();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, "\"ferrumcraft:stone\"");
        let deserialized: NamespacedId = serde_json::from_str(&json).unwrap();
        assert_eq!(id, deserialized);
    }
}
