//! Creating and opening things from the sidebar and its hosts, performed in Rust instead of the
//! old app runtime (the F4 family of docs/2026-09-25/app-runtime-port/PLAN.md).
//!
//! `claim` holds the three doors these commands arrive through (the sidebar dispatch, the host
//! message allowlist and the app modal host's `sidebarCommand`) and the counters; `browser` holds
//! the two browser opens and the Find Prompts shortcut, which only ever looped back into Rust;
//! `projects` a project's Remove and Close; `board` and `board_links_host` the Project Board's
//! conversation requests.

mod agent;
mod board;
mod board_links_host;
mod browser;
mod claim;
mod focus_created;
mod folder_pick;
mod os_integration;
mod projects;
mod terminal;

pub(crate) use claim::CreateHost;
