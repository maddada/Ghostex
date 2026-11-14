//! The `sessionDetails` document value: every section of the panel, worded.
//!
//! `null` while the panel does not apply (the extension is off, or the chat has no agent yet).
//! Claude's panel has every section; another agent's has the ones its data fills, and a section
//! with nothing to show is left out rather than drawn empty.
//! Otherwise `{enabled, visible, ring, loading, error, sections}`, where `sections` is the ordered
//! list the renderers draw as cards, each `{id, title, meta, collapsed, …}` with its own rows. A row
//! that opens something carries the action to send back (`sessionDetailsOpen`, `…Stop`, …).

use serde_json::{json, Map, Value};

use crate::extras::activity::format_activity_elapsed;
use crate::extras::agent_tasks::agent_task_panel;
use crate::extras::session_details::settle::{claude, enabled};
use crate::extras::session_details::state::is_collapsed;
use crate::extras::time::parse_iso_millis;
use crate::menus::context::usage::{format_context_percentage, format_context_tokens};
use crate::state::{ChatContext, ChatState};

/// The live context use: `(used tokens, window size, used percentage)`.
struct Usage {
    used: Option<f64>,
    window: Option<f64>,
    percent: Option<f64>,
}

fn number(value: Option<&Value>) -> Option<f64> {
    value
        .and_then(Value::as_f64)
        .filter(|value| value.is_finite())
}

/// The statusline's numbers the context meter shows, else the ones the panel's read carried.
fn live_usage(state: &ChatState, data: Option<&Value>) -> Usage {
    let reported = state
        .session
        .selected_options
        .as_ref()
        .and_then(|options| options.get("contextUsage"));
    let served = data.and_then(|data| data.pointer("/context/live"));
    let pick = |key: &str| {
        number(reported.and_then(|usage| usage.get(key)))
            .or_else(|| number(served.and_then(|usage| usage.get(key))))
    };
    let used = pick("usedTokens");
    let window = pick("windowSize");
    let percent = match (used, window) {
        (Some(used), Some(window)) if window > 0.0 => Some(used / window * 100.0),
        _ => pick("usedPercentage"),
    };
    Usage {
        used,
        window,
        percent,
    }
}

fn percent_label(tokens: f64, window: Option<f64>) -> Option<String> {
    let window = window.filter(|window| *window > 0.0)?;
    let percent = tokens / window * 100.0;
    if tokens > 0.0 && percent < 0.05 {
        return Some("<0.1%".to_string());
    }
    format_context_percentage(Some(percent))
}

fn ago(now_ms: f64, at_ms: f64) -> String {
    let seconds = ((now_ms - at_ms) / 1_000.0).max(0.0);
    if seconds < 10.0 {
        "just now".to_string()
    } else if seconds < 60.0 {
        format!("{}s ago", seconds as i64)
    } else if seconds < 3_600.0 {
        format!("{}m ago", (seconds / 60.0) as i64)
    } else if seconds < 86_400.0 {
        format!("{}h ago", (seconds / 3_600.0) as i64)
    } else {
        format!("{}d ago", (seconds / 86_400.0) as i64)
    }
}

fn text(value: &Value, pointer: &str) -> Option<String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn open_action(target: &str, extra: Value) -> Value {
    let mut action = Map::new();
    action.insert("type".into(), json!("sessionDetailsOpen"));
    action.insert("target".into(), json!(target));
    if let Value::Object(extra) = extra {
        action.extend(extra);
    }
    Value::Object(action)
}

fn section(id: &str, title: &str, collapsed: &[String], body: Value) -> Value {
    let mut section = Map::new();
    section.insert("id".into(), json!(id));
    section.insert("title".into(), json!(title));
    section.insert("collapsed".into(), json!(is_collapsed(collapsed, id)));
    if let Value::Object(body) = body {
        section.extend(body);
    }
    Value::Object(section)
}

/// A card still waiting for the panel's first answer: its title and `rows` skeleton rows, about
/// what it usually holds, so the panel keeps its height when the real rows replace them.
fn loading_section(id: &str, title: &str, collapsed: &[String], rows: usize) -> Value {
    section(
        id,
        title,
        collapsed,
        json!({"loading": true, "skeletonRows": rows}),
    )
}

/// The breakdown's usual length (its categories and the deferred tools), sketched while Claude
/// counts it for the first time.
const BREAKDOWN_SKELETON_ROWS: usize = 11;

/// How many characters the account row fits on its one line in the panel's width.
const ACCOUNT_ROW_CHARS: usize = 34;

/// `email` cut to `max` characters by dropping the middle of its name, so its domain stays
/// (`sharptabs.co…@gmail.com`), or its middle when even the domain is too long.
fn shorten_email(email: &str, max: usize) -> String {
    let characters: Vec<char> = email.chars().collect();
    if characters.len() <= max {
        return email.to_string();
    }
    if let Some((name, domain)) = email.rsplit_once('@') {
        let room = max.saturating_sub(domain.chars().count() + 2);
        if room >= 4 {
            let head: String = name.chars().take(room).collect();
            return format!("{head}\u{2026}@{domain}");
        }
    }
    let head = (max - 1) / 2;
    let tail = max - 1 - head;
    let start: String = characters[..head].iter().collect();
    let end: String = characters[characters.len() - tail..].iter().collect();
    format!("{start}\u{2026}{end}")
}

/// CDXC:SessionDetails 2026-10-25 DECISION:
/// User: the account row shows the account's email with its short tag ("sharptabs.com@gmail.com (ST)") instead of "account 5 (ST)", which stays only while Ghostex knows no email for that account. A long email loses the middle of its name so the row stays one line with its tag, and the tooltip holds the whole email. Hide Account Emails masks it, like the chat's other account text.
///
/// CDXC:SessionDetails 2026-10-29 DECISION:
/// User: "pls remove (ST) here". The row shows the email alone, using the whole line; "account 5 (ST)" stays only while Ghostex knows no email for that account.
fn account_row(environment: &Value, hide_emails: bool) -> Option<Value> {
    let Some(email) = text(environment, "/accountEmail") else {
        return text(environment, "/account")
            .map(|account| json!({"icon": "user", "text": account}));
    };
    let email = crate::menus::native_accounts::account_text(hide_emails, &email);
    Some(json!({
        "icon": "user",
        "text": shorten_email(&email, ACCOUNT_ROW_CHARS),
        "tooltip": email,
    }))
}

/// "640 MB RAM", "1.2 GB RAM": the Resources panel's units, with one decimal from a gigabyte on.
fn ram_label(bytes: f64) -> String {
    let mb = bytes / (1024.0 * 1024.0);
    if mb >= 1024.0 {
        format!("{:.1} GB RAM", mb / 1024.0)
    } else {
        format!("{:.0} MB RAM", mb.max(1.0))
    }
}

/// CDXC:SessionDetails 2026-11-02 DECISION:
/// User: show the session's RAM usage in the Environment card ("1.2 GB RAM"), the Resources panel's number for the session's whole process tree, kept current with the panel's poll. The row is left out when no number is known (the session isn't running, or it runs on a Cloud Box or another computer that can't report it), never shown as 0 or a dash.
fn environment(data: &Value, hide_emails: bool) -> Option<Value> {
    let environment = data.get("environment")?;
    let mut rows = Vec::new();
    let location = text(environment, "/location/label").unwrap_or_else(|| "This computer".into());
    let boxed = environment
        .pointer("/location/kind")
        .and_then(Value::as_str)
        == Some("box");
    rows.push(json!({"icon": if boxed { "box" } else { "laptop" }, "text": location}));
    if let Some(folder) = text(environment, "/folderLabel") {
        let project = text(environment, "/project");
        rows.push(json!({
            "icon": "folder",
            "text": match project { Some(project) => format!("{project} · {folder}"), None => folder },
            "tooltip": text(environment, "/folder"),
            "badge": (environment.get("worktree") == Some(&Value::Bool(true))).then_some("worktree"),
        }));
    }
    if let Some(branch) = text(data, "/changes/branch") {
        rows.push(json!({"icon": "branch", "text": branch}));
    }
    if let Some(row) = account_row(environment, hide_emails) {
        rows.push(row);
    }
    if let Some(bytes) = number(environment.get("memoryBytes")).filter(|bytes| *bytes > 0.0) {
        rows.push(json!({"icon": "memory", "text": ram_label(bytes)}));
    }
    Some(json!({ "rows": rows }))
}

/// The colour key of a breakdown category, which the renderers map to their palette.
fn category_color(id: &str) -> &'static str {
    match id {
        "messages" => "messages",
        "memory_files" => "memory",
        "skills" => "skills",
        "mcp_instructions" | "mcp_tools" => "mcp",
        "custom_agents" => "agents",
        "autocompact_buffer" => "buffer",
        "free_space" => "free",
        _ => "system",
    }
}

/// Used categories first in the panel's order, then whatever else Claude counted.
fn category_rank(id: &str) -> usize {
    [
        "messages",
        "memory_files",
        "skills",
        "system_tools",
        "system_prompt",
        "custom_agents",
        "mcp_instructions",
        "mcp_tools",
    ]
    .iter()
    .position(|known| *known == id)
    .unwrap_or(50)
}

fn context_card(
    state: &ChatState,
    data: Option<&Value>,
    usage: &Usage,
    now_ms: f64,
    utc_offset_minutes: i32,
) -> Value {
    let details = &state.extras.session_details;
    let claude = claude(state);
    let served = data.and_then(|data| data.get("context"));
    let breakdown = served
        .and_then(|context| context.get("breakdown"))
        .filter(|breakdown| breakdown.is_object());
    let window = usage
        .window
        .or_else(|| number(breakdown.and_then(|breakdown| breakdown.get("maxTokens"))));
    let used = usage
        .used
        .or_else(|| number(breakdown.and_then(|breakdown| breakdown.get("totalTokens"))));
    let percent = usage.percent.or_else(|| match (used, window) {
        (Some(used), Some(window)) if window > 0.0 => Some(used / window * 100.0),
        _ => None,
    });
    let fraction = |tokens: f64| match window {
        Some(window) if window > 0.0 => (tokens / window).clamp(0.0, 1.0),
        _ => 0.0,
    };
    let mut rows = Vec::new();
    let mut deferred = Vec::new();
    let mut segments = Vec::new();
    let mut messages = Vec::new();
    if let Some(breakdown) = breakdown {
        let mut categories: Vec<&Value> = breakdown["categories"]
            .as_array()
            .into_iter()
            .flatten()
            .collect();
        categories
            .sort_by_key(|category| category_rank(category["id"].as_str().unwrap_or_default()));
        let counted = number(breakdown.get("totalTokens")).unwrap_or(0.0);
        // What the conversation added since Claude last counted, from the live total.
        let since = used
            .map(|used| used - counted)
            .filter(|since| *since >= 500.0);
        let mut tail = Vec::new();
        for category in categories {
            let id = category["id"].as_str().unwrap_or_default();
            let tokens = number(category.get("tokens")).unwrap_or(0.0);
            let name = category["name"].as_str().unwrap_or(id);
            let row = json!({
                "id": id,
                "label": name,
                "tokens": format_context_tokens(Some(tokens)),
                "percent": percent_label(tokens, window),
                "color": category_color(id),
            });
            match category["kind"].as_str() {
                Some("used") => {
                    if tokens > 0.0 {
                        segments.push(
                            json!({"color": category_color(id), "fraction": fraction(tokens)}),
                        );
                    }
                    rows.push(row);
                }
                Some("deferred") => deferred.push(row),
                // The buffer and the free space close the list, dimmed.
                _ => {
                    let mut row = row;
                    row["dim"] = json!(true);
                    if id == "free_space" {
                        if let Some(since) = since {
                            let free = (tokens - since).max(0.0);
                            row["tokens"] = json!(format_context_tokens(Some(free)));
                            row["percent"] = json!(percent_label(free, window));
                        }
                    }
                    tail.push(row);
                }
            }
        }
        if let Some(since) = since {
            segments.push(json!({"color": "since", "fraction": fraction(since)}));
            rows.push(json!({
                "id": "since_last_count",
                "label": "Since last update",
                "tokens": format_context_tokens(Some(since)),
                "percent": percent_label(since, window),
                "color": "since",
            }));
        }
        rows.extend(tail);
        let parts = &breakdown["messages"];
        let mut tools: Vec<(String, f64)> = parts["toolCalls"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|tool| {
                Some((
                    tool["name"].as_str()?.to_string(),
                    number(tool.get("tokens")).unwrap_or(0.0),
                ))
            })
            .collect();
        tools.sort_by(|a, b| b.1.total_cmp(&a.1));
        for (name, tokens) in tools.into_iter().take(5) {
            messages.push(json!({"label": format!("{name} calls"), "tokens": format_context_tokens(Some(tokens))}));
        }
        for (label, key) in [
            ("Claude's replies", "assistantTokens"),
            ("Your messages", "userTokens"),
            ("Attachments", "attachmentTokens"),
        ] {
            if let Some(tokens) = number(parts.get(key)).filter(|tokens| *tokens > 0.0) {
                messages
                    .push(json!({"label": label, "tokens": format_context_tokens(Some(tokens))}));
            }
        }
    } else if let Some(used) = used {
        segments.push(json!({"color": "messages", "fraction": fraction(used)}));
    }
    let running =
        served.and_then(|context| context.get("breakdownRunning")) == Some(&Value::Bool(true));
    let error = served.and_then(|context| text(context, "/breakdownError"));
    let breakdown_coming = claude
        && breakdown.is_none()
        && (running || details.refreshing || (data.is_none() && details.error.is_none()));
    let counted_at = served
        .and_then(|context| text(context, "/breakdownAt"))
        .and_then(|at| parse_iso_millis(&at, utc_offset_minutes));
    let status = if !claude {
        String::new()
    } else if running || details.refreshing {
        "Counting with Claude…".to_string()
    } else if let Some(error) = error.as_ref().filter(|_| breakdown.is_none()) {
        format!("Couldn't count: {error}")
    } else if let Some(at) = counted_at {
        format!("Breakdown from {}", ago(now_ms, at))
    } else if data.is_some() {
        "Breakdown not counted yet".to_string()
    } else {
        String::new()
    };
    // An agent that names no context window (OpenCode on an unknown model, Antigravity) shows
    // the tokens it used in place of the percentage.
    let percent_label = format_context_percentage(percent)
        .map(|percent| format!("{percent} used"))
        .or_else(|| {
            used.filter(|_| !claude)
                .map(|used| format!("{} used", format_context_tokens(Some(used))))
        });
    // CDXC:SessionDetails 2026-11-12 DECISION:
    // User: hide Context's fold chevron, and let its title fold nothing, when the card has nothing
    // under its usage line and bar (an agent that sends no breakdown, such as Codex).
    let skeleton_rows = if breakdown_coming {
        BREAKDOWN_SKELETON_ROWS
    } else {
        0
    };
    let foldable =
        !rows.is_empty() || !deferred.is_empty() || skeleton_rows > 0 || !status.is_empty();
    json!({
        "foldable": foldable,
        "percentLabel": percent_label,
        "tokensLabel": match (used, window) {
            (Some(used), Some(window)) => Some(format!("{} / {}", format_context_tokens(Some(used)), format_context_tokens(Some(window)))),
            (Some(used), None) if claude => Some(format_context_tokens(Some(used))),
            _ => None,
        },
        "segments": segments,
        "rows": rows,
        "deferred": deferred,
        "messages": messages,
        "messagesOpen": details.open_lists.iter().any(|list| list == "messages"),
        "status": status,
        "statusError": error.is_some() && breakdown.is_none() && !running,
        "skeletonRows": skeleton_rows,
        "refreshing": running || details.refreshing,
        "refresh": claude.then(|| json!({"type": "sessionDetailsRefresh"})),
    })
}

/// The Tasks card: the Tasks pill's list (`extras/agent_tasks.rs`), so both show the same rows,
/// the same header line and the same "Show more".
fn tasks(state: &ChatState) -> Option<Value> {
    let step = state.extras.session_details.tasks_step;
    let panel = agent_task_panel(state.session.agent_tasks.as_ref(), false, step)?;
    let fold = (panel.folded_count > 0)
        .then(|| json!({"type": "sessionDetailsList", "list": "tasks", "step": step + 1}));
    Some(json!({
        "meta": panel.meta,
        "rows": panel.rows,
        "foldLabel": panel.fold_label,
        "foldAction": fold,
    }))
}

fn connectors(data: &Value) -> Option<Value> {
    let connectors = data.get("connectors")?;
    let mut groups = Vec::new();
    if let Some(workspace) = connectors
        .get("workspace")
        .filter(|workspace| workspace.is_object())
    {
        let rows: Vec<Value> = workspace["apps"]
            .as_array()
            .into_iter()
            .flatten()
            .map(|app| {
                json!({
                    "text": app["name"].as_str().unwrap_or_default(),
                    "detail": app["detail"].as_str(),
                    "status": "connected",
                    "action": open_action("connectors", Value::Null),
                })
            })
            .collect();
        let name = workspace["name"].as_str().unwrap_or("Personal");
        groups.push(json!({
            "title": format!("{name} workspace"),
            "rows": rows,
            "empty": "No apps connected",
        }));
    }
    let servers: Vec<Value> = connectors["servers"]
        .as_array()
        .into_iter()
        .flatten()
        .map(|server| {
            let status = match server["status"].as_str() {
                Some("pending") => "pending",
                Some("needsAuth" | "needs-auth") => "needsAuth",
                Some("failed") => "failed",
                _ => "connected",
            };
            // Codex and OpenCode name their servers without counting their tools.
            let tools = server["tools"].as_u64();
            let detail = match (status, tools) {
                ("pending", _) => Some("connecting".to_string()),
                ("needsAuth", _) => Some("needs sign-in".to_string()),
                ("failed", _) => Some("failed".to_string()),
                (_, Some(1)) => Some("1 tool".to_string()),
                (_, Some(tools)) => Some(format!("{tools} tools")),
                (_, None) => None,
            };
            json!({
                "text": server["name"].as_str().unwrap_or_default(),
                "detail": detail,
                "status": status,
            })
        })
        .collect();
    if !servers.is_empty() {
        groups.push(json!({"title": "MCP servers", "rows": servers}));
    }
    if groups.is_empty() {
        return None;
    }
    Some(json!({
        "groups": groups,
        "manage": open_action("connectors", Value::Null),
    }))
}

fn skills(state: &ChatState, data: &Value) -> Option<Value> {
    let skills = data.get("skills")?;
    let available: Vec<&Value> = skills["available"]
        .as_array()
        .into_iter()
        .flatten()
        .collect();
    // A home skill opens in Agents Hub at its own SKILL.md, a project or plugin skill (which
    // Agents Hub does not list) opens that file, and a skill built into Claude Code has none
    // (server/src/session_details/skills.rs). An older gxserver sends no scope: Agents Hub by name.
    let row = |name: &str, skill: Option<&&Value>| {
        let description = skill.and_then(|skill| skill["description"].as_str());
        let path = skill.and_then(|skill| skill["path"].as_str());
        let action = match (skill.and_then(|skill| skill["scope"].as_str()), path) {
            (Some("builtin"), _) => None,
            (Some("project" | "plugin"), Some(path)) => {
                Some(open_action("file", json!({ "path": path })))
            }
            (_, path) => Some(open_action("skill", json!({ "name": name, "path": path }))),
        };
        let tooltip = match (&action, description) {
            (None, Some(description)) => Some(format!("{description}\n\nBuilt into Claude Code.")),
            (None, None) => Some("Built into Claude Code.".to_string()),
            (Some(_), description) => description.map(str::to_string),
        };
        json!({
            "text": name,
            "tooltip": tooltip,
            "link": action.is_some(),
            "action": action,
        })
    };
    let used: Vec<Value> = skills["used"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .map(|name| row(name, available.iter().find(|skill| skill["name"] == name)))
        .collect();
    if used.is_empty() && available.is_empty() {
        return None;
    }
    // Only Claude's transcript says which skills the session used; other agents list what they
    // can load, with no "used" part.
    let tracks_use = skills["tracksUse"] != false;
    let all: Vec<Value> = available
        .iter()
        .map(|skill| row(skill["name"].as_str().unwrap_or_default(), Some(skill)))
        .collect();
    let tokens = number(data.pointer("/context/breakdown/skills/tokens"));
    let count = all.len();
    Some(json!({
        "meta": format!("{count} available"),
        "used": used,
        "usedEmpty": tracks_use.then_some("None used in this session yet"),
        "all": all,
        "allLabel": match tokens {
            Some(tokens) => format!("All {count} skills · {} tokens", format_context_tokens(Some(tokens))),
            None => format!("All {count} skills"),
        },
        "allOpen": state.extras.session_details.open_lists.iter().any(|list| list == "skills"),
        "allToggle": json!({"type": "sessionDetailsList", "list": "skills"}),
    }))
}

/// `AGENTS.md` for a file in the session's folder; else its name and whose it is (`CLAUDE.md
/// (user)`), the whole path being the row's tooltip.
fn memory_label(path: &str, kind: &str, folder: Option<&str>) -> String {
    if let Some(relative) = folder
        .and_then(|folder| path.strip_prefix(folder))
        .and_then(|rest| rest.strip_prefix('/'))
    {
        return relative.to_string();
    }
    let name = path.rsplit(['/', '\\']).next().unwrap_or(path);
    match kind {
        "" | "Project" => name.to_string(),
        kind => format!("{name} ({})", kind.to_lowercase()),
    }
}

fn memory(data: &Value) -> Option<Value> {
    let files = data.get("memoryFiles")?.as_array()?;
    if files.is_empty() {
        return None;
    }
    let folder = data.pointer("/environment/folder").and_then(Value::as_str);
    let rows: Vec<Value> = files
        .iter()
        .filter_map(|file| {
            let path = file["path"].as_str()?;
            let detail = match number(file.get("tokens")) {
                Some(tokens) => format_context_tokens(Some(tokens)),
                None => file["kind"].as_str().unwrap_or_default().to_string(),
            };
            Some(json!({
                "text": memory_label(path, file["kind"].as_str().unwrap_or_default(), folder),
                "link": true,
                "detail": detail,
                "tooltip": path,
                "action": open_action("file", json!({"path": path})),
            }))
        })
        .collect();
    Some(json!({ "rows": rows }))
}

fn changes(data: &Value) -> Option<Value> {
    let changes = data.get("changes").filter(|changes| changes.is_object())?;
    let additions = changes["additions"].as_i64().unwrap_or(0);
    let deletions = changes["deletions"].as_i64().unwrap_or(0);
    let on_default = changes["onDefaultBranch"] == true;
    let default = changes["defaultBranch"].as_str().unwrap_or("main");
    let pull_request = changes.get("pullRequest").filter(|pr| pr.is_object());
    let open_pr = pull_request.filter(|pr| pr["open"] == true);
    let button = if let Some(pr) = open_pr {
        Some(json!({
            "label": format!("View PR #{}", pr["number"]),
            "action": open_action("url", json!({"url": pr["url"]})),
        }))
    } else if on_default {
        (additions + deletions > 0)
            .then(|| json!({"label": "Commit", "action": open_action("commit", Value::Null)}))
    } else {
        Some(json!({"label": "Create PR", "action": open_action("createPullRequest", Value::Null)}))
    };
    Some(json!({
        "additions": format!("+{additions}"),
        "deletions": format!("\u{2212}{deletions}"),
        "empty": additions + deletions == 0,
        "detail": if additions + deletions == 0 {
            "No changes".to_string()
        } else if on_default {
            "uncommitted".to_string()
        } else {
            format!("vs {default}")
        },
        "action": open_action("changes", Value::Null),
        "button": button,
    }))
}

fn background(
    state: &ChatState,
    data: Option<&Value>,
    now_ms: f64,
    utc_offset_minutes: i32,
) -> Option<Value> {
    let details = &state.extras.session_details;
    let claude = claude(state);
    // Claude's running commands come from its footer, every other agent's from the panel's read;
    // only Claude's and OpenCode's can be stopped (`stoppable`).
    let live = if claude {
        state
            .session
            .terminal_activity
            .as_ref()
            .and_then(|activity| activity.pointer("/background/tasks"))
    } else {
        data.and_then(|data| data.pointer("/background/running"))
    };
    let running: Vec<Value> = live
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .map(|task| {
            let id = task["id"].as_str().unwrap_or_default();
            let started = task["startedAt"]
                .as_str()
                .and_then(|at| parse_iso_millis(at, utc_offset_minutes));
            let monitor = task["kind"] == "monitor";
            json!({
                "id": id,
                "text": task["title"].as_str().unwrap_or_default(),
                "kind": if monitor { "monitor" } else { "shell" },
                "tooltip": task["command"].as_str(),
                "elapsed": started.map(|started| format_activity_elapsed(((now_ms - started) / 1_000.0).max(0.0))),
                "stopping": details.stopping.iter().any(|known| known == id),
                "stop": (!id.is_empty() && (claude || task["stoppable"] == true))
                    .then(|| json!({"type": "sessionDetailsStop", "id": id})),
            })
        })
        .collect();
    let finished: Vec<Value> = data
        .and_then(|data| data.pointer("/background/finished"))
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter(|task| {
            !running
                .iter()
                .any(|row| row["id"] == task["id"])
        })
        .map(|task| {
            let status = task["status"].as_str().unwrap_or("completed");
            let ended = task["endedAt"]
                .as_str()
                .and_then(|at| parse_iso_millis(at, utc_offset_minutes));
            let word = match status {
                "completed" => "done",
                "killed" | "stopped" | "cancelled" | "canceled" => "stopped",
                other => other,
            };
            json!({
                "text": task["title"].as_str().unwrap_or_default(),
                "detail": match ended {
                    Some(ended) => format!("{word} {}", ago(now_ms, ended)),
                    None => word.to_string(),
                },
                "status": if status == "completed" { "done" } else if status == "failed" { "failed" } else { "stopped" },
                "tooltip": task["summary"].as_str().or_else(|| task["command"].as_str()),
            })
        })
        .collect();
    if running.is_empty() && finished.is_empty() {
        return None;
    }
    Some(json!({
        "meta": (!running.is_empty()).then(|| format!("{} running", running.len())),
        "running": running,
        "finished": finished,
        "finishedLabel": (!finished.is_empty()).then(|| format!("{} finished", finished.len())),
        "finishedOpen": details.open_lists.iter().any(|list| list == "finished"),
        "finishedToggle": json!({"type": "sessionDetailsList", "list": "finished"}),
    }))
}

/// The panel's two tabs, `(id, name)` in their order: the one place their names live, for the
/// desktop, the web build and the phone.
///
/// CDXC:SessionDetails 2026-11-12 DECISION:
/// User, of the phone's ⋯ "Context window" sheet and Session details showing the same things: "let's do 2 tabs that
/// show the 2 things. both are under Session details. we need to deduplicate stuff too"; then, from the "Session
/// details: final phone sheet and desktop match" page, the merged panel as drawn (Q2755, Q2762) with "i dont like that
/// we lost the stuff that's below the context fam" and the Context card moved to the first tab, the desktop taking the
/// same tabs in its panel while the chat box's Context window popover goes (Q2763, option A), and the names "Overview |
/// Limits" (Q2769). The first tab is what the session is (`OVERVIEW_ORDER`; its Context card folds again since
/// 2026-11-14, see `FOLDED_BY_DEFAULT` in `state.rs`); the second lists the context meter's More details rows (the ones Choose details picks, which also pin
/// the status line) as cards (`USAGE_CARDS`). Rows another place already shows are not listed (`SHOWN_ELSEWHERE`).
pub const TABS: [(&str, &str); 2] = [("overview", "Overview"), ("usage", "Limits")];

/// The first tab's cards in this order; a card added later follows them.
const OVERVIEW_ORDER: [&str; 9] = [
    "environment",
    "changes",
    "context",
    "tasks",
    "connectors",
    "skills",
    "memory",
    "background",
    "agent",
];

/// The second tab's cards: `(id, title, the context meter's detail rows in the card's order)`.
const USAGE_CARDS: [(&str, &str, &[&str]); 3] = [
    (
        "limits",
        // Not "Limits": the tab it sits in is named that (Q2769).
        "Plan limits",
        &[
            "fiveHourLimit",
            "sevenDayLimit",
            "modelLimit",
            "accountResets",
            "credits",
            "accountSpending",
            "plan",
            "accountUsageStatus",
            "accountUsageUpdated",
        ],
    ),
    (
        "costAndTime",
        "Cost and time",
        &[
            "costUsd",
            "sessionTime",
            "apiTime",
            "lastTurnDuration",
            "firstTokenTime",
            "startedAt",
        ],
    ),
    (
        "cache",
        "Cache",
        &[
            "cacheState",
            "cacheTtl",
            "idleCompactAt",
            "idleCompactionNote",
            "cacheHitRate",
            "cacheRatio",
            "cacheMisses",
            "cacheLastMiss",
            "lastRequestInput",
            "lastRequestOutput",
            "lastRequestCached",
            "lastRequestCacheWrite",
            "cachedTokens",
            "cacheWriteTokens",
            "tokens",
            "totalTokens",
            "totalInputTokens",
            "totalOutputTokens",
            "turnTokens",
            "reasoningTokens",
        ],
    ),
];

/// A row that rides on another's ("34% · resets in 2h 10m", "Warm · 4m left"): `(rider, host)`.
const RIDES_ON: [(&str, &str); 3] = [
    ("fiveHourReset", "fiveHourLimit"),
    ("sevenDayReset", "sevenDayLimit"),
    ("cacheTimeLeft", "cacheState"),
];

/// Detail rows the panel does not list: the Context card, Environment, Changes, the model pill or the
/// chat's title already show them. They can still be pinned to the status line.
const SHOWN_ELSEWHERE: [&str; 14] = [
    "contextUsed",
    "contextTokens",
    "model",
    "thinking",
    "accountName",
    "accountEmail",
    "repo",
    "folder",
    "branch",
    "worktree",
    "lines",
    "pr",
    "sessionName",
    "agentName",
];

/// The context meter's detail rows (`contextMeter.details`, the ones Choose details picked) with a
/// value, in the meter's order: `(id, label, value)`.
fn detail_items(meter: Option<&Value>) -> Vec<(String, String, String)> {
    meter
        .and_then(|meter| meter["details"].as_array())
        .into_iter()
        .flatten()
        .flat_map(|group| group["items"].as_array().into_iter().flatten())
        .filter_map(|item| {
            let id = item["id"].as_str()?;
            let value = item["value"].as_str().filter(|value| !value.is_empty())?;
            Some((
                id.to_string(),
                item["label"].as_str().unwrap_or_default().to_string(),
                value.to_string(),
            ))
        })
        .collect()
}

/// A rider's words on its host's row: a reset is the window's name and the countdown ("5h 2h 10m"),
/// which reads "resets in 2h 10m" there.
fn rider_text(rider: &str, value: &str) -> String {
    if rider.ends_with("Reset") {
        let countdown = value.split_once(' ').map_or(value, |(_, rest)| rest);
        format!("resets in {countdown}")
    } else {
        value.to_string()
    }
}

/// The second tab: its cards from the detail rows, and Choose details (the meter's editor).
fn usage_tab(items: &[(String, String, String)]) -> Value {
    let find = |id: &str| items.iter().find(|item| item.0 == id);
    let cards: Vec<Value> = USAGE_CARDS
        .iter()
        .filter_map(|(id, title, ids)| {
            let rows: Vec<Value> = ids
                .iter()
                .filter_map(|row_id| {
                    let rider = RIDES_ON
                        .iter()
                        .find(|(_, host)| host == row_id)
                        .and_then(|(rider, _)| find(rider).map(|item| (*rider, item)));
                    let own = find(row_id);
                    if own.is_none() && rider.is_none() {
                        return None;
                    }
                    let label = own
                        .map(|item| item.1.clone())
                        .or_else(|| rider.map(|(_, item)| item.1.clone()))
                        .unwrap_or_default();
                    let value = [
                        own.map(|item| item.2.clone()),
                        rider.map(|(rider, item)| rider_text(rider, &item.2)),
                    ]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" · ");
                    Some(json!({"id": row_id, "label": label, "value": value}))
                })
                .collect();
            (!rows.is_empty()).then(|| json!({"id": id, "title": title, "rows": rows}))
        })
        .collect();
    json!({
        "cards": cards,
        "empty": "Nothing picked to show here. Choose details picks which rows show.",
        "chooseDetails": {"label": "Choose details", "action": {"type": "contextEdit"}},
    })
}

/// Environment gains the agent's version ("Claude Code 2.1.4") when the details carry it.
fn version_row(items: &[(String, String, String)]) -> Option<Value> {
    let (_, label, value) = items.iter().find(|item| item.0 == "version")?;
    let name = label.strip_suffix(" version").unwrap_or(label);
    Some(json!({"icon": "agent", "text": format!("{name} {value}")}))
}

/// The Agent card: the agent's other settings rows (output style, approval policy, sandbox and the
/// like) that no Limits card and no other place shows, so none of them is lost.
fn agent_section(items: &[(String, String, String)]) -> Option<Value> {
    let listed = |id: &str| {
        id == "version"
            || SHOWN_ELSEWHERE.contains(&id)
            || RIDES_ON.iter().any(|(rider, _)| *rider == id)
            || USAGE_CARDS.iter().any(|(_, _, ids)| ids.contains(&id))
    };
    let rows: Vec<Value> = items
        .iter()
        .filter(|item| !listed(&item.0))
        .map(|(_, label, value)| json!({"text": label, "detail": value}))
        .collect();
    (!rows.is_empty()).then(|| {
        json!({"id": "agent", "title": "Agent", "collapsed": false, "pinnedOpen": true, "rows": rows})
    })
}

/// The `sessionDetails` value, or `null` while the panel does not apply to this chat. `meter` is the
/// chat's `contextMeter` (the menus family projects it first), whose details fill the second tab
/// and whose Compact context rides on the Context card.
pub fn document(state: &ChatState, context: &ChatContext, meter: Option<&Value>) -> Value {
    if !enabled(state) {
        return Value::Null;
    }
    let details = &state.extras.session_details;
    let data = details.data.as_ref();
    let usage = live_usage(state, data);
    let collapsed = &details.collapsed;
    let now_ms = context.now_ms;
    let offset = context.utc_offset_minutes;
    // CDXC:SessionDetails 2026-10-25 DECISION:
    // User: "when i open the session details sidebar pls make it show shimmer skeleton for the parts that are still loading". Until the panel's first answer arrives, each card that answer fills shows its title over skeleton rows (Context sketches its breakdown while Claude counts it), drawn in the chat's own skeleton tint and pulse.
    let waiting = data.is_none() && details.error.is_none();
    let claude = claude(state);
    // Before the first answer, skeletons stand only where a card will surely come: every card for
    // Claude, Environment and Changes for any other agent.
    let sketch = |id: &str| waiting && (claude || matches!(id, "environment" | "changes"));
    let mut sections = Vec::new();
    let items = detail_items(meter);
    if let Some(mut body) = data.and_then(|data| environment(data, state.core.hide_account_emails))
    {
        if let (Some(rows), Some(row)) = (body["rows"].as_array_mut(), version_row(&items)) {
            rows.push(row);
        }
        sections.push(section("environment", "Environment", collapsed, body));
    } else if sketch("environment") {
        sections.push(loading_section("environment", "Environment", collapsed, 4));
    }
    let context_known = usage.used.is_some()
        || data
            .and_then(|data| data.pointer("/context/breakdown"))
            .is_some_and(Value::is_object);
    if claude || context_known {
        let mut card = section(
            "context",
            "Context",
            collapsed,
            context_card(state, data, &usage, now_ms, offset),
        );
        // Folded by default (`FOLDED_BY_DEFAULT`); the meter's Compact context rides under its bar.
        if let Some(meter) = meter {
            card["compact"] = json!({
                "label": "Compact context",
                "action": {"type": "contextCompact"},
                "disabled": meter["compactDisabled"] == true,
                "reason": meter["compactDisabledReason"],
            });
        }
        sections.push(card);
    }
    if let Some(body) = tasks(state) {
        sections.push(section("tasks", "Tasks", collapsed, body));
    }
    if let Some(body) = data.and_then(connectors) {
        sections.push(section("connectors", "Connectors", collapsed, body));
    } else if sketch("connectors") {
        sections.push(loading_section("connectors", "Connectors", collapsed, 5));
    }
    if let Some(body) = data.and_then(|data| skills(state, data)) {
        sections.push(section("skills", "Skills", collapsed, body));
    } else if sketch("skills") {
        sections.push(loading_section("skills", "Skills", collapsed, 2));
    }
    if let Some(body) = data.and_then(memory) {
        sections.push(section("memory", "Memory files", collapsed, body));
    } else if sketch("memory") {
        sections.push(loading_section("memory", "Memory files", collapsed, 2));
    }
    if let Some(body) = data.and_then(changes) {
        sections.push(section("changes", "Changes", collapsed, body));
    } else if sketch("changes") {
        sections.push(loading_section("changes", "Changes", collapsed, 1));
    }
    if let Some(body) = background(state, data, now_ms, offset) {
        sections.push(section("background", "Background tasks", collapsed, body));
    }
    if data.is_some() {
        sections.extend(agent_section(&items));
    }
    let rank = |section: &Value| {
        let id = section["id"].as_str().unwrap_or_default();
        OVERVIEW_ORDER
            .iter()
            .position(|known| *known == id)
            .unwrap_or(OVERVIEW_ORDER.len())
    };
    // A stable sort: a card the order does not name keeps its place among the others like it.
    sections.sort_by_key(rank);
    json!({
        "enabled": true,
        "visible": details.visible,
        "ring": {
            "percent": usage.percent,
            "label": format_context_percentage(usage.percent)
                .or_else(|| {
                    usage
                        .used
                        .filter(|_| !claude)
                        .map(|used| format_context_tokens(Some(used)))
                })
                .unwrap_or_default(),
        },
        "loading": data.is_none() && details.request.is_some(),
        "error": details.error,
        "tabs": TABS.iter().map(|(id, label)| json!({"id": id, "label": label})).collect::<Vec<_>>(),
        "sections": sections,
        "usage": usage_tab(&items),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn context_of(agent: &str, data: Option<Value>) -> Value {
        let mut state = ChatState::default();
        state.session.agent = Some(agent.to_string());
        state.extras.session_details.data = data;
        let usage = Usage {
            used: Some(24_000.0),
            window: Some(258_000.0),
            percent: None,
        };
        context_card(
            &state,
            state.extras.session_details.data.as_ref(),
            &usage,
            0.0,
            0,
        )
    }

    #[test]
    fn a_context_card_with_nothing_under_its_bar_does_not_fold() {
        // Codex sends no breakdown: the usage line and bar are the whole card.
        assert_eq!(context_of("codex", Some(json!({})))["foldable"], false);
        // Claude's breakdown is still coming: its skeleton rows sit in the fold.
        assert_eq!(context_of("claude", None)["foldable"], true);
        let breakdown = json!({"context": {"breakdown": {"totalTokens": 24000, "categories": [
            {"id": "messages", "name": "Messages", "tokens": 20000, "kind": "used"}
        ]}}});
        assert_eq!(context_of("codex", Some(breakdown))["foldable"], true);
    }

    fn rows(environment: Value) -> Vec<Value> {
        let body = super::environment(&json!({ "environment": environment }), false).unwrap();
        body["rows"].as_array().unwrap().clone()
    }

    #[test]
    fn a_running_session_shows_its_ram_last() {
        let rows = rows(json!({"memoryBytes": 1.25 * 1024.0 * 1024.0 * 1024.0}));
        assert_eq!(
            rows.last().unwrap(),
            &json!({"icon": "memory", "text": "1.2 GB RAM"})
        );
        let rows = super::environment(
            &json!({"environment": {"memoryBytes": 640.0 * 1024.0 * 1024.0}}),
            false,
        )
        .unwrap();
        assert_eq!(rows["rows"][1]["text"], "640 MB RAM");
    }

    #[test]
    fn no_known_ram_hides_the_row() {
        for environment in [
            json!({}),
            json!({"memoryBytes": null}),
            json!({"memoryBytes": 0}),
        ] {
            assert!(rows(environment).iter().all(|row| row["icon"] != "memory"));
        }
    }
}
