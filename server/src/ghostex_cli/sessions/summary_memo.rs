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
//! Freshness:
//!
//! CDXC:StateSync 2026-11-10 WHY: an answer is reused only while nothing gxserver keeps in its
//! database has changed since the run that printed it started. A review found the first version,
//! which reused an answer for 2 seconds unless this CLI itself had sent a request, could answer a
//! phone's fresh read after a tapped push with the list from before the agent's question (a hook
//! writes the question into the session row, not through this CLI); the phone then opened the
//! session without its question card. Every session and project write (hook activity, questions,
//! titles, lifecycle) and every presentation change gxserver publishes (each bumps
//! `presentationRevision`) commits to `state.db`, which appends to `state.db-wal`, so the
//! modification time (in nanoseconds) and size of those two files, read before a run starts, change
//! with every commit. A reader whose stamp differs from the memo's runs the summary again. Measured
//! on the user's Mac with agents working: 4 commits in 10 s, so an old phone's 3 to 4 reads a
//! second still mostly reuse. What the stamp cannot see is what gxserver works out at read time
//! without storing it (time-based activity, cached git status); the 2 second cap [`MEMO_MS`] still
//! bounds those. Any other gxserver request this CLI sends also leaves a change mark, so its own
//! action is never answered from before it. Only the local gxserver's files are stamped, so a run
//! against another computer is never memoized.

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::ghostex_cli::rpc::gxserver_root;

const MEMO_MS: u64 = 2_000;
const MEMO_FILE: &str = "cli-mobile-summary-memo.txt";
const CHANGE_MARK_FILE: &str = "cli-mobile-summary-memo.changed";

/// The summary's own reads, and the session lookup that changes nothing; every other gxserver
/// path stamps the change mark.
const SUMMARY_READ_PATHS: [&str; 6] = [
    "/api/listProjects",
    "/api/listRecentProjects",
    "/api/listSessions",
    "/api/readPresentationSnapshot",
    "/api/readSidebarHud",
    "/api/findSessions",
];

pub(crate) fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_millis() as u64)
        .unwrap_or(0)
}

/// One summary run: what its answer depends on besides gxserver's state (the arguments and the
/// server the CLI is pointed at), when it started, and gxserver's database stamp at its start.
pub(crate) struct SummaryRun {
    key: String,
    started_ms: u64,
    stamp: String,
}

/// The run about to start, or `None` when its answer must not be memoized (another computer's
/// gxserver, whose database this CLI cannot stamp).
pub(crate) fn begin_summary_run(args: &[String], local_gxserver: bool) -> Option<SummaryRun> {
    if !local_gxserver {
        return None;
    }
    let server = std::env::var("GHOSTEX_GXSERVER_SERVER").unwrap_or_default();
    let mut key = format!("server={server}");
    for arg in args {
        key.push('\u{1f}');
        key.push_str(arg);
    }
    Some(SummaryRun {
        key: key.replace('\n', " "),
        started_ms: now_ms(),
        stamp: database_stamp(),
    })
}

/// The modification time in nanoseconds and the size of `state.db` and `state.db-wal`, `-` for a
/// missing file. One space-free word.
fn database_stamp() -> String {
    let root = gxserver_root();
    ["state.db", "state.db-wal"]
        .iter()
        .map(|name| {
            fs::metadata(root.join(name))
                .ok()
                .map(|meta| {
                    let modified = meta
                        .modified()
                        .ok()
                        .and_then(|modified| modified.duration_since(UNIX_EPOCH).ok())
                        .map_or(0, |elapsed| elapsed.as_nanos());
                    format!("{modified}.{}", meta.len())
                })
                .unwrap_or_else(|| "-".to_string())
        })
        .collect::<Vec<_>>()
        .join(",")
}

/// The printed text of a run with the same key that started less than [`MEMO_MS`] ago, when
/// gxserver's database is still exactly as it was at that run's start and this CLI sent nothing
/// to gxserver since.
pub(crate) fn read_summary_memo(run: &SummaryRun) -> Option<String> {
    let text = fs::read_to_string(memo_path()).ok()?;
    let (header, body) = text.split_once('\n')?;
    let (started, rest) = header.split_once(' ')?;
    let (stamp, memo_key) = rest.split_once(' ')?;
    let started: u64 = started.parse().ok()?;
    let now = run.started_ms;
    if memo_key != run.key || stamp != run.stamp || now < started || now - started >= MEMO_MS {
        return None;
    }
    // A mark that cannot be read as a time counts as a change.
    match fs::read_to_string(change_mark_path()) {
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(_) => return None,
        Ok(mark) => {
            let changed = mark.trim().parse::<u64>().ok()?;
            if changed >= started {
                return None;
            }
        }
    }
    Some(body.to_string())
}

/// Keeps `printed` as `run`'s answer. Written to a private temporary file and renamed, so a reader
/// never sees half of it. Failing to write only loses the memo.
pub(crate) fn write_summary_memo(run: &SummaryRun, printed: &str) {
    let header = format!("{} {} {}\n", run.started_ms, run.stamp, run.key);
    write_atomically(&memo_path(), &[header.as_bytes(), printed.as_bytes()]);
}

fn write_atomically(path: &std::path::Path, parts: &[&[u8]]) {
    let mut temporary = path.as_os_str().to_owned();
    temporary.push(format!(".{}.{}.tmp", std::process::id(), now_ms()));
    let temporary = PathBuf::from(temporary);
    let written = (|| -> std::io::Result<()> {
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temporary)?;
        for part in parts {
            file.write_all(part)?;
        }
        drop(file);
        fs::rename(&temporary, path)
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
    // Renamed into place like the memo, so a reader never catches it empty.
    write_atomically(&change_mark_path(), &[now_ms().to_string().as_bytes()]);
}

fn memo_path() -> PathBuf {
    gxserver_root().join(MEMO_FILE)
}

fn change_mark_path() -> PathBuf {
    gxserver_root().join(CHANGE_MARK_FILE)
}
