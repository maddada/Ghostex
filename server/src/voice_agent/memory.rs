//! The voice agent's memory of the last conversation: while a call runs, its final captions, tool
//! lines and Ghostex updates are kept in a pending file; when it ends (any way, or at the next start
//! after a crash or restart) a small text model on the same provider and key writes a ~120-word
//! summary, which replaces the previous one and is kept until the next call replaces it or the user
//! forgets it. The raw captions are deleted once the summary exists. A call can hand the summary to
//! the agent as a `[Ghostex memory]` message (Continue).
//!
//! CDXC:VoiceMode 2026-10-20 DECISION:
//! User's answers (voice agent memory): continuing is offered by a Continue / Start fresh card inside the live call, and saying "pick up where we left off" does the same; on Continue the agent says a one-sentence recap; the summary is made by a small text model on the same provider and key the call used; only one summary is kept, "until replaced" (no expiry); it can be forgotten from the voice panel and from Settings. If the text model call fails (for example a key limited to voice) no summary is kept and the reason is logged.
//! CDXC:VoiceMode 2026-10-20 WHY:
//! The summary is made from gxserver's own captions after the call, not by the voice model before hang-up: it works the same after Hang up, the idle hang-up, a dropped provider connection or a restart (the pending file waits on disk), costs a fraction of a cent instead of voice-rate audio tokens, and xAI documents no silent text reply.

use std::fs;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use super::provider::Provider;
use super::{keys, registry};
use crate::paths::GxserverPaths;
use crate::server::AppState;

/// The prefix of the message Continue adds to the conversation, beside `[Ghostex update]`.
const MEMORY_PREFIX: &str = "[Ghostex memory]";
/// Transcript kept for the summary; a longer call keeps its latest part.
const MAX_LOG_CHARS: usize = 40_000;
const MAX_SUMMARY_CHARS: usize = 1_200;
const MAX_GIST_CHARS: usize = 140;
const XAI_CHAT_URL: &str = "https://api.x.ai/v1/chat/completions";
const OPENAI_CHAT_URL: &str = "https://api.openai.com/v1/chat/completions";
/// Small, fast text models on each provider (2026-10-20 price lists: xAI $1.25 / $2.50, OpenAI
/// $0.20 / $1.25 per million tokens); a summary reads about 2,000 tokens and writes about 200.
const XAI_SUMMARY_MODEL: &str = "grok-4.20-0309-non-reasoning";
const OPENAI_SUMMARY_MODEL: &str = "gpt-5.4-nano";
/// Waits before each retry of a summary the provider could not make right now.
const RETRY_DELAYS: [Duration; 3] = [
    Duration::from_secs(5),
    Duration::from_secs(30),
    Duration::from_secs(120),
];

/// Bumped by Forget, so a summary being made when the user forgets is not saved.
static FORGET_GENERATION: AtomicU64 = AtomicU64::new(0);

fn summarizing() -> &'static tokio::sync::Mutex<()> {
    static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| tokio::sync::Mutex::new(()))
}

fn memory_dir(paths: &GxserverPaths) -> PathBuf {
    paths.root_dir.join("voice-agent")
}

fn memory_path(paths: &GxserverPaths) -> PathBuf {
    memory_dir(paths).join("last-conversation.json")
}

fn pending_dir(paths: &GxserverPaths) -> PathBuf {
    memory_dir(paths).join("pending")
}

/// Where a call's transcript waits for its summary (tests check what a call recorded).
#[cfg(test)]
pub(super) fn pending_path(paths: &GxserverPaths, conversation_id: &str) -> PathBuf {
    pending_dir(paths).join(format!("{conversation_id}.json"))
}

/// The remembered conversation.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Memory {
    pub(super) summary: String,
    pub(super) gist: String,
    pub(super) ended_at: chrono::DateTime<chrono::Utc>,
    pub(super) provider: String,
}

impl Memory {
    /// What clients get: the card shows `gist` and how long ago `endedAt` was.
    pub(super) fn to_client(&self) -> Value {
        json!({
            "endedAt": self.ended_at.to_rfc3339_opts(chrono::SecondsFormat::Secs, true),
            "gist": self.gist,
            "summary": self.summary,
            "provider": self.provider,
        })
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
struct LogLine {
    who: String,
    text: String,
}

/// A call's transcript as it is written to the pending file.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Pending {
    conversation_id: String,
    provider: String,
    updated_at: Option<chrono::DateTime<chrono::Utc>>,
    /// The call was handed the previous summary, which the new one then carries forward.
    continued: bool,
    user_turns: u64,
    lines: Vec<LogLine>,
}

/// The running call's transcript, mirrored to its pending file as it grows so a crash or restart
/// loses nothing.
#[derive(Default)]
pub(super) struct CallLog {
    path: Option<PathBuf>,
    pending: Pending,
    chars: usize,
}

impl CallLog {
    pub(super) fn new(paths: &GxserverPaths, conversation_id: &str, provider: Provider) -> Self {
        Self {
            path: Some(pending_dir(paths).join(format!("{conversation_id}.json"))),
            pending: Pending {
                conversation_id: conversation_id.to_string(),
                provider: provider.id().to_string(),
                ..Pending::default()
            },
            chars: 0,
        }
    }

    pub(super) fn continued(&self) -> bool {
        self.pending.continued
    }

    pub(super) fn set_continued(&mut self, continued: bool) {
        if self.pending.continued != continued {
            self.pending.continued = continued;
            self.save();
        }
    }

    /// Records a frame sent to the client: final captions, finished tool calls and announcements.
    pub(super) fn record(&mut self, frame: &Value) {
        let field = |key: &str| {
            frame
                .get(key)
                .and_then(Value::as_str)
                .unwrap_or_default()
                .trim()
                .to_string()
        };
        let (who, text) = match field("type").as_str() {
            "caption" if frame.get("final").and_then(Value::as_bool) == Some(true) => {
                let who = if field("role") == "user" {
                    "User"
                } else {
                    "Assistant"
                };
                (who, field("text"))
            }
            "tool" if field("phase") == "finished" => {
                let ok = frame.get("ok").and_then(Value::as_bool) == Some(true);
                let label = field("label");
                let label = if label.is_empty() {
                    field("name")
                } else {
                    label
                };
                (
                    "Tool",
                    format!("{label} ({})", if ok { "done" } else { "failed" }),
                )
            }
            "announcement" => ("Ghostex update", field("text")),
            _ => return,
        };
        if text.is_empty() {
            return;
        }
        if who == "User" {
            self.pending.user_turns += 1;
        }
        self.chars += text.len();
        self.pending.lines.push(LogLine {
            who: who.to_string(),
            text,
        });
        while self.chars > MAX_LOG_CHARS && self.pending.lines.len() > 1 {
            let dropped = self.pending.lines.remove(0);
            self.chars -= dropped.text.len();
        }
        self.save();
    }

    fn save(&mut self) {
        let Some(path) = &self.path else { return };
        if self.pending.user_turns == 0 {
            return;
        }
        self.pending.updated_at = Some(chrono::Utc::now());
        if let Ok(bytes) = serde_json::to_vec(&self.pending) {
            let _ = write_private(path, &bytes);
        }
    }
}

/// Writes a 0600 file through a temporary file renamed over the old one.
fn write_private(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let temp = path.with_extension("json.tmp");
    let _ = fs::remove_file(&temp);
    {
        let mut options = fs::OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
    }
    fs::rename(&temp, path)
}

pub(super) fn read(paths: &GxserverPaths) -> Option<Memory> {
    let text = fs::read_to_string(memory_path(paths)).ok()?;
    serde_json::from_str(&text).ok()
}

/// The remembered conversation for clients, or `null`.
pub(super) fn client_view(paths: &GxserverPaths) -> Value {
    read(paths).map_or(Value::Null, |memory| memory.to_client())
}

fn pending_files(paths: &GxserverPaths) -> Vec<PathBuf> {
    fs::read_dir(pending_dir(paths))
        .map(|entries| {
            entries
                .filter_map(Result::ok)
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
                .collect()
        })
        .unwrap_or_default()
}

fn conversation_id_of(path: &Path) -> String {
    path.file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_default()
}

/// A summary is still being made for a call that ended.
pub(super) fn summary_pending(paths: &GxserverPaths) -> bool {
    pending_files(paths)
        .iter()
        .any(|path| registry::sender(&conversation_id_of(path)).is_none())
}

/// Forgets the remembered conversation and the summaries still being made for ended calls, and
/// tells the open conversations (their next summary then starts fresh).
pub(super) fn forget(state: &AppState) {
    FORGET_GENERATION.fetch_add(1, Ordering::SeqCst);
    let paths = &state.paths;
    let _ = fs::remove_file(memory_path(paths));
    for path in pending_files(paths) {
        if registry::sender(&conversation_id_of(&path)).is_none() {
            let _ = fs::remove_file(path);
        }
    }
    for sender in registry::senders() {
        let _ = sender.send(super::conversation::Input::MemoryForgotten);
    }
}

/// How long ago, in words the agent can say.
fn ago(ended_at: chrono::DateTime<chrono::Utc>) -> String {
    let minutes = (chrono::Utc::now() - ended_at).num_minutes().max(0);
    match minutes {
        0..=1 => "a minute ago".to_string(),
        2..=59 => format!("{minutes} minutes ago"),
        60..=119 => "an hour ago".to_string(),
        120..=1439 => format!("{} hours ago", minutes / 60),
        1440..=2879 => "yesterday".to_string(),
        _ => format!("{} days ago", minutes / 1440),
    }
}

/// The message Continue adds to the conversation.
pub(super) fn continue_message(memory: &Memory) -> String {
    format!(
        "{MEMORY_PREFIX} The user wants to continue your previous voice conversation, which ended {}. Your notes from it: {}\nIn one short sentence, say where things stood, then ask what they want to do next. Sessions may have changed since then, so check with your tools before saying what a session is doing now.",
        ago(memory.ended_at),
        memory.summary
    )
}

/// The `recall_last_conversation` tool ("pick up where we left off"): the same notes Continue
/// hands over.
pub(super) fn recall(paths: &GxserverPaths) -> Result<Value, String> {
    let memory = read(paths).ok_or_else(|| {
        "Nothing is remembered from a previous voice conversation, so there is nothing to pick up."
            .to_string()
    })?;
    Ok(json!({
        "ok": true,
        "ended": ago(memory.ended_at),
        "notes": memory.summary,
        "reminder": "Say in one short sentence where things stood. Sessions may have changed since then, so check with your tools before saying what a session is doing now.",
    }))
}

/// The call ended: makes its summary in the background. A call where the user said nothing keeps
/// the previous memory.
pub(super) fn call_ended(state: Arc<AppState>, mut log: CallLog) {
    let Some(path) = log.path.take() else { return };
    if log.pending.user_turns == 0 {
        let _ = fs::remove_file(&path);
        return;
    }
    log.path = Some(path.clone());
    log.save();
    tokio::spawn(async move { summarize(state, path).await });
}

/// Makes the summary of the call that was still pending when gxserver last stopped (a restart
/// during or right after a call). Only the newest one counts; older ones are dropped.
///
/// While the voice agent is turned off nothing is summarized; the pending call stays on disk for
/// a start with it on (`feature.rs`).
pub(crate) fn recover_after_restart(state: &Arc<AppState>) {
    if !super::feature::voice_agent_on(&state.paths) {
        return;
    }
    let mut pending: Vec<(std::time::SystemTime, PathBuf)> = pending_files(&state.paths)
        .into_iter()
        .map(|path| {
            let modified = fs::metadata(&path)
                .and_then(|meta| meta.modified())
                .unwrap_or(std::time::UNIX_EPOCH);
            (modified, path)
        })
        .collect();
    pending.sort();
    let Some((_, newest)) = pending.pop() else {
        return;
    };
    for (_, older) in pending {
        let _ = fs::remove_file(older);
    }
    let state = state.clone();
    tokio::spawn(async move { summarize(state, newest).await });
}

enum Failure {
    /// Worth trying again later (network, rate limit, provider trouble).
    Retry(String),
    /// Will not work with this key or input.
    Fail(String),
}

async fn summarize(state: Arc<AppState>, path: PathBuf) {
    let _turn = summarizing().lock().await;
    let generation = FORGET_GENERATION.load(Ordering::SeqCst);
    let Some(pending) = fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str::<Pending>(&text).ok())
    else {
        let _ = fs::remove_file(&path);
        return;
    };
    let Some(provider) = Provider::from_id(&pending.provider) else {
        let _ = fs::remove_file(&path);
        return;
    };
    let fail = |message: String| {
        super::warn_error(
            &state,
            "voiceAgent.memoryFailed",
            &message,
            json!({"provider": provider.id()}),
        );
    };
    let key = match keys::read_key_async(&state.paths, provider).await {
        Ok(Some(key)) => key,
        Ok(None) => {
            fail("No API key is saved for the provider any more.".to_string());
            let _ = fs::remove_file(&path);
            return;
        }
        Err(message) => {
            fail(message);
            let _ = fs::remove_file(&path);
            return;
        }
    };
    let previous = pending
        .continued
        .then(|| read(&state.paths))
        .flatten()
        .map(|memory| memory.summary);
    let messages = summary_messages(&pending, previous.as_deref());
    let mut attempt = 0;
    let outcome = loop {
        let request_messages = messages.clone();
        let request_key = key.clone();
        let result = tokio::task::spawn_blocking(move || {
            request_summary(provider, &request_key, request_messages)
        })
        .await
        .unwrap_or_else(|error| Err(Failure::Fail(error.to_string())));
        match result {
            Ok(summary) => break Ok(summary),
            Err(Failure::Retry(_)) if attempt < RETRY_DELAYS.len() => {
                tokio::time::sleep(RETRY_DELAYS[attempt]).await;
                attempt += 1;
            }
            Err(failure) => break Err(failure),
        }
    };
    if FORGET_GENERATION.load(Ordering::SeqCst) != generation {
        let _ = fs::remove_file(&path);
        return;
    }
    match outcome {
        Ok((gist, summary)) => {
            let memory = Memory {
                summary,
                gist,
                ended_at: pending.updated_at.unwrap_or_else(chrono::Utc::now),
                provider: provider.id().to_string(),
            };
            match serde_json::to_vec_pretty(&memory)
                .map_err(|error| error.to_string())
                .and_then(|bytes| {
                    write_private(&memory_path(&state.paths), &bytes)
                        .map_err(|error| error.to_string())
                }) {
                Ok(()) => {
                    let _ = fs::remove_file(&path);
                }
                Err(message) => fail(format!("Couldn't save the summary: {message}")),
            }
        }
        // The pending file stays for the next start.
        Err(Failure::Retry(message)) => fail(format!("{message} Trying again at the next start.")),
        Err(Failure::Fail(message)) => {
            fail(message);
            let _ = fs::remove_file(&path);
        }
    }
}

fn summary_messages(pending: &Pending, previous: Option<&str>) -> Value {
    let mut system = String::from(
        "You write the notes a voice assistant keeps between calls. The assistant is the voice manager in Ghostex: it tells the user what their AI coding agent sessions are doing, reads them what a session said, sends sessions prompts, answers the questions and approvals agents wait on, and starts new agents. From the call transcript, write what the assistant needs to pick the conversation up next time: what the user asked about, what was sent, answered, approved or started and to which session, what is still unfinished or waiting on the user, and anything the user asked to be told or reminded of. Leave out greetings and small talk. Write plain past-tense sentences addressed to the user (\"You asked...\"), no lists, no Markdown, at most 120 words. Also write a gist: one line under 90 characters naming the main threads, for a card the user glances at. Answer only with JSON: {\"gist\": \"...\", \"summary\": \"...\"}",
    );
    if let Some(previous) = previous {
        system.push_str(&format!("\n\nThis call continued an earlier one, whose notes were: {previous}\nCarry over threads from those notes that are still open and drop the ones this call settled."));
    }
    let transcript = pending
        .lines
        .iter()
        .map(|line| format!("{}: {}", line.who, line.text))
        .collect::<Vec<_>>()
        .join("\n");
    json!([
        {"role": "system", "content": system},
        {"role": "user", "content": format!("Call transcript:\n{transcript}")},
    ])
}

fn truncate_chars(text: &str, max: usize) -> String {
    let text = text.trim();
    if text.chars().count() <= max {
        return text.to_string();
    }
    let cut: String = text.chars().take(max).collect();
    match cut.rfind(char::is_whitespace) {
        Some(index) => format!("{}…", cut[..index].trim_end()),
        None => cut,
    }
}

/// One summary request (blocking): `(gist, summary)`.
fn request_summary(
    provider: Provider,
    key: &str,
    messages: Value,
) -> Result<(String, String), Failure> {
    let (url, body) = match provider {
        Provider::Xai => (
            XAI_CHAT_URL,
            json!({
                "model": XAI_SUMMARY_MODEL,
                "messages": messages,
                "max_tokens": 600,
                "temperature": 0.2,
                "response_format": {"type": "json_object"},
            }),
        ),
        Provider::OpenAi => (
            OPENAI_CHAT_URL,
            json!({
                "model": OPENAI_SUMMARY_MODEL,
                "messages": messages,
                "max_completion_tokens": 2_000,
                "reasoning_effort": "low",
                "response_format": {"type": "json_object"},
            }),
        ),
    };
    let response = crate::http_agent::answering_agent()
        .post(url)
        .header("authorization", format!("Bearer {key}"))
        .config()
        .timeout_global(Some(Duration::from_secs(60)))
        .build()
        .send_json(body);
    let response = match response {
        Ok(response) if response.status().as_u16() < 400 => response,
        Ok(response) => {
            let status = response.status().as_u16();
            let detail = crate::http_agent::read_json::<Value>(response)
                .ok()
                .and_then(|body| {
                    body.pointer("/error/message")
                        .or_else(|| body.get("error"))
                        .and_then(Value::as_str)
                        .map(|message| truncate_chars(message, 300))
                })
                .unwrap_or_default();
            let message = format!(
                "{} answered HTTP {status} to the summary request. {detail}",
                provider.label()
            );
            // xAI answers 402 for a few minutes at a time on an account whose X subscription
            // covers the API (live, 2026-10-20), so it is retried like a rate limit.
            return Err(
                if matches!(status, 402 | 408 | 409 | 429) || status >= 500 {
                    Failure::Retry(message)
                } else {
                    Failure::Fail(message)
                },
            );
        }
        Err(error) => {
            return Err(Failure::Retry(format!(
                "Couldn't reach {} for the summary: {error}.",
                provider.label()
            )))
        }
    };
    let body: Value = crate::http_agent::read_json(response)
        .map_err(|error| Failure::Retry(format!("Unreadable summary answer: {error}")))?;
    let content = body
        .pointer("/choices/0/message/content")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let parsed: Value = serde_json::from_str(content.trim())
        .map_err(|_| Failure::Fail("The summary answer was not the JSON asked for.".to_string()))?;
    let text = |key: &str| parsed.get(key).and_then(Value::as_str).unwrap_or_default();
    let summary = truncate_chars(text("summary"), MAX_SUMMARY_CHARS);
    let gist = truncate_chars(text("gist"), MAX_GIST_CHARS);
    if summary.is_empty() || gist.is_empty() {
        return Err(Failure::Fail(
            "The summary answer had no summary or gist.".to_string(),
        ));
    }
    Ok((gist, summary))
}
