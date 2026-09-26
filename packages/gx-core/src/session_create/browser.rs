//! The two browser opens the sidebar asks for: a project header's New Browser Tab
//! (`openBrowserPaneInGroup`) and the Quick Browser Tab (`openBrowserChat`).
//!
//! CDXC:Browser 2026-09-25 WHY:
//! Both used to leave Rust for the app runtime, which resolved the group and posted
//! `postOpenBrowserUrl` or the remote ports action straight back to Rust. The resolution is this
//! file; the host performs the open with the same payload the runtime posted, so the reviewed
//! browser door (`open_browser_url_from_renderer_command`) and the remote ports page stay the only
//! places a tab is made.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/create/browser.rs.

use serde_json::{json, Value};

use crate::keys::ProjectKey;
use crate::sidebar_actions::{
    NATIVE_PROJECT_PATH_ACTION_MESSAGE_TYPE, NATIVE_PROJECT_PATH_ACTION_MESSAGE_VERSION,
};

use super::target::group_project;

/// `DEFAULT_BROWSER_LAUNCH_URL` in packages/shared/sidebar-commands.ts.
pub const DEFAULT_BROWSER_LAUNCH_URL: &str = "https://www.google.com";

/// What a project header's New Browser Tab does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BrowserPaneOpen {
    /// A project on this computer: make its group the active one, then open the launch URL in
    /// that project's browser tabs with no reuse.
    ///
    /// CDXC:Browser 2026-07-12:
    /// Browser tabs are project-keyed local CEF panes, so remote projects reuse the same workarea
    /// through their machine-scoped project ids. The open carries the explicit target project id
    /// so the host swaps the browser project model before creating the tab instead of racing the
    /// async active-project context round-trip.
    Local {
        project_id: String,
        group_id: String,
    },
    /// A project on a remote machine: make its group the active one, then open the machine's
    /// listening-ports page.
    ///
    /// CDXC:RemoteMachines 2026-07-30:
    /// A remote project's Browser pane defaults to the machine's listening-ports page instead of
    /// the generic launch URL, so the tab lands on the remote's address with its running apps one
    /// click away. Rust owns SSH port discovery, page generation, and the final tab URL; the
    /// request carries only the fixed action plus the machine-scoped project id.
    Remote {
        project: ProjectKey,
        group_id: String,
    },
}

impl BrowserPaneOpen {
    /// The `postNativeProjectPathAction` payload the remote branch posted, built the same way:
    /// the machine-scoped project id and the fixed action name, nothing else.
    pub fn remote_native_payload(&self) -> Option<Value> {
        match self {
            Self::Local { .. } => None,
            Self::Remote { project, .. } => Some(json!({
                "action": "openRemoteProjectPortsBrowser",
                "projectId": project.to_workspace_project_id(),
                "type": NATIVE_PROJECT_PATH_ACTION_MESSAGE_TYPE,
                "version": NATIVE_PROJECT_PATH_ACTION_MESSAGE_VERSION,
            })),
        }
    }

    /// The group the open makes active.
    pub fn group_id(&self) -> &str {
        match self {
            Self::Local { group_id, .. } | Self::Remote { group_id, .. } => group_id,
        }
    }
}

/// `openBrowserPaneInGroup(groupId = this.activeGroupId)`. `None` is the TypeScript's early return:
/// no group, or a group that names no project.
pub fn plan_browser_pane_open(group_id: Option<&str>) -> Option<BrowserPaneOpen> {
    let group_id = group_id.filter(|group_id| !group_id.is_empty())?;
    let project = group_project(group_id)?;
    Some(match project.machine.is_local() {
        true => BrowserPaneOpen::Local {
            project_id: project.project_id,
            group_id: group_id.to_string(),
        },
        false => BrowserPaneOpen::Remote {
            project,
            group_id: group_id.to_string(),
        },
    })
}
