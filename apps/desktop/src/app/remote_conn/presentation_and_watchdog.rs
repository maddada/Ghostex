use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering;
use std::time::Duration;

use futures::StreamExt as _;
use futures::channel::mpsc;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn refresh_gpui_remote_gxserver_presentation_in_background(
        &mut self,
        remote_machine_id: String,
        mark_failed_on_error: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(target) = self.gpui_remote_gxserver_request_target(&remote_machine_id) else {
            return;
        };
        let refresh_stream_generation = self
            .remote_gxserver_connections
            .get(&remote_machine_id)
            .and_then(|connection| connection.presentation_stream_generation);
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    gpui_remote_gxserver_rpc_result(
                        &target,
                        "/api/readPresentationSnapshot",
                        &serde_json::json!({}),
                        Duration::from_secs(15),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if !refresh_stream_generation.is_some_and(|generation| {
                    !this.gpui_remote_gxserver_presentation_stream_is_current(
                        remote_machine_id.as_str(),
                        generation,
                    )
                }) {
                    match result {
                        Ok(result) => {
                            if let Some(snapshot) = result.get("snapshot").cloned() {
                                this.dispatch_gpui_sidebar_remote_event(
                                    serde_json::json!({
                                        "payload": {
                                            "snapshot": snapshot,
                                            "type": "presentationSnapshot",
                                        },
                                        "remoteMachineId": remote_machine_id.as_str(),
                                        "type": "remoteGxserverPresentation",
                                    }),
                                    cx,
                                );
                            } else if mark_failed_on_error {
                                this.dispatch_gpui_remote_machine_status(
                                    remote_machine_id.as_str(),
                                    "failed",
                                    cx,
                                );
                            }
                        }
                        Err(_) if mark_failed_on_error => {
                            this.dispatch_gpui_remote_machine_status(
                                remote_machine_id.as_str(),
                                "failed",
                                cx,
                            );
                        }
                        Err(_) => {}
                    }
                }
            });
        })
        .detach();
    }

    pub(crate) fn next_gpui_remote_gxserver_connect_generation(
        &mut self,
        remote_machine_id: &str,
    ) -> u64 {
        let generation = self
            .remote_gxserver_connect_generations
            .entry(remote_machine_id.to_string())
            .or_insert(0);
        *generation = generation.wrapping_add(1);
        if *generation == 0 {
            *generation = 1;
        }
        *generation
    }

    pub(crate) fn gpui_remote_gxserver_connect_generation_is_current(
        &self,
        remote_machine_id: &str,
        generation: u64,
    ) -> bool {
        self.remote_gxserver_connect_generations
            .get(remote_machine_id)
            .copied()
            == Some(generation)
    }

    pub(crate) fn next_gpui_remote_gxserver_presentation_stream_generation(&mut self) -> u64 {
        self.remote_gxserver_presentation_stream_generation = self
            .remote_gxserver_presentation_stream_generation
            .wrapping_add(1);
        if self.remote_gxserver_presentation_stream_generation == 0 {
            self.remote_gxserver_presentation_stream_generation = 1;
        }
        self.remote_gxserver_presentation_stream_generation
    }

    pub(crate) fn gpui_remote_gxserver_presentation_stream_is_current(
        &self,
        remote_machine_id: &str,
        generation: u64,
    ) -> bool {
        self.remote_gxserver_connections
            .get(remote_machine_id)
            .and_then(|connection| connection.presentation_stream_generation)
            == Some(generation)
    }

    pub(crate) fn restart_gpui_remote_gxserver_presentation_stream(
        &mut self,
        remote_machine_id: String,
        client_id: String,
        last_revision: Option<u64>,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        if !self
            .remote_gxserver_connections
            .contains_key(remote_machine_id.as_str())
        {
            return false;
        }
        let generation = self.next_gpui_remote_gxserver_presentation_stream_generation();
        let cancel = Arc::new(AtomicBool::new(false));
        let target = {
            let Some(connection) = self
                .remote_gxserver_connections
                .get_mut(remote_machine_id.as_str())
            else {
                return false;
            };
            if let Some(previous_cancel) = connection.presentation_stream_cancel.as_ref() {
                previous_cancel.store(true, Ordering::SeqCst);
            }
            let target = connection.request_target();
            connection.presentation_stream_cancel = Some(cancel.clone());
            connection.presentation_stream_generation = Some(generation);
            target
        };
        self.start_gpui_remote_gxserver_presentation_stream(
            remote_machine_id,
            target,
            generation,
            cancel,
            client_id,
            last_revision,
            cx,
        );
        true
    }

    pub(crate) fn start_gpui_remote_gxserver_presentation_stream(
        &mut self,
        remote_machine_id: String,
        target: GpuiRemoteGxserverRequestTarget,
        generation: u64,
        cancel: Arc<AtomicBool>,
        client_id: String,
        last_revision: Option<u64>,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUIRemotePresentationStreaming 2026-06-24-19:54:
        A connected remote machine needs the same live gxserver presentation contract as local GPUI, but CEF must not receive remote base URLs or bearer tokens. Rust opens `/api/events` through the localhost SSH tunnel, subscribes with the shared sidebar client id, and forwards only sanitized snapshot/delta payloads. A terminal stream failure enters the shared broken-status funnel, which tears down the stale tunnel before the sidebar schedules a full reconnect.
        */
        let (tx, mut rx) = mpsc::unbounded::<GpuiRemoteGxserverPresentationStreamMessage>();
        let background = cx.background_executor().clone();
        background
            .spawn(async move {
                gpui_remote_gxserver_presentation_stream_loop(
                    target,
                    cancel,
                    tx,
                    client_id,
                    last_revision,
                );
            })
            .detach();
        cx.spawn(async move |this, cx| {
            while let Some(message) = rx.next().await {
                let should_continue = this
                    .update(cx, |this, cx| {
                        if !this.gpui_remote_gxserver_presentation_stream_is_current(
                            remote_machine_id.as_str(),
                            generation,
                        ) {
                            return false;
                        }
                        match message {
                            GpuiRemoteGxserverPresentationStreamMessage::Event(payload) => {
                                this.dispatch_gpui_sidebar_remote_event(
                                    serde_json::json!({
                                        "payload": payload,
                                        "remoteMachineId": remote_machine_id.as_str(),
                                        "type": "remoteGxserverPresentation",
                                    }),
                                    cx,
                                );
                                true
                            }
                            GpuiRemoteGxserverPresentationStreamMessage::Failed => {
                                this.dispatch_gpui_remote_machine_status(
                                    remote_machine_id.as_str(),
                                    GpuiRemoteGxserverConnectState::PresentationStreamFailed
                                        .wire_status_state(),
                                    cx,
                                );
                                false
                            }
                        }
                    })
                    .unwrap_or(false);
                if !should_continue {
                    break;
                }
            }
        })
        .detach();
    }

    pub(crate) fn start_gpui_remote_gxserver_watchdog(&mut self, cx: &mut gpui::Context<Self>) {
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor()
                    .timer(GPUI_REMOTE_GXSERVER_WATCHDOG_INTERVAL)
                    .await;
                if this
                    .update(cx, |this, cx| {
                        this.validate_gpui_remote_gxserver_connections(false, cx);
                    })
                    .is_err()
                {
                    break;
                }
            }
        })
        .detach();
    }

    pub(crate) fn validate_gpui_remote_gxserver_connections(
        &mut self,
        wake_validation: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let exited_machine_ids = self
            .remote_gxserver_connections
            .iter_mut()
            .filter_map(|(machine_id, connection)| {
                (!matches!(connection.child.try_wait(), Ok(None))).then(|| machine_id.clone())
            })
            .collect::<Vec<_>>();
        for machine_id in exited_machine_ids {
            self.stop_gpui_remote_gxserver_connection(machine_id.as_str());
            self.dispatch_gpui_remote_machine_status_with_message(
                machine_id.as_str(),
                "disconnected",
                Some("The remote SSH tunnel disconnected."),
                cx,
            );
        }

        if !wake_validation && self.remote_gxserver_watchdog_probe_in_flight {
            return;
        }
        let probes = self
            .remote_gxserver_connections
            .iter()
            .filter_map(|(machine_id, connection)| {
                let generation = self
                    .remote_gxserver_connect_generations
                    .get(machine_id)
                    .copied()?;
                Some((machine_id.clone(), generation, connection.request_target()))
            })
            .collect::<Vec<_>>();
        if probes.is_empty() {
            return;
        }
        if !wake_validation {
            self.remote_gxserver_watchdog_probe_in_flight = true;
        }
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let results = background
                .spawn(async move {
                    probes
                        .into_iter()
                        .map(|(machine_id, generation, target)| {
                            let healthy = gpui_remote_authenticated_health(
                                target.local_port,
                                target.token.as_str(),
                            )
                            .is_some();
                            (machine_id, generation, target.local_port, healthy)
                        })
                        .collect::<Vec<_>>()
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if !wake_validation {
                    this.remote_gxserver_watchdog_probe_in_flight = false;
                }
                let mut failed_machine_ids = Vec::new();
                for (machine_id, generation, local_port, healthy) in results {
                    if !this.gpui_remote_gxserver_connect_generation_is_current(
                        machine_id.as_str(),
                        generation,
                    ) {
                        continue;
                    }
                    let Some(connection) = this
                        .remote_gxserver_connections
                        .get_mut(machine_id.as_str())
                    else {
                        continue;
                    };
                    if connection.local_port != local_port {
                        continue;
                    }
                    if healthy {
                        connection.health_check_failures = 0;
                        continue;
                    }
                    connection.health_check_failures =
                        connection.health_check_failures.saturating_add(1);
                    if wake_validation
                        || connection.health_check_failures
                            >= GPUI_REMOTE_GXSERVER_WATCHDOG_FAILURE_THRESHOLD
                    {
                        failed_machine_ids.push(machine_id);
                    }
                }
                for machine_id in failed_machine_ids {
                    this.stop_gpui_remote_gxserver_connection(machine_id.as_str());
                    this.dispatch_gpui_remote_machine_status_with_message(
                        machine_id.as_str(),
                        "disconnected",
                        Some("The remote gxserver tunnel stopped responding."),
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    pub(crate) fn stop_gpui_remote_gxserver_connection(&mut self, remote_machine_id: &str) {
        if let Some(mut connection) = self.remote_gxserver_connections.remove(remote_machine_id) {
            connection.terminate();
        }
        // The Easy Connect forwarder shares the machine's connection lifetime.
        gpui_stop_easy_connect_forward(remote_machine_id);
    }

    pub(crate) fn stop_all_gpui_remote_gxserver_connections(&mut self) {
        for (_, mut connection) in self.remote_gxserver_connections.drain() {
            connection.terminate();
        }
        gpui_stop_all_easy_connect_forwards();
    }
}
