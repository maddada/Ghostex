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

    /// A chat view's `broker` request, which is only ever the presentation cache the next view of
    /// the same session opens from (`gx_chat/effects.rs`). The chat's socket and reads are the Rust
    /// chat host's own (`gx_chat/transport.rs`); the binding picks the cache entry, so a delayed
    /// request cannot select another machine or session.
    pub(crate) fn relay_session_chat_runtime_request(
        &mut self,
        generation: u64,
        message: &serde_json::Value,
    ) {
        if message["method"] != "presentation" {
            return;
        }
        if let Some(key) = self.session_chat_runtime_key(generation) {
            self.cache_session_chat_presentation(key, &message["params"]["state"]);
        }
    }

    /// Hands the Rust chat host every machine's gxserver: this computer's daemon, and each remote
    /// machine a chat view is open on. Only a changed endpoint is sent.
    pub(crate) fn refresh_session_chat_runtime_endpoints(
        &mut self,
        force: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        // CDXC:RemoteMachines 2026-09-23 WHY:
        // Reconnecting can replace the SSH forward port while native chat views remain alive. Refresh their mutation/upload target alongside the chat socket's endpoint, including parked views, without recreating controllers or drafts.
        let native_views = self
            .native_chat_views
            .values()
            .chain(
                self.parked_agents_chat_runtimes_by_project
                    .values()
                    .flat_map(|parked| parked.native_views.values()),
            )
            .cloned()
            .collect::<Vec<_>>();
        let mut machines = vec![crate::app::gx_chat::LOCAL_MACHINE_ID.to_string()];
        for view in native_views {
            let machine_id = view.read(cx).config.machine_id.clone();
            if machine_id == crate::app::gx_chat::LOCAL_MACHINE_ID {
                continue;
            }
            if let Some(target) = self.gpui_remote_gxserver_request_target(&machine_id) {
                view.update(cx, |view, _| view.config.remote = Some(target));
            }
            if !machines.contains(&machine_id) {
                machines.push(machine_id);
            }
        }
        for machine_id in machines {
            let endpoint = if machine_id == crate::app::gx_chat::LOCAL_MACHINE_ID {
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
            crate::app::gx_chat::set_endpoint(&machine_id, &endpoint.0, &endpoint.1);
            self.session_chat_broker_endpoints
                .insert(machine_id, endpoint);
        }
    }
}
