//! `ghostex grokbot …`: the Grok Bot connection (server/src/grokbot/). The webhook key is read
//! here without echo, from stdin, or from the private secret card, handed to gxserver once, and
//! never printed or put in an environment variable.

use std::io::{BufRead, IsTerminal, Write};

use serde_json::{json, Map, Value};

use crate::ghostex_cli::agents::identity;
use crate::ghostex_cli::args::{parse_args, Flags, ParsedArgs};
use crate::ghostex_cli::output::print_json;
use crate::ghostex_cli::rpc::{call_gxserver_rpc, CliError, CliResult};
use crate::ghostex_cli::{notify, secret, set_exit_code};

const ENDPOINT: &str = "/api/grokbot";
/// The published Ghostex bot (CDXC:GrokBot 2026-11-19 in tabs/grok_bot.rs).
const TEMPLATE_URL: &str = "https://grok-bot.directory/bots/ghostex";
/// `connect` and `test` wait for Grok Bot's answer (up to 20 seconds) inside the call.
const SLOW_CALL_TIMEOUT_MS: &str = "45000";

pub(crate) fn usage() -> String {
    r#"Usage:
  ghostex grokbot [status] [--json]          Whether Grok Bot is connected, the last note sent and any error
  ghostex grokbot connect [<url>] [--key-stdin] [--json]
                                             Connect the "Ghostex reports" routine's webhook: <url> is its
                                             POST to address; the key is asked without showing it (or read
                                             from stdin with --key-stdin; inside an agent session, on a
                                             private card in its chat). Ghostex keeps both and sends a test
  ghostex grokbot notify [--kind ask|blocked|done|report|info] "<text>" [--json]
                                             Send the user a note through Grok Bot (queued and retried;
                                             at most one call every 20 seconds, notes waiting go together)
  ghostex grokbot test [--json]              Send a test note now
  ghostex grokbot disconnect [--json]        Forget the address and key, and drop notes not sent yet
  ghostex grokbot brief                      The Grok Bot bot's current commands and rules (it runs this)
  ghostex grokbot instructions               The instructions to paste into a Ghostex bot of your own
                                             (or add the ready-made one: https://grok-bot.directory/bots/ghostex)

The key never leaves this computer except to Grok Bot, and no command prints it. Stop the bot
reaching this computer in Grok Bot: Settings > Computer > Execution on this computer: Never allow.
More: the "Grok Bot" section of `ghostex guide features`."#
        .to_string()
}

pub(crate) fn run(args: &[String]) -> CliResult<()> {
    let verb = args.first().map(String::as_str).unwrap_or("status");
    let rest = args.get(1..).unwrap_or_default();
    // `notify`'s words are its message, so only a leading -h/--help asks for help there: an
    // unquoted `notify I need help` is sent, never answered with the usage.
    let wants_help = if verb == "notify" || verb == "send" {
        matches!(rest.first().map(String::as_str), Some("-h" | "--help"))
    } else {
        rest.iter()
            .any(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
    };
    if wants_help {
        println!("{}", usage());
        return Ok(());
    }
    let parsed = parse_args(rest);
    match verb {
        "help" | "-h" | "--help" => {
            println!("{}", usage());
            Ok(())
        }
        "status" | "--json" => status_command(&parse_args(args)),
        "connect" => connect_command(&parsed),
        "notify" | "send" => notify_command(&parsed),
        "test" => simple_command("test", &parsed, "Test note sent. Look for it in the Ghostex bot's chat."),
        "disconnect" => simple_command(
            "disconnect",
            &parsed,
            "Grok Bot disconnected: Ghostex forgot the webhook address and key. Delete the routine's webhook in Grok Bot too.",
        ),
        "brief" => brief_command(&parsed),
        "instructions" => {
            // stderr, so what's printed on stdout stays exactly the text to paste.
            eprintln!("The ready-made Ghostex bot: {TEMPLATE_URL} (add it to Grok Bot). To build your own, paste the text below into a new bot named Ghostex.\n");
            print!("{}", crate::grokbot::INSTRUCTIONS);
            Ok(())
        }
        other => Err(CliError::Other(format!(
            "Unknown grokbot command: {other}\n\n{}",
            usage()
        ))),
    }
}

fn slow_flags(flags: &Flags) -> Flags {
    let mut flags = flags.clone();
    flags.0.remove("timeoutMs");
    flags.insert_text("timeout", SLOW_CALL_TIMEOUT_MS);
    flags
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// CDXC:GrokBot 2026-11-19 WHY: the bot runs `brief` at the start of every chat, so its first run
/// is how Settings > Grok Bot sees "the bot reached this computer" (step 2 of the checklist). The
/// mark is best-effort with a short wait: an older or stopped gxserver must not keep the bot from
/// its commands. While Grok Bot Integration is off, the bot is told so instead of getting the commands.
fn brief_command(parsed: &ParsedArgs) -> CliResult<()> {
    let mut flags = parsed.flags.clone();
    flags.0.remove("timeoutMs");
    flags.insert_text("timeout", "3000");
    match call_gxserver_rpc(ENDPOINT, &json!({ "op": "briefSeen" }), &flags) {
        Err(CliError::Rpc { message, response })
            if response.to_string().contains(crate::grokbot::TURNED_OFF) =>
        {
            Err(CliError::Other(message))
        }
        _ => {
            print!("{}", crate::grokbot::BRIEF);
            Ok(())
        }
    }
}

fn print_status(answer: &Value) {
    if answer["enabled"] == false {
        println!("Grok Bot Integration is turned off: Ghostex sends nothing to Grok Bot. Turn it on at the top of Settings > Grok Bot.");
        return;
    }
    if answer["connected"] != true {
        println!("Grok Bot isn't connected. Set it up in Settings > Grok Bot, or run `ghostex grokbot connect` in your own terminal.");
        return;
    }
    println!("Connected to Grok Bot: {}", text(answer, "url"));
    match answer["lastSentAt"].as_i64() {
        Some(_) => println!(
            "Notes sent: {}",
            answer["sent"].as_u64().unwrap_or_default()
        ),
        None => println!("No note sent yet."),
    }
    let queued = answer["queued"].as_u64().unwrap_or_default();
    if queued > 0 {
        println!("Waiting to send: {queued}");
    }
    if let Some(error) = answer["lastError"].as_str() {
        println!("Last problem: {error}");
    }
}

fn status_command(parsed: &ParsedArgs) -> CliResult<()> {
    let answer = call_gxserver_rpc(ENDPOINT, &json!({ "op": "status" }), &parsed.flags)?;
    if parsed.flags.contains("json") {
        print_json(&answer);
    } else {
        print_status(&answer);
    }
    Ok(())
}

fn simple_command(op: &str, parsed: &ParsedArgs, done: &str) -> CliResult<()> {
    let answer = call_gxserver_rpc(ENDPOINT, &json!({ "op": op }), &slow_flags(&parsed.flags))?;
    // `test` answers `sent: false` with Grok Bot's refusal (Settings shows it as the step's state);
    // the command still fails, with or without --json.
    if answer["sent"] == false {
        if parsed.flags.contains("json") {
            print_json(&answer);
            set_exit_code(1);
            return Ok(());
        }
        return Err(CliError::Other(text(&answer, "message").to_string()));
    }
    if parsed.flags.contains("json") {
        print_json(&answer);
    } else {
        println!("{done}");
    }
    Ok(())
}

/// One line typed at the terminal; `hidden` turns echo off while it is typed.
fn prompt_line(question: &str, hidden: bool) -> CliResult<String> {
    eprint!("{question}");
    let _ = std::io::stderr().flush();
    #[cfg(unix)]
    let echo_off = hidden
        && std::process::Command::new("stty")
            .arg("-echo")
            .stdin(std::process::Stdio::inherit())
            .status()
            .map(|status| status.success())
            .unwrap_or(false);
    #[cfg(not(unix))]
    let echo_off = false;
    if hidden && !echo_off {
        return Err(CliError::Other(
            "This terminal can't hide what you type. Pipe the key in instead: ghostex grokbot connect <url> --key-stdin".to_string(),
        ));
    }
    let mut line = String::new();
    let read = std::io::stdin().lock().read_line(&mut line);
    #[cfg(unix)]
    if echo_off {
        let _ = std::process::Command::new("stty")
            .arg("echo")
            .stdin(std::process::Stdio::inherit())
            .status();
        eprintln!();
    }
    read.map_err(|error| CliError::Other(format!("Couldn't read what you typed: {error}")))?;
    Ok(line.trim().to_string())
}

/// The key from the private card in the calling agent session's chat. The card's owner-only file
/// is read here and deleted at once.
fn key_from_card(
    project_id: &str,
    session_id: &str,
    url: &str,
    flags: &Flags,
) -> CliResult<String> {
    // The card names where the key will go, so the user sees an address an agent chose.
    let host = url::Url::parse(url.trim())
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .ok_or_else(|| {
            CliError::Other(
                "That isn't a web address. Give the routine's POST to address.".to_string(),
            )
        })?;
    let reason = format!(
        "Ghostex keeps it and sends your agents' notes with it to {host} only. Give it only if that is your Grok Bot routine's webhook: copy key from the \"Ghostex reports\" routine's Webhook section."
    );
    let mut rpc_flags = flags.clone();
    rpc_flags.0.remove("timeout");
    rpc_flags.0.remove("timeoutMs");
    let Some(outcome) = secret::ask_for_secret(
        project_id,
        session_id,
        "Grok Bot webhook key",
        &reason,
        Some("The routine's key".to_string()),
        10.0,
        &rpc_flags,
    )?
    else {
        return Err(CliError::Other(
            "Ghostex couldn't show the key card.".to_string(),
        ));
    };
    let Some(file) = outcome.get("file").and_then(Value::as_str) else {
        return Err(CliError::Other(format!(
            "No key: the card ended {}.",
            text(&outcome, "status")
        )));
    };
    let key = std::fs::read_to_string(file)
        .map_err(|error| CliError::Other(format!("Couldn't read the key's file: {error}")));
    let _ = std::fs::remove_file(file);
    Ok(key?.trim().to_string())
}

fn read_key(parsed: &ParsedArgs, url: &str) -> CliResult<String> {
    if parsed.flags.contains("keyStdin") {
        let mut line = String::new();
        std::io::stdin()
            .lock()
            .read_line(&mut line)
            .map_err(|error| {
                CliError::Other(format!("Couldn't read the key from stdin: {error}"))
            })?;
        return Ok(line.trim().to_string());
    }
    // A person at a terminal types it unseen; an agent (no terminal) gets the private card in its
    // session's chat, which only the user can answer.
    if std::io::stdin().is_terminal() {
        return prompt_line("Paste the routine's key (it won't show): ", true);
    }
    if let Some((project_id, session_id)) = notify::session_reference_from_environment() {
        return key_from_card(&project_id, &session_id, url, &parsed.flags);
    }
    Err(CliError::Other(
        "No key: run this in your own terminal (it asks for the key without showing it), or pipe it in with --key-stdin.".to_string(),
    ))
}

fn connect_command(parsed: &ParsedArgs) -> CliResult<()> {
    // CDXC:GrokBot 2026-11-19 WHY: the key is never a flag or positional argument, which shells
    // keep in their history and every process listing shows; only the address is.
    if parsed.flags.contains("key") {
        return Err(CliError::Other(
            "Don't put the key on the command line. Run `ghostex grokbot connect <url>` and paste it when asked, or pipe it in with --key-stdin.".to_string(),
        ));
    }
    let url = match parsed
        .flags
        .text("url")
        .or_else(|| parsed.rest.first().cloned())
    {
        Some(url) => url,
        None if std::io::stdin().is_terminal() => {
            prompt_line("Paste the routine's POST to address: ", false)?
        }
        None => {
            return Err(CliError::Other(
                "Give the routine's POST to address: ghostex grokbot connect <url>".to_string(),
            ))
        }
    };
    let key = read_key(parsed, &url)?;
    if key.is_empty() {
        return Err(CliError::Other("No key was given.".to_string()));
    }
    let answer = call_gxserver_rpc(
        ENDPOINT,
        &json!({ "op": "connect", "url": url.trim(), "key": key }),
        &slow_flags(&parsed.flags),
    )?;
    if parsed.flags.contains("json") {
        print_json(&answer);
        return Ok(());
    }
    if answer["tested"] == true {
        println!("Connected. A test note is on its way to the Ghostex bot's chat in Grok Bot.");
    } else {
        println!("{}", text(&answer, "message"));
        set_exit_code(1);
    }
    Ok(())
}

fn notify_command(parsed: &ParsedArgs) -> CliResult<()> {
    let body = parsed.rest.join(" ");
    if body.trim().is_empty() {
        return Err(CliError::Other(
            "Say what to tell the user: ghostex grokbot notify --kind ask \"<one or two lines>\""
                .to_string(),
        ));
    }
    let mut params = Map::new();
    params.insert("op".to_string(), json!("notify"));
    params.insert("text".to_string(), json!(body));
    if let Some(kind) = parsed.flags.text("kind") {
        params.insert("kind".to_string(), json!(kind));
    }
    // Who it is from, so the bot can name the agent and answer it with `ghostex agents send <ref>`.
    if let Ok(Some(caller)) = identity::agent_caller() {
        let summary = identity::summary(&caller);
        params.insert(
            "from".to_string(),
            json!({
                "project": text(&summary, "projectName"),
                "agent": text(&summary, "title"),
                "ref": text(&summary, "globalRef"),
            }),
        );
    }
    let answer = call_gxserver_rpc(ENDPOINT, &Value::Object(params), &parsed.flags)?;
    if parsed.flags.contains("json") {
        print_json(&answer);
    } else {
        println!("Sent to Grok Bot (it goes out within a minute).");
    }
    Ok(())
}
