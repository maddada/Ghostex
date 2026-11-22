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
//! agent in use when every usable account is low (see `LONG_PERCENT` and `SHORT_PERCENT`) or none
//! is usable, and a plain note when only one usable account is left of several. The playbook's
//! "Usage room" says what the coordinator does then.
//!
//! CDXC:Coordinators 2026-11-22 DECISION:
//! User, on the Usage room rules page (Q4408 "split", Q4409 "never"): Ghostex itself moves threads
//! between accounts, so a coordinator only guards the agent it runs on. LOW means every usable
//! account has its 7d (or any longer) window at 80% or more, or its 5h window at 95% or more with an
//! hour or more left before that window resets: a 5h wall that frees within the hour is Ghostex's
//! wait to handle, not a reason to bother the user. "Last account" (one usable account of several)
//! stays a status line, never a held ask. A coordinator never moves a thread between accounts on
//! its own; it does so only when the user asks.
//!
//! CDXC:Coordinators 2026-11-22 DECISION:
//! "Usable" means an account a thread may run on without asking: ready, and one Ghostex's own
//! automatic switching may pick (the roster's `autoSwitchable`, `SavedAccount::auto_switchable`).
//! A manual-only account never counts as room, for "most room" or for LOW and OUT; it showed as
//! "most room #2" on the user's work account. User, asked whether status should still say a
//! manual-only account has room: "mention". It is named as "(manual only: #2 has room, ask the
//! user)", an option only the user can pick. Turned-off accounts are left out entirely.
//! SEE-ALSO: server/src/accounts/endpoint.rs (`cachedOnly` roster), server/src/ghostex_cli/accounts.rs,
//! server/src/coordinators/playbook.md ("Usage room").

use std::collections::BTreeMap;

use chrono::{DateTime, Local, Utc};
use serde_json::{json, Value};

use crate::ghostex_cli::{
    accounts as accounts_cli, agents, args::Flags, rpc::call_gxserver_rpc,
};

/// An account at this share of its 7d window (or any window longer than 5h, such as a usage-only
/// account's month) or more is close to out: days of work are gone when it fills.
const LONG_PERCENT: f64 = 80.0;

/// An account at this share of its 5h window or more is close to out, unless that window resets
/// within `SHORT_RESET_HOURS`: one long thread turn can take it from here to its limit.
const SHORT_PERCENT: f64 = 95.0;

/// A 5h window that resets sooner than this is waited out by Ghostex, not a LOW.
const SHORT_RESET_HOURS: i64 = 1;

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
    email: String,
    state: String,
    usable: bool,
    /// Ready with room, but manual only: the user's to pick, never counted as room.
    manual_room: bool,
    /// Ghostex's automatic switching may pick it (ready or not).
    automatic: bool,
    default: bool,
    manual_only: bool,
    no_usage_reading: bool,
    coordinator_on: bool,
    /// The fullest account-wide window's percent, `None` with no reading.
    used: Option<f64>,
    /// The fullest window that is not the 5h one (7d, a month), `None` with none.
    long: Option<f64>,
    /// The 5h window's percent and when it resets, `None` with no 5h window.
    short: Option<(f64, Option<DateTime<Utc>>)>,
    windows: String,
    windows_json: Vec<Value>,
    resets: accounts_cli::LimitResetSummary,
    /// When a limited account's last full window resets.
    frees_at: Option<DateTime<Utc>>,
    updated_at: Option<DateTime<Utc>>,
}

fn parse_time(text: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|at| at.with_timezone(&Utc))
}

impl Account {
    /// Close to out: its long window is at `LONG_PERCENT`, or its 5h window at `SHORT_PERCENT`
    /// with an hour or more before it resets (no reset time counts as more than an hour).
    fn low(&self, now: DateTime<Utc>) -> bool {
        self.long.is_some_and(|used| used >= LONG_PERCENT)
            || self.short.is_some_and(|(used, resets)| {
                used >= SHORT_PERCENT
                    && resets.is_none_or(|at| at - now >= chrono::Duration::hours(SHORT_RESET_HOURS))
            })
    }
}

fn account(row: &Value, coordinator: &Value, now: DateTime<Utc>) -> Account {
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
    let long = windows
        .iter()
        .filter(|window| window["label"] != "5h")
        .filter_map(|window| window["usedPercent"].as_f64())
        .reduce(f64::max);
    let short = windows
        .iter()
        .filter(|window| window["label"] == "5h")
        .filter_map(|window| {
            Some((
                window["usedPercent"].as_f64()?,
                window["resetsAt"].as_str().and_then(parse_time),
            ))
        })
        .max_by(|a, b| a.0.total_cmp(&b.0));
    let full: Vec<DateTime<Utc>> = windows
        .iter()
        .filter(|window| window["usedPercent"].as_f64().unwrap_or(0.0) >= 100.0)
        .filter_map(|window| window["resetsAt"].as_str().and_then(parse_time))
        .filter(|at| *at > now)
        .collect();
    // An older gxserver's roster has no `autoSwitchable`: `eligible` is the same setting.
    let automatic = usage_only
        || row
            .get("autoSwitchable")
            .and_then(Value::as_bool)
            .unwrap_or(row["eligible"] == true);
    let ready = if usage_only {
        // A failed read keeps its last numbers (`usageStale`); they still say how much is left.
        let read = row["status"] == "ready" || (row["status"] == "unavailable" && used.is_some());
        read && used.is_none_or(|used| used < 100.0)
    } else {
        row["state"] == "ready" && row["userDisabled"] != true
    };
    let usable = ready && automatic;
    let manual_room =
        ready && !automatic && row["loginExpired"] != true && used.is_none_or(|used| used < 100.0);
    let email = agents::text(row, "email").to_string();
    let name = if usage_only {
        if email.is_empty() {
            agents::text(row, "name").to_string()
        } else {
            email.clone()
        }
    } else {
        format!("#{}", agents::text(row, "number"))
    };
    let limited = row["limitReached"]
        .as_array()
        .is_some_and(|windows| !windows.is_empty());
    let state = if usage_only {
        match agents::text(row, "status") {
            "loginRequired" => "sign-in".to_string(),
            "loading" => "loading".to_string(),
            other => other.to_string(),
        }
    } else if agents::text(row, "state") == "needsSignIn" || row["loginExpired"] == true {
        "sign-in".to_string()
    } else if limited {
        "limit".to_string()
    } else if agents::text(row, "state") == "ready" {
        "ready".to_string()
    } else {
        agents::text(row, "state").to_string()
    };
    let coord_ref = agents::text(coordinator, "globalRef");
    let coord_pid = agents::text(coordinator, "projectId");
    let coord_sid = agents::text(coordinator, "sessionId");
    let coordinator_on = !usage_only
        && row["sessions"].as_array().is_some_and(|sessions| {
            sessions.iter().any(|session| {
                (!coord_ref.is_empty() && agents::text(session, "globalRef") == coord_ref)
                    || (!coord_pid.is_empty()
                        && !coord_sid.is_empty()
                        && agents::text(session, "projectId") == coord_pid
                        && agents::text(session, "sessionId") == coord_sid)
            })
        });
    Account {
        name,
        email,
        state,
        usable,
        manual_room,
        automatic,
        default: !usage_only && row["default"] == true,
        manual_only: !usage_only && row["registered"] == true && row["eligible"] != true,
        no_usage_reading: !usage_only && row["noUsageReading"] == true,
        coordinator_on,
        used,
        long,
        short,
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
        windows_json: accounts_cli::account_windows_json(row),
        resets: accounts_cli::limit_resets(row, now),
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
            .map(|row| account(row, &view["coordinator"], now))
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
            .filter(|account| !account.usable && account.automatic)
            .filter_map(|account| account.frees_at)
            .min();
        let manual_room: Vec<Value> = accounts
            .iter()
            .filter(|account| account.manual_room)
            .map(|account| json!({ "account": account.name, "windows": account.windows }))
            .collect();
        let limit_resets_count: u64 = accounts.iter().map(|account| account.resets.count).sum();
        let account_details: Vec<Value> = accounts
            .iter()
            .map(|account| {
                let mut detail = json!({
                    "account": account.name,
                    "email": account.email,
                    "state": account.state,
                    "usable": account.usable,
                    "default": account.default,
                    "manualOnly": account.manual_only,
                    "windows": account.windows_json,
                    "limitResets": account.resets.to_json(),
                });
                if account.no_usage_reading {
                    detail["noUsageReading"] = json!(true);
                }
                if account.coordinator_on {
                    detail["coordinatorOn"] = json!(true);
                }
                detail
            })
            .collect();
        // The accounts a thread could ever run on without asking: one usable account left of
        // several of those is a warning; manual-only and turned-off ones never were in the pool.
        let pool = accounts.iter().filter(|account| account.automatic).count();
        let in_use = coordinator || threads > 0;
        let flag = if accounts.is_empty() {
            None
        } else if usable.is_empty() {
            Some("out")
        } else if unread == 0 && usable.iter().all(|a| a.low(now)) {
            Some("low")
        } else if usable.len() == 1 && pool > 1 {
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
            "manualRoom": manual_room,
            "limitResetsCount": limit_resets_count,
            "accountDetails": account_details,
            "usedBy": { "coordinator": coordinator, "threads": threads },
            "inUse": in_use,
            "flag": flag,
        }));
    }
    json!({
        "agents": out,
        "lowLongPercent": LONG_PERCENT,
        "lowShortPercent": SHORT_PERCENT,
        "oldestReadingAt": oldest.map(|at| at.to_rfc3339()),
    })
}

fn local(at: &str) -> Option<String> {
    parse_time(at).map(|at| at.with_timezone(&Local).format("%b %-d %H:%M").to_string())
}

/// "you + 2 threads", as the compact snapshot names who runs on an agent.
fn users_short(agent: &Value) -> String {
    let coordinator = agent["usedBy"]["coordinator"] == true;
    let threads = agent["usedBy"]["threads"].as_u64().unwrap_or(0);
    let threads_text = match threads {
        0 => String::new(),
        1 => "1 thread".to_string(),
        n => format!("{n} threads"),
    };
    match (coordinator, threads) {
        (true, 0) => "you".into(),
        (true, _) => format!("you + {threads_text}"),
        _ => threads_text,
    }
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

/// "(manual only: #2 has room, ask the user)" for an agent's manual-only accounts with room.
fn manual_text(agent: &Value) -> String {
    let names: Vec<&str> = agent["manualRoom"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|account| account["account"].as_str().unwrap_or_default())
        .collect();
    match names.as_slice() {
        [] => String::new(),
        [one] => format!(" (manual only: {one} has room, ask the user)"),
        many => format!(
            " (manual only: {} have room, ask the user)",
            many.join(", ")
        ),
    }
}

/// "1 of 4 accounts usable for threads, most room #1 (5h 27%, 7d 4%)" for one agent.
fn room_text(agent: &Value, compact: bool) -> String {
    let accounts = agent["accounts"].as_u64().unwrap_or(0);
    let usable = agent["usable"].as_u64().unwrap_or(0);
    if accounts == 0 {
        return "no account in Settings > Accounts reports its usage".into();
    }
    let mut line = format!(
        "{usable} of {accounts}{}",
        match (compact, accounts) {
            (true, _) => " usable",
            (false, 1) => " account usable for threads",
            (false, _) => " accounts usable for threads",
        }
    );
    if let Some(best) = agent["best"].as_object() {
        let windows = best["windows"].as_str().unwrap_or_default();
        let name = best["account"].as_str().unwrap_or_default();
        if usable > 1 {
            line.push_str(&format!(", most room {name} ({windows})"));
        } else if accounts > 1 && name.starts_with('#') {
            line.push_str(&format!(", {name} ({windows})"));
        } else if !windows.is_empty() {
            line.push_str(&format!(" ({windows})"));
        }
    }
    let unread = agent["noReading"].as_u64().unwrap_or(0);
    if unread > 0 {
        line.push_str(&format!(", {unread} with no usage reading"));
    }
    line
}

/// The summary as `coordinator status --full` and `status usage` print it.
pub(super) fn print(room: &Value) {
    print_room(room, false);
}

/// The summary as the compact `coordinator status` prints it: one line per agent the coordinator
/// can move threads between, the single-account usage-only agents on one line, and LOW and OUT.
pub(super) fn print_compact(room: &Value) {
    print_room(room, true);
}

fn account_detail_line(detail: &Value, name_width: usize) -> String {
    let account = agents::text(detail, "account");
    let state = agents::text(detail, "state");
    let windows = detail["windows"]
        .as_array()
        .map(|windows| accounts_cli::format_windows_json(windows))
        .unwrap_or_default();
    let room = if !windows.is_empty() {
        windows
    } else if state == "sign-in" {
        "needs signing in".to_string()
    } else if state == "loading" {
        "no reading yet".to_string()
    } else if detail["noUsageReading"] == true {
        "no usage reading".to_string()
    } else {
        String::new()
    };
    let resets = accounts_cli::format_limit_resets_json(&detail["limitResets"]);
    let mut parts = Vec::new();
    if !room.is_empty() {
        parts.push(room);
    }
    if let Some(resets) = resets {
        parts.push(resets);
    }
    let mut tags = Vec::new();
    if detail["default"] == true {
        tags.push("default");
    }
    if detail["manualOnly"] == true {
        tags.push("manual only");
    }
    let mut line = format!("    {account:<name_width$}  {state:<7}");
    if !parts.is_empty() {
        line.push_str(&format!("  {}", parts.join(" · ")));
    }
    if !tags.is_empty() {
        line.push_str(&format!("  [{}]", tags.join(", ")));
    }
    line
}

fn print_room(room: &Value, compact: bool) {
    if let Some(error) = room["error"].as_str() {
        println!("Usage room: not read ({error}). ghostex accounts status lists every account.");
        return;
    }
    let rows = room["agents"].as_array().cloned().unwrap_or_default();
    if rows.is_empty() {
        println!("Usage room: no accounts report usage (Settings > Accounts adds them).");
        return;
    }
    let minutes = room["oldestReadingAt"]
        .as_str()
        .and_then(parse_time)
        .map(|at| (Utc::now() - at).num_minutes().max(0));
    let age = minutes
        .filter(|minutes| !compact || *minutes >= 30)
        .map(|minutes| match minutes {
            0 => " (read under a minute ago".to_string(),
            1..=119 => format!(" (oldest reading {minutes}m ago"),
            _ => format!(" (oldest reading {}h ago", minutes / 60),
        })
        .map(|text| format!("{text}; ghostex accounts --refresh reads again)"))
        .unwrap_or_default();
    println!("Usage room{age}:");
    let mut brief = Vec::new();
    for agent in &rows {
        let users = users_text(agent);
        let single = agent["accounts"].as_u64() == Some(1)
            && agent["agent"] != "claude"
            && agent["agent"] != "codex";
        if compact && single && users.is_empty() && agent["flag"].is_null() {
            let windows = agent["best"]["windows"].as_str().unwrap_or_default();
            brief.push(format!(
                "{}: {}",
                agents::text(agent, "label"),
                if agent["usable"] == 1 && !windows.is_empty() {
                    windows.to_string()
                } else {
                    "no room".to_string()
                }
            ));
            continue;
        }
        let mut line = format!(
            "  {}: {}",
            agents::text(agent, "label"),
            room_text(agent, compact)
        );
        if compact {
            let resets = agent["limitResetsCount"].as_u64().unwrap_or(0);
            if resets > 0 {
                line.push_str(&format!(
                    " · {resets} reset{}",
                    if resets == 1 { "" } else { "s" }
                ));
            }
        }
        if compact && !users.is_empty() {
            line.push_str(&format!(" [{}]", users_short(agent)));
        } else if !users.is_empty() {
            line.push_str(&format!(" [used by {users}]"));
        }
        line.push_str(&manual_text(agent));
        println!("{line}");
        if !compact {
            if let Some(details) = agent["accountDetails"].as_array() {
                let name_width = details
                    .iter()
                    .map(|detail| agents::text(detail, "account").chars().count())
                    .max()
                    .unwrap_or(0);
                for detail in details {
                    println!("{}", account_detail_line(detail, name_width));
                }
            }
        }
    }
    if !brief.is_empty() {
        println!("  {}", brief.join(" · "));
    }
    let long = room["lowLongPercent"].as_f64().unwrap_or(LONG_PERCENT);
    let short = room["lowShortPercent"].as_f64().unwrap_or(SHORT_PERCENT);
    for agent in rows.iter().filter(|agent| agent["inUse"] == true) {
        let why = match agent["flag"].as_str() {
            Some("out") => "no account has room left".to_string(),
            Some("low") => format!(
                "every usable account is at {long}% or more of its 7d window, or at {short}% or more of its 5h window with an hour or more to reset"
            ),
            Some("lastAccount") => {
                // A note, never a held ask: one account of several left says nothing of its room.
                println!(
                    "Note: {}: only one usable account is left (no ask unless a LOW or OUT line follows).",
                    agents::text(agent, "label")
                );
                continue;
            }
            _ => continue,
        };
        let frees = agent["freesAt"]
            .as_str()
            .and_then(local)
            .map(|at| format!(" The next account frees at {at}."))
            .unwrap_or_default();
        let (reset_option, note) = match reset_hint(agent) {
            Some(summary) => (
                format!(", or ({summary}) whether the user wants to use a reset"),
                "include its expiry and when the window refills by itself; playbook: Usage room",
            ),
            None => (String::new(), "playbook: Usage room"),
        };
        if compact {
            // Who runs on it and the manual-only room are on the agent's line above.
            println!(
                "{}: {}: {why}.{frees} Raise the held ask: which other agent takes new work{reset_option} ({note}). Ghostex moves threads between accounts; you never do unless the user asks, and you never use a limit reset yourself.",
                if agent["flag"] == "out" { "OUT" } else { "LOW" },
                agents::text(agent, "label"),
            );
            continue;
        }
        println!(
            "{}: {}, which {} run{} on: {why}.{frees}{} Raise the held ask now: which other agent takes new work{reset_option} ({note}). Ghostex moves threads between accounts; you never do unless the user asks, and you never use a limit reset yourself.",
            if agent["flag"] == "out" { "OUT" } else { "LOW" },
            agents::text(agent, "label"),
            users_text(agent),
            if agent["usedBy"]["coordinator"] == true || agent["usedBy"]["threads"].as_u64() != Some(1) { "" } else { "s" },
            manual_text(agent),
        );
    }
    if !compact {
        println!("Limit resets are the user's to use: never spend them.");
    }
}

/// Short summary of available limit resets for the LOW/OUT hint, preferring the account the
/// coordinator itself runs on when known (e.g. `"#1 has 2 resets, first expires Nov 24 18:00"`).
fn reset_hint(agent: &Value) -> Option<String> {
    let details = agent["accountDetails"].as_array()?;
    let format_one = |detail: &Value| -> Option<String> {
        let count = detail["limitResets"]["count"].as_u64().unwrap_or(0);
        if count == 0 {
            return None;
        }
        let account = agents::text(detail, "account");
        let noun = if count == 1 {
            "1 reset".to_string()
        } else {
            format!("{count} resets")
        };
        let first_expiry = detail["limitResets"]["resets"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|reset| reset["expiresAt"].as_str())
            .find_map(local);
        Some(match first_expiry {
            Some(at) if count == 1 => format!("{account} has {noun}, expires {at}"),
            Some(at) => format!("{account} has {noun}, first expires {at}"),
            None => format!("{account} has {noun}"),
        })
    };
    if let Some(own) = details
        .iter()
        .find(|detail| detail["coordinatorOn"] == true)
    {
        if let Some(summary) = format_one(own) {
            return Some(summary);
        }
    }
    let parts: Vec<String> = details.iter().filter_map(format_one).collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("; "))
    }
}
