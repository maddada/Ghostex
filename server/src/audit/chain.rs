//! The log file itself: one JSON line per entry, each carrying the hash of the one before it, and
//! the check that walks the chain.

use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, PoisonError};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::types::{Action, Actor, ActorRecord, ApprovalRef, Outcome};

/// The `prevHash` of the first entry.
pub const GENESIS_HASH: &str = "0000000000000000000000000000000000000000000000000000000000000000";
const ENTRY_VERSION: u32 = 1;
const LOG_FILE: &str = "audit-log.jsonl";
const LOG_DIR: &str = "security";
#[cfg(unix)]
const DIR_MODE: u32 = 0o700;
#[cfg(unix)]
const FILE_MODE: u32 = 0o600;

/// One line of the log. `hash` is the SHA-256 (hex) of the line's JSON without `hash`, which
/// includes `prevHash`, so each entry seals every entry before it.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct StoredEntry {
    pub v: u32,
    pub seq: u64,
    pub at_ms: i64,
    pub action: String,
    pub summary: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<String>,
    pub actor: ActorRecord,
    pub outcome: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub approval: Option<ApprovalRef>,
    pub prev_hash: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub hash: String,
}

impl StoredEntry {
    /// The hash this entry must carry: SHA-256 of its JSON with `hash` left out.
    pub fn computed_hash(&self) -> String {
        let mut body = self.clone();
        body.hash.clear();
        let text = serde_json::to_string(&body).unwrap_or_default();
        hex::encode(Sha256::digest(text.as_bytes()))
    }
}

/// What a walk of the chain found.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainCheck {
    pub intact: bool,
    pub entries: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub problem: Option<ChainProblem>,
}

/// The first place the chain breaks. `kind`: `unreadable` (not an entry), `edited` (its hash no
/// longer matches), `missing` (an entry before it was removed or moved), `truncated` (entries at
/// the end were removed or the file replaced while gxserver ran).
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ChainProblem {
    pub kind: &'static str,
    /// 1-based line in the file.
    pub line: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seq: Option<u64>,
    pub message: String,
}

struct Head {
    seq: u64,
    hash: String,
    /// Read from the file once; until then `seq`/`hash` are not known.
    loaded: bool,
}

/// One log file. Appends are serialized; the newest hash is kept in memory so a file cut short or
/// rewritten while this process runs is caught by `check`.
pub struct AuditLog {
    path: PathBuf,
    head: Mutex<Head>,
}

impl AuditLog {
    pub fn default_path(gxserver_state_dir: &Path) -> PathBuf {
        gxserver_state_dir.join(LOG_DIR).join(LOG_FILE)
    }

    pub fn open(path: PathBuf) -> Self {
        Self {
            path,
            head: Mutex::new(Head {
                seq: 0,
                hash: GENESIS_HASH.to_string(),
                loaded: false,
            }),
        }
    }

    /// Reads the newest entry now, so a log cut short after this point is caught by `check` even
    /// before this process writes to it.
    pub fn prime(&self) -> Result<()> {
        let mut head = self.head.lock().unwrap_or_else(PoisonError::into_inner);
        if !head.loaded {
            let (seq, hash) = self.read_tail()?;
            head.seq = seq;
            head.hash = hash;
            head.loaded = true;
        }
        Ok(())
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn append(
        &self,
        action: &Action,
        actor: &Actor,
        outcome: &Outcome,
        approval: Option<&ApprovalRef>,
    ) -> Result<StoredEntry> {
        self.append_at(
            chrono::Utc::now().timestamp_millis(),
            action,
            actor,
            outcome,
            approval,
        )
    }

    pub(crate) fn append_at(
        &self,
        at_ms: i64,
        action: &Action,
        actor: &Actor,
        outcome: &Outcome,
        approval: Option<&ApprovalRef>,
    ) -> Result<StoredEntry> {
        let mut head = self.head.lock().unwrap_or_else(PoisonError::into_inner);
        if !head.loaded {
            let (seq, hash) = self.read_tail()?;
            head.seq = seq;
            head.hash = hash;
            head.loaded = true;
        }
        let (outcome, reason) = outcome.wire();
        let mut entry = StoredEntry {
            v: ENTRY_VERSION,
            seq: head.seq + 1,
            at_ms,
            action: action.kind.clone(),
            summary: action.summary.clone(),
            target: action.target.clone(),
            actor: actor.record(),
            outcome: outcome.to_string(),
            reason: reason.map(|reason| reason.chars().take(500).collect()),
            approval: approval.cloned(),
            prev_hash: head.hash.clone(),
            hash: String::new(),
        };
        entry.hash = entry.computed_hash();
        self.write_line(&serde_json::to_string(&entry)?)?;
        head.seq = entry.seq;
        head.hash = entry.hash.clone();
        Ok(entry)
    }

    /// The newest `limit` entries, newest first (unreadable lines skipped; `check` reports them).
    pub fn read_recent(&self, limit: usize) -> Vec<StoredEntry> {
        let Ok(text) = std::fs::read_to_string(&self.path) else {
            return Vec::new();
        };
        text.lines()
            .rev()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str::<StoredEntry>(line).ok())
            .take(limit)
            .collect()
    }

    /// The entries newest first, back to `oldest_seq` and the `window_ms` before it (by time),
    /// so a reader that needs only the end of the log parses no more than that.
    pub fn read_back_to(&self, oldest_seq: u64, window_ms: i64) -> Vec<StoredEntry> {
        let Ok(text) = std::fs::read_to_string(&self.path) else {
            return Vec::new();
        };
        let mut entries = Vec::new();
        let mut oldest_at_ms = None;
        for entry in text
            .lines()
            .rev()
            .filter(|line| !line.trim().is_empty())
            .filter_map(|line| serde_json::from_str::<StoredEntry>(line).ok())
        {
            if entry.seq <= oldest_seq && oldest_at_ms.is_none() {
                oldest_at_ms = Some(entry.at_ms);
            }
            if entry.seq < oldest_seq
                && oldest_at_ms.is_some_and(|at_ms| at_ms - entry.at_ms > window_ms)
            {
                break;
            }
            entries.push(entry);
        }
        entries
    }

    /// Walks the whole chain and, when this process wrote to the log, compares its end with the
    /// newest hash it wrote.
    pub fn check(&self) -> ChainCheck {
        let head = self.head.lock().unwrap_or_else(PoisonError::into_inner);
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => {
                return ChainCheck {
                    intact: false,
                    entries: 0,
                    problem: Some(ChainProblem {
                        kind: "unreadable",
                        line: 0,
                        seq: None,
                        message: format!("The security log could not be read: {error}"),
                    }),
                };
            }
        };
        let mut check = check_text(&text);
        if check.problem.is_none() && head.loaded && head.seq > 0 {
            let last = last_entry(&text);
            let matches = last
                .as_ref()
                .is_some_and(|entry| entry.seq == head.seq && entry.hash == head.hash);
            if !matches {
                check.intact = false;
                check.problem = Some(ChainProblem {
                    kind: "truncated",
                    line: check.entries,
                    seq: last.map(|entry| entry.seq),
                    message: format!(
                        "The log ends before entry {}, the newest one Ghostex wrote: entries were removed or the file was replaced.",
                        head.seq
                    ),
                });
            }
        }
        check
    }

    /// The newest entry's seq and hash, or the genesis values for an empty or missing file. A last
    /// line that does not parse is skipped, so a damaged tail is reported by `check` instead of
    /// stopping the log.
    fn read_tail(&self) -> Result<(u64, String)> {
        let text = match std::fs::read_to_string(&self.path) {
            Ok(text) => text,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => String::new(),
            Err(error) => return Err(error).context("read the security log"),
        };
        Ok(last_entry(&text)
            .map(|entry| (entry.seq, entry.hash))
            .unwrap_or_else(|| (0, GENESIS_HASH.to_string())))
    }

    fn write_line(&self, line: &str) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent).context("create the security log folder")?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let _ = std::fs::set_permissions(parent, std::fs::Permissions::from_mode(DIR_MODE));
            }
        }
        let mut options = std::fs::OpenOptions::new();
        options.create(true).append(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(FILE_MODE);
        }
        let mut file = options.open(&self.path).context("open the security log")?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = file.set_permissions(std::fs::Permissions::from_mode(FILE_MODE));
        }
        file.write_all(format!("{line}\n").as_bytes())
            .context("write the security log")?;
        file.sync_data().context("flush the security log")?;
        Ok(())
    }
}

fn last_entry(text: &str) -> Option<StoredEntry> {
    text.lines()
        .rev()
        .filter(|line| !line.trim().is_empty())
        .find_map(|line| serde_json::from_str::<StoredEntry>(line).ok())
}

/// Walks a log's text from the first line: every entry must parse, carry its own hash, follow the
/// previous entry's hash, and count up by one.
pub fn check_text(text: &str) -> ChainCheck {
    let mut prev_hash = GENESIS_HASH.to_string();
    let mut prev_seq = 0_u64;
    let mut entries = 0_usize;
    for (index, line) in text.lines().enumerate() {
        if line.trim().is_empty() {
            continue;
        }
        let line_number = index + 1;
        let broken = |kind: &'static str, seq: Option<u64>, message: String| ChainCheck {
            intact: false,
            entries,
            problem: Some(ChainProblem {
                kind,
                line: line_number,
                seq,
                message,
            }),
        };
        let entry = match serde_json::from_str::<StoredEntry>(line) {
            Ok(entry) => entry,
            Err(_) => {
                return broken(
                    "unreadable",
                    None,
                    format!("Line {line_number} is not a security log entry."),
                );
            }
        };
        if entry.hash != entry.computed_hash() {
            return broken(
                "edited",
                Some(entry.seq),
                format!("Entry {} was changed after it was written.", entry.seq),
            );
        }
        if entry.prev_hash != prev_hash || entry.seq != prev_seq + 1 {
            return broken(
                "missing",
                Some(entry.seq),
                format!(
                    "The entry before {} was removed or moved (expected entry {}).",
                    entry.seq,
                    prev_seq + 1
                ),
            );
        }
        prev_hash = entry.hash;
        prev_seq = entry.seq;
        entries += 1;
    }
    ChainCheck {
        intact: true,
        entries,
        problem: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn log() -> (tempfile::TempDir, AuditLog) {
        let dir = tempfile::tempdir().expect("tempdir");
        let log = AuditLog::open(AuditLog::default_path(dir.path()));
        (dir, log)
    }

    fn add(log: &AuditLog, n: i64) -> StoredEntry {
        log.append_at(
            1_000 + n,
            &Action::new("pairing.pair", format!("Paired phone {n}")).target(format!("phone-{n}")),
            &Actor::SharedToken {
                claimed_session: Some("G2dyx".into()),
            },
            &Outcome::Done,
            Some(&ApprovalRef {
                id: format!("apr-{n}"),
                signer: Some("MacBook".into()),
            }),
        )
        .expect("append")
    }

    fn lines(log: &AuditLog) -> Vec<String> {
        std::fs::read_to_string(log.path())
            .unwrap()
            .lines()
            .map(str::to_string)
            .collect()
    }

    fn write_lines(log: &AuditLog, lines: &[String]) {
        std::fs::write(log.path(), format!("{}\n", lines.join("\n"))).unwrap();
    }

    #[test]
    fn reading_back_stops_past_the_window_before_the_oldest() {
        let (_dir, log) = log();
        for at_ms in [0, 100, 900, 1_000, 2_000] {
            log.append_at(
                at_ms,
                &Action::new("t.test", "test"),
                &Actor::Ghostex,
                &Outcome::Done,
                None,
            )
            .unwrap();
        }
        let seqs = |entries: Vec<StoredEntry>| entries.iter().map(|e| e.seq).collect::<Vec<_>>();
        // Entry 4 is at 1000: entries at 900 are within 200 ms of it, 100 and 0 are not.
        assert_eq!(seqs(log.read_back_to(4, 200)), [5, 4, 3]);
        assert_eq!(seqs(log.read_back_to(5, 0)), [5]);
        assert_eq!(seqs(log.read_back_to(1, 10_000)), [5, 4, 3, 2, 1]);
    }

    #[test]
    fn entries_chain_each_to_the_one_before() {
        let (_dir, log) = log();
        let first = add(&log, 1);
        let second = add(&log, 2);
        assert_eq!(first.seq, 1);
        assert_eq!(first.prev_hash, GENESIS_HASH);
        assert_eq!(second.seq, 2);
        assert_eq!(second.prev_hash, first.hash);
        assert_eq!(
            log.check(),
            ChainCheck {
                intact: true,
                entries: 2,
                problem: None
            }
        );
        let recent = log.read_recent(10);
        assert_eq!(recent, vec![second, first]);
    }

    #[test]
    fn a_reopened_log_continues_the_chain() {
        let (dir, log) = log();
        let first = add(&log, 1);
        drop(log);
        let reopened = AuditLog::open(AuditLog::default_path(dir.path()));
        let second = add(&reopened, 2);
        assert_eq!(second.seq, 2);
        assert_eq!(second.prev_hash, first.hash);
        assert!(reopened.check().intact);
    }

    #[test]
    fn an_edited_entry_breaks_the_chain() {
        let (_dir, log) = log();
        add(&log, 1);
        add(&log, 2);
        add(&log, 3);
        let mut all = lines(&log);
        all[1] = all[1].replace("Paired phone 2", "Paired phone 9");
        write_lines(&log, &all);
        let check = log.check();
        assert!(!check.intact);
        let problem = check.problem.unwrap();
        assert_eq!(
            (problem.kind, problem.line, problem.seq),
            ("edited", 2, Some(2))
        );
    }

    #[test]
    fn a_rehashed_edit_is_caught_by_the_next_entry() {
        let (_dir, log) = log();
        add(&log, 1);
        add(&log, 2);
        add(&log, 3);
        let mut all = lines(&log);
        let mut entry: StoredEntry = serde_json::from_str(&all[1]).unwrap();
        entry.outcome = "denied".into();
        entry.hash = entry.computed_hash();
        all[1] = serde_json::to_string(&entry).unwrap();
        write_lines(&log, &all);
        let problem = log.check().problem.unwrap();
        assert_eq!((problem.kind, problem.seq), ("missing", Some(3)));
    }

    #[test]
    fn a_removed_entry_breaks_the_chain() {
        let (_dir, log) = log();
        add(&log, 1);
        add(&log, 2);
        add(&log, 3);
        let mut all = lines(&log);
        all.remove(1);
        write_lines(&log, &all);
        let problem = log.check().problem.unwrap();
        assert_eq!((problem.kind, problem.seq), ("missing", Some(3)));
    }

    #[test]
    fn an_added_field_is_refused() {
        let (_dir, log) = log();
        add(&log, 1);
        let mut all = lines(&log);
        all[0] = all[0].replacen('{', "{\"note\":\"x\",", 1);
        write_lines(&log, &all);
        assert_eq!(log.check().problem.unwrap().kind, "unreadable");
    }

    #[test]
    fn removing_the_newest_entries_while_running_is_caught() {
        let (_dir, log) = log();
        add(&log, 1);
        add(&log, 2);
        let mut all = lines(&log);
        all.pop();
        write_lines(&log, &all);
        let problem = log.check().problem.unwrap();
        assert_eq!(problem.kind, "truncated");
        // A file deleted outright is caught the same way.
        std::fs::remove_file(log.path()).unwrap();
        assert_eq!(log.check().problem.unwrap().kind, "truncated");
    }

    #[test]
    fn writing_after_an_edit_keeps_the_break_visible() {
        let (_dir, log) = log();
        add(&log, 1);
        add(&log, 2);
        let mut all = lines(&log);
        all.pop();
        write_lines(&log, &all);
        // The next entry still chains from what this process wrote, so the gap stays.
        add(&log, 3);
        let problem = log.check().problem.unwrap();
        assert_eq!((problem.kind, problem.seq), ("missing", Some(3)));
    }

    #[test]
    fn an_empty_log_is_intact() {
        let (_dir, log) = log();
        assert_eq!(
            log.check(),
            ChainCheck {
                intact: true,
                entries: 0,
                problem: None
            }
        );
        assert!(log.read_recent(5).is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn the_log_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let (_dir, log) = log();
        add(&log, 1);
        let mode = |path: &Path| std::fs::metadata(path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode(log.path()), 0o600);
        assert_eq!(mode(log.path().parent().unwrap()), 0o700);
    }
}
