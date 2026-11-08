//! Session links: the GitHub PRs, GitHub issues and Linear tickets one session is linked to, on
//! every session (not only in work mode), with their state, checks and review, and the PR and
//! issue URLs its chat mentioned, offered as suggestions. Split by concern; every submodule is
//! re-exported here.
//!
//! SEE-ALSO: server/src/server/session_links_sync.rs (the background pass),
//! server/src/server/route_http/session_links.rs (`/api/addSessionLink`, `/api/removeSessionLink`,
//! `/api/readSessionLinks`), `PresentationSessionLinks` in packages/gx-protocol/src/presentation.rs,
//! packages/gx-core/src/sidebar_view/links.rs, apps/desktop/src/app/render/workarea_header/session_links.rs,
//! apps/mobile/app/src/chat/native/SessionLinksStrip.tsx.

mod presentation;
mod reference;
mod store;
mod transcript_scan;

pub(crate) use presentation::*;
pub(crate) use reference::*;
pub(crate) use store::*;
pub(crate) use transcript_scan::*;
