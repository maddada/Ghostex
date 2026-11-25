//! Pluggable tunnel provider trait (`TunnelProvider`) and default `OpenTunnelProvider` backed by
//! the embedded `opentunnel` Rust library crate.
//!
//! CDXC:Sharing 2026-11-24 DECISION:
//! User chose OpenTunnel as the default built-in provider behind a pluggable provider trait,
//! supporting custom `OPENTUNNEL_API` URLs now while leaving a clean seam for `cloudflared` and
//! Tailscale Funnel later.

use std::{future::Future, net::SocketAddr, path::PathBuf, pin::Pin, sync::Arc, time::Duration};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use tokio::sync::Mutex;

use super::config::{sharing_dir, DEFAULT_RELAY_URL};

pub(crate) type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

#[derive(Clone, Debug)]
pub(crate) struct ProviderStatus {
    pub(crate) id: &'static str,
    pub(crate) display_name: &'static str,
    pub(crate) connected: bool,
    pub(crate) hostname: Option<String>,
    pub(crate) last_error: Option<String>,
}

impl ProviderStatus {
    pub(crate) fn to_json(&self) -> Value {
        json!({
            "id": self.id,
            "displayName": self.display_name,
            "connected": self.connected,
            "hostname": self.hostname,
            "lastError": self.last_error,
        })
    }
}

/// Pluggable seam for public tunnel providers (OpenTunnel today; cloudflared / Tailscale Funnel
/// can implement this same trait later).
pub(crate) trait TunnelProvider: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    /// Exposes `route_name` pointing ONLY at `target_addr` (an isolated ephemeral loopback
    /// share-proxy, never gxserver). Returns the public HTTPS URL.
    fn add_route(
        &self,
        route_name: String,
        target_addr: SocketAddr,
    ) -> BoxFuture<'_, Result<String, String>>;
    /// Removes `route_name` immediately; shuts down the underlying tunnel when no routes remain.
    fn remove_route(&self, route_name: String) -> BoxFuture<'_, Result<(), String>>;
    /// Tears down all routes and disconnects immediately.
    fn stop_all(&self) -> BoxFuture<'_, Result<(), String>>;
    fn status(&self) -> BoxFuture<'_, ProviderStatus>;
}

/// How long `add_route` waits for the OpenTunnel bridge to report `Connected`.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(20);

struct OpenTunnelRuntime {
    relay_url: String,
    profile: String,
    hostname: Option<String>,
    routes: opentunnel::Routes,
    tunnel: Option<opentunnel::Tunnel>,
    last_error: Option<String>,
}

/// Embedded OpenTunnel provider using the `opentunnel` crate (blind end-to-end TLS: the relay only
/// inspects TLS SNI and never sees HTTP plaintext, cookies, or private keys).
pub(crate) struct OpenTunnelProvider {
    storage_root: PathBuf,
    runtime: Arc<Mutex<OpenTunnelRuntime>>,
}

impl OpenTunnelProvider {
    pub(crate) fn new(state_dir: PathBuf, relay_url: String) -> Self {
        let profile = profile_name_for_relay(&relay_url);
        let storage_root = sharing_dir(&state_dir).join("opentunnel");
        Self {
            storage_root,
            runtime: Arc::new(Mutex::new(OpenTunnelRuntime {
                relay_url,
                profile,
                hostname: None,
                routes: opentunnel::Routes::new(),
                tunnel: None,
                last_error: None,
            })),
        }
    }

    fn client(&self, relay_url: &str) -> Result<opentunnel::Client, String> {
        let api = reqwest_url(relay_url)?;
        let storage = opentunnel::Storage::at(self.storage_root.clone());
        let ca_cert = std::env::var_os("OPENTUNNEL_CA_CERT")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from);
        Ok(opentunnel::Client::new(opentunnel::ClientOptions {
            api: Some(api),
            storage: Some(storage),
            ca_cert,
        }))
    }
}

fn reqwest_url(raw: &str) -> Result<url::Url, String> {
    url::Url::parse(raw).map_err(|error| format!("Invalid OpenTunnel relay URL: {error}"))
}

fn profile_name_for_relay(relay_url: &str) -> String {
    let trimmed = relay_url.trim().trim_end_matches('/');
    if trimmed.is_empty() || trimmed.eq_ignore_ascii_case(DEFAULT_RELAY_URL) {
        return "default".to_string();
    }
    let digest = Sha256::digest(trimmed.as_bytes());
    let suffix: String = digest.iter().take(6).map(|b| format!("{b:02x}")).collect();
    format!("custom-{suffix}")
}

impl TunnelProvider for OpenTunnelProvider {
    fn id(&self) -> &'static str {
        "opentunnel"
    }

    fn display_name(&self) -> &'static str {
        "OpenTunnel"
    }

    fn add_route(
        &self,
        route_name: String,
        target_addr: SocketAddr,
    ) -> BoxFuture<'_, Result<String, String>> {
        Box::pin(async move {
            let mut rt = self.runtime.lock().await;
            let client = self.client(&rt.relay_url)?;
            let identity = match client.ensure(&rt.profile).await {
                Ok(identity) => identity,
                Err(error) => {
                    let message = format!("Could not set up OpenTunnel: {error}");
                    rt.last_error = Some(message.clone());
                    return Err(message);
                }
            };
            let hostname = identity.hostname.clone();
            rt.hostname = Some(hostname.clone());
            rt.routes.insert(
                route_name.clone(),
                opentunnel::Route::from(target_addr.to_string()),
            );

            if rt.tunnel.is_some() {
                let routes_snapshot = rt.routes.clone();
                let mut watch = {
                    let tunnel = rt.tunnel.as_ref().unwrap();
                    if let Err(error) = tunnel.set_routes(routes_snapshot) {
                        rt.routes.remove(&route_name);
                        let message = format!("Could not update tunnel route: {error}");
                        rt.last_error = Some(message.clone());
                        return Err(message);
                    }
                    tunnel.status_watch()
                };
                let wait = tokio::time::timeout(CONNECT_TIMEOUT, async {
                    loop {
                        let status = watch.borrow().clone();
                        if status.state == opentunnel::State::Connected
                            && status.routes.contains_key(&route_name)
                        {
                            return Ok(());
                        }
                        if status.state == opentunnel::State::Stopped {
                            return Err(status
                                .last_error
                                .unwrap_or_else(|| "Tunnel stopped unexpectedly.".to_string()));
                        }
                        if watch.changed().await.is_err() {
                            return Err("Tunnel disconnected while adding route.".to_string());
                        }
                    }
                })
                .await;
                match wait {
                    Ok(Ok(())) => {
                        rt.last_error = None;
                    }
                    Ok(Err(message)) => {
                        rt.routes.remove(&route_name);
                        let rollback = rt.routes.clone();
                        if let Some(tunnel) = rt.tunnel.as_ref() {
                            let _ = tunnel.set_routes(rollback);
                        }
                        rt.last_error = Some(message.clone());
                        return Err(message);
                    }
                    Err(_) => {
                        rt.routes.remove(&route_name);
                        let rollback = rt.routes.clone();
                        if let Some(tunnel) = rt.tunnel.as_ref() {
                            let _ = tunnel.set_routes(rollback);
                        }
                        let message =
                            "Timed out connecting to OpenTunnel. Check your internet connection."
                                .to_string();
                        rt.last_error = Some(message.clone());
                        return Err(message);
                    }
                }
            } else {
                let tunnel = match client.connect(&rt.profile, rt.routes.clone()) {
                    Ok(tunnel) => tunnel,
                    Err(error) => {
                        rt.routes.remove(&route_name);
                        let message = format!("Could not connect to OpenTunnel: {error}");
                        rt.last_error = Some(message.clone());
                        return Err(message);
                    }
                };
                let mut watch = tunnel.status_watch();
                let wait = tokio::time::timeout(CONNECT_TIMEOUT, async {
                    loop {
                        let status = watch.borrow().clone();
                        if status.state == opentunnel::State::Connected {
                            return Ok(());
                        }
                        if status.state == opentunnel::State::Stopped {
                            return Err(status
                                .last_error
                                .unwrap_or_else(|| "Tunnel stopped unexpectedly.".to_string()));
                        }
                        if watch.changed().await.is_err() {
                            return Err("Tunnel closed before connecting.".to_string());
                        }
                    }
                })
                .await;
                match wait {
                    Ok(Ok(())) => {
                        rt.tunnel = Some(tunnel);
                        rt.last_error = None;
                    }
                    Ok(Err(message)) => {
                        let _ = tunnel.close().await;
                        rt.routes.remove(&route_name);
                        rt.last_error = Some(message.clone());
                        return Err(message);
                    }
                    Err(_) => {
                        let _ = tunnel.close().await;
                        rt.routes.remove(&route_name);
                        let message =
                            "Timed out connecting to OpenTunnel. Check your internet connection."
                                .to_string();
                        rt.last_error = Some(message.clone());
                        return Err(message);
                    }
                }
            }

            Ok(format!("https://{route_name}.{hostname}"))
        })
    }

    fn remove_route(&self, route_name: String) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async move {
            let mut rt = self.runtime.lock().await;
            rt.routes.remove(&route_name);
            if rt.routes.is_empty() {
                if let Some(tunnel) = rt.tunnel.take() {
                    let _ = tunnel.close().await;
                }
            } else if let Some(tunnel) = rt.tunnel.as_ref() {
                let _ = tunnel.set_routes(rt.routes.clone());
            }
            Ok(())
        })
    }

    fn stop_all(&self) -> BoxFuture<'_, Result<(), String>> {
        Box::pin(async move {
            let mut rt = self.runtime.lock().await;
            rt.routes.clear();
            if let Some(tunnel) = rt.tunnel.take() {
                let _ = tunnel.close().await;
            }
            Ok(())
        })
    }

    fn status(&self) -> BoxFuture<'_, ProviderStatus> {
        Box::pin(async move {
            let rt = self.runtime.lock().await;
            let (connected, last_error) = match rt.tunnel.as_ref() {
                Some(tunnel) => {
                    let s = tunnel.status();
                    (
                        s.state == opentunnel::State::Connected,
                        s.last_error.or_else(|| rt.last_error.clone()),
                    )
                }
                None => (false, rt.last_error.clone()),
            };
            ProviderStatus {
                id: self.id(),
                display_name: self.display_name(),
                connected,
                hostname: rt.hostname.clone(),
                last_error,
            }
        })
    }
}
