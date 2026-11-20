//! `ghostex debug show-modal <name> | --list`: open a one-time pop-up on this computer's Ghostex
//! window for checks and screenshots, without its once-only marker or any setting changing.
//!
//! The names and why only some pop-ups are listed: `DEBUG_SHOW_MODALS` in
//! packages/gx-core/src/renderer_commands/verbs.rs (CDXC:AppModal 2026-11-20 DECISION). The app
//! opens the preview (apps/desktop/src/app/gx_store/renderer_commands/perform.rs); gxserver lets
//! only this computer ask (server/src/server/route_http/agents.rs `debug_modal_caller_allowed`).

use ghostex_gx_core::DEBUG_SHOW_MODALS;
use serde_json::{json, Value};

use crate::ghostex_cli::args::parse_args;
use crate::ghostex_cli::output::print_json;
use crate::ghostex_cli::rpc::{self, CliError, CliResult};

const USAGE: &str = "Usage: ghostex debug show-modal <name> [--json]
       ghostex debug show-modal --list [--json]

Tools for checking the Ghostex app by hand or from an agent.

  show-modal <name>   Open a one-time pop-up on this computer's Ghostex window, as it
                      looks for real, for checks and screenshots. Its once-only marker
                      and your settings stay as they are: its buttons only change the
                      pop-up, and it closes with Done or Escape. Names follow the
                      native-modal-demo previews; --list prints the ones it can open.
                      Works only for the Ghostex app on this computer: it refuses
                      --computer, another --server, and a terminal over SSH.

  ghostex debug show-modal rail-intro      Meet the rail (the 11.12.0 sidebar pop-up)

Other native-modal-demo previews act for real in the app (they pair devices, install
updates or save settings), so they open only in the preview binary:
  GHOSTEX_NATIVE_MODAL_DEMO=<name> cargo run --release --bin native-modal-demo";

pub fn debug_command(args: &[String]) -> CliResult<()> {
    match args.first().map(String::as_str) {
        Some("show-modal") => show_modal(&args[1..]),
        None | Some("-h" | "--help" | "help") => {
            println!("{USAGE}");
            Ok(())
        }
        Some(other) => Err(CliError::Other(format!(
            "Unknown debug command: {other}.\n\n{USAGE}"
        ))),
    }
}

fn show_modal(args: &[String]) -> CliResult<()> {
    if args.iter().any(|arg| arg == "-h" || arg == "--help") {
        println!("{USAGE}");
        return Ok(());
    }
    let parsed = parse_args(args);
    let flags = parsed.flags;
    let json = flags.truthy("json");
    if flags.truthy("list") {
        if json {
            print_json(&json!({ "ok": true, "modals": DEBUG_SHOW_MODALS }));
        } else {
            for name in DEBUG_SHOW_MODALS {
                println!("{name}");
            }
        }
        return Ok(());
    }
    let name = match parsed.rest.as_slice() {
        [name] => name.trim().to_string(),
        _ => return Err(CliError::Other(USAGE.to_string())),
    };
    if !DEBUG_SHOW_MODALS.contains(&name.as_str()) {
        return Err(CliError::Other(format!(
            "show-modal can't open {name}. It opens: {}. Other native-modal-demo previews act for real in the app, so they open only in the preview: GHOSTEX_NATIVE_MODAL_DEMO={name} cargo run --release --bin native-modal-demo",
            DEBUG_SHOW_MODALS.join(", ")
        )));
    }
    if flags.contains("computer") {
        return Err(CliError::Other(
            "show-modal opens a pop-up in this computer's own Ghostex app; it doesn't go to another computer. Run it on that computer instead.".into(),
        ));
    }
    if std::env::var_os("SSH_CONNECTION").is_some() || std::env::var_os("SSH_TTY").is_some() {
        return Err(CliError::Other(
            "show-modal opens a pop-up on this computer's screen, so it doesn't run over SSH from another computer or a phone. Run it in a terminal on this computer.".into(),
        ));
    }
    let target = rpc::resolve_gxserver_server_target(&flags, &json!({}))?;
    if target.kind != "local" {
        return Err(CliError::Other(
            "show-modal opens a pop-up in this computer's own Ghostex app, not one reached with --server.".into(),
        ));
    }
    let result = rpc::call_gxserver_rpc(
        "/api/dispatchRendererCommand",
        &json!({ "action": "showDebugModal", "payload": { "name": name } }),
        &flags,
    )?;
    if json {
        print_json(&result);
    } else if result.get("ok") == Some(&Value::Bool(false)) {
        return Err(CliError::Other(
            result
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("The app could not open the pop-up.")
                .to_string(),
        ));
    } else {
        println!(
            "Opened {name} on the Ghostex window. Nothing was saved; close it with Done or Escape."
        );
    }
    Ok(())
}
