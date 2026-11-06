//! `ghostex gbots`: Ghostex's bots (server/src/gbots/). The work happens in gxserver
//! (`/api/gbots`); `attach` is the small client a GBots conversation's own terminal runs. The chat
//! is the way to talk to a bot; this terminal shows the same conversation as plain text.

use std::io::{BufRead, Write};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{json, Value};

use crate::ghostex_cli::{
    args::{parse_args, Flags},
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
};

const HELP: &str = "GBots: bots that run on your own Claude Code or Codex, shown in the Bots space.

USAGE
  ghostex gbots status [--json]
  ghostex gbots list [--json]
  ghostex gbots create <name> [--instructions <text>] [--agent claude|codex] [--json]

  status says whether GBots is on, which computer runs it and whether it runs.
  list shows each bot with its status and conversations. create makes a bot
  that runs on Claude Code (the default) or Codex; it shows in the Bots space.
  Every bot can run `ghostex` itself, so it can start, message and read
  sessions on this computer and every connected one.

  Turn GBots on in Settings > Extensions. Add --computer <name> to run a
  command on another computer.
";

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value[key].as_str().unwrap_or_default()
}

pub(crate) fn run(args: &[String]) -> CliResult<()> {
    // The wrapper the runtime runs as its `claude` / `codex`: every argument after `--` is the
    // CLI's own, so none of them is read as a flag here.
    if args.first().map(String::as_str) == Some("agent") {
        return agent(&args[1..]);
    }
    let parsed = parse_args(args);
    let flags = parsed.flags.clone();
    if flags.contains("help") || flags.contains("h") || parsed.rest.is_empty() {
        print!("{HELP}");
        return Ok(());
    }
    let json_output = flags.contains("json");
    let request = |params: Value| call_gxserver_rpc("/api/gbots", &params, &flags);
    match parsed.rest[0].as_str() {
        "attach" => attach(&flags),
        "status" => {
            let status = request(json!({ "action": "status" }))?;
            if json_output {
                print_json(&status);
            } else if status["enabled"] != json!(true) {
                println!("GBots is off. Turn it on in Settings > Extensions.");
            } else if status["runsHere"] != json!(true) {
                println!(
                    "GBots runs on another computer ({}).",
                    text(&status, "computer")
                );
            } else {
                println!("GBots: {}", text(&status, "status"));
                if let Some(error) = status["error"].as_str() {
                    println!("{error}");
                }
            }
            Ok(())
        }
        "list" => {
            let answer = request(json!({ "action": "list" }))?;
            if json_output {
                print_json(&answer);
                return Ok(());
            }
            let bots = answer["bots"].as_array().cloned().unwrap_or_default();
            if bots.is_empty() {
                println!("No bots yet. Make one with: ghostex gbots create <name>");
            }
            for bot in bots {
                println!(
                    "{}\t{}\t{}",
                    text(&bot, "name"),
                    text(&bot, "status"),
                    text(&bot, "botId")
                );
            }
            Ok(())
        }
        "create" => {
            let name = parsed.rest[1..].join(" ");
            if name.trim().is_empty() {
                return Err(CliError::Other(format!(
                    "gbots create needs a name.\n\n{HELP}"
                )));
            }
            let created = request(json!({
                "action": "create",
                "name": name.trim(),
                "instructions": flags.text("instructions"),
                "agent": flags.text("agent").unwrap_or_else(|| "claude".to_string()),
            }))?;
            if json_output {
                print_json(&created);
            } else {
                println!("Made {} (bot {}).", name.trim(), text(&created, "botId"));
            }
            Ok(())
        }
        other => Err(CliError::Other(format!(
            "unknown gbots command `{other}`.\n\n{HELP}"
        ))),
    }
}

fn own_session() -> CliResult<(String, String)> {
    std::env::var("GHOSTEX_GLOBAL_SESSION_REF")
        .ok()
        .map(|global| crate::agent_hooks::probing::parse_global_session_ref(global.trim()))
        .and_then(|(project, session)| Some((project?, session?)))
        .ok_or_else(|| {
            CliError::Other("gbots attach runs inside a GBots conversation's own terminal.".into())
        })
}

/// Reads typed lines on a thread of its own, so the screen can refresh while the user types.
fn typed_lines() -> mpsc::Receiver<Option<String>> {
    let (sender, lines) = mpsc::channel();
    std::thread::spawn(move || loop {
        let mut line = String::new();
        let typed = match std::io::stdin().lock().read_line(&mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(line.trim().to_string()),
        };
        let closed = typed.is_none();
        if sender.send(typed).is_err() || closed {
            break;
        }
    });
    lines
}

fn attach(flags: &Flags) -> CliResult<()> {
    let (project_id, session_id) = own_session()?;
    let call = |params: Value| -> CliResult<Value> {
        let mut params = params;
        params["projectId"] = json!(project_id);
        params["sessionId"] = json!(session_id);
        call_gxserver_rpc("/api/gbots", &params, flags)
    };
    let thread_id = loop {
        match call(json!({ "action": "attach" })) {
            Ok(answer) => break text(&answer, "threadId").to_string(),
            Err(error) => {
                println!("{error} Trying again…");
                std::thread::sleep(Duration::from_secs(5));
            }
        }
    };
    let lines = typed_lines();
    let mut shown: Option<String> = None;
    let mut notice: Option<String> = None;
    loop {
        let conversation = call(json!({ "action": "conversation", "threadId": thread_id }));
        let screen = match &conversation {
            Ok(answer) => conversation_lines(answer),
            Err(error) => vec![format!("GBots is not answering ({error}). Trying again…")],
        };
        let key = screen.join("\n");
        if shown.as_deref() != Some(key.as_str()) {
            print!("\x1b[2J\x1b[H");
            for line in screen.iter().chain(notice.iter()) {
                println!("{line}");
            }
            print!("> ");
            let _ = std::io::stdout().flush();
            shown = Some(key);
        }
        match lines.recv_timeout(Duration::from_secs(3)) {
            Ok(Some(line)) if !line.is_empty() => {
                notice = match call(json!({ "action": "send", "text": line })) {
                    Ok(_) => None,
                    Err(error) => Some(error.to_string()),
                };
                shown = None;
            }
            Ok(Some(_)) => shown = None,
            Ok(None) | Err(mpsc::RecvTimeoutError::Disconnected) => {
                std::thread::sleep(Duration::from_secs(30))
            }
            Err(mpsc::RecvTimeoutError::Timeout) => {}
        }
    }
}

fn conversation_lines(answer: &Value) -> Vec<String> {
    let mut lines = Vec::new();
    for message in answer["messages"].as_array().into_iter().flatten() {
        lines.push(match text(message, "role") {
            "user" => "You".to_string(),
            "assistant" => "Bot".to_string(),
            _ => "Note".to_string(),
        });
        for line in text(message, "text").lines() {
            lines.push(format!("  {line}"));
        }
        lines.push(String::new());
    }
    if answer["working"] == json!(true) {
        lines.push("Working…".to_string());
    }
    lines.push("Type a message and press Enter, or use the chat.".to_string());
    lines
}

/// `gbots agent <claude|codex> -- <args>`: runs the CLI for a bot's turn on the account gxserver
/// picks (server/src/gbots/accounts.rs), through Claude Swap or Codex Swap; on the CLI's own login
/// when no saved account is usable or gxserver cannot be asked.
fn agent(args: &[String]) -> CliResult<()> {
    let engine = args
        .first()
        .filter(|engine| matches!(engine.as_str(), "claude" | "codex"))
        .ok_or_else(|| CliError::Other("gbots agent needs claude or codex.".into()))?
        .clone();
    let rest: Vec<String> = match args.get(1).map(String::as_str) {
        Some("--") => args[2..].to_vec(),
        _ => args[1..].to_vec(),
    };
    let prefix: Vec<String> = call_gxserver_rpc(
        "/api/gbots",
        &json!({ "action": "agentArgv", "agent": engine }),
        &Flags::default(),
    )
    .ok()
    .and_then(|answer| {
        answer["argv"].as_array().map(|argv| {
            argv.iter()
                .filter_map(|part| part.as_str().map(str::to_string))
                .collect()
        })
    })
    .unwrap_or_default();
    let mut argv = if prefix.is_empty() {
        vec![engine]
    } else {
        prefix
    };
    argv.extend(rest);
    let mut command = std::process::Command::new(&argv[0]);
    command.args(&argv[1..]);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt as _;
        let error = command.exec();
        Err(CliError::Other(format!(
            "Could not run {}: {error}",
            argv[0]
        )))
    }
    #[cfg(not(unix))]
    {
        let status = command
            .status()
            .map_err(|error| CliError::Other(format!("Could not run {}: {error}", argv[0])))?;
        std::process::exit(status.code().unwrap_or(1));
    }
}
