//! Cloud Boxes on this computer's client: home gxserver's box registry read into the store, a
//! hidden machine (`cloudbox:<boxId>`) connected for every reachable box, the cached rows of a box
//! that is not, the box's sessions reported back to home gxserver, and the wake a send needs.
//!
//! CDXC:CloudBoxes 2026-11-02 WHY:
//! A box is another gxserver, so it is fed exactly like a remote computer (`remote_clients.rs`):
//! same client, same store, same chat endpoint and request target. What differs is where its
//! address comes from: not the user's saved `remoteMachines` (a box never enters that list and
//! never gets a machine tab) but home gxserver's `connectInfo`, which hands a `direct` loopback
//! port (the `local` Docker provider) or a `tailcat` address that the Easy Connect forwarder
//! (`helpers/remote/easy_connect_forward.rs`) turns into one. The state lives in one process-wide
//! table rather than on the store host because the chat view's send path, which runs on the
//! background executor, has to reach it too ([`cloud_box_send_target`]).
//!
//! SEE-ALSO: packages/gx-core/src/cloud_boxes.rs (registry, merge rule, cached rows),
//! server/src/cloud_boxes/endpoint.rs (`list`, `connectInfo`, `resume`, `reportBoxSessions`),
//! apps/desktop/src/app/native_chat/state.rs (`rpc`, the send that wakes a box).

use std::collections::{BTreeMap, BTreeSet};
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use ghostex_gx_core::cloud_boxes::{
    box_id_of_machine, cached_box_snapshot, cloud_box_machine, is_cloud_box_machine,
};
use ghostex_gx_core::protocol::cloud_box::{
    CLOUD_BOX_TRANSPORT_DIRECT, CLOUD_BOX_TRANSPORT_TAILCAT, CloudBox, CloudBoxConnectInfo,
    CloudBoxSessionRow, CloudBoxState, cloud_box_machine_id,
};
use ghostex_gx_core::{Event, MachineId};
use serde_json::{Value, json};

use super::host::now_ms;
use super::rpc::gx_rpc;
use crate::app::model::{GpuiRemoteGxserverCapabilities, GpuiRemoteGxserverRequestTarget};
use crate::{GhostexGpuiApp, GpuiRemoteExecutionTarget};

const CLOUD_BOXES_ROUTE: &str = "/api/cloudBoxes";
/// The error code a chat call to a paused or stopped box is refused with: the chat may still take
/// a message, whose send wakes the box (Q1013).
pub(crate) const ASLEEP: &str = "cloudBoxAsleep";
/// The error code of a chat call to a box that cannot be reached and that a send would not wake.
pub(crate) const UNAVAILABLE: &str = "cloudBoxUnavailable";

/// Why a chat call to a box was not sent: `code` is [`ASLEEP`] or [`UNAVAILABLE`].
pub(crate) struct CloudBoxRefusal {
    pub(crate) code: &'static str,
    pub(crate) message: String,
}

fn refusal(code: &'static str, message: &str) -> CloudBoxRefusal {
    CloudBoxRefusal {
        code,
        message: message.to_string(),
    }
}
/// A failed `connectInfo` is asked again after this, while the box stays reachable.
const CONNECT_RETRY: Duration = Duration::from_secs(5);
/// How long a send waits for a woken box to come back and connect.
const WAKE_WAIT: Duration = Duration::from_secs(5 * 60);
const WAKE_POLL: Duration = Duration::from_millis(500);
/// A box's sessions are reported to home gxserver at most this often.
const REPORT_DEBOUNCE: Duration = Duration::from_secs(2);

/// What this client knows about each box beyond the store: where its gxserver answers.
#[derive(Default)]
struct CloudBoxLinks {
    /// Box state by box id, copied from the store's registry on every sync, for the send path.
    states: BTreeMap<String, CloudBoxState>,
    /// The loopback port and bearer token of each connected box, by machine id.
    targets: BTreeMap<String, (u16, String)>,
    /// `connectInfo` asks in flight, and when a failed one may be asked again, by box id.
    connecting: BTreeSet<String>,
    retry_at: BTreeMap<String, Instant>,
    /// Boxes whose cached rows were seeded in this run.
    seeded: BTreeSet<String>,
    /// The sessions last reported per box, so an unchanged list is not sent again.
    reported: BTreeMap<String, Vec<CloudBoxSessionRow>>,
    report_due: BTreeSet<String>,
    /// `resume` asks in flight, by box id: concurrent sends wake a box once.
    waking: BTreeSet<String>,
}

static LINKS: LazyLock<Mutex<CloudBoxLinks>> = LazyLock::new(Mutex::default);

fn links<T>(apply: impl FnOnce(&mut CloudBoxLinks) -> T) -> T {
    let mut guard = LINKS
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    apply(&mut guard)
}

fn request_target(port: u16, token: &str) -> GpuiRemoteGxserverRequestTarget {
    GpuiRemoteGxserverRequestTarget {
        capabilities: GpuiRemoteGxserverCapabilities::default(),
        code_server_component_platform: None,
        execution_target: GpuiRemoteExecutionTarget::PosixHost,
        local_port: port,
        token: token.to_string(),
    }
}

/// The request target of a connected box's gxserver, for `gpui_remote_gxserver_request_target`:
/// every remote-machine path (chat endpoint, sidebar actions, terminal) reaches a box through it.
pub(crate) fn cloud_box_request_target(
    machine_id: &str,
) -> Option<GpuiRemoteGxserverRequestTarget> {
    if !machine_id.starts_with(ghostex_gx_core::protocol::cloud_box::CLOUD_BOX_MACHINE_PREFIX) {
        return None;
    }
    links(|links| links.targets.get(machine_id).cloned())
        .map(|(port, token)| request_target(port, &token))
}

/// The connected box machines and where their gxserver answers, for the remote clients'
/// reconcile, which feeds them like any connected machine.
pub(super) fn connected_cloud_box_targets() -> BTreeMap<String, (u16, String)> {
    links(|links| links.targets.clone())
}

/// Whether a remote machine id is a box the registry still holds, so the reconcile that forgets
/// machines the user removed from settings keeps it.
pub(super) fn cloud_box_machine_known(machine_id: &str) -> bool {
    ghostex_gx_core::protocol::cloud_box::cloud_box_id_from_machine_id(machine_id)
        .is_some_and(|box_id| links(|links| links.states.contains_key(box_id)))
}

/// Where a chat request to a session on a box goes (Q1013: "On sending a message there. Not on
/// opening the session."). `Ok(None)` for a machine that is not a box. A send to a paused or
/// stopped box asks home gxserver to `resume` it and waits until this client is connected to it;
/// every other request to a box that is not connected is refused with the reason, never sent to
/// this computer's gxserver, which does not hold the box's sessions.
pub(crate) async fn cloud_box_send_target(
    machine_id: &str,
    wake: bool,
) -> Result<Option<GpuiRemoteGxserverRequestTarget>, CloudBoxRefusal> {
    let Some(box_id) =
        ghostex_gx_core::protocol::cloud_box::cloud_box_id_from_machine_id(machine_id)
    else {
        return Ok(None);
    };
    if let Some(target) = cloud_box_request_target(machine_id) {
        return Ok(Some(target));
    }
    let state = links(|links| links.states.get(box_id).cloned());
    let Some(state) = state else {
        return Err(refusal(UNAVAILABLE, "This cloud box is no longer listed."));
    };
    if !wake {
        return Err(match state {
            state if state.wakes_on_send() => refusal(
                ASLEEP,
                "This cloud box is stopped. Sending a message wakes it.",
            ),
            _ => refusal(UNAVAILABLE, "This cloud box is not connected yet."),
        });
    }
    if state.wakes_on_send() && links(|links| links.waking.insert(box_id.to_string())) {
        let resumed = gx_rpc(
            None,
            CLOUD_BOXES_ROUTE,
            json!({ "action": "resume", "boxId": box_id }),
        )
        .await;
        links(|links| links.waking.remove(box_id));
        if let Err(error) = resumed {
            return Err(refusal(
                UNAVAILABLE,
                &format!("Could not wake the cloud box: {}", error.message),
            ));
        }
    }
    let deadline = Instant::now() + WAKE_WAIT;
    while Instant::now() < deadline {
        if let Some(target) = cloud_box_request_target(machine_id) {
            return Ok(Some(target));
        }
        if links(|links| links.states.get(box_id) == Some(&CloudBoxState::Failed)) {
            return Err(refusal(UNAVAILABLE, "The cloud box failed to start."));
        }
        futures_timer(WAKE_POLL).await;
    }
    Err(refusal(
        UNAVAILABLE,
        "The cloud box did not come back in time.",
    ))
}

/// A timer that does not need an executor of its own: the send path awaits on the background
/// executor without a `cx`.
async fn futures_timer(duration: Duration) {
    let (sender, receiver) = futures::channel::oneshot::channel::<()>();
    let _ = std::thread::Builder::new()
        .name("ghostex-cloud-box-wait".into())
        .spawn(move || {
            std::thread::sleep(duration);
            let _ = sender.send(());
        });
    let _ = receiver.await;
}

impl GhostexGpuiApp {
    /// Reads home gxserver's box registry into the store (`list`). Asked when this computer's
    /// stream goes live and when a `cloudBoxesChanged` frame without a row asks for it.
    pub(super) fn gx_store_refresh_cloud_boxes(&mut self, cx: &mut gpui::Context<Self>) {
        if !crate::shared_settings::shared_sidebar_settings_snapshot().cloud_boxes_enabled() {
            return;
        }
        cx.spawn(async move |this, cx| {
            let answer = gx_rpc(None, CLOUD_BOXES_ROUTE, json!({ "action": "list" })).await;
            // A gxserver without Cloud Boxes, or one that failed, leaves the registry as it was.
            let Ok(answer) = answer else {
                return;
            };
            let boxes: Vec<CloudBox> = answer
                .get("boxes")
                .cloned()
                .and_then(|boxes| serde_json::from_value(boxes).ok())
                .unwrap_or_default();
            let _ = this.update(cx, |this, cx| {
                let output = this
                    .gx_store
                    .core
                    .handle(Event::CloudBoxesListed { boxes }, now_ms());
                this.gx_store.sidebar_list.note_changes(&output.changes);
                this.gx_store.run_effects(output.effects);
                if output.changes.cloud_boxes_changed {
                    this.gx_store_sync_remote_clients(true, cx);
                    this.gx_store_sidebar_state_changed(cx);
                }
            });
        })
        .detach();
    }

    /// Brings the box machines in line with the registry: connects every reachable box this
    /// client has no address for, drops the address of a box that went down, and lists the cached
    /// rows of a box that is not reachable. Called from the remote clients' reconcile, which then
    /// starts or stops the clients themselves.
    pub(super) fn gx_store_sync_cloud_box_links(&mut self, cx: &mut gpui::Context<Self>) {
        let boxes: Vec<CloudBox> = self.gx_store.core.cloud_boxes().iter().cloned().collect();
        let reachable: BTreeSet<String> = boxes
            .iter()
            .filter(|cloud_box| cloud_box.state.is_reachable())
            .map(|cloud_box| cloud_box.box_id.clone())
            .collect();
        let (gone, to_connect) = links(|links| {
            links.states = boxes
                .iter()
                .map(|cloud_box| (cloud_box.box_id.clone(), cloud_box.state.clone()))
                .collect();
            let gone: Vec<String> = links
                .targets
                .keys()
                .filter(|machine_id| {
                    box_id_of_machine(&MachineId::Remote((*machine_id).clone()))
                        .is_none_or(|box_id| !reachable.contains(box_id))
                })
                .cloned()
                .collect();
            for machine_id in &gone {
                links.targets.remove(machine_id);
            }
            let now = Instant::now();
            let to_connect: Vec<String> = reachable
                .iter()
                .filter(|box_id| !links.targets.contains_key(&cloud_box_machine_id(box_id)))
                .filter(|box_id| !links.connecting.contains(*box_id))
                .filter(|box_id| links.retry_at.get(*box_id).is_none_or(|at| now >= *at))
                .cloned()
                .collect();
            for box_id in &to_connect {
                links.connecting.insert(box_id.clone());
            }
            (gone, to_connect)
        });
        for machine_id in &gone {
            crate::app::helpers::gpui_stop_easy_connect_forward(machine_id);
        }
        for box_id in to_connect {
            self.gx_store_connect_cloud_box(box_id, cx);
        }
        for cloud_box in boxes
            .iter()
            .filter(|cloud_box| !cloud_box.state.is_reachable())
        {
            self.gx_store_seed_cloud_box_rows(cloud_box, cx);
        }
    }

    /// Asks home gxserver how to reach a box (`connectInfo`) and records its address.
    fn gx_store_connect_cloud_box(&mut self, box_id: String, cx: &mut gpui::Context<Self>) {
        cx.spawn(async move |this, cx| {
            let answer = gx_rpc(
                None,
                CLOUD_BOXES_ROUTE,
                json!({ "action": "connectInfo", "boxId": box_id }),
            )
            .await;
            let machine_id = cloud_box_machine_id(&box_id);
            let target = answer
                .map_err(|error| error.message.to_string())
                .and_then(|answer| {
                    serde_json::from_value::<CloudBoxConnectInfo>(answer)
                        .map_err(|error| error.to_string())
                })
                .and_then(|info| cloud_box_local_port(&machine_id, &info).map(|port| (port, info)));
            let _ = this.update(cx, |this, cx| {
                let connected = links(|links| {
                    links.connecting.remove(&box_id);
                    match target {
                        Ok((port, info)) => {
                            links.retry_at.remove(&box_id);
                            links
                                .targets
                                .insert(machine_id.clone(), (port, info.gxserver_token));
                            true
                        }
                        Err(_) => {
                            links
                                .retry_at
                                .insert(box_id.clone(), Instant::now() + CONNECT_RETRY);
                            false
                        }
                    }
                });
                if connected {
                    this.gx_store_sync_remote_clients(true, cx);
                    this.refresh_session_chat_runtime_endpoints(false, cx);
                    // A restored pane of this box re-arms now, as a saved machine's does when its
                    // connect lands (os_integration/toast_and_status_dispatch.rs).
                    this.attach_surfaced_remote_workspace_terminals(&machine_id, cx);
                } else {
                    // Asked again once the retry time has passed.
                    cx.spawn(async move |this, cx| {
                        cx.background_executor().timer(CONNECT_RETRY).await;
                        let _ = this.update(cx, |this, cx| {
                            this.gx_store_sync_remote_clients(true, cx);
                        });
                    })
                    .detach();
                }
            });
        })
        .detach();
    }

    /// Lists a box's cached rows while it is not reachable, through the store's last-seen seed:
    /// faded, read-only rows that open without waking the box. A machine the store already holds
    /// (it was connected earlier in this run) keeps its own rows; the seed refuses them anyway.
    fn gx_store_seed_cloud_box_rows(&mut self, cloud_box: &CloudBox, cx: &mut gpui::Context<Self>) {
        let machine = cloud_box_machine(&cloud_box.box_id);
        if self.gx_store.core.presentation().loaded(&machine).is_some() {
            return;
        }
        if !links(|links| links.seeded.insert(cloud_box.box_id.clone())) {
            return;
        }
        let Some(snapshot) = cached_box_snapshot(cloud_box) else {
            links(|links| links.seeded.remove(&cloud_box.box_id));
            return;
        };
        let output = self
            .gx_store
            .core
            .seed_last_seen_presentation(&machine, snapshot);
        if output.changes.is_empty() {
            return;
        }
        self.gx_store.sidebar_list.note_changes(&output.changes);
        self.gx_store.run_effects(output.effects);
        self.gx_store_sidebar_state_changed(cx);
    }

    /// A box machine's rows moved: report its sessions to home gxserver (`reportBoxSessions`) so a
    /// stopped box still lists them, on every client, and the archive rule can read them.
    pub(super) fn gx_store_note_cloud_box_sessions(
        &mut self,
        machine_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let machine = MachineId::Remote(machine_id.to_string());
        let Some(box_id) = box_id_of_machine(&machine).map(str::to_string) else {
            return;
        };
        if !links(|links| links.report_due.insert(box_id.clone())) {
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(REPORT_DEBOUNCE).await;
            let _ = this.update(cx, |this, cx| {
                links(|links| links.report_due.remove(&box_id));
                this.gx_store_report_cloud_box_sessions(&box_id, cx);
            });
        })
        .detach();
    }

    fn gx_store_report_cloud_box_sessions(&mut self, box_id: &str, cx: &mut gpui::Context<Self>) {
        let machine = cloud_box_machine(box_id);
        let store = self.gx_store.core.presentation();
        // Only rows the box itself sent: a seeded copy is what home gxserver already holds.
        if store.loaded(&machine).is_none_or(|loaded| loaded.last_seen) {
            return;
        }
        let Some(entry) = store.machine(&machine) else {
            return;
        };
        let mut rows: Vec<CloudBoxSessionRow> = Vec::new();
        if let Some(presentation) = entry.loaded() {
            for group in presentation.groups() {
                for session_id in &group.session_ids {
                    let Some(session) = entry.effective_session(&group.project_id, session_id)
                    else {
                        continue;
                    };
                    let runtime = session.cloud_box.as_ref();
                    rows.push(CloudBoxSessionRow {
                        project_id: group.project_id.clone(),
                        session_id: session_id.clone(),
                        title: session.title.clone(),
                        agent_id: session.agent_id.clone(),
                        branch: session_branch(&session),
                        agent_session_id: session.agent_session_id.clone(),
                        last_active_at: Some(session.updated_at.clone())
                            .filter(|updated_at| !updated_at.is_empty()),
                        home_session_id: None,
                        archived: !session.listed_in_sidebar() && runtime.is_some(),
                    });
                }
            }
        }
        let unchanged = links(|links| links.reported.get(box_id) == Some(&rows));
        if unchanged {
            return;
        }
        let box_id = box_id.to_string();
        cx.spawn(async move |_, _| {
            let sent = gx_rpc(
                None,
                CLOUD_BOXES_ROUTE,
                json!({ "action": "reportBoxSessions", "boxId": box_id, "sessions": rows }),
            )
            .await;
            if sent.is_ok() {
                links(|links| links.reported.insert(box_id, rows));
            }
        })
        .detach();
    }
}

/// The loopback port a box's gxserver answers on for this client: the port Docker published for
/// a `local` box, or the Easy Connect forwarder's port for a box reached over tailcat.
fn cloud_box_local_port(machine_id: &str, info: &CloudBoxConnectInfo) -> Result<u16, String> {
    match info.transport.as_str() {
        CLOUD_BOX_TRANSPORT_DIRECT => info
            .base_url
            .as_deref()
            .and_then(|base_url| base_url.rsplit_once(':'))
            .and_then(|(_, port)| port.trim_end_matches('/').parse::<u16>().ok())
            .ok_or_else(|| "The box's address has no port.".to_string()),
        CLOUD_BOX_TRANSPORT_TAILCAT => {
            let address = info
                .tailcat_address
                .as_deref()
                .ok_or_else(|| "The box has no tailcat address yet.".to_string())?;
            let port = u16::try_from(info.gxserver_port)
                .map_err(|_| "The box's gxserver port is invalid.".to_string())?;
            crate::app::helpers::gpui_start_easy_connect_forward(machine_id, address, port)
        }
        other => Err(format!("Unknown box transport {other}.")),
    }
}

/// The branch a box session works on, as its git row says.
fn session_branch(session: &ghostex_gx_core::protocol::PresentationSession) -> Option<String> {
    let value: Value = serde_json::to_value(session.git_status.as_ref()?).ok()?;
    value
        .get("branch")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Whether a session key is on a box machine (for callers outside the store).
#[allow(dead_code)]
pub(crate) fn is_cloud_box_session(session: &ghostex_gx_core::SessionKey) -> bool {
    is_cloud_box_machine(&session.machine)
}
