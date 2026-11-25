//! `ghostex accounts hub`: list, add, change and remove CLIProxyAPI hubs (server/src/accounts/hub/)
//! from a terminal, on the computer that uses them.
//!
//! CDXC:AgentProviders 2026-11-19 DECISION:
//! User: "im getting tired of logging in again and again on my different machines"; a hub is added on each machine without its keys passing through a chat. The keys are read only from a file (`--client-key-file <path>`, such as the owner-only file `ghostex secret request` saves) or standard input (`-`, or `--client-key-stdin`, for `ssh hub 'printf %s "$KEY"' | ghostex accounts hub add …`); a key is never taken as a command-line value, which shells keep in their history and other processes can read. Nothing this verb prints holds a key.

use std::io::{IsTerminal, Read};

use serde_json::{json, Map, Value};

use super::{
    args::Flags,
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
};

pub(super) const HELP: &str = "Usage: ghostex accounts hub [list] [--json]
       ghostex accounts hub add --address <url> [--name <text>]
                                (--client-key-file <path|-> | --client-key-stdin)
                                [--management-key-file <path|-> | --management-key-stdin]
                                [--json]
       ghostex accounts hub update <hub> [--name <text>] [--address <url>]
                                [--client-key-file <path|-> | --client-key-stdin]
                                [--management-key-file <path|-> | --management-key-stdin |
                                 --clear-management-key] [--json]
       ghostex accounts hub remove <hub> [--json]

CLIProxyAPI hubs you run yourself (the CLIProxyAPI built-in extension; turn it on in
Settings > Extensions > Built-in on each computer first). <hub> is a hub's name, host
or id, as list shows them.

The client key is one of the hub's api-keys; the management key is optional and lets
Ghostex list the hub's accounts and their usage. Keys are read only from a file or
from standard input (-), never from the command line, and never printed:
  ssh me@hub 'cat ~/hub-client-key' \\
    | ghostex accounts hub add --name Hub --address http://100.64.0.1:8317 --client-key-stdin
Only one key can come from standard input per command. A new --address needs the client key
again (and the management key, when the hub has one): saved keys never go to a new address.

Run hub add, update, or remove in Settings > Accounts or from a terminal outside Ghostex
(agent sessions and Ghostex terminal panes can't add, change or remove hubs).";

#[derive(Default)]
struct Options {
    json: bool,
    positional: Vec<String>,
    name: Option<String>,
    address: Option<String>,
    client_key: Option<String>,
    management_key: Option<String>,
    clear_management_key: bool,
    flags: Flags,
}

/// `text` to repeat in a message, or "that" when it is long enough to be a key typed by mistake.
fn shown(text: &str) -> &str {
    if text.chars().count() >= 16 && !text.contains(char::is_whitespace) {
        "that"
    } else {
        text
    }
}

/// Where a key comes from: a file, or standard input (`-`).
fn read_key(source: &str, what: &str) -> CliResult<String> {
    let text = if source == "-" {
        let mut stdin = std::io::stdin();
        if stdin.is_terminal() {
            return Err(CliError::Other(format!(
                "The {what} is read from standard input, and nothing was piped in. Pipe it in, or use a file."
            )));
        }
        let mut text = String::new();
        stdin.read_to_string(&mut text).map_err(|error| {
            CliError::Other(format!(
                "Could not read the {what} from standard input: {error}"
            ))
        })?;
        text
    } else {
        std::fs::read_to_string(source).map_err(|error| {
            // The path isn't repeated: a key typed where its file belongs would show here.
            CliError::Other(format!("Could not read the {what} file: {error}"))
        })?
    };
    let key = text.trim_start_matches('\u{feff}').trim().to_string();
    if key.is_empty() {
        return Err(CliError::Other(format!("The {what} is empty.")));
    }
    if key.contains(char::is_whitespace) {
        return Err(CliError::Other(format!(
            "The {what} has spaces or more than one line. Give only the key."
        )));
    }
    Ok(key)
}

fn parse(args: &[String]) -> CliResult<Options> {
    let mut options = Options::default();
    let mut stdin_taken = false;
    let mut rest = args.iter();
    let value = |flag: &str, rest: &mut std::slice::Iter<'_, String>| {
        rest.next()
            .filter(|value| *value == "-" || !value.starts_with("--"))
            .cloned()
            .ok_or_else(|| CliError::Other(format!("{flag} requires a value.")))
    };
    let mut key_source = |source: String, what: &str| -> CliResult<String> {
        if source == "-" {
            if stdin_taken {
                return Err(CliError::Other(
                    "Only one key can come from standard input. Give the other one as a file."
                        .to_string(),
                ));
            }
            stdin_taken = true;
        }
        read_key(&source, what)
    };
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--json" => options.json = true,
            "--name" | "--label" => options.name = Some(value(arg, &mut rest)?),
            "--address" | "--url" => options.address = Some(value(arg, &mut rest)?),
            "--client-key-file" => {
                let source = value(arg, &mut rest)?;
                options.client_key = Some(key_source(source, "client key")?);
            }
            "--client-key-stdin" => {
                options.client_key = Some(key_source("-".into(), "client key")?);
            }
            "--management-key-file" => {
                let source = value(arg, &mut rest)?;
                options.management_key = Some(key_source(source, "management key")?);
            }
            "--management-key-stdin" => {
                options.management_key = Some(key_source("-".into(), "management key")?);
            }
            "--clear-management-key" => options.clear_management_key = true,
            "--client-key" | "--management-key" | "--key" => {
                let flag = if arg == "--management-key" {
                    "--management-key"
                } else {
                    "--client-key"
                };
                return Err(CliError::Other(format!(
                    "{arg} isn't taken: a key on the command line stays in your shell's history. Use {flag}-file <path> or {flag}-stdin."
                )));
            }
            // Accounts are not among what another computer's agents may use
            // (connected_computers/scope.rs), so a hub is added on the computer itself.
            "--server" => {
                let target = value(arg, &mut rest)?;
                options.flags.insert_text("server", &target);
            }
            "--computer" => {
                return Err(CliError::Other(
                    "Hubs are added on each computer itself: use Settings > Accounts or run this command from a terminal outside Ghostex on that computer.".into(),
                ))
            }
            // `--flag=value`: never repeated, since the value may be a key.
            other if other.starts_with("--") => {
                let name = other.split_once('=').map_or(other, |(name, _)| name);
                return Err(CliError::Other(if name != other {
                    format!(
                        "Give {name} its value after a space, and a key only from a file or standard input. See ghostex accounts hub --help."
                    )
                } else {
                    format!("Unknown argument: {name}. See ghostex accounts hub --help.")
                }));
            }
            other => options.positional.push(other.to_string()),
        }
    }
    // Checking a hub's keys waits on the hub (up to 20 seconds each).
    options.flags.insert_text("timeout", "90000");
    Ok(options)
}

fn call(params: Value, flags: &Flags) -> CliResult<Value> {
    call_gxserver_rpc("/api/agentAccounts", &params, flags)
}

/// The hubs of a `list` answer; an answer without `cliproxyHubs` means the extension is off.
fn hubs(answer: &Value) -> CliResult<Vec<Value>> {
    answer
        .get("cliproxyHubs")
        .and_then(Value::as_array)
        .cloned()
        .ok_or_else(|| {
            CliError::Other(
                "CLIProxyAPI is turned off on that computer. Turn it on in Settings > Extensions > Built-in."
                    .to_string(),
            )
        })
}

/// The hub `wanted` names: its id, name or host (any case).
fn find_hub<'a>(hubs: &'a [Value], wanted: &str) -> CliResult<&'a Value> {
    let text = |hub: &Value, key: &str| hub[key].as_str().unwrap_or_default().to_string();
    hubs.iter()
        .find(|hub| text(hub, "id") == wanted)
        .or_else(|| {
            hubs.iter().find(|hub| {
                text(hub, "label").eq_ignore_ascii_case(wanted)
                    || text(hub, "host").eq_ignore_ascii_case(wanted)
            })
        })
        .ok_or_else(|| {
            let names: Vec<String> = hubs.iter().map(|hub| text(hub, "label")).collect();
            CliError::Other(if names.is_empty() {
                "No hub is added. Add one with ghostex accounts hub add.".to_string()
            } else {
                format!(
                    "No hub is called {}. Hubs: {}.",
                    shown(wanted),
                    names.join(", ")
                )
            })
        })
}

fn print_hubs(hubs: &[Value]) {
    if hubs.is_empty() {
        println!("No hub is added. Add one with ghostex accounts hub add (see --help).");
    }
    for hub in hubs {
        let text = |key: &str| hub[key].as_str().unwrap_or_default();
        let status = match text("status") {
            "noManagementKey" => "no management key: accounts not listed".to_string(),
            "unavailable" => format!("can't be read: {}", text("error")),
            other => other.to_string(),
        };
        println!(
            "{}  {}  ({})  {status}",
            text("label"),
            text("url"),
            text("id")
        );
        for account in hub["accounts"].as_array().into_iter().flatten() {
            let text = |key: &str| account[key].as_str().unwrap_or_default();
            let usage: Vec<String> = account["usage"]
                .as_array()
                .into_iter()
                .flatten()
                .map(|window| {
                    format!(
                        "{} {}%",
                        window["label"].as_str().unwrap_or_default(),
                        window["usedPercent"].as_f64().unwrap_or(0.0).round()
                    )
                })
                .collect();
            let mut line = format!(
                "  {:<7} {:<36} {}",
                text("provider"),
                text("name"),
                text("status")
            );
            if !usage.is_empty() {
                line.push_str(&format!("  {}", usage.join(" · ")));
            }
            if let Some(error) = account["usageError"].as_str() {
                line.push_str(&format!("  ({error})"));
            }
            println!("{line}");
        }
    }
}

/// `ghostex accounts hub …`.
pub(super) fn run(args: &[String]) -> CliResult<()> {
    if args
        .iter()
        .any(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
    {
        println!("{HELP}");
        return Ok(());
    }
    let (command, rest) = match args.first().map(String::as_str) {
        Some("add" | "update" | "remove" | "list") => (args[0].as_str(), &args[1..]),
        _ => ("list", args),
    };
    let options = parse(rest)?;
    let list = || call(json!({ "operation": "list" }), &options.flags);
    let answer = match command {
        "list" => {
            if !options.positional.is_empty() {
                return Err(CliError::Other(format!(
                    "Unknown argument: {}. See ghostex accounts hub --help.",
                    shown(&options.positional[0])
                )));
            }
            list()?
        }
        "add" => {
            let address = options.address.clone().ok_or_else(|| {
                CliError::Other("hub add needs --address (like http://100.64.0.1:8317).".into())
            })?;
            let client_key = options.client_key.clone().ok_or_else(|| {
                CliError::Other(
                    "hub add needs the client key: --client-key-file <path|-> or --client-key-stdin."
                        .into(),
                )
            })?;
            let mut params = Map::new();
            params.insert("operation".into(), json!("hubAdd"));
            params.insert("url".into(), json!(address));
            params.insert("clientKey".into(), json!(client_key));
            if let Some(key) = &options.management_key {
                params.insert("managementKey".into(), json!(key));
            }
            if let Some(name) = &options.name {
                params.insert("label".into(), json!(name));
            }
            call(Value::Object(params), &options.flags)?
        }
        _ => {
            let [wanted] = options.positional.as_slice() else {
                return Err(CliError::Other(format!(
                    "hub {command} needs the hub's name, host or id. See ghostex accounts hub list."
                )));
            };
            let current = hubs(&list()?)?;
            let hub_id = find_hub(&current, wanted)?["id"]
                .as_str()
                .unwrap_or_default()
                .to_string();
            let mut params = Map::new();
            params.insert("hubId".into(), json!(hub_id));
            if command == "remove" {
                params.insert("operation".into(), json!("hubRemove"));
            } else {
                params.insert("operation".into(), json!("hubUpdate"));
                if options.management_key.is_some() && options.clear_management_key {
                    return Err(CliError::Other(
                        "Give a new management key or --clear-management-key, not both.".into(),
                    ));
                }
                for (key, value) in [
                    ("label", &options.name),
                    ("url", &options.address),
                    ("clientKey", &options.client_key),
                    ("managementKey", &options.management_key),
                ] {
                    if let Some(value) = value {
                        params.insert(key.into(), json!(value));
                    }
                }
                if options.clear_management_key {
                    params.insert("clearManagementKey".into(), json!(true));
                }
                if params.len() == 2 {
                    return Err(CliError::Other(
                        "Nothing to change. See ghostex accounts hub --help.".into(),
                    ));
                }
            }
            call(Value::Object(params), &options.flags)?
        }
    };
    let hubs = hubs(&answer)?;
    if options.json {
        print_json(&json!({ "ok": true, "hubs": hubs }));
    } else {
        match command {
            "add" => println!("Hub added."),
            "update" => println!("Hub changed."),
            "remove" => {
                println!("Hub removed. New sessions that were set to start on it go back to Auto.")
            }
            _ => {}
        }
        print_hubs(&hubs);
    }
    Ok(())
}
