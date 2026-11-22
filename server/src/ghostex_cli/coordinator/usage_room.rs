//! `ghostex coordinator status`'s "Usage room": how much room each agent has left across its
//! accounts, from the `/api/agentAccounts` `roster` as Ghostex keeps it (`cachedOnly`), and a
//! warning for each agent the coordinator or its live threads run on that is close to out.
//!
//! CDXC:Coordinators 2026-11-22 DECISION:
//! User: "coordinators must never burn all the user's usage and get stuck. Before the agent their
//! threads run on runs out, a coordinator raises a held Needs you that names its current model
//! config and offers agents that still have room." Every usable Claude account had hit its weekly
//! limit and the coordinator and its threads stopped while Antigravity sat at 0% used. Status is
//! where a coordinator looks anyway, so it carries one line per agent and a LOW or OUT line for an
//! agent in use when every usable account is at `LOW_PERCENT` or more of an account-wide window,
//! only one usable account is left of several, or none is. The playbook's "Usage room" says what
//! the coordinator does then.
//! SEE-ALSO: server/src/accounts/endpoint.rs (`cachedOnly` roster), server/src/ghostex_cli/accounts.rs,
//! server/src/coordinators/playbook.md ("Usage room").

use std::collections::BTreeMap;

use chrono::{DateTime, Local, Utc};
use serde_json::{json, Value};

use crate::ghostex_cli::{agents, args::Flags, rpc::call_gxserver_rpc};

/// An account at this share of its 5h or 7d window (or a usage-only account's month) or more is
/// close to out: one long thread turn can take a Claude 5h window from 85% to its limit.
const LOW_PERCENT: f64 = 85.0;

/// The agents whose accounts report usage, in the order status lists them. The key is the
/// roster's `provider`; Antigravity's quota is its Google account's Gemini reading.
const AGENTS: [(&str, &str); 5] = [
    ("claude", "Claude"),
    ("codex", "Codex"),
    ("gemini", "Antigravity (Gemini)"),
    ("cursor", "Cursor"),
    ("copilot", "Copilot"),
];

/// The roster provider an agent id runs on: a built-in id, or a custom launcher named after its
/// family (`custom-claude-70-…`).
fn provider_of(agent_id: &str) -> Option<&'static str> {
    let id = agent_id.strip_prefix("custom-").unwrap_or(agent_id);
    [
        ("claude", "claude"),
        ("codex", "codex"),
        ("antigravity", "gemini"),
        ("agy", "gemini"),
        ("cursor", "cursor"),
        ("copilot", "copilot"),
    ]
    .into_iter()
    .find(|(family, _)| id == *family || id.starts_with(&format!("{family}-")))
    .map(|(_, provider)| provider)
}

/// The room summary for `view` (a `readCoordinator` answer), or `{"error": …}` when the accounts
/// can't be read. Never asks a provider for usage: it reads what Ghostex already keeps.
pub(super) fn read(view: &Value, flags: &Flags) -> Value {
    let roster = match call_gxserver_rpc(
        "/api/agentAccounts",
        &json!({ "operation": "roster", "cachedOnly": true }),
        flags,
    ) {
        Ok(roster) => roster,
        Err(error) => return json!({ "error": error.to_string() }),
    };
    summarize(view, &roster, Utc::now())
}

/// One account as the summary weighs it.
struct Account {
    name: String,
    usable: bool,
    /// The fullest account-wide window's percent, `None` with no reading.
    used: Option<f64>,
    windows: String,
    /// When a limited account's last full window resets.
    frees_at: Option<DateTime<Utc>>,
    updated_at: Option<DateTime<Utc>>,
}

fn parse_time(text: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|at| at.with_timezone(&Utc))
}

fn account(row: &Value, now: DateTime<Utc>) -> Account {
    let usage_only = row["usageOnly"] == true;
    let windows: Vec<&Value> = row["usage"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|window| window["model"].is_null() && window["id"] != "spend")
        .collect();
    let used = windows
        .iter()
        .filter_map(|window| window["usedPercent"].as_f64())
        .reduce(f64::max);
    let full: Vec<DateTime<Utc>> = windows
        .iter()
        .filter(|window| window["usedPercent"].as_f64().unwrap_or(0.0) >= 100.0)
        .filter_map(|window| window["resetsAt"].as_str().and_then(parse_time))
        .filter(|at| *at > now)
        .collect();
    let usable = if usage_only {
        // A failed read keeps its last numbers (`usageStale`); they still say how much is left.
        let read = row["status"] == "ready" || (row["status"] == "unavailable" && used.is_some());
        read && used.is_none_or(|used| used < 100.0)
    } else {
        row["state"] == "ready" && row["userDisabled"] != true
    };
    let name = if usage_only {
        let email = agents::text(row, "email");
        if email.is_empty() {
            agents::text(row, "name").to_string()
        } else {
            email.to_string()
        }
    } else {
        format!("#{}", agents::text(row, "number"))
    };
    Account {
        name,
        usable,
        used,
        windows: windows
            .iter()
            .map(|window| {
                format!(
                    "{} {}%",
                    agents::text(window, "label"),
                    window["usedPercent"].as_f64().unwrap_or(0.0).round()
                )
            })
            .collect::<Vec<_>>()
            .join(", "),
        frees_at: full.into_iter().max(),
        updated_at: parse_time(agents::text(row, "usageUpdatedAt")),
    }
}

pub(super) fn summarize(view: &Value, roster: &Value, now: DateTime<Utc>) -> Value {
    // Who runs on what: the coordinator itself and its active threads (a sleeping thread spends
    // nothing until it wakes; a done, closed or other computer's thread no longer spends this
    // computer's accounts).
    let mut users: BTreeMap<&'static str, (bool, u64)> = BTreeMap::new();
    if let Some(provider) = provider_of(agents::text(&view["coordinator"], "agentId")) {
        users.entry(provider).or_default().0 = true;
    }
    for thread in view["threads"].as_array().into_iter().flatten() {
        if matches!(
            agents::text(thread, "state"),
            "sleeping" | "done" | "closed" | "unreachable"
        ) {
            continue;
        }
        if let Some(provider) = provider_of(agents::text(thread, "agentId")) {
            users.entry(provider).or_default().1 += 1;
        }
    }
    let rows: Vec<&Value> = roster["accounts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|row| row["registered"] == true)
        .chain(roster["usageAccounts"].as_array().into_iter().flatten())
        .filter(|row| row["userDisabled"] != true)
        .collect();
    let mut oldest: Option<DateTime<Utc>> = None;
    let mut out = Vec::new();
    for (provider, label) in AGENTS {
        let accounts: Vec<Account> = rows
            .iter()
            .filter(|row| row["provider"] == provider)
            .map(|row| account(row, now))
            .collect();
        let (coordinator, threads) = users.get(provider).copied().unwrap_or_default();
        if accounts.is_empty() && !coordinator && threads == 0 {
            continue;
        }
        let usable: Vec<&Account> = accounts.iter().filter(|a| a.usable).collect();
        for account in &usable {
            if let Some(at) = account.updated_at {
                oldest = Some(oldest.map_or(at, |oldest| oldest.min(at)));
            }
        }
        let best = usable
            .iter()
            .filter(|account| account.used.is_some())
            .min_by(|a, b| {
                a.used
                    .partial_cmp(&b.used)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
        let unread = usable
            .iter()
            .filter(|account| account.used.is_none())
            .count();
        let frees_at = accounts
            .iter()
            .filter(|account| !account.usable)
            .filter_map(|account| account.frees_at)
            .min();
        let in_use = coordinator || threads > 0;
        let flag = if accounts.is_empty() {
            None
        } else if usable.is_empty() {
            Some("out")
        } else if unread == 0 && usable.iter().all(|a| a.used.unwrap_or(0.0) >= LOW_PERCENT) {
            Some("low")
        } else if usable.len() == 1 && accounts.len() > 1 {
            Some("lastAccount")
        } else {
            None
        };
        out.push(json!({
            "agent": provider,
            "label": label,
            "accounts": accounts.len(),
            "usable": usable.len(),
            "noReading": unread,
            "best": best.map(|best| json!({
                "account": best.name,
                "usedPercent": best.used,
                "windows": best.windows,
            })),
            "freesAt": frees_at.map(|at| at.to_rfc3339()),
            "usedBy": { "coordinator": coordinator, "threads": threads },
            "inUse": in_use,
            "flag": flag,
        }));
    }
    json!({
        "agents": out,
        "lowPercent": LOW_PERCENT,
        "oldestReadingAt": oldest.map(|at| at.to_rfc3339()),
    })
}

fn local(at: &str) -> Option<String> {
    parse_time(at).map(|at| at.with_timezone(&Local).format("%b %-d %H:%M").to_string())
}

fn users_text(agent: &Value) -> String {
    let coordinator = agent["usedBy"]["coordinator"] == true;
    let threads = agent["usedBy"]["threads"].as_u64().unwrap_or(0);
    let threads_text = match threads {
        0 => String::new(),
        1 => "1 active thread".to_string(),
        n => format!("{n} active threads"),
    };
    match (coordinator, threads) {
        (true, 0) => "you".into(),
        (true, _) => format!("you and {threads_text}"),
        _ => threads_text,
    }
}

/// The summary as `coordinator status` prints it.
pub(super) fn print(room: &Value) {
    if let Some(error) = room["error"].as_str() {
        println!("Usage room: not read ({error}). ghostex accounts lists every account.");
        return;
    }
    let rows = room["agents"].as_array().cloned().unwrap_or_default();
    if rows.is_empty() {
        println!("Usage room: no accounts report usage (Settings > Accounts adds them).");
        return;
    }
    let age = room["oldestReadingAt"]
        .as_str()
        .and_then(parse_time)
        .map(|at| (Utc::now() - at).num_minutes().max(0))
        .map(|minutes| match minutes {
            0 => " (read under a minute ago".to_string(),
            1..=119 => format!(" (oldest reading {minutes}m ago"),
            _ => format!(" (oldest reading {}h ago", minutes / 60),
        })
        .map(|text| format!("{text}; ghostex accounts --refresh reads again)"))
        .unwrap_or_default();
    println!("Usage room{age}:");
    for agent in &rows {
        let accounts = agent["accounts"].as_u64().unwrap_or(0);
        let usable = agent["usable"].as_u64().unwrap_or(0);
        let mut line = format!("  {}: ", agents::text(agent, "label"));
        if accounts == 0 {
            line.push_str("no account in Settings > Accounts reports its usage");
        } else {
            line.push_str(&format!(
                "{usable} of {accounts} account{} usable",
                if accounts == 1 { "" } else { "s" }
            ));
            if let Some(best) = agent["best"].as_object() {
                let windows = best["windows"].as_str().unwrap_or_default();
                if usable > 1 {
                    line.push_str(&format!(
                        ", most room {} ({windows})",
                        best["account"].as_str().unwrap_or_default()
                    ));
                } else if !windows.is_empty() {
                    line.push_str(&format!(" ({windows})"));
                }
            }
            let unread = agent["noReading"].as_u64().unwrap_or(0);
            if unread > 0 {
                line.push_str(&format!(", {unread} with no usage reading"));
            }
        }
        let users = users_text(agent);
        if !users.is_empty() {
            line.push_str(&format!(" [used by {users}]"));
        }
        println!("{line}");
    }
    let percent = room["lowPercent"].as_f64().unwrap_or(LOW_PERCENT);
    for agent in rows.iter().filter(|agent| agent["inUse"] == true) {
        let why = match agent["flag"].as_str() {
            Some("out") => "no account has room left".to_string(),
            Some("low") => {
                format!("every usable account is at {percent}% or more of its 5h or 7d window")
            }
            Some("lastAccount") => "only one usable account is left".to_string(),
            _ => continue,
        };
        let frees = agent["freesAt"]
            .as_str()
            .and_then(local)
            .map(|at| format!(" The next account frees at {at}."))
            .unwrap_or_default();
        println!(
            "{}: {}, which {} run{} on: {why}.{frees} Ask the user which agent to move to now (playbook: Usage room); never use a limit reset.",
            if agent["flag"] == "out" { "OUT" } else { "LOW" },
            agents::text(agent, "label"),
            users_text(agent),
            if agent["usedBy"]["coordinator"] == true || agent["usedBy"]["threads"].as_u64() != Some(1) { "" } else { "s" },
        );
    }
}
