//! The runtime's one-way facts channel: every value the Rust sidebar still takes from outside the
//! store.
//!
//! CDXC:Sidebar 2026-09-21 DECISION:
//! User (M4d part 2, question 1 option A): the facts the Rust sidebar still borrows from the
//! deleted page (the sidebar HUD, a project's git numbers, the Close After Done and Delayed Send
//! timers, and a reveal request) arrive on a narrow one-way channel the runtime posts, rather than
//! by porting the HUD and the timers into Rust now. The channel has no diffing and no patches and
//! dies with QuickJS in M8.
//!
//! CDXC:Sidebar 2026-09-21 WHY:
//! The list, its menus and the snapshot it installs READ this channel (`sidebar_list_inputs.rs`,
//! `sidebar_menus.rs`, `sidebar_snapshot.rs`). It was measured against the publish it replaced for
//! two live rounds before that publish was deleted in step 7, at zero differences in every family.
//! `remainingMs` and `remainingLabel` ride along for the reader but decide nothing: the labels a
//! row and a chat's working row draw are formatted from the deadline against the host's clock
//! (gx-core `timer_trailing_label`, `armed_actions_by_session`).
//!
//! CDXC:Sidebar 2026-09-25 WHY:
//! The channel goes one fact at a time as the app runtime port moves each one to Rust, which is how
//! it dies with QuickJS. The HUD left it first: gx_store/hud/ composes it (family F2) and writes
//! `hud` here, where every reader already looked. The remote machines' client-parked projects come
//! on their own `remoteRecentProjects` post, because the runtime still writes them, and the HUD
//! reads them from here.
//!
//! SEE-ALSO: apps/desktop/sidebar/gxserver-runtime/sidebar-runtime-facts.ts,
//! apps/desktop/src/app/gx_store/diagnostics_runtime_facts.rs, apps/desktop/src/app/gx_store/hud/.

use std::collections::HashMap;
use std::sync::Arc;

use ghostex_gx_core::{CloseAfterDoneInput, DelayedSendInput, ProjectDiffStats};
use serde_json::Value;

use crate::GhostexGpuiApp;
use crate::app::helpers::gpui_remote_project_reference_from_project_id;
use crate::app::native_sidebar::model::NativeSidebarRevealRequest;

/// What the channel has delivered.
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
    pub(super) close_after_done: HashMap<String, CloseAfterDoneInput>,
    /// This app's own Delayed Sends of workspace sessions (local_delayed_sends.rs).
    pub(super) delayed_sends: HashMap<String, DelayedSendInput>,
    /// Each remote machine's client-parked projects, newest first, read once at launch
    /// (remote_recent_projects.rs): an input of the HUD.
    pub(super) remote_recent_projects: Vec<(String, Vec<Value>)>,
    /// Bumped when a post really replaced the HUD, and when one replaced the per-row facts. The
    /// two are apart so a rows post, which arrives with every projection the runtime builds, does
    /// not make the list re-read the HUD's Recent Projects.
    pub(super) hud_generation: u64,
    pub(super) rows_generation: u64,
    /// The newest reveal from either side, which is what the installed list carries: the runtime's
    /// on the channel, or this app's own from the titlebar.
    pub(super) newest_reveal: Option<NativeSidebarRevealRequest>,
    pub(super) counters: RuntimeFactsCounters,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct RuntimeFactsCounters {
    pub(super) hud_posts: u64,
    pub(super) row_posts: u64,
    pub(super) reveal_posts: u64,
    pub(super) unparsable: u64,
    /// Reveals that arrived before the list was ready and were held for the replay. Above zero
    /// only in the first instants of a launch, and each one is answered once
    /// (`revealsReplayed` moves with it).
    pub(super) reveals_held: u64,
    pub(super) reveals_replayed: u64,
}

impl SidebarRuntimeFacts {
    /// The HUD the runtime posted, or `None` while it has not posted one.
    pub(super) fn hud(&self) -> Option<&Arc<Value>> {
        self.hud.as_ref()
    }

    pub(super) fn counters(&self) -> RuntimeFactsCounters {
        self.counters
    }
}

impl GhostexGpuiApp {
    /// A channel payload. Every value the list, its menus and its snapshot take from outside the
    /// store arrives here, so a post brings the list up to date the way a publish used to.
    pub(crate) fn receive_sidebar_runtime_facts(
        &mut self,
        payload: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let Ok(value) = serde_json::from_str::<Value>(payload) else {
            self.gx_store.runtime_facts.counters.unparsable += 1;
            return;
        };
        // The runtime's own focus paths (focusSession, Split Right) acknowledge through the store's
        // one attention tracker while the runtime still owns focus (gx_store/attention/).
        if value.get("kind").and_then(Value::as_str) == Some("attentionAcknowledge") {
            match value
                .get("sessionId")
                .and_then(Value::as_str)
                .and_then(ghostex_gx_core::SessionKey::parse_sidebar_session_id)
            {
                Some(session) => self.gx_store_acknowledge_attention(session, cx),
                None => self.gx_store.runtime_facts.counters.unparsable += 1,
            }
            return;
        }
        let mut reveal: Option<NativeSidebarRevealRequest> = None;
        let facts = &mut self.gx_store.runtime_facts;
        match value.get("kind").and_then(Value::as_str) {
            Some("rows") => {
                // A project's git numbers are the Rust poll's since the runtime port's F5
                // (gx_store/git/poll.rs), which writes `project_diff_stats` itself.
                facts.close_after_done = value
                    .get("closeAfterDone")
                    .and_then(Value::as_object)
                    .map(|entries| {
                        entries
                            .iter()
                            .map(|(id, entry)| (id.clone(), close_after_done(entry)))
                            .collect()
                    })
                    .unwrap_or_default();
                facts.rows_generation += 1;
                facts.counters.row_posts += 1;
            }
            Some("reveal") => {
                let session_id = value.get("sessionId").and_then(Value::as_str);
                let request_id = value.get("requestId").and_then(Value::as_u64);
                if let (Some(session_id), Some(request_id)) = (session_id, request_id) {
                    facts.newest_reveal = Some(NativeSidebarRevealRequest {
                        session_id: session_id.to_string(),
                        request_id,
                    });
                    reveal = facts.newest_reveal.clone();
                    facts.counters.reveal_posts += 1;
                } else {
                    facts.counters.unparsable += 1;
                }
            }
            _ => {
                facts.counters.unparsable += 1;
                return;
            }
        }
        // The sidebar's own state answers a reveal, and it used to learn of one only on the publish
        // that carried it. Taking the request id here and again in the receiver is harmless: the
        // second call finds it already taken and returns (`take_reveal_request`). A reveal that
        // arrives before the list is ready is HELD rather than answered, and
        // `gx_store_update_sidebar_list` replays it (`gx_store_replay_held_sidebar_reveal`).
        if let Some(reveal) = reveal {
            self.gx_store_note_sidebar_reveal(&reveal.session_id, reveal.request_id, cx);
        }
        // Everything the list still borrows moved with this post, so the list is brought up to
        // date now rather than at the next thing that happens to move the store.
        self.gx_store_sidebar_state_changed(cx);
    }

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
    /// the runtime never sees, so the installed list carries it.
    pub(crate) fn gx_store_note_local_sidebar_reveal(&mut self, session_id: &str, request_id: u64) {
        self.gx_store.runtime_facts.newest_reveal = Some(NativeSidebarRevealRequest {
            session_id: session_id.to_string(),
            request_id,
        });
    }
}

fn close_after_done(entry: &Value) -> CloseAfterDoneInput {
    CloseAfterDoneInput {
        armed: entry.get("armed").and_then(Value::as_bool) == Some(true),
        deadline_at: text(entry, "deadlineAt"),
        remaining_label: text(entry, "remainingLabel"),
        remaining_ms: entry.get("remainingMs").and_then(Value::as_i64),
    }
}

fn text(entry: &Value, key: &str) -> Option<String> {
    entry.get(key).and_then(Value::as_str).map(str::to_string)
}
