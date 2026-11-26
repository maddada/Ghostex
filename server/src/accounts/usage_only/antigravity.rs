//! Antigravity: the usage of the Antigravity CLI (`agy`) signed in on this computer, read from agy
//! itself (`agy -p "/usage" --output-format json`). Ghostex never signs in to Google for it, never
//! holds a token and never calls a Google service: agy asks with its own sign-in, as itself.
//!
//! CDXC:AgentProviders 2026-11-26 DECISION:
//! User, on the Antigravity compliance page (Q5146 to Q5152): Antigravity supports only ACP for
//! external clients, and calling Google's internal APIs or posing as another client can get the
//! user's Google account flagged. So the Gemini usage account (Google's internal Cloud Code quota
//! API, signed in with the Antigravity app's own OAuth client and its user agent) is gone, and the
//! Antigravity meter comes from agy's own `/usage` (option A). One check on 2026-11-26 (agy 1.5.18)
//! showed it is a read-only slash command in print mode: `num_turns` 0, no tokens, no conversation
//! written, and the same groups as agy's `/usage` panel (`command.data.groups[].buckets[]`).
//! It is read sparingly (`FRESH_FOR`), never more than once a minute even when asked.
//! SEE-ALSO: server/src/accounts/usage_only/retire_gemini.rs (what happens to saved Gemini accounts).

use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::Value;

use super::http::{self, Failure};
use super::signin::{Progress, SignedIn};
use super::store::{Credential, UsageAccount};
use super::Reading;
use crate::accounts::model::UsageWindow;

/// How long a reading is kept before agy is asked again.
pub(super) const FRESH_FOR: Duration = Duration::from_secs(10 * 60);
/// The least time between two reads, even for Settings' Refresh.
pub(super) const AT_MOST_EVERY: Duration = Duration::from_secs(60);
/// The one identity of this computer's Antigravity entry: agy's own sign-in, whoever it is.
const IDENTITY: &str = "antigravity:this-computer";
const READ_TIMEOUT: Duration = Duration::from_secs(45);
const NOT_INSTALLED: &str =
    "The Antigravity CLI isn't installed on this computer. Install it, then try again.";
const SIGNED_OUT: &str =
    "The Antigravity CLI on this computer is signed out. Run agy in a terminal to sign in, then try again.";
const NO_USAGE: &str = "The Antigravity CLI didn't report its usage. Ghostex tries again later.";

/// Adding Antigravity in Settings > Accounts: no browser and no token, only a first read from agy.
pub(crate) fn sign_in(progress: &Progress) -> Result<SignedIn, Failure> {
    progress.saving();
    read_usage()?;
    Ok(SignedIn {
        identity: IDENTITY.into(),
        name: "Antigravity".into(),
        email: String::new(),
        credential: Credential::default(),
    })
}

pub(crate) fn read(_account: &UsageAccount) -> Result<(Reading, Option<Credential>), Failure> {
    read_usage().map(|reading| (reading, None))
}

fn read_usage() -> Result<Reading, Failure> {
    let home = crate::resume_lookup::home_dir();
    let program = crate::agent_hooks::probing::resolve_probe_command("agy", &home)
        .ok_or_else(|| Failure::Unavailable(NOT_INSTALLED.into()))?;
    let mut command = Command::new(program);
    command
        .args(["-p", "/usage", "--output-format", "json"])
        .current_dir(&home)
        .env(
            "PATH",
            crate::agent_hooks::probing::normalize_gxserver_process_path(
                std::env::var("PATH").ok().as_deref(),
                &home,
            ),
        )
        .env("NO_COLOR", "1");
    let (key, value) = crate::agent_hooks::probing::AGY_BACKGROUND_AUTO_UPDATE_OFF;
    command.env(key, value);
    let (stdout, stderr) = run(command)?;
    parse(&stdout, &stderr)
}

/// Runs agy to its end (or `READ_TIMEOUT`), with what it printed on stdout and stderr.
fn run(mut command: Command) -> Result<(String, String), Failure> {
    crate::platform::process::NoConsoleWindow::no_console_window(&mut command)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|_| Failure::Unavailable(NOT_INSTALLED.into()))?;
    let mut stdout = child.stdout.take();
    let mut stderr = child.stderr.take();
    let out = std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(stdout) = stdout.as_mut() {
            let _ = stdout.read_to_string(&mut text);
        }
        text
    });
    let err = std::thread::spawn(move || {
        let mut text = String::new();
        if let Some(stderr) = stderr.as_mut() {
            let _ = stderr.read_to_string(&mut text);
        }
        text
    });
    let started = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(_)) => break,
            Ok(None) if started.elapsed() < READ_TIMEOUT => {
                std::thread::sleep(Duration::from_millis(100))
            }
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Failure::Unavailable(NO_USAGE.into()));
            }
        }
    }
    Ok((
        out.join().unwrap_or_default(),
        err.join().unwrap_or_default(),
    ))
}

/// The reading in what agy printed, or why there is none.
fn parse(stdout: &str, stderr: &str) -> Result<Reading, Failure> {
    let answer = stdout
        .lines()
        .rev()
        .find_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
        .or_else(|| serde_json::from_str::<Value>(stdout.trim()).ok());
    let signed_out = |text: &str| {
        let text = text.to_ascii_lowercase();
        text.contains("sign in") || text.contains("signed out") || text.contains("not signed")
    };
    let Some(answer) = answer else {
        return Err(if signed_out(stdout) || signed_out(stderr) {
            Failure::NeedsSignIn(SIGNED_OUT.into())
        } else {
            Failure::Unavailable(NO_USAGE.into())
        });
    };
    let groups = answer
        .pointer("/command/data/groups")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    let reading = map(&groups);
    if !reading.usage.is_empty() {
        return Ok(reading);
    }
    let said = [
        answer["error"].as_str().unwrap_or_default(),
        answer["response"].as_str().unwrap_or_default(),
        stderr,
    ]
    .join(" ");
    Err(if signed_out(&said) {
        Failure::NeedsSignIn(SIGNED_OUT.into())
    } else {
        Failure::Unavailable(NO_USAGE.into())
    })
}

/// Every group agy's `/usage` lists. The Gemini group's 5h and weekly buckets are the account's own
/// `5h` and `7d` windows; every other group (Claude and GPT models) gets the same windows scoped to
/// its name ("Claude/GPT 5h"), as Claude's Sonnet and Codex's Spark limits are.
fn map(groups: &[Value]) -> Reading {
    let mut main = Vec::new();
    let mut scoped = Vec::new();
    for group in groups {
        let buckets = group["buckets"]
            .as_array()
            .map(Vec::as_slice)
            .unwrap_or(&[]);
        let gemini = buckets
            .iter()
            .any(|bucket| bucket_id(bucket).starts_with("gemini-"));
        let model = (!gemini).then(|| group_name(group, buckets));
        let mut group_windows = Vec::new();
        for bucket in buckets {
            let Some((short, seconds)) = bucket_window(bucket) else {
                continue;
            };
            let Some(left) = http::number(field(bucket, "remaining_fraction", "remainingFraction"))
            else {
                continue;
            };
            let id = bucket_id(bucket);
            let window = UsageWindow {
                id: match (&model, short) {
                    (None, "5h") => "fiveHour".into(),
                    (None, _) => "sevenDay".into(),
                    (Some(_), _) if !id.is_empty() => id.into(),
                    (Some(_), _) => short.into(),
                },
                label: match &model {
                    Some(model) => format!("{model} {short}"),
                    None => short.into(),
                },
                used_percent: http::clamp_percent(((1.0 - left.clamp(0.0, 1.0)) * 100.0).round()),
                limit_window_seconds: Some(seconds),
                resets_at: reset_time(bucket),
                model: model.clone(),
            };
            if model.is_some() {
                group_windows.push(window);
            } else if !main.iter().any(|w: &UsageWindow| w.id == window.id) {
                main.push(window);
            }
        }
        group_windows.sort_by_key(|window| window.limit_window_seconds);
        scoped.extend(group_windows);
    }
    // The Gemini 5h window first, then its 7d.
    main.sort_by_key(|window| window.id != "fiveHour");
    let mut usage = main;
    usage.extend(scoped);
    Reading {
        usage,
        plan: None,
        details: Vec::new(),
    }
}

fn field<'a>(value: &'a Value, snake: &str, camel: &str) -> &'a Value {
    match &value[snake] {
        Value::Null => &value[camel],
        found => found,
    }
}

fn bucket_id(bucket: &Value) -> &str {
    field(bucket, "id", "bucketId").as_str().unwrap_or_default()
}

/// When a bucket resets; agy writes the epoch for a window nothing has used yet.
fn reset_time(bucket: &Value) -> Option<String> {
    let text = field(bucket, "reset_time", "resetTime").as_str()?;
    let at = chrono::DateTime::parse_from_rfc3339(text).ok()?;
    (at.timestamp() > 0).then(|| text.to_string())
}

/// A bucket's window as Ghostex labels it and its length: agy's `window` ("5h", "weekly"), else
/// its id's suffix (`gemini-5h`, `3p-weekly`).
fn bucket_window(bucket: &Value) -> Option<(&'static str, i64)> {
    let window = bucket["window"].as_str().unwrap_or_default();
    let id = bucket_id(bucket);
    if window == "5h" || id.ends_with("-5h") {
        Some(("5h", 5 * 3600))
    } else if window == "weekly" || id.ends_with("-weekly") {
        Some(("7d", 7 * 86400))
    } else {
        None
    }
}

/// The short name a non-Gemini group's windows carry: "Claude/GPT" for the third-party models'
/// group, else agy's group name without a trailing "models".
fn group_name(group: &Value, buckets: &[Value]) -> String {
    if buckets
        .iter()
        .any(|bucket| bucket_id(bucket).starts_with("3p-"))
    {
        return "Claude/GPT".into();
    }
    let name = field(group, "name", "displayName")
        .as_str()
        .unwrap_or_default()
        .trim();
    let lower = name.to_lowercase();
    let name = if lower.ends_with(" models") {
        name[..name.len() - " models".len()].trim()
    } else {
        name
    };
    if name.is_empty() {
        "Other".into()
    } else {
        name.to_string()
    }
}
