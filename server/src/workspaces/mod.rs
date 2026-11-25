//! Workspaces: each one is a company (or "Personal") with its own projects, Spaces, Linear key,
//! Claude account and browser sign-ins. Split by concern; every submodule is re-exported here.
//!
//! CDXC:Workspaces 2026-10-09 DECISION:
//! User: one workspace tile sits to the left of the Spaces row (not a rail); its menu switches
//! workspaces, opens one in a new window, opens workspace settings and creates a workspace. Each
//! workspace has its own Spaces, projects, browser sign-ins, Claude account and connections, and a
//! window shows one workspace at a time. Existing installs start with one workspace, "Personal",
//! holding every project and Space.
//!
//! CDXC:Workspaces 2026-10-09 WHY:
//! A project or Space with no `workspaceId` belongs to the default workspace, so the first run
//! needs no migration and an older client that rewrites a Space without the field cannot move it
//! (`sidebar_spaces.rs` carries the stored id across such a write). The project's id lives in
//! `launchSettings.workspaceId` beside `workMode`, because the presentation projection of a
//! project reads only the project row.
//!
//! SEE-ALSO: `SidebarWorkspacesState` and `PresentationProject.workspace_id` in
//! packages/gx-protocol (TypeScript mirror in packages/shared/gxserver-protocol-presentation.ts),
//! the sidebar filter in packages/gx-core/src/sidebar_view/, and the routes in
//! server/src/server/route_http/workspaces.rs.

mod feature;
mod instructions;
mod projects;
mod store;

pub(crate) use feature::*;
pub(crate) use instructions::*;
pub(crate) use projects::*;
pub use store::*;
