//! A wire value several holders share instead of copying.

use std::sync::Arc;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// A value held behind an `Arc`, written and read exactly as the value itself.
///
/// CDXC:SessionChat 2026-11-10 WHY: a coordinator chat's side data (`resourceIndex` 275 KB of JSON and 2 MB parsed, `coordinatorThreads` 169 KB and 1.1 MB, the `draft`) was copied whole by every holder in the chat core (the frame, the retained fold, the session state, the board's kept inputs) and again by the fold on every state frame. Shared, a frame's value is parsed once and every holder keeps the same allocation; the bytes on the wire and in saved records are the value's own.
#[derive(Debug, Default)]
pub struct Shared<T>(pub Arc<T>);

impl<T> Shared<T> {
    pub fn new(value: T) -> Self {
        Self(Arc::new(value))
    }

    /// Whether both hold the same allocation.
    pub fn same(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}

impl<T> Clone for Shared<T> {
    fn clone(&self) -> Self {
        Self(Arc::clone(&self.0))
    }
}

impl<T> std::ops::Deref for Shared<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T> From<T> for Shared<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

/// Equal when the values are; the same allocation is equal without a look.
impl<T: PartialEq> PartialEq for Shared<T> {
    fn eq(&self, other: &Self) -> bool {
        self.same(other) || *self.0 == *other.0
    }
}

impl<T: Eq> Eq for Shared<T> {}

impl<T: Serialize> Serialize for Shared<T> {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        (*self.0).serialize(serializer)
    }
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Shared<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        T::deserialize(deserializer).map(Self::new)
    }
}
