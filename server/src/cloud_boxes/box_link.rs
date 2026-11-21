//! One Easy Connect link from this computer to each of its running Cloud Boxes, which every
//! `ghostex cloud attach` here rides.
//!
//! CDXC:CloudBoxes 2026-11-21 DECISION:
//! Review of c1d1a8a3a ("fix the two-chat stall"): each `ghostex cloud attach` (every box chat's
//! pane runs one) started its own tailcat client, all dialing a box with this computer's one
//! `server` box key, and a relay delivers a key's traffic only to that key's newest connection
//! (GitHub #526), so a second chat on a box could stall the first. The coordinator offered a key per
//! attach or one link; one link is taken. A key per attach would have to be added to a box that is
//! already running, which is exactly what the box rules refuse (a running box's devices only ever
//! go, #698; letting a device in asks for approval), and it would still need a pool of keys fixed at
//! boot. The desktop's own link to a box can't be handed to a pane either: a pane's attach is a
//! separate `ghostex` process that runs again after a dropped link, and the plain CLI has no desktop.
//! So home gxserver keeps one link per box (`ghostex_paths::tailcat_forward`, the same supervised
//! link the desktop and gxserver's computer tunnels run, which carries any number of connections
//! on one client), dialing with the `server` box key, and each attach asks for its loopback port
//! (`/api/cloudBoxLink`, this computer's own clients only) every time it starts. A box then has at
//! most two clients from this computer, each on a key of its own: the desktop's (`app`) and this
//! one. A link goes when its box is stopped, deleted or found not running.
//! SEE-ALSO: server/src/ghostex_cli/cloud_attach.rs, server/src/cloud_boxes/home_keys.rs.

use std::collections::HashMap;
use std::net::TcpListener;
use std::sync::{Arc, Mutex, OnceLock};

use ghostex_gx_protocol::cloud_box::{CloudBoxState, CloudProviderId};
use ghostex_paths::tailcat_forward::{
    ClientEnd, ForwardEvent, ForwardProbe, ForwardSpec, TailcatForward,
};
use serde_json::{json, Map, Value};

use super::{home_keys, params, registry};
use crate::domain::DomainStateError;
use crate::paths::GxserverPaths;
use crate::remote_access::computer_link_keys::LINK_KEY_PURPOSE_SERVER;

/// `{ boxId }` → `{ port }`: the loopback port of this computer's link to its box.
pub const BOX_LINK_ENDPOINT: &str = "/api/cloudBoxLink";

struct Link {
    address: String,
    forward: TailcatForward,
}

fn links() -> &'static Mutex<HashMap<String, Link>> {
    static LINKS: OnceLock<Mutex<HashMap<String, Link>>> = OnceLock::new();
    LINKS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn log_link_event(box_id: String) -> impl Fn(ForwardEvent) + Send + Sync {
    move |event| {
        if let ForwardEvent::ClientEnded {
            reason,
            uptime,
            connections_cut,
        } = event
        {
            if !matches!(reason, ClientEnd::Stopped | ClientEnd::Refreshed) {
                eprintln!(
                    "[gxserver] Cloud Boxes: the link to {box_id} ended ({}) after {} s with {connections_cut} open connection(s); starting it again",
                    reason.as_str(),
                    uptime.as_secs()
                );
            }
        }
    }
}

/// The loopback port of this computer's link to its running box `box_id`, started the first time
/// (and again when the box's address changed). Blocking only briefly: the link dials on its own.
pub(crate) fn link_port(paths: &GxserverPaths, box_id: &str) -> Result<u16, DomainStateError> {
    let row = registry::row(paths, box_id)?;
    if row.provider == CloudProviderId::Local {
        return Err(DomainStateError::bad_request(
            "A local box is reached directly, not over Easy Connect.",
        ));
    }
    if !row.state.is_reachable() {
        drop_link(box_id);
        return Err(params::wrong_state(format!(
            "{} is {}; send a message to wake it first.",
            row.name,
            row.state.as_str()
        )));
    }
    let address = row
        .tailcat_address
        .clone()
        .filter(|address| !address.trim().is_empty())
        .ok_or_else(|| params::wrong_state(format!("{} has no private address yet.", row.name)))?;
    let mut links = links()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(link) = links.get(box_id).filter(|link| link.address == address) {
        // An unused client may have lost the box without showing it; a fresh one costs nobody.
        link.forward.refresh_idle_client();
        return Ok(link.forward.local_port());
    }
    if let Some(old) = links.remove(box_id) {
        old.forward.stop();
    }
    let binary = crate::tailcat::resolve_client_tailcat_binary().ok_or_else(|| {
        params::wrong_state(format!(
            "Easy Connect (tailcat {} or newer) is not installed on this computer. Install it in Settings › Remote › Easy Connect.",
            ghostex_paths::tailcat::TAILCAT_MIN_VERSION
        ))
    })?;
    let listener = TcpListener::bind(("127.0.0.1", 0))
        .map_err(|error| params::internal(format!("Couldn't open the box's link: {error}")))?;
    let key_paths = paths.clone();
    let key_address = address.clone();
    let forward = TailcatForward::start(
        listener,
        ForwardSpec {
            binary_path: binary,
            address_blob: address.clone(),
            remote_port: crate::constants::GXSERVER_LOCAL_API_PORT,
            probe: ForwardProbe::HttpGet("/api/health"),
        },
        Arc::new(move || home_keys::own_box_key(&key_paths, &key_address, LINK_KEY_PURPOSE_SERVER)),
        Arc::new(log_link_event(box_id.to_string())),
    )
    .map_err(|error| params::internal(format!("Couldn't start the box's link: {error}")))?;
    let port = forward.local_port();
    links.insert(box_id.to_string(), Link { address, forward });
    Ok(port)
}

/// Ends this computer's link to `box_id`, if it has one.
pub(super) fn drop_link(box_id: &str) {
    let link = links()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .remove(box_id);
    if let Some(link) = link {
        link.forward.stop();
    }
}

/// Ends the link of a box a stored row shows not running.
pub(super) fn drop_link_unless_running(box_id: &str, state: &CloudBoxState) {
    if !state.is_reachable() {
        drop_link(box_id);
    }
}

/// `/api/cloudBoxLink`.
pub(crate) async fn answer(
    paths: GxserverPaths,
    params: Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let box_id = params
        .get("boxId")
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| DomainStateError::bad_request("boxId is required."))?;
    let port = params::blocking(move || link_port(&paths, &box_id)).await??;
    Ok(json!({ "port": port }))
}
