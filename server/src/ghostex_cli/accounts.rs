use super::{
    args::Flags,
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
};
use serde_json::{json, Value};

const HELP: &str = "Usage: ghostex accounts [--all] [--refresh] [--json] [--server <profile>]

Lists every saved Claude (Claude Swap) and Codex (Codex Swap) account: its number, email,
state (ready, limitReached, needsSignIn, disabled when turned off in Settings; a ready account
with no usage reading says so, noUsageReading in --json, and may be at its limit unseen), usage
per window with reset times, which account new sessions use (default), and its running and
sleeping sessions, with a count of stopped ones (--all lists those too). The same reading Settings > Accounts shows; --refresh reads usage
again instead of using a reading up to two minutes old.

CLIProxyAPI hubs and their accounts: ghostex accounts hub --help
Move a session to another account:   ghostex agents switch-account <session-ref> --account <number|email>
Restart a stuck session's agent:     ghostex agents reload <session-ref>";

/// `ghostex accounts`: the `/api/agentAccounts` `roster` (server/src/accounts/cli.rs).
pub(crate) fn run(args: &[String]) -> CliResult<()> {
    if args.first().map(String::as_str) == Some("hub") {
        return super::accounts_hub::run(&args[1..]);
    }
    let mut json_output = false;
    let mut refresh = false;
    let mut all_sessions = false;
    let mut flags = Flags::default();
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "-h" | "--help" => {
                println!("{HELP}");
                return Ok(());
            }
            "--json" => json_output = true,
            "--refresh" => refresh = true,
            "--all" => all_sessions = true,
            "--server" => {
                let profile = rest
                    .next()
                    .filter(|value| !value.starts_with("--"))
                    .ok_or_else(|| CliError::Other("--server requires a value.".into()))?;
                flags.insert_text("server", profile);
            }
            other => {
                return Err(CliError::Other(format!(
                    "Unknown argument: {other}. See ghostex accounts --help."
                )))
            }
        }
    }
    // Reading usage asks every account's provider; give it longer than the usual 15 seconds.
    flags.insert_text("timeout", "90000");
    let roster = call_gxserver_rpc(
        "/api/agentAccounts",
        &json!({ "operation": "roster", "refresh": refresh, "allSessions": all_sessions }),
        &flags,
    )?;
    if json_output {
        print_json(&json!({
            "ok": true,
            "accounts": roster["accounts"],
            "helpers": roster["helpers"],
        }));
        return Ok(());
    }
    let rows = roster["accounts"].as_array().cloned().unwrap_or_default();
    if rows.is_empty() {
        println!("No Claude or Codex accounts are saved. Add one in Settings > Accounts.");
    }
    for provider in ["claude", "codex"] {
        let accounts: Vec<&Value> = rows
            .iter()
            .filter(|row| row["provider"] == provider)
            .collect();
        if accounts.is_empty() {
            continue;
        }
        println!(
            "{}",
            if provider == "claude" {
                "CLAUDE"
            } else {
                "CODEX"
            }
        );
        for row in accounts {
            print_account(row, all_sessions);
        }
        println!();
    }
    Ok(())
}

fn print_account(row: &Value, all_sessions: bool) {
    let text = |key: &str| row[key].as_str().unwrap_or_default();
    let label = if text("email").is_empty() {
        text("name")
    } else {
        text("email")
    };
    let mut marks = vec![match text("state") {
        "ready" if row["noUsageReading"] == true => "ready, no usage reading".to_string(),
        "ready" => "ready".to_string(),
        "limitReached" => "limit reached".to_string(),
        "needsSignIn" => "needs sign-in".to_string(),
        other => other.to_string(),
    }];
    if row["default"] == true {
        marks.push("default".into());
    }
    if row["registered"] != true {
        marks.push("not added in Settings".into());
    } else if row["eligible"] != true {
        marks.push("manual only".into());
    }
    println!(
        "  #{:<3} {:<36} {}",
        text("number"),
        label,
        marks.join(", ")
    );
    let usage: Vec<String> = row["usage"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|window| {
            let percent = window["usedPercent"].as_f64().unwrap_or(0.0).round();
            let mut part = format!(
                "{} {percent}%",
                window["label"].as_str().unwrap_or_default()
            );
            if let Some(at) = window["resetsAt"]
                .as_str()
                .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
            {
                part.push_str(&format!(
                    " (resets {})",
                    at.with_timezone(&chrono::Local).format("%b %-d %H:%M")
                ));
            }
            part
        })
        .collect();
    if !usage.is_empty() {
        let stale = if row["usageStale"] == true {
            " [last known]"
        } else {
            ""
        };
        println!("        {}{stale}", usage.join(" · "));
    }
    if let Some(error) = row["usageError"].as_str().filter(|error| !error.is_empty()) {
        println!("        {error}");
    }
    let sessions = row["sessions"].as_array().cloned().unwrap_or_default();
    if !sessions.is_empty() {
        println!("        Sessions ({}):", sessions.len());
        for session in sessions {
            println!(
                "          {}  {}  {}",
                session["globalRef"].as_str().unwrap_or_default(),
                session["lifecycleState"].as_str().unwrap_or_default(),
                session["title"]
                    .as_str()
                    .unwrap_or_default()
                    .replace(['\n', '\r', '\t'], " ")
            );
        }
    }
    let stopped = row["stoppedSessionCount"].as_u64().unwrap_or(0);
    if stopped > 0 && !all_sessions {
        println!("        + {stopped} stopped (ghostex accounts --all lists them)");
    }
}
