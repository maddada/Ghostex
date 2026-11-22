//! CDXC:SessionChat 2026-10-28 DECISION:
//! User: "add deepseek harness support for chat view and add it in settings for installing and updating it and all that (like we have for codex for example)". Ghostex runs DeepSeek Harness through its terminal app `dsh-tui` (`@deepseek-harness-tui/dsh-tui`), whose DSH kernel keeps every conversation in the official JSONL session store, `$DSH_HOME/sessions/--<folder>--/<session id>/session.v<N>.jsonl.zstd` (`~/.dsh` by default; checksummed Zstandard frames, one per durable append, or plain lines when compression is off).
//! Chat follows a plain JSONL mirror of that log holding only the events chat reads (the user's prompts, settled assistant messages without their raw stream, tool results, turn starts and ends), so a compressed log is read the way a ZCode database or a Freebuff chat is.
//! SEE-ALSO: server/src/session_chat_decode_dsh.rs reads the mirror rows; server/src/dsh_activity.rs turns the log's turns into working and done.

use serde_json::Value;
use std::{
    collections::HashMap,
    fs,
    hash::{Hash, Hasher},
    io::{self, Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
    time::SystemTime,
};

use ruzstd::decoding::errors::{FrameDecoderError, ReadFrameHeaderError};
use ruzstd::decoding::StreamingDecoder;

/// The events chat reads; everything else in the log (request headers with every tool schema, the
/// system prompt, runtime context, jobs) is left out of the mirror.
const MIRRORED_EVENTS: [&str; 5] = [
    "user/message",
    "assistant/message",
    "tool/result",
    "turn/start",
    "turn/end",
];

/// How long `dsh-tui` may take to start after its session row is created (it boots the DSH kernel
/// and may set its profile up with pnpm on the first run).
const DSH_STARTUP_MS: i64 = 120_000;

/// A mirror's session folder and the (log, size, mtime) it was last mirrored from.
struct Mirror {
    session_dir: PathBuf,
    synced: Option<(PathBuf, u64, SystemTime)>,
}

static MIRRORS: Mutex<Option<HashMap<PathBuf, Mirror>>> = Mutex::new(None);

/// DeepSeek Harness's home: `DSH_HOME` when it is set and not blank (with `~` expanded), else `~/.dsh`.
pub(crate) fn dsh_home() -> PathBuf {
    let home = crate::resume_lookup::home_dir();
    match std::env::var("DSH_HOME")
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
    {
        Some(value) if value == "~" => home,
        Some(value) => match value
            .strip_prefix("~/")
            .or_else(|| value.strip_prefix("~\\"))
        {
            Some(rest) => home.join(rest),
            None => PathBuf::from(value),
        },
        None => home.join(".dsh"),
    }
}

fn sessions_root() -> PathBuf {
    dsh_home().join("sessions")
}

/// Session ids are written to disk as one path segment that keeps `[A-Za-z0-9._-]` literal, so an id
/// of only those characters names its folder as-is; anything else is refused.
pub(crate) fn is_safe_dsh_session_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 128
        && !id.starts_with('.')
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
}

/// The store's folder name for a project path (`projectKey` in DSH's
/// `session-persistence-jsonl/src/format.ts`): separators become one `-`, other unsafe UTF-16 code
/// units `~XXXX`, leading dashes are dropped, and the result is cut to 251 units between `--`.
pub(crate) fn dsh_project_key(cwd: &str) -> String {
    let mut readable = String::new();
    let mut separator_run = false;
    for unit in cwd.encode_utf16() {
        let ch = char::from_u32(u32::from(unit)).filter(|_| unit < 0x80);
        match ch {
            Some('/' | '\\' | ':') => {
                if !separator_run {
                    readable.push('-');
                }
                separator_run = true;
            }
            Some(ch) if ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-') => {
                readable.push(ch);
                separator_run = false;
            }
            _ => {
                readable.push_str(&format!("~{unit:04X}"));
                separator_run = false;
            }
        }
    }
    let slug = readable.trim_start_matches('-');
    let slug = if slug.is_empty() { "root" } else { slug };
    format!("--{}--", &slug[..slug.len().min(251)])
}

/// The generation number of a session log file name (`session.jsonl` is 0, `session.v4.jsonl.zstd`
/// is 4), or `None` for any other file.
fn log_generation(name: &str) -> Option<u32> {
    let stem = name
        .strip_suffix(".jsonl.zstd")
        .or_else(|| name.strip_suffix(".jsonl"))?;
    if stem == "session" {
        return Some(0);
    }
    stem.strip_prefix("session.v")?.parse().ok()
}

/// The newest generation's log in a session folder; DSH reads and appends that one.
pub(crate) fn dsh_session_log(session_dir: &Path) -> Option<PathBuf> {
    fs::read_dir(session_dir)
        .ok()?
        .flatten()
        .filter_map(|entry| {
            let generation = log_generation(entry.file_name().to_str()?)?;
            entry
                .file_type()
                .ok()?
                .is_file()
                .then(|| (generation, entry.path()))
        })
        .max_by_key(|(generation, _)| *generation)
        .map(|(_, path)| path)
}

fn is_compressed(path: &Path) -> bool {
    path.extension().and_then(|ext| ext.to_str()) == Some("zstd")
}

/// Decodes concatenated Zstandard frames in order, passing over skippable ones. DSH appends one
/// frame per batch and a write can be cut off mid-frame, so decoding stops at the first frame that
/// does not decode and keeps what came before it; `max_frames` reads only the leading frames.
fn decode_frames(bytes: &[u8], max_frames: Option<usize>) -> Vec<u8> {
    let mut input = bytes;
    let mut output = Vec::new();
    let mut frames = 0;
    while !input.is_empty() && max_frames.is_none_or(|max| frames < max) {
        let mut frame = Vec::new();
        let decoded = match StreamingDecoder::new(&mut input) {
            Ok(mut decoder) => decoder.read_to_end(&mut frame).map(|_| true),
            Err(FrameDecoderError::ReadFrameHeaderError(ReadFrameHeaderError::SkipFrame {
                length,
                ..
            })) => {
                let skip = (length as usize).min(input.len());
                input = &input[skip..];
                Ok(false)
            }
            Err(error) => Err(io::Error::new(io::ErrorKind::InvalidData, error)),
        };
        match decoded {
            Ok(true) => {
                output.extend_from_slice(&frame);
                frames += 1;
            }
            Ok(false) => {}
            Err(_) => break,
        }
    }
    output
}

/// The log's complete lines: a torn last line (a write in progress) is left for the next read.
fn complete_lines(bytes: &[u8]) -> String {
    let end = bytes
        .iter()
        .rposition(|byte| *byte == b'\n')
        .map_or(0, |index| index + 1);
    String::from_utf8_lossy(&bytes[..end]).into_owned()
}

pub(crate) fn read_dsh_log(path: &Path) -> Option<String> {
    let bytes = fs::read(path).ok()?;
    Some(if is_compressed(path) {
        complete_lines(&decode_frames(&bytes, None))
    } else {
        complete_lines(&bytes)
    })
}

/// What a session log's header line says about it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct DshHeader {
    pub id: String,
    pub created_at: i64,
    pub cwd: Option<String>,
    /// A subagent's or other child session's log, never a terminal's own conversation.
    pub child: bool,
}

fn parse_header(line: &str) -> Option<DshHeader> {
    let value: Value = serde_json::from_str(line).ok()?;
    let id = value.get("id")?.as_str()?.to_string();
    Some(DshHeader {
        id,
        created_at: value.get("createdAt")?.as_i64()?,
        cwd: value.get("cwd").and_then(Value::as_str).map(str::to_string),
        child: value.get("origin").and_then(Value::as_str) == Some("subagent")
            || value
                .get("parentSession")
                .is_some_and(|parent| !parent.is_null()),
    })
}

/// The header of a log, reading only its first frame (or line).
pub(crate) fn read_dsh_header(path: &Path) -> Option<DshHeader> {
    let text = if is_compressed(path) {
        let mut bytes = Vec::new();
        fs::File::open(path)
            .ok()?
            .take(256 * 1024)
            .read_to_end(&mut bytes)
            .ok()?;
        String::from_utf8_lossy(&decode_frames(&bytes, Some(1))).into_owned()
    } else {
        let mut bytes = Vec::new();
        fs::File::open(path)
            .ok()?
            .take(64 * 1024)
            .read_to_end(&mut bytes)
            .ok()?;
        String::from_utf8_lossy(&bytes).into_owned()
    };
    parse_header(text.lines().next()?)
}

/// The session folder of `id`: the folder of `supplied` when it is that session's log, else the
/// newest folder named `id` in any project of the store.
fn session_dir_for(id: &str, supplied: Option<&Path>) -> Option<PathBuf> {
    if let Some(dir) = supplied
        .and_then(Path::parent)
        .filter(|dir| dir.file_name().and_then(|name| name.to_str()) == Some(id))
        .filter(|dir| dsh_session_log(dir).is_some())
    {
        return Some(dir.to_path_buf());
    }
    fs::read_dir(sessions_root())
        .ok()?
        .flatten()
        .map(|project| project.path().join(id))
        .filter(|dir| dsh_session_log(dir).is_some())
        .max_by_key(|dir| {
            fs::metadata(dir)
                .and_then(|metadata| metadata.modified())
                .ok()
        })
}

/// The log a session's chat reads, for the activity observer.
pub(crate) fn dsh_session_log_for(id: &str, supplied: Option<&Path>) -> Option<PathBuf> {
    is_safe_dsh_session_id(id)
        .then(|| session_dir_for(id, supplied))
        .flatten()
        .and_then(|dir| dsh_session_log(&dir))
}

pub(crate) fn resolve_dsh_session_transcript_path(
    id: &str,
    supplied: Option<&Path>,
) -> Option<PathBuf> {
    if !is_safe_dsh_session_id(id) {
        return None;
    }
    let session_dir = session_dir_for(id, supplied)?;
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    session_dir.hash(&mut hasher);
    let path = ghostex_paths::GhostexPaths::resolve()
        .gxserver_state_dir()
        .join("dsh-session-mirror")
        .join(format!("{:016x}", hasher.finish()))
        .join(format!("{id}.jsonl"));
    let mut guard = MIRRORS.lock().ok()?;
    let synced = sync_mirror(&session_dir, &path, None)?;
    guard.get_or_insert_with(HashMap::new).insert(
        path.clone(),
        Mirror {
            session_dir,
            synced: Some(synced),
        },
    );
    Some(path)
}

pub(crate) fn sync_dsh_transcript_mirror_for_path(path: &Path) {
    let Ok(mut guard) = MIRRORS.lock() else {
        return;
    };
    let Some(mirror) = guard.as_mut().and_then(|map| map.get_mut(path)) else {
        return;
    };
    if let Some(synced) = sync_mirror(&mirror.session_dir, path, mirror.synced.as_ref()) {
        mirror.synced = Some(synced);
    }
}

/// The mirror rows of a log's text: the chat events, assistant messages without their stream.
pub(crate) fn dsh_mirror_records(log: &str) -> String {
    let mut content = String::new();
    for line in log.lines() {
        let Ok(mut record) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let Some(kind) = record.get("type").and_then(Value::as_str) else {
            continue;
        };
        if !MIRRORED_EVENTS.contains(&kind) {
            continue;
        }
        // Only the person's own prompts; DSH also records runtime context, time, skills and goal
        // rounds as user-role messages the model sees.
        if kind == "user/message"
            && record.pointer("/data/source/kind").and_then(Value::as_str) != Some("user")
        {
            continue;
        }
        if let Some(data) = record.get_mut("data").and_then(Value::as_object_mut) {
            data.remove("stream");
        }
        if let Some(thinking) = reasoning_row(&record) {
            content.push_str(&thinking.to_string());
            content.push('\n');
        }
        content.push_str(&record.to_string());
        content.push('\n');
    }
    content
}

/// CDXC:SessionChat 2026-10-28 WHY:
/// DSH records a step's thinking, reply and tool calls as one `assistant/message`, as Pi does, and a chat row has one role, so the thinking of every step that also said or did something was lost. A step with thinking and anything else gets a `dsh/reasoning` row of its thinking ahead of it, stamped a millisecond earlier, as Pi's entries are split (`decode_pi_transcript_entry`).
fn reasoning_row(record: &Value) -> Option<Value> {
    if record.get("type").and_then(Value::as_str) != Some("assistant/message") {
        return None;
    }
    let message = record.pointer("/data/message")?;
    let parts = message.get("content")?.as_array()?;
    let has_text = |part: &Value| {
        part.get("text")
            .and_then(Value::as_str)
            .is_some_and(|text| !text.trim().is_empty())
    };
    let reasoning: Vec<Value> = parts
        .iter()
        .filter(|part| part.get("type").and_then(Value::as_str) == Some("reasoning"))
        .filter(|part| has_text(part))
        .cloned()
        .collect();
    let says_or_does = parts
        .iter()
        .any(|part| match part.get("type").and_then(Value::as_str) {
            Some("text") => has_text(part),
            Some("tool-call") => true,
            _ => false,
        });
    if reasoning.is_empty() || !says_or_does {
        return None;
    }
    let id = message.get("id").and_then(Value::as_str)?;
    Some(serde_json::json!({
        "type": "dsh/reasoning",
        "time": record.get("time").and_then(Value::as_i64).map(|time| time - 1),
        "data": {
            "turn": record.pointer("/data/turn"),
            "message": {"id": format!("{id}:thinking"), "content": reasoning},
        },
    }))
}

/// Rewrites the mirror when the session's log changed; answers the (log, size, mtime) it read.
fn sync_mirror(
    session_dir: &Path,
    path: &Path,
    previous: Option<&(PathBuf, u64, SystemTime)>,
) -> Option<(PathBuf, u64, SystemTime)> {
    let log = dsh_session_log(session_dir)?;
    let metadata = fs::metadata(&log).ok()?;
    let stamp = (log.clone(), metadata.len(), metadata.modified().ok()?);
    if previous == Some(&stamp) && path.is_file() {
        return Some(stamp);
    }
    let content = dsh_mirror_records(&read_dsh_log(&log)?);
    let existing = fs::read_to_string(path).ok();
    if existing.as_deref() == Some(content.as_str()) {
        return Some(stamp);
    }
    fs::create_dir_all(path.parent()?).ok()?;
    if let Some(existing) = existing.filter(|existing| content.starts_with(existing.as_str())) {
        fs::OpenOptions::new()
            .append(true)
            .open(path)
            .ok()?
            .write_all(content[existing.len()..].as_bytes())
            .ok()?;
    } else {
        let temp = path.with_extension(format!("{}.tmp", uuid::Uuid::new_v4()));
        fs::write(&temp, content).ok()?;
        if fs::rename(&temp, path).is_err() {
            let _ = fs::remove_file(temp);
            return None;
        }
    }
    Some(stamp)
}

/// Whether the log's newest turn is still running: a `turn/start` with no `turn/end` after it.
/// `None` when the log has no turn yet.
pub(crate) fn dsh_turn_running(log: &str) -> Option<bool> {
    log.lines().rev().find_map(|line| {
        if !line.contains("\"turn/") {
            return None;
        }
        let record: Value = serde_json::from_str(line).ok()?;
        match record.get("type")?.as_str()? {
            "turn/start" => Some(true),
            "turn/end" => Some(false),
            _ => None,
        }
    })
}

/// CDXC:SessionIdentity 2026-10-28 WHY:
/// dsh-tui has no hooks and picks its session id itself (`--resume <id>` only reopens one), and DSH writes a session's log only with its first message, so a new Ghostex session learns its conversation from the store: the oldest top-level session of this session's folder that was created after the session row, that no other live session owns, and that no other live DeepSeek Harness session of the same folder could have started. The log header's own `cwd` and `createdAt` are the evidence. When two sessions could both own a log, neither takes it and the conversation stays in the terminal.
pub(crate) fn discover_dsh_session_for_session(
    repository: &crate::domain::DomainRepository<'_>,
    session: &Value,
    sessions: &[Value],
) -> Option<(String, PathBuf)> {
    let (cwd, created_ms) =
        crate::session_chat_freebuff::freebuff_session_context(repository, session)?;
    let key = |row: &Value| (row.get("projectId").cloned(), row.get("sessionId").cloned());
    let mut claimed = Vec::new();
    let mut rivals = Vec::new();
    for other in sessions {
        if key(other) == key(session) || !crate::agents::is_active_identity_owner(other) {
            continue;
        }
        let bound = crate::presentation::util::read_runtime_text(other, "agentSessionId");
        if other.get("agentId").and_then(Value::as_str) == Some("dsh") {
            if let Some((other_cwd, other_ms)) =
                crate::session_chat_freebuff::freebuff_session_context(repository, other)
            {
                if other_cwd == cwd {
                    rivals.push((other_ms, bound.is_some()));
                }
            }
        }
        claimed.extend(bound);
    }
    discover_dsh_session(&sessions_root(), &cwd, created_ms, &rivals, &claimed)
}

/// `rivals` are the other live DeepSeek Harness sessions of the same folder: (created at, has a log).
fn discover_dsh_session(
    root: &Path,
    cwd: &Path,
    since_ms: i64,
    rivals: &[(i64, bool)],
    claimed: &[String],
) -> Option<(String, PathBuf)> {
    let cwd_text = cwd.to_str()?;
    // macOS reports some folders through a symlink (`/tmp` is `/private/tmp`); the store keys the
    // folder the way the process saw it.
    let canonical = fs::canonicalize(cwd)
        .ok()
        .and_then(|path| path.to_str().map(str::to_string));
    let mut keys = vec![cwd_text.to_string()];
    keys.extend(canonical.filter(|path| path != cwd_text));
    // The session is created a moment after the TUI starts its kernel.
    let could_start = |created_ms: i64, started: i64| created_ms <= started + 2_000;
    keys.iter()
        .filter_map(|key| fs::read_dir(root.join(dsh_project_key(key))).ok())
        .flat_map(|entries| entries.flatten())
        .filter_map(|entry| {
            let log = dsh_session_log(&entry.path())?;
            let header = read_dsh_header(&log)?;
            let same_folder = header
                .cwd
                .as_deref()
                .is_some_and(|header_cwd| keys.iter().any(|key| key == header_cwd));
            let started = header.created_at;
            let contested = rivals.iter().any(|&(rival_ms, rival_bound)| {
                could_start(rival_ms, started)
                    && (rival_ms > since_ms
                        || (!rival_bound && rival_ms + DSH_STARTUP_MS >= started))
            });
            (is_safe_dsh_session_id(&header.id)
                && !header.child
                && same_folder
                && could_start(since_ms, started)
                && !contested
                && !claimed.contains(&header.id))
            .then_some((started, header.id, log))
        })
        .min()
        .map(|(_, id, log)| (id, log))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A real DSH 0.2.1 session log (the harness's own `snapshots/web/fresh-round-trip` recording:
    /// one bash call, then a one-word reply), with its system prompt and stream shortened.
    pub(crate) const RECORDED_LOG: &str = include_str!("../testdata/dsh/fresh-round-trip.v4.jsonl");

    fn temp_dir(name: &str) -> PathBuf {
        let dir =
            std::env::temp_dir().join(format!("ghostex-dsh-test-{name}-{}", uuid::Uuid::new_v4()));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    fn compress_frames(lines: &[&str]) -> Vec<u8> {
        let mut bytes = Vec::new();
        for chunk in lines {
            bytes.extend(ruzstd::encoding::compress_to_vec(
                chunk.as_bytes(),
                ruzstd::encoding::CompressionLevel::Fastest,
            ));
        }
        bytes
    }

    #[test]
    fn project_keys_match_the_store_folder_names() {
        assert_eq!(
            dsh_project_key("/Users/me/dev/My App"),
            "--Users-me-dev-My~0020App--"
        );
        assert_eq!(dsh_project_key("C:\\work\\x"), "--C-work-x--");
        assert_eq!(dsh_project_key("/"), "--root--");
        assert_eq!(dsh_project_key("/a~b/é"), "--a~007Eb-~00E9--");
    }

    #[test]
    fn the_newest_generation_is_the_session_log() {
        let dir = temp_dir("generation");
        for name in [
            "session.jsonl",
            "session.v3.jsonl.zstd",
            "session.v4.jsonl.zstd",
            "session.lock",
        ] {
            fs::write(dir.join(name), b"").unwrap();
        }
        assert_eq!(
            dsh_session_log(&dir).unwrap().file_name().unwrap(),
            "session.v4.jsonl.zstd"
        );
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn compressed_logs_read_every_complete_frame_and_drop_a_torn_one() {
        let dir = temp_dir("zstd");
        let path = dir.join("session.v4.jsonl.zstd");
        let lines: Vec<String> = RECORDED_LOG
            .lines()
            .map(|line| format!("{line}\n"))
            .collect();
        let (head, rest) = lines.split_at(1);
        let mut bytes = compress_frames(&[&head[0]]);
        bytes.extend(compress_frames(&[&rest.concat()]));
        // A batch cut off mid-write.
        let torn = compress_frames(&["{\"type\":\"turn/start\",\"data\":{\"turn\":2}}\n"]);
        bytes.extend_from_slice(&torn[..torn.len() / 2]);
        fs::write(&path, &bytes).unwrap();
        assert_eq!(read_dsh_log(&path).unwrap(), RECORDED_LOG);
        let header = read_dsh_header(&path).unwrap();
        assert_eq!(header.id, "session-7f3c1d2e");
        assert_eq!(header.created_at, 1_788_240_666_255);
        assert!(!header.child);
        fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn the_mirror_keeps_only_chat_events_and_drops_streams() {
        let mirror = dsh_mirror_records(RECORDED_LOG);
        let types: Vec<String> = mirror
            .lines()
            .map(|line| {
                serde_json::from_str::<Value>(line).unwrap()["type"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect();
        assert_eq!(
            types,
            [
                "turn/start",
                "user/message",
                "dsh/reasoning",
                "assistant/message",
                "tool/result",
                "dsh/reasoning",
                "assistant/message",
                "turn/end"
            ]
        );
        assert!(!mirror.contains("\"stream\""));
        assert!(!mirror.contains("Current runtime context"));
    }

    #[test]
    fn a_turn_runs_until_its_end_is_logged() {
        let lines: Vec<&str> = RECORDED_LOG.lines().collect();
        let until_tool = lines
            .iter()
            .position(|line| line.contains("\"tool/result\""))
            .unwrap();
        assert_eq!(
            dsh_turn_running(&lines[..until_tool].join("\n")),
            Some(true)
        );
        assert_eq!(dsh_turn_running(RECORDED_LOG), Some(false));
        assert_eq!(dsh_turn_running(lines[0]), None);
    }

    #[test]
    fn discovery_takes_the_oldest_unclaimed_session_started_after_the_row() {
        let root = temp_dir("discover");
        let cwd = temp_dir("project");
        let cwd = fs::canonicalize(cwd).unwrap();
        let cwd_text = cwd.to_str().unwrap();
        let project = root.join(dsh_project_key(cwd_text));
        let write = |id: &str, created: i64, extra: &str| {
            let dir = project.join(id);
            fs::create_dir_all(&dir).unwrap();
            fs::write(
                dir.join("session.v4.jsonl"),
                format!(
                    "{{\"type\":\"session\",\"version\":4,\"id\":\"{id}\",\"createdAt\":{created},\"cwd\":{},\"isSeeded\":false{extra}}}\n",
                    serde_json::to_string(cwd_text).unwrap()
                ),
            )
            .unwrap();
        };
        write("session-old", 1_000, "");
        write("session-a", 10_500, "");
        write("session-b", 20_000, "");
        write(
            "session-child",
            10_100,
            ",\"origin\":\"subagent\",\"parentSession\":\"session-a\"",
        );
        let found = discover_dsh_session(&root, &cwd, 10_000, &[], &[]).unwrap();
        assert_eq!(found.0, "session-a");
        assert!(found.1.ends_with("session-a/session.v4.jsonl"));
        let claimed = vec!["session-a".to_string()];
        assert_eq!(
            discover_dsh_session(&root, &cwd, 10_000, &[], &claimed)
                .unwrap()
                .0,
            "session-b"
        );
        // Another live session of the folder started before this log and has none yet.
        assert!(discover_dsh_session(&root, &cwd, 10_000, &[(10_200, false)], &[]).is_none());
        fs::remove_dir_all(root).unwrap();
        fs::remove_dir_all(cwd).unwrap();
    }

    #[test]
    fn only_plain_segment_ids_are_accepted() {
        assert!(is_safe_dsh_session_id("session-7f3c1d2e"));
        assert!(is_safe_dsh_session_id(
            "0a5c2f1e-1b2c-4d5e-8f90-123456789abc"
        ));
        assert!(!is_safe_dsh_session_id("../x"));
        assert!(!is_safe_dsh_session_id(".hidden"));
        assert!(!is_safe_dsh_session_id("a b"));
        assert!(!is_safe_dsh_session_id(""));
    }
}
