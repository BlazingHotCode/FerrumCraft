//! Namespaced registries for data-defined game content.
//!
//! A registry maps [`NamespacedId`]s to typed entries (blocks, items, entities,
//! biomes, etc.). The bootstrap order below must be respected because later
//! registries reference entries from earlier ones (e.g. an item registry entry
//! references a block model).
//!
//! # Bootstrap order
//!
//! 1. **Blocks** — block definitions first; item models depend on them.
//! 2. **Items** — item definitions referencing block models or independent models.
//! 3. **Entities** — entity types.
//! 4. **Biomes** — biome definitions used by worldgen.
//! 5. **Worldgen features** — trees, ores, lakes, structures.
//! 6. **Recipes** — crafting/smelting recipes referencing items.
//! 7. **Loot tables** — block/entity drop tables referencing items.
//! 8. **Tags** — block/item/entity tags grouping entries from earlier registries.
//! 9. **Sounds** — sound event definitions.
//! 10. **Particles** — particle type definitions.
//! 11. **Dimensions** — dimension/world-type definitions.
//! 12. **Commands** — command and argument type registrations.

use std::collections::HashMap;

use crate::id::NamespacedId;

/// A namespaced registry that maps stable string IDs to typed entries.
#[derive(Debug, Clone)]
pub struct Registry<T> {
    entries: HashMap<NamespacedId, T>,
    order: Vec<NamespacedId>,
}

impl<T: 'static> Registry<T> {
    /// An empty registry.
    pub fn new() -> Self {
        Self {
            entries: HashMap::new(),
            order: Vec::new(),
        }
    }

    /// Registers an entry under the given ID.
    ///
    /// Panics if the ID is already registered (double-registration is a bug).
    pub fn register(&mut self, id: NamespacedId, entry: T) {
        assert!(
            self.entries.insert(id.clone(), entry).is_none(),
            "duplicate registration: {id}",
        );
        self.order.push(id);
    }

    /// Looks up an entry by its namespaced ID.
    pub fn get(&self, id: &NamespacedId) -> Option<&T> {
        self.entries.get(id)
    }

    /// Returns `true` if the given ID is registered.
    pub fn contains(&self, id: &NamespacedId) -> bool {
        self.entries.contains_key(id)
    }

    /// Number of registered entries.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// Iterates over all registered entries in registration order.
    pub fn iter(&self) -> impl Iterator<Item = (&NamespacedId, &T)> {
        self.order.iter().map(|id| (id, &self.entries[id]))
    }

    /// Iterates over all registered entries, consuming the registry.
    pub fn into_iter(mut self) -> impl Iterator<Item = (NamespacedId, T)> {
        let order = std::mem::take(&mut self.order);
        order
            .into_iter()
            .filter_map(move |id| self.entries.remove(&id).map(|entry| (id, entry)))
    }
}

impl<T: 'static> Default for Registry<T> {
    fn default() -> Self {
        Self::new()
    }
}

/// Builds a registry declaratively.
#[derive(Debug)]
pub struct RegistryBuilder<T> {
    entries: Vec<(NamespacedId, T)>,
}

impl<T: 'static> RegistryBuilder<T> {
    /// Creates a new empty builder.
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Adds an entry to the builder.
    pub fn add(mut self, id: NamespacedId, entry: T) -> Self {
        self.entries.push((id, entry));
        self
    }

    /// Consumes the builder and produces a [`Registry`].
    pub fn build(self) -> Registry<T> {
        let mut reg = Registry::new();
        for (id, entry) in self.entries {
            reg.register(id, entry);
        }
        reg
    }
}

impl<T: 'static> Default for RegistryBuilder<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn register_and_lookup() {
        let mut reg: Registry<u32> = Registry::new();
        let id: NamespacedId = "ferrumcraft:stone".parse().unwrap();
        reg.register(id.clone(), 42);
        assert_eq!(reg.get(&id), Some(&42));
    }

    #[test]
    fn contains_works() {
        let mut reg: Registry<u32> = Registry::new();
        let id: NamespacedId = "ferrumcraft:stone".parse().unwrap();
        assert!(!reg.contains(&id));
        reg.register(id.clone(), 1);
        assert!(reg.contains(&id));
    }

    #[test]
    fn len_tracks_count() {
        let mut reg: Registry<u32> = Registry::new();
        assert_eq!(reg.len(), 0);
        reg.register("a:b".parse().unwrap(), 1);
        reg.register("c:d".parse().unwrap(), 2);
        assert_eq!(reg.len(), 2);
    }

    #[test]
    fn iter_in_order() {
        let mut reg: Registry<u32> = Registry::new();
        reg.register("mod:a".parse().unwrap(), 10);
        reg.register("mod:b".parse().unwrap(), 20);
        let ids: Vec<String> = reg.iter().map(|(id, _)| id.to_string()).collect();
        assert_eq!(ids, vec!["mod:a", "mod:b"]);
    }

    #[test]
    fn builder_constructs_registry() {
        let reg: Registry<&str> = RegistryBuilder::new()
            .add("ferrumcraft:a".parse().unwrap(), "alpha")
            .add("ferrumcraft:b".parse().unwrap(), "beta")
            .build();
        assert_eq!(reg.len(), 2);
    }

    #[test]
    fn into_iter_yields_all() {
        let mut reg: Registry<i32> = Registry::new();
        reg.register("m:x".parse().unwrap(), 100);
        reg.register("m:y".parse().unwrap(), 200);
        let vec: Vec<(String, i32)> = reg.into_iter().map(|(id, v)| (id.to_string(), v)).collect();
        assert_eq!(vec.len(), 2);
    }

    #[test]
    #[should_panic(expected = "duplicate registration")]
    fn panics_on_duplicate() {
        let mut reg: Registry<u32> = Registry::new();
        reg.register("mod:dup".parse().unwrap(), 1);
        reg.register("mod:dup".parse().unwrap(), 2);
    }
}
