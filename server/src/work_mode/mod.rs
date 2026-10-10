//! Per-project work mode: a project's sessions are linked to a GitHub PR, GitHub issues, Linear
//! issues and a Linear project, and every client shows those links on the session's card.
//! Split by concern; every submodule is re-exported here.
//!
//! CDXC:WorkMode 2026-10-09 DECISION:
//! User: work mode is a per-project switch (right-click the project → Work mode). Outside a Work
//! workspace it needs no Slack and no Convex: it is just Linear (an API key) and GitHub (`gh`),
//! and Linear already ties its issues to their GitHub PRs. A session links to one PR, one Linear
//! project, and its Linear or GitHub issues; it is linked automatically from its branch name and
//! by hand with `ghostex link-session`.
//!
//! CDXC:WorkMode 2026-10-09 WHY:
//! Everything slow (Linear's API, `gh`) runs in the background pass (`refresh.rs`) and lands in
//! in-memory caches; the presentation projection only reads them, so building a snapshot never
//! waits on the network.
//!
//! SEE-ALSO: `PresentationSessionWork` / `PresentationProject.work_mode` in
//! packages/gx-protocol/src/presentation.rs and their TypeScript mirror in
//! packages/shared/gxserver-protocol-presentation.ts; the sidebar cards in
//! apps/desktop/src/app/native_sidebar/.

mod branch;
mod candidates;
mod cleanup;
mod cloud_work;
mod credentials;
mod feeds;
mod github;
mod github_projects;
mod github_tickets;
mod item_details;
mod items;
mod linear;
mod linear_tickets;
mod link_add;
mod links;
mod notices;
mod presentation;
mod project;
mod pull_request_tickets;
mod refresh;
mod start_pull_request;
mod start_work;
mod team_flow;
mod tracker;

pub(crate) use branch::*;
pub(crate) use candidates::*;
pub(crate) use cleanup::*;
pub(crate) use cloud_work::*;
pub(crate) use credentials::*;
pub(crate) use feeds::*;
pub(crate) use github::*;
pub(crate) use github_projects::*;
pub(crate) use github_tickets::*;
pub(crate) use item_details::*;
pub(crate) use items::*;
pub(crate) use linear::*;
pub(crate) use linear_tickets::*;
pub(crate) use link_add::*;
pub(crate) use links::*;
pub(crate) use notices::*;
pub(crate) use presentation::*;
pub(crate) use project::*;
pub(crate) use pull_request_tickets::*;
pub(crate) use refresh::*;
pub(crate) use start_pull_request::*;
pub(crate) use start_work::*;
pub(crate) use team_flow::*;
pub(crate) use tracker::*;
