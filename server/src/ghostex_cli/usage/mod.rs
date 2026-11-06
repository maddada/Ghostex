//! `ghostex` help text, split by concern from the former `ghostex_cli/usage.rs`: the top-level
//! catalog in `catalog.rs`, each command's focused help in `focused.rs`. Every submodule is glob
//! re-exported here, so callers keep using `usage::…` unchanged.

mod catalog;
mod focused;
#[cfg(test)]
mod tests;

pub use catalog::*;
pub use focused::*;

// Moved bodies reach sibling modules of the parent through `super::<module>::…` paths; this
// import keeps that path resolving from inside this directory.
use super::connectors;
