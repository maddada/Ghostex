use std::io::Write;

use serde_json::Value;

use crate::ghostex_cli::rpc::CliResult;

/*
CDXC:GhostexRustCli 2026-07-13:
Faithful port of the Node CLI's lightweight attach picker and compact session
list printing (scripts/ghostex-cli.mjs lines 76-122 and 6178-6560). The rendered
bytes — ANSI styles, agent indicator labels, layout, ordering, viewport math —
must match the Node picker exactly so `gx a` looks identical after the cutover.
The raw-mode keypress loop is reimplemented with crossterm but writes the same
escape sequences (alt screen, cursor hide, wrap off, \x1b[H + \x1b[2K rows).
*/

pub const RESET_ANSI: &str = "\x1b[0m";
pub const PICKER_TITLE: &str = "Attach to Ghostex Session";
pub const PICKER_TITLE_STYLE: &str = "\x1b[1m\x1b[38;2;255;255;255m";
pub const PROJECT_HEADER_STYLE: &str = "\x1b[1m\x1b[38;2;130;183;255m";
pub const SELECTED_SESSION_STYLE: &str = "\x1b[1m\x1b[38;2;255;255;255m";
const QUICK_TERMINALS_PROJECT_NAME: &str = "Quick Terminals";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AgentIndicator {
    pub color: &'static str,
    pub label: &'static str,
}

const AGENT_PICKER_INDICATORS: &[(&str, AgentIndicator)] = &[
    (
        "amp",
        AgentIndicator {
            color: "#ffffff",
            label: "AMP",
        },
    ),
    (
        "amp-cli",
        AgentIndicator {
            color: "#ffffff",
            label: "AMP",
        },
    ),
    (
        "antigravity",
        AgentIndicator {
            color: "#749bff",
            label: "AGY",
        },
    ),
    (
        "antigravity-cli",
        AgentIndicator {
            color: "#749bff",
            label: "AGY",
        },
    ),
    (
        "claude",
        AgentIndicator {
            color: "#d97757",
            label: "CLD",
        },
    ),
    (
        "claude-code",
        AgentIndicator {
            color: "#d97757",
            label: "CLD",
        },
    ),
    (
        "codex",
        AgentIndicator {
            color: "#a991ff",
            label: "CDX",
        },
    ),
    (
        "codex-cli",
        AgentIndicator {
            color: "#a991ff",
            label: "CDX",
        },
    ),
    (
        "copilot",
        AgentIndicator {
            color: "#ffffff",
            label: "PLT",
        },
    ),
    (
        "cursor",
        AgentIndicator {
            color: "#749bff",
            label: "CRS",
        },
    ),
    (
        "cursor-cli",
        AgentIndicator {
            color: "#749bff",
            label: "CRS",
        },
    ),
    (
        "droid",
        AgentIndicator {
            color: "#ff7a1a",
            label: "DRD",
        },
    ),
    (
        "factory-droid",
        AgentIndicator {
            color: "#ff7a1a",
            label: "DRD",
        },
    ),
    (
        "gemini",
        AgentIndicator {
            color: "#8b9aff",
            label: "GEM",
        },
    ),
    (
        "grok",
        AgentIndicator {
            color: "#ffffff",
            label: "GRK",
        },
    ),
    (
        "grok-build",
        AgentIndicator {
            color: "#ffffff",
            label: "GRK",
        },
    ),
    (
        "opencode",
        AgentIndicator {
            color: "#6d96c0",
            label: "OPN",
        },
    ),
    (
        "open-code",
        AgentIndicator {
            color: "#6d96c0",
            label: "OPN",
        },
    ),
    (
        "pi",
        AgentIndicator {
            color: "#c8ff62",
            label: "PIA",
        },
    ),
    (
        "work-codex",
        AgentIndicator {
            color: "#a991ff",
            label: "CDX",
        },
    ),
];
const DEFAULT_PICKER_AGENT_INDICATOR: AgentIndicator = AgentIndicator {
    color: "#9ca3af",
    label: "UNK",
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PickerItemKind {
    Title,
    Separator,
    Project,
    Session,
}

#[derive(Clone, Debug)]
pub struct PickerItem {
    pub kind: PickerItemKind,
    pub plain_text: String,
    pub render_text: String,
    pub project_index: Option<usize>,
    pub session_index: Option<usize>,
    pub session: Option<Value>,
    pub agent_indicator: Option<AgentIndicator>,
}

#[derive(Clone, Debug)]
pub struct PickerGroup {
    pub project_name: String,
    pub project_path: String,
    pub sessions: Vec<Value>,
    pub start_session_index: usize,
    pub end_session_index: usize,
}

#[derive(Clone, Debug)]
pub struct PickerModel {
    pub groups: Vec<PickerGroup>,
    pub items: Vec<PickerItem>,
    /// Indices into `items` for kind == Session rows, in selection order
    /// (mirrors the JS model.sessionItems object references).
    pub session_items: Vec<usize>,
}

#[derive(Clone, Debug)]
pub struct PickerRow {
    pub agent_indicator: Option<AgentIndicator>,
    pub kind: PickerItemKind,
    pub selected: bool,
    pub text: String,
}

pub fn print_session_list(sessions: &[Value], grouped: bool) {
    if sessions.is_empty() {
        println!("No running terminal sessions.");
        return;
    }
    let project_groups = group_sessions_preserving_sidebar_order(sessions);
    if !grouped {
        for project in &project_groups {
            for session in &project.sessions {
                println!(
                    "{}",
                    format_compact_session_line(session, Some(&project.project_name))
                );
            }
        }
        return;
    }
    for (project_index, project) in project_groups.iter().enumerate() {
        if project_index > 0 {
            println!();
        }
        println!("{}", project.project_name);
        if !project.project_path.is_empty() {
            println!("{}", project.project_path);
        }
        for session in &project.sessions {
            println!("{}", format_compact_session_line(session, None));
        }
    }
}

pub fn print_session_picker_rows(sessions: &[Value]) {
    for row in build_session_picker_rows(sessions, 0) {
        println!("{}", row.text);
    }
}

pub fn is_interactive_terminal() -> bool {
    use crossterm::tty::IsTty;
    std::io::stdin().is_tty() && std::io::stdout().is_tty()
}

pub fn build_session_picker_model(sessions: &[Value]) -> PickerModel {
    let mut groups: Vec<PickerGroup> = group_sessions_preserving_sidebar_order(sessions)
        .into_iter()
        .filter(|project| !project.sessions.is_empty())
        .collect();
    let mut items = vec![
        PickerItem {
            kind: PickerItemKind::Title,
            plain_text: PICKER_TITLE.to_string(),
            render_text: format!("{PICKER_TITLE_STYLE}{PICKER_TITLE}{RESET_ANSI}"),
            project_index: None,
            session_index: None,
            session: None,
            agent_indicator: None,
        },
        PickerItem {
            kind: PickerItemKind::Separator,
            plain_text: "─".to_string(),
            render_text: "─".to_string(),
            project_index: None,
            session_index: None,
            session: None,
            agent_indicator: None,
        },
    ];
    let mut session_items: Vec<usize> = Vec::new();
    let mut session_index = 0usize;
    for (project_index, project) in groups.iter_mut().enumerate() {
        let start_session_index = session_index;
        items.push(PickerItem {
            kind: PickerItemKind::Project,
            plain_text: project.project_name.clone(),
            render_text: format!("{PROJECT_HEADER_STYLE}{}{RESET_ANSI}", project.project_name),
            project_index: Some(project_index),
            session_index: None,
            session: None,
            agent_indicator: None,
        });
        for session in &project.sessions {
            let agent_indicator = resolve_session_picker_agent_indicator(session);
            let title = js_string_or_empty(session.get("title"));
            items.push(PickerItem {
                kind: PickerItemKind::Session,
                plain_text: format!("[{}] {title}", agent_indicator.label),
                render_text: format!(
                    "{}[{}]{RESET_ANSI} {title}",
                    ansi_color(agent_indicator.color),
                    agent_indicator.label
                ),
                project_index: Some(project_index),
                session_index: Some(session_index),
                session: Some(session.clone()),
                agent_indicator: Some(agent_indicator),
            });
            session_items.push(items.len() - 1);
            session_index += 1;
        }
        project.start_session_index = start_session_index;
        project.end_session_index = session_index.wrapping_sub(1);
    }
    PickerModel {
        groups,
        items,
        session_items,
    }
}

pub fn build_session_picker_rows(
    sessions: &[Value],
    selected_session_index: usize,
) -> Vec<PickerRow> {
    let model = build_session_picker_model(sessions);
    model
        .items
        .iter()
        .map(|item| PickerRow {
            agent_indicator: if item.kind == PickerItemKind::Session {
                item.agent_indicator
            } else {
                None
            },
            kind: item.kind,
            selected: item.kind == PickerItemKind::Session
                && item.session_index == Some(selected_session_index),
            text: item.plain_text.clone(),
        })
        .collect()
}

pub fn move_session_picker_selection(
    model: &PickerModel,
    selected_session_index: usize,
    direction: &str,
) -> usize {
    let session_count = model.session_items.len();
    if session_count == 0 {
        return 0;
    }
    let selected = selected_session_index as i64;
    let count = session_count as i64;
    match direction {
        "up" => wrap_session_picker_index(selected - 1, count),
        "down" => wrap_session_picker_index(selected + 1, count),
        "pageup" => wrap_session_picker_index(selected - 5, count),
        "pagedown" => wrap_session_picker_index(selected + 5, count),
        "left" | "right" => {
            let Some(current) = model
                .session_items
                .get(selected_session_index)
                .and_then(|index| model.items.get(*index))
            else {
                return selected_session_index;
            };
            let delta: i64 = if direction == "left" { -1 } else { 1 };
            let target_project_index = wrap_session_picker_index(
                current.project_index.unwrap_or(0) as i64 + delta,
                model.groups.len() as i64,
            );
            model
                .groups
                .get(target_project_index)
                .map(|project| project.start_session_index)
                .unwrap_or(selected_session_index)
        }
        _ => selected_session_index,
    }
}

pub fn wrap_session_picker_index(index: i64, count: i64) -> usize {
    (((index % count) + count) % count) as usize
}

pub fn interactive_session_picker(sessions: &[Value]) -> CliResult<Option<Value>> {
    let model = build_session_picker_model(sessions);
    if model.session_items.is_empty() {
        return Ok(None);
    }
    let was_raw = crossterm::terminal::is_raw_mode_enabled().unwrap_or(false);
    let mut output = std::io::stdout();
    // Same enter sequence as the Node picker: alt screen, hide cursor, wrap off.
    let _ = write!(output, "\x1b[?1049h\x1b[?25l\x1b[?7l");
    let _ = output.flush();
    if !was_raw {
        let _ = crossterm::terminal::enable_raw_mode();
    }
    let result = run_picker_loop(&model, &mut output);
    if !was_raw {
        let _ = crossterm::terminal::disable_raw_mode();
    }
    let _ = write!(output, "\x1b[?7h\x1b[?25h\x1b[?1049l");
    let _ = output.flush();
    result
}

fn run_picker_loop(model: &PickerModel, output: &mut std::io::Stdout) -> CliResult<Option<Value>> {
    use crossterm::event::{read, Event, KeyCode, KeyEventKind, KeyModifiers};
    let mut selected_session_index = 0usize;
    let mut viewport_start = 0usize;
    viewport_start = render_session_picker(model, selected_session_index, viewport_start, output)?;
    loop {
        let event = read()?;
        let Event::Key(key) = event else {
            continue;
        };
        if key.kind == KeyEventKind::Release {
            continue;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c') | KeyCode::Char('C'))
        {
            crate::ghostex_cli::set_exit_code(130);
            return Ok(None);
        }
        match key.code {
            KeyCode::Esc | KeyCode::Char('q') | KeyCode::Char('Q') => return Ok(None),
            KeyCode::Enter | KeyCode::Char(' ') => {
                return Ok(model
                    .session_items
                    .get(selected_session_index)
                    .and_then(|index| model.items.get(*index))
                    .and_then(|item| item.session.clone()));
            }
            _ => {}
        }
        let direction = match key.code {
            KeyCode::Up => "up",
            KeyCode::Down => "down",
            KeyCode::Left => "left",
            KeyCode::Right => "right",
            KeyCode::PageUp => "pageup",
            KeyCode::PageDown => "pagedown",
            _ => continue,
        };
        let next_selection =
            move_session_picker_selection(model, selected_session_index, direction);
        if next_selection != selected_session_index {
            selected_session_index = next_selection;
            viewport_start =
                render_session_picker(model, selected_session_index, viewport_start, output)?;
        }
    }
}

fn render_session_picker(
    model: &PickerModel,
    selected_session_index: usize,
    viewport_start: usize,
    output: &mut impl Write,
) -> CliResult<usize> {
    let (columns, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let terminal_rows = (rows as usize).max(1);
    let selected_line_index = model
        .items
        .iter()
        .position(|item| {
            item.kind == PickerItemKind::Session
                && item.session_index == Some(selected_session_index)
        })
        .unwrap_or(0);
    let max_viewport_start = model.items.len().saturating_sub(terminal_rows);
    let mut next_viewport_start = viewport_start.min(max_viewport_start);
    if selected_line_index < next_viewport_start {
        next_viewport_start = selected_line_index;
    } else if selected_line_index >= next_viewport_start + terminal_rows {
        next_viewport_start = selected_line_index - terminal_rows + 1;
    }

    write!(output, "\x1b[H")?;
    for row in 0..terminal_rows {
        let mut line = String::new();
        if let Some(item) = model.items.get(next_viewport_start + row) {
            let text = if item.kind == PickerItemKind::Separator {
                format!(
                    "{PROJECT_HEADER_STYLE}{}{RESET_ANSI}",
                    "─".repeat(columns as usize)
                )
            } else {
                item.render_text.clone()
            };
            line = if item.kind == PickerItemKind::Session
                && item.session_index == Some(selected_session_index)
            {
                format!("{SELECTED_SESSION_STYLE}{}{RESET_ANSI}", strip_ansi(&text))
            } else {
                text
            };
        }
        write!(
            output,
            "\x1b[2K{line}{}",
            if row == terminal_rows - 1 { "" } else { "\r\n" }
        )?;
    }
    output.flush()?;
    Ok(next_viewport_start)
}

fn resolve_session_picker_project_name(session: &Value, is_first_group: bool) -> String {
    if is_first_group
        && js_string_or_empty(session.get("projectPath"))
            .trim()
            .is_empty()
    {
        return QUICK_TERMINALS_PROJECT_NAME.to_string();
    }
    for key in ["projectName", "projectPath"] {
        if js_truthy(session.get(key)) {
            return js_string(session.get(key).expect("truthy value present"));
        }
    }
    QUICK_TERMINALS_PROJECT_NAME.to_string()
}

pub fn resolve_session_picker_agent_indicator(session: &Value) -> AgentIndicator {
    for field in ["agent", "agentIcon", "agentId", "agentName", "provider"] {
        let key = normalize_agent_indicator_key(session.get(field));
        if key.is_empty() {
            continue;
        }
        if let Some((_, indicator)) = AGENT_PICKER_INDICATORS
            .iter()
            .find(|(candidate, _)| *candidate == key)
        {
            return *indicator;
        }
    }
    DEFAULT_PICKER_AGENT_INDICATOR
}

fn normalize_agent_indicator_key(value: Option<&Value>) -> String {
    let text = js_string_or_empty(value);
    let trimmed = text.trim().to_lowercase();
    // replace(/[\s_]+/g, "-"); the JS replace(/-cli$/u, "-cli") is a no-op.
    let mut normalized = String::with_capacity(trimmed.len());
    let mut in_run = false;
    for character in trimmed.chars() {
        if is_js_whitespace(character) || character == '_' {
            if !in_run {
                normalized.push('-');
                in_run = true;
            }
            continue;
        }
        in_run = false;
        normalized.push(character);
    }
    normalized
}

pub fn ansi_color(hex_color: &str) -> String {
    // ^#?([0-9a-f]{6})$ with the i flag.
    let value = hex_color.strip_prefix('#').unwrap_or(hex_color);
    if value.len() != 6 || !value.chars().all(|c| c.is_ascii_hexdigit()) {
        return String::new();
    }
    let red = u8::from_str_radix(&value[0..2], 16).unwrap_or(0);
    let green = u8::from_str_radix(&value[2..4], 16).unwrap_or(0);
    let blue = u8::from_str_radix(&value[4..6], 16).unwrap_or(0);
    format!("\x1b[38;2;{red};{green};{blue}m")
}

pub fn strip_ansi(value: &str) -> String {
    let chars: Vec<char> = value.chars().collect();
    let mut output = String::with_capacity(value.len());
    let mut index = 0;
    while index < chars.len() {
        if chars[index] == '\x1b' {
            if let Some(end) = match_ansi_escape(&chars, index) {
                index = end;
                continue;
            }
        }
        output.push(chars[index]);
        index += 1;
    }
    output
}

/// Matches the JS strip regex /\x1b\[[0-9;?]*[ -/]*[@-~]/ at `start`.
fn match_ansi_escape(chars: &[char], start: usize) -> Option<usize> {
    let mut index = start + 1;
    if chars.get(index) != Some(&'[') {
        return None;
    }
    index += 1;
    while matches!(chars.get(index), Some(c) if c.is_ascii_digit() || *c == ';' || *c == '?') {
        index += 1;
    }
    while matches!(chars.get(index), Some(c) if (' '..='/').contains(c)) {
        index += 1;
    }
    match chars.get(index) {
        Some(c) if ('@'..='~').contains(c) => Some(index + 1),
        _ => None,
    }
}

pub fn format_compact_session_line(session: &Value, project_label: Option<&str>) -> String {
    format_compact_session_line_at(session, project_label, now_ms())
}

fn format_compact_session_line_at(
    session: &Value,
    project_label: Option<&str>,
    now_ms: i64,
) -> String {
    let marker = if js_truthy(session.get("isFocused")) {
        "›"
    } else {
        " "
    };
    let title = ["displayTitle", "title"]
        .iter()
        .find(|key| js_truthy(session.get(**key)))
        .map(|key| js_string(session.get(*key).expect("truthy value present")))
        .unwrap_or_else(|| "-".to_string());
    let alias = js_display(session.get("alias"));
    let headline = match project_label {
        Some(project_label) => format!("{marker} #{alias}  {project_label} · {title}"),
        None => format!("{marker} #{alias}  {title}"),
    };
    let details: Vec<String> = [
        js_string_or_empty(session.get("agent")),
        format_compact_provider(session).unwrap_or_default(),
        js_string_or_empty(session.get("status")),
        format_active_time(session.get("lastInteractionAt"), now_ms),
    ]
    .iter()
    .map(|value| value.trim().to_string())
    .filter(|value| !value.is_empty() && value != "-")
    .collect();
    if details.is_empty() {
        return headline;
    }
    format!("{headline}\n    {}", details.join(" · "))
}

pub fn format_compact_provider(session: &Value) -> Option<String> {
    let provider = session
        .get("provider")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty())?;
    let provider_session_name = session
        .get("providerSessionName")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty());
    match provider_session_name {
        Some(name) => Some(format!("{provider}/{name}")),
        None => Some(provider.to_string()),
    }
}

pub fn group_sessions_preserving_sidebar_order(sessions: &[Value]) -> Vec<PickerGroup> {
    let mut groups: Vec<PickerGroup> = Vec::new();
    // JS keys the Map by session.projectId (undefined groups together, and
    // null is a distinct key from undefined) — Option<Value> mirrors that.
    let mut keys: Vec<Option<Value>> = Vec::new();
    for session in sessions {
        let key = session.get("projectId").cloned();
        let group_index = match keys.iter().position(|candidate| *candidate == key) {
            Some(index) => index,
            None => {
                let is_first_group = groups.is_empty();
                let project_path = if js_truthy(session.get("projectPath")) {
                    js_string(session.get("projectPath").expect("truthy value present"))
                } else {
                    String::new()
                };
                groups.push(PickerGroup {
                    project_name: resolve_session_picker_project_name(session, is_first_group),
                    project_path,
                    sessions: Vec::new(),
                    start_session_index: 0,
                    end_session_index: 0,
                });
                keys.push(key);
                groups.len() - 1
            }
        };
        groups[group_index].sessions.push(session.clone());
    }
    groups
}

pub fn format_active_time(value: Option<&Value>, now_ms: i64) -> String {
    // Date.parse(value ?? "") — the inventory always sends ISO strings; other
    // shapes parse to NaN in practice and render "-".
    let Some(text) = value.and_then(Value::as_str) else {
        return "-".to_string();
    };
    let timestamp = match chrono::DateTime::parse_from_rfc3339(text) {
        Ok(parsed) => parsed.timestamp_millis(),
        Err(_) => match chrono::NaiveDate::parse_from_str(text, "%Y-%m-%d") {
            Ok(date) => date
                .and_hms_opt(0, 0, 0)
                .map(|naive| naive.and_utc().timestamp_millis())
                .unwrap_or(i64::MIN),
            Err(_) => return "-".to_string(),
        },
    };
    if timestamp == i64::MIN {
        return "-".to_string();
    }
    let seconds = (((now_ms - timestamp) as f64 / 1000.0).round()).max(0.0) as i64;
    if seconds < 60 {
        return format!("{seconds}s ago");
    }
    let minutes = ((seconds as f64) / 60.0).round() as i64;
    if minutes < 60 {
        return format!("{minutes}m ago");
    }
    let hours = ((minutes as f64) / 60.0).round() as i64;
    if hours < 48 {
        return format!("{hours}h ago");
    }
    let days = ((hours as f64) / 24.0).round() as i64;
    format!("{days}d ago")
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

/// JS \s character class membership.
fn is_js_whitespace(character: char) -> bool {
    matches!(
        character,
        '\t' | '\n' | '\u{b}' | '\u{c}' | '\r' | ' ' | '\u{a0}' | '\u{1680}' | '\u{2000}'
            ..='\u{200a}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202f}'
                | '\u{205f}'
                | '\u{3000}'
                | '\u{feff}'
    )
}

/// JS truthiness of an optional JSON value (missing/undefined → false).
fn js_truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(flag)) => *flag,
        Some(Value::Number(number)) => number.as_f64().map(|n| n != 0.0).unwrap_or(true),
        Some(Value::String(text)) => !text.is_empty(),
        Some(Value::Array(_)) | Some(Value::Object(_)) => true,
    }
}

/// JS String(value) coercion for JSON values.
fn js_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(flag) => flag.to_string(),
        Value::Number(number) => js_number_string(number),
        Value::String(text) => text.clone(),
        Value::Array(items) => items
            .iter()
            .map(|item| match item {
                Value::Null => String::new(),
                other => js_string(other),
            })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".to_string(),
    }
}

/// String(value ?? "") — null/undefined become "".
fn js_string_or_empty(value: Option<&Value>) -> String {
    match value {
        None | Some(Value::Null) => String::new(),
        Some(other) => js_string(other),
    }
}

/// Template interpolation of a possibly-missing value (undefined → "undefined").
fn js_display(value: Option<&Value>) -> String {
    match value {
        None => "undefined".to_string(),
        Some(other) => js_string(other),
    }
}

fn js_number_string(number: &serde_json::Number) -> String {
    if let Some(int) = number.as_i64() {
        return int.to_string();
    }
    if let Some(int) = number.as_u64() {
        return int.to_string();
    }
    let float = number.as_f64().unwrap_or(0.0);
    if float.fract() == 0.0 && float.abs() < 9.007_199_254_740_992e15 {
        return format!("{}", float as i64);
    }
    format!("{float}")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn fixture_sessions() -> Vec<Value> {
        vec![
            json!({
                "alias": 1,
                "projectId": "p1",
                "projectName": "Ghostex",
                "projectPath": "/Users/dev/ghostex",
                "title": "fix sidebar",
                "agent": "claude",
                "sessionId": "s1",
            }),
            json!({
                "alias": 2,
                "projectId": "p1",
                "projectName": "Ghostex",
                "projectPath": "/Users/dev/ghostex",
                "title": "port cli",
                "provider": "zmx",
                "sessionId": "s2",
            }),
            json!({
                "alias": 3,
                "projectId": "p2",
                "projectName": "Zephyr",
                "projectPath": "/Users/dev/zephyr",
                "title": "review",
                "agent": "codex",
                "sessionId": "s3",
            }),
        ]
    }

    #[test]
    fn model_layout_matches_node_picker() {
        let model = build_session_picker_model(&fixture_sessions());
        let kinds: Vec<PickerItemKind> = model.items.iter().map(|item| item.kind).collect();
        assert_eq!(
            kinds,
            vec![
                PickerItemKind::Title,
                PickerItemKind::Separator,
                PickerItemKind::Project,
                PickerItemKind::Session,
                PickerItemKind::Session,
                PickerItemKind::Project,
                PickerItemKind::Session,
            ]
        );
        assert_eq!(model.items[0].plain_text, "Attach to Ghostex Session");
        assert_eq!(
            model.items[0].render_text,
            "\x1b[1m\x1b[38;2;255;255;255mAttach to Ghostex Session\x1b[0m"
        );
        assert_eq!(model.items[2].plain_text, "Ghostex");
        assert_eq!(
            model.items[2].render_text,
            "\x1b[1m\x1b[38;2;130;183;255mGhostex\x1b[0m"
        );
        assert_eq!(model.items[3].plain_text, "[CLD] fix sidebar");
        assert_eq!(
            model.items[3].render_text,
            "\x1b[38;2;217;119;87m[CLD]\x1b[0m fix sidebar"
        );
        // No agent metadata at all → UNK default.
        assert_eq!(model.items[4].plain_text, "[UNK] port cli");
        assert_eq!(model.session_items.len(), 3);
        assert_eq!(model.groups[0].start_session_index, 0);
        assert_eq!(model.groups[0].end_session_index, 1);
        assert_eq!(model.groups[1].start_session_index, 2);
        assert_eq!(model.groups[1].end_session_index, 2);
    }

    #[test]
    fn quick_terminals_group_naming() {
        let sessions = vec![
            json!({ "alias": 1, "title": "scratch", "sessionId": "s1" }),
            json!({
                "alias": 2,
                "projectId": "p1",
                "projectName": "Ghostex",
                "projectPath": "/Users/dev/ghostex",
                "title": "work",
                "sessionId": "s2",
            }),
        ];
        let groups = group_sessions_preserving_sidebar_order(&sessions);
        assert_eq!(groups[0].project_name, "Quick Terminals");
        assert_eq!(groups[0].project_path, "");
        assert_eq!(groups[1].project_name, "Ghostex");
    }

    #[test]
    fn movement_wraps_and_jumps_projects() {
        let model = build_session_picker_model(&fixture_sessions());
        assert_eq!(move_session_picker_selection(&model, 0, "up"), 2);
        assert_eq!(move_session_picker_selection(&model, 2, "down"), 0);
        assert_eq!(move_session_picker_selection(&model, 1, "down"), 2);
        // Page moves jump five with wrap.
        assert_eq!(move_session_picker_selection(&model, 0, "pagedown"), 2);
        assert_eq!(move_session_picker_selection(&model, 0, "pageup"), 1);
        // Left/right move by project boundary.
        assert_eq!(move_session_picker_selection(&model, 0, "right"), 2);
        assert_eq!(move_session_picker_selection(&model, 2, "right"), 0);
        assert_eq!(move_session_picker_selection(&model, 1, "left"), 2);
        // Unknown direction keeps selection.
        assert_eq!(move_session_picker_selection(&model, 1, "space"), 1);
    }

    #[test]
    fn wrap_index_handles_negatives() {
        assert_eq!(wrap_session_picker_index(-1, 3), 2);
        assert_eq!(wrap_session_picker_index(3, 3), 0);
        assert_eq!(wrap_session_picker_index(-7, 3), 2);
    }

    #[test]
    fn rows_mark_selected_session_only() {
        let rows = build_session_picker_rows(&fixture_sessions(), 1);
        let selected: Vec<bool> = rows.iter().map(|row| row.selected).collect();
        assert_eq!(
            selected,
            vec![false, false, false, false, true, false, false]
        );
        assert_eq!(rows[4].text, "[UNK] port cli");
        assert!(rows[2].agent_indicator.is_none());
        assert_eq!(rows[3].agent_indicator.map(|i| i.label), Some("CLD"));
    }

    #[test]
    fn agent_indicator_resolution_order_and_normalization() {
        let session = json!({ "agent": "Claude Code", "provider": "zmx" });
        assert_eq!(
            resolve_session_picker_agent_indicator(&session).label,
            "CLD"
        );
        let session = json!({ "provider": "codex_cli" });
        assert_eq!(
            resolve_session_picker_agent_indicator(&session).label,
            "CDX"
        );
        let session = json!({ "agent": "mystery" });
        assert_eq!(
            resolve_session_picker_agent_indicator(&session).label,
            "UNK"
        );
        assert_eq!(
            normalize_agent_indicator_key(Some(&json!("  Factory  Droid "))),
            "factory-droid"
        );
    }

    #[test]
    fn ansi_color_and_strip_ansi_match_js() {
        assert_eq!(ansi_color("#d97757"), "\x1b[38;2;217;119;87m");
        assert_eq!(ansi_color("FFFFFF"), "\x1b[38;2;255;255;255m");
        assert_eq!(ansi_color("#abc"), "");
        assert_eq!(ansi_color("nothex"), "");
        assert_eq!(
            strip_ansi("\x1b[1m\x1b[38;2;1;2;3mhi\x1b[0m there"),
            "hi there"
        );
        assert_eq!(strip_ansi("plain"), "plain");
        assert_eq!(strip_ansi("\x1b[2Kline"), "line");
    }

    #[test]
    fn compact_session_line_formats_details() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-07-13T10:01:30Z")
            .unwrap()
            .timestamp_millis();
        let session = json!({
            "alias": 4,
            "isFocused": true,
            "displayTitle": "My Thread",
            "agent": "claude",
            "provider": "zmx",
            "providerSessionName": "g-0713-1",
            "status": "running",
            "lastInteractionAt": "2026-07-13T10:00:00Z",
        });
        assert_eq!(
            format_compact_session_line_at(&session, Some("Ghostex"), now),
            "› #4  Ghostex · My Thread\n    claude · zmx/g-0713-1 · running · 2m ago"
        );
        let bare = json!({ "alias": 9, "title": "t" });
        assert_eq!(format_compact_session_line_at(&bare, None, now), "  #9  t");
    }

    #[test]
    fn active_time_buckets() {
        let now = chrono::DateTime::parse_from_rfc3339("2026-07-13T12:00:00Z")
            .unwrap()
            .timestamp_millis();
        let at = |iso: &str| format_active_time(Some(&json!(iso)), now);
        assert_eq!(at("2026-07-13T11:59:30Z"), "30s ago");
        assert_eq!(at("2026-07-13T11:30:00Z"), "30m ago");
        assert_eq!(at("2026-07-13T02:00:00Z"), "10h ago");
        assert_eq!(at("2026-07-01T12:00:00Z"), "12d ago");
        assert_eq!(at("2026-07-14T12:00:00Z"), "0s ago");
        assert_eq!(format_active_time(None, now), "-");
        assert_eq!(format_active_time(Some(&json!("garbage")), now), "-");
    }
}
