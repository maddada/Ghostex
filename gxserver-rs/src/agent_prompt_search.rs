/*
CDXC:AgentHistorySearch 2026-08-20:
The Find surface (a GUI for `gx f`) is served from here. It keeps one warm
`zehn::SearchIndex` — the same scanner, matcher, favorites file, and Codex cache
the terminal picker uses — so both surfaces rank identically and a prompt starred
in one is starred in the other.

Rows carry a stable per-prompt key (the favorites hash of agent + text), not an
index position. A rebuild reorders records, and the user may sit on a result for
minutes before acting on it; keying by identity means the follow-up call still
lands on the prompt they were looking at instead of being rejected or, worse,
acting on whatever moved into that slot.

Opening a result is deliberately NOT a session factory here. This module resolves
the decision — focus a live Ghostex session that already owns the conversation, or
hand the host a cwd plus the exact command to run — and the host performs it with
the session-creation path it already has.
*/

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use zehn::agent::{Agent, ALL_AGENTS};
use zehn::index::{day_key, QueryOptions, SearchIndex};
use zehn::{favorites, index as zehn_index};

/// Rows carry a bounded slice of the prompt so a keystroke never ships megabytes
/// of pasted transcripts — a full page of 60 pasted AGENTS.md prompts is a
/// quarter-megabyte at 4k each, which is a poor deal over an SSH-tunnelled
/// mobile link. The row slice only has to fill one line; the preview pane pulls
/// the selected prompt in full through `readAgentPromptText`.
const DEFAULT_TEXT_LIMIT: usize = 1_200;
const MAX_TEXT_LIMIT: usize = 200_000;
const DEFAULT_ROW_LIMIT: usize = 60;
const MAX_ROW_LIMIT: usize = 500;

/// How long a built index is served before the next query rebuilds it. Agent
/// history files only change when a prompt is sent, and a rebuild reads ~25k
/// records in about a second, so this trades a little staleness for not
/// re-scanning on every keystroke.
const INDEX_MAX_AGE: Duration = Duration::from_secs(90);

struct CachedIndex {
    index: SearchIndex,
    epoch: u64,
    built_at: Instant,
    built_at_unix: i64,
}

static CACHE: OnceLock<Mutex<Option<CachedIndex>>> = OnceLock::new();

fn cache() -> &'static Mutex<Option<CachedIndex>> {
    CACHE.get_or_init(|| Mutex::new(None))
}

#[derive(Debug)]
pub struct PromptSearchError {
    pub code: &'static str,
    pub message: String,
}

impl PromptSearchError {
    fn invalid(message: impl Into<String>) -> Self {
        Self { code: "invalidParams", message: message.into() }
    }

    fn unknown_prompt() -> Self {
        Self {
            code: "notFound",
            message: "That prompt is no longer in this machine's agent history.".to_string(),
        }
    }
}

fn now_unix() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

/*
CDXC:AgentHookIsolation 2026-08-06-21:35 (same rule, applied to history):
GHOSTEX_HOME and explicit daemon homes are isolated profiles. Their history
scan, derived cache, and favorites file must stay inside that profile instead
of following the process environment back into the real user's agent data.
Production daemons keep zehn's own environment contract so `gx f` and this
surface read and write exactly the same files.
*/
fn resolve_search_paths(paths: &crate::paths::GxserverPaths) -> (String, PathBuf, PathBuf) {
    if let Some(isolated) = paths.isolated_agent_home_dir.as_ref() {
        let home = isolated.to_string_lossy().to_string();
        return (
            home,
            isolated.join(".cache").join("ghostex").join("zehn"),
            isolated.join(".config").join("zehn").join("favorites"),
        );
    }
    let home = paths.home_dir.to_string_lossy().to_string();
    let cache_root = zehn_index::cache_root_from_env(&home);
    let xdg_config = std::env::var("XDG_CONFIG_HOME").ok();
    let favorites_path = favorites::path_for(&home, xdg_config.as_deref());
    (home, cache_root, favorites_path)
}

fn build_index(paths: &crate::paths::GxserverPaths) -> SearchIndex {
    let (home, cache_root, favorites_path) = resolve_search_paths(paths);
    SearchIndex::build(&home, &cache_root, favorites_path)
}

/// Run `action` against the warm index, rebuilding first when it is missing,
/// stale, or the caller explicitly asked for a refresh.
fn with_index<T>(
    paths: &crate::paths::GxserverPaths,
    force_refresh: bool,
    action: impl FnOnce(&mut CachedIndex) -> Result<T, PromptSearchError>,
) -> Result<T, PromptSearchError> {
    let mut guard = cache().lock().map_err(|_| PromptSearchError {
        code: "internalError",
        message: "The prompt index lock was poisoned.".to_string(),
    })?;
    let needs_build = match guard.as_ref() {
        None => true,
        Some(cached) => force_refresh || cached.built_at.elapsed() > INDEX_MAX_AGE,
    };
    if needs_build {
        let next_epoch = guard.as_ref().map(|cached| cached.epoch + 1).unwrap_or(1);
        *guard = Some(CachedIndex {
            index: build_index(paths),
            epoch: next_epoch,
            built_at: Instant::now(),
            built_at_unix: now_unix(),
        });
    }
    let cached = guard.as_mut().expect("index built above");
    action(cached)
}

/// Drop the warm index so the next call rebuilds. Used after an action that is
/// known to change history on disk.
pub fn invalidate_index() {
    if let Ok(mut guard) = cache().lock() {
        *guard = None;
    }
}

fn read_agents(params: &Map<String, Value>) -> Result<Vec<Agent>, PromptSearchError> {
    let Some(value) = params.get("agents") else { return Ok(Vec::new()) };
    let Some(items) = value.as_array() else {
        return Err(PromptSearchError::invalid("agents must be an array of agent names."));
    };
    let mut agents = Vec::new();
    for item in items {
        let Some(name) = item.as_str() else {
            return Err(PromptSearchError::invalid("agents must be an array of agent names."));
        };
        let Some(agent) = Agent::parse(name.trim()) else {
            return Err(PromptSearchError::invalid(format!("Unknown agent \"{name}\".")));
        };
        agents.push(agent);
    }
    Ok(agents)
}

fn read_usize(params: &Map<String, Value>, key: &str, default: usize, max: usize) -> usize {
    params
        .get(key)
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(default)
        .min(max)
}

fn truncate_on_char_boundary(text: &str, limit: usize) -> &str {
    if text.len() <= limit {
        return text;
    }
    let mut end = limit;
    while end > 0 && !text.is_char_boundary(end) {
        end -= 1;
    }
    &text[..end]
}

/// `POST /api/searchAgentPrompts`
pub fn search_agent_prompts(
    paths: &crate::paths::GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, PromptSearchError> {
    let query = params.get("query").and_then(Value::as_str).unwrap_or("").to_string();
    let agents = read_agents(params)?;
    let project = params
        .get("project")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string);
    let group_by_day = params.get("groupByDay").and_then(Value::as_bool).unwrap_or(false);
    let offset = read_usize(params, "offset", 0, usize::MAX);
    let limit = read_usize(params, "limit", DEFAULT_ROW_LIMIT, MAX_ROW_LIMIT);
    let text_limit = read_usize(params, "textLimit", DEFAULT_TEXT_LIMIT, MAX_TEXT_LIMIT);
    let include_facets = params.get("includeFacets").and_then(Value::as_bool).unwrap_or(true);
    let refresh = params.get("refresh").and_then(Value::as_bool).unwrap_or(false);

    with_index(paths, refresh, |cached| {
        let options = QueryOptions {
            query,
            agents,
            project,
            group_by_day,
            offset,
            limit,
        };
        let result = cached.index.query(&options);
        let rows: Vec<Value> = result
            .hits
            .iter()
            .map(|hit| {
                let rec = &cached.index.records[hit.index];
                let text = truncate_on_char_boundary(&rec.text, text_limit);
                json!({
                    "key": format!("{:016x}", zehn_index::record_key(rec)),
                    "index": hit.index,
                    "agent": rec.agent.label(),
                    "agentColor": rec.agent.hex_color(),
                    "text": text,
                    "textLength": rec.text.len(),
                    "truncated": text.len() < rec.text.len(),
                    "title": rec.display_title(),
                    "project": rec.project,
                    "projectName": rec.project_display_name(),
                    "sessionId": rec.session,
                    "ts": rec.ts,
                    "dayKey": day_key(rec.ts),
                    "favorite": hit.favorite,
                    "score": hit.score,
                    "highlights": hit.positions,
                    "meta": {
                        "provider": rec.meta.provider,
                        "model": rec.meta.model,
                        "thinking": rec.meta.thinking,
                        "plan": rec.meta.plan,
                        "usage": {
                            "input": rec.meta.usage.input,
                            "output": rec.meta.usage.output,
                            "cacheRead": rec.meta.usage.cache_read,
                            "cacheWrite": rec.meta.usage.cache_write,
                            "total": rec.meta.usage.total,
                            "contextWindow": rec.meta.usage.context_window,
                            "ratePercent": rec.meta.usage.rate_percent,
                            "cost": rec.meta.usage.cost,
                        },
                    },
                })
            })
            .collect();

        let mut payload = json!({
            "indexEpoch": cached.epoch,
            "indexedAt": cached.built_at_unix,
            "total": result.total,
            "matched": result.matched,
            "offset": offset,
            "rows": rows,
        });
        if let Some(error) = &cached.index.opencode_error {
            payload["opencodeError"] = json!(error);
        }
        if include_facets {
            payload["projects"] = json!(cached
                .index
                .projects()
                .into_iter()
                .map(|path| json!({
                    "path": path,
                    "name": zehn::scan::project_display_name(path),
                }))
                .collect::<Vec<_>>());
            payload["agents"] = json!(ALL_AGENTS
                .into_iter()
                .map(|agent| json!({
                    "agent": agent.label(),
                    "color": agent.hex_color(),
                    "present": cached.index.present_agents().contains(&agent),
                }))
                .collect::<Vec<_>>());
        }
        Ok(payload)
    })
}

fn parse_prompt_key(params: &Map<String, Value>) -> Result<u64, PromptSearchError> {
    let key = params
        .get("key")
        .and_then(Value::as_str)
        .ok_or_else(|| PromptSearchError::invalid("key is required."))?;
    u64::from_str_radix(key.trim(), 16)
        .map_err(|_| PromptSearchError::invalid("key must be a 16-digit hex prompt key."))
}

fn resolve_record_index(
    cached: &CachedIndex,
    params: &Map<String, Value>,
) -> Result<usize, PromptSearchError> {
    let key = parse_prompt_key(params)?;
    cached
        .index
        .find_by_key(key)
        .ok_or_else(PromptSearchError::unknown_prompt)
}

/// `POST /api/readAgentPromptText` — the untruncated prompt for one row.
pub fn read_agent_prompt_text(
    paths: &crate::paths::GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, PromptSearchError> {
    with_index(paths, false, |cached| {
        let index = resolve_record_index(cached, params)?;
        let rec = &cached.index.records[index];
        Ok(json!({
            "key": format!("{:016x}", zehn_index::record_key(rec)),
            "text": rec.text,
        }))
    })
}

/// `POST /api/toggleAgentPromptFavorite`
pub fn toggle_agent_prompt_favorite(
    paths: &crate::paths::GxserverPaths,
    params: &Map<String, Value>,
) -> Result<Value, PromptSearchError> {
    with_index(paths, false, |cached| {
        let index = resolve_record_index(cached, params)?;
        let rec = cached.index.records[index].clone();
        let favorite = match params.get("favorite").and_then(Value::as_bool) {
            Some(next) => cached.index.set_favorite(rec.agent, &rec.text, next),
            None => cached.index.toggle_favorite(rec.agent, &rec.text),
        };
        Ok(json!({
            "key": format!("{:016x}", zehn_index::record_key(&rec)),
            "favorite": favorite,
        }))
    })
}

/// The decision for opening a result, resolved server-side so every host applies
/// the same rule and the same command text.
#[derive(Debug, Clone, PartialEq)]
pub enum PromptLaunchPlan {
    /// A live Ghostex session already owns this agent conversation.
    Focus { project_id: String, session_id: String },
    /// Nothing owns it; run `command` in `cwd` as a new agent session.
    Launch { agent: Agent, command: Vec<String>, cwd: String, cwd_exists: bool, title: String },
}

/// True when a session row is a live owner of `agent_session_id`.
/// Mirrors `isLive` in the CLI session projection: a running lifecycle or a
/// provider that still exists.
fn session_owns_agent_conversation(session: &Value, agent_session_id: &str, agent: Agent) -> bool {
    let runtime_settings = session.get("runtimeSettings");
    let stored = runtime_settings
        .and_then(|settings| settings.get("agentSessionId"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if stored != agent_session_id {
        return false;
    }
    let lifecycle = session.get("lifecycleState").and_then(Value::as_str).unwrap_or_default();
    let provider_state = session
        .get("providerState")
        .and_then(|state| state.get("lifecycleState"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    if lifecycle != "running" && provider_state != "exists" {
        return false;
    }
    let session_agent = session.get("agentId").and_then(Value::as_str).unwrap_or_default();
    session_agent.is_empty() || session_agent.eq_ignore_ascii_case(agent.label())
}

/// `POST /api/resolveAgentPromptLaunch`
///
/// `action` is `"resume"` (default) or `"fork"`. Resume prefers focusing a live
/// owner, exactly like pressing Enter in the terminal picker, so Ghostex never
/// opens a second writer onto one agent conversation.
pub fn resolve_agent_prompt_launch(
    paths: &crate::paths::GxserverPaths,
    params: &Map<String, Value>,
    live_sessions: &[Value],
) -> Result<Value, PromptSearchError> {
    let action = params.get("action").and_then(Value::as_str).unwrap_or("resume");
    let accept_all = params.get("acceptAll").and_then(Value::as_bool).unwrap_or(false);
    let fork_agent = match params.get("forkAgent").and_then(Value::as_str) {
        Some(name) => Some(
            Agent::parse(name.trim())
                .ok_or_else(|| PromptSearchError::invalid(format!("Unknown agent \"{name}\".")))?,
        ),
        None => None,
    };

    with_index(paths, false, |cached| {
        let index = resolve_record_index(cached, params)?;
        let rec = cached.index.records[index].clone();
        let plan = match action {
            "resume" => {
                if rec.session.is_empty() {
                    return Err(PromptSearchError::invalid(format!(
                        "No session id was recorded for this {} prompt, so it cannot be resumed. Fork it instead.",
                        rec.agent.label()
                    )));
                }
                let owner = live_sessions
                    .iter()
                    .find(|session| session_owns_agent_conversation(session, &rec.session, rec.agent));
                match owner {
                    Some(session) => PromptLaunchPlan::Focus {
                        project_id: session
                            .get("projectId")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                        session_id: session
                            .get("sessionId")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_string(),
                    },
                    None => PromptLaunchPlan::Launch {
                        agent: rec.agent,
                        command: rec.agent.resume_argv(&rec.session, accept_all),
                        cwd: rec.project.clone(),
                        cwd_exists: !rec.project.is_empty()
                            && std::path::Path::new(&rec.project).is_dir(),
                        title: rec.display_title().to_string(),
                    },
                }
            }
            "fork" => {
                let agent = fork_agent.unwrap_or(rec.agent);
                PromptLaunchPlan::Launch {
                    agent,
                    command: agent.fresh_session_argv(&rec.text),
                    cwd: rec.project.clone(),
                    cwd_exists: !rec.project.is_empty()
                        && std::path::Path::new(&rec.project).is_dir(),
                    title: rec.display_title().to_string(),
                }
            }
            other => {
                return Err(PromptSearchError::invalid(format!(
                    "Unknown action \"{other}\"; expected \"resume\" or \"fork\"."
                )))
            }
        };
        Ok(launch_plan_payload(&format!("{:016x}", zehn_index::record_key(&rec)), &plan))
    })
}

fn launch_plan_payload(key: &str, plan: &PromptLaunchPlan) -> Value {
    match plan {
        PromptLaunchPlan::Focus { project_id, session_id } => json!({
            "key": key,
            "mode": "focus",
            "projectId": project_id,
            "sessionId": session_id,
        }),
        PromptLaunchPlan::Launch { agent, command, cwd, cwd_exists, title } => json!({
            "key": key,
            "mode": "launch",
            "agent": agent.label(),
            "command": command,
            "commandLine": shell_command_line(command),
            "cwd": cwd,
            "cwdExists": cwd_exists,
            "title": title,
        }),
    }
}

/// Quote an argv into a single POSIX shell command line, so a host that can only
/// type text into a terminal still runs exactly the argv resolved here.
pub fn shell_command_line(argv: &[String]) -> String {
    argv.iter().map(|arg| shell_quote(arg)).collect::<Vec<_>>().join(" ")
}

fn shell_quote(arg: &str) -> String {
    if !arg.is_empty()
        && arg
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'/' | b':' | b'=' | b'@' | b'+' | b','))
    {
        return arg.to_string();
    }
    format!("'{}'", arg.replace('\'', r"'\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shell_quoting_keeps_plain_words_bare_and_escapes_the_rest() {
        assert_eq!(shell_quote("claude"), "claude");
        assert_eq!(shell_quote("--resume"), "--resume");
        assert_eq!(shell_quote("/a/b-c.d"), "/a/b-c.d");
        assert_eq!(shell_quote(""), "''");
        assert_eq!(shell_quote("two words"), "'two words'");
        assert_eq!(shell_quote("it's"), r"'it'\''s'");
        assert_eq!(
            shell_command_line(&["codex".into(), "resume".into(), "a b".into()]),
            "codex resume 'a b'"
        );
    }

    #[test]
    fn text_truncation_never_splits_a_utf8_character() {
        let text = "aé日本語";
        for limit in 0..text.len() + 2 {
            let cut = truncate_on_char_boundary(text, limit);
            assert!(text.starts_with(cut));
            assert!(cut.len() <= limit.max(0) || cut.len() == text.len());
        }
        assert_eq!(truncate_on_char_boundary(text, 2), "a");
        assert_eq!(truncate_on_char_boundary(text, 100), text);
    }

    fn session(agent_session_id: &str, lifecycle: &str, agent: &str) -> Value {
        json!({
            "projectId": "p1",
            "sessionId": "s1",
            "agentId": agent,
            "lifecycleState": lifecycle,
            "runtimeSettings": { "agentSessionId": agent_session_id },
        })
    }

    #[test]
    fn a_live_running_session_owns_the_conversation() {
        assert!(session_owns_agent_conversation(
            &session("abc", "running", "claude"),
            "abc",
            Agent::Claude
        ));
    }

    #[test]
    fn a_stopped_session_does_not_own_the_conversation() {
        assert!(!session_owns_agent_conversation(
            &session("abc", "stopped", "claude"),
            "abc",
            Agent::Claude
        ));
    }

    #[test]
    fn an_existing_provider_counts_as_live_even_when_the_lifecycle_lags() {
        let mut value = session("abc", "stopped", "claude");
        value["providerState"] = json!({ "lifecycleState": "exists" });
        assert!(session_owns_agent_conversation(&value, "abc", Agent::Claude));
    }

    #[test]
    fn a_different_agent_or_conversation_never_matches() {
        assert!(!session_owns_agent_conversation(
            &session("abc", "running", "codex"),
            "abc",
            Agent::Claude
        ));
        assert!(!session_owns_agent_conversation(
            &session("other", "running", "claude"),
            "abc",
            Agent::Claude
        ));
    }

    #[test]
    fn prompt_keys_must_be_hex_and_present() {
        let mut params = Map::new();
        assert_eq!(parse_prompt_key(&params).unwrap_err().code, "invalidParams");
        params.insert("key".to_string(), json!("not-hex"));
        assert_eq!(parse_prompt_key(&params).unwrap_err().code, "invalidParams");
        params.insert("key".to_string(), json!("00000000000000ff"));
        assert_eq!(parse_prompt_key(&params).unwrap(), 0xff);
        params.insert("key".to_string(), json!(" 08775375308e09c0 "));
        assert_eq!(parse_prompt_key(&params).unwrap(), 0x0877_5375_308e_09c0);
    }

    #[test]
    fn isolated_profiles_keep_history_state_inside_the_profile() {
        let mut paths = crate::paths::get_gxserver_paths(None);
        paths.isolated_agent_home_dir = Some(PathBuf::from("/tmp/gx-isolated"));
        let (home, cache_root, favorites_path) = resolve_search_paths(&paths);
        assert_eq!(home, "/tmp/gx-isolated");
        assert_eq!(cache_root, PathBuf::from("/tmp/gx-isolated/.cache/ghostex/zehn"));
        assert_eq!(favorites_path, PathBuf::from("/tmp/gx-isolated/.config/zehn/favorites"));
    }

    #[test]
    fn production_profiles_follow_zehn_own_environment_contract() {
        let mut paths = crate::paths::get_gxserver_paths(None);
        paths.isolated_agent_home_dir = None;
        paths.home_dir = PathBuf::from("/Users/example");
        let (home, _cache_root, favorites_path) = resolve_search_paths(&paths);
        assert_eq!(home, "/Users/example");
        // Matches zehn::favorites::path_for, so `gx f` and the GUI share stars.
        let expected = match std::env::var("XDG_CONFIG_HOME") {
            Ok(dir) if !dir.is_empty() => PathBuf::from(dir).join("zehn").join("favorites"),
            _ => PathBuf::from("/Users/example/.config/zehn/favorites"),
        };
        assert_eq!(favorites_path, expected);
    }

    #[test]
    fn launch_plans_serialize_the_fields_hosts_need() {
        let focus = launch_plan_payload(
            "00000000000000ab",
            &PromptLaunchPlan::Focus { project_id: "p".into(), session_id: "s".into() },
        );
        assert_eq!(focus["mode"], "focus");
        assert_eq!(focus["projectId"], "p");
        assert_eq!(focus["key"], "00000000000000ab");

        let launch = launch_plan_payload(
            "00000000000000cd",
            &PromptLaunchPlan::Launch {
                agent: Agent::Codex,
                command: vec!["codex".into(), "resume".into(), "id".into()],
                cwd: "/tmp/x".into(),
                cwd_exists: false,
                title: "T".into(),
            },
        );
        assert_eq!(launch["mode"], "launch");
        assert_eq!(launch["agent"], "codex");
        assert_eq!(launch["commandLine"], "codex resume id");
        assert_eq!(launch["cwdExists"], false);
    }
}
