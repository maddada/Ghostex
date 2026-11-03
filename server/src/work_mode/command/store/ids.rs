//! A ticket command's id as `WorkSourceId::Command` carries it: interned, so `WorkSourceId` stays
//! `Copy` and `as_wire` stays `&'static str`. Only the store interns (`intern` is private to
//! it): when it loads the record at startup and before it writes a record that names an id it
//! never held. Everything else, requests, links and typed references included, can only look up
//! an id the store already interned (`known`), so outside input never grows the set.

use std::collections::BTreeMap;
use std::sync::{Mutex, OnceLock};

use super::*;

/// One leaked `CommandSource` per distinct id this process has interned. Compares, orders and
/// hashes by the id's text.
#[derive(Clone, Copy)]
pub(crate) struct CommandSourceId(&'static CommandSource);

impl CommandSourceId {
    /// An id the store interned; never interns.
    pub(crate) fn known(id: &str) -> Option<Self> {
        registry().known(id)
    }

    pub(crate) fn as_str(self) -> &'static str {
        self.0.id()
    }

    pub(crate) fn source(self) -> &'static CommandSource {
        self.0
    }
}

impl PartialEq for CommandSourceId {
    fn eq(&self, other: &Self) -> bool {
        self.as_str() == other.as_str()
    }
}

impl Eq for CommandSourceId {}

impl std::hash::Hash for CommandSourceId {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_str().hash(state);
    }
}

impl PartialOrd for CommandSourceId {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for CommandSourceId {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.as_str().cmp(other.as_str())
    }
}

impl std::fmt::Debug for CommandSourceId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_tuple("CommandSourceId")
            .field(&self.as_str())
            .finish()
    }
}

/// Why a record naming an id this process never held can't be written: the process already
/// interned `MAX_SOURCE_IDS` ids. The record refuses a 257th id of its own before this (4a's set);
/// this is reached only by ids a process held and the record has since pruned, which stay
/// interned until gxserver restarts.
pub(crate) const TOO_MANY_SOURCE_IDS: &str =
    "Ghostex has seen too many ticket source ids since it started; restart it and try again.";

/// The interned ids, at most `capacity` of them.
pub(crate) struct SourceIds {
    capacity: usize,
    ids: Mutex<BTreeMap<String, &'static CommandSource>>,
}

impl SourceIds {
    pub(crate) fn new(capacity: usize) -> Self {
        Self {
            capacity,
            ids: Mutex::new(BTreeMap::new()),
        }
    }

    pub(crate) fn known(&self, id: &str) -> Option<CommandSourceId> {
        let ids = self
            .ids
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        ids.get(id).copied().map(CommandSourceId)
    }

    /// Interns every id of `ids` this process doesn't hold yet, all or none: `Err` when that
    /// would pass the capacity, before any is interned.
    pub(crate) fn intern_all<'a>(
        &self,
        ids: impl IntoIterator<Item = &'a str>,
    ) -> Result<(), &'static str> {
        let mut interned = self
            .ids
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut new: Vec<&str> = ids
            .into_iter()
            .filter(|id| !interned.contains_key(*id))
            .collect();
        new.sort_unstable();
        new.dedup();
        if interned.len() + new.len() > self.capacity {
            return Err(TOO_MANY_SOURCE_IDS);
        }
        for id in new {
            let source: &'static CommandSource = Box::leak(Box::new(CommandSource::new(
                Box::leak(id.to_string().into_boxed_str()),
            )));
            interned.insert(id.to_string(), source);
        }
        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.ids
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .len()
    }
}

/// This process's interned ids.
pub(super) fn registry() -> &'static SourceIds {
    static IDS: OnceLock<SourceIds> = OnceLock::new();
    IDS.get_or_init(|| SourceIds::new(MAX_SOURCE_IDS))
}
