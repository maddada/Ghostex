//! Empryo fork: a copy of the session folder under a new id, resumed with `empryo --session`.

use std::collections::{BTreeSet, HashSet};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::{json, Map, Value};

use super::*;
use crate::domain::DomainStateError;
use crate::session_chat_empryo_mirror::{is_safe_empryo_session_id, pending_prompts};
use crate::session_git_status::run_git_probe_command;

/// CDXC:SessionFork 2026-10-06 DECISION:
/// "Fork. Gated on a spike that copies the session folder under a new id, cuts `session.jsonl` at the chosen turn, drops `writer.lock` and resumes with `--session`. If it fails, fork stays off for Empryo, with no fallback." The spike passed on Empryo 3.9.0-beta (card agent-bo-95422942): the resumed copy answered from the kept turns only and the original's files stayed byte-identical. Ghostex's Fork has no turn picker, so the copy keeps every finished turn and drops a turn still in flight.
///
/// CDXC:SessionFork 2026-10-06 WHY:
/// Empryo replays history from `session.jsonl` and recomputes `messageRange`, but `--session` reports "Session not found" without a `meta.json`, so both files are written: the log first, then the index whose `id` names the folder. `~/.empryo/threads.db` registers the copy on its first resume and is never written here. `.lock/` and `prompt-prefix-*.json` stay behind; Empryo recreates them. The fork's name goes into Empryo's own records, the way the `/rename` path persists `Fork: <name>` for Claude and Codex: `Fork: ` before Empryo's own session title, kept as a custom title, and before the active tab's label, which Empryo puts in its terminal title and Ghostex shows as the session name (CDXC:SessionFork 2026-09-11).
///
/// CDXC:SessionFork 2026-10-06 WHY:
/// Empryo deletes checkpoint tags in two places: deleting a session runs `git tag -d` on every tag its `meta.json` lists, and closing a tab deletes every `empryo/cp-<first 8 of tab id>` tag. A copy that kept the original's tab ids and tag names would delete the original's checkpoints either way. So the fork gets fresh tab ids and its own copy of each inherited tag (`git tag <new> <old>`, renamed to the new tab's prefix), and every reference in the copied log and index is rewritten; undo in the fork still restores files, and nothing the fork does can name the original's tags.
pub(crate) fn fork_empryo_session_folder(
    project: &Value,
    source_session: &Value,
    fork_id: &str,
) -> Result<EmpryoForkFolder, DomainStateError> {
    let runtime = object_field(source_session, "runtimeSettings");
    let source_id = read_text_from_map(&runtime, "agentSessionId")
        .filter(|id| is_safe_empryo_session_id(id))
        .ok_or_else(|| {
            DomainStateError::bad_request(
                "This Empryo session has no saved conversation to fork yet.",
            )
        })?;
    let source_dir =
        empryo_session_dir(project, source_session, &runtime, &source_id).ok_or_else(|| {
            DomainStateError::bad_request(format!(
                "Empryo's saved conversation {source_id} was not found, so it cannot be forked."
            ))
        })?;
    let fork_dir = source_dir.with_file_name(fork_id);
    let log = fs::read_to_string(source_dir.join("session.jsonl")).map_err(io_error)?;
    let meta: Value = fs::read_to_string(source_dir.join("meta.json"))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .ok_or_else(|| {
            DomainStateError::bad_request("Empryo's session index (meta.json) is unreadable.")
        })?;
    // CDXC:SessionFork 2026-10-07 WHY: a session whose window hosts the repository's shared Empryo engine logs every joined window's tab too, so its log and `meta.json` hold other Ghostex sessions' conversations (seen live 2026-10-07: a second fork of such a session opened on the first fork's conversation, its newest turn). The fork keeps only the tabs the source's chat shows, leaving out the ones `empryo_tab_scan` names as other sessions' (session_chat_empryo_mirror.rs follows the same set).
    let foreign =
        crate::session_chat_empryo_tabs::empryo_tab_scan(&source_dir.join("session.jsonl"))
            .map(|(_, foreign)| foreign)
            .unwrap_or_default();
    let meta = without_foreign_tabs(meta, &foreign);
    let names = ForkNames::of(&meta);
    let (fork_log, mut references) = fork_session_log(&log, fork_id, &names, &foreign).ok_or_else(|| {
        DomainStateError::bad_request(
            "Fork is only available after an Empryo turn has finished. Wait for its reply, then fork it.",
        )
    })?;
    references.add_meta(&meta);
    let renames = references.renames();
    let fork_meta = fork_session_meta(meta, &source_id, fork_id, &names);
    let meta_text = serde_json::to_string_pretty(&fork_meta)
        .map_err(|error| DomainStateError::corrupt_state(error.to_string()))?;
    let (fork_log, meta_text) = (renames.apply(&fork_log), renames.apply(&meta_text));
    let repo =
        read_text_value(&fork_meta, "cwd").or_else(|| read_text_value(source_session, "cwd"));
    create_private_dir(&fork_dir).map_err(io_error)?;
    let copied_tags = repo
        .as_deref()
        .map(|repo| copy_checkpoint_tags(repo, &renames.tag_copies))
        .unwrap_or_default();
    let written = write_private_file(&fork_dir.join("session.jsonl"), fork_log.as_bytes())
        .and_then(|()| write_private_file(&fork_dir.join("meta.json"), meta_text.as_bytes()));
    let folder = EmpryoForkFolder {
        dir: fork_dir,
        repo,
        tags: copied_tags,
    };
    if let Err(error) = written {
        folder.discard();
        return Err(io_error(error));
    }
    Ok(folder)
}

/// What [`fork_empryo_session_folder`] wrote: the fork's folder and the checkpoint tags it copied.
pub(crate) struct EmpryoForkFolder {
    dir: PathBuf,
    repo: Option<String>,
    tags: Vec<String>,
}

impl EmpryoForkFolder {
    /// The fork's `session.jsonl`, recorded on the fork's row as its `agentSessionPath`.
    pub(crate) fn session_log(&self) -> PathBuf {
        self.dir.join("session.jsonl")
    }

    /// Removes the fork's folder and tags, for a fork whose session was never created. The folder
    /// was created by the fork (`create` refuses an existing one) and the tags carry the fork's
    /// fresh tab prefix, so both hold only this fork's state.
    pub(crate) fn discard(self) {
        let _ = fs::remove_dir_all(&self.dir);
        if let Some(repo) = self.repo.as_deref() {
            delete_checkpoint_tags(repo, &self.tags);
        }
    }
}

/// `<root>/.empryo/sessions/<id>`: the folder the hook-reported log sits in, else the one under the
/// session's working folder, which is where `empryo --session` looks.
fn empryo_session_dir(
    project: &Value,
    session: &Value,
    runtime: &Map<String, Value>,
    source_id: &str,
) -> Option<PathBuf> {
    let from_hook = runtime
        .get("agentSessionPath")
        .and_then(Value::as_str)
        .map(Path::new)
        .and_then(Path::parent)
        .map(Path::to_path_buf);
    let from_cwd = read_text_value(session, "cwd")
        .or_else(|| read_text_value(project, "path"))
        .map(|cwd| Path::new(&cwd).join(".empryo/sessions").join(source_id));
    [from_hook, from_cwd].into_iter().flatten().find(|dir| {
        dir.file_name().and_then(|name| name.to_str()) == Some(source_id)
            && dir.join("meta.json").is_file()
    })
}

/// The source log up to its last finished turn, with the header record renamed to the fork and the
/// fork's name appended; every other kept line is copied byte for byte. Turns start and end by the
/// chat mirror's rule (`CDXC:SessionChat` in `session_chat_empryo_mirror.rs`), so the fork keeps the
/// turns the chat shows as finished; anything after the last one that starts another turn is cut,
/// as is a torn last line. `None` when no turn has finished.
fn fork_session_log(
    log: &str,
    fork_id: &str,
    names: &ForkNames,
    foreign_tabs: &HashSet<String>,
) -> Option<(String, ForkReferences)> {
    let lines: Vec<(&str, RecordHead)> = log
        .split_inclusive('\n')
        .take_while(|line| line.ends_with('\n'))
        .map_while(|line| Some((line, serde_json::from_str::<RecordHead>(line).ok()?)))
        .filter(|(_, record)| {
            record
                .tab_id
                .as_ref()
                .is_none_or(|tab| !foreign_tabs.contains(tab))
        })
        .collect();
    let last_reply = lines.iter().rposition(|(_, record)| record.ends_turn())?;
    let end = lines[last_reply + 1..]
        .iter()
        .position(|(_, record)| record.starts_turn())
        .map_or(lines.len(), |offset| last_reply + 1 + offset);
    let (header_line, _) = lines.first().filter(|(_, record)| record.k == "meta")?;
    let mut header: Value = serde_json::from_str(header_line).ok()?;
    header["id"] = json!(fork_id);
    let next_seq = lines[..end]
        .iter()
        .filter_map(|(_, record)| record.seq)
        .max()
        .unwrap_or(0)
        + 1;
    let mut appended = vec![json!({
        "k": "title",
        "title": names.title,
        "custom": true,
        "seq": next_seq,
        "ts": now_ms(),
    })];
    if let Some((tab_id, label)) = &names.active_tab {
        appended.push(json!({
            "k": "tab",
            "tabId": tab_id,
            "patch": { "label": label },
            "seq": next_seq + 1,
            "ts": now_ms(),
        }));
    }
    let mut fork_log = format!("{header}\n");
    let mut references = ForkReferences::default();
    for (line, record) in &lines[1..end] {
        fork_log.push_str(line);
        references.tab_ids.extend(record.tab_id.iter().cloned());
        if let Some(patch) = &record.patch {
            references.add_tags(patch);
        }
    }
    for record in appended {
        fork_log.push_str(&format!("{record}\n"));
    }
    Some((fork_log, references))
}

/// The fields of one `session.jsonl` record the cut reads; the large `ui`/`core` copies are skipped.
#[derive(serde::Deserialize)]
struct RecordHead {
    #[serde(default)]
    k: String,
    seq: Option<u64>,
    status: Option<String>,
    patch: Option<Value>,
    #[serde(rename = "tabId")]
    tab_id: Option<String>,
}

impl RecordHead {
    fn starts_turn(&self) -> bool {
        self.k == "user"
            || (self.k == "tab"
                && self
                    .patch
                    .as_ref()
                    .and_then(pending_prompts)
                    .is_some_and(|pending| !pending.is_empty()))
    }

    fn ends_turn(&self) -> bool {
        self.k == "assistant" && matches!(self.status.as_deref(), Some("complete" | "partial"))
    }
}

const CHECKPOINT_TAG_PREFIX: &str = "empryo/cp-";

/// The tab ids and checkpoint tags a fork copies from its source.
#[derive(Default)]
struct ForkReferences {
    tab_ids: BTreeSet<String>,
    tags: BTreeSet<String>,
}

impl ForkReferences {
    fn add_meta(&mut self, meta: &Value) {
        for tab in meta
            .get("tabs")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            self.tab_ids.extend(read_text_value(tab, "id"));
            self.add_tags(tab);
        }
    }

    /// A tab's `checkpointTags[].gitTag` and `retiredCheckpointTags[]`, in a meta tab or a tab patch.
    fn add_tags(&mut self, tab: &Value) {
        let live = tab.get("checkpointTags").and_then(Value::as_array);
        let retired = tab.get("retiredCheckpointTags").and_then(Value::as_array);
        let names = live
            .into_iter()
            .flatten()
            .filter_map(|checkpoint| checkpoint.get("gitTag"))
            .chain(retired.into_iter().flatten())
            .filter_map(Value::as_str)
            .filter(|name| name.starts_with(CHECKPOINT_TAG_PREFIX));
        self.tags.extend(names.map(str::to_string));
    }

    /// A fresh id per tab, its tag prefix moved with it, and the tag copies that rename implies.
    fn renames(&self) -> ForkRenames {
        let tabs: Vec<(String, String)> = self
            .tab_ids
            .iter()
            .map(|id| (id.clone(), uuid::Uuid::new_v4().to_string()))
            .collect();
        let tag_prefixes = tabs
            .iter()
            .flat_map(|(old, new)| {
                let (old, new) = (checkpoint_tag_prefix(old), checkpoint_tag_prefix(new));
                [".", "-"]
                    .map(|separator| (format!("{old}{separator}"), format!("{new}{separator}")))
            })
            .collect();
        let mut renames = ForkRenames {
            tabs,
            tag_prefixes,
            tag_copies: Vec::new(),
        };
        renames.tag_copies = self
            .tags
            .iter()
            .map(|tag| (tag.clone(), renames.apply(tag)))
            .filter(|(old, new)| old != new)
            .collect();
        renames
    }
}

struct ForkRenames {
    tabs: Vec<(String, String)>,
    tag_prefixes: Vec<(String, String)>,
    tag_copies: Vec<(String, String)>,
}

impl ForkRenames {
    /// One pass per tab id and per tab's tag prefix. Tab ids are UUIDs and a tag prefix carries only
    /// an id's first 8 characters before a `.` or `-`, so neither replacement touches the other's text.
    fn apply(&self, text: &str) -> String {
        self.tabs
            .iter()
            .chain(&self.tag_prefixes)
            .fold(text.to_string(), |text, (old, new)| {
                text.replace(old.as_str(), new)
            })
    }
}

/// `empryo/cp-` and Empryo's tag segment for a tab: its id's word characters, at most 8.
fn checkpoint_tag_prefix(tab_id: &str) -> String {
    let segment: String = tab_id
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || *ch == '_')
        .take(8)
        .collect();
    format!("{CHECKPOINT_TAG_PREFIX}{segment}")
}

/// `git tag <new> <old>` for each inherited checkpoint; a tag the source no longer has is skipped,
/// and undo in the fork then behaves as it would have in the source. Returns the tags created.
fn copy_checkpoint_tags(repo: &str, copies: &[(String, String)]) -> Vec<String> {
    copies
        .iter()
        .filter(|(old, new)| run_git_probe_command(repo, &["tag", "--", new, old]).is_some())
        .map(|(_, new)| new.clone())
        .collect()
}

fn delete_checkpoint_tags(repo: &str, tags: &[String]) {
    if !tags.is_empty() {
        let mut args = vec!["tag", "-d", "--"];
        args.extend(tags.iter().map(String::as_str));
        run_git_probe_command(repo, &args);
    }
}

/// `Fork: ` before Empryo's own session title and before the active tab's label.
struct ForkNames {
    title: String,
    active_tab: Option<(String, String)>,
}

impl ForkNames {
    fn of(meta: &Value) -> Self {
        let title = read_text_value(meta, "title").unwrap_or_else(|| "Empryo session".to_string());
        let active_tab = read_text_value(meta, "activeTabId").and_then(|active| {
            let tabs = meta.get("tabs").and_then(Value::as_array)?;
            let tab = tabs
                .iter()
                .find(|tab| tab.get("id").and_then(Value::as_str) == Some(active.as_str()))?;
            let label = read_text_value(tab, "label")?;
            Some((active, fork_title(&label)))
        });
        Self {
            title: fork_title(&title),
            active_tab,
        }
    }
}

/// `meta` without the tabs in `foreign_tabs`; an active tab among them gives way to the first kept.
fn without_foreign_tabs(mut meta: Value, foreign_tabs: &HashSet<String>) -> Value {
    let Some(tabs) = meta.get_mut("tabs").and_then(Value::as_array_mut) else {
        return meta;
    };
    tabs.retain(|tab| {
        tab.get("id")
            .and_then(Value::as_str)
            .is_none_or(|id| !foreign_tabs.contains(id))
    });
    let first = tabs.first().and_then(|tab| tab.get("id")).cloned();
    let active_is_foreign = meta
        .get("activeTabId")
        .and_then(Value::as_str)
        .is_some_and(|id| foreign_tabs.contains(id));
    if let (true, Some(first)) = (active_is_foreign, first) {
        meta["activeTabId"] = first;
    }
    meta
}

fn fork_session_meta(mut meta: Value, source_id: &str, fork_id: &str, names: &ForkNames) -> Value {
    meta["id"] = json!(fork_id);
    meta["title"] = json!(names.title);
    meta["customTitle"] = json!(names.title);
    if let Some(tabs) = meta.get_mut("tabs").and_then(Value::as_array_mut) {
        for tab in tabs {
            if tab.get("sessionId").and_then(Value::as_str) == Some(source_id) {
                tab["sessionId"] = json!(fork_id);
            }
            if let Some((active, label)) = &names.active_tab {
                if tab.get("id").and_then(Value::as_str) == Some(active.as_str()) {
                    tab["label"] = json!(label);
                }
            }
        }
    }
    meta
}

#[cfg(unix)]
pub(crate) fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    fs::DirBuilder::new().mode(0o700).create(dir)
}

#[cfg(not(unix))]
pub(crate) fn create_private_dir(dir: &Path) -> std::io::Result<()> {
    fs::create_dir(dir)
}

pub(crate) fn write_private_file(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    options.open(path)?.write_all(bytes)
}

fn io_error(error: std::io::Error) -> DomainStateError {
    DomainStateError::bad_request(format!("Empryo fork failed: {error}"))
}
