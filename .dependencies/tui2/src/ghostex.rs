use std::collections::{BTreeMap, HashMap};
use std::io;
use std::process::Command;

use serde::Deserialize;

use crate::detect::AgentState;

pub(crate) const SESSION_LIST_REFRESH: std::time::Duration = std::time::Duration::from_secs(5);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionActivity {
    Attention,
    Working,
}

#[derive(Debug, Deserialize, Clone)]
pub(crate) struct SessionItem {
    #[serde(default)]
    pub(crate) activity: Option<String>,
    #[serde(default)]
    pub(crate) agent: Option<String>,
    #[serde(default, rename = "projectId")]
    pub(crate) project_id: Option<String>,
    #[serde(default, rename = "groupId")]
    pub(crate) group_id: Option<String>,
    #[serde(default, rename = "isFavorite")]
    pub(crate) is_favorite: Option<bool>,
    #[serde(default, rename = "projectName")]
    pub(crate) project_name: Option<String>,
    #[serde(default, rename = "projectPath")]
    pub(crate) project_path: Option<String>,
    #[serde(default, rename = "sessionId")]
    pub(crate) session_id: String,
    #[serde(default)]
    pub(crate) status: String,
    #[serde(default)]
    pub(crate) title: String,
}

#[derive(Debug, Deserialize)]
struct SessionListResult {
    #[serde(default)]
    sessions: Vec<SessionItem>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreateSessionResult {
    #[serde(default)]
    pub(crate) session: Option<CreatedSession>,
}

#[derive(Debug, Deserialize)]
pub(crate) struct CreatedSession {
    #[serde(default, rename = "ghostexId")]
    pub(crate) ghostex_id: Option<String>,
    #[serde(default, rename = "sessionId")]
    pub(crate) session_id: Option<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct ProjectGroup {
    pub(crate) project_id: Option<String>,
    pub(crate) group_id: Option<String>,
    pub(crate) name: String,
    pub(crate) sessions: Vec<SessionItem>,
}

#[derive(Debug, Clone)]
pub(crate) enum SidebarRow {
    Project(ProjectHeader),
    NewTerminal {
        project_id: Option<String>,
        group_id: Option<String>,
    },
    Session(SessionItem),
}

#[derive(Debug, Clone)]
pub(crate) struct ProjectHeader {
    pub(crate) name: String,
}

pub(crate) fn fetch_sessions() -> io::Result<Vec<SessionItem>> {
    let output = Command::new("/bin/zsh")
        .arg("-lc")
        .arg(format!("{} sessions --json", cli_command()))
        .output()?;
    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    let result: SessionListResult = serde_json::from_slice(&output.stdout)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))?;
    Ok(result.sessions)
}

pub(crate) fn create_terminal(
    project_id: Option<&str>,
    group_id: Option<&str>,
) -> io::Result<CreateSessionResult> {
    let mut args = vec!["create-session".to_string()];
    if let Some(project_id) = project_id.filter(|value| !value.trim().is_empty()) {
        args.extend(["--project-id".to_string(), project_id.to_string()]);
    }
    if let Some(group_id) = group_id.filter(|value| !value.trim().is_empty()) {
        args.extend(["--group-id".to_string(), group_id.to_string()]);
    }
    let output = run_cli(&args)?;
    serde_json::from_slice(&output).map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

pub(crate) fn acknowledge_session_attention(session: &SessionItem) -> io::Result<()> {
    run_cli(&session_command_args(
        "acknowledge-session-attention",
        session,
        None,
    ))
    .map(|_| ())
}

pub(crate) fn attach_shell_command(session: &SessionItem) -> String {
    let mut command = format!(
        "{} attach --session-id {}",
        cli_command(),
        shell_quote(&session.session_id)
    );
    if let Some(project_id) = session
        .project_id
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        command.push_str(" --project-id ");
        command.push_str(&shell_quote(project_id));
    }
    command
}

pub(crate) fn session_identity_key(session: &SessionItem) -> String {
    session_identity_key_parts(session.project_id.as_deref(), &session.session_id)
}

pub(crate) fn session_identity_key_parts(project_id: Option<&str>, session_id: &str) -> String {
    let project_id = project_id
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or("");
    if project_id.is_empty() {
        session_id.to_string()
    } else {
        format!("{project_id}/{session_id}")
    }
}

pub(crate) fn session_activity(session: &SessionItem) -> Option<SessionActivity> {
    let value = session
        .activity
        .as_deref()
        .unwrap_or(session.status.as_str())
        .trim()
        .to_lowercase();
    match value.as_str() {
        "attention" => Some(SessionActivity::Attention),
        "working" => Some(SessionActivity::Working),
        _ => None,
    }
}

pub(crate) fn session_agent_state(session: &SessionItem) -> (AgentState, bool) {
    match session_activity(session) {
        Some(SessionActivity::Attention) => return (AgentState::Blocked, false),
        Some(SessionActivity::Working) => return (AgentState::Working, true),
        None => {}
    }

    match session.status.trim().to_ascii_lowercase().as_str() {
        "sleep" | "sleeping" => (AgentState::Unknown, true),
        "done" | "complete" | "completed" => (AgentState::Idle, false),
        _ => (AgentState::Idle, true),
    }
}

pub(crate) fn session_state_labels(session: &SessionItem) -> HashMap<String, String> {
    let mut labels = HashMap::new();
    if session_activity(session) == Some(SessionActivity::Attention) {
        labels.insert("blocked".to_string(), "attention".to_string());
    }
    if matches!(
        session.status.trim().to_ascii_lowercase().as_str(),
        "sleep" | "sleeping"
    ) {
        labels.insert("unknown".to_string(), "sleep".to_string());
    }
    labels
}

pub(crate) fn session_title(session: &SessionItem) -> String {
    let title = session.title.trim();
    if title.is_empty() {
        session.session_id.clone()
    } else {
        title.to_string()
    }
}

pub(crate) fn project_label(session: &SessionItem) -> String {
    session
        .project_name
        .as_deref()
        .filter(|value| !value.trim().is_empty())
        .or(session.project_path.as_deref())
        .unwrap_or("Project")
        .to_string()
}

pub(crate) fn normalized_agent_label(session: &SessionItem) -> Option<String> {
    session
        .agent
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
}

pub(crate) fn workspace_title(session: &SessionItem) -> String {
    format!("{} · {}", project_label(session), session_title(session))
}

pub(crate) fn group_sessions(sessions: &[SessionItem]) -> Vec<ProjectGroup> {
    /*
     * CDXC:GhostexTui2 2026-06-16-23:18:
     * TUI2 must copy the current Ghostex TUI sidebar ordering: gxserver already
     * returns projects and sessions in desktop sidebar order, so grouping should
     * preserve first project occurrence and per-project session order instead of
     * sorting or deriving Herdr work areas.
     */
    let mut indexes = BTreeMap::<String, usize>::new();
    let mut groups = Vec::<ProjectGroup>::new();
    for session in sessions {
        let key = session
            .project_id
            .clone()
            .unwrap_or_else(|| project_label(session));
        let idx = if let Some(idx) = indexes.get(&key).copied() {
            idx
        } else {
            let idx = groups.len();
            indexes.insert(key, idx);
            groups.push(ProjectGroup {
                project_id: session.project_id.clone(),
                group_id: session.group_id.clone(),
                name: if groups.is_empty()
                    && session.project_path.as_deref().unwrap_or("").is_empty()
                {
                    "Quick Terminals".to_string()
                } else {
                    project_label(session)
                },
                sessions: Vec::new(),
            });
            idx
        };
        groups[idx].sessions.push(session.clone());
    }
    groups
}

pub(crate) fn sidebar_rows(sessions: &[SessionItem]) -> Vec<SidebarRow> {
    let mut rows = Vec::new();
    for group in group_sessions(sessions) {
        rows.push(SidebarRow::Project(ProjectHeader { name: group.name }));
        rows.push(SidebarRow::NewTerminal {
            project_id: group.project_id.clone(),
            group_id: group.group_id.clone(),
        });
        rows.extend(group.sessions.into_iter().map(SidebarRow::Session));
    }
    rows
}

pub(crate) fn agent_indicator(session: &SessionItem) -> &'static str {
    match normalized_agent_key(session).as_str() {
        "agy" | "anti-gravity" | "anti-gravity-cli" | "antigravity" | "antigravity-cli" => "AGY",
        "amp" | "amp-cli" => "AMP",
        "claude" | "claude-code" | "claude-work" => "CLD",
        "codebuddy" | "code-buddy" => "CDB",
        "codex" | "codex-cli" | "work-codex" | "open-ai" | "openai" | "openai-codex" => "CDX",
        "copilot" | "github-copilot" => "PLT",
        "cursor" | "cursor-agent" | "cursor-cli" => "CRS",
        "droid" | "factory" | "factory-droid" => "DRD",
        "gemini" => "GEM",
        "grok" | "grok-build" => "GRK",
        "hermes" | "hermes-agent" => "HMS",
        "opencode" | "open-code" => "OPC",
        "pi" | "pi-agent" | "π" => "PIA",
        "qoder" | "qodercli" => "QDR",
        "rovo" | "rovo-dev" | "rovodev" => "RVO",
        _ => "UNK",
    }
}

pub(crate) fn normalized_agent_key(session: &SessionItem) -> String {
    session
        .agent
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_lowercase()
        .replace([' ', '_'], "-")
}

fn session_command_args(
    command: &str,
    session: &SessionItem,
    boolean: Option<bool>,
) -> Vec<String> {
    let mut args = vec![
        command.to_string(),
        "--session-id".to_string(),
        session.session_id.clone(),
    ];
    if let Some(project_id) = session
        .project_id
        .as_ref()
        .filter(|value| !value.trim().is_empty())
    {
        args.extend(["--project-id".to_string(), project_id.to_string()]);
    }
    if let Some(value) = boolean {
        args.push(value.to_string());
    }
    args
}

fn run_cli(args: &[String]) -> io::Result<Vec<u8>> {
    let mut command = cli_command();
    for arg in args {
        command.push(' ');
        command.push_str(&shell_quote(arg));
    }
    let output = Command::new("/bin/zsh").arg("-lc").arg(command).output()?;
    if !output.status.success() {
        return Err(io::Error::new(
            io::ErrorKind::Other,
            String::from_utf8_lossy(&output.stderr).trim().to_string(),
        ));
    }
    Ok(output.stdout)
}

fn cli_command() -> String {
    std::env::var("GHOSTEX_TUI_CLI_COMMAND").unwrap_or_else(|_| "gx".to_string())
}

fn shell_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\\''"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_session(project_id: &str, title: &str) -> SessionItem {
        SessionItem {
            activity: None,
            agent: Some("codex".to_string()),
            project_id: Some(project_id.to_string()),
            group_id: Some(format!("{project_id}-group")),
            is_favorite: Some(false),
            project_name: Some(project_id.to_string()),
            project_path: Some(format!("/{project_id}")),
            session_id: format!("{project_id}-{title}"),
            status: "idle".to_string(),
            title: title.to_string(),
        }
    }

    #[test]
    fn sidebar_rows_preserve_project_and_session_order_from_gxserver() {
        let rows = sidebar_rows(&[
            test_session("alpha", "one"),
            test_session("alpha", "two"),
            test_session("beta", "one"),
        ]);

        assert!(matches!(
            &rows[0],
            SidebarRow::Project(ProjectHeader { name, .. }) if name == "alpha"
        ));
        assert!(matches!(
            &rows[1],
            SidebarRow::NewTerminal {
                project_id: Some(project_id),
                group_id: Some(group_id),
            } if project_id == "alpha" && group_id == "alpha-group"
        ));
        assert!(matches!(
            &rows[2],
            SidebarRow::Session(SessionItem { title, .. }) if title == "one"
        ));
        assert!(matches!(
            &rows[3],
            SidebarRow::Session(SessionItem { title, .. }) if title == "two"
        ));
        assert!(matches!(
            &rows[4],
            SidebarRow::Project(ProjectHeader { name, .. }) if name == "beta"
        ));
        assert!(matches!(
            &rows[6],
            SidebarRow::Session(SessionItem { title, .. }) if title == "one"
        ));
    }

    #[test]
    fn agent_indicator_matches_old_tui_badges() {
        let mut session = test_session("alpha", "one");
        session.agent = Some("Claude Work".to_string());
        assert_eq!(agent_indicator(&session), "CLD");

        session.agent = Some("work-codex".to_string());
        assert_eq!(agent_indicator(&session), "CDX");

        session.agent = Some("unknown".to_string());
        assert_eq!(agent_indicator(&session), "UNK");
    }
}
