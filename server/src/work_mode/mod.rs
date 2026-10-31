//! Issues Workflow (formerly Work mode; the code keeps the old name): starting work from a ticket,
//! a session's hand-set links (Link to), the Issues view's tickets, branch titles and the Clean up
//! offer when a linked PR merges. Split by concern; every submodule is re-exported here.
//!
//! CDXC:WorkMode 2026-10-09 DECISION:
//! User: outside a Work workspace work mode needs no Slack and no Convex: it is just Linear (an API key) and GitHub (`gh`), and Linear already ties its issues to their GitHub PRs. A session links to one PR, one Linear project, and its Linear or GitHub issues; it is linked automatically from its branch name and by hand with `ghostex link-session`. (Its per-project switch is superseded by CDXC:WorkMode 2026-10-24 in project.rs.)
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
/// Nothing calls it outside tests until the command source is registered as a `WorkSource`.
#[cfg_attr(not(test), allow(dead_code, unused_imports))]
mod command;
mod credentials;
mod details_share;
mod feeds;
mod github;
mod github_batch;
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
mod settings;
mod source;
mod source_github;
mod source_linear;
mod start_pull_request;
mod start_work;
mod team_flow;
#[cfg(test)]
mod tests;
mod tracker;

pub(crate) use branch::*;
pub(crate) use candidates::*;
pub(crate) use cleanup::*;
pub(crate) use cloud_work::*;
#[cfg_attr(not(test), allow(unused_imports))]
pub(crate) use command::*;
pub(crate) use credentials::*;
pub(crate) use details_share::*;
pub(crate) use feeds::*;
pub(crate) use github::*;
pub(crate) use github_batch::*;
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
pub(crate) use settings::*;
pub(crate) use source::*;
pub(crate) use source_github::*;
pub(crate) use source_linear::*;
pub(crate) use start_pull_request::*;
pub(crate) use start_work::*;
pub(crate) use team_flow::*;
pub(crate) use tracker::*;
