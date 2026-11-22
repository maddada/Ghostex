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

Then the accounts Ghostex reads only for their usage (Settings > Accounts): Gemini, whose
quota is what Antigravity runs on, Cursor and Copilot, with their state (ready, needsSignIn,
unavailable, disabled, loading in --json as status) and usage per window with reset times.
--json lists them under usageAccounts.

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
            "usageAccounts": roster["usageAccounts"],
            "helpers": roster["helpers"],
        }));
        return Ok(());
    }
    let rows = roster["accounts"].as_array().cloned().unwrap_or_default();
    let usage_rows = roster["usageAccounts"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    if rows.is_empty() && usage_rows.is_empty() {
        println!("No accounts are saved. Add one in Settings > Accounts.");
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
    // CDXC:Coordinators 2026-11-22 DECISION:
    // A coordinator asked to move work off a Claude pool that is out needs to see which other
    // agents have room. Antigravity's quota is its Google account's Gemini reading, so its
    // heading says so; these accounts have no number (they never run or switch a session).
    for (provider, heading) in [
        ("gemini", "GEMINI (Antigravity quota)"),
        ("cursor", "CURSOR"),
        ("copilot", "COPILOT"),
    ] {
        let accounts: Vec<&Value> = usage_rows
            .iter()
            .filter(|row| row["provider"] == provider)
            .collect();
        if accounts.is_empty() {
            continue;
        }
        println!("{heading}");
        for row in accounts {
            print_usage_account(row);
        }
        println!();
    }
    Ok(())
}

/// A Gemini, Cursor or Copilot account (`usageAccounts`, server/src/accounts/usage_only/).
fn print_usage_account(row: &Value) {
    let text = |key: &str| row[key].as_str().unwrap_or_default();
    let label = if text("email").is_empty() {
        text("name")
    } else {
        text("email")
    };
    let mut marks = vec![match text("status") {
        "loginRequired" => "needs sign-in".to_string(),
        "loading" => "no usage reading yet".to_string(),
        other => other.to_string(),
    }];
    if !text("plan").is_empty() {
        marks.push(format!("plan {}", text("plan")));
    }
    println!("  {:<40} {}", label, marks.join(", "));
    let usage = usage_text(row);
    if !usage.is_empty() {
        let stale = if row["usageStale"] == true {
            " [last known]"
        } else {
            ""
        };
        println!("        {usage}{stale}");
    }
    let details: Vec<String> = row["details"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|detail| {
            format!(
                "{} {}",
                detail["label"].as_str().unwrap_or_default(),
                detail["value"].as_str().unwrap_or_default()
            )
        })
        .collect();
    if !details.is_empty() {
        println!("        {}", details.join(" · "));
    }
    if let Some(error) = row["usageError"].as_str().filter(|error| !error.is_empty()) {
        println!("        {error}");
    }
}

/// "5h 12% (resets Nov 22 09:00) · 7d 4% (resets Nov 27 10:00)" for a row's `usage`.
fn usage_text(row: &Value) -> String {
    row["usage"]
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
        .collect::<Vec<_>>()
        .join(" · ")
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
    let usage = usage_text(row);
    if !usage.is_empty() {
        let stale = if row["usageStale"] == true {
            " [last known]"
        } else {
            ""
        };
        println!("        {usage}{stale}");
    }
    if let Some(error) = row["usageError"].as_str().filter(|error| !error.is_empty()) {
        println!("        {error}");
    }
    if let Some(line) = resets_line(row) {
        println!("        {line}");
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

/// CDXC:AgentProviders 2026-11-21 DECISION:
/// User: "why can't u see that they have resets btw?" The JSON always carried `resetCredits` (Claude's from Anthropic's reset grants, claude_resets.rs; Codex's reset credits), but the text listing never printed it, so an account at its limit looked the same with or without a reset to free it. Each account that reports resets now says how many are left and when the soonest one expires; an account whose plan has no reset program prints nothing.
fn resets_line(row: &Value) -> Option<String> {
    if let Some(error) = row["resetCreditsError"]
        .as_str()
        .filter(|error| !error.is_empty())
    {
        return Some(format!("Limit resets: {error}"));
    }
    let count = row["resetCredits"].as_u64()?;
    if count == 0 {
        return Some("Limit resets: none left".into());
    }
    let soonest = row["resetCreditDetails"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|credit| credit["expiresAt"].as_str())
        .filter_map(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
        .min();
    let mut line = format!("Limit resets: {count} left");
    if let Some(at) = soonest {
        line.push_str(&format!(
            " (soonest expires {})",
            at.with_timezone(&chrono::Local).format("%b %-d %H:%M")
        ));
    }
    Some(line)
}
