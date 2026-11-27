use super::{
    args::Flags,
    output::print_json,
    rpc::{call_gxserver_rpc, CliError, CliResult},
};
use serde_json::{json, Value};

const HELP: &str = "Usage: ghostex accounts [--all] [--refresh] [--json] [--server <profile>]
       ghostex accounts status [--refresh] [--json]   One line per account: its room, window reset
                                                      times, available limit resets with expiries,
                                                      and whether an agent may move a thread to it

Lists every saved Claude (Claude Swap) and Codex (Codex Swap) account: its number, email,
state (ready, limitReached, needsSignIn, disabled when turned off in Settings; a ready account
with no usage reading says so, noUsageReading in --json, and may be at its limit unseen), usage
per window with reset times, available limit resets with their expiry times, which account new
sessions use (default), and its running and sleeping sessions, with a count of stopped ones (--all
lists those too). The same reading Settings > Accounts shows; --refresh reads usage again instead
of using a reading up to two minutes old.

`accounts status` prints one line per account with its 5h/7d window reset times, any available
limit resets and when each expires (local time), and whether an agent may switch a thread to it.
With --json, each account includes structured `windows` (with `resetsAt`) and `limitResets`
(`count` and `resets` with `expiresAt`). Limit resets are read-only here and are the user's to
use: no CLI command spends them.

Then the accounts Ghostex reads only for their usage (Settings > Accounts): Cursor and Copilot,
with their state (ready, needsSignIn, unavailable, disabled, loading in --json as status) and
usage per window with reset times. --json lists them under usageAccounts. Ghostex reads no
Antigravity usage: Antigravity offers other apps only ACP, which has no usage yet.

CLIProxyAPI hubs and their accounts: ghostex accounts hub --help
Move a session to another account:   ghostex agents switch-account <session-ref> --account <number|email>
Restart a stuck session's agent:     ghostex agents reload <session-ref>";

/// `ghostex accounts`: the `/api/agentAccounts` `roster` (server/src/accounts/cli.rs).
pub(crate) fn run(args: &[String]) -> CliResult<()> {
    if args.first().map(String::as_str) == Some("hub") {
        return super::accounts_hub::run(&args[1..]);
    }
    let status = args.first().map(String::as_str) == Some("status");
    let args = if status { &args[1..] } else { args };
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
    if status {
        return print_status(&roster, json_output);
    }
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
    // agents have room. These accounts have no number (they never run or switch a session).
    for (provider, heading) in [("cursor", "CURSOR"), ("copilot", "COPILOT")] {
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

/// A Cursor or Copilot account (`usageAccounts`, server/src/accounts/usage_only/).
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

/// CDXC:Coordinators 2026-11-22 DECISION:
/// User (context-bloat page, fix 4): `ghostex accounts` printed 312 lines, 307 of them the
/// sessions on each account, when a coordinator needs five: which account it may move a thread
/// to. `accounts status` prints one line per account with its room, window reset times, available
/// limit resets with each expiry, and that answer, from the roster's `autoSwitchable` (Ghostex's
/// own automatic switching rule, `SavedAccount::auto_switchable`): manual only is the user's to
/// name, turned off or signed out is never. Limit resets are read-only and are the user's to use.
/// SEE-ALSO: server/src/accounts/cli.rs (`roster`, `resolve_ready_account`),
/// server/src/ghostex_cli/coordinator/usage_room.rs, server/src/coordinators/playbook.md.
fn print_status(roster: &Value, json_output: bool) -> CliResult<()> {
    let rows = roster["accounts"].as_array().cloned().unwrap_or_default();
    let usage_rows = roster["usageAccounts"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    let now = chrono::Utc::now();
    let verdict = |row: &Value| -> (&'static str, String, String) {
        let text = |key: &str| row[key].as_str().unwrap_or_default();
        let limited = row["limitReached"]
            .as_array()
            .is_some_and(|windows| !windows.is_empty());
        let frees = row["usage"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|window| window["usedPercent"].as_f64().unwrap_or(0.0) >= 100.0)
            .filter_map(|window| window["resetsAt"].as_str())
            .filter_map(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
            .filter(|at| *at > now)
            .max()
            .map(|at| {
                at.with_timezone(&chrono::Local)
                    .format("%b %-d %H:%M")
                    .to_string()
            });
        let automatic = row
            .get("autoSwitchable")
            .and_then(Value::as_bool)
            .unwrap_or(row["eligible"] == true);
        let manual = row["registered"] == true && row["eligible"] != true;
        if row["userDisabled"] == true || text("status") == "disabled" {
            return ("off", "turned off in Settings".into(), "no".into());
        }
        if row["registered"] != true {
            return (
                "new",
                "not added in Settings".into(),
                "no (not added in Settings)".into(),
            );
        }
        if text("state") == "needsSignIn" || row["loginExpired"] == true {
            return (
                "sign-in",
                "needs signing in".into(),
                "no, the user must sign in".into(),
            );
        }
        let room = account_windows_text(row);
        if limited {
            let room = if !room.is_empty() {
                room
            } else {
                let at = frees.unwrap_or_default();
                format!(
                    "{}{}",
                    row["limitReached"]
                        .as_array()
                        .into_iter()
                        .flatten()
                        .filter_map(Value::as_str)
                        .map(|window| format!("{window} 100%"))
                        .collect::<Vec<_>>()
                        .join(" · "),
                    if at.is_empty() {
                        String::new()
                    } else {
                        format!(", frees {at}")
                    }
                )
            };
            return (
                "limit",
                room,
                if manual {
                    "no (manual only, at its limit)".into()
                } else {
                    "no, until then".into()
                },
            );
        }
        let room = if row["noUsageReading"] == true {
            "no usage reading".to_string()
        } else {
            room
        };
        if manual || !automatic {
            return (
                "ready",
                room,
                "only if the user names it (manual only)".into(),
            );
        }
        let mut answer = "yes".to_string();
        if row["noUsageReading"] == true {
            answer.push_str(", but check its room first");
        }
        if row["default"] == true {
            answer.push_str(", default");
        }
        ("ready", room, answer)
    };
    if json_output {
        let accounts: Vec<Value> = rows
            .iter()
            .map(|row| {
                let (state, room, answer) = verdict(row);
                let windows = account_windows_json(row);
                let resets = limit_resets(row, now).to_json();
                json!({
                    "provider": row["provider"],
                    "number": row["number"],
                    "email": row["email"],
                    "default": row["default"] == true,
                    "state": state,
                    "room": room,
                    "windows": windows,
                    "limitResets": resets,
                    "maySwitch": answer.starts_with("yes"),
                    "manualOnly": row["registered"] == true && row["eligible"] != true,
                    "answer": answer,
                })
            })
            .collect();
        let usage: Vec<Value> = usage_rows
            .iter()
            .map(|row| {
                let windows = account_windows_json(row);
                json!({
                    "provider": row["provider"],
                    "email": row["email"],
                    "status": row["status"],
                    "room": format_windows_json(&windows),
                    "windows": windows,
                })
            })
            .collect();
        print_json(&json!({ "ok": true, "accounts": accounts, "usageAccounts": usage }));
        return Ok(());
    }
    if rows.is_empty() && usage_rows.is_empty() {
        println!("No accounts are saved. Add one in Settings > Accounts.");
        return Ok(());
    }
    for (provider, heading) in [("claude", "Claude"), ("codex", "Codex")] {
        let accounts: Vec<&Value> = rows
            .iter()
            .filter(|row| row["provider"] == provider)
            .collect();
        if accounts.is_empty() {
            continue;
        }
        let lines: Vec<(&Value, (&'static str, String, String))> =
            accounts.iter().map(|row| (*row, verdict(row))).collect();
        let default = accounts
            .iter()
            .find(|row| row["default"] == true)
            .map(|row| format!("#{}", row["number"].as_str().unwrap_or_default()));
        let may: Vec<String> = lines
            .iter()
            .filter(|(_, (_, _, answer))| answer.starts_with("yes"))
            .map(|(row, _)| format!("#{}", row["number"].as_str().unwrap_or_default()))
            .collect();
        println!(
            "{heading}  ({}you may switch a thread to: {})",
            default
                .map(|number| format!("new threads use {number}; "))
                .unwrap_or_default(),
            if may.is_empty() {
                "none, ask the user".to_string()
            } else {
                may.join(", ")
            }
        );
        let email_width = lines
            .iter()
            .map(|(row, _)| label(row).chars().count())
            .max()
            .unwrap_or(0);
        for (row, (state, room, answer)) in &lines {
            let resets = limit_resets(row, now).text();
            let detail = match (room.is_empty(), resets) {
                (true, Some(resets)) => resets,
                (false, Some(resets)) => format!("{room} · {resets}"),
                (false, None) => room.clone(),
                (true, None) => String::new(),
            };
            if detail.is_empty() {
                println!(
                    "  #{:<2} {:<email_width$}  {:<7}  {answer}",
                    row["number"].as_str().unwrap_or_default(),
                    label(row),
                    state,
                );
            } else {
                println!(
                    "  #{:<2} {:<email_width$}  {:<7}  {detail}  {answer}",
                    row["number"].as_str().unwrap_or_default(),
                    label(row),
                    state,
                );
            }
        }
    }
    for (provider, heading) in [("cursor", "Cursor"), ("copilot", "Copilot")] {
        for row in usage_rows.iter().filter(|row| row["provider"] == provider) {
            let status = match row["status"].as_str().unwrap_or_default() {
                "loginRequired" => "needs sign-in",
                "loading" => "no reading yet",
                other => other,
            };
            let room = account_windows_text(row);
            if room.is_empty() {
                println!("{heading:<21} {}  {status}", label(row));
            } else {
                println!("{heading:<21} {}  {status}  {room}", label(row));
            }
        }
    }
    println!(
        "Sessions per account: ghostex accounts. Limit resets are the user's to use: never spend them."
    );
    Ok(())
}

fn label(row: &Value) -> &str {
    match row["email"].as_str().unwrap_or_default() {
        "" => row["name"].as_str().unwrap_or_default(),
        email => email,
    }
}

/// One available limit reset as reported by `accounts status` and `coordinator status usage`.
pub(crate) struct LimitResetItem {
    pub(crate) expires_at: Option<String>,
    pub(crate) note: Option<String>,
}

/// Summary of an account's available limit resets, read-only.
pub(crate) struct LimitResetSummary {
    pub(crate) count: u64,
    pub(crate) resets: Vec<LimitResetItem>,
    pub(crate) error: Option<String>,
}

impl LimitResetSummary {
    pub(crate) fn to_json(&self) -> Value {
        let resets: Vec<Value> = self
            .resets
            .iter()
            .map(|item| {
                let mut entry = json!({ "expiresAt": item.expires_at });
                if let Some(note) = &item.note {
                    entry["note"] = json!(note);
                }
                entry
            })
            .collect();
        let mut out = json!({
            "count": self.count,
            "resets": resets,
        });
        if let Some(error) = &self.error {
            out["error"] = json!(error);
        }
        out
    }

    pub(crate) fn text(&self) -> Option<String> {
        format_limit_resets_json(&self.to_json())
    }
}

/// Reads an account's unexpired limit resets, soonest expiry first.
pub(crate) fn limit_resets(row: &Value, now: chrono::DateTime<chrono::Utc>) -> LimitResetSummary {
    let error = row["resetCreditsError"]
        .as_str()
        .filter(|error| !error.is_empty())
        .map(str::to_string);
    if let Some(details) = row["resetCreditDetails"].as_array() {
        let mut items: Vec<(Option<chrono::DateTime<chrono::Utc>>, LimitResetItem)> = Vec::new();
        for credit in details {
            let expires_at = credit["expiresAt"]
                .as_str()
                .filter(|value| !value.is_empty())
                .map(str::to_string);
            let parsed = expires_at
                .as_deref()
                .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
                .map(|at| at.with_timezone(&chrono::Utc));
            if parsed.is_some_and(|at| at <= now) {
                continue;
            }
            let note = credit["note"]
                .as_str()
                .filter(|note| !note.is_empty())
                .map(str::to_string);
            items.push((parsed, LimitResetItem { expires_at, note }));
        }
        items.sort_by(|a, b| match (a.0, b.0) {
            (Some(a), Some(b)) => a.cmp(&b),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        });
        let resets: Vec<LimitResetItem> = items.into_iter().map(|(_, item)| item).collect();
        return LimitResetSummary {
            count: resets.len() as u64,
            resets,
            error,
        };
    }
    let count = row["resetCredits"].as_u64().unwrap_or(0);
    LimitResetSummary {
        count,
        resets: Vec::new(),
        error,
    }
}

/// Formats structured `limitResets` JSON into e.g. `"2 resets (expire Nov 24 18:00, Dec 1 09:00)"`.
pub(crate) fn format_limit_resets_json(value: &Value) -> Option<String> {
    let count = value["count"].as_u64().unwrap_or(0);
    if count == 0 {
        return None;
    }
    let noun = if count == 1 {
        "1 reset".to_string()
    } else {
        format!("{count} resets")
    };
    let verb = if count == 1 { "expires" } else { "expire" };
    let resets = value["resets"].as_array().map(Vec::as_slice).unwrap_or(&[]);
    if resets.is_empty() {
        return Some(format!("{noun} (expiry unavailable)"));
    }
    let mut any_expiry = false;
    let mut any_paused = false;
    let mut parts = Vec::with_capacity(resets.len());
    for reset in resets {
        let paused = reset["note"]
            .as_str()
            .is_some_and(|note| note.to_ascii_lowercase().contains("paused"));
        any_paused |= paused;
        let local = reset["expiresAt"]
            .as_str()
            .and_then(|at| chrono::DateTime::parse_from_rfc3339(at).ok())
            .map(|at| {
                at.with_timezone(&chrono::Local)
                    .format("%b %-d %H:%M")
                    .to_string()
            });
        match local {
            Some(at) => {
                any_expiry = true;
                if paused {
                    parts.push(format!("{at}, paused"));
                } else {
                    parts.push(at);
                }
            }
            None => {
                if paused {
                    parts.push("no expiry, paused".to_string());
                } else {
                    parts.push("no expiry".to_string());
                }
            }
        }
    }
    if !any_expiry && !any_paused {
        return Some(format!("{noun} (no expiry)"));
    }
    Some(format!("{noun} ({verb} {})", parts.join(", ")))
}

/// Account-wide usage windows (excluding model-scoped windows and Extra usage), with `resetsAt`.
pub(crate) fn account_windows_json(row: &Value) -> Vec<Value> {
    row["usage"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|window| window["model"].is_null() && window["id"] != "spend")
        .map(|window| {
            json!({
                "label": window["label"].as_str().unwrap_or_default(),
                "usedPercent": window["usedPercent"],
                "resetsAt": window["resetsAt"],
            })
        })
        .collect()
}

/// `"5h 98% (resets Nov 22 15:00) · 7d 19% (resets Nov 27 10:00)"` from structured `windows` JSON.
pub(crate) fn format_windows_json(windows: &[Value]) -> String {
    windows
        .iter()
        .map(|window| {
            let mut part = format!(
                "{} {}%",
                window["label"].as_str().unwrap_or_default(),
                window["usedPercent"].as_f64().unwrap_or(0.0).round()
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

/// Account-wide windows with each window's local reset time.
pub(crate) fn account_windows_text(row: &Value) -> String {
    format_windows_json(&account_windows_json(row))
}
