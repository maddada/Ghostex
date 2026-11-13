//! Reads a Claude account's usage off Claude Code's own `/usage` screen in a hidden terminal and
//! hands it to Claude Swap as that account's newest reading.
//!
//! CDXC:AgentProviders 2026-11-13 DECISION:
//! User, after using a reset on account 4 while its usage panel kept "7d 100%", "Updated 7m ago" and "7d limit reached.": "after reset, I want you to load a quick terminal in the background and open claude on that account using claude swap, and check usage from the terminal screen to get the updated number shown there right away." Claude Swap reads each account's usage on its own schedule, so its reading from before the claim stood until its next poll. After every Claude reset gxserver claims (the usage panel's and the reset watcher's, both through `reset_claim::claim_for_account`), and when the usage panel's "Updated … ago" is clicked (Refresh now), gxserver starts the account the way a session starts it (`cswap run <N> --share-history --`, hooks off so it never shows as a session) in a hidden terminal in its cache folder, opens `/usage`, reads the 5h, 7d and per-model 7d windows, leaves with Escape and `/exit`, and gives the reading to Claude Swap (`cswap import-usage`). Every reader of Claude Swap's usage (the usage panel, the sidebar meters, Settings > Accounts, `ghostex accounts`, Ghostex's and Claude Swap's own auto-switch) then has it at once. A read that fails leaves the usual usage refresh in charge and says so in gxserver's log.
//! SEE-ALSO: server/src/agent_model_pickers.rs (the hidden terminal and Claude's startup), apps/desktop/src/app/window/account_usage/panel.rs (Refresh now).

use super::{
    helpers, launch,
    model::{Provider, SavedAccount},
    store,
};
use crate::{
    agent_model_pickers::{wait_for_claude_prompt, Hidden, ENTER, ESCAPE},
    domain::DomainStateError,
    server::AppState,
};
use jiff::{civil, tz::TimeZone, Timestamp};
use portable_pty::CommandBuilder;
use serde_json::{json, Map, Value};
use std::{
    collections::HashSet,
    io::{Read, Write},
    path::Path,
    process::Stdio,
    sync::Mutex,
    time::{Duration, Instant},
};

/// gxserver's own folder Claude starts in, inside its cache folder.
const SCRATCH: &str = "usage-reader-scratch";

/// Accounts whose screen is being read now, so a second request waits for none and starts none.
static IN_FLIGHT: Mutex<Option<HashSet<String>>> = Mutex::new(None);

struct InFlight(String);
impl Drop for InFlight {
    fn drop(&mut self) {
        if let Some(set) = IN_FLIGHT.lock().unwrap_or_else(|e| e.into_inner()).as_mut() {
            set.remove(&self.0);
        }
    }
}

/// Which limit a `/usage` section shows.
#[derive(Clone, Debug, PartialEq)]
enum Limit {
    /// "Current session"
    FiveHour,
    /// "Current week (all models)"
    SevenDay,
    /// "Current week (Fable)": the model's own 7d limit, named as Claude Swap names it.
    Model(String),
}

#[derive(Clone, Debug, PartialEq)]
struct ScreenWindow {
    limit: Limit,
    pct: f64,
    /// "Nov 15 at 1:59pm (Asia/Dubai)", the text after "Resets ".
    resets: Option<String>,
}

/// Reads the account's usage off its `/usage` screen and gives it to Claude Swap, after a reset
/// was claimed for it, without holding up the claim's answer. A failed read is logged and the
/// usual usage refresh stays in charge.
pub(crate) fn refresh_in_background(
    state: &AppState,
    account: &SavedAccount,
    reason: &'static str,
) {
    if account.provider != Provider::Claude {
        return;
    }
    let (state, account) = (state.clone(), account.clone());
    std::thread::spawn(move || {
        let started = Instant::now();
        let result = refresh(&state, &account);
        log(&state, &account, reason, started, &result);
        if result.is_ok() {
            state.accounts.refresh_since(&state.paths.home_dir, started);
        }
    });
}

/// `readUsageNow`: the usage panel's Refresh now. A Claude account is read off its `/usage`
/// screen, falling back to the usual refresh when that fails; a Codex account's usual refresh
/// already asks its usage service directly.
pub(crate) fn read_now(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let id = params
        .get("id")
        .and_then(Value::as_str)
        .filter(|id| !id.trim().is_empty() && id.len() <= 128)
        .ok_or_else(|| DomainStateError::bad_request("id is required."))?;
    let account = {
        let db = crate::storage::open_gxserver_database(&state.paths).map_err(store::error)?;
        store::read(&db)?
            .accounts
            .into_iter()
            .find(|a| a.id == id)
            .ok_or_else(|| DomainStateError::bad_request("Choose a saved account."))?
    };
    let started = Instant::now();
    let home = &state.paths.home_dir;
    if account.provider != Provider::Claude {
        state.accounts.refresh_since(home, started);
        return Ok(json!({"ok": true, "source": "usage"}));
    }
    let result = refresh(state, &account);
    log(state, &account, "refreshNow", started, &result);
    state.accounts.refresh_since(home, started);
    Ok(match result {
        Ok(_) => json!({"ok": true, "source": "screen"}),
        Err(reason) => json!({"ok": false, "source": "usage", "message": reason}),
    })
}

fn log(
    state: &AppState,
    account: &SavedAccount,
    reason: &str,
    started: Instant,
    result: &Result<String, String>,
) {
    let (level, error, outcome) = match result {
        Ok(outcome) => (crate::logging::LogLevel::Info, None, outcome.as_str()),
        Err(error) => (
            crate::logging::LogLevel::Warn,
            Some(format!(
                "{error} The usual usage refresh stays in charge of this account."
            )),
            "failed",
        ),
    };
    let _ = state.logger.log(crate::logging::GxserverLogInput {
        level,
        event: "accounts.usageScreenRead".to_string(),
        server_id: None,
        request_id: None,
        client: None,
        duration_ms: Some(started.elapsed().as_millis()),
        error,
        details: Some(json!({
            "provider": account.provider,
            "selector": account.selector,
            "reason": reason,
            "outcome": outcome,
        })),
    });
}

/// Reads the screen and hands the reading over; `Ok` carries Claude Swap's answer ("adopted", or
/// "kept" when it already held a newer reading).
fn refresh(state: &AppState, account: &SavedAccount) -> Result<String, String> {
    let _in_flight = {
        let mut set = IN_FLIGHT.lock().unwrap_or_else(|e| e.into_inner());
        if !set
            .get_or_insert_with(HashSet::new)
            .insert(account.id.clone())
        {
            return Err("This account's usage screen is already being read.".into());
        }
        InFlight(account.id.clone())
    };
    let home = &state.paths.home_dir;
    let row = listed_row(home, account)?;
    let argv = launch::claude_argv(home, account).map_err(|e| e.message)?;
    let scratch = state.paths.app_cache_dir.join(SCRATCH);
    std::fs::create_dir_all(&scratch)
        .map_err(|_| "gxserver's scratch folder could not be made.".to_string())?;
    let (screen, read_at) = read_screen(&argv, &scratch)
        .ok_or("Claude Code's usage screen could not be read on this account.")?;
    let windows = parse_usage_screen(&screen);
    if !windows.iter().any(|w| w.limit == Limit::SevenDay) {
        return Err("Claude Code's usage screen showed no 7d limit.".into());
    }
    let previous = if row["usage"].is_object() {
        &row["usage"]
    } else {
        &row["lastGoodUsage"]
    };
    let usage = usage_json(&windows, previous, Timestamp::now());
    import(home, &row, usage, read_at.elapsed().as_secs_f64())
}

/// The account's `cswap list --json` row, refused when the slot holds another login now.
fn listed_row(home: &Path, account: &SavedAccount) -> Result<Value, String> {
    let listed = helpers::json_command(home, Provider::Claude.helper(), &["list", "--json"])?;
    let row = listed["accounts"]
        .as_array()
        .and_then(|rows| {
            rows.iter().find(|row| {
                row["number"].as_u64().map(|n| n.to_string()).as_deref()
                    == Some(account.selector.as_str())
            })
        })
        .cloned()
        .ok_or("The saved account is no longer in Claude Swap.")?;
    let identity = format!(
        "{}:{}",
        row["email"].as_str().unwrap_or_default().to_lowercase(),
        row["organizationUuid"].as_str().unwrap_or_default()
    );
    if identity != account.identity {
        return Err("The saved account changed. Refresh Accounts.".into());
    }
    Ok(row)
}

/// Starts Claude on the account, opens `/usage` and returns the screen once its limits show, with
/// when it was read. Leaves with Escape and `/exit`; the terminal is killed when this returns.
fn read_screen(argv: &[String], scratch: &Path) -> Option<(String, Instant)> {
    let (program, args) = argv.split_first()?;
    let mut command = CommandBuilder::new(program);
    command.args(args);
    command.args(["--settings", r#"{"disableAllHooks":true}"#]);
    command.cwd(scratch);
    // The account's own login, never an API key or a Claude profile gxserver was started in;
    // `argv` names the slot's profile itself when it needs one.
    command.env_remove("ANTHROPIC_API_KEY");
    command.env_remove("CLAUDE_CONFIG_DIR");
    let cli = Hidden::spawn_command(command)?;
    wait_for_claude_prompt(&cli)?;
    std::thread::sleep(Duration::from_millis(2000));
    let mut screen = None;
    for _ in 0..2 {
        cli.press(b"/usage", 700);
        cli.press(ENTER, 300);
        if let Some(text) = cli.wait_for(Duration::from_secs(20), usage_screen_ready) {
            // The sections arrive together; one more beat lets the last of them draw.
            std::thread::sleep(Duration::from_millis(800));
            let settled = cli.text();
            screen = Some(if usage_screen_ready(&settled) {
                settled
            } else {
                text
            });
            break;
        }
        // A notice that took the first keys: dismiss it and type the command again.
        cli.press(ESCAPE, 500);
    }
    let read_at = Instant::now();
    cli.press(ESCAPE, 500);
    cli.press(b"/exit", 500);
    cli.press(ENTER, 1500);
    screen.map(|screen| (screen, read_at))
}

fn usage_screen_ready(text: &str) -> bool {
    parse_usage_screen(text)
        .iter()
        .any(|w| w.limit == Limit::SevenDay)
}

/// The limits the `/usage` screen shows: each section's title line, its bar ending "N% used",
/// then "Resets …".
fn parse_usage_screen(text: &str) -> Vec<ScreenWindow> {
    let mut windows: Vec<ScreenWindow> = Vec::new();
    let mut section: Option<Limit> = None;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with("Current session") {
            section = Some(Limit::FiveHour);
            continue;
        }
        if let Some(rest) = line.strip_prefix("Current week") {
            let rest = rest.trim();
            section = match rest
                .strip_prefix('(')
                .and_then(|rest| rest.strip_suffix(')'))
                .map(str::trim)
            {
                None if rest.is_empty() => Some(Limit::SevenDay),
                Some("all models") => Some(Limit::SevenDay),
                Some(model) if !model.is_empty() => Some(Limit::Model(
                    model.trim_end_matches(" only").trim().to_string(),
                )),
                _ => None,
            };
            continue;
        }
        let Some(limit) = section.clone() else {
            continue;
        };
        if let Some(pct) = used_percent(line) {
            windows.retain(|w| w.limit != limit);
            windows.push(ScreenWindow {
                limit,
                pct,
                resets: None,
            });
            continue;
        }
        if let Some(resets) = line.strip_prefix("Resets ") {
            if let Some(window) = windows.iter_mut().rev().find(|w| w.limit == limit) {
                window.resets = Some(resets.trim().to_string());
            }
            section = None;
        }
    }
    windows
}

/// `███   92% used` ⇒ 92.
fn used_percent(line: &str) -> Option<f64> {
    let at = line.find("% used")?;
    let digits = line[..at]
        .rsplit(|c: char| !(c.is_ascii_digit() || c == '.'))
        .next()?;
    let pct: f64 = digits.parse().ok()?;
    (pct.is_finite() && pct >= 0.).then(|| pct.min(100.))
}

/// `Nov 15 at 1:59pm (Asia/Dubai)`, `Nov 15, 1:59pm (Asia/Dubai)` or `3pm (Asia/Dubai)` as the
/// next such instant after `now`, in RFC 3339.
fn reset_instant(text: &str, now: Timestamp) -> Option<String> {
    let (wall, zone) = match text.rfind(" (") {
        Some(at) => (
            text[..at].trim(),
            text[at + 2..].trim().trim_end_matches(')').trim(),
        ),
        None => (text.trim(), ""),
    };
    let zone = TimeZone::get(zone).unwrap_or_else(|_| TimeZone::system());
    let (date, time) = match wall.split_once(" at ").or_else(|| wall.split_once(", ")) {
        Some((date, time)) => (Some(date.trim()), time.trim()),
        None => (None, wall),
    };
    let (hour, minute) = clock(time)?;
    let today = now.to_zoned(zone.clone()).date();
    let instant = |date: civil::Date| -> Option<Timestamp> {
        Some(
            date.at(hour, minute, 0, 0)
                .to_zoned(zone.clone())
                .ok()?
                .timestamp(),
        )
    };
    let resets = match date {
        Some(date) => {
            let (month, day) = date.split_once(' ')?;
            let month = month_number(month)?;
            let day: i8 = day.trim().parse().ok()?;
            let this_year = instant(civil::Date::new(today.year(), month, day).ok()?)?;
            // A date already a day gone is next year's ("Jan 2" read on Dec 30).
            if this_year.as_second() < now.as_second() - 86_400 {
                instant(civil::Date::new(today.year() + 1, month, day).ok()?)?
            } else {
                this_year
            }
        }
        None => {
            let today_at = instant(today)?;
            if today_at < now {
                instant(today.tomorrow().ok()?)?
            } else {
                today_at
            }
        }
    };
    Some(resets.to_string())
}

/// `1:59pm` ⇒ (13, 59); `3pm` ⇒ (15, 0).
fn clock(text: &str) -> Option<(i8, i8)> {
    let text = text.trim().to_ascii_lowercase();
    let (digits, pm) = if let Some(rest) = text.strip_suffix("pm") {
        (rest.trim(), true)
    } else if let Some(rest) = text.strip_suffix("am") {
        (rest.trim(), false)
    } else {
        return None;
    };
    let (hour, minute) = match digits.split_once(':') {
        Some((hour, minute)) => (hour.parse::<i8>().ok()?, minute.parse::<i8>().ok()?),
        None => (digits.parse::<i8>().ok()?, 0),
    };
    if !(1..=12).contains(&hour) || !(0..60).contains(&minute) {
        return None;
    }
    Some(((hour % 12) + if pm { 12 } else { 0 }, minute))
}

fn month_number(name: &str) -> Option<i8> {
    const MONTHS: [&str; 12] = [
        "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
    ];
    let name = name.trim().to_ascii_lowercase();
    let at = MONTHS.iter().position(|month| name.starts_with(month))?;
    Some(at as i8 + 1)
}

/// The reading in `cswap list --json`'s `usage` shape. A window whose reset time can't be read
/// keeps the one Claude Swap's last reading had, and that reading's extra usage (spend) stays.
fn usage_json(windows: &[ScreenWindow], previous: &Value, now: Timestamp) -> Value {
    let window = |w: &ScreenWindow, previous: &Value| {
        let mut out = json!({ "pct": w.pct });
        let resets_at = w
            .resets
            .as_deref()
            .and_then(|text| reset_instant(text, now))
            .map(Value::String)
            .or_else(|| previous.get("resetsAt").filter(|v| v.is_string()).cloned());
        if let Some(resets_at) = resets_at {
            out["resetsAt"] = resets_at;
        }
        out
    };
    let mut usage = json!({});
    let mut scoped = Vec::new();
    for w in windows {
        match &w.limit {
            Limit::FiveHour => usage["fiveHour"] = window(w, &previous["fiveHour"]),
            Limit::SevenDay => usage["sevenDay"] = window(w, &previous["sevenDay"]),
            Limit::Model(name) => {
                let before = previous["scoped"]
                    .as_array()
                    .and_then(|rows| rows.iter().find(|row| row["name"] == name.as_str()))
                    .unwrap_or(&Value::Null);
                let mut entry = window(w, before);
                entry["name"] = json!(name);
                scoped.push(entry);
            }
        }
    }
    if !scoped.is_empty() {
        usage["scoped"] = json!(scoped);
    }
    if previous["spend"].is_object() {
        usage["spend"] = previous["spend"].clone();
    }
    usage
}

/// Gives the reading to Claude Swap as the account's newest (`cswap import-usage`, which keeps a
/// newer reading of its own).
fn import(home: &Path, row: &Value, usage: Value, age_seconds: f64) -> Result<String, String> {
    let document = json!({
        "schemaVersion": 1,
        "accounts": [{
            "email": row["email"],
            "organizationUuid": row["organizationUuid"],
            "usageStatus": "ok",
            "usageAgeSeconds": age_seconds.max(0.),
            "usage": usage,
        }],
    });
    let binary = helpers::executable(home, Provider::Claude.helper())
        .ok_or("Claude Swap is not installed.")?;
    let mut command = crate::platform::process::background_command(binary);
    command
        .args(["import-usage", "-"])
        .env("HOME", home)
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    command.env("USERPROFILE", home);
    let mut child = command
        .spawn()
        .map_err(|_| "Claude Swap could not be started.".to_string())?;
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(document.to_string().as_bytes());
    }
    let mut stderr = child.stderr.take();
    let deadline = Instant::now() + Duration::from_secs(30);
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(50)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err("Claude Swap did not take the reading in time.".into());
            }
        }
    };
    let mut said = String::new();
    if let Some(stderr) = stderr.as_mut() {
        let _ = stderr.take(64 * 1024).read_to_string(&mut said);
    }
    if !status.success() {
        return Err("Claude Swap refused the reading.".into());
    }
    Ok(if said.contains("Adopted usage") {
        "adopted".into()
    } else {
        "kept".into()
    })
}
