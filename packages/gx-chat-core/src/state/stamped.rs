//! A state field that knows when it may have changed.

use std::sync::atomic::{AtomicU64, Ordering};

/// The last stamp handed out, shared by every field of every chat, so two values ever held under
/// the same stamp are the same value.
static LAST_STAMP: AtomicU64 = AtomicU64::new(0);

fn fresh_stamp() -> u64 {
    LAST_STAMP.fetch_add(1, Ordering::Relaxed) + 1
}

/// A value with a stamp that moves on every mutable access to it.
///
/// CDXC:SessionChat 2026-11-10 WHY: the document's kept parts (`document::AssembleCache`) decided whether to reuse their result by cloning and deep-comparing their inputs on every event: the questions state alone was compared six times per event (forms, board and questions list, in the probe and again in the document), about a quarter of the document assembly (itself two thirds of the chat host thread) in the live profile with nine agents working. A stamp equal to the one a part was built at proves the input unchanged in one integer compare. The stamp moves on ANY `&mut` (through `DerefMut`), so no writer can forget to bump it, and a value built anew takes a fresh stamp; a moved stamp only sends the part to the deep compare it always ran, so a write that left the value equal still keeps the part. Writers that run on every event write only when the value differs, or the stamp would move every time.
#[derive(Clone, Debug)]
pub struct Stamped<T> {
    value: T,
    stamp: u64,
}

impl<T> Stamped<T> {
    pub fn new(value: T) -> Self {
        Self {
            value,
            stamp: fresh_stamp(),
        }
    }

    /// The stamp of the value held now: equal stamps mean an equal value.
    pub fn stamp(&self) -> u64 {
        self.stamp
    }
}

impl<T: PartialEq> Stamped<T> {
    /// Whether this kept copy holds what `current` holds: at once when their stamps agree, else by
    /// comparing the values, after which the copy takes `current`'s stamp so the next check is
    /// quick again.
    pub fn same_as(&mut self, current: &Self) -> bool {
        if self.stamp == current.stamp {
            return true;
        }
        if self.value != current.value {
            return false;
        }
        self.stamp = current.stamp;
        true
    }

    /// Takes `value` in place of an equal value, keeping the stamp (the value did not change), so
    /// two holders of equal values can come to share one allocation; does nothing otherwise.
    pub fn share_equal(&mut self, value: T) {
        if self.value == value {
            self.value = value;
        }
    }

    /// Replaces the value only when `value` differs, so an unchanged write keeps the stamp.
    pub fn set_if_changed(&mut self, value: T) {
        if self.value != value {
            *self = Self::new(value);
        }
    }
}

impl<T: Default> Default for Stamped<T> {
    fn default() -> Self {
        Self::new(T::default())
    }
}

impl<T> From<T> for Stamped<T> {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}

impl<T> std::ops::Deref for Stamped<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.value
    }
}

impl<T> std::ops::DerefMut for Stamped<T> {
    fn deref_mut(&mut self) -> &mut T {
        self.stamp = fresh_stamp();
        &mut self.value
    }
}

/// Equal when the values are, whatever the stamps say.
impl<T: PartialEq> PartialEq for Stamped<T> {
    fn eq(&self, other: &Self) -> bool {
        (self.stamp == other.stamp) || self.value == other.value
    }
}

impl<T: PartialEq> PartialEq<T> for Stamped<T> {
    fn eq(&self, other: &T) -> bool {
        self.value == *other
    }
}

impl<T: Eq> Eq for Stamped<T> {}

impl<'a, T> IntoIterator for &'a Stamped<T>
where
    &'a T: IntoIterator,
{
    type Item = <&'a T as IntoIterator>::Item;
    type IntoIter = <&'a T as IntoIterator>::IntoIter;

    fn into_iter(self) -> Self::IntoIter {
        (&self.value).into_iter()
    }
}
