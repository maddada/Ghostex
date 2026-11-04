use super::*;

pub(crate) fn transcript_tail_text(path: &Path) -> std::io::Result<String> {
    let mut file = File::open(path)?;
    let length = file.metadata()?.len();
    let start = length.saturating_sub(SESSION_CHAT_OPTION_TRANSCRIPT_SCAN_BYTES);
    file.seek(SeekFrom::Start(start))?;
    let mut bytes = Vec::with_capacity((length - start) as usize);
    file.read_to_end(&mut bytes)?;
    if start > 0 {
        if let Some(first_newline) = bytes.iter().position(|byte| *byte == b'\n') {
            bytes.drain(..=first_newline);
        } else {
            bytes.clear();
        }
    }
    Ok(String::from_utf8_lossy(&bytes).into_owned())
}

fn transcript_text(value: Option<&Value>) -> Option<&str> {
    value?
        .as_str()
        .map(str::trim)
        .filter(|text| !text.is_empty())
}

/// CDXC:AgentProviders 2026-09-05 WHY:
/// `claude-opus-5[1m]` and `claude-opus-5` are two rows of Claude's own picker
/// ("Opus (1M context)" and "Opus"), so the context-window suffix is carried
/// into the detected value as `opus[1m]`, matching the published catalog. It
/// is still not a version token, so it is stripped before the version scan.
/// SEE-ALSO: `match_claude_model`, agent-model-catalog.json.
pub(crate) fn claude_transcript_model_choice(model: &str) -> Option<SessionChatDetectedChoice> {
    let normalized = model.trim().to_ascii_lowercase();
    let variant = normalized
        .split_once('[')
        .and_then(|(_, tail)| tail.strip_suffix(']'))
        .map(str::to_string);
    let normalized = normalized
        .split_once('[')
        .map_or(normalized.as_str(), |(head, _)| head)
        .to_string();
    let tokens: Vec<&str> = normalized.split('-').collect();
    let (family_index, family) =
        tokens
            .iter()
            .enumerate()
            .find_map(|(index, token)| match *token {
                "fable" | "opus" | "sonnet" | "haiku" => Some((index, *token)),
                _ => None,
            })?;
    let title = match family {
        "fable" => "Fable",
        "opus" => "Opus",
        "sonnet" => "Sonnet",
        "haiku" => "Haiku",
        _ => return None,
    };
    let following_version: Vec<&str> = tokens
        .iter()
        .skip(family_index + 1)
        .copied()
        .take_while(|token| {
            token.len() <= 2 && !token.is_empty() && token.chars().all(|ch| ch.is_ascii_digit())
        })
        .take(2)
        .collect();
    let preceding_version: Vec<&str> = tokens
        .iter()
        .take(family_index)
        .rev()
        .copied()
        .take_while(|token| {
            token.len() <= 2 && !token.is_empty() && token.chars().all(|ch| ch.is_ascii_digit())
        })
        .take(2)
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let version = if following_version.is_empty() {
        preceding_version
    } else {
        following_version
    };
    let label = if version.is_empty() {
        title.to_string()
    } else {
        format!("{title} {}", version.join("."))
    };
    let id = claude_model_value(family, &version.join("."));
    let (value, label) = match variant {
        Some(variant) => (
            format!("{id}[{variant}]"),
            format!("{label} ({})", variant.to_ascii_uppercase()),
        ),
        // Opus 5.5 is one `opus[1m]` row (Claude Code 2.1.284 offers no 200K
        // Opus), so a bare id the catalog lists only as its 1M row reads as it.
        None if crate::agent_model_catalog::catalog_model("claude", id).is_none()
            && crate::agent_model_catalog::catalog_model("claude", &format!("{id}[1m]"))
                .is_some() =>
        {
            (format!("{id}[1m]"), label)
        }
        None => (id.to_string(), label),
    };
    Some(SessionChatDetectedChoice {
        value,
        label,
        source: SessionChatOptionEvidence::Transcript,
    })
}

fn transcript_effort_choice(effort: &str) -> Option<SessionChatDetectedChoice> {
    let normalized = effort.trim().to_ascii_lowercase();
    matches!(
        normalized.as_str(),
        "minimal" | "low" | "medium" | "high" | "xhigh" | "max"
    )
    .then(|| SessionChatDetectedChoice {
        value: normalized.clone(),
        label: normalized,
        source: SessionChatOptionEvidence::Transcript,
    })
}

/*
CDXC:AgentScreenDetection 2026-09-03 WHY:
Claude's transcript records its permission mode too: a `permission-mode` row
when the mode is set, and `permissionMode` on every user row. Reading them
gives the mode pill a value before the first screen capture and without any
screen at all (a sleeping session, a capped capture). The footer scrape still
wins when present because it is the live value.
*/
pub(crate) fn claude_transcript_mode_choice(mode: &str) -> Option<SessionChatDetectedChoice> {
    let (value, label) = match mode.trim() {
        "auto" => ("auto", "Auto"),
        "bypassPermissions" => ("bypass", "Bypass permissions"),
        "plan" => ("plan", "Plan"),
        "acceptEdits" => ("accept-edits", "Accept edits"),
        "default" => ("manual", "Manual"),
        _ => return None,
    };
    Some(SessionChatDetectedChoice {
        value: value.to_string(),
        label: label.to_string(),
        source: SessionChatOptionEvidence::Transcript,
    })
}

pub(super) fn detect_session_chat_transcript_selection(
    agent: SessionChatOptionAgent,
    text: &str,
) -> Option<SessionChatDetectedSelection> {
    if agent == SessionChatOptionAgent::Claude {
        return detect_claude_transcript_selection(text);
    }
    for line in text.lines().rev() {
        let Ok(Value::Object(record)) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        let selection = match agent {
            SessionChatOptionAgent::Codex
                if transcript_text(record.get("type")) == Some("turn_context") =>
            {
                let payload = record.get("payload").and_then(Value::as_object);
                SessionChatDetectedSelection {
                    model: payload
                        .and_then(|payload| transcript_text(payload.get("model")))
                        .map(|model| SessionChatDetectedChoice {
                            value: model.to_string(),
                            label: model.to_string(),
                            source: SessionChatOptionEvidence::Transcript,
                        }),
                    effort: payload
                        .and_then(|payload| {
                            transcript_text(payload.get("effort"))
                                .or_else(|| transcript_text(payload.get("reasoning_effort")))
                        })
                        .and_then(transcript_effort_choice),
                    mode: None,
                    permissions: payload.and_then(codex_turn_context_permissions),
                    permissions_recorded_at: transcript_text(record.get("timestamp"))
                        .map(str::to_string),
                    context_window: None,
                    terminal_status_line: None,
                    fast: None,
                    context_usage: None,
                    claude_status: None,
                    codex_status: None,
                    cursor_status: None,
                    hermes_status: None,
                    pi_status: None,
                    checkout_status: None,
                    model_catalog: None,
                }
            }
            _ => continue,
        };
        if selection.model.is_some()
            || selection.effort.is_some()
            || selection.mode.is_some()
            || selection.permissions.is_some()
        {
            return Some(selection);
        }
    }
    None
}

/// Newest assistant row for model/effort, newest permission row for mode;
/// the scan stops as soon as all three are known.
fn detect_claude_transcript_selection(text: &str) -> Option<SessionChatDetectedSelection> {
    let mut selection = SessionChatDetectedSelection::default();
    let mut assistant_seen = false;
    for line in text.lines().rev() {
        if assistant_seen && selection.mode.is_some() {
            break;
        }
        let Ok(Value::Object(record)) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if record.get("isSidechain") == Some(&Value::Bool(true)) {
            continue;
        }
        match transcript_text(record.get("type")) {
            Some("assistant") if !assistant_seen => {
                let message = record.get("message").and_then(Value::as_object);
                selection.model = message
                    .and_then(|message| transcript_text(message.get("model")))
                    .and_then(claude_transcript_model_choice);
                selection.effort =
                    transcript_text(record.get("effort")).and_then(transcript_effort_choice);
                assistant_seen = true;
            }
            Some("permission-mode") | Some("user") if selection.mode.is_none() => {
                selection.mode = transcript_text(record.get("permissionMode"))
                    .and_then(claude_transcript_mode_choice);
            }
            _ => {}
        }
    }
    (selection.model.is_some() || selection.effort.is_some() || selection.mode.is_some())
        .then_some(selection)
}

/*
CDXC:AgentScreenDetection 2026-09-03 WHY:
The payload the Ghostex statusLine script stored for this Claude session id.
Model and effort are the live session values (Claude re-runs the script on
`/model`, `/effort`, each assistant message, compaction and mode changes),
so they outrank the transcript, which only learns a change on the next turn.
*/
/// CDXC:AgentProviders 2026-09-28 WHY:
/// Claude Code 2.1.283 on a Max account runs Opus 5.5 with a 1M window but reports it as `claude-opus-5-5` ("Opus 5.5"), with no `[1m]` suffix, and its `/model` list has no separate 1M row. Reading the id alone showed "Opus 5.5 · 200K" for a session Claude itself measured at 1,000,000 tokens. The payload's `context_window_size` is Claude's own answer, so a model with a 1M catalog row that reports a window of at least 1M reads as that row.
/// SEE-ALSO: server/src/session_chat_claude_effort_slider.rs `claude_live_selection` decides "already applied" from the same reading.
pub(crate) fn claude_statusline_model_choice(
    payload: &serde_json::Map<String, Value>,
) -> Option<SessionChatDetectedChoice> {
    let id = transcript_text(payload.get("model")?.get("id"))?;
    let reports_long_context = payload
        .get("context_window")
        .and_then(|window| window.get("context_window_size"))
        .and_then(Value::as_u64)
        .is_some_and(|size| size >= 1_000_000);
    // Fable 5.1 and Sonnet 5.5 run with a 1M window too but have only one catalog row, which a
    // `fable[1m]` reading matched nothing in.
    if reports_long_context && !id.contains('[') {
        if let Some(long) = claude_transcript_model_choice(&format!("{id}[1m]")).filter(|long| {
            crate::agent_model_catalog::catalog_model("claude", &long.value).is_some()
        }) {
            return Some(long);
        }
    }
    claude_transcript_model_choice(id)
}

fn read_session_chat_statusline_selection(
    hook_state_directory: &Path,
    agent_session_id: Option<&str>,
) -> Option<SessionChatDetectedSelection> {
    read_claude_statusline_reading(hook_state_directory, agent_session_id?)
        .map(|(selection, _)| selection)
}

/// Claude's last statusline payload as a selection, with the time Ghostex stored it.
pub(crate) fn read_claude_statusline_reading(
    hook_state_directory: &Path,
    agent_session_id: &str,
) -> Option<(SessionChatDetectedSelection, String)> {
    let stored = crate::agent_hooks::statusline::read_claude_statusline_payload(
        hook_state_directory,
        agent_session_id,
    )?;
    let payload = &stored.payload;
    let choice = |value: &str, label: &str| SessionChatDetectedChoice {
        value: value.to_string(),
        label: label.to_string(),
        source: SessionChatOptionEvidence::Statusline,
    };
    let model =
        claude_statusline_model_choice(payload).map(|found| choice(&found.value, &found.label));
    let effort = payload
        .get("effort")
        .and_then(|effort| transcript_text(effort.get("level")))
        .and_then(transcript_effort_choice)
        .map(|found| choice(&found.value, &found.label));
    let fast = payload.get("fast_mode").and_then(Value::as_bool);
    let context_window = payload.get("context_window");
    let context_usage = SessionChatContextUsage {
        used_percentage: context_window
            .and_then(|window| window.get("used_percentage"))
            .and_then(Value::as_f64)
            .map(|value| value.round().clamp(0.0, 100.0) as u32),
        used_tokens: context_window
            .and_then(|window| window.get("total_input_tokens"))
            .and_then(Value::as_u64),
        window_size: context_window
            .and_then(|window| window.get("context_window_size"))
            .and_then(Value::as_u64),
    };
    let selection = SessionChatDetectedSelection {
        model,
        effort,
        mode: None,
        permissions: None,
        permissions_recorded_at: None,
        context_window: None,
        terminal_status_line: None,
        fast,
        context_usage: (!context_usage.is_empty()).then_some(context_usage),
        claude_status: claude_statusline_status_value(payload),
        codex_status: None,
        cursor_status: None,
        hermes_status: None,
        pi_status: None,
        checkout_status: None,
        model_catalog: None,
    };
    // CDXC:AgentProviders 2026-09-09 WHY:
    // Claude's reported usage is useful before model or effort detection succeeds; do not discard the statusline stats with an unrecognized choice.
    (selection.model.is_some()
        || selection.effort.is_some()
        || selection.context_usage.is_some()
        || selection.claude_status.is_some())
    .then_some((selection, stored.updated_at))
}

/*
CDXC:SessionChatDetectedOptions 2026-09-04 DECISION:
User: the context meter popover gets a "More details" section (cost, rate
limits, prompt cache, last request, lines, thinking, version, ...) with a pen
icon to pick, reorder within groups, and star rows for a text status line under
the chat box. Everything the chat can show is lifted here from the stored
payload, renamed to camelCase and dropped when Claude did not report it, so
the client owns which rows exist and never sees the raw payload.
*/
fn claude_statusline_status_value(payload: &Map<String, Value>) -> Option<Value> {
    fn get<'a>(value: Option<&'a Value>, key: &str) -> Option<&'a Value> {
        value.and_then(|value| value.get(key))
    }
    fn put(map: &mut Map<String, Value>, key: &str, value: Option<Value>) {
        if let Some(value) = value {
            map.insert(key.to_string(), value);
        }
    }
    fn number(value: Option<&Value>) -> Option<Value> {
        value
            .and_then(Value::as_f64)
            .filter(|number| number.is_finite())
            .map(|number| json!(number))
    }
    fn integer(value: Option<&Value>) -> Option<Value> {
        value.and_then(Value::as_i64).map(|number| json!(number))
    }
    fn boolean(value: Option<&Value>) -> Option<Value> {
        value.and_then(Value::as_bool).map(|flag| json!(flag))
    }
    fn text(value: Option<&Value>) -> Option<Value> {
        value
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|text| !text.is_empty())
            .map(|text| json!(text))
    }
    fn object(map: Map<String, Value>) -> Option<Value> {
        (!map.is_empty()).then_some(Value::Object(map))
    }

    let mut status = Map::new();

    let cost = payload.get("cost");
    let mut cost_map = Map::new();
    put(
        &mut cost_map,
        "totalUsd",
        number(get(cost, "total_cost_usd")),
    );
    put(
        &mut cost_map,
        "durationMs",
        integer(get(cost, "total_duration_ms")),
    );
    put(
        &mut cost_map,
        "apiDurationMs",
        integer(get(cost, "total_api_duration_ms")),
    );
    put(
        &mut cost_map,
        "linesAdded",
        integer(get(cost, "total_lines_added")),
    );
    put(
        &mut cost_map,
        "linesRemoved",
        integer(get(cost, "total_lines_removed")),
    );
    put(&mut status, "cost", object(cost_map));

    let rate_limits = payload.get("rate_limits");
    let mut limits_map = Map::new();
    for (key, name) in [("five_hour", "fiveHour"), ("seven_day", "sevenDay")] {
        let window = get(rate_limits, key);
        let mut window_map = Map::new();
        put(
            &mut window_map,
            "usedPercentage",
            number(get(window, "used_percentage")),
        );
        put(
            &mut window_map,
            "resetsAt",
            integer(get(window, "resets_at")),
        );
        put(&mut limits_map, name, object(window_map));
    }
    put(&mut status, "rateLimits", object(limits_map));

    let cache = payload.get("prompt_cache");
    let mut cache_map = Map::new();
    put(&mut cache_map, "warm", boolean(get(cache, "warm")));
    put(&mut cache_map, "ttl", text(get(cache, "ttl")));
    put(
        &mut cache_map,
        "expiresAt",
        integer(get(cache, "expires_at")),
    );
    put(&mut cache_map, "hitRatio", number(get(cache, "hit_ratio")));
    put(&mut cache_map, "requests", integer(get(cache, "requests")));
    put(&mut cache_map, "misses", integer(get(cache, "misses")));
    put(
        &mut cache_map,
        "lastMissCause",
        text(get(cache, "last_miss_cause")),
    );
    put(
        &mut cache_map,
        "cacheWriteTokens",
        integer(get(cache, "cache_write_tokens")),
    );
    put(
        &mut cache_map,
        "recacheTokensIfCold",
        integer(get(cache, "recache_tokens_if_cold")),
    );
    put(&mut status, "promptCache", object(cache_map));

    let context_window = payload.get("context_window");
    let usage = get(context_window, "current_usage");
    let mut request_map = Map::new();
    put(
        &mut request_map,
        "inputTokens",
        integer(get(usage, "input_tokens")),
    );
    put(
        &mut request_map,
        "outputTokens",
        integer(get(usage, "output_tokens")),
    );
    put(
        &mut request_map,
        "cacheReadTokens",
        integer(get(usage, "cache_read_input_tokens")),
    );
    put(
        &mut request_map,
        "cacheWriteTokens",
        integer(get(usage, "cache_creation_input_tokens")),
    );
    put(&mut status, "lastRequest", object(request_map));
    put(
        &mut status,
        "totalOutputTokens",
        integer(get(context_window, "total_output_tokens")),
    );
    put(
        &mut status,
        "remainingPercentage",
        number(get(context_window, "remaining_percentage")),
    );
    put(
        &mut status,
        "exceeds200kTokens",
        boolean(payload.get("exceeds_200k_tokens")),
    );

    put(
        &mut status,
        "thinkingEnabled",
        boolean(get(payload.get("thinking"), "enabled")),
    );
    put(
        &mut status,
        "outputStyle",
        text(get(payload.get("output_style"), "name")),
    );
    put(
        &mut status,
        "sessionName",
        text(payload.get("session_name")),
    );
    put(&mut status, "sessionId", text(payload.get("session_id")));
    put(&mut status, "version", text(payload.get("version")));

    let workspace = payload.get("workspace");
    let repo = get(workspace, "repo");
    let mut repo_map = Map::new();
    put(&mut repo_map, "host", text(get(repo, "host")));
    put(&mut repo_map, "owner", text(get(repo, "owner")));
    put(&mut repo_map, "name", text(get(repo, "name")));
    put(&mut status, "repo", object(repo_map));
    let added_dirs: Vec<Value> = get(workspace, "added_dirs")
        .and_then(Value::as_array)
        .map(|dirs| dirs.iter().filter_map(|dir| text(Some(dir))).collect())
        .unwrap_or_default();
    if !added_dirs.is_empty() {
        status.insert("addedDirs".to_string(), Value::Array(added_dirs));
    }
    put(
        &mut status,
        "projectDir",
        text(get(workspace, "project_dir")),
    );
    put(
        &mut status,
        "currentDir",
        text(get(workspace, "current_dir")).or_else(|| text(payload.get("cwd"))),
    );

    let pr = payload.get("pr");
    let mut pr_map = Map::new();
    put(&mut pr_map, "number", integer(get(pr, "number")));
    put(&mut pr_map, "url", text(get(pr, "url")));
    put(&mut pr_map, "reviewState", text(get(pr, "review_state")));
    put(&mut status, "pr", object(pr_map));

    object(status)
}

fn read_session_chat_transcript_selection(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    agent: SessionChatOptionAgent,
) -> Option<SessionChatDetectedSelection> {
    let session = repository.get_session(project_id, session_id).ok()??;
    let runtime = session.get("runtimeSettings").and_then(Value::as_object);
    let agent_session_id =
        runtime.and_then(|runtime| transcript_text(runtime.get("agentSessionId")));
    let agent_session_path =
        runtime.and_then(|runtime| transcript_text(runtime.get("agentSessionPath")));
    let transcript_agent =
        crate::session_chat::resolve_session_chat_transcript_agent(match agent {
            SessionChatOptionAgent::Claude => Some("claude"),
            SessionChatOptionAgent::Codex => Some("codex"),
            // Antigravity's footer names both values for the whole session,
            // and its mirrored step log carries no model field.
            SessionChatOptionAgent::Antigravity => return None,
            SessionChatOptionAgent::Cursor => return None,
            /*
            Grok's statusline is on screen for the whole session and names both
            values, and its update-stream rows carry no effort at all, so there
            is nothing a transcript read could add here.
            */
            SessionChatOptionAgent::Grok => return None,
            /*
            Hermes names the model in a statusline that is on screen for the
            whole session, and its mirrored transcript rows carry no model
            field, so there is nothing a transcript read could add here.
            */
            SessionChatOptionAgent::Hermes => return None,
            SessionChatOptionAgent::Omp => return None,
            SessionChatOptionAgent::Pi => return None,
            // Empryo's model rides its tab state, which the stored reading takes from meta.json.
            SessionChatOptionAgent::Empryo => return None,
            // ZCode's statusline names no catalog values chat needs.
            SessionChatOptionAgent::Zcode => return None,
            // Copilot's footer names the model and the mode for the whole session.
            SessionChatOptionAgent::Copilot => return None,
            // dsh-tui's status line names the model and the level for the whole session.
            SessionChatOptionAgent::Dsh => return None,
        })?;
    let path = crate::session_chat::resolve_session_chat_transcript_path(
        transcript_agent,
        agent_session_id,
        agent_session_path,
    )?;
    let stamp = TranscriptStamp::read(&path)?;
    let catalog = crate::agent_model_catalog::current();
    if let Some(selection) = cached_transcript_selection(&path, agent, &stamp, &catalog) {
        return selection;
    }
    let text = transcript_tail_text(&path).ok()?;
    let selection = detect_session_chat_transcript_selection(agent, &text);
    remember_transcript_selection(path, agent, stamp, catalog, selection.clone());
    selection
}

/// What identifies one version of a transcript file: its size and modification time, and on Unix the file itself, so a file replaced at the same path is read again. Shared with `session_chat_notice_progress`.
#[derive(Clone, PartialEq, Eq)]
pub(crate) struct TranscriptStamp {
    length: u64,
    modified: Option<std::time::SystemTime>,
    #[cfg(unix)]
    file: (u64, u64),
}

impl TranscriptStamp {
    pub(crate) fn read(path: &Path) -> Option<Self> {
        let metadata = std::fs::metadata(path).ok()?;
        Some(Self {
            length: metadata.len(),
            modified: metadata.modified().ok(),
            #[cfg(unix)]
            file: {
                use std::os::unix::fs::MetadataExt;
                (metadata.dev(), metadata.ino())
            },
        })
    }
}

struct CachedTranscriptSelection {
    agent: SessionChatOptionAgent,
    stamp: TranscriptStamp,
    /// The model catalog the reading was made with: a Claude model's 1M row depends on it.
    catalog: Arc<Value>,
    selection: Option<SessionChatDetectedSelection>,
}

/// More transcripts than any machine follows at once; past it the cache simply starts over.
const TRANSCRIPT_SELECTION_CACHE_LIMIT: usize = 256;

/// CDXC:AgentScreenDetection 2026-10-23 WHY:
/// Every screen probe (once a second for a followed session that is working) read the last 6 MB of the session's transcript to find the model and effort it records, even when the file had not changed since the previous probe. The reading depends only on the file's bytes and the model catalog, so it is kept per transcript and reused until the file's stamp or the catalog changes. The stamp is taken before the read, so an append that lands during it only causes one more read.
static TRANSCRIPT_SELECTION_CACHE: std::sync::OnceLock<
    Mutex<HashMap<std::path::PathBuf, CachedTranscriptSelection>>,
> = std::sync::OnceLock::new();

fn cached_transcript_selection(
    path: &Path,
    agent: SessionChatOptionAgent,
    stamp: &TranscriptStamp,
    catalog: &Arc<Value>,
) -> Option<Option<SessionChatDetectedSelection>> {
    let cache = TRANSCRIPT_SELECTION_CACHE.get()?.lock().ok()?;
    let entry = cache.get(path)?;
    (entry.agent == agent && &entry.stamp == stamp && Arc::ptr_eq(&entry.catalog, catalog))
        .then(|| entry.selection.clone())
}

fn remember_transcript_selection(
    path: std::path::PathBuf,
    agent: SessionChatOptionAgent,
    stamp: TranscriptStamp,
    catalog: Arc<Value>,
    selection: Option<SessionChatDetectedSelection>,
) {
    let Ok(mut cache) = TRANSCRIPT_SELECTION_CACHE
        .get_or_init(Mutex::default)
        .lock()
    else {
        return;
    };
    if cache.len() >= TRANSCRIPT_SELECTION_CACHE_LIMIT && !cache.contains_key(&path) {
        cache.clear();
    }
    cache.insert(
        path,
        CachedTranscriptSelection {
            agent,
            stamp,
            catalog,
            selection,
        },
    );
}

/// Every `Some` in `layer` replaces the value beneath it.
fn overlay_session_chat_option_selection(
    merged: &mut SessionChatDetectedSelection,
    layer: SessionChatDetectedSelection,
) {
    if let Some(model) = layer.model {
        merged.model = Some(model);
    }
    if layer.effort.is_some() {
        merged.effort = layer.effort;
    }
    if layer.mode.is_some() {
        merged.mode = layer.mode;
    }
    if layer.permissions.is_some() {
        merged.permissions = layer.permissions;
        merged.permissions_recorded_at = layer.permissions_recorded_at;
    }
    if layer.context_window.is_some() {
        merged.context_window = layer.context_window;
    }
    if layer.terminal_status_line.is_some() {
        merged.terminal_status_line = layer.terminal_status_line;
    }
    if layer.fast.is_some() {
        merged.fast = layer.fast;
    }
    if layer.context_usage.is_some() {
        merged.context_usage = layer.context_usage;
    }
    if layer.claude_status.is_some() {
        merged.claude_status = layer.claude_status;
    }
    if layer.cursor_status.is_some() {
        merged.cursor_status = layer.cursor_status;
    }
    if layer.hermes_status.is_some() {
        merged.hermes_status = layer.hermes_status;
    }
    if layer.pi_status.is_some() {
        merged.pi_status = layer.pi_status;
    }
    if layer.model_catalog.is_some() {
        merged.model_catalog = layer.model_catalog;
    }
}

/// CDXC:AgentProviders 2026-09-28 WHY:
/// Claude's footer prints "Opus 5.5" for both context sizes, so the screen alone reads `opus`, the 200K twin, while the statusline JSON Claude pipes to Ghostex reads `opus[1m]` from its own `context_window_size`. Chat took whichever reading arrived last, so the composer pill showed "200K" and dropped it again every few seconds on a 1M session. The terminal still names the model (the 2026-09-08 decision below); the statusline only adds the window the footer cannot print, when both name the same model. This is the rule the model picker already used to decide a pick was applied.
/// SEE-ALSO: server/src/session_chat_claude_effort_slider.rs `claude_live_selection`.
pub(crate) fn claude_long_context_twin(model: &str, statusline: Option<&str>) -> Option<String> {
    let statusline = statusline?;
    (!model.contains('[') && statusline.strip_suffix("[1m]") == Some(model))
        .then(|| statusline.to_string())
}

/// Precedence, lowest first: the launch command's flags (only until the agent
/// has reported through its transcript or statusline), transcript (a turn
/// behind), statusline payload (live, but only what Claude puts in it),
/// terminal screen (live, and the only source for the permission mode footer).
///
/// CDXC:AgentScreenDetection 2026-09-08 DECISION:
/// User: terminal evidence always has the highest priority, including for Claude.
/// This supersedes preserving an older Claude model variant over the model visible in the terminal; a visible (1M) suffix is parsed from the terminal itself.
/// SEE-ALSO: `merge_options` in packages/gx-chat-core/src/session/fold.rs preserves source priority when reads arrive separately.
pub(super) fn merge_session_chat_option_selections(
    launch: Option<SessionChatDetectedSelection>,
    transcript: Option<SessionChatDetectedSelection>,
    statusline: Option<SessionChatDetectedSelection>,
    terminal: Option<SessionChatDetectedSelection>,
) -> Option<SessionChatDetectedSelection> {
    let launch_permissions = launch
        .as_ref()
        .and_then(|launch| launch.permissions.clone());
    let mut merged = match transcript {
        Some(transcript) => transcript,
        // Cursor's payload carries only context usage, so it must not hide a pinned model.
        None => launch
            .filter(|_| {
                statusline
                    .as_ref()
                    .is_none_or(|statusline| statusline.model.is_none())
            })
            .unwrap_or_default(),
    };
    let statusline_model = statusline
        .as_ref()
        .and_then(|statusline| statusline.model.as_ref())
        .map(|model| model.value.clone());
    // A transcript whose turn records name no permission mode leaves the command line's own
    // flags as the evidence.
    if merged.permissions.is_none() {
        merged.permissions = launch_permissions;
    }
    if let Some(statusline) = statusline {
        overlay_session_chat_option_selection(&mut merged, statusline);
    }
    if let Some(terminal) = terminal {
        overlay_session_chat_option_selection(&mut merged, terminal);
    }
    if let Some(model) = merged.model.as_mut() {
        if let Some(long) = claude_long_context_twin(&model.value, statusline_model.as_deref()) {
            model.value = long;
        }
        // Pi, OMP and Empryo footers name a model by id or name; the picker keys it `provider/id`.
        if let Some(value) = merged.model_catalog.as_ref().and_then(|catalog| {
            crate::session_chat_pi_models::pi_family_catalog_value(
                catalog,
                &model.value,
                statusline_model.as_deref(),
            )
        }) {
            model.value = value;
        }
    }
    (merged.model.is_some()
        || merged.effort.is_some()
        || merged.mode.is_some()
        || merged.permissions.is_some()
        || merged.context_usage.is_some()
        || merged.claude_status.is_some()
        || merged.cursor_status.is_some()
        || merged.hermes_status.is_some()
        || merged.pi_status.is_some())
    .then_some(merged)
}

/// The option readings that live on disk rather than on the screen: the transcript's structured
/// metadata, and the statusline sidecar Claude, Cursor and Hermes write. Also returns Claude's own
/// session id and transcript path, which the task store hangs off.
#[allow(clippy::type_complexity)]
pub(super) fn read_session_chat_stored_selections(
    repository: &DomainRepository<'_>,
    hook_state_directory: &Path,
    project_id: &str,
    session_id: &str,
    agent: Option<SessionChatOptionAgent>,
) -> (
    Option<SessionChatDetectedSelection>,
    Option<SessionChatDetectedSelection>,
    Option<String>,
    Option<String>,
) {
    let transcript = agent.and_then(|agent| {
        read_session_chat_transcript_selection(repository, project_id, session_id, agent)
    });
    // The hooks record Claude's own session id and transcript path on the
    // session row; both the statusline sidecar and the task store hang off them.
    let (claude_session_id, claude_session_path) = (agent == Some(SessionChatOptionAgent::Claude))
        .then(|| {
            repository
                .get_session(project_id, session_id)
                .ok()
                .flatten()
        })
        .flatten()
        .map(|session| {
            let runtime_text = |key: &str| {
                session
                    .get("runtimeSettings")
                    .and_then(|runtime| runtime.get(key))
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .map(str::to_string)
            };
            (
                runtime_text("agentSessionId"),
                runtime_text("agentSessionPath"),
            )
        })
        .unwrap_or((None, None));
    let statusline = match agent {
        Some(SessionChatOptionAgent::Claude) => read_session_chat_statusline_selection(
            hook_state_directory,
            claude_session_id.as_deref(),
        ),
        Some(SessionChatOptionAgent::Cursor) => {
            crate::session_chat_cursor_status::read_cursor_statusline_selection(
                repository,
                hook_state_directory,
                project_id,
                session_id,
            )
        }
        Some(SessionChatOptionAgent::Hermes) => {
            crate::session_chat_hermes_status::read_hermes_status_selection(
                repository, project_id, session_id,
            )
        }
        Some(
            agent @ (SessionChatOptionAgent::Pi
            | SessionChatOptionAgent::Omp
            | SessionChatOptionAgent::Empryo),
        ) => crate::session_chat_pi_models::read_pi_family_selection(
            repository, project_id, session_id, agent,
        ),
        Some(SessionChatOptionAgent::Antigravity) => {
            crate::session_chat_antigravity_status::read_antigravity_context_selection(
                repository, project_id, session_id,
            )
        }
        Some(SessionChatOptionAgent::Grok) => {
            crate::session_chat_grok_status::read_grok_context_selection(
                repository, project_id, session_id,
            )
        }
        _ => None,
    };
    (
        transcript,
        statusline,
        claude_session_id,
        claude_session_path,
    )
}

/// CDXC:AgentScreenDetection 2026-09-30 WHY:
/// A session that is not running has no screen, but its last model, effort and mode are still on
/// disk (the transcript's metadata and the statusline sidecar), which is what the chat's pills show
/// until the session runs again. Answering a sleeping session's read with "probed, nothing
/// detected" drew a bare "Model" pill for as long as it slept, and flashed it on every chat opened
/// on a session that was still waking.
pub(crate) fn detect_session_chat_stored_options(
    repository: &DomainRepository<'_>,
    hook_state_directory: &Path,
    project_id: &str,
    session_id: &str,
    agent_id: Option<&str>,
) -> Option<SessionChatDetectedOptions> {
    let agent = session_chat_option_agent(agent_id)?;
    let (transcript, statusline, _, _) = read_session_chat_stored_selections(
        repository,
        hook_state_directory,
        project_id,
        session_id,
        Some(agent),
    );
    let launch =
        read_session_chat_launch_selection(repository, project_id, session_id, Some(agent));
    let remembered =
        read_session_chat_remembered_selection(repository, project_id, session_id, agent);
    merge_session_chat_option_selections(launch, transcript, statusline, None)
        .map(|mut selection| {
            if let Some(remembered) = remembered {
                overlay_session_chat_option_selection(&mut selection, remembered);
            }
            selection
        })
        .map(|mut selection| {
            crate::session_chat_hermes_status::restore_hermes_model_id(&mut selection);
            selection
        })
        .map(SessionChatDetectedOptions::new)
}
