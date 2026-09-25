//! The facts the sidebar list, its menus and its snapshot read from outside the store: the HUD
//! (gx_store/hud/), a project's git numbers (gx_store/git/poll.rs), this app's own Delayed Sends
//! (local_delayed_sends.rs), the remote machines' client-parked projects
//! (remote_recent_projects.rs) and the newest reveal request.
//!
//! CDXC:Sidebar 2026-09-25 WHY:
//! These used to arrive on the old QuickJS runtime's one-way facts channel (the 2026-09-21
//! decision said the channel "dies with QuickJS"). Each fact moved to a Rust writer during the app
//! runtime port and the channel went with QuickJS; the holder keeps its name because every reader
//! already looks here.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/diagnostics_runtime_facts.rs, apps/desktop/src/app/gx_store/hud/.

use std::collections::HashMap;
use std::sync::Arc;

use ghostex_gx_core::{DelayedSendInput, ProjectDiffStats};
use serde_json::Value;

use crate::GhostexGpuiApp;
use crate::app::helpers::gpui_remote_project_reference_from_project_id;
use crate::app::native_sidebar::model::NativeSidebarRevealRequest;

/// The facts, as their writers last left them.
#[derive(Default)]
pub(crate) struct SidebarRuntimeFacts {
    /// The sidebar HUD, composed by gx_store/hud/ (it used to be the runtime's post).
    ///
    /// CDXC:Sidebar 2026-09-21 WHY:
    /// Behind an `Arc` because every list install used to deep-clone it three times (the install
    /// itself, the menus and the snapshot the renderer draws), and since step 3 an install happens
    /// on every focus move: holding "next tab" copied a few hundred kilobytes of HUD per keystroke.
    /// Nothing mutates it, so the three copies were three readers of one immutable document.
    pub(super) hud: Option<Arc<Value>>,
    pub(super) project_diff_stats: HashMap<String, ProjectDiffStats>,
    /// This app's own Delayed Sends of workspace sessions (local_delayed_sends.rs).
    pub(super) delayed_sends: HashMap<String, DelayedSendInput>,
    /// Each remote machine's client-parked projects, newest first, read once at launch
    /// (remote_recent_projects.rs): an input of the HUD.
    pub(super) remote_recent_projects: Vec<(String, Vec<Value>)>,
    /// Bumped when a post really replaced the HUD, and when one replaced the per-row facts. The
    /// two are apart so a rows post (which arrived with every projection the old runtime built) does
    /// not make the list re-read the HUD's Recent Projects.
    pub(super) hud_generation: u64,
    pub(super) rows_generation: u64,
    /// The newest reveal, which is what the installed list carries.
    pub(super) newest_reveal: Option<NativeSidebarRevealRequest>,
    pub(super) counters: RuntimeFactsCounters,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RuntimeFactsCounters {
    pub(super) hud_posts: u64,
    /// Reveals that arrived before the list was ready and were held for the replay. Above zero
    /// only in the first instants of a launch, and each one is answered once
    /// (`revealsReplayed` moves with it).
    pub(super) reveals_held: u64,
    pub(super) reveals_replayed: u64,
}

impl SidebarRuntimeFacts {
    /// The HUD, or `None` before the store's first composition.
    pub(super) fn hud(&self) -> Option<&Arc<Value>> {
        self.hud.as_ref()
    }

    pub(super) fn counters(&self) -> RuntimeFactsCounters {
        self.counters
    }
}

impl GhostexGpuiApp {
    /// CDXC:AgentLauncher 2026-09-23 WHY:
    /// Settings shares the sidebar's machine-scoped project Actions; a local HUD read cannot resolve a remote project and supplies unrelated defaults. Global Actions still come from the local Settings hydrate.
    pub(crate) fn with_remote_project_action_rows(&self, mut message: Value) -> Value {
        if let Some(project) = self
            .gpui_app_modal_active_project_id()
            .filter(|id| gpui_remote_project_reference_from_project_id(id).is_some())
        {
            message["hud"]["commands"] = self
                .gx_store
                .runtime_facts
                .hud
                .as_deref()
                .and_then(|hud| hud.get("commandsByProject"))
                .and_then(|rows| rows.get(&project))
                .cloned()
                .unwrap_or_else(|| Value::Array(Vec::new()));
        }
        message
    }

    pub(crate) fn gpui_action_scope_for_command(
        &self,
        command_id: &str,
    ) -> Option<crate::app::helpers::GpuiSidebarCommandScope> {
        use crate::app::helpers::GpuiSidebarCommandScope;
        let hud = self.gx_store.runtime_facts.hud.as_deref()?;
        let contains = |rows: Option<&Value>| {
            rows.and_then(Value::as_array).is_some_and(|rows| {
                rows.iter()
                    .any(|row| row["commandId"].as_str() == Some(command_id))
            })
        };
        let project = self.gpui_app_modal_active_project_id()?;
        let project_match = contains(
            hud.get("commandsByProject")
                .and_then(|rows| rows.get(&project)),
        );
        let global_match = contains(hud.get("globalCommands"));
        match (project_match, global_match) {
            (true, false) => Some(GpuiSidebarCommandScope::Project),
            (false, true) => Some(GpuiSidebarCommandScope::Global),
            _ => None,
        }
    }

    /// Remembers a reveal this app asked for itself (the titlebar's Reveal Active Session), which
    /// no publish carries back, so the installed list carries it.
    pub(crate) fn gx_store_note_local_sidebar_reveal(&mut self, session_id: &str, request_id: u64) {
        self.gx_store.runtime_facts.newest_reveal = Some(NativeSidebarRevealRequest {
            session_id: session_id.to_string(),
            request_id,
        });
    }
}
