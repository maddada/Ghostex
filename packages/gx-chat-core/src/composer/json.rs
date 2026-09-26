//! A JSON object that keeps the order its keys were written in.
//!
//! CDXC:SessionChat 2026-09-22 WHY:
//! The replay gate that checked the port fingerprinted a pure query's answer by hashing its
//! serialized text, so the key order of every object inside it was part of the contract.
//! `serde_json::Map` is a `BTreeMap` unless the crate turns on `preserve_order`, and without it
//! `{action, path, type, view, line}` came out as `{action, column, endLine, line, path, type,
//! view}` and failed the gate on a value that was in fact identical. Rather than add `indexmap` to
//! the whole crate, the free-form objects family d emits are built here, in the order the
//! TypeScript wrote them.

use serde::de::{MapAccess, Visitor};
use serde::ser::SerializeMap;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use serde_json::Value;
use std::fmt;

/// An object whose keys serialize in insertion order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct OrderedMap(Vec<(String, Value)>);

impl OrderedMap {
    /// An empty object.
    pub fn new() -> Self {
        Self(Vec::new())
    }

    /// Appends a key, or replaces its value in place when it is already present.
    pub fn insert(&mut self, key: impl Into<String>, value: impl Into<Value>) -> &mut Self {
        let key = key.into();
        let value = value.into();
        match self.0.iter_mut().find(|(name, _)| *name == key) {
            Some(slot) => slot.1 = value,
            None => self.0.push((key, value)),
        }
        self
    }

    /// A value by key.
    pub fn get(&self, key: &str) -> Option<&Value> {
        self.0
            .iter()
            .find(|(name, _)| name == key)
            .map(|(_, value)| value)
    }

    /// The pairs in order.
    pub fn entries(&self) -> &[(String, Value)] {
        &self.0
    }

    /// Whether the object has no keys.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// How many keys the object has.
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

/// Builds an object from pairs in order.
impl FromIterator<(String, Value)> for OrderedMap {
    fn from_iter<T: IntoIterator<Item = (String, Value)>>(iter: T) -> Self {
        let mut map = Self::new();
        for (key, value) in iter {
            map.insert(key, value);
        }
        map
    }
}

impl Serialize for OrderedMap {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(Some(self.0.len()))?;
        for (key, value) in &self.0 {
            map.serialize_entry(key, value)?;
        }
        map.end()
    }
}

impl<'de> Deserialize<'de> for OrderedMap {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct OrderedMapVisitor;

        impl<'de> Visitor<'de> for OrderedMapVisitor {
            type Value = OrderedMap;

            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("a JSON object")
            }

            fn visit_map<A: MapAccess<'de>>(self, mut access: A) -> Result<OrderedMap, A::Error> {
                let mut map = OrderedMap::new();
                while let Some((key, value)) = access.next_entry::<String, Value>()? {
                    map.insert(key, value);
                }
                Ok(map)
            }
        }

        deserializer.deserialize_map(OrderedMapVisitor)
    }
}

/// Builds an [`OrderedMap`] from literal pairs, in the order they are written.
#[macro_export]
macro_rules! ordered {
    ($($key:literal : $value:expr),* $(,)?) => {{
        let mut map = $crate::composer::json::OrderedMap::new();
        $(map.insert($key, ::serde_json::json!($value));)*
        map
    }};
}
