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
use super::shadow_diff::{ObservedFocus, ShadowDiff};
use super::sidebar_list::SidebarList;
use super::sidebar_shadow::SidebarShadow;
use super::sidebar_ui::SidebarUiHost;
use crate::GhostexGpuiApp;
use crate::app::helpers::GpuiGxserverPresentationFocusEcho;
use crate::app::model::GpuiGxserverPresentationFocusState;

/// The daemon does not route by this id; it only tells this socket apart from the old runtime's
/// (`ghostex-gpui-sidebar`) in a frame capture.
const GX_STORE_CLIENT_ID: &str = "ghostex-gpui-store";
/// How long a difference between the two tab lists must last before it counts. The old runtime
/// and the store read the same daemon over two sockets, so either can be a few frames ahead.
const SHADOW_SETTLE: Duration = Duration::from_millis(1000);
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
}

/// CDXC:StateSync 2026-09-19 DECISION:
/// User: the desktop app stops running product logic in QuickJS; one Rust state store owns projects, sessions, tabs, panes, focus, and chat state, and gxserver is the only thing the app syncs with.
/// This is that store inside the app, fed by its own socket to the local daemon. It owns focus: local selections are its intents, the sidebar row highlight reads it, and the old runtime's focus payloads pass through it (local_focus.rs). Projects, sessions and tab membership on screen still come from the old runtime until the sidebar milestone, so the tab list comparison in shadow_diff.rs keeps running. It always runs; only the disk logging is gated.
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
    connecting_since: Option<Instant>,
    pub(super) shadow: ShadowDiff,
    pub(crate) sidebar_ui: SidebarUiHost,
    pub(crate) sidebar_list: SidebarList,
    pub(super) sidebar_shadow: SidebarShadow,
    pub(super) diagnostics: GxStoreDiagnostics,
    pub(crate) local_focus: LocalFocus,
    pub(crate) layout_persist: LayoutPersist,
    /// One client per connected remote machine, and the machine tabs the sidebar draws.
    pub(crate) remote: super::remote_clients::RemoteClients,
    pub(super) menu_host: super::sidebar_menus::MenuHostCache,
    pub(super) sidebar_actions: super::sidebar_actions::SidebarActionCounters,
    pub(super) sidebar_lifecycle: super::sidebar_lifecycle::SidebarLifecycleCounters,
    pub(super) sidebar_flags: super::sidebar_flags::SidebarFlagsCounters,
    pub(super) sidebar_modals: super::sidebar_modals::SidebarModalCounters,
    pub(super) sidebar_snooze: super::sidebar_snooze::SidebarSnoozeCounters,
    pub(super) sidebar_bulk: super::sidebar_bulk::SidebarBulkCounters,
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
        };
        self.run_effects(output.effects);
        // New frames may be exactly what a pending tab list difference was waiting for.
        self.settle_shadow_diff();
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

    /// Mirrors the old runtime's focus into the core and compares its tab list with the store's.
    /// Returns `true` when a difference started waiting to settle, so the caller schedules its
    /// judgement.
    fn observe_old_runtime_focus_state(
        &mut self,
        old_state: &GpuiGxserverPresentationFocusState,
        echo: &GpuiGxserverPresentationFocusEcho,
    ) -> bool {
        let waiting_since = self.shadow.pending_since();
        let observed = self
            .shadow
            .observe(&mut self.core, old_state, echo, now_ms());
        self.note_observed_focus(observed);
        self.diagnostics.shadow_summary(&self.shadow, &self.core);
        // A new difference, or another one than was waiting, starts its own clock.
        let now_waiting_since = self.shadow.pending_since();
        now_waiting_since.is_some() && now_waiting_since != waiting_since
    }

    /// Whether the old runtime's accepted focus is a row the store cannot hold.
    fn note_observed_focus(&mut self, observed: ObservedFocus) {
        match observed {
            ObservedFocus::Stale => {}
            ObservedFocus::Local => self.local_focus.foreign_focus = false,
            ObservedFocus::Foreign => self.local_focus.foreign_focus = true,
        }
    }

    pub(super) fn settle_shadow_diff(&mut self) {
        if let Some(mismatch) = self.shadow.settle(&mut self.core, SHADOW_SETTLE, now_ms()) {
            self.diagnostics.shadow_mismatch(&mismatch, &self.core);
        }
        // Judging mirrors the old focus again: a session that was missing a moment ago may now be
        // placed.
        if let Some(observed) = self.shadow.take_remirrored() {
            self.note_observed_focus(observed);
        }
        self.diagnostics.shadow_summary(&self.shadow, &self.core);
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
        // The sidebar's own state is read once, and again later if that read failed.
        self.gx_store_restore_sidebar_ui(cx);
        // A machine may already have connected before the store came up, and the machine tabs are
        // built here whether or not one has.
        self.gx_store_sync_remote_clients(true, cx);
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
        if self.gx_store_after_pump(outcome.tab_lists_changed, cx) {
            cx.notify();
        }
        // The list reads the store, so it is brought up to date once per burst rather than per
        // event, and does nothing at all when the burst changed nothing it draws.
        self.gx_store_update_sidebar_list(cx);
        outcome.thread_ended
    }

    /// The old runtime published its focus state with the stamp it had been told. The store
    /// follows it unless a newer local selection exists, and the tab lists are compared.
    pub(super) fn gx_store_observe_old_runtime_focus_state(
        &mut self,
        old_state: &GpuiGxserverPresentationFocusState,
        echo: &GpuiGxserverPresentationFocusEcho,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self
            .gx_store
            .observe_old_runtime_focus_state(old_state, echo)
        {
            return;
        }
        // A difference that no later frame or publish resolves still has to be judged.
        cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(SHADOW_SETTLE + Duration::from_millis(200))
                .await;
            let _ = this.update(cx, |this, cx| {
                this.gx_store.settle_shadow_diff();
                // Judging mirrors the old focus again, which can move the store's focus.
                if this.gx_store.refresh_row_focus_cache() {
                    cx.notify();
                }
            });
        })
        .detach();
    }
}

pub(super) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}
