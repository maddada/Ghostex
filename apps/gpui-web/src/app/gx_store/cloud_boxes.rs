//! Cloud Boxes in the browser build: the desktop's `gx_store/cloud_boxes.rs` with the page's
//! transport. Home gxserver's box registry is read into the store, every reachable box is a hidden
//! machine (`cloudbox:<boxId>`) fed by its own event socket, a box that is not reachable lists its
//! cached rows, and a chat send to a stopped box wakes it first.
//!
//! A page cannot run the Easy Connect forwarder, so it reaches only boxes whose `connectInfo` is
//! `direct` (the `local` Docker provider publishes on this computer's loopback, which gxserver's
//! CORS rule lets a loopback page call); a `tailcat` box is listed from its cached rows.
//!
//! SEE-ALSO: apps/desktop/src/app/gx_store/cloud_boxes.rs (the desktop host),
//! packages/gx-core/src/cloud_boxes.rs (registry, merge rule, cached rows).

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};

use futures::StreamExt as _;
use futures::channel::mpsc;
use ghostex_gx_core::cloud_boxes::{box_id_of_machine, cached_box_snapshot, cloud_box_machine};
use ghostex_gx_core::protocol::ClientMessage;
use ghostex_gx_core::protocol::cloud_box::{
    CLOUD_BOX_TRANSPORT_DIRECT, CloudBox, CloudBoxConnectInfo, CloudBoxSessionRow, CloudBoxState,
    cloud_box_id_from_machine_id, cloud_box_machine_id,
};
use ghostex_gx_core::{ConnectionUpdate, Event, MachineId};
use serde_json::json;

use super::host::now_ms;
use super::web_transport::{self, GxserverEndpoint, StreamEvent};
use crate::GhostexGpuiApp;
use crate::app::model::GpuiRemoteGxserverRequestTarget;

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
const RETRY_MS: f64 = 5_000.0;
const WAKE_WAIT_MS: f64 = 5.0 * 60_000.0;

/// One connected box: its gxserver and the socket that feeds the store.
struct BoxLink {
    endpoint: GxserverEndpoint,
    socket: Option<web_sys::WebSocket>,
}

#[derive(Default)]
struct CloudBoxLinks {
    states: BTreeMap<String, CloudBoxState>,
    /// Connected boxes by machine id.
    links: BTreeMap<String, BoxLink>,
    connecting: BTreeSet<String>,
    retry_at: BTreeMap<String, f64>,
    seeded: BTreeSet<String>,
    reported: BTreeMap<String, Vec<CloudBoxSessionRow>>,
    waking: BTreeSet<String>,
}

thread_local! {
    static LINKS: RefCell<CloudBoxLinks> = RefCell::new(CloudBoxLinks::default());
}

fn links<T>(apply: impl FnOnce(&mut CloudBoxLinks) -> T) -> T {
    LINKS.with(|links| apply(&mut links.borrow_mut()))
}

/// The gxserver of a connected box, for the page's chat calls and its terminal.
pub(crate) fn cloud_box_endpoint(machine_id: &str) -> Option<GxserverEndpoint> {
    links(|links| {
        links
            .links
            .get(machine_id)
            .map(|link| link.endpoint.clone())
    })
}

/// The desktop's name for the same lookup, in the page's target shape.
pub(crate) fn cloud_box_request_target(
    machine_id: &str,
) -> Option<GpuiRemoteGxserverRequestTarget> {
    let endpoint = cloud_box_endpoint(machine_id)?;
    let port = endpoint.base_url.rsplit_once(':')?.1.parse::<u16>().ok()?;
    Some(GpuiRemoteGxserverRequestTarget {
        local_port: port,
        token: endpoint.auth_token,
    })
}

/// The desktop's `cloud_box_send_target`: a send to a paused or stopped box resumes it and waits
/// for this page to connect; any other call to a box that is not connected is refused, never sent
/// to this computer's gxserver.
pub(crate) async fn cloud_box_send_target(
    machine_id: &str,
    wake: bool,
) -> Result<Option<GpuiRemoteGxserverRequestTarget>, CloudBoxRefusal> {
    let Some(box_id) = cloud_box_id_from_machine_id(machine_id).map(str::to_string) else {
        return Ok(None);
    };
    let machine_id = machine_id.to_string();
    // The page's calls are page tasks; this runs on the background executor, so it hops over.
    let (sender, receiver) = futures::channel::oneshot::channel();
    wasm_bindgen_futures::spawn_local(async move {
        let _ = sender.send(send_target_on_page(&machine_id, &box_id, wake).await);
    });
    receiver
        .await
        .unwrap_or_else(|_| Err(refusal(UNAVAILABLE, "The request was dropped.")))
}

async fn send_target_on_page(
    machine_id: &str,
    box_id: &str,
    wake: bool,
) -> Result<Option<GpuiRemoteGxserverRequestTarget>, CloudBoxRefusal> {
    if let Some(target) = cloud_box_request_target(machine_id) {
        return Ok(Some(target));
    }
    let Some(state) = links(|links| links.states.get(box_id).cloned()) else {
        return Err(refusal(UNAVAILABLE, "This cloud box is no longer listed."));
    };
    if !wake {
        return Err(if state.wakes_on_send() {
            refusal(
                ASLEEP,
                "This cloud box is stopped. Sending a message wakes it.",
            )
        } else {
            refusal(UNAVAILABLE, "This cloud box is not connected yet.")
        });
    }
    if state.wakes_on_send() && links(|links| links.waking.insert(box_id.to_string())) {
        let resumed = super::gx_rpc(
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
    let started = js_sys::Date::now();
    while js_sys::Date::now() - started < WAKE_WAIT_MS {
        if let Some(target) = cloud_box_request_target(machine_id) {
            return Ok(Some(target));
        }
        if links(|links| links.states.get(box_id) == Some(&CloudBoxState::Failed)) {
            return Err(refusal(UNAVAILABLE, "The cloud box failed to start."));
        }
        sleep_ms(500).await;
    }
    Err(refusal(
        UNAVAILABLE,
        "The cloud box did not come back in time.",
    ))
}

async fn sleep_ms(ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _reject| {
        if let Some(window) = web_sys::window() {
            let _ = window.set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms);
        }
    });
    let _ = wasm_bindgen_futures::JsFuture::from(promise).await;
}

/// Asks a connected box for a full snapshot on its socket (the store's `ResubscribePresentation`
/// for that machine). A box with no socket gets one when it connects.
pub(crate) fn cloud_box_resubscribe(machine_id: &str) {
    let message = ClientMessage::SubscribePresentation {
        client_id: Some("ghostex-gpui-web".to_string()),
        last_revision: None,
        renderer_commands: None,
    };
    let Ok(text) = serde_json::to_string(&message) else {
        return;
    };
    links(|links| {
        if let Some(socket) = links
            .links
            .get(machine_id)
            .and_then(|link| link.socket.as_ref())
        {
            let _ = socket.send_with_str(&text);
        }
    });
}

impl GhostexGpuiApp {
    /// Reads home gxserver's box registry (`list`) into the store.
    pub(crate) fn gx_store_refresh_cloud_boxes(&mut self, cx: &mut gpui::Context<Self>) {
        cx.spawn(async move |app, cx| {
            let Ok(answer) =
                super::gx_rpc(None, CLOUD_BOXES_ROUTE, json!({ "action": "list" })).await
            else {
                return;
            };
            let boxes: Vec<CloudBox> = answer
                .get("boxes")
                .cloned()
                .and_then(|boxes| serde_json::from_value(boxes).ok())
                .unwrap_or_default();
            let _ = app.update(cx, |app, cx| {
                app.gx_store_handle(Event::CloudBoxesListed { boxes }, cx);
            });
        })
        .detach();
    }

    /// Connects every reachable box, drops a box that went down, lists the cached rows of the
    /// rest. Run after every store change that moved the registry.
    pub(crate) fn gx_store_sync_cloud_boxes(&mut self, cx: &mut gpui::Context<Self>) {
        let boxes: Vec<CloudBox> = self.gx_store.core.cloud_boxes().iter().cloned().collect();
        let reachable: BTreeSet<String> = boxes
            .iter()
            .filter(|cloud_box| cloud_box.state.is_reachable())
            .map(|cloud_box| cloud_box.box_id.clone())
            .collect();
        let now = js_sys::Date::now();
        let (gone, to_connect) = links(|links| {
            links.states = boxes
                .iter()
                .map(|cloud_box| (cloud_box.box_id.clone(), cloud_box.state.clone()))
                .collect();
            let gone: Vec<String> = links
                .links
                .keys()
                .filter(|machine_id| {
                    cloud_box_id_from_machine_id(machine_id)
                        .is_none_or(|box_id| !reachable.contains(box_id))
                })
                .cloned()
                .collect();
            for machine_id in &gone {
                if let Some(socket) = links.links.remove(machine_id).and_then(|link| link.socket) {
                    let _ = socket.close();
                }
            }
            let to_connect: Vec<String> = reachable
                .iter()
                .filter(|box_id| !links.links.contains_key(&cloud_box_machine_id(box_id)))
                .filter(|box_id| !links.connecting.contains(*box_id))
                .filter(|box_id| links.retry_at.get(*box_id).is_none_or(|at| now >= *at))
                .cloned()
                .collect();
            for box_id in &to_connect {
                links.connecting.insert(box_id.clone());
            }
            (gone, to_connect)
        });
        for machine_id in gone {
            self.gx_store_handle(
                Event::Connection {
                    machine: MachineId::Remote(machine_id),
                    update: ConnectionUpdate::Lost {
                        error: Some("The cloud box stopped.".to_string()),
                    },
                },
                cx,
            );
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

    fn gx_store_connect_cloud_box(&mut self, box_id: String, cx: &mut gpui::Context<Self>) {
        cx.spawn(async move |app, cx| {
            let answer = super::gx_rpc(
                None,
                CLOUD_BOXES_ROUTE,
                json!({ "action": "connectInfo", "boxId": box_id }),
            )
            .await;
            let endpoint = answer
                .ok()
                .and_then(|answer| serde_json::from_value::<CloudBoxConnectInfo>(answer).ok())
                .filter(|info| info.transport == CLOUD_BOX_TRANSPORT_DIRECT)
                .and_then(|info| {
                    Some(GxserverEndpoint {
                        base_url: info.base_url?.trim_end_matches('/').to_string(),
                        auth_token: info.gxserver_token,
                    })
                });
            let _ = app.update(cx, |app, cx| {
                links(|links| links.connecting.remove(&box_id));
                match endpoint {
                    Some(endpoint) => app.gx_store_open_cloud_box(&box_id, endpoint, cx),
                    None => {
                        links(|links| {
                            links
                                .retry_at
                                .insert(box_id.clone(), js_sys::Date::now() + RETRY_MS)
                        });
                    }
                }
            });
        })
        .detach();
    }

    /// Opens the box's event socket and pumps its frames into the store under its machine.
    fn gx_store_open_cloud_box(
        &mut self,
        box_id: &str,
        endpoint: GxserverEndpoint,
        cx: &mut gpui::Context<Self>,
    ) {
        let machine_id = cloud_box_machine_id(box_id);
        let machine = MachineId::Remote(machine_id.clone());
        let (sender, mut receiver) = mpsc::unbounded();
        let socket = match web_transport::open_events(&endpoint, sender) {
            Ok(socket) => socket,
            Err(_) => {
                links(|links| {
                    links
                        .retry_at
                        .insert(box_id.to_string(), js_sys::Date::now() + RETRY_MS)
                });
                return;
            }
        };
        // The chat host's socket for this machine (`app/gx_chat/`).
        crate::app::gx_chat::set_endpoint(&machine_id, &endpoint.base_url, &endpoint.auth_token);
        links(|links| {
            links.links.insert(
                machine_id.clone(),
                BoxLink {
                    endpoint,
                    socket: Some(socket),
                },
            )
        });
        self.gx_store_handle(
            Event::Connection {
                machine: machine.clone(),
                update: ConnectionUpdate::Connecting { attempt: 0 },
            },
            cx,
        );
        cx.spawn(async move |app, cx| {
            while let Some(event) = receiver.next().await {
                let closed = matches!(event, StreamEvent::Closed);
                let alive = app.update(cx, |app, cx| {
                    app.gx_store_cloud_box_event(&machine_id, event, cx)
                });
                if alive.is_err() || closed {
                    break;
                }
            }
        })
        .detach();
    }

    fn gx_store_cloud_box_event(
        &mut self,
        machine_id: &str,
        event: StreamEvent,
        cx: &mut gpui::Context<Self>,
    ) {
        let machine = MachineId::Remote(machine_id.to_string());
        match event {
            StreamEvent::Open => {
                let message = ClientMessage::SubscribePresentation {
                    client_id: Some("ghostex-gpui-web".to_string()),
                    last_revision: None,
                    renderer_commands: None,
                };
                links(|links| {
                    if let (Some(socket), Ok(text)) = (
                        links
                            .links
                            .get(machine_id)
                            .and_then(|link| link.socket.as_ref()),
                        serde_json::to_string(&message),
                    ) {
                        let _ = socket.send_with_str(&text);
                    }
                });
            }
            StreamEvent::Frame(text) => {
                if text.contains("\"type\":\"apiRequestHandled\"") || text.contains("\"sessionChat")
                {
                    return;
                }
                if let Ok(output) =
                    self.gx_store
                        .core
                        .handle_raw_frame(machine.clone(), &text, now_ms())
                {
                    let moved = !output.changes.is_empty();
                    self.gx_store_apply_output(output, cx);
                    if moved {
                        self.gx_store_report_cloud_box_sessions(machine_id, cx);
                    }
                }
            }
            StreamEvent::Closed => {
                links(|links| {
                    links.links.remove(machine_id);
                    if let Some(box_id) = cloud_box_id_from_machine_id(machine_id) {
                        links
                            .retry_at
                            .insert(box_id.to_string(), js_sys::Date::now() + RETRY_MS);
                    }
                });
                self.gx_store_handle(
                    Event::Connection {
                        machine,
                        update: ConnectionUpdate::Lost { error: None },
                    },
                    cx,
                );
            }
        }
    }

    fn gx_store_seed_cloud_box_rows(&mut self, cloud_box: &CloudBox, cx: &mut gpui::Context<Self>) {
        let machine = cloud_box_machine(&cloud_box.box_id);
        if self.gx_store.core.presentation().loaded(&machine).is_some()
            || !links(|links| links.seeded.insert(cloud_box.box_id.clone()))
        {
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
        self.gx_store_apply_output(output, cx);
    }

    /// Reports a box's sessions to home gxserver so a stopped box still lists them.
    fn gx_store_report_cloud_box_sessions(
        &mut self,
        machine_id: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let machine = MachineId::Remote(machine_id.to_string());
        let Some(box_id) = box_id_of_machine(&machine).map(str::to_string) else {
            return;
        };
        let store = self.gx_store.core.presentation();
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
                    rows.push(CloudBoxSessionRow {
                        project_id: group.project_id.clone(),
                        session_id: session_id.clone(),
                        title: session.title.clone(),
                        agent_id: session.agent_id.clone(),
                        branch: None,
                        agent_session_id: session.agent_session_id.clone(),
                        last_active_at: Some(session.updated_at.clone())
                            .filter(|updated_at| !updated_at.is_empty()),
                        home_session_id: None,
                        archived: false,
                    });
                }
            }
        }
        if links(|links| links.reported.get(&box_id) == Some(&rows)) {
            return;
        }
        links(|links| links.reported.insert(box_id.clone(), rows.clone()));
        cx.spawn(async move |_, _| {
            let _ = super::gx_rpc(
                None,
                CLOUD_BOXES_ROUTE,
                json!({ "action": "reportBoxSessions", "boxId": box_id, "sessions": rows }),
            )
            .await;
        })
        .detach();
    }
}
