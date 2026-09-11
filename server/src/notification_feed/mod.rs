/*
CDXC:Notifications 2026-09-11 DECISION:
User: "copy a notifications system into Ghostex, put the bell to the right of the Next button" and, on the recommended design, "ok let's do it": gxserver owns a persisted notification feed (one live row per session, superseded when the session rings again, read state shared by every client), the desktop titlebar shows a bell with an unread count, and the panel body is the agent's last message so the row says what happened.
This supersedes the earlier rule that attention notification content is never persisted: the macOS banner path still keeps nothing, but the feed table here is the durable list the bell, the web app, and mobile read.
SEE-ALSO: packages/shared/notification-feed/notification-feed-contract.ts (wire contract), server/src/server/agent_http.rs (the transition observer call), apps/desktop/src/notification_feed (the titlebar bell and panel).
*/

pub(crate) mod api;
pub(crate) mod body;
pub(crate) mod producer;
pub(crate) mod store;

pub(crate) use api::*;
pub(crate) use producer::*;
