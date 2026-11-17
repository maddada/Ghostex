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

pub(crate) const HELP: &str = "Usage: ghostex computer-code [--once] [--json] [--copy]
       gxserver computer-code [--once] [--json] [--copy]

Prints this computer's pairing code, the same code Connect a Computer shows (Settings > Remote,
or Add a computer). Paste it into Connect a Computer on the other computer to connect it to this
one. It works on a server without the Ghostex app too: gxserver must be running here and Easy
Connect on (ghostex tailcat enable).

Once this computer has an owner (the Ghostex app on one of your other computers, or a paired
phone), the code prints at once, and each computer or phone that uses it shows here with its
three words. Approve it in Ghostex on one of them, where it asks \"Let <it> connect to <this
computer>?\" with the same words; this terminal names them. You don't need this computer's screen.
When this computer's only owner is its own Ghostex app, the first computer to join can be let in
here instead (y), and it can then ask to manage this computer (y again, with its words).
On a computer with no owner yet, nothing asks: check the words match the ones on the other
computer, then answer y to approve it or n to deny it here.

It keeps running so the other computer can reach this one. A code works once and lapses after
five minutes; a new one is printed when it does. It stops by itself after 10 minutes with no
computer waiting, and once the first computer is approved. Press Ctrl+C to stop sooner.

  --once   Print the code and exit (connect within a minute, then approve in the app or phone)
  --json   Print the code as JSON and exit: { ok, computerName, code }
  --copy   Also copy each code to the clipboard (over SSH, your terminal's clipboard when it
           allows that), so you don't have to select the long text

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
    let copy = parsed.flags.truthy("copy");
    print_code(&code, true, copy);
    if parsed.flags.truthy("once") {
        return Ok(());
    }
    println!("Waiting for the other computer. Press Ctrl+C when you are done.");
    let interactive = std::io::stdin().is_terminal();
    let answers_here = may_approve_here(&parsed.flags);
    let status = call_gxserver_rpc(
        crate::security::SECURITY_STATUS_ENDPOINT,
        &json!({}),
        &parsed.flags,
    )
    .ok();
    // Where a waiting computer is approved when it can't be here (CDXC:Security 2026-11-17 at
    // `approvers_here` in server/src/server/security_http.rs): the owners' apps and phones by name.
    let place = status.as_ref().and_then(super::rpc::where_to_approve);
    // Only this computer's own app owns it: the first computer to join may be let in here, and
    // may then manage it (CDXC:Security 2026-11-17 DECISION at `terminal_may_approve`).
    let hands_over = status
        .as_ref()
        .and_then(|status| status.get("terminalMayApproveJoin"))
        .and_then(Value::as_bool)
        == Some(true);
    let here = code.computer_name.clone();
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
                print_code(&code, false, copy);
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
            let phone = matches!(device.platform.as_str(), "ios" | "android");
            if hands_over && !answers_here && interactive && !phone {
                println!(
                    "Only {here}'s own Ghostex app owns {here}, so you can let {} in here. It can then ask to manage {here}.",
                    device.name
                );
                let approve =
                    ask_yes_no("Do they match the words on that computer? Approve it? [y/N] ");
                if !approve {
                    let _ = call_gxserver_rpc(
                        "/api/denyApprovalRequest",
                        &json!({ "id": device.approval_id }),
                        &parsed.flags,
                    );
                    println!("Denied: {}.", device.name);
                    continue;
                }
                if requires_presence(&parsed.flags) && confirm_it_is_you(&device.name).is_err() {
                    println!("Not approved: {} is still waiting.", device.name);
                    seen.remove(&device.approval_id);
                    continue;
                }
                match call_gxserver_rpc(
                    "/api/approveApprovalRequest",
                    &json!({ "id": device.approval_id }),
                    &parsed.flags,
                ) {
                    Ok(_) => {
                        hand_over(&device.name, &here, &parsed.flags);
                        return Ok(());
                    }
                    Err(error) => {
                        eprintln!("{error}");
                        continue;
                    }
                }
            }
            if !answers_here || !interactive {
                match &place {
                    Some(place) => println!("Approve it {place}."),
                    None => println!(
                        "Approve it in Ghostex on a computer or phone that owns {here}, where it shows with the same words."
                    ),
                }
                println!("Waiting… (Ctrl+C to stop)");
                approved_elsewhere.push((device.approval_id.clone(), device.name.clone()));
                continue;
            }
            let approve =
                ask_yes_no("Do they match the words on that computer? Approve it? [y/N] ");
            if approve && requires_presence(&parsed.flags) {
                match confirm_it_is_you(&device.name) {
                    Ok(()) => {}
                    Err(PresenceFailure::Cancelled(message)) => {
                        // Closed or failed the prompt: ask again on the next poll, so n still denies.
                        println!("{message} Not approved.");
                        seen.remove(&device.approval_id);
                        continue;
                    }
                    Err(PresenceFailure::Unavailable(message)) => {
                        println!("{message}");
                        match &place {
                            Some(place) => println!(
                                "Not approved: {} is still waiting. Approve it {place} instead.",
                                device.name
                            ),
                            None => println!(
                                "Not approved: {} is still waiting. Approve it in Ghostex on a computer or phone paired with {here} instead.",
                                device.name
                            ),
                        }
                        continue;
                    }
                }
            }
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

/// Whether Settings > Security > "Require Touch ID or your password to approve" is on for this
/// computer (`requirePresence` in `/api/securityStatus`; a gxserver from before it never asks).
fn requires_presence(flags: &super::args::Flags) -> bool {
    call_gxserver_rpc(crate::security::SECURITY_STATUS_ENDPOINT, &json!({}), flags)
        .ok()
        .and_then(|status| status.get("requirePresence").and_then(Value::as_bool))
        .unwrap_or(false)
}

enum PresenceFailure {
    /// The prompt showed and was closed or failed: they may try again.
    Cancelled(String),
    /// No prompt can be shown here (a server with no screen, no polkit agent, an SSH session).
    Unavailable(String),
}

/*
CDXC:Security 2026-11-16 DECISION:
User (W524, gap 2 of Connect a Computer, option A, all platforms): with "Require Touch ID or your
password to approve" on, a y here does what the app's Approve does: the system's own presence
prompt first (`ghostex_owner_keys::confirm_presence`: Touch ID or the Mac password, Windows Hello,
polkit), then the plain approval. A computer where no prompt can be shown (a Linux server with no
screen or polkit agent, a Windows SSH session) refuses and points to the app or a paired phone
instead of approving unchecked. The prompt shows on the screen of the computer this runs on, so a
terminal over SSH to a Mac or Linux desktop still asks the person at its screen.
*/
fn confirm_it_is_you(device_name: &str) -> Result<(), PresenceFailure> {
    let ssh = std::env::var_os("SSH_CONNECTION").is_some() || std::env::var_os("SSH_TTY").is_some();
    let screen =
        std::env::var_os("DISPLAY").is_some() || std::env::var_os("WAYLAND_DISPLAY").is_some();
    // polkit needs a desktop session's agent to show its password prompt; Windows Hello can't
    // show in an SSH session. (macOS shows its sheet on the Mac's own screen.)
    if (cfg!(target_os = "linux")
        && !screen
        && (ssh || std::env::var_os("XDG_SESSION_TYPE").is_none_or(|kind| kind == "tty")))
        || (cfg!(target_os = "windows") && ssh)
    {
        return Err(PresenceFailure::Unavailable(
            "This computer asks for Touch ID, Windows Hello or your password to approve, and it can't show that prompt in this terminal.".to_string(),
        ));
    }
    println!("Confirm it's you on this computer to approve {device_name}...");
    match ghostex_owner_keys::confirm_presence(&format!("Approve {device_name} connecting to this computer")) {
        Ok(_) => Ok(()),
        Err(ghostex_owner_keys::KeyError::PresenceUnavailable(detail)) => {
            Err(PresenceFailure::Unavailable(format!(
                "This computer asks for Touch ID, Windows Hello or your password to approve, and it can't show that prompt here ({detail})."
            )))
        }
        Err(error) => Err(PresenceFailure::Cancelled(error.to_string())),
    }
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

fn print_code(code: &ComputerCode, first: bool, copy: bool) {
    if first {
        println!(
            "{}'s code. Paste it into Connect a Computer on the other computer:\n",
            code.computer_name
        );
    } else {
        println!("\nThe last code was used or lapsed. The new code:\n");
    }
    println!("{}\n", code.text);
    if copy {
        println!("{}\n", copy_to_clipboard(&code.text));
    }
}

/// Puts `text` on the clipboard and says how it went: this computer's clipboard tool when it has
/// one that works, else the terminal's own clipboard (OSC 52, which most terminals accept even over
/// SSH, and which has no answer to wait for).
fn copy_to_clipboard(text: &str) -> String {
    let tools: &[(&str, &[&str])] = if cfg!(target_os = "macos") {
        &[("pbcopy", &[])]
    } else if cfg!(target_os = "windows") {
        &[("clip", &[])]
    } else {
        &[
            ("wl-copy", &[]),
            ("xclip", &["-selection", "clipboard"]),
            ("xsel", &["--clipboard", "--input"]),
        ]
    };
    for (tool, args) in tools {
        let Ok(mut child) = std::process::Command::new(tool)
            .args(*args)
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        else {
            continue;
        };
        if let Some(mut stdin) = child.stdin.take() {
            let _ = stdin.write_all(text.as_bytes());
        }
        if child.wait().is_ok_and(|status| status.success()) {
            return "Copied to the clipboard.".to_string();
        }
    }
    if std::io::stdout().is_terminal() {
        use base64::Engine as _;
        let encoded = base64::engine::general_purpose::STANDARD.encode(text);
        print!("\x1b]52;c;{encoded}\x07");
        let _ = std::io::stdout().flush();
        return "Asked your terminal to copy it. If it didn't, select the code above instead."
            .to_string();
    }
    "Could not copy it here. Select the code above instead.".to_string()
}

struct WaitingDevice {
    name: String,
    /// "ios", "android", or the computer's system.
    platform: String,
    words: Vec<String>,
    approval_id: String,
}

/// After this terminal let `name` in on a computer only its own app owned: waits (ten minutes at
/// most) for `name`'s app to ask to manage `here`, and approves it on y with matching words
/// (CDXC:Security 2026-11-17 DECISION at `terminal_may_approve` in server/src/security/policy.rs).
fn hand_over(name: &str, here: &str, flags: &super::args::Flags) {
    println!("Let in: {name}.");
    println!(
        "Waiting for {name} to ask to manage {here}: it shows the words to check there. (Ctrl+C to skip; {name} still connects.)"
    );
    let started = Instant::now();
    let mut seen: HashSet<String> = HashSet::new();
    while started.elapsed() < IDLE_LIMIT {
        std::thread::sleep(POLL_EVERY);
        let Ok(listed) = call_gxserver_rpc("/api/listApprovalRequests", &json!({}), flags) else {
            continue;
        };
        let asks = listed
            .get("requests")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(|request| {
                serde_json::from_value::<ghostex_gx_protocol::approval::ApprovalRequest>(
                    request.clone(),
                )
                .ok()
            })
            .filter(|request| {
                request.status == ghostex_gx_protocol::approval::ApprovalStatus::Pending
                    && request.action
                        == ghostex_gx_protocol::approval::SensitiveAction::AddApproverKey
            });
        for request in asks {
            if !seen.insert(request.id.clone()) {
                continue;
            }
            let Some(ask) = ghostex_gx_protocol::plain_ask::plain_ask(&request, here) else {
                continue;
            };
            if !ask.question.contains(" manage ") {
                continue;
            }
            println!("\n{}", ask.question);
            println!("Its words: {}", ask.words.join(" "));
            if !ask_yes_no(&format!(
                "Do they match the words on {name}? Approve it? [y/N] "
            )) {
                let _ = call_gxserver_rpc(
                    "/api/denyApprovalRequest",
                    &json!({ "id": request.id }),
                    flags,
                );
                println!(
                    "Denied. {name} can still open {here}; it can't add phones or computers to it."
                );
                return;
            }
            match call_gxserver_rpc(
                "/api/approveApprovalRequest",
                &json!({ "id": request.id }),
                flags,
            ) {
                Ok(_) => {
                    println!(
                        "{name} now manages {here}. Approve new phones and computers for {here} in Ghostex on {name}; this terminal approves nothing more."
                    );
                    return;
                }
                Err(error) => eprintln!("{error}"),
            }
        }
    }
    println!(
        "{name} didn't ask to manage {here} within 10 minutes. It can still open {here}, but can't add phones or computers to it."
    );
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
                platform: text("platform").unwrap_or_default().to_lowercase(),
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
