//! Owns the core and the sidebar view model, and turns the daemon's frames into the list the shared renderer draws. The desktop's `host.rs` + `sidebar_list.rs` do the same with a native client thread and SQLite-backed sidebar state; neither exists here, so the inputs this host cannot read yet stay at their defaults. Like the desktop, the page runs no QuickJS: the facts in `runtime_facts.rs` come from Rust writers, and the chat beside this store is the desktop's own Rust chat host on `gx-chat-core` (`app/gx_chat/`).
//!
//! CDXC:WebGpui 2026-09-25 DECISION:
//! User (app runtime port, question 2, answered 2A): new code is written web-ready and step 4 wires it into the web build right after. So the desktop's store executor files are symlinked into this folder rather than rewritten for the page, and this host gives them the desktop's field and method names (`sidebar_list`, `runtime_facts`, `diagnostics`, the counters); what they hand to the app is answered in `app/web_host/`, and what a page cannot do answers there with a toast rather than a stand-in that pretends.
use std::sync::Arc;

use futures::StreamExt as _;
use futures::channel::mpsc;
use ghostex_gx_core::protocol::ClientMessage;
use ghostex_gx_core::{ConnectionUpdate, Core, Event, MachineId, MenuHost, SessionKey, SidebarUiStore};
use serde_json::Value;

use super::web_transport::{self, GxserverEndpoint, StreamEvent};
use crate::GhostexGpuiApp;

#[derive(Default)]
pub(crate) struct GxStoreHost {
    pub(crate) core: Core,
    pub(crate) endpoint: Option<GxserverEndpoint>,
    /// What the page shows while there is no list: the connection's last failure.
    pub(crate) status: Option<String>,
    /// The sidebar's own state (collapse, Space, filters, multi-selection); `web_commands.rs` moves it.
    pub(crate) sidebar_ui: SidebarUiStore,
    /// The drawn list, under the desktop's field name so its executor files read it unchanged.
    pub(crate) sidebar_list: super::sidebar_list::SidebarList,
    pub(crate) menu_host: MenuHost,
    /// The desktop's record lines; the page writes none (`diagnostics.rs`).
    pub(crate) diagnostics: super::diagnostics::GxStoreDiagnostics,
    /// The counters of the desktop executor files this build compiles.
    pub(crate) sidebar_actions: super::sidebar_actions::SidebarActionCounters,
    pub(crate) sidebar_modals: super::sidebar_modals::SidebarModalCounters,
    pub(crate) sidebar_open: super::sidebar_open::SidebarOpenCounters,
    pub(crate) sidebar_lifecycle: super::sidebar_lifecycle::SidebarLifecycleCounters,
    pub(crate) sidebar_flags: super::sidebar_flags::SidebarFlagsCounters,
    pub(crate) sidebar_snooze: super::sidebar_snooze::SidebarSnoozeCounters,
    pub(crate) sidebar_bulk: super::sidebar_bulk::SidebarBulkCounters,
    pub(crate) sidebar_drag: super::sidebar_drag::SidebarDragCounters,
    pub(crate) drop_queue: super::sidebar_drop_queue::SidebarDropQueue,
    /// The client-owned workspace session groups document (`workspace_groups.rs`), stored in the page's localStorage and pushed to gxserver.
    pub(crate) workspace_groups: super::workspace_groups::WorkspaceGroupsHost,
    /// The project collections and Spaces documents (`client_document.rs`, `project_docs.rs`).
    pub(crate) collections:
        super::client_document::ClientDocumentHost<ghostex_gx_core::CollectionsDocument>,
    pub(crate) spaces: super::client_document::ClientDocumentHost<ghostex_gx_core::SpacesDocument>,
    pub(crate) project_moves: super::project_docs::ProjectMoveCounters,
    pub(crate) collection_menu: super::collection_menu::CollectionMenuCounters,
    pub(crate) pending_collection_rename: Option<(String, u64)>,
    /// Creates and opens (`create/`).
    pub(crate) create: super::create::CreateHost,
    /// Git, worktrees and Handoff / Export: the desktop's `gx_store/git/`.
    pub(crate) git: super::git::GitHost,
    /// The HUD and the per-row facts the list reads, under the desktop's name (`runtime_facts.rs`).
    pub(crate) runtime_facts: super::runtime_facts::SidebarRuntimeFacts,
    /// The HUD's sources and its composition: the desktop's `hud/`.
    pub(crate) hud: super::hud::HudHost,
    /// The page's socket to the daemon, under the desktop's name for its client (`request_resubscribe`).
    pub(crate) client: Option<WebStoreClient>,
    pub(crate) custom_tags: super::custom_tags_sync::CustomTagsSyncHost,
    pub(crate) close_project: super::sidebar_close_project::CloseProjectCounters,
}

/// The desktop's `GxClient`, as far as the shared files use it: ask for a fresh full snapshot.
pub(crate) struct WebStoreClient {
    socket: web_sys::WebSocket,
}

impl WebStoreClient {
    pub(crate) fn request_resubscribe(&self) {
        let message = ClientMessage::SubscribePresentation {
            client_id: Some("ghostex-gpui-web".to_string()),
            last_revision: None,
            renderer_commands: None,
        };
        if let Ok(text) = serde_json::to_string(&message) {
            let _ = self.socket.send_with_str(&text);
        }
    }
}

pub(crate) fn now_ms() -> u64 {
    js_sys::Date::now() as u64
}

impl GhostexGpuiApp {
    /// Bootstraps, connects and pumps until the page goes away, reconnecting on a close.
    pub(crate) fn gx_store_start(&mut self, cx: &mut gpui::Context<Self>) {
        self.gx_store_restore_sidebar_ui();
        cx.spawn(async move |app, cx| {
            let endpoint = match web_transport::bootstrap().await {
                Ok(endpoint) => endpoint,
                Err(error) => {
                    let _ = app.update(cx, |app, cx| {
                        app.gx_store.status = Some(format!("Could not reach Ghostex: {error}"));
                        cx.notify();
                    });
                    return;
                }
            };
            // The chat host's socket connects to the same daemon (`app/gx_chat/`).
            crate::app::gx_chat::set_endpoint(
                crate::app::gx_chat::LOCAL_MACHINE_ID,
                &endpoint.base_url,
                &endpoint.auth_token,
            );
            let _ = app.update(cx, |app, _| app.gx_store.endpoint = Some(endpoint.clone()));
            let mut attempt = 0u32;
            loop {
                let (sender, mut receiver) = mpsc::unbounded();
                let _ = app.update(cx, |app, cx| {
                    app.gx_store_handle(
                        Event::Connection {
                            machine: MachineId::Local,
                            update: ConnectionUpdate::Connecting { attempt },
                        },
                        cx,
                    );
                    match web_transport::open_events(&endpoint, sender) {
                        Ok(socket) => app.gx_store.client = Some(WebStoreClient { socket }),
                        Err(error) => app.gx_store.status = Some(error),
                    }
                });
                while let Some(event) = receiver.next().await {
                    let closed = matches!(event, StreamEvent::Closed);
                    if app
                        .update(cx, |app, cx| app.gx_store_stream_event(event, cx))
                        .is_err()
                    {
                        return;
                    }
                    if closed {
                        break;
                    }
                }
                attempt += 1;
                cx.background_executor()
                    .timer(std::time::Duration::from_secs(u64::from(attempt.min(5))))
                    .await;
            }
        })
        .detach();
    }

    fn gx_store_stream_event(&mut self, event: StreamEvent, cx: &mut gpui::Context<Self>) {
        match event {
            StreamEvent::Open => self.gx_store_subscribe(true),
            StreamEvent::Frame(text) => {
                // One per HTTP request any client makes, and nothing reads it.
                if text.contains("\"type\":\"apiRequestHandled\"") || text.contains("\"sessionChat") {
                    return;
                }
                match self.gx_store.core.handle_raw_frame(MachineId::Local, &text, now_ms()) {
                    Ok(output) => self.gx_store_after_frame(output, cx),
                    Err(error) => log::warn!("frame did not parse: {error:?}"),
                }
            }
            StreamEvent::Closed => {
                self.gx_store.client = None;
                self.gx_store_handle(
                    Event::Connection {
                        machine: MachineId::Local,
                        update: ConnectionUpdate::Lost { error: None },
                    },
                    cx,
                );
            }
        }
    }

    fn gx_store_subscribe(&mut self, full_snapshot: bool) {
        let held = self
            .gx_store
            .core
            .presentation()
            .machine(&MachineId::Local)
            .and_then(|machine| machine.loaded())
            .map(|loaded| loaded.revision);
        let message = ClientMessage::SubscribePresentation {
            client_id: Some("ghostex-gpui-web".to_string()),
            last_revision: if full_snapshot { None } else { held },
            renderer_commands: None,
        };
        if let (Some(client), Ok(text)) = (&self.gx_store.client, serde_json::to_string(&message)) {
            let _ = client.socket.send_with_str(&text);
        }
    }

    pub(crate) fn gx_store_handle(&mut self, event: Event, cx: &mut gpui::Context<Self>) {
        let output = self.gx_store.core.handle(event, now_ms());
        self.gx_store_after(output, cx);
    }

    /// A daemon frame: the client-owned documents' guards judge the daemon's copy, as the desktop's pump does (`gx_store/host.rs`), then the frame's changes are applied like any other.
    fn gx_store_after_frame(&mut self, output: ghostex_gx_core::Output, cx: &mut gpui::Context<Self>) {
        let side = &output.changes.side_state;
        let (groups_changed, collections_changed, spaces_changed) =
            (side.workspace_groups, side.project_collections, side.spaces);
        let local_reloaded = output.changes.machines_reloaded.iter().any(MachineId::is_local);
        self.gx_store_after(output, cx);
        if ghostex_gx_core::document_reconcile_wanted(groups_changed, local_reloaded) {
            self.gx_store.workspace_groups.counters.reconcile_seen += 1;
            self.gx_store_reconcile_workspace_groups(cx);
        }
        self.gx_store_book_project_docs_read(cx);
        let collections_wanted = ghostex_gx_core::document_reconcile_wanted(collections_changed, local_reloaded);
        let spaces_wanted = ghostex_gx_core::document_reconcile_wanted(spaces_changed, local_reloaded);
        if collections_wanted || spaces_wanted {
            self.gx_store_reconcile_project_docs(collections_wanted, spaces_wanted, cx);
        }
        self.gx_store_prune_workspace_groups(cx);
    }

    fn gx_store_after(&mut self, output: ghostex_gx_core::Output, cx: &mut gpui::Context<Self>) {
        for effect in output.effects {
            match effect {
                ghostex_gx_core::Effect::ResubscribePresentation { .. } => self.gx_store_subscribe(true),
                effect @ (ghostex_gx_core::Effect::MachineLive { .. }
                | ghostex_gx_core::Effect::RefetchSidebarHud { .. }
                | ghostex_gx_core::Effect::DomainProjectChanged { .. }) => {
                    self.gx_store_perform_hud_effect(effect, cx)
                }
                _ => {}
            }
        }
        self.gx_store_hud_store_changed(&output.changes, cx);
        self.gx_store.sidebar_list.note_changes(&output.changes);
        self.gx_store_update_sidebar_list(cx);
    }

    /// Something outside the store moved (the sidebar's own state, a setting): the list is rebuilt.
    pub(crate) fn gx_store_sidebar_state_changed(&mut self, cx: &mut gpui::Context<Self>) {
        self.gx_store.sidebar_list.mark_dirty();
        self.gx_store_update_sidebar_list(cx);
    }

    /// Applies what changed since the last update and installs the list the shared renderer draws.
    pub(crate) fn gx_store_update_sidebar_list(&mut self, cx: &mut gpui::Context<Self>) {
        let now_ms = now_ms();
        let store = &mut self.gx_store;
        let ui = store.sidebar_ui.state().clone();
        let hud = store.runtime_facts.hud_value();
        // The menus' facts, from the HUD and the launcher's stored default, as the desktop's `gx_store_menu_host` builds them. The page has no Split Right (no panes) and no Keep Awake.
        store.menu_host = MenuHost {
            workspace_focus_bridge: false,
            agents: super::menu_host_lifted::launcher_agents(&hud["agents"]),
            primary_agent_id: super::read_primary_agent_launcher_id(),
            global_commands: super::menu_host_lifted::header_commands(&hud["globalCommands"]),
            project_commands: super::menu_host_lifted::header_commands_by_project(&hud["commandsByProject"]),
            keep_awake_minutes: None,
            machine_connected: true,
        };
        store.sidebar_list.last_inputs.host.project_diff_stats = store
            .runtime_facts
            .project_diff_stats
            .iter()
            .map(|(project_id, stats)| (project_id.clone(), *stats))
            .collect();
        let snapshot = store.sidebar_list.update(&store.core, ui, &store.menu_host, &hud, now_ms);
        self.latest_sidebar_project_snapshot = Some(crate::app::model::GpuiProjectSnapshot {
            active_project_id: self
                .gx_store
                .core
                .focus()
                .active_project
                .as_ref()
                .filter(|project| project.machine == MachineId::Local)
                .map(|project| crate::app::model::GpuiProjectId(project.project_id.clone())),
        });
        // The worktree dialog's agent list: the HUD's agents (the desktop reads the New Thread picker's list, which the page does not have).
        self.new_thread_picker_agents = hud["agents"].as_array().cloned();
        self.install_native_sidebar_snapshot(Arc::new(snapshot), cx);
        self.web_open_linked_session(cx);
        self.web_book_sidebar_deadline(cx);
        // A showing Quick Access republishes from the store it reads (`quick_access/host.rs`).
        self.gx_store_quick_access_store_changed(true, cx);
        cx.notify();
    }

    /// Whether the list is drawn yet: the page has no launch window to wait out, only the first snapshot.
    /// The desktop's immediate install; the page's update installs at once.
    pub(crate) fn gx_store_install_sidebar_list(&mut self, cx: &mut gpui::Context<Self>) {
        self.gx_store_update_sidebar_list(cx);
    }

    pub(crate) fn gx_store_sidebar_list_ready(&self) -> bool {
        self.gx_store.sidebar_list.view().ready
    }

    /// The HUD the list carries, as the desktop's `gx_store_sidebar_hud`.
    pub(crate) fn gx_store_sidebar_hud(&self) -> Option<Arc<Value>> {
        Some(self.gx_store.runtime_facts.hud_value())
    }

    pub(crate) fn gx_store_sidebar_hud_value(&self) -> Arc<Value> {
        self.gx_store.runtime_facts.hud_value()
    }
}

impl GxStoreHost {
    pub(crate) fn sidebar_view(&self) -> &ghostex_gx_core::SidebarView {
        self.sidebar_list.view()
    }

    /// The store key of a drawn row; `None` for a browser tab or a row the list no longer holds.
    pub(crate) fn session_key_for_row(&self, sidebar_session_id: &str) -> Option<SessionKey> {
        self.sidebar_list
            .view()
            .groups
            .iter()
            .flat_map(|group| group.core.sessions.iter())
            .find(|session| session.row.sidebar_session_id == sidebar_session_id)
            .and_then(|session| session.row.key.clone())
    }
}
