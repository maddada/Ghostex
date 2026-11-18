//! Agent hook install, repair, inspection and uninstall, split by concern from the
//! former `agent_hooks/install.rs`. Every submodule is glob re-exported here, so callers
//! keep using `super::install::…` / `crate::agent_hooks::install::…` unchanged.
mod hermes;
mod inspect;
mod install_hook;
mod json_merge;
mod marked_yaml;
mod notify_hook;
mod opencode;
mod uninstall;

use hermes::*;
pub(crate) use inspect::*;
pub(crate) use install_hook::*;
use json_merge::*;
use marked_yaml::*;
pub(crate) use notify_hook::*;
pub(crate) use opencode::*;
pub(crate) use uninstall::*;

// The moved bodies reach sibling agent_hooks modules through `super::<module>::…`
// paths; these imports keep those paths resolving from inside `install/`.
use super::{claude_retention, codex_trust, zcode};
