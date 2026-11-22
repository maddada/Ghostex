//! The idle countdown (Q1011): a box is idle while no agent turn runs and no terminal is open.
//! `ghostex box idle-watch` (started by boot, one per box) checks every few seconds and records
//! since when the box has been idle; `idle-status` reports it. Where the provider has no idle
//! timer of its own (Boat; `local` when its ticket asks), the watcher stops the box itself once
//! the countdown runs out. Instabox pauses idle boxes on its own (`autoSuspendAfterSeconds`).

use std::process::{Command, Stdio};
use std::time::Duration;

use chrono::{DateTime, Utc};
use ghostex_gx_protocol::cloud_box::{BoxBootTicket, BoxIdleStatus, CloudProviderId};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::state;
use crate::ghostex_cli::{args::Flags, rpc};

const CHECK_INTERVAL: Duration = Duration::from_secs(10);

/// What the watcher last saw.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct IdleRecord {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    idle_since: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    checked_at: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    self_stopped_at: Option<String>,
}

/// Busy counts right now: agent turns running (a working agent, or a background shell or monitor
/// it left running) and sessions with a terminal attached.
struct Busy {
    running_turns: u64,
    open_terminals: u64,
}

fn running_turns() -> Result<u64, String> {
    let snapshot = rpc::call_gxserver_rpc(
        "/api/readPresentationSnapshot",
        &json!({}),
        &Flags::default(),
    )
    .map_err(|error| format!("The box's gxserver did not answer: {error}"))?;
    let sessions = snapshot
        .pointer("/snapshot/sessions")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    Ok(sessions
        .iter()
        .filter(|session| {
            let text = |key: &str| session.get(key).and_then(Value::as_str);
            text("lifecycleState") == Some("running")
                && (text("activity") == Some("working")
                    || session
                        .get("backgroundWorkDetectedAt")
                        .is_some_and(|value| !value.is_null()))
        })
        .count() as u64)
}

/// Sessions with at least one zmx client attached (`zmx list` prints `clients=<n>`).
fn open_terminals() -> u64 {
    let zmx = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|bin| bin.join("zmx")))
        .filter(|path| path.is_file())
        .unwrap_or_else(|| "zmx".into());
    let Ok(output) = Command::new(zmx)
        .arg("list")
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .output()
    else {
        return 0;
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|line| {
            line.split('\t').any(|field| {
                field
                    .trim()
                    .strip_prefix("clients=")
                    .and_then(|count| count.parse::<u64>().ok())
                    .is_some_and(|count| count > 0)
            })
        })
        .count() as u64
}

fn busy() -> Result<Busy, String> {
    Ok(Busy {
        running_turns: running_turns()?,
        open_terminals: open_terminals(),
    })
}

fn parse_time(text: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(text)
        .ok()
        .map(|time| time.with_timezone(&Utc))
}

fn iso(time: DateTime<Utc>) -> String {
    time.to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

fn countdown(ticket: &BoxBootTicket) -> chrono::Duration {
    chrono::Duration::minutes(i64::try_from(ticket.idle_stop_minutes.max(1)).unwrap_or(15))
}

fn status_from(ticket: &BoxBootTicket, busy: &Busy, idle_since: Option<&str>) -> BoxIdleStatus {
    let idle = busy.running_turns == 0 && busy.open_terminals == 0;
    let idle_since = idle.then(|| idle_since.map(str::to_string)).flatten();
    BoxIdleStatus {
        idle,
        running_turns: busy.running_turns,
        open_terminals: busy.open_terminals,
        stops_at: idle_since
            .as_deref()
            .and_then(parse_time)
            .map(|since| iso(since + countdown(ticket))),
        idle_since,
        idle_stop_minutes: ticket.idle_stop_minutes,
        self_stop: ticket.self_stop,
    }
}

/// `idle-status`: counted now; the idle start comes from the watcher (now, when it has not seen
/// this idle stretch yet).
pub(crate) fn run_idle_status() -> Result<BoxIdleStatus, String> {
    let ticket = state::require_ticket()?;
    let busy = busy()?;
    let record: IdleRecord = state::read_json(&state::idle_file()).unwrap_or_default();
    let since = record.idle_since.unwrap_or_else(state::now_iso);
    Ok(status_from(&ticket, &busy, Some(&since)))
}

/// Stops this box from inside. Boat: its CLI with the per-box key Boat writes into every box
/// (`boat stop self`; that key can stop only its own box). `local`: ends the container's first
/// process, which stops the container the way `docker stop` does.
fn stop_self(provider: &CloudProviderId) -> Result<(), String> {
    let status = match provider {
        CloudProviderId::Boat => Command::new("boat")
            .args(["stop", "self"])
            .stdin(Stdio::null())
            .status(),
        CloudProviderId::Local => Command::new("kill")
            .args(["-TERM", "1"])
            .stdin(Stdio::null())
            .status(),
        other => {
            return Err(format!(
                "{} pauses idle boxes itself; Ghostex does not stop them.",
                other.label()
            ))
        }
    }
    .map_err(|error| format!("Can't stop the box: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("Stopping the box exited with {status}."))
    }
}

/// `ghostex box idle-watch`: runs until the box stops. A failed check (gxserver restarting)
/// keeps the countdown where it was rather than resetting or advancing it.
pub(crate) fn run_idle_watch() -> Result<(), String> {
    let ticket = state::require_ticket()?;
    let mut record = IdleRecord::default();
    state::write_json(&state::idle_file(), &record)?;
    loop {
        std::thread::sleep(CHECK_INTERVAL);
        // A later boot (a resume, or new settings) rewrites the ticket; follow it.
        let ticket = state::read_ticket().unwrap_or_else(|| ticket.clone());
        super::logins::restore_after_restart();
        let Ok(busy) = busy() else { continue };
        let now = Utc::now();
        if busy.running_turns > 0 || busy.open_terminals > 0 {
            record.idle_since = None;
        } else if record.idle_since.is_none() {
            record.idle_since = Some(iso(now));
        }
        record.checked_at = Some(iso(now));
        let due = record
            .idle_since
            .as_deref()
            .and_then(parse_time)
            .is_some_and(|since| now >= since + countdown(&ticket));
        if due && ticket.self_stop && record.self_stopped_at.is_none() {
            record.self_stopped_at = Some(iso(now));
            let _ = state::write_json(&state::idle_file(), &record);
            if let Err(message) = stop_self(&ticket.provider) {
                eprintln!("{message}");
                record.self_stopped_at = None;
            }
        }
        let _ = state::write_json(&state::idle_file(), &record);
    }
}

/// Starts a fresh `ghostex box idle-watch` for this boot, ending the previous one (its ticket
/// may be out of date).
pub(crate) fn restart_watcher(bin: &std::path::Path) -> Result<(), String> {
    let pid_file = state::idle_watch_pid_file();
    if let Some(pid) = std::fs::read_to_string(&pid_file)
        .ok()
        .and_then(|text| text.trim().parse::<i32>().ok())
        .filter(|pid| *pid > 1)
    {
        #[cfg(unix)]
        unsafe {
            libc::kill(pid, libc::SIGTERM);
        }
    }
    let mut command = Command::new(bin.join("ghostex"));
    command
        .args(["box", ghostex_gx_protocol::cloud_box::box_verbs::IDLE_WATCH])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.process_group(0);
    }
    let child = command
        .spawn()
        .map_err(|error| format!("Can't start the idle countdown: {error}"))?;
    state::write_private(&pid_file, child.id().to_string().as_bytes())
}
