//! The Live Activity's content, built here from the same per-session rows the phone reads (the mobile summary
//! rows, `ghostex_cli::sessions::mobile_summary_for_session`), by the same rules as the phone's
//! `apps/mobile/app/src/notifications/contentState.ts` and the agent names, artwork and tints of
//! `apps/mobile/app/src/contract/mobileSummary/agentIcons.ts`. The JSON is what the app's
//! `GhostexActivityContent` decodes.
//!
//! CDXC:Notifications 2026-11-11 WHY: While the phone is locked only a push keeps its banner current, and the
//! push must carry exactly what the app would have shown: counts, up to four rows, "+N more", and each row's
//! leading name by that phone's "Show session names" switch (off: the agent's name; on: the session's title,
//! or the agent's name for a session without one). That is the push titles' privacy rule
//! (`push::message`): no session title reaches a phone that keeps names off. No prompt, reply or tool text
//! is ever part of it.

use std::collections::{HashMap, HashSet};

use serde_json::{json, Map, Value};

/// Four rows plus "+N more".
pub(crate) const MAX_ROWS: usize = 4;
/// A finished agent stays a row this long after it finished.
pub(crate) const FINISHED_ROW_MS: i64 = 120_000;
/// Names and projects are cut to this many characters.
const MAX_NAME_CHARS: usize = 64;
/// The content state stays well inside Apple's 4 KB push with the relay's own fields.
const CONTENT_BYTES_MAX: usize = 3_500;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RowState {
    Waiting,
    Working,
    Finished,
}

impl RowState {
    fn wire(self) -> &'static str {
        match self {
            Self::Waiting => "waiting",
            Self::Working => "working",
            Self::Finished => "finished",
        }
    }

    fn rank(self) -> u8 {
        match self {
            Self::Waiting => 0,
            Self::Working => 1,
            Self::Finished => 2,
        }
    }
}

/// A row before it knows which phone it is for: both names, so each phone's switch picks one.
#[derive(Clone, Debug)]
pub(crate) struct Shown {
    /// The computer the session runs on (its server id) and its name.
    pub(crate) server_id: String,
    pub(crate) computer: String,
    pub(crate) session_id: String,
    pub(crate) agent_name: String,
    /// The session's title as the Sessions list shows it ('' for none).
    pub(crate) session_name: String,
    pub(crate) agent_icon: String,
    pub(crate) agent_tint: u32,
    pub(crate) project: String,
    pub(crate) state: RowState,
    pub(crate) started_at_ms: i64,
    pub(crate) finished_at_ms: Option<i64>,
    pub(crate) has_question: bool,
    /// Working, waiting: the start; finished: the finish.
    sort_time: i64,
}

/// A working, waiting or recently finished agent; `shown` is None when it cannot have a row (no start time,
/// or identifiers outside the deep link's pattern), and it then counts in "+N more".
#[derive(Clone, Debug)]
pub(crate) struct Candidate {
    pub(crate) state: RowState,
    pub(crate) shown: Option<Shown>,
    pub(crate) finished_at_ms: Option<i64>,
}

/// The finished events seen, by computer and session: each stays a row for its 2 minutes also once the
/// computer drops the session's attention (an acknowledgement on any device).
pub(crate) type FinishedMemory = HashMap<String, Candidate>;

/// One pass's candidates, the same for every phone; `render` makes one phone's content from them.
#[derive(Clone, Debug, Default)]
pub(crate) struct Candidates {
    pub(crate) list: Vec<Candidate>,
    pub(crate) working: usize,
    pub(crate) waiting: usize,
    /// One key per working agent: its computer, its session and the start of its stint. A key the last pass
    /// did not have is a working stint that began.
    pub(crate) working_stints: HashSet<String>,
    /// When each computer's list was read (ms).
    pub(crate) read_at: HashMap<String, i64>,
    /// The computers with an agent on the banner (a working, waiting or recently finished candidate).
    pub(crate) shown_computers: HashSet<String>,
}

/// One computer's session list as the phone's summary carries it: this computer's, or a connected
/// computer's read over the connection to it.
pub(crate) struct Source<'a> {
    pub(crate) server_id: &'a str,
    /// Its name, which a row carries when the banner shows more than one computer.
    pub(crate) computer: &'a str,
    pub(crate) rows: &'a [Value],
    pub(crate) launchers: &'a [Value],
    pub(crate) read_at: i64,
}

impl Candidates {
    pub(crate) fn active(&self) -> bool {
        self.working + self.waiting > 0
    }
}

fn truncate(value: &str) -> String {
    value.chars().take(MAX_NAME_CHARS).collect()
}

fn text<'a>(row: &'a Value, key: &str) -> &'a str {
    row.get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .unwrap_or("")
}

fn first_non_empty<'a>(values: &[&'a str]) -> &'a str {
    values
        .iter()
        .copied()
        .find(|value| !value.is_empty())
        .unwrap_or("")
}

fn host_time(value: &str) -> Option<i64> {
    if value.is_empty() {
        return None;
    }
    chrono::DateTime::parse_from_rfc3339(value)
        .ok()
        .map(|time| time.timestamp_millis())
}

fn iso(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms)
        .unwrap_or_default()
        .to_rfc3339_opts(chrono::SecondsFormat::Millis, true)
}

/// The deep link's id pattern: printable ASCII, 1 to 64 characters.
fn routable(value: &str) -> bool {
    (1..=64).contains(&value.len()) && value.bytes().all(|byte| (0x21..=0x7e).contains(&byte))
}

// ---------------------------------------------------------------------------------------------------------
// Agent names, artwork and tints (the phone's agentIcons.ts).
// ---------------------------------------------------------------------------------------------------------

/// Icon id, tint (0xRRGGBB) and the built-in name, as the phone's registry.
const AGENT_ICONS: &[(&str, u32, Option<&str>)] = &[
    ("amp-cli", 0xFFFFFF, Some("Amp CLI")),
    ("antigravity-cli", 0x749BFF, Some("Antigravity CLI")),
    ("browser", 0x82B7FF, None),
    ("claude", 0xD97757, Some("Claude")),
    ("codebuddy", 0x72D6FF, Some("CodeBuddy")),
    ("cursor-cli", 0xEDECEC, Some("Cursor CLI")),
    ("codex", 0xFFFFFF, Some("Codex")),
    ("copilot", 0xFFFFFF, Some("Copilot")),
    ("dsh", 0x4D6BFE, Some("DeepSeek Harness")),
    ("empryo", 0x1FA31D, Some("Empryo")),
    ("factory-droid", 0xFF7A1A, Some("Factory Droid")),
    ("freebuff", 0xFFFFFF, Some("Freebuff")),
    ("grok-build", 0xFFFFFF, Some("Grok Build")),
    ("hermes-agent", 0xF3C46B, Some("Hermes Agent")),
    ("kiro", 0xA6E3FF, Some("Kiro CLI")),
    ("omp", 0xC8FF62, Some("OMP")),
    ("openclaude", 0xF0A68A, Some("OpenClaude")),
    ("opencode", 0x6D96C0, Some("OpenCode")),
    ("pi", 0xC8FF62, Some("Pi Agent")),
    ("qoder", 0xA991FF, Some("Qoder")),
    ("rovo-dev", 0x4FC3A1, Some("Rovo Dev")),
    ("zcode", 0xFFFFFF, Some("ZCode")),
    ("terminal", 0xFAFAFA, None),
];
const CLAUDE_CLOUD_TINT: u32 = 0xD67556;

/// Lowercased names and aliases to icon ids.
const AGENT_NAME_ALIASES: &[(&str, &str)] = &[
    ("codex", "codex"),
    ("codex cli", "codex"),
    ("claude", "claude"),
    ("claude code", "claude"),
    ("claude-cloud", "claude"),
    ("claude (cloud)", "claude"),
    ("openclaude", "openclaude"),
    ("open claude", "openclaude"),
    ("cursor", "cursor-cli"),
    ("cursor cli", "cursor-cli"),
    ("cursor agent", "cursor-cli"),
    ("cursor-agent", "cursor-cli"),
    ("pi", "pi"),
    ("pi agent", "pi"),
    ("π", "pi"),
    ("opencode", "opencode"),
    ("open code", "opencode"),
    ("copilot", "copilot"),
    ("github copilot", "copilot"),
    ("droid", "factory-droid"),
    ("factory droid", "factory-droid"),
    ("grok", "grok-build"),
    ("grok build", "grok-build"),
    ("antigravity", "antigravity-cli"),
    ("antigravity cli", "antigravity-cli"),
    ("agy", "antigravity-cli"),
    ("amp", "amp-cli"),
    ("amp cli", "amp-cli"),
    ("hermes", "hermes-agent"),
    ("hermes agent", "hermes-agent"),
    ("codebuddy", "codebuddy"),
    ("code buddy", "codebuddy"),
    ("kiro", "kiro"),
    ("omp", "omp"),
    ("oh my pi", "omp"),
    ("qoder", "qoder"),
    ("rovo", "rovo-dev"),
    ("rovo dev", "rovo-dev"),
    ("rovo-dev", "rovo-dev"),
    ("zcode", "zcode"),
    ("zcode-cli", "zcode"),
    ("z code", "zcode"),
    ("freebuff", "freebuff"),
    ("dsh", "dsh"),
    ("dsh-tui", "dsh"),
    ("deepseek", "dsh"),
    ("deepseek harness", "dsh"),
    ("browser", "browser"),
];

fn known_icon(value: &str) -> Option<&'static str> {
    AGENT_ICONS
        .iter()
        .find(|(id, _, _)| *id == value)
        .map(|(id, _, _)| *id)
}

fn alias(value: &str) -> Option<&'static str> {
    AGENT_NAME_ALIASES
        .iter()
        .find(|(name, _)| *name == value)
        .map(|(_, id)| *id)
}

pub(crate) fn resolve_icon_id(agent_icon: &str, agent_name: &str) -> &'static str {
    let icon = agent_icon.trim().to_lowercase();
    if !icon.is_empty() {
        if let Some(id) = known_icon(&icon).or_else(|| alias(&icon)) {
            return id;
        }
    }
    let name = agent_name.trim().to_lowercase();
    if !name.is_empty() {
        if let Some(id) = alias(&name).or_else(|| known_icon(&name)) {
            return id;
        }
    }
    "terminal"
}

/// The artwork id: the icon id, except the Claude (cloud) agent's orange cloud.
pub(crate) fn resolve_art_id(agent_icon: &str, agent_name: &str) -> &'static str {
    let icon = agent_icon.trim().to_lowercase();
    let name = agent_name.trim().to_lowercase();
    if icon == "claude-cloud"
        || (icon.is_empty() && (name == "claude-cloud" || name == "claude (cloud)"))
    {
        return "claude-cloud";
    }
    resolve_icon_id(agent_icon, agent_name)
}

pub(crate) fn art_tint(art_id: &str) -> u32 {
    if art_id == "claude-cloud" {
        return CLAUDE_CLOUD_TINT;
    }
    AGENT_ICONS
        .iter()
        .find(|(id, _, _)| *id == art_id)
        .map(|(_, tint, _)| *tint)
        .unwrap_or(0xFAFAFA)
}

fn is_custom_agent_id(value: &str) -> bool {
    value.trim().to_lowercase().starts_with("custom-")
}

fn icon_name(icon: &str) -> Option<&'static str> {
    AGENT_ICONS
        .iter()
        .find(|(id, _, _)| *id == icon)
        .and_then(|(_, _, name)| *name)
}

/// A built-in agent's id as the app names it ("claude" is "Claude"); any other name as it is.
fn built_in_agent_name(name: &str) -> String {
    let id = name.to_lowercase();
    if name != id || id == "claude-cloud" || id == "claude (cloud)" {
        return name.to_string();
    }
    known_icon(&id)
        .or_else(|| alias(&id))
        .and_then(icon_name)
        .map(str::to_string)
        .unwrap_or_else(|| name.to_string())
}

/// The name a row shows for a session's agent: its launcher's name, else the session's own agent name or id
/// (a built-in id as the app names it), else its base agent's; '' for a plain terminal.
pub(crate) fn session_agent_name(row: &Value, launchers: &[Value]) -> String {
    let agent = text(row, "agent");
    let agent_name = text(row, "agentName");
    let launcher_name = launchers
        .iter()
        .find(|launcher| text(launcher, "agentId") == agent && !agent.is_empty())
        .map(|launcher| text(launcher, "name"))
        .unwrap_or("");
    if !launcher_name.is_empty() && !is_custom_agent_id(launcher_name) {
        return launcher_name.to_string();
    }
    if let Some(named) = [agent_name, agent]
        .into_iter()
        .find(|name| !name.is_empty() && !is_custom_agent_id(name))
    {
        return built_in_agent_name(named);
    }
    if agent.is_empty() && agent_name.is_empty() {
        return String::new();
    }
    let icon = resolve_icon_id(text(row, "agentIcon"), "");
    let base = launchers.iter().find(|launcher| {
        let id = text(launcher, "agentId");
        !is_custom_agent_id(id) && resolve_icon_id(text(launcher, "icon"), id) == icon
    });
    base.map(|launcher| text(launcher, "name"))
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .or_else(|| icon_name(icon).map(str::to_string))
        .unwrap_or_else(|| "Agent".to_string())
}

// ---------------------------------------------------------------------------------------------------------
// Candidates and content (the phone's contentState.ts).
// ---------------------------------------------------------------------------------------------------------

/// The candidate a mobile session row makes at `now` (ms), or None (not an agent, idle, or a finish older
/// than 2 minutes). `awaitingAnswer` counts: this computer announces it.
pub(crate) fn candidate_for(row: &Value, launchers: &[Value], now: i64) -> Option<Candidate> {
    let agent_name = truncate(&session_agent_name(row, launchers));
    if agent_name.is_empty() {
        return None;
    }
    let activity = text(row, "activity");
    let attention = row.get("attention").filter(|value| value.is_object());
    let attention_text = |key: &str| attention.map(|value| text(value, key)).unwrap_or("");
    let attention_kind = match attention_text("kind") {
        kind @ ("finished" | "needsInput") => kind,
        _ => "",
    };
    let in_attention = activity == "attention";
    let pending_questions = row
        .get("pendingQuestionCount")
        .and_then(Value::as_f64)
        .filter(|count| count.is_finite())
        .unwrap_or(0.0);
    let flag = |key: &str| row.get(key).and_then(Value::as_bool).unwrap_or(false);
    let question = pending_questions >= 1.0 || flag("awaitingAnswer");
    let awaiting_approval = flag("awaitingApproval");
    let waits_on_user = awaiting_approval || question;
    let agent = text(row, "agent");
    let art = resolve_art_id(
        text(row, "agentIcon"),
        first_non_empty(&[text(row, "agentName"), agent]),
    );
    let session_id = text(row, "sessionId").to_string();
    let shown_with = |state: RowState, started: i64, finished: Option<i64>, sort_time: i64| Shown {
        server_id: String::new(),
        computer: String::new(),
        session_id: session_id.clone(),
        agent_name: agent_name.clone(),
        session_name: truncate(first_non_empty(&[
            text(row, "displayTitle"),
            text(row, "title"),
        ])),
        agent_icon: art.to_string(),
        agent_tint: art_tint(art),
        project: truncate(first_non_empty(&[
            text(row, "projectName"),
            text(row, "groupTitle"),
        ])),
        state,
        started_at_ms: started,
        finished_at_ms: finished,
        has_question: state == RowState::Waiting && question,
        sort_time,
    };
    let can_route = routable(&session_id);

    if in_attention && attention_kind == "finished" && !waits_on_user {
        let finished_at = host_time(attention_text("enteredAt"))?;
        if now - finished_at >= FINISHED_ROW_MS {
            return None;
        }
        let work_start = host_time(attention_text("workingStartedAt"));
        let shown = match work_start {
            Some(start) if can_route && start <= finished_at => Some(shown_with(
                RowState::Finished,
                start,
                Some(finished_at),
                finished_at,
            )),
            _ => None,
        };
        return Some(Candidate {
            state: RowState::Finished,
            shown,
            finished_at_ms: Some(finished_at),
        });
    }
    if in_attention || awaiting_approval {
        let since = host_time(attention_text("enteredAt")).filter(|_| can_route);
        return Some(Candidate {
            state: RowState::Waiting,
            shown: since.map(|since| shown_with(RowState::Waiting, since, None, since)),
            finished_at_ms: None,
        });
    }
    if activity == "working" {
        let since = host_time(text(row, "workingStartedAt")).filter(|_| can_route);
        return Some(Candidate {
            state: RowState::Working,
            shown: since.map(|since| shown_with(RowState::Working, since, None, since)),
            finished_at_ms: None,
        });
    }
    None
}

/// Every candidate over the computers' rows at `now`, with the finished ones `memory` keeps (updated here; a
/// computer not read this pass keeps its remembered finishes out, as the phone does).
pub(crate) fn collect_candidates(
    sources: &[Source<'_>],
    now: i64,
    memory: &mut FinishedMemory,
) -> Candidates {
    let mut candidates = Candidates::default();
    for source in sources {
        candidates
            .read_at
            .insert(source.server_id.to_string(), source.read_at);
        for row in source.rows {
            let session_id = text(row, "sessionId");
            if session_id.is_empty() {
                continue;
            }
            let key = format!("{}\n{session_id}", source.server_id);
            let mut candidate = candidate_for(row, source.launchers, now);
            if let Some(shown) = candidate
                .as_mut()
                .and_then(|candidate| candidate.shown.as_mut())
            {
                shown.server_id = source.server_id.to_string();
                shown.computer = source.computer.to_string();
            }
            if let Some(candidate) = candidate
                .as_ref()
                .filter(|candidate| candidate.state == RowState::Finished)
            {
                memory.insert(key, candidate.clone());
                continue;
            }
            // Working, waiting or in another attention: a finish remembered for this session is over.
            if candidate.is_some() || text(row, "activity") == "attention" {
                memory.remove(&key);
            }
            let Some(candidate) = candidate else { continue };
            match candidate.state {
                RowState::Working => {
                    candidates.working += 1;
                    candidates
                        .working_stints
                        .insert(format!("{key}\n{}", text(row, "workingStartedAt")));
                }
                RowState::Waiting => candidates.waiting += 1,
                RowState::Finished => {}
            }
            candidates
                .shown_computers
                .insert(source.server_id.to_string());
            candidates.list.push(candidate);
        }
    }
    memory.retain(|_, candidate| {
        candidate
            .finished_at_ms
            .is_some_and(|finished| now - finished < FINISHED_ROW_MS)
    });
    for (key, candidate) in memory.iter() {
        let server_id = key.split('\n').next().unwrap_or_default();
        if candidates.read_at.contains_key(server_id) {
            candidates.shown_computers.insert(server_id.to_string());
            candidates.list.push(candidate.clone());
        }
    }
    candidates
}

/// The data time: when the oldest list with an agent on the banner was read; with none, the newest read;
/// `now` when nothing was read (the phone's `dataTimeOf`).
fn data_time(candidates: &Candidates, now: i64) -> i64 {
    let shown = candidates
        .read_at
        .iter()
        .filter(|(server_id, _)| candidates.shown_computers.contains(*server_id))
        .map(|(_, at)| *at)
        .min();
    shown
        .or_else(|| candidates.read_at.values().copied().max())
        .unwrap_or(now)
}

fn compare(a: &Shown, b: &Shown) -> std::cmp::Ordering {
    a.state
        .rank()
        .cmp(&b.state.rank())
        .then_with(|| {
            if a.state == RowState::Finished {
                b.sort_time.cmp(&a.sort_time)
            } else {
                a.sort_time.cmp(&b.sort_time)
            }
        })
        .then_with(|| a.session_id.cmp(&b.session_id))
        .then_with(|| a.server_id.cmp(&b.server_id))
}

/// One phone's content. `target` gives the id that phone uses for a computer (its rows open sessions through
/// it): this computer's is the phone's registration id, another computer's is what the phone said it calls
/// it, and a computer the phone does not know gets '' (a tap then opens the app). `show_session_names` is the
/// phone's switch. A row names its computer (`computer`) when the banner shows more than one; the app's own
/// builder has no such field yet, and an app that does not read it draws the row as before.
pub(crate) fn render(
    candidates: &Candidates,
    target: &dyn Fn(&str) -> Option<String>,
    show_session_names: bool,
    now: i64,
) -> Value {
    let mut shown: Vec<&Shown> = candidates
        .list
        .iter()
        .filter_map(|candidate| candidate.shown.as_ref())
        .collect();
    shown.sort_by(|a, b| compare(a, b));
    let several = candidates.shown_computers.len() > 1;
    let rows: Vec<Value> = shown
        .iter()
        .take(MAX_ROWS)
        .map(|shown| {
            let leading = if show_session_names && !shown.session_name.is_empty() {
                &shown.session_name
            } else {
                &shown.agent_name
            };
            let machine_id = target(&shown.server_id)
                .filter(|id| routable(id))
                .unwrap_or_default();
            let mut row = Map::new();
            row.insert(
                "target".into(),
                json!({ "machineId": machine_id, "sessionId": shown.session_id }),
            );
            row.insert("agentName".into(), json!(leading));
            row.insert("agentIcon".into(), json!(shown.agent_icon));
            row.insert("agentTint".into(), json!(shown.agent_tint));
            row.insert("project".into(), json!(shown.project));
            if several {
                row.insert("computer".into(), json!(truncate(&shown.computer)));
            }
            row.insert("state".into(), json!(shown.state.wire()));
            row.insert("startedAt".into(), json!(iso(shown.started_at_ms)));
            if let Some(finished) = shown.finished_at_ms {
                row.insert("finishedAt".into(), json!(iso(finished)));
                row.insert("expiresAt".into(), json!(iso(finished + FINISHED_ROW_MS)));
            }
            if shown.has_question {
                row.insert("hasQuestion".into(), json!(true));
            }
            Value::Object(row)
        })
        .collect();
    fit(json!({
        "updatedAt": iso(data_time(candidates, now)),
        "counts": { "working": candidates.working, "waiting": candidates.waiting },
        "moreCount": candidates.list.len() - rows.len(),
        "agents": rows,
    }))
}

/// Drops rows from the end into "+N more" until the content fits (the app does the same for its own).
fn fit(mut content: Value) -> Value {
    while content.to_string().len() > CONTENT_BYTES_MAX {
        let Some(agents) = content["agents"].as_array_mut() else {
            break;
        };
        if agents.pop().is_none() {
            break;
        }
        let more = content["moreCount"].as_u64().unwrap_or(0) + 1;
        content["moreCount"] = json!(more);
    }
    content
}

/// The content without its data time, to tell a change from a newer read of the same content.
pub(crate) fn shape_of(content: &Value) -> String {
    let mut shape = content.clone();
    if let Some(map) = shape.as_object_mut() {
        map.remove("updatedAt");
    }
    shape.to_string()
}
