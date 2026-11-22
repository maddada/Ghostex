//! `claude attach <job>` clients, which show a conversation Claude's daemon hosts and write no
//! session record of their own.
//!
//! CDXC:AgentHooks 2026-11-02 DECISION:
//! User, asked how to close the gaps that keep Claude's hooks installed: "Everything, including Windows". A conversation opened with `claude attach <job>` (or picked in `claude agents`, which attaches the same way) has no `<config>/sessions/<pid>.json` for the terminal's client, so the observer could not follow that pane. The client's command line names the job; its pane is found with the same pane walk as any `claude`, and the job folder `<config>/jobs/<job>/state.json` names the conversation. The observer then treats the client as a terminal parked on that job, so the daemon's `kind: bg` record for the job speaks for the pane exactly as it does for a parked `claude` (claude.rs), and the job's own `state` stands in while no such record is live.
//! SEE-ALSO: server/src/hookless_activity/claude.rs (`parkedJobId`), server/src/agent_hooks/claude_daemon.rs (the daemon's hooks, routed to the parked terminal).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use super::claude::{session_dirs, text, LiveRecord};

/// How often the process list is read while the daemon hosts a conversation.
const SCAN_EVERY: Duration = Duration::from_secs(2);
/// Marks a record made up for an attach client, which is no new process on its conversation.
pub(super) const ATTACH_CLIENT: &str = "attachClient";

#[derive(Clone, Debug, PartialEq)]
struct Client {
    pid: u32,
    /// The `<id|name>` the client was given.
    target: String,
    /// When the client started, in ms since the epoch.
    started_at: i64,
}

#[derive(Default)]
pub(super) struct AttachClients {
    clients: Vec<Client>,
    scanned_at: Option<Instant>,
}

impl AttachClients {
    /// One interactive record parked on its job for each live `claude attach` client. The process
    /// list is read only while Claude's daemon hosts a conversation or a client was seen.
    pub(super) fn records(&mut self, home: &Path, live: &[LiveRecord]) -> Vec<LiveRecord> {
        let daemon_runs = live.iter().any(|live| live.record["kind"] == "bg");
        if !daemon_runs && self.clients.is_empty() {
            return Vec::new();
        }
        if self.scanned_at.is_none_or(|at| at.elapsed() >= SCAN_EVERY) {
            self.clients = list_attach_clients();
            self.scanned_at = Some(Instant::now());
        }
        if self.clients.is_empty() {
            return Vec::new();
        }
        let config_dirs: Vec<PathBuf> = session_dirs(home)
            .iter()
            .filter_map(|dir| dir.parent().map(Path::to_path_buf))
            .collect();
        let hosted: HashMap<String, &Value> = live
            .iter()
            .filter(|live| live.record["kind"] == "bg")
            .filter_map(|live| Some((text(&live.record, "jobId")?, &live.record)))
            .collect();
        self.clients
            .iter()
            .filter_map(|client| {
                let (config_dir, job, state) = find_job(&config_dirs, &client.target, &hosted)?;
                Some(LiveRecord {
                    record: client_record(client, &job, &state),
                    config_dir,
                })
            })
            .collect()
    }
}

/// The record a `claude` parked on `job` would write.
fn client_record(client: &Client, job: &str, state: &Value) -> Value {
    // The job's own word on where it stands, for when no daemon record for it is live.
    let (status, waiting_for) = match state["state"].as_str() {
        Some("working") => ("busy", None),
        Some("blocked") => ("waiting", Some("input needed")),
        _ => ("idle", None),
    };
    json!({
        "pid": client.pid,
        "kind": "interactive",
        "parkedJobId": job,
        "sessionId": text(state, "sessionId"),
        "cwd": text(state, "cwd"),
        "status": status,
        "waitingFor": waiting_for,
        "startedAt": client.started_at,
        ATTACH_CLIENT: true,
    })
}

/// The job a client's `<id|name>` names: a job folder, a conversation id or its start, else a job
/// name (Claude takes part of one too). A job the daemon hosts right now wins a tie.
fn find_job(
    config_dirs: &[PathBuf],
    target: &str,
    hosted: &HashMap<String, &Value>,
) -> Option<(PathBuf, String, Value)> {
    let mut named = Vec::new();
    for config_dir in config_dirs {
        let Ok(entries) = std::fs::read_dir(config_dir.join("jobs")) else {
            continue;
        };
        for entry in entries.flatten() {
            let job = entry.file_name().to_string_lossy().into_owned();
            let Some(state) = std::fs::read_to_string(entry.path().join("state.json"))
                .ok()
                .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
            else {
                continue;
            };
            let conversation = text(&state, "sessionId").unwrap_or_default();
            if job == target || (!conversation.is_empty() && conversation.starts_with(target)) {
                return Some((config_dir.clone(), job, state));
            }
            let name = text(&state, "name").unwrap_or_default().to_lowercase();
            let wanted = target.to_lowercase();
            if !wanted.is_empty() && name.contains(&wanted) {
                named.push((
                    name == wanted,
                    hosted.contains_key(&job),
                    config_dir.clone(),
                    job,
                    state,
                ));
            }
        }
    }
    named.sort_by_key(|(exact, live, ..)| (!*exact, !*live));
    named
        .into_iter()
        .next()
        .map(|(_, _, config_dir, job, state)| (config_dir, job, state))
}

/// Every running `claude attach <id|name>`.
#[cfg(unix)]
fn list_attach_clients() -> Vec<Client> {
    let Ok(output) = std::process::Command::new("/bin/ps")
        .args(["-A", "-o", "pid=,etime=,command="])
        .env("LC_ALL", "C")
        .output()
    else {
        return Vec::new();
    };
    let now = chrono::Utc::now().timestamp_millis();
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter_map(|line| parse_ps_line(line, now))
        .collect()
}

/// Every running `claude attach <id|name>` on native Windows: each `claude.exe`'s command line,
/// read the way the wmx pane walk reads its host's (agent_hooks/wmx_pane.rs).
#[cfg(windows)]
fn list_attach_clients() -> Vec<Client> {
    crate::agent_hooks::claude_processes()
        .into_iter()
        .filter_map(|(pid, started_at, command_line)| {
            parse_command_line(pid, started_at, &command_line)
        })
        .collect()
}

#[cfg(not(any(unix, windows)))]
fn list_attach_clients() -> Vec<Client> {
    Vec::new()
}

/// `<pid> <elapsed> <command…>` as `ps -o pid=,etime=,command=` prints it.
#[cfg_attr(not(any(unix, test)), allow(dead_code))]
fn parse_ps_line(line: &str, now: i64) -> Option<Client> {
    let mut words = line.split_whitespace();
    let pid = words.next()?.parse::<u32>().ok()?;
    let elapsed = elapsed_ms(words.next()?)?;
    let program = words.next()?;
    if Path::new(program).file_name()?.to_str()? != "claude" {
        return None;
    }
    let target = attach_target(words)?;
    Some(Client {
        pid,
        target,
        started_at: now - elapsed,
    })
}

/// A Windows command line, `"C:\…\claude.exe" attach <id|name>` (the program quoted or not, any
/// case of `.exe`).
#[cfg_attr(not(any(windows, test)), allow(dead_code))]
fn parse_command_line(pid: u32, started_at: i64, command_line: &str) -> Option<Client> {
    let line = command_line.trim_start();
    let (program, arguments) = match line.strip_prefix('"') {
        Some(quoted) => quoted.split_once('"')?,
        None => line.split_once(char::is_whitespace)?,
    };
    let program = program.rsplit(['\\', '/']).next()?.to_ascii_lowercase();
    if program.strip_suffix(".exe").unwrap_or(&program) != "claude" {
        return None;
    }
    let target = attach_target(arguments.split_whitespace())?;
    Some(Client {
        pid,
        target,
        started_at,
    })
}

/// The `<id|name>` of `claude [flags…] attach <id|name>`: a name with spaces arrives as several
/// words; flags are not part of it.
///
/// CDXC:AgentHooks 2026-11-11 WHY:
/// A Ghostex pane's `claude attach f4a34329` ran as `claude.exe --dangerously-skip-permissions attach f4a34329` on Windows on 2026-11-11, so flags may stand before the subcommand.
fn attach_target<'a>(mut words: impl Iterator<Item = &'a str>) -> Option<String> {
    if words.find(|word| !word.starts_with('-'))? != "attach" {
        return None;
    }
    // A quoted name keeps its spaces; the quotes on a Windows command line are not the name's.
    let target = words
        .map(|word| word.trim_matches('"'))
        .filter(|word| !word.is_empty() && !word.starts_with('-'))
        .collect::<Vec<_>>()
        .join(" ");
    (!target.is_empty()).then_some(target)
}

/// `[[dd-]hh:]mm:ss` in milliseconds.
#[cfg_attr(not(any(unix, test)), allow(dead_code))]
fn elapsed_ms(etime: &str) -> Option<i64> {
    let (days, clock) = match etime.split_once('-') {
        Some((days, clock)) => (days.parse::<i64>().ok()?, clock),
        None => (0, etime),
    };
    let mut seconds = 0_i64;
    for part in clock.split(':') {
        seconds = seconds * 60 + part.parse::<i64>().ok()?;
    }
    Some((days * 86_400 + seconds) * 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_attach_client_is_read_off_the_process_list() {
        let now = 1_000_000_000;
        assert_eq!(
            parse_ps_line(" 4242   01:05 claude attach 3f2a9c1e", now),
            Some(Client {
                pid: 4242,
                target: "3f2a9c1e".to_string(),
                started_at: now - 65_000,
            })
        );
        assert_eq!(
            parse_ps_line(
                "17 1-02:00:00 /Users/x/.local/bin/claude attach my job",
                now
            )
            .map(|client| (client.target, client.started_at)),
            Some(("my job".to_string(), now - 93_600_000))
        );
        assert_eq!(parse_ps_line("18 00:01 claude --resume abc", now), None);
        assert_eq!(parse_ps_line("19 00:01 claude attach", now), None);
        assert_eq!(parse_ps_line("20 00:01 vim attach notes", now), None);
    }

    #[test]
    fn the_target_names_a_job_folder_a_conversation_or_a_name() {
        let dir = std::env::temp_dir().join(format!("gx-attach-{}", uuid::Uuid::new_v4()));
        let write = |job: &str, state: Value| {
            let folder = dir.join("jobs").join(job);
            std::fs::create_dir_all(&folder).unwrap();
            std::fs::write(folder.join("state.json"), state.to_string()).unwrap();
        };
        write(
            "3f2a9c1e",
            json!({"sessionId": "3f2a9c1e-1111-4222-8333-444455556666", "name": "fix the build", "state": "working", "cwd": "/w"}),
        );
        write(
            "9b8c7d6e",
            json!({"sessionId": "9b8c7d6e-1111-4222-8333-444455556666", "name": "write docs", "state": "blocked"}),
        );
        let dirs = [dir.clone()];
        let hosted = HashMap::new();
        let job = |target: &str| find_job(&dirs, target, &hosted).map(|(_, job, _)| job);
        assert_eq!(job("3f2a9c1e").as_deref(), Some("3f2a9c1e"));
        assert_eq!(job("9b8c7d6e-1111").as_deref(), Some("9b8c7d6e"));
        assert_eq!(job("docs").as_deref(), Some("9b8c7d6e"));
        assert_eq!(job("nothing like it"), None);

        let (_, job, state) = find_job(&dirs, "write docs", &hosted).unwrap();
        let client = Client {
            pid: 7,
            target: "write docs".to_string(),
            started_at: 5,
        };
        let record = client_record(&client, &job, &state);
        assert_eq!(record["parkedJobId"], "9b8c7d6e");
        assert_eq!(record["sessionId"], "9b8c7d6e-1111-4222-8333-444455556666");
        assert_eq!(record["status"], "waiting");
        assert_eq!(record["kind"], "interactive");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
