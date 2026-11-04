//! `ghostex` CLI bridge actions: the action -> gxserver endpoint switch, session
//! creation helpers, and the `parse*` payload parsers, split by concern from the former
//! `ghostex_cli/actions.rs`. Every submodule is glob re-exported here, so callers keep
//! using `actions::…` unchanged. The payload-parser tests stay inline at the end of
//! `js_coercion.rs`.

/*
CDXC:Cli 2026-07-13:
Faithful port of the Node CLI's bridgeAction/resolvedSessionBridgeAction
wrappers, sendGxserverCliAction (the action → gxserver endpoint switch), the
renderer-command dispatch helpers, and every parse* payload parser. Payload
field names and undefined-vs-null semantics match the JS exactly: JS
`undefined` is represented as an absent key, JS `null` as Value::Null, and the
JS compactObject (which filters only `undefined`) therefore keeps explicit
nulls such as `sessionTag: null`.
*/

mod bridge;
mod create;
mod dispatch;
mod js_coercion;
mod parse_chat;
mod parse_sessions;
mod parse_views;
mod parse_workspace;

pub use bridge::*;
use create::*;
pub use dispatch::*;
pub(crate) use js_coercion::resolve_cli_cwd;
use js_coercion::*;
use parse_chat::*;
use parse_sessions::*;
use parse_views::*;
use parse_workspace::*;

// Moved bodies reach sibling modules of the parent through `super::<module>::…`
// paths; these imports keep those paths resolving from inside this directory.
use super::{session_chat_model, session_parking};
