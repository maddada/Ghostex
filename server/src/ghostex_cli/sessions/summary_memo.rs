//! A two second memo of the printed `ghostex sessions --json --mobile-summary` answer.
//!
//! CDXC:StateSync 2026-11-10 WHY: phones before the `useSessionLinksRefresh` fix re-run the summary
//! back to back while a chat is open (measured 70 runs in 20 s from one phone, about 3.9 a second
//! from two). Each run is a new CLI process making five gxserver calls (listProjects,
//! listRecentProjects, listSessions, readPresentationSnapshot, readSidebarHud) that rebuild the
//! project set, the session list and the snapshot: about half of gxserver's CPU. The memo sits in
//! the CLI, in front of all five, so a repeated read costs gxserver nothing. It keeps the exact
//! printed text, so the phone reads the same bytes. Phones at 11.2.1 and later run this CLI over
//! SSH and need no change.
//!
//! Freshness: an answer is reused only while less than [`MEMO_MS`] has passed since its run
//! started, so no change to sessions stays hidden longer than that. Any other gxserver request this
//! CLI sends (a session action, a rename, a chat send) stamps a change mark first, and a memo whose
//! run started before the mark is not reused; a phone's own action followed by its fresh read is
//! always answered live, even when another phone's read was in flight during the action.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::ghostex_cli::rpc::gxserver_root;

const MEMO_MS: u64 = 2_000;
const MEMO_FILE: &str = "cli-mobile-summary-memo.txt";
const CHANGE_MARK_FILE: &str = "cli-mobile-summary-memo.changed";

/// The summary's own reads; every other gxserver path stamps the change mark.
const SUMMARY_READ_PATHS: [&str; 5] = [
    "/api/listProjects",
    "/api/listRecentProjects",
    "/api/listSessions",
    "/api/readPresentationSnapshot",
    "/api/readSidebarHud",
];

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

/// What the answer depends on besides gxserver's state: the arguments and the server the CLI is
/// pointed at.
pub(crate) fn summary_memo_key(args: &[String]) -> String {
    let server = std::env::var("GHOSTEX_GXSERVER_SERVER").unwrap_or_default();
    let mut key = format!("server={server}");
    for arg in args {
        key.push('\u{1f}');
        key.push_str(arg);
    }
    key.replace('\n', " ")
}

/// The printed text of a run that started less than [`MEMO_MS`] ago with the same key and after the
/// last change mark.
pub(crate) fn read_summary_memo(key: &str) -> Option<String> {
    let text = fs::read_to_string(memo_path()).ok()?;
    let (header, body) = text.split_once('\n')?;
    let (started, memo_key) = header.split_once(' ')?;
    let started: u64 = started.parse().ok()?;
    let now = now_ms();
    if memo_key != key || now < started || now - started >= MEMO_MS {
        return None;
    }
    let changed = fs::read_to_string(change_mark_path())
        .ok()
        .and_then(|mark| mark.trim().parse::<u64>().ok());
    if changed.is_some_and(|changed| changed >= started) {
        return None;
    }
    Some(body.to_string())
}

/// Keeps `printed` for runs within [`MEMO_MS`] of `started_ms`. Written to a private temporary file
/// and renamed, so a reader never sees half of it. Failing to write only loses the memo.
pub(crate) fn write_summary_memo(key: &str, started_ms: u64, printed: &str) {
    let path = memo_path();
    let temporary = path.with_extension(format!("txt.{}.tmp", std::process::id()));
    let written = (|| -> std::io::Result<()> {
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        file.write_all(format!("{started_ms} {key}\n").as_bytes())?;
        file.write_all(printed.as_bytes())?;
        drop(file);
        fs::rename(&temporary, &path)
    })();
    if written.is_err() {
        let _ = fs::remove_file(&temporary);
    }
}

/// Called before every gxserver request the CLI sends: anything but the summary's own reads may
/// change what the summary shows, so the memo stops being reused from now on.
pub(crate) fn note_gxserver_request(pathname: &str) {
    if SUMMARY_READ_PATHS.contains(&pathname) {
        return;
    }
    let _ = fs::write(change_mark_path(), now_ms().to_string());
}

fn memo_path() -> PathBuf {
    gxserver_root().join(MEMO_FILE)
}

fn change_mark_path() -> PathBuf {
    gxserver_root().join(CHANGE_MARK_FILE)
}
