//! Preview fixtures for Settings > Updates: the app's `updatesState` answer to the page's
//! `updates` posts, with the versions this Mac had on 2026-11-05.
//!
//! States: `updates` (failures, updates, one updating, Up to date), `updates-done` (everything up
//! to date), `updates-loading` (no answer yet), `updates-computers` (this computer and three remote
//! ones: a Linux box with its own updates and Not installed, one offline, one on an old Ghostex),
//! `updates-computers-away` (this computer up to date, the others offline or too old),
//! `updates-computers-box` (the Linux box picked), `updates-computers-air` (MacBook Air picked
//! while it is offline), `updates-computers-run` (an Update all machines running: this computer
//! updating, the Linux box's Claude Code failed, the others skipped), `updates-computers-open`
//! (the machine picker open). The `updates-computers*` states save the three machines, so the
//! picker shows.
use serde_json::{Map, Value, json};

/// The saved machines the picker lists (the Linux box connected, the others not).
pub(super) fn story_settings(state: &str, settings: &mut Map<String, Value>) {
    if !state.starts_with("updates-computers") {
        return;
    }
    settings.insert(
        "remoteMachines".into(),
        json!([
            { "id": "remote-linux-box", "name": "Linux box", "sshHost": "10.0.0.2", "sshUser": "madda" },
            { "id": "remote-air", "name": "MacBook Air", "sshHost": "10.0.0.3", "sshUser": "madda" },
            { "id": "remote-mini", "name": "Old Mini", "sshHost": "10.0.0.4", "sshUser": "madda" }
        ]),
    );
}

pub(super) fn owns(state: &str) -> bool {
    state.starts_with("updates")
}

pub(super) fn open_message(state: &str) -> Option<Value> {
    let machine = match state {
        "updates-computers-box" => Some("remote-linux-box"),
        "updates-computers-air" => Some("remote-air"),
        _ => None,
    };
    owns(state).then(|| match machine {
        Some(machine) => json!({ "initialTab": "updates", "initialMachineId": machine }),
        None => json!({ "initialTab": "updates" }),
    })
}

#[allow(clippy::too_many_arguments)]
fn item(
    id: &str,
    kind: &str,
    group: &str,
    name: &str,
    icon: &str,
    detail: &str,
    installed: Option<&str>,
    latest: Option<&str>,
    state: &str,
    extra: Value,
) -> Value {
    let mut item = json!({
        "id": id, "kind": kind, "group": group, "name": name, "icon": icon, "detail": detail,
        "installed": installed, "latest": latest, "state": state,
        "canUpdate": state != "updating", "canAuto": true, "auto": false,
        "topBar": matches!(kind, "app" | "agent" | "helper" | "component"),
        "major": false, "inUse": false,
    });
    if let (Some(item), Some(extra)) = (item.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            item.insert(key.clone(), value.clone());
        }
    }
    let counts = item["state"] == "failed"
        || (item["topBar"] == true
            && matches!(item["state"].as_str(), Some("available" | "needsYou")));
    item["counts"] = json!(counts);
    item
}

fn items(state: &str) -> Vec<Value> {
    let mut items = vec![
        item(
            "agent:grok",
            "agent",
            "agents",
            "Grok",
            "grok-build",
            "Agent · mise",
            Some("1.0.70"),
            Some("1.0.70"),
            "upToDate",
            json!({}),
        ),
        item(
            "agent:copilot",
            "agent",
            "agents",
            "Copilot",
            "copilot",
            "Agent · its own updater",
            Some("1.0.105"),
            Some("1.0.105"),
            "upToDate",
            json!({}),
        ),
        item(
            "helper:claude",
            "helper",
            "tools",
            "Claude Swap",
            "claude-swap",
            "Account switcher · uv",
            Some("0.28.0"),
            Some("0.28.0"),
            "upToDate",
            json!({}),
        ),
        item(
            "helper:codex",
            "helper",
            "tools",
            "Codex Swap",
            "codex-swap",
            "Account switcher · install.sh",
            Some("0.9.4"),
            Some("0.9.4"),
            "upToDate",
            json!({}),
        ),
        item(
            "tool:trycua",
            "tool",
            "tools",
            "Fast Computer & Browser Use",
            "trycua",
            "Tool · cua-driver",
            Some("0.4.2"),
            Some("0.4.2"),
            "upToDate",
            json!({}),
        ),
        item(
            "tool:spaceo",
            "tool",
            "tools",
            "SpaceO",
            "spaceo",
            "Tool",
            Some("0.10.0"),
            Some("0.10.0"),
            "upToDate",
            json!({}),
        ),
        item(
            "tool:uv",
            "tool",
            "tools",
            "uv",
            "uv",
            "Tool",
            Some("0.9.7"),
            Some("0.9.7"),
            "upToDate",
            json!({}),
        ),
        item(
            "agent:cursor",
            "agent",
            "agents",
            "Cursor",
            "cursor-cli",
            "Agent · installer",
            Some("2026.11.05-86047df"),
            None,
            "unknown",
            json!({"canAuto": false}),
        ),
    ];
    if state == "updates-done" {
        items.push(item(
            "app",
            "app",
            "ghostex",
            "Ghostex",
            "ghostex",
            "This computer",
            Some("10.47.0"),
            Some("10.47.0"),
            "upToDate",
            json!({}),
        ));
        items.push(item(
            "agent:claude",
            "agent",
            "agents",
            "Claude Code",
            "claude",
            "Agent · mise",
            Some("2.1.318"),
            Some("2.1.318"),
            "upToDate",
            json!({"auto": true}),
        ));
        return items;
    }
    items.extend([
        item("agent:qoder", "agent", "agents", "Qoder", "qoder", "Agent · npm", Some("0.41.2"), Some("0.67.0"), "failed", json!({"error": "npm: EACCES, permission denied /usr/local/lib/node_modules", "major": false})),
        item("extension:lazygit", "extension", "extensions", "Lazygit", "extension", "Extension", Some("1.2.0"), Some("1.3.0"), "needsYou", json!({"error": "This version of lazygit adds a background process or a remote website. Update it from Settings > Extensions so you can approve that first."})),
        item("app", "app", "ghostex", "Ghostex", "ghostex", "This computer", Some("10.47.0"), Some("10.48.0"), "available", json!({"note": "Restarts Ghostex once; agents keep running."})),
        item("machine:studio", "machine", "ghostex", "Studio Mac", "machine", "Remote machine", Some("10.46.2"), Some("10.47.0"), "available", json!({"note": "Restarts Ghostex there; its agents keep running.", "auto": true, "topBar": false})),
        item("agent:claude", "agent", "agents", "Claude Code", "claude", "Agent · mise", Some("2.1.312"), Some("2.1.318"), "available", json!({"note": "4 sessions restart on it when idle", "auto": true})),
        item("agent:codex", "agent", "agents", "Codex", "codex", "Agent · mise", Some("0.165.0"), Some("0.166.0"), "updating", json!({"progress": "Updating…"})),
        item("agent:opencode", "agent", "agents", "OpenCode", "opencode", "Agent · mise", Some("1.19.4"), Some("2.2.5"), "available", json!({"major": true})),
        item("agent:droid", "agent", "agents", "Droid", "factory-droid", "Agent · installer", Some("0.244.0"), Some("1.4.1"), "available", json!({"major": true, "note": "Its updater stops running Droid sessions, so it waits until they're idle."})),
        item("agent:hermes-agent", "agent", "agents", "Hermes", "hermes-agent", "Agent · installer", Some("0.22.5"), None, "available", json!({"summary": "Update available: 503 commits behind origin/main."})),
        item("agent:pi", "agent", "agents", "Pi", "pi", "Agent · mise", Some("1.4.0"), Some("1.5.0"), "available", json!({"note": "1 session restarts on it when idle"})),
        item("agent:omp", "agent", "agents", "OMP", "omp", "Agent · mise", Some("18.13.2"), Some("18.16.0"), "available", json!({})),
        item("agent:dsh", "agent", "agents", "DeepSeek Harness", "dsh", "Agent · mise", Some("0.17.0"), Some("0.18.0"), "available", json!({})),
    ]);
    // The order the app sends: problems, then updates, then the rest.
    let rank = |item: &Value| match item["state"].as_str() {
        Some("failed") => 0,
        Some("needsYou") => 1,
        Some("available" | "updating") => 2,
        Some("unknown") => 4,
        _ => 5,
    };
    let kind = |item: &Value| match item["kind"].as_str() {
        Some("app") => 0,
        Some("machine") => 1,
        Some("agent") => 2,
        _ => 3,
    };
    items.sort_by_key(|item| (rank(item), kind(item)));
    items
}

/// A not installed item as gxserver lists it (server/src/updates/missing.rs).
fn missing(
    id: &str,
    kind: &str,
    group: &str,
    name: &str,
    icon: &str,
    detail: &str,
    extra: Value,
) -> Value {
    let mut item = json!({
        "id": id, "kind": kind, "group": group, "name": name, "icon": icon, "detail": detail,
        "installed": null, "latest": null, "state": "notInstalled", "canInstall": true,
        "canUpdate": false, "canAuto": false, "auto": false, "topBar": false, "counts": false,
        "major": false, "inUse": false,
    });
    if let (Some(item), Some(extra)) = (item.as_object_mut(), extra.as_object()) {
        for (key, value) in extra {
            item.insert(key.clone(), value.clone());
        }
    }
    item
}

/// Another computer's item, as app/updates/remote.rs lists it.
fn on(machine: &str, machine_name: &str, mut item: Value) -> Value {
    let id = item["id"].as_str().unwrap_or_default().to_string();
    if !id.starts_with("machine:") {
        item["id"] = json!(format!("remote:{machine}:{id}"));
    }
    item["computer"] = json!(machine);
    item["computerName"] = json!(machine_name);
    item
}

fn computers_items() -> Vec<Value> {
    let box_id = "remote-linux-box";
    let box_name = "Linux box";
    let mut items = vec![
        item(
            "app",
            "app",
            "ghostex",
            "Ghostex",
            "ghostex",
            "This computer",
            Some("10.47.0"),
            Some("10.48.0"),
            "available",
            json!({"note": "Restarts Ghostex once; agents keep running."}),
        ),
        item(
            "agent:claude",
            "agent",
            "agents",
            "Claude Code",
            "claude",
            "Agent · mise",
            Some("2.1.312"),
            Some("2.1.318"),
            "available",
            json!({"note": "4 sessions restart on it when idle", "auto": true}),
        ),
        item(
            "agent:codex",
            "agent",
            "agents",
            "Codex",
            "codex",
            "Agent · mise",
            Some("0.166.0"),
            Some("0.166.0"),
            "upToDate",
            json!({}),
        ),
        item(
            "tool:spaceo",
            "tool",
            "tools",
            "SpaceO",
            "spaceo",
            "Tool",
            Some("0.10.0"),
            Some("0.10.0"),
            "upToDate",
            json!({}),
        ),
        // The VS Code view's code-server version is a build id (app/updates/items.rs).
        item(
            "component:code",
            "component",
            "ghostex",
            "VS Code view",
            "code",
            "Ghostex component",
            Some(
                "6f3e4df1e6b8-p2-2dbe7754240c2a59f76ef13c11d2bfd240d0e313f144446ced3e2413eecb60d1",
            ),
            Some(
                "6f3e4df1e6b8-p2-2dbe7754240c2a59f76ef13c11d2bfd240d0e313f144446ced3e2413eecb60d1",
            ),
            "upToDate",
            json!({}),
        ),
        missing(
            "agent:kimi",
            "agent",
            "agents",
            "Kimi Code",
            "kimi",
            "Agent · npm",
            json!({}),
        ),
        missing(
            "agent:opencode",
            "agent",
            "agents",
            "OpenCode",
            "opencode",
            "Agent · mise",
            json!({}),
        ),
        missing(
            "agent:pi",
            "agent",
            "agents",
            "Pi",
            "pi",
            "Agent · mise",
            json!({}),
        ),
        missing(
            "tool:hex",
            "tool",
            "tools",
            "Hex",
            "hex",
            "Dictation app",
            json!({}),
        ),
        missing(
            "helper:codex",
            "helper",
            "tools",
            "Codex Swap",
            "codex-swap",
            "Account switcher",
            json!({}),
        ),
        item(
            "machine:remote-linux-box",
            "machine",
            "ghostex",
            box_name,
            "machine",
            "Remote machine",
            Some("10.47.0"),
            Some("10.47.0"),
            "upToDate",
            json!({"topBar": false}),
        ),
        on(
            box_id,
            box_name,
            item(
                "agent:claude",
                "agent",
                "agents",
                "Claude Code",
                "claude",
                "Agent · npm",
                Some("2.1.300"),
                Some("2.1.318"),
                "available",
                json!({"note": "1 session restarts on it when idle"}),
            ),
        ),
        on(
            box_id,
            box_name,
            item(
                "agent:opencode",
                "agent",
                "agents",
                "OpenCode",
                "opencode",
                "Agent · npm",
                Some("1.19.4"),
                Some("2.2.5"),
                "updating",
                json!({"progress": "Waiting…", "major": true}),
            ),
        ),
        on(
            box_id,
            box_name,
            item(
                "tool:uv",
                "tool",
                "tools",
                "uv",
                "uv",
                "Tool",
                Some("0.9.7"),
                Some("0.9.7"),
                "upToDate",
                json!({}),
            ),
        ),
        on(
            box_id,
            box_name,
            missing(
                "agent:codex",
                "agent",
                "agents",
                "Codex",
                "codex",
                "Agent · npm",
                json!({}),
            ),
        ),
        on(
            box_id,
            box_name,
            missing(
                "tool:gh",
                "tool",
                "tools",
                "GitHub CLI",
                "gh",
                "Tool",
                json!({"error": "apt: could not get lock /var/lib/dpkg/lock-frontend"}),
            ),
        ),
        on(
            box_id,
            box_name,
            missing(
                "tool:node",
                "tool",
                "tools",
                "Node.js",
                "node",
                "Tool",
                json!({}),
            ),
        ),
    ];
    for item in &mut items {
        if item["computer"].is_null() {
            item["computer"] = if item["id"]
                .as_str()
                .is_some_and(|id| id.starts_with("machine:"))
            {
                json!(box_id)
            } else {
                json!("local")
            };
        }
    }
    let rank = |item: &Value| match item["state"].as_str() {
        Some("failed") => 0,
        Some("needsYou") => 1,
        Some("available" | "updating") => 2,
        Some("unknown") => 4,
        _ => 5,
    };
    items.sort_by_key(|item| {
        (
            item["computer"] != "local",
            item["kind"] != "machine",
            rank(item),
        )
    });
    items
}

/// The app's answer to a page's `updates` post.
pub(super) fn answers(state: &str, message: &Value) -> Vec<Value> {
    if state.starts_with("updates-computers")
        && message["type"] == "requestRemoteMachineConnectStates"
    {
        return vec![json!({
            "type": "remoteMachineConnectStates",
            "states": { "remote-linux-box": "connected", "remote-air": "sshFailed", "remote-mini": "connected" },
        })];
    }
    if !owns(state) || message["type"] != "updates" || state == "updates-loading" {
        return Vec::new();
    }
    let computers_state = state.starts_with("updates-computers");
    let away = state == "updates-computers-away";
    let mut items = if away {
        computers_items()
            .into_iter()
            .filter(|item| item["computer"] == "local" && item["state"] != "available")
            .collect()
    } else if computers_state {
        computers_items()
    } else {
        items(state)
    };
    if state == "updates-computers-box" {
        // An install running from the Linux box's Node.js pill.
        for item in &mut items {
            if item["id"] == "remote:remote-linux-box:tool:node" {
                item["state"] = json!("updating");
                item["progress"] = json!("Installing…");
            }
        }
    }
    if state == "updates-computers-run" {
        // Mid-run: Ghostex updating here, Claude Code done; the Linux box's Claude Code failed.
        for item in &mut items {
            match item["id"].as_str() {
                Some("app") => {
                    item["state"] = json!("updating");
                    item["progress"] = json!("Downloading 10.48.0…");
                }
                Some("agent:claude") => {
                    item["state"] = json!("upToDate");
                    item["installed"] = json!("2.1.318");
                }
                Some("remote:remote-linux-box:agent:claude") => {
                    item["state"] = json!("failed");
                    item["error"] = json!("npm: EACCES, permission denied /usr/lib/node_modules");
                }
                Some("agent:kimi") => {
                    // An install running from its Not installed pill.
                    item["state"] = json!("updating");
                    item["progress"] = json!("Installing…");
                }
                _ => {}
            }
        }
    }
    let count = items.iter().filter(|item| item["counts"] == true).count();
    let computers = if away {
        json!([
            { "id": "local", "name": "Mac Studio", "thisComputer": true, "state": "ready", "line": null },
            { "id": "remote-linux-box", "name": "Linux box", "thisComputer": false, "state": "offline", "line": "Offline" },
            { "id": "remote-mini", "name": "Old Mini", "thisComputer": false, "state": "tooOld", "line": "Update Ghostex on Old Mini to see its updates" },
        ])
    } else if computers_state {
        json!([
            { "id": "local", "name": "Mac Studio", "thisComputer": true, "state": "ready", "line": null },
            { "id": "remote-linux-box", "name": "Linux box", "thisComputer": false, "state": "ready", "line": null },
            { "id": "remote-air", "name": "MacBook Air", "thisComputer": false, "state": "offline", "line": "Offline" },
            { "id": "remote-mini", "name": "Old Mini", "thisComputer": false, "state": "tooOld", "line": "Update Ghostex on Old Mini to see its updates" },
        ])
    } else {
        json!([{ "id": "local", "name": "Mac Studio", "thisComputer": true, "state": "ready", "line": null }])
    };
    vec![json!({
        "type": "updatesState",
        "items": items,
        "count": count,
        "running": false,
        "error": null,
        "checkedAt": chrono::Utc::now().timestamp_millis() - 4 * 60_000,
        "computers": computers,
    })]
}
