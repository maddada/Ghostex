use crate::*;

impl GhostexGpuiApp {
    fn session_chat_runtime_key(&self, generation: u64) -> Option<GpuiWorkspaceTerminalSessionKey> {
        self.agents_chat_page_states
            .values()
            .chain(
                self.parked_agents_chat_runtimes_by_project
                    .values()
                    .flat_map(|parked| parked.page_states.values()),
            )
            .find(|state| state.generation == generation)
            .and_then(|state| state.account_key.clone())
    }

    pub(crate) fn cache_session_chat_runtime_snapshot(
        &mut self,
        generation: u64,
        snapshot: &serde_json::Value,
    ) {
        let Some(key) = self.session_chat_runtime_key(generation) else {
            return;
        };
        self.session_chat_shared_snapshots
            .retain(|(previous, _)| previous != &key);
        if !snapshot["messages"].is_array() || snapshot.to_string().len() > 768 * 1024 {
            return;
        }
        self.session_chat_shared_snapshots
            .push((key, snapshot.clone()));
        let excess = self.session_chat_shared_snapshots.len().saturating_sub(12);
        self.session_chat_shared_snapshots.drain(..excess);
    }

    pub(crate) fn cached_session_chat_runtime_snapshot(
        &self,
        key: Option<&GpuiWorkspaceTerminalSessionKey>,
    ) -> Option<serde_json::Value> {
        let key = key?;
        self.session_chat_shared_snapshots
            .iter()
            .rev()
            .find(|(candidate, _)| candidate == key)
            .map(|(_, snapshot)| snapshot.clone())
    }

    /// CDXC:SessionChat 2026-09-13 WHY:
    /// Packaged chat pages have opaque file origins, so the existing sidebar owns the shared cache and sockets instead of a SharedWorker or one cache per renderer.
    /// Derive the endpoint and conversation from the native binding; a delayed page request cannot select another machine or session.
    pub(crate) fn relay_session_chat_runtime_request(
        &mut self,
        generation: u64,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(key) = self.session_chat_runtime_key(generation) else {
            return;
        };
        let Some(method) = message["method"].as_str().filter(|method| {
            matches!(
                *method,
                "read"
                    | "seed"
                    | "subscribe"
                    | "unsubscribe"
                    | "reconnect"
                    | "endpoint"
                    | "adoptDrafts"
            )
        }) else {
            return;
        };
        let request_id = message["requestId"].as_str().unwrap_or_default();
        if request_id.len() > 100 {
            return;
        }
        let (machine_id, project_id, session_id, bootstrap) = match key {
            GpuiWorkspaceTerminalSessionKey::Local(key) => (
                "local".to_string(),
                key.project_id,
                key.session_id,
                self.sidebar_gxserver_bootstrap
                    .as_ref()
                    .map(|bootstrap| (bootstrap.base_url.clone(), bootstrap.auth_token.clone())),
            ),
            GpuiWorkspaceTerminalSessionKey::Remote(key) => {
                let bootstrap = self
                    .gpui_remote_gxserver_request_target(&key.remote_machine_id)
                    .map(|target| {
                        (
                            format!("http://127.0.0.1:{}", target.local_port),
                            target.token,
                        )
                    });
                (
                    key.remote_machine_id,
                    key.project_id,
                    key.session_id,
                    bootstrap,
                )
            }
        };
        let (base_url, auth_token) = bootstrap.unwrap_or_default();
        self.session_chat_broker_endpoints
            .insert(machine_id.clone(), (base_url.clone(), auth_token.clone()));
        let Some(epoch) = self.session_chat_broker_epoch.as_ref() else {
            self.dispatch_session_chat_generation_response(generation, "onSessionChatRuntimeMessage", &serde_json::json!({"kind":"response", "requestId":request_id,"error":"The shared chat service is starting."}), false, cx);
            return;
        };
        let mut params = serde_json::Map::new();
        for field in ["limit", "beforeOffset"] {
            if let Some(value) = message["params"][field].as_u64() {
                params.insert(field.to_string(), value.into());
            }
        }
        if method == "adoptDrafts" {
            let Some(drafts) = message["params"]["drafts"]
                .as_array()
                .filter(|drafts| drafts.len() <= 1000)
            else {
                return;
            };
            if drafts.iter().any(|draft| {
                !draft["content"].is_string()
                    || !draft["version"]["draftId"].is_string()
                    || draft["version"]["revision"].as_u64().is_none()
            }) {
                return;
            }
            params.insert(
                "drafts".to_string(),
                serde_json::Value::Array(drafts.clone()),
            );
        }
        let client_id = message["clientId"]
            .as_str()
            .filter(|id| id.len() <= 128)
            .unwrap_or_default();
        let payload = serde_json::json!({"clientId":client_id,"epoch":epoch,"generation":generation.to_string(),"requestId":request_id,"method":method,"params":params,"identity":{"machineId":machine_id,"projectId":project_id,"sessionId":session_id},"endpoint":{"baseUrl":base_url,"authToken":auth_token}});
        if let Some(sidebar) = self.sidebar.as_ref() {
            sidebar.update(cx, |sidebar, _| {
                sidebar.execute_app_owned_script(&format!(
                    "window.ghostexGpui?.onSessionChatRuntimeRequest?.({payload}); undefined;"
                ));
            });
        }
    }

    pub(crate) fn release_session_chat_runtime_subscription(
        &self,
        generation: u64,
        cx: &mut gpui::Context<Self>,
    ) {
        if let Some(sidebar) = self.sidebar.as_ref() {
            let payload = serde_json::json!({"epoch":self.session_chat_broker_epoch,"generation":generation.to_string(),"method":"release"});
            sidebar.update(cx, |sidebar, _| {
                sidebar.execute_app_owned_script(&format!(
                    "window.ghostexGpui?.onSessionChatRuntimeRequest?.({payload}); undefined;"
                ));
            });
        }
    }

    pub(crate) fn release_parked_session_chat_runtime_subscriptions(
        &self,
        parked: &ParkedAgentsChatRuntime,
        cx: &mut gpui::Context<Self>,
    ) {
        for state in parked.page_states.values() {
            self.release_session_chat_runtime_subscription(state.generation, cx);
        }
    }

    pub(crate) fn refresh_session_chat_runtime_endpoints(
        &mut self,
        force: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(epoch) = self.session_chat_broker_epoch.clone() else {
            return;
        };
        let Some(sidebar) = self.sidebar.clone() else {
            return;
        };
        let machines = self
            .session_chat_broker_endpoints
            .keys()
            .cloned()
            .collect::<Vec<_>>();
        for machine_id in machines {
            let endpoint = if machine_id == "local" {
                self.sidebar_gxserver_bootstrap
                    .as_ref()
                    .map(|bootstrap| (bootstrap.base_url.clone(), bootstrap.auth_token.clone()))
            } else {
                self.gpui_remote_gxserver_request_target(&machine_id)
                    .map(|target| {
                        (
                            format!("http://127.0.0.1:{}", target.local_port),
                            target.token,
                        )
                    })
            }
            .unwrap_or_default();
            if !force && self.session_chat_broker_endpoints.get(&machine_id) == Some(&endpoint) {
                continue;
            }
            let payload = serde_json::json!({"epoch":epoch,"method":"machineEndpoint","machineId":machine_id,"endpoint":{"baseUrl":endpoint.0,"authToken":endpoint.1}});
            self.session_chat_broker_endpoints
                .insert(machine_id, endpoint);
            sidebar.update(cx, |sidebar, _| {
                sidebar.execute_app_owned_script(&format!(
                    "window.ghostexGpui?.onSessionChatRuntimeRequest?.({payload}); undefined;"
                ));
            });
        }
    }

    pub(crate) fn receive_session_chat_runtime_broker(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(epoch) = message["epoch"]
            .as_str()
            .filter(|epoch| !epoch.is_empty() && epoch.len() < 100)
        else {
            return;
        };
        if message["kind"] == "ready" {
            if self.session_chat_broker_epoch.as_deref() == Some(epoch) {
                return;
            }
            self.session_chat_broker_epoch = Some(epoch.to_string());
            self.refresh_session_chat_runtime_endpoints(true, cx);
            let generations = self
                .agents_chat_page_states
                .values()
                .chain(
                    self.parked_agents_chat_runtimes_by_project
                        .values()
                        .flat_map(|parked| parked.page_states.values()),
                )
                .map(|state| state.generation)
                .collect::<Vec<_>>();
            for generation in generations {
                self.dispatch_session_chat_generation_response(
                    generation,
                    "onSessionChatRuntimeMessage",
                    &serde_json::json!({"kind":"reset"}),
                    false,
                    cx,
                );
            }
            return;
        }
        if self.session_chat_broker_epoch.as_deref() != Some(epoch) {
            return;
        }
        let Some(generation) = message["generation"]
            .as_str()
            .and_then(|value| value.parse::<u64>().ok())
        else {
            return;
        };
        if self.session_chat_runtime_key(generation).is_none() {
            return;
        }
        if message["cacheable"] == true {
            if let Some(snapshot) = message.get("snapshot") {
                self.cache_session_chat_runtime_snapshot(generation, snapshot);
            }
        }
        self.dispatch_session_chat_generation_response(
            generation,
            "onSessionChatRuntimeMessage",
            message,
            false,
            cx,
        );
    }
}
