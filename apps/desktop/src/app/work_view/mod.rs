//! The Work view: a first-party web page (`work.html`, React + shadcn, built by
//! apps/desktop/vite.config.ts) hosted in the view panel like the Files embed page. It lists the
//! tickets, issues and PRs of every work-mode project the window shows, opens a ticket's details,
//! and starts or opens the chat on one.
//!
//! CDXC:WorkMode 2026-10-09 DECISION:
//! User: the Work list is "a web page (React + shadcn)" shipped inside the app, not an extension,
//! styled like the native Kanban/Automate views. A briefcase button at the top of the sidebar opens
//! it as a tab in the side panel, and it is listed in "Open a view". It shows every work-mode
//! project the window shows, Personal work-mode projects too. "Open chat" when a session already
//! links the ticket, otherwise "Start chat", which starts a session (new worktree on the ticket's
//! branch, linked) and sends nothing.
//!
//! SEE-ALSO: apps/desktop/views/work/ (the page), apps/desktop/sidebar/work-main.tsx (its CEF entry),
//! server/src/work_mode/items.rs and item_details.rs (`/api/listWorkItems`, `/api/readWorkItem`),
//! server/src/work_mode/team_flow.rs (`/api/readTeamFlow`), server/src/work_mode/start_work.rs
//! (`/api/startWorkOnTicket`), apps/gpui-web/src/app/web_host/work_view.rs (the browser's answer).

mod bridge;
mod current_session;
mod host;

pub(crate) use host::WorkViewState;
