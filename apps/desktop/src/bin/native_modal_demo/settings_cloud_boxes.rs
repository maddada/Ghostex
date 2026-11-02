//! Preview fixtures for the Cloud Boxes Settings page: a scripted `/api/agentbox` with the data of
//! the Storybook `CloudBoxes` story (Docker ready, Hetzner logged in but not prepared, one
//! registered server, two boxes).
//!
//! States: `cloud-boxes` (the page), `cloud-boxes-add-server` (the Add Server form open),
//! `cloud-boxes-missing` (agentbox not installed), `cloud-boxes-error` (gxserver has no
//! `/api/agentbox`), `cloud-boxes-loading` (status never answers), `cloud-boxes-windows`
//! (`supported: false`), `cloud-boxes-offline` (no gxserver).
//!
//! The Cloud Boxes part (`/api/cloudBoxes`): Instabox connected on the Pro plan, Boat not
//! connected, this month under the $20 cap, two boxes (one with unpushed work) and three agent
//! accounts. `cloud-boxes-not-connected` has no provider connected and no box,
//! `cloud-boxes-cap-warning` is past 80% of the cap, `cloud-boxes-cap-reached` at it.
use serde_json::{Value, json};

pub(super) fn owns(state: &str) -> bool {
    state.starts_with("cloud-boxes")
}

pub(super) fn open_message(state: &str) -> Option<Value> {
    owns(state).then(|| json!({ "initialTab": "cloudBoxes" }))
}

pub(super) fn gxserver_rpc_available(state: &str) -> bool {
    owns(state) && state != "cloud-boxes-offline"
}

fn status(state: &str) -> Value {
    match state {
        "cloud-boxes-missing" => json!({ "supported": true, "installed": false, "providers": [] }),
        "cloud-boxes-windows" => json!({ "supported": false, "installed": false, "providers": [] }),
        _ => json!({
            "supported": true,
            "installed": true,
            "version": "0.33.0",
            "dockerReady": true,
            "portlessInstalled": false,
            "agentSignIns": { "claude": false, "codex": true },
            "providers": [
                { "id": "docker", "kind": "local", "ready": true, "configured": true, "prepared": true },
                { "id": "hetzner", "kind": "cloud", "ready": false, "configured": true, "prepared": false },
                { "id": "vercel", "kind": "cloud", "ready": false, "configured": false, "prepared": false },
                { "id": "daytona", "kind": "cloud", "ready": false, "configured": false, "prepared": false },
                { "id": "e2b", "kind": "cloud", "ready": false, "configured": false, "prepared": false },
                { "id": "digitalocean", "kind": "cloud", "ready": false, "configured": false, "prepared": false },
                { "id": "docker:selfhost", "kind": "remoteDocker", "label": "selfhost", "description": "Your server over SSH", "ready": true }
            ],
            "checkedAt": "2026-10-01T00:00:00Z",
            "error": null,
        }),
    }
}

fn boxes() -> Value {
    json!({
        "boxes": [
            {
                "name": "gx-ghostex-a1b2c3",
                "agent": "codex",
                "provider": "docker",
                "state": "running",
                "webUrl": "http://127.0.0.1:32772",
                "projectId": "project-ghostex",
                "sessionId": "session-1",
                "sessionTitle": "Fix the login page"
            },
            { "name": "gx-remote1", "agent": "claude", "provider": "remote-docker", "state": "paused" }
        ]
    })
}

/// The scripted `/api/agentbox`; `None` never answers (the loading state).
pub(super) fn rpc(
    state: &str,
    path: &str,
    params: &Value,
) -> Option<Option<Result<Value, String>>> {
    if path == "/api/cloudBoxes" {
        return owns(state).then(|| Some(cloud_rpc(state, params)));
    }
    if path != "/api/agentbox" {
        return None;
    }
    if state == "cloud-boxes-error" {
        return Some(Some(Err(
            "gxserver does not know /api/agentbox yet. Update Ghostex.".to_string(),
        )));
    }
    Some(match params["action"].as_str() {
        Some("status") if state == "cloud-boxes-loading" => None,
        Some("status") => Some(Ok(status(state))),
        Some("list") => Some(Ok(boxes())),
        Some("openTarget") => Some(Ok(json!({ "url": "http://127.0.0.1:32772" }))),
        Some("stop" | "destroy") => Some(Ok(json!({ "ok": true, "output": "" }))),
        _ => Some(Err("Unknown agentbox action.".to_string())),
    })
}

fn cloud_providers(state: &str) -> Value {
    let instabox = if state == "cloud-boxes-not-connected" {
        json!({ "id": "instabox", "label": "Instabox", "state": "notConnected", "connectMethods": ["browser", "key"], "available": false })
    } else {
        json!({
            "id": "instabox", "label": "Instabox", "state": "connected",
            "connectMethods": ["browser", "key"], "available": true,
            "accountLabel": "maddada", "accountEmail": "you@example.com", "plan": "pro",
            "connectedAt": "2026-11-02T08:00:00Z"
        })
    };
    json!({ "providers": [
        instabox,
        { "id": "boat", "label": "Boat", "state": "notConnected", "connectMethods": ["browser", "key"], "available": false }
    ] })
}

fn cloud_spend(state: &str) -> Value {
    let (instabox, state_name) = match state {
        "cloud-boxes-cap-warning" => (16.4, "warn"),
        "cloud-boxes-cap-reached" => (20.15, "reached"),
        "cloud-boxes-not-connected" => (0.0, "ok"),
        _ => (1.42, "ok"),
    };
    json!({
        "month": "2026-11",
        "providers": [
            { "provider": "instabox", "usd": instabox, "scope": "account", "readAt": "2026-11-02T09:00:00Z" },
            { "provider": "boat", "usd": 0.3, "scope": "ghostexBoxes", "estimated": true }
        ],
        "totalUsd": instabox + 0.3,
        "capUsd": 20,
        "state": state_name
    })
}

fn cloud_boxes(state: &str) -> Value {
    if state == "cloud-boxes-not-connected" {
        return json!({ "boxes": [], "machines": [] });
    }
    json!({
        "boxes": [
            {
                "boxId": "gx_abcdefabcdef", "provider": "instabox", "providerBoxId": "m-1",
                "name": "box-c31", "state": "running", "projectKey": "github.com/maddada/ghostex",
                "homeProjectId": "project-ghostex", "size": "2x4",
                "createdAt": "2026-11-01T09:00:00Z", "costTotalUsd": 0.42, "costEstimated": true,
                "usdPerHour": 0.08
            },
            {
                "boxId": "gx_bcdefabcdefa", "provider": "instabox", "providerBoxId": "m-2",
                "name": "box-7qa", "state": "stopped", "projectKey": "github.com/maddada/site",
                "createdAt": "2026-10-28T09:00:00Z", "costTotalUsd": 1.1, "costEstimated": true,
                "hasUnpushedWork": true
            }
        ],
        "machines": []
    })
}

fn cloud_logins() -> Value {
    json!({ "logins": [
        { "accountId": "claude-1", "agent": "claude", "name": "Work", "state": "ready", "allowedAt": "2026-11-02T08:10:00Z" },
        { "accountId": "claude-2", "agent": "claude", "name": "Personal", "state": "notAllowed" },
        { "accountId": "codex-1", "agent": "codex", "name": "ChatGPT Pro", "state": "notAllowed" }
    ] })
}

/// The scripted `/api/cloudBoxes`.
fn cloud_rpc(state: &str, params: &Value) -> Result<Value, String> {
    match params["action"].as_str() {
        Some("providers") => Ok(cloud_providers(state)),
        Some("sizes") => Ok(json!({ "sizes": [
            { "id": "2x4", "provider": "instabox", "vcpus": 2, "memMib": 4096, "label": "2 CPU · 4 GB", "usdPerHour": 0.08, "isDefault": true },
            { "id": "4x8", "provider": "instabox", "vcpus": 4, "memMib": 8192, "label": "4 CPU · 8 GB", "usdPerHour": 0.16 }
        ] })),
        Some("spend") => Ok(cloud_spend(state)),
        Some("list") => Ok(cloud_boxes(state)),
        Some("agentLogins") => Ok(cloud_logins()),
        Some("setAgentLogin") => Ok(cloud_logins()),
        Some("delete") if params["force"] != true => {
            Err("box-7qa has unpushed work: push it, or delete anyway.".to_string())
        }
        Some("connect") => Ok(json!({
            "provider": params["provider"], "state": "connecting", "connectId": "c-1",
            "authUrl": "https://boat.dev/claim", "userCode": "482 193", "browserOpened": true
        })),
        Some("connectStatus") => Ok(json!({
            "provider": params["provider"], "state": "connecting", "connectId": "c-1",
            "authUrl": "https://boat.dev/claim", "userCode": "482 193", "browserOpened": true
        })),
        _ => Err("Not in this preview.".to_string()),
    }
}
