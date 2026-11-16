//! `ghostex computer-code` (also `gxserver computer-code`): this computer's pairing code, the one
//! the Connect a Computer window shows, from a terminal.
//!
//! CDXC:RemotePairing 2026-11-16 DECISION:
//! A Discord report (Linux, labwc): "Generate token or code from gxserver cli". A server without the
//! app had no way to show its code, and a computer pairing with it waits as Pending until it is
//! approved here, which only the app's card (or a paired phone) could do. This command asks the
//! running gxserver for the code the way the window does (`/api/remotePairingCode` with
//! `openTunnel`, which only you get: the token file in your own terminal, never an agent's session
//! token, CDXC:RemotePairing 2026-11-08 in remote_access/http.rs), keeps asking every 3 seconds so
//! the pairing window stays open, and shows each computer or phone that then waits with its three
//! words to approve or deny here. It lives in `ghostex` beside `tailcat` and `computers` because
//! gxserver makes the code; `gxserver computer-code` runs the same thing (one program, `main.rs`).

use std::collections::HashSet;
use std::io::{BufRead, IsTerminal, Write};
use std::time::Duration;

use serde_json::{json, Value};

use super::args::parse_args;
use super::output::print_json;
use super::rpc::{call_gxserver_rpc, CliError, CliResult};

/// As often as the Connect a Computer window asks; the pairing window stays open 60 s after each.
const POLL_EVERY: Duration = Duration::from_secs(3);

pub(crate) const HELP: &str = "Usage: ghostex computer-code [--once] [--json]
       gxserver computer-code [--once] [--json]

Prints this computer's pairing code, the same code Connect a Computer shows (Settings > Remote,
or Add a computer). Paste it into Connect a Computer on the other computer to connect it to this
one. It works on a server without the Ghostex app too: gxserver must be running here and Easy
Connect on (ghostex tailcat enable).

It keeps running so the other computer can reach this one, and shows each computer (or phone)
that then asks to connect with its three words. Check they match the words on that computer,
then answer y to approve it or n to deny it. Press Ctrl+C when you are done.

A code works once and lapses after five minutes; a new one is printed when it does.

  --once   Print the code and exit (connect within a minute, or approve in the app)
  --json   Print the code as JSON and exit: { ok, computerName, code }

Run it in your own terminal: an agent's session can't make a pairing code.";

pub(crate) fn run(args: &[String]) -> CliResult<()> {
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
    {
        println!("{HELP}");
        return Ok(());
    }
    let parsed = parse_args(args);
    if let Some(extra) = parsed.rest.first() {
        return Err(CliError::Other(format!(
            "computer-code takes no arguments (got {extra}).\n\n{HELP}"
        )));
    }
    let json_output = parsed.flags.truthy("json");
    let code = read_code(&parsed.flags)?;
    if json_output {
        print_json(&json!({
            "ok": true,
            "computerName": code.computer_name,
            "code": code.text,
        }));
        return Ok(());
    }
    print_code(&code, true);
    if parsed.flags.truthy("once") {
        return Ok(());
    }
    println!("Waiting for the other computer. Press Ctrl+C when you are done.");
    let interactive = std::io::stdin().is_terminal();
    let mut shown_code = code.text;
    let mut seen = HashSet::new();
    loop {
        std::thread::sleep(POLL_EVERY);
        // A used or lapsed code comes back as a new one.
        match read_code(&parsed.flags) {
            Ok(code) if code.text != shown_code => {
                print_code(&code, false);
                shown_code = code.text;
            }
            Ok(_) => {}
            Err(error) => eprintln!("{error}"),
        }
        let answer = match call_gxserver_rpc("/api/pairedDevices", &json!({}), &parsed.flags) {
            Ok(answer) => answer,
            Err(error) => {
                eprintln!("{error}");
                continue;
            }
        };
        for device in waiting_devices(&answer) {
            if !seen.insert(device.approval_id.clone()) {
                continue;
            }
            println!(
                "\n{} wants to connect. Its words: {}",
                device.name,
                device.words.join(" ")
            );
            if !interactive {
                println!("Approve it in the Ghostex app or on your phone.");
                continue;
            }
            let approve =
                ask_yes_no("Do they match the words on that computer? Approve it? [y/N] ");
            let (endpoint, done) = if approve {
                ("/api/approveApprovalRequest", "Approved")
            } else {
                ("/api/denyApprovalRequest", "Denied")
            };
            match call_gxserver_rpc(
                endpoint,
                &json!({ "id": device.approval_id }),
                &parsed.flags,
            ) {
                Ok(_) => println!("{done}: {}.", device.name),
                Err(error) => eprintln!("{error}"),
            }
        }
    }
}

struct ComputerCode {
    computer_name: String,
    text: String,
}

/// The code that pairs (it carries the one-time secret), as the Connect a Computer window reads it
/// (`poll` in apps/desktop/src/app/connect_device_modal_lifecycle.rs).
fn read_code(flags: &super::args::Flags) -> CliResult<ComputerCode> {
    let answer = call_gxserver_rpc(
        "/api/remotePairingCode",
        &json!({ "openTunnel": true }),
        flags,
    )?;
    let Some(entry) = answer.get("easyConnect") else {
        return Err(CliError::Other(
            "Easy Connect is off on this computer, so it has no code. Turn it on in Settings > Remote, or run: ghostex tailcat enable".to_string(),
        ));
    };
    let text_of = |key: &str| {
        entry
            .get(key)
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let (payload, text) = (text_of("payload"), text_of("text"));
    if payload.is_empty() || text.is_empty() {
        return Err(CliError::Other(
            "Only you can make a pairing code: run ghostex computer-code in your own terminal, not from an agent's session.".to_string(),
        ));
    }
    Ok(ComputerCode {
        computer_name: entry
            .pointer("/code/name")
            .and_then(Value::as_str)
            .unwrap_or("This computer")
            .to_string(),
        text,
    })
}

fn print_code(code: &ComputerCode, first: bool) {
    if first {
        println!(
            "{}'s code. Paste it into Connect a Computer on the other computer:\n",
            code.computer_name
        );
    } else {
        println!("\nThe last code was used or lapsed. The new code:\n");
    }
    println!("{}\n", code.text);
}

struct WaitingDevice {
    name: String,
    words: Vec<String>,
    approval_id: String,
}

/// `/api/pairedDevices` `pending`: the computers and phones waiting for approval.
fn waiting_devices(answer: &Value) -> Vec<WaitingDevice> {
    answer
        .get("pending")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|device| {
            let text = |key: &str| device.get(key).and_then(Value::as_str).map(str::trim);
            Some(WaitingDevice {
                name: text("name")
                    .filter(|name| !name.is_empty())
                    .unwrap_or("A new device")
                    .to_string(),
                words: device
                    .get("words")?
                    .as_array()?
                    .iter()
                    .filter_map(|word| word.as_str().map(str::to_string))
                    .collect(),
                approval_id: text("approvalId").filter(|id| !id.is_empty())?.to_string(),
            })
        })
        .collect()
}

fn ask_yes_no(question: &str) -> bool {
    print!("{question}");
    let _ = std::io::stdout().flush();
    let mut line = String::new();
    if std::io::stdin().lock().read_line(&mut line).is_err() {
        return false;
    }
    matches!(line.trim().to_ascii_lowercase().as_str(), "y" | "yes")
}
