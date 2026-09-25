use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use futures::StreamExt as _;
use futures::channel::mpsc;
use ghostex_gx_client::{ClientOutput, GxClient, GxClientConfig};
use ghostex_gx_core::{ConnectionPhase, ConnectionUpdate, Core, Event, MachineId};

use super::diagnostics::GxStoreDiagnostics;
use super::layout_persist::LayoutPersist;
use super::local_focus::LocalFocus;
use super::sidebar_list::SidebarList;
use super::sidebar_self_check::SidebarSelfCheck;
use super::sidebar_ui::SidebarUiHost;
use crate::GhostexGpuiApp;

/// The daemon does not route by this id; it only tells this socket apart from the old runtime's
/// (`ghostex-gpui-sidebar`) in a frame capture.
const GX_STORE_CLIENT_ID: &str = "ghostex-gpui-store";
/// Delay before a client whose thread ended on its own is replaced (the last entry repeats). The
/// client reconnects by itself for as long as its thread lives, so this path only runs after a
/// panic; the steps keep a panic that repeats on every start from spinning.
const CLIENT_RESTART_BACKOFF_MS: [u64; 5] = [1000, 5000, 15000, 30000, 60000];
/// A client that lived this long before its thread ended starts the backoff over.
const CLIENT_HEALTHY_LIFETIME: Duration = Duration::from_secs(60);

#[derive(Clone, PartialEq, Eq)]
struct GxStoreTransport {
    base_url: String,
    auth_token: String,
}

/// What happened since the app started. Memory only; the log lines are built from it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct GxStoreCounters {
    pub(crate) client_starts: u64,
    pub(crate) pumps: u64,
    pub(crate) events: u64,
    pub(crate) largest_burst: usize,
    pub(crate) connections_lost: u64,
    pub(crate) reloads: u64,
    pub(crate) resubscribes_requested: u64,
    pub(crate) skipped_row_reports: u64,
    pub(crate) client_diagnostics: u64,
    pub(crate) client_thread_exits: u64,
}

/// What one pump changed, for the app-level follow-up that needs more than the host.
#[derive(Clone, Copy, Debug, Default)]
pub(super) struct PumpOutcome {
    /// The drain found the client's channel closed: its thread is gone and must be replaced.
    pub(super) thread_ended: bool,
    pub(super) tab_lists_changed: bool,
    /// The daemon's copy of the workspace session groups document landed. The guard decides what
    /// that means, one level up, where there is a `cx` to write storage and book a push with.
    pub(super) workspace_groups_changed: bool,
    /// The daemon's copy of the project collections document landed.
    pub(super) project_collections_changed: bool,
    /// The daemon's copy of the Spaces document landed.
    pub(super) spaces_changed: bool,
    /// A full snapshot replaced this computer's machine.
    ///
    /// CDXC:Sessions 2026-09-21 WHY:
    /// Every client-owned document has to be judged against the daemon's copy on a reload, whatever
    /// the three flags above say. A host that SEEDS the store with a document (all three of these
    /// do: the stored key is read and put into the side state) makes those flags mean "the daemon's
    /// copy differs from what this host last wrote there", so on an ordinary launch, where they
    /// agree, the guard is never asked at all. That cost two live rounds to find, because it is
    /// invisible except as a counter that stays at zero.
    pub(super) local_reloaded: bool,
}

/// CDXC:StateSync 2026-09-19 DECISION:
/// User: the desktop app stops running product logic in QuickJS; one Rust state store owns projects, sessions, tabs, panes, focus, and chat state, and gxserver is the only thing the app syncs with.
/// This is that store inside the app, fed by its own socket to the local daemon. It owns focus: local selections are its intents, the sidebar row highlight reads it (local_focus.rs), and the workspace's tab list and active project are published from it (focus_publish.rs). It always runs; only the disk logging is gated.
#[derive(Default)]
pub(crate) struct GxStoreHost {
    pub(super) core: Core,
    pub(super) client: Option<GxClient>,
    transport: Option<GxStoreTransport>,
    /// Bumped by every client start, so a pump task or a restart timer of an earlier client can
    /// tell that it is no longer the one in charge.
    client_generation: u64,
    client_started_at: Option<Instant>,
    client_restart_attempt: u32,
    /// Shared with the client thread, which quotes it as `lastRevision` when it subscribes.
    held_revision: Arc<AtomicI64>,
    pub(super) counters: GxStoreCounters,
    /// The remote machines' start-up connect and reconnect ladder (remote_conn/reconnect_ladder.rs).
    pub(crate) remote_reconnect: crate::app::remote_conn::reconnect_ladder::RemoteReconnectLadder,
    /// CLI renderer commands the local client handed over in the last pump, performed right after
    /// it (renderer_commands/).
    pending_renderer_commands: Vec<ghostex_gx_core::protocol::RendererCommand>,
    connecting_since: Option<Instant>,
    pub(crate) sidebar_ui: SidebarUiHost,
    pub(crate) sidebar_list: SidebarList,
    pub(super) sidebar_self_check: SidebarSelfCheck,
    pub(super) diagnostics: GxStoreDiagnostics,
    pub(crate) local_focus: LocalFocus,
    /// The workspace's focus state and active project context, published from the store.
    pub(crate) focus_publish: super::focus_publish::FocusPublish,
    pub(crate) focus_perform: super::focus_perform::FocusPerformCounters,
    pub(crate) project_activation: super::project_activation::ProjectActivationHost,
    pub(crate) layout_persist: LayoutPersist,
    /// One client per connected remote machine, and the machine tabs the sidebar draws.
    pub(crate) remote: super::remote_clients::RemoteClients,
    pub(super) menu_host: super::sidebar_menus::MenuHostCache,
    pub(super) sidebar_actions: super::sidebar_actions::SidebarActionCounters,
    pub(super) sidebar_lifecycle: super::sidebar_lifecycle::SidebarLifecycleCounters,
    pub(super) sidebar_flags: super::sidebar_flags::SidebarFlagsCounters,
    pub(super) sidebar_modals: super::sidebar_modals::SidebarModalCounters,
    pub(super) sidebar_open: super::sidebar_open::SidebarOpenCounters,
    /// The two account pages' state and counters.
    pub(super) sidebar_accounts: super::sidebar_accounts::SidebarAccountsHost,
    pub(super) sidebar_snooze: super::sidebar_snooze::SidebarSnoozeCounters,
    pub(super) sidebar_bulk: super::sidebar_bulk::SidebarBulkCounters,
    pub(super) sidebar_drag: super::sidebar_drag::SidebarDragCounters,
    /// Close Project's successor, which the sidebar page used to fill in.
    pub(super) close_project: super::sidebar_close_project::CloseProjectCounters,
    /// A local row's click, routed straight to the runtime instead of through the page.
    pub(super) local_focus_route: super::sidebar_focus_route::LocalFocusRouteCounters,
    /// Drags dropped before the document they edit had been read.
    pub(super) drop_queue: super::sidebar_drop_queue::SidebarDropQueue,
    /// A remote row's session actions: their counters and their log budgets.
    pub(super) sidebar_remote: super::sidebar_remote::SidebarRemoteHost,
    pub(super) sidebar_remote_focus: super::sidebar_remote_focus::SidebarRemoteFocusHost,
    /// The project slot hotkeys' jump: its counters and its log budget.
    pub(super) slot_jump: super::sidebar_slot_jump::SlotJumpHost,
    /// The session slot hotkeys (cmd+1..9): their counters and their log budget.
    pub(super) session_slot: super::sidebar_session_slot::SessionSlotHost,
    /// The client-owned workspace session groups document, its stored key and its pending-push
    /// guard.
    pub(crate) workspace_groups: super::workspace_groups::WorkspaceGroupsHost,
    /// K5, the project collections document, on the generic client-document host.
    pub(crate) collections:
        super::client_document::ClientDocumentHost<ghostex_gx_core::CollectionsDocument>,
    /// K6, the Spaces document. Same host, no stored key.
    pub(crate) spaces: super::client_document::ClientDocumentHost<ghostex_gx_core::SpacesDocument>,
    pub(super) project_moves: super::project_docs::ProjectMoveCounters,
    /// K5 and K6 on a REMOTE machine: the per-machine `nextCollectionNumber` floor and what this
    /// run sent down the machine tunnels.
    pub(super) remote_project_docs: super::remote_project_docs::RemoteProjectDocsHost,
    /// A Project Group's Rename, colour and Ungroup, which write the same K5 document.
    pub(super) collection_menu: super::collection_menu::CollectionMenuCounters,
    /// The New/Edit Space dialog's results, which write the K6 document.
    pub(super) space_editor: super::space_editor::SpaceEditorCounters,
    /// The focus a Space switch restores.
    pub(super) space_switch: super::space_switch::SpaceSwitchCounters,
    /// The project the Add Project dialog last reported, and where it goes.
    pub(super) added_project: super::added_project::AddedProjectHost,
    /// The last-seen copy of every remote machine: which ones owe a write, and what is unwritten.
    pub(crate) last_seen: super::remote_last_seen::RemoteLastSeenWriter,
    /// What the fall-through of the sidebar dispatch did with each command.
    pub(super) runtime_route: super::sidebar_runtime_route::SidebarRuntimeRouteCounters,
    /// The runtime's one-way facts channel, beside the publish it is compared with.
    pub(crate) runtime_facts: super::runtime_facts::SidebarRuntimeFacts,
    /// Git, worktrees and transcript export (family F5, gx_store/git/).
    pub(crate) git: super::git::GitHost,
    /// The effects the app performs rather than the store (`effects.rs`).
    pub(super) app_effects: super::effects::AppEffectQueue,
    /// The sidebar HUD's sources (hud/).
    pub(super) hud: super::hud::HudHost,
    /// The status item's and the pet's per-machine views (indicators/).
    pub(super) indicators: super::indicators::IndicatorsHost,
    /// The collection a project move just created, which the renderer opens its Rename on. Held
    /// here rather than carried from the old projection's publish, because the create is the
    /// store's now and the publish would not know about it.
    pub(super) pending_collection_rename: Option<(String, u64)>,
    pub(crate) shown_sessions: super::terminal_lifecycle::shown_sessions::ShownSessionsHost,
    /// F4's creates and opens: counters and the browser open waiting for its project switch.
    pub(crate) create: super::create::CreateHost,
    /// The custom session tag catalog's debounced push to this computer's gxserver.
    pub(crate) custom_tags: super::custom_tags_sync::CustomTagsSyncHost,
    /// The last session an App Shot went to (app_shot.rs).
    pub(crate) app_shot: super::app_shot::AppShotHost,
    #[cfg(target_os = "windows")]
    pub(crate) terminal_title_settle:
        super::terminal_lifecycle::terminal_events::TerminalTitleSettle,
}

impl GxStoreHost {
    /// Drains the client and applies the burst. Runs on the UI thread and does no I/O: frames are
    /// parsed on the client thread, and everything here is memory work bounded by the burst.
    ///
    /// The outcome says whether the client's thread is gone (the caller must replace the client)
    /// and what changed, so the caller repaints only when something on screen reads it.
    fn pump(&mut self) -> PumpOutcome {
        let Some(client) = &self.client else {
            return PumpOutcome::default();
        };
        let outputs = client.drain();
        let thread_ended = client.thread_ended();
        if outputs.is_empty() {
            return PumpOutcome {
                thread_ended,
                ..PumpOutcome::default()
            };
        }
        let mut events = Vec::with_capacity(outputs.len());
        for output in outputs {
            match output {
                ClientOutput::Event(event) => {
                    if let Event::Connection { update, .. } = &event {
                        self.note_connection(update);
                    }
                    events.push(event);
                }
                ClientOutput::Diagnostic(diagnostic) => {
                    self.counters.client_diagnostics += 1;
                    self.diagnostics.client_diagnostic(&diagnostic);
                }
                ClientOutput::RendererCommand(command) => {
                    self.pending_renderer_commands.push(command);
                }
            }
        }
        self.counters.pumps += 1;
        self.counters.events += events.len() as u64;
        self.counters.largest_burst = self.counters.largest_burst.max(events.len());
        let output = self.core.handle_batch(events, now_ms());
        // The sidebar list derives from these; it applies them when it next updates.
        self.sidebar_list.note_changes(&output.changes);
        if let Some(loaded) = self.core.presentation().loaded(&MachineId::Local) {
            self.held_revision.store(loaded.revision, Ordering::Release);
        }
        if !output.changes.machines_reloaded.is_empty() {
            self.counters.reloads += 1;
            let since_connect = self.connecting_since.map(|since| since.elapsed());
            self.diagnostics
                .store_loaded(&self.core, &self.counters, since_connect);
        }
        let live = self
            .core
            .presentation()
            .machine(&MachineId::Local)
            .is_some_and(|machine| machine.connection().phase == ConnectionPhase::Live);
        if live {
            // The next "connect to loaded" time starts at the next reconnect.
            self.connecting_since = None;
        }
        let outcome = PumpOutcome {
            thread_ended,
            tab_lists_changed: output.changes.tab_lists_changed(),
            workspace_groups_changed: output.changes.side_state.workspace_groups,
            project_collections_changed: output.changes.side_state.project_collections,
            spaces_changed: output.changes.side_state.spaces,
            local_reloaded: output
                .changes
                .machines_reloaded
                .iter()
                .any(MachineId::is_local),
        };
        self.run_effects(output.effects);
        outcome
    }

    /// Drops a client whose thread ended on its own and returns the delay before its successor
    /// starts. The thread normally reports `Lost` itself as its last output; when it could not,
    /// the core is told here, so the rows never stay marked live behind a dead socket.
    fn retire_dead_client(&mut self) -> Duration {
        self.client = None;
        self.counters.client_thread_exits += 1;
        let still_connected = self
            .core
            .presentation()
            .machine(&MachineId::Local)
            .is_some_and(|machine| {
                matches!(
                    machine.connection().phase,
                    ConnectionPhase::Connecting | ConnectionPhase::Live
                )
            });
        if still_connected {
            let lost = ConnectionUpdate::Lost {
                error: Some("the client thread ended".to_string()),
            };
            self.note_connection(&lost);
            self.core.handle(
                Event::Connection {
                    machine: MachineId::Local,
                    update: lost,
                },
                now_ms(),
            );
        }
        let delay = self.next_restart_delay();
        self.diagnostics
            .client_thread_ended(self.client_restart_attempt, delay);
        delay
    }

    fn next_restart_delay(&mut self) -> Duration {
        if self
            .client_started_at
            .take()
            .is_some_and(|at| at.elapsed() >= CLIENT_HEALTHY_LIFETIME)
        {
            self.client_restart_attempt = 0;
        }
        let step = (self.client_restart_attempt as usize).min(CLIENT_RESTART_BACKOFF_MS.len() - 1);
        self.client_restart_attempt = self.client_restart_attempt.saturating_add(1);
        Duration::from_millis(CLIENT_RESTART_BACKOFF_MS[step])
    }

    fn note_connection(&mut self, update: &ConnectionUpdate) {
        match update {
            ConnectionUpdate::Connecting { .. } => {
                self.connecting_since.get_or_insert_with(Instant::now);
            }
            ConnectionUpdate::Lost { .. } => self.counters.connections_lost += 1,
            _ => {}
        }
        self.diagnostics.connection(&MachineId::Local, update);
    }
}

impl GhostexGpuiApp {
    /// Starts the store's client for the local daemon, or restarts it when the transport (base
    /// URL or token) changed. Called wherever the sidebar bootstrap is set. The core and its rows
    /// survive a restart of the client: the new socket subscribes with the held revision, and a
    /// daemon with another identity makes the core ask for a full snapshot.
    pub(crate) fn sync_gx_store_transport(&mut self, cx: &mut gpui::Context<Self>) {
        let next = self
            .sidebar_gxserver_bootstrap
            .as_ref()
            .map(|bootstrap| GxStoreTransport {
                base_url: bootstrap.base_url.clone(),
                auth_token: bootstrap.auth_token.clone(),
            });
        // Both run once, before the first frame can be applied: the persisted focus seeds the core,
        // and the task that writes the shell layout starts waiting.
        self.gx_store
            .restore_focus_once(&self.sidebar_gxserver_presentation_focus_state);
        self.start_gx_store_layout_persist_task(cx);
        // The sidebar's own once-a-second tick, which the armed-timer labels and the menu-host
        // re-read ride (sidebar_clock.rs). Started once, and not tied to having a transport.
        self.gx_store_start_sidebar_clock(cx);
        self.gx_store_start_app_effects(cx);
        // The sidebar's own state is read once, and again later if that read failed.
        self.gx_store_restore_sidebar_ui(cx);
        // A machine may already have connected before the store came up, and the machine tabs are
        // built here whether or not one has.
        self.gx_store_sync_remote_clients(true, cx);
        self.remote_reconnect_on_launch(cx);
        if self.gx_store.transport == next {
            return;
        }
        self.gx_store.transport = next;
        self.gx_store.client_restart_attempt = 0;
        self.start_gx_store_client(cx);
    }

    /// The one start path: a transport change and the replacement of a dead client both end here.
    fn start_gx_store_client(&mut self, cx: &mut gpui::Context<Self>) {
        let host = &mut self.gx_store;
        // Dropping the client stops its thread; its wake channel closes and ends the old pump.
        host.client = None;
        host.client_generation += 1;
        let generation = host.client_generation;
        let Some(transport) = host.transport.clone() else {
            return;
        };
        let (wake, mut wakes) = mpsc::unbounded::<()>();
        let client = GxClient::start(
            GxClientConfig {
                machine: MachineId::Local,
                base_url: transport.base_url,
                auth_token: transport.auth_token,
                client_id: GX_STORE_CLIENT_ID.to_string(),
                held_revision: host.held_revision.clone(),
                forward_chat_frames: false,
                // This socket is the app's one renderer-command target (renderer_commands/).
                renderer_commands: true,
            },
            move || {
                let _ = wake.unbounded_send(());
            },
        );
        match client {
            Ok(client) => {
                host.client = Some(client);
                host.client_started_at = Some(Instant::now());
                host.counters.client_starts += 1;
            }
            Err(error) => {
                host.diagnostics.client_start_failed(&error);
                let delay = host.next_restart_delay();
                self.schedule_gx_store_client_restart(generation, delay, cx);
                return;
            }
        }
        cx.spawn(async move |this, cx| {
            while wakes.next().await.is_some() {
                let alive = this.update(cx, |this, cx| {
                    if this.gx_store_pump(cx) {
                        this.gx_store_client_thread_ended(generation, cx);
                    }
                });
                if alive.is_err() {
                    return;
                }
            }
            // The wake closure lives on the client thread and is dropped with it, so the stream
            // ending is the signal that the thread is gone. Unlike the channel check in the pump
            // it cannot be missed: it arrives even when the thread died without a last output.
            let _ = this.update(cx, |this, cx| {
                this.gx_store_client_thread_ended(generation, cx);
            });
        })
        .detach();
    }

    /// The thread of client `generation` is gone. Nothing to do when that client was replaced or
    /// dropped on purpose; otherwise its last outputs are applied, the store is marked lost, and
    /// a new client starts after a bounded backoff.
    fn gx_store_client_thread_ended(&mut self, generation: u64, cx: &mut gpui::Context<Self>) {
        let host = &mut self.gx_store;
        if host.client_generation != generation || host.client.is_none() {
            return;
        }
        self.gx_store_pump(cx);
        let delay = self.gx_store.retire_dead_client();
        self.schedule_gx_store_client_restart(generation, delay, cx);
    }

    fn schedule_gx_store_client_restart(
        &mut self,
        generation: u64,
        delay: Duration,
        cx: &mut gpui::Context<Self>,
    ) {
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(delay).await;
            let _ = this.update(cx, |this, cx| {
                // A transport change in the meantime already started a newer client.
                if this.gx_store.client_generation == generation && this.gx_store.client.is_none() {
                    this.start_gx_store_client(cx);
                }
            });
        })
        .detach();
    }

    /// Applies what the client delivered and follows up on it. Repaints only when the focused or
    /// visible sessions moved, because focus is the only store state the screen reads so far.
    /// Returns `true` when the client's thread is gone.
    fn gx_store_pump(&mut self, cx: &mut gpui::Context<Self>) -> bool {
        let outcome = self.gx_store.pump();
        let renderer_commands = std::mem::take(&mut self.gx_store.pending_renderer_commands);
        if !renderer_commands.is_empty() {
            self.gx_store_take_renderer_commands(renderer_commands, cx);
        }
        // NOT `workspace_groups_changed` alone. This host seeds the core's side state with the
        // stored document itself, so on an ordinary launch the reducer compares the daemon's copy
        // against what this host just put there, agrees, and reports no change: the guard was then
        // never asked anything for a whole run. A reload asks it regardless
        // (gx-core `doc_sync::document_reconcile_wanted`). K5 and K6 need the same treatment on
        // their own flags below; `local_reloaded` is here for all three.
        if ghostex_gx_core::document_reconcile_wanted(
            outcome.workspace_groups_changed,
            outcome.local_reloaded,
        ) {
            self.gx_store.workspace_groups.counters.reconcile_seen += 1;
            // The daemon's copy is already in the store by now; the guard says whether it may stay.
            self.gx_store_reconcile_workspace_groups(cx);
        }
        // The collections key is read here rather than by whatever asks for it first, so the read
        // is already in hand when the daemon's first snapshot arrives and the first echo of a run
        // is judged rather than deferred (gx_store/project_docs.rs).
        self.gx_store_book_project_docs_read(cx);
        // The same funnel for K5 and K6: the daemon's copy is already in the store by now, and the
        // guard says whether it may stay. One place, off the change summary, rather than an
        // interception on the frame path.
        let collections_wanted = ghostex_gx_core::document_reconcile_wanted(
            outcome.project_collections_changed,
            outcome.local_reloaded,
        );
        let spaces_wanted = ghostex_gx_core::document_reconcile_wanted(
            outcome.spaces_changed,
            outcome.local_reloaded,
        );
        if collections_wanted || spaces_wanted {
            self.gx_store_reconcile_project_docs(collections_wanted, spaces_wanted, cx);
        }
        // A session the daemon stopped listing is a member no group may keep, which the old runtime
        // asked on every `createSidebarGroups`. Asked here on the same cadence, and free for the
        // users who have no user-made groups at all: the document's project map is empty.
        self.gx_store_prune_workspace_groups(cx);
        if self.gx_store_after_pump(outcome.tab_lists_changed, cx) {
            cx.notify();
        }
        // A project the Add Project dialog reported is held until the daemon lists it; this is the
        // burst that can be the one carrying it (gx_store/added_project.rs).
        self.gx_store_place_added_project(cx);
        // The list reads the store, so it is brought up to date once per burst rather than per
        // event, and does nothing at all when the burst changed nothing it draws.
        self.gx_store_update_sidebar_list(cx);
        outcome.thread_ended
    }
}

pub(super) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}
