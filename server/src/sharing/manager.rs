//! Active share lifecycle manager: owns all active Live page and localhost port shares in memory,
//! enforces expiry on a background sweep, stops everything cleanly when gxserver shuts down, and
//! broadcasts `sharingChanged` events.

use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

use rand::TryRng;
use serde_json::{json, Value};
use tokio::sync::{broadcast, Mutex};

use crate::{constants::GXSERVER_PROTOCOL_VERSION, server::AppState, visual_pages::shared_page};

use super::{
    blocklist::check_port_allowed,
    config::{
        load_config, save_config, validate_relay_url, ShareExpiry, SharingConfig, DEFAULT_PROVIDER,
    },
    provider::{OpenTunnelProvider, TunnelProvider},
    proxy::{spawn_share_proxy, RunningShareProxy, ShareProxyTarget},
};

const EXPIRY_SWEEP_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ShareKind {
    Page { page_id: String },
    Port { port: u16 },
}

struct ActiveShare {
    share_id: String,
    route_name: String,
    kind: ShareKind,
    title: String,
    url: String,
    provider: String,
    created_at_ms: i64,
    expires_at_ms: Option<i64>,
    expiry: ShareExpiry,
    passcode: Option<String>,
    proxy: RunningShareProxy,
}

impl ActiveShare {
    fn to_json(&self) -> Value {
        let mut obj = json!({
            "id": self.share_id,
            "shareId": self.share_id,
            "kind": match self.kind {
                ShareKind::Page { .. } => "page",
                ShareKind::Port { .. } => "port",
            },
            "title": self.title,
            "url": self.url,
            "provider": self.provider,
            "createdAtMs": self.created_at_ms,
            "expiresAtMs": self.expires_at_ms,
            "expiry": self.expiry.as_str(),
            "passcodeRequired": self.passcode.is_some(),
        });
        if let Some(code) = &self.passcode {
            obj["passcode"] = json!(code);
        }
        match &self.kind {
            ShareKind::Page { page_id } => {
                obj["pageId"] = json!(page_id);
            }
            ShareKind::Port { port } => {
                obj["port"] = json!(port);
            }
        }
        obj
    }
}

struct ManagerInner {
    config: SharingConfig,
    provider_relay_url: String,
    provider: Arc<dyn TunnelProvider>,
    shares: HashMap<String, ActiveShare>,
}

pub(crate) struct ShareManager {
    state_dir: PathBuf,
    gxserver_port: u16,
    inner: Mutex<ManagerInner>,
}

impl ShareManager {
    pub(crate) fn new(state_dir: PathBuf, gxserver_port: u16) -> Self {
        let config = load_config(&state_dir);
        let effective_relay = config.effective_relay_url();
        let provider: Arc<dyn TunnelProvider> = Arc::new(OpenTunnelProvider::new(
            state_dir.clone(),
            effective_relay.clone(),
        ));
        Self {
            state_dir,
            gxserver_port,
            inner: Mutex::new(ManagerInner {
                config,
                provider_relay_url: effective_relay,
                provider,
                shares: HashMap::new(),
            }),
        }
    }

    pub(crate) fn supported() -> bool {
        true
    }

    pub(crate) async fn read_status(&self) -> Value {
        self.reap_expired_inner().await;
        let (config, provider, shares) = {
            let guard = self.inner.lock().await;
            let mut shares: Vec<Value> = guard.shares.values().map(ActiveShare::to_json).collect();
            shares.sort_by(|a, b| b["createdAtMs"].as_i64().cmp(&a["createdAtMs"].as_i64()));
            (guard.config.clone(), guard.provider.clone(), shares)
        };
        let provider_status = provider.status().await;
        json!({
            "supported": Self::supported(),
            "config": config.to_json(),
            "providerStatus": provider_status.to_json(),
            "providers": [
                {
                    "id": "opentunnel",
                    "name": "OpenTunnel",
                    "description": "Blind end-to-end TLS tunnel (zero signup; self-hostable via relay URL)",
                    "available": true,
                }
            ],
            "shares": shares,
        })
    }

    pub(crate) async fn update_config(
        &self,
        state: &AppState,
        patch: &Value,
    ) -> Result<Value, String> {
        let Some(fields) = patch.as_object() else {
            return Err("Settings patch must be an object.".to_string());
        };
        let mut stop_all_provider: Option<Arc<dyn TunnelProvider>> = None;
        let mut proxies_to_stop: Vec<RunningShareProxy> = Vec::new();
        {
            let mut guard = self.inner.lock().await;
            let mut next = guard.config.clone();
            if let Some(enabled) = fields.get("enabled").and_then(Value::as_bool) {
                next.enabled = enabled;
            }
            if let Some(provider) = fields.get("provider").and_then(Value::as_str) {
                let provider = provider.trim();
                if provider != DEFAULT_PROVIDER {
                    return Err(format!("Unsupported tunnel provider '{provider}'."));
                }
                next.provider = provider.to_string();
            }
            if let Some(relay_url) = fields.get("relayUrl").and_then(Value::as_str) {
                next.relay_url = validate_relay_url(relay_url)?;
            }
            if let Some(expiry_raw) = fields.get("defaultExpiry").and_then(Value::as_str) {
                let Some(expiry) = ShareExpiry::parse(expiry_raw) else {
                    return Err(
                        "Invalid default expiry. Use 1h, 24h, 7d, or untilStopped.".to_string()
                    );
                };
                next.default_expiry = expiry;
            }
            if let Some(default_passcode) = fields.get("defaultPasscode").and_then(Value::as_bool) {
                next.default_passcode = default_passcode;
            }

            save_config(&self.state_dir, &next)?;
            let new_relay = next.effective_relay_url();
            let relay_changed = new_relay != guard.provider_relay_url;
            let disabled = !next.enabled;
            guard.config = next;

            if disabled || relay_changed {
                stop_all_provider = Some(guard.provider.clone());
                for (_, share) in guard.shares.drain() {
                    proxies_to_stop.push(share.proxy);
                }
            }
            if relay_changed {
                guard.provider_relay_url = new_relay.clone();
                guard.provider =
                    Arc::new(OpenTunnelProvider::new(self.state_dir.clone(), new_relay));
            }
        }
        for proxy in proxies_to_stop {
            proxy.stop();
        }
        if let Some(provider) = stop_all_provider {
            let _ = provider.stop_all().await;
        }
        broadcast_sharing_changed(state);
        Ok(self.read_status().await)
    }

    pub(crate) async fn create_page_share(
        &self,
        state: Arc<AppState>,
        page_id: &str,
        expiry_override: Option<ShareExpiry>,
        passcode_override: Option<bool>,
        debug_expires_in_ms: Option<i64>,
    ) -> Result<Value, String> {
        let page_id = page_id.trim().to_ascii_lowercase();
        let state_dir = self.state_dir.clone();
        let page_id_clone = page_id.clone();
        let Some(page) =
            tokio::task::spawn_blocking(move || shared_page(&state_dir, &page_id_clone))
                .await
                .ok()
                .flatten()
        else {
            return Err("That Live page could not be found.".to_string());
        };

        let (config, provider, existing_share_id) = {
            let guard = self.inner.lock().await;
            if !guard.config.enabled {
                return Err("Sharing is turned off. Turn it on in Settings > Sharing.".to_string());
            }
            let existing = guard.shares.values().find_map(|share| match &share.kind {
                ShareKind::Page { page_id: existing } if existing == &page_id => {
                    Some(share.share_id.clone())
                }
                _ => None,
            });
            (guard.config.clone(), guard.provider.clone(), existing)
        };

        // If this page already has an active share, replace it cleanly with the new options.
        if let Some(existing_id) = existing_share_id {
            let _ = self.stop_share_inner(&existing_id).await;
        }

        let expiry = expiry_override.unwrap_or(config.default_expiry);
        let require_passcode = passcode_override.unwrap_or(config.default_passcode);
        let passcode = require_passcode.then(generate_passcode);
        let now_ms = chrono::Utc::now().timestamp_millis();
        let expires_at_ms = if cfg!(debug_assertions) && debug_expires_in_ms.is_some() {
            debug_expires_in_ms.map(|ms| now_ms + ms)
        } else {
            expiry.duration_ms().map(|ms| now_ms + ms)
        };
        let share_id = random_hex(12);
        let route_name = random_hex(8);

        let proxy = spawn_share_proxy(
            state.clone(),
            ShareProxyTarget::Page {
                page_id: page.id.clone(),
                title: page.title.clone(),
                html: Arc::new(page.html),
                answerable: page.answerable,
            },
            expires_at_ms,
            passcode.clone(),
        )
        .await?;

        let listen_addr = proxy.listen_addr;
        let url = match provider.add_route(route_name.clone(), listen_addr).await {
            Ok(url) => url,
            Err(error) => {
                proxy.stop();
                return Err(error);
            }
        };

        let active = ActiveShare {
            share_id: share_id.clone(),
            route_name,
            kind: ShareKind::Page {
                page_id: page.id.clone(),
            },
            title: page.title,
            url,
            provider: provider.id().to_string(),
            created_at_ms: now_ms,
            expires_at_ms,
            expiry,
            passcode,
            proxy,
        };
        let share_json = active.to_json();
        {
            let mut guard = self.inner.lock().await;
            guard.shares.insert(share_id, active);
        }
        broadcast_sharing_changed(&state);
        Ok(json!({
            "share": share_json,
            "status": self.read_status().await,
        }))
    }

    pub(crate) async fn create_port_share(
        &self,
        state: Arc<AppState>,
        port: u16,
        title: Option<String>,
        expiry_override: Option<ShareExpiry>,
        passcode_override: Option<bool>,
    ) -> Result<Value, String> {
        let (config, provider, existing_share_id, active_proxy_ports) = {
            let guard = self.inner.lock().await;
            if !guard.config.enabled {
                return Err("Sharing is turned off. Turn it on in Settings > Sharing.".to_string());
            }
            let existing = guard.shares.values().find_map(|share| match &share.kind {
                ShareKind::Port { port: existing } if *existing == port => {
                    Some(share.share_id.clone())
                }
                _ => None,
            });
            let proxy_ports: HashSet<u16> = guard
                .shares
                .values()
                .map(|share| share.proxy.listen_addr.port())
                .collect();
            (
                guard.config.clone(),
                guard.provider.clone(),
                existing,
                proxy_ports,
            )
        };

        // Check process name from listening ports if available.
        let process_name = tokio::task::spawn_blocking(move || {
            crate::tcp_listeners::listener_discovery::read_all_tcp_listeners()
                .ok()
                .and_then(|listeners| {
                    listeners
                        .into_iter()
                        .find(|entry| entry.port == port)
                        .and_then(|entry| entry.command)
                })
        })
        .await
        .ok()
        .flatten();

        check_port_allowed(
            port,
            self.gxserver_port,
            &active_proxy_ports,
            process_name.as_deref(),
        )?;

        if let Some(existing_id) = existing_share_id {
            let _ = self.stop_share_inner(&existing_id).await;
        }

        let expiry = expiry_override.unwrap_or(config.default_expiry);
        let require_passcode = passcode_override.unwrap_or(config.default_passcode);
        let passcode = require_passcode.then(generate_passcode);
        let now_ms = chrono::Utc::now().timestamp_millis();
        let expires_at_ms = expiry.duration_ms().map(|ms| now_ms + ms);
        let share_id = random_hex(12);
        let route_name = random_hex(8);
        let label = title
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
            .unwrap_or_else(|| format!("localhost:{port}"));

        let proxy = spawn_share_proxy(
            state.clone(),
            ShareProxyTarget::Port {
                port,
                title: label.clone(),
            },
            expires_at_ms,
            passcode.clone(),
        )
        .await?;

        let listen_addr = proxy.listen_addr;
        let url = match provider.add_route(route_name.clone(), listen_addr).await {
            Ok(url) => url,
            Err(error) => {
                proxy.stop();
                return Err(error);
            }
        };

        let active = ActiveShare {
            share_id: share_id.clone(),
            route_name,
            kind: ShareKind::Port { port },
            title: label,
            url,
            provider: provider.id().to_string(),
            created_at_ms: now_ms,
            expires_at_ms,
            expiry,
            passcode,
            proxy,
        };
        let share_json = active.to_json();
        {
            let mut guard = self.inner.lock().await;
            guard.shares.insert(share_id, active);
        }
        broadcast_sharing_changed(&state);
        Ok(json!({
            "share": share_json,
            "status": self.read_status().await,
        }))
    }

    pub(crate) async fn stop_share(
        &self,
        state: &AppState,
        share_id: &str,
    ) -> Result<Value, String> {
        let removed = self.stop_share_inner(share_id).await?;
        if removed {
            broadcast_sharing_changed(state);
        }
        Ok(self.read_status().await)
    }

    async fn stop_share_inner(&self, share_id: &str) -> Result<bool, String> {
        let removed = {
            let mut guard = self.inner.lock().await;
            guard
                .shares
                .remove(share_id)
                .map(|share| (share, guard.provider.clone()))
        };
        let Some((share, provider)) = removed else {
            return Ok(false);
        };
        share.proxy.stop();
        let _ = provider.remove_route(share.route_name).await;
        Ok(true)
    }

    pub(crate) async fn stop_all(&self, state: Option<&AppState>) -> Value {
        let (provider, proxies) = {
            let mut guard = self.inner.lock().await;
            let proxies: Vec<RunningShareProxy> =
                guard.shares.drain().map(|(_, share)| share.proxy).collect();
            (guard.provider.clone(), proxies)
        };
        for proxy in proxies {
            proxy.stop();
        }
        let _ = provider.stop_all().await;
        if let Some(state) = state {
            broadcast_sharing_changed(state);
        }
        self.read_status().await
    }

    async fn reap_expired_inner(&self) -> bool {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let expired: Vec<(ActiveShare, Arc<dyn TunnelProvider>)> = {
            let mut guard = self.inner.lock().await;
            let expired_ids: Vec<String> = guard
                .shares
                .iter()
                .filter(|(_, share)| share.expires_at_ms.is_some_and(|expires| now_ms >= expires))
                .map(|(id, _)| id.clone())
                .collect();
            let provider = guard.provider.clone();
            expired_ids
                .into_iter()
                .filter_map(|id| guard.shares.remove(&id).map(|s| (s, provider.clone())))
                .collect()
        };
        if expired.is_empty() {
            return false;
        }
        for (share, provider) in expired {
            share.proxy.stop();
            let _ = provider.remove_route(share.route_name).await;
        }
        true
    }

    /// Background sweep that tears down expired shares and stops every active share + tunnel
    /// immediately when gxserver shuts down.
    pub(crate) fn start_runtime(state: Arc<AppState>, mut shutdown_rx: broadcast::Receiver<()>) {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(EXPIRY_SWEEP_INTERVAL);
            loop {
                tokio::select! {
                    _ = shutdown_rx.recv() => {
                        let _ = state.share_manager.stop_all(None).await;
                        break;
                    }
                    _ = interval.tick() => {
                        if state.share_manager.reap_expired_inner().await {
                            broadcast_sharing_changed(&state);
                        }
                    }
                }
            }
        });
    }
}

fn broadcast_sharing_changed(state: &AppState) {
    state.event_hub.broadcast(json!({
        "protocolVersion": GXSERVER_PROTOCOL_VERSION,
        "serverId": state.metadata.server_id.clone(),
        "type": "sharingChanged",
    }));
}

fn random_hex(num_bytes: usize) -> String {
    let mut bytes = vec![0u8; num_bytes];
    rand::rngs::SysRng
        .try_fill_bytes(&mut bytes)
        .expect("OS RNG failed");
    hex::encode(bytes)
}

fn generate_passcode() -> String {
    let mut bytes = [0u8; 4];
    rand::rngs::SysRng
        .try_fill_bytes(&mut bytes)
        .expect("OS RNG failed");
    let value = u32::from_be_bytes(bytes) % 1_000_000;
    format!("{value:06}")
}
