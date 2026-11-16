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
//!
//! CDXC:Security 2026-11-16 DECISION:
//! User (W524, "Owner only"): only the Ghostex app or a paired phone makes pairing codes and lets
//! new devices in. Once this computer has either, the token file this command uses (which any agent
//! can read) gets the code only after you approve "Show this computer's pairing code" there (the
//! CLI's usual wait, then ten minutes of codes), and it can't approve a device: each one that waits
//! is shown here with its words to approve in the app or on your phone, and the command follows its
//! answer. On a computer with neither yet, nothing asks and y/n here works as before
//! (`mayApproveDevices` in `/api/securityStatus`; CDXC:Security 2026-11-16 in security/policy.rs).

use std::collections::HashSet;
use std::io::{BufRead, IsTerminal, Write};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::args::parse_args;
use super::output::print_json;
use super::rpc::{call_gxserver_rpc, CliError, CliResult};

/// As often as the Connect a Computer window asks; the pairing window stays open 60 s after each.
const POLL_EVERY: Duration = Duration::from_secs(3);

/// How long it keeps the pairing window open with no computer or phone waiting.
const IDLE_LIMIT: Duration = Duration::from_secs(10 * 60);

pub(crate) const HELP: &str = "Usage: ghostex computer-code [--once] [--json]
       gxserver computer-code [--once] [--json]

Prints this computer's pairing code, the same code Connect a Computer shows (Settings > Remote,
or Add a computer). Paste it into Connect a Computer on the other computer to connect it to this
one. It works on a server without the Ghostex app too: gxserver must be running here and Easy
Connect on (ghostex tailcat enable).

Once this computer has the Ghostex app or a paired phone, only they let a new device in:
  - First you approve \"Show this computer's pairing code\" in the Ghostex app or on your phone;
    then the code prints. After 10 minutes it asks again.
  - Each computer or phone that uses the code shows here with its three words. Approve it in the
    app or on your phone, where it shows with the same words.
On a computer with neither yet, nothing asks: check the words match the ones on the other
computer, then answer y to approve it or n to deny it here.

It keeps running so the other computer can reach this one. A code works once and lapses after
five minutes; a new one is printed when it does. It stops by itself after 10 minutes with no
computer waiting, and once the first computer is approved. Press Ctrl+C to stop sooner.

  --once   Print the code and exit (connect within a minute, then approve in the app or phone)
  --json   Print the code as JSON and exit: { ok, computerName, code }

Run ghostex computer-code in your own terminal, not in a Ghostex session.";

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
    let answers_here = may_approve_here(&parsed.flags);
    let mut shown_code = code.text;
    let mut seen = HashSet::new();
    // Devices to approve in the app or on a phone: approval id → name, followed until answered.
    let mut approved_elsewhere: Vec<(String, String)> = Vec::new();
    let mut last_waiting = Instant::now();
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
        let waiting = waiting_devices(&answer);
        if let Some(done) = follow_answers(&mut approved_elsewhere, &waiting, &parsed.flags) {
            println!("{done}");
            return Ok(());
        }
        if waiting
            .iter()
            .any(|device| !seen.contains(&device.approval_id))
        {
            last_waiting = Instant::now();
        } else if last_waiting.elapsed() >= IDLE_LIMIT {
            println!("Stopped: no computer waited for 10 minutes. Run ghostex computer-code to start again.");
            return Ok(());
        }
        for device in waiting {
            if !seen.insert(device.approval_id.clone()) {
                continue;
            }
            println!(
                "\n{} wants to connect. Its words: {}",
                device.name,
                device.words.join(" ")
            );
            if !answers_here || !interactive {
                println!(
                    "Approve it in the Ghostex app on this computer or on your phone, where it shows with the same words."
                );
                approved_elsewhere.push((device.approval_id.clone(), device.name.clone()));
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
                Ok(_) if approve => {
                    println!(
                        "{done}: {}. Stopped after the first computer. Run ghostex computer-code to connect another.",
                        device.name
                    );
                    return Ok(());
                }
                Ok(_) => println!("{done}: {}.", device.name),
                Err(error) => eprintln!("{error}"),
            }
        }
    }
}

/// Whether y/n here may approve a device: gxserver says so for the token file only while this
/// computer has no owner device (`mayApproveDevices`); a gxserver from before it always let it.
fn may_approve_here(flags: &super::args::Flags) -> bool {
    call_gxserver_rpc(crate::security::SECURITY_STATUS_ENDPOINT, &json!({}), flags)
        .ok()
        .and_then(|status| status.get("mayApproveDevices").and_then(Value::as_bool))
        .unwrap_or(true)
}

/// Follows the devices waiting for an Approve in the app or on a phone: says how each one that is
/// no longer waiting ended, and returns the last words once one was approved.
fn follow_answers(
    asked: &mut Vec<(String, String)>,
    waiting: &[WaitingDevice],
    flags: &super::args::Flags,
) -> Option<String> {
    let mut approved = None;
    asked.retain(|(id, name)| {
        if waiting.iter().any(|device| &device.approval_id == id) {
            return true;
        }
        let status = call_gxserver_rpc(
            crate::security::APPROVALS_READ_ENDPOINT,
            &json!({ "id": id }),
            flags,
        )
        .ok()
        .and_then(|read| {
            read.pointer("/request/status")
                .and_then(Value::as_str)
                .map(str::to_string)
        });
        match status.as_deref() {
            Some("approved" | "consumed") => {
                approved = Some(format!(
                    "Approved: {name}. Stopped after the first computer. Run ghostex computer-code to connect another."
                ));
            }
            Some("denied") => println!("Denied: {name}."),
            _ => println!("{name} is no longer waiting (it lapsed or was removed)."),
        }
        false
    });
    approved
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
            "ghostex computer-code does not run inside a Ghostex session. Run ghostex computer-code in your own terminal.".to_string(),
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
