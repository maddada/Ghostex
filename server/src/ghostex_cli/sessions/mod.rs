//! `ghostex sessions` and the gxserver session inventory, split by concern from the
//! former `ghostex_cli/sessions.rs`. Every submodule is glob re-exported here, so callers
//! keep using `sessions::…` unchanged. The module's unit tests stay inline at the end of
//! `human_list.rs`.

/*
CDXC:Cli 2026-07-13:
Faithful port of the Node CLI's gxserver session inventory surface:
sessionsCommand (--json / --mobile-summary / grouped human list),
fetchGxserverSessionList with the persisted-state SQLite fallback, toCliSession,
the mobile summary compactors consumed by React Native Android, run-action, and the
attach-metadata helpers. JSON field names and console strings match the Node
CLI byte-for-byte; serde_json's alphabetical key order is the one accepted
difference.
*/

mod attach_metadata;
mod cli_session;
mod command;
mod human_list;
mod inventory;
mod js_values;
mod mobile_summary;
mod persisted;
mod summary_memo;

pub use attach_metadata::*;
use cli_session::*;
pub use command::*;
use human_list::*;
pub use inventory::*;
pub(crate) use js_values::*;
pub use mobile_summary::*;
use persisted::*;
pub(crate) use summary_memo::note_gxserver_request;
use summary_memo::*;

// Moved bodies reach sibling modules of the parent through `super::<module>::…`
// paths; these imports keep those paths resolving from inside this directory.
use super::selector;
