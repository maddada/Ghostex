// Directory split of server/src/presentation.rs (~3.8k lines), pure move, no
// logic changes. Each submodule is glob-re-exported here so every existing
// crate::presentation::* call site (repository_clone.rs, project_git_remote.rs,
// session_lifecycle.rs, delayed_sends.rs, board_start_work.rs,
// session_chat_queue_runtime.rs, project_icon.rs, agents::*, server::mod, ...)
// keeps resolving without per-call-site qualification. If two submodules ever
// define the same name, drop the glob for one of them here and qualify its
// call sites instead.
pub mod fork_branches;
pub mod fork_family;
pub mod payload_inserts;
pub mod search;
pub mod session_attributes;
pub mod session_projection;
pub mod snapshot;
#[cfg(test)]
mod tests;
pub mod title_normalization;
pub mod util;

pub(crate) use fork_branches::*;
pub(crate) use fork_family::*;
pub(crate) use payload_inserts::*;
pub(crate) use search::*;
pub(crate) use session_attributes::*;
pub(crate) use session_projection::*;
pub(crate) use snapshot::*;
pub(crate) use title_normalization::*;
pub(crate) use util::*;
