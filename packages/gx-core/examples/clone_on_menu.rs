//! Prints each project's Clone on… row in the one-list Project sidebar, built from real
//! presentations: this computer's and a connected machine's, as two gxservers answer
//! `/api/readPresentationSnapshot`, plus a saved machine that is not connected.
//!
//! Usage: `cargo run --example clone_on_menu -- <this computer's snapshot.json> <machine's snapshot.json>`
//! (each file the RPC's whole answer, `{ ok, result: { snapshot } }`).
//!
//! This is tooling, not a test suite: it prints what the menu holds.

use std::process::ExitCode;

use ghostex_gx_core::{
    Core, MachineId, MachineTabInput, MenuHost, SidebarInputs, SidebarMenus, SidebarViewModel,
    MACHINE_STATE_CONNECTED,
};
use serde_json::{json, Value};

const NOW_MS: u64 = 1_790_000_000_000;
const REMOTE: &str = "machine-bigbox";
const OFFLINE: &str = "machine-studio";

fn frame(path: &str) -> String {
    let answer: Value =
        serde_json::from_str(&std::fs::read_to_string(path).expect("a snapshot file"))
            .expect("JSON");
    let snapshot = answer["result"]["snapshot"].clone();
    json!({
        "type": "presentationSnapshot",
        "protocolVersion": ghostex_gx_core::protocol::GXSERVER_PROTOCOL_VERSION,
        "serverId": snapshot["serverId"].as_str().unwrap_or("S00"),
        "revision": snapshot["revision"].as_u64().unwrap_or(1),
        "snapshot": snapshot,
    })
    .to_string()
}

fn tab(machine_id: &str, label: &str, state: &str, fed: bool) -> MachineTabInput {
    MachineTabInput {
        machine_id: machine_id.to_string(),
        label: label.to_string(),
        short_name: String::new(),
        state: state.to_string(),
        message: None,
        fed,
        this_computer: false,
    }
}

fn print_menu(items: &[Value], depth: usize) {
    for item in items {
        if item["separator"] == true {
            continue;
        }
        let label = item["label"].as_str().unwrap_or("");
        if depth == 0 && !label.starts_with("Clone on") {
            continue;
        }
        let mut line = format!("{}{label}", "    ".repeat(depth + 1));
        if let Some(suffix) = item["suffix"].as_str() {
            line.push_str(&format!("   [{suffix}]"));
        }
        if item["disabled"] == true {
            line.push_str("   (greyed)");
        }
        if item["command"].is_object() {
            line.push_str(&format!("   -> {}", item["command"]));
        }
        println!("{line}");
        if let Some(children) = item["children"].as_array() {
            print_menu(children, depth + 1);
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let [local, remote] = args.as_slice() else {
        eprintln!("usage: clone_on_menu <this computer's snapshot.json> <machine's snapshot.json>");
        return ExitCode::FAILURE;
    };
    let mut core = Core::new();
    core.handle_raw_frame(MachineId::Local, &frame(local), NOW_MS)
        .expect("this computer's frame parses");
    core.handle_raw_frame(
        MachineId::Remote(REMOTE.to_string()),
        &frame(remote),
        NOW_MS,
    )
    .expect("the machine's frame parses");
    let mut inputs = SidebarInputs::default();
    inputs.host.machines = vec![
        tab("local", "This computer", MACHINE_STATE_CONNECTED, true),
        tab(REMOTE, "Bigbox", MACHINE_STATE_CONNECTED, true),
        tab(OFFLINE, "Studio", "disconnected", false),
    ];
    inputs.host.merge_machines = true;
    inputs.ui.selected_machine_id = "local".to_string();
    let view = SidebarViewModel::build_from_scratch(&core, &inputs, NOW_MS);
    let host = MenuHost::default();
    let menus = SidebarMenus::new(&core, &view, &inputs, &host, NOW_MS);
    for group in &view.groups {
        let Some(project) = group.core.project_context.as_ref() else {
            continue;
        };
        let machines: Vec<&str> = group
            .core
            .machine_names
            .iter()
            .map(|chip| chip.name.as_str())
            .collect();
        println!(
            "{} ({}) on: {}",
            group.core.title,
            project
                .git_remote_origin_url
                .as_deref()
                .unwrap_or("no git remote"),
            if machines.is_empty() {
                "this computer".to_string()
            } else {
                machines.join(", ")
            }
        );
        let menu: Vec<Value> = menus
            .project_menu(group)
            .iter()
            .map(|item| item.to_json())
            .collect();
        print_menu(&menu, 0);
    }
    ExitCode::SUCCESS
}
