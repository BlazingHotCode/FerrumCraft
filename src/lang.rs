//! Language / translation system for display names.
//!
//! Translations are loaded from JSON files in
//! `assets/<namespace>/lang/<locale>.json` (e.g. `en_us.json`) and provide
//! human-readable names for blocks, items, entities, and other game content.
//!
//! If a translation key has no entry the key itself is returned as a fallback,
//! matching Minecraft's behavior.

use std::collections::HashMap;

/// A table of translation key → display name mappings for one locale.
#[derive(Debug, Clone)]
pub struct TranslationTable {
    entries: HashMap<String, String>,
}

impl TranslationTable {
    /// An empty translation table (useful before lang data is loaded).
    pub fn empty() -> Self {
        Self {
            entries: HashMap::new(),
        }
    }

    /// Loads translations from a JSON file via the resource manager.
    pub fn load(
        resources: &crate::resource::ResourceManager,
        namespace: &str,
        locale: &str,
    ) -> Result<Self, crate::resource::ResourceError> {
        let filename = format!("{locale}.json");
        let entries: HashMap<String, String> = resources.read_json(
            namespace,
            crate::resource::ResourceCategory::Lang,
            &filename,
        )?;
        Ok(Self { entries })
    }

    /// Returns the display name for a translation key.
    ///
    /// Returns `None` if the key is not present.
    pub fn translate(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(|s| s.as_str())
    }

    /// Returns the display name for a key, falling back to the key itself.
    pub fn translate_or_default<'a>(&'a self, key: &'a str) -> &'a str {
        self.translate(key).unwrap_or(key)
    }

    /// Number of loaded translation entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

impl Default for TranslationTable {
    fn default() -> Self {
        Self::empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_table_returns_none() {
        let table = TranslationTable::empty();
        assert_eq!(table.translate("block.stone"), None);
    }

    #[test]
    fn fallback_returns_key() {
        let table = TranslationTable::empty();
        assert_eq!(table.translate_or_default("block.missing"), "block.missing");
    }

    #[test]
    fn loads_from_inline_json() {
        let json: HashMap<String, String> =
            serde_json::from_str(r#"{"block.stone":"Stone"}"#).unwrap();
        let table = TranslationTable { entries: json };
        assert_eq!(table.translate("block.stone"), Some("Stone"));
    }
}
