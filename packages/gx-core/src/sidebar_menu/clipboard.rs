//! The text Copy Details puts on the clipboard.
//!
//! SEE-ALSO: packages/shared/session-details-copy.ts, which records the decision that this copies
//! stable row metadata and never terminal output or the saved first prompt.

use crate::sidebar_view::view::SessionRow;

use super::text::{format_identifier, js_trim};

/// The project facts the text quotes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DetailsGroup<'a> {
    pub(crate) title: &'a str,
    pub(crate) project_path: Option<&'a str>,
    pub(crate) worktree_name: Option<&'a str>,
    pub(crate) worktree_branch: Option<&'a str>,
    pub(crate) remote_machine_name: Option<&'a str>,
    pub(crate) server_id: Option<&'a str>,
}

/// `buildSidebarSessionDetailsClipboardText`, trimmed to what a user or another agent needs.
///
/// CDXC:ContextMenus 2026-09-26 DECISION:
/// The user asked for Copy Details to carry only what another agent needs to reach this session through `$ghostex-agents`: first the agent, the title, the Global Ref, the agent session id and the zmx name (both kept because the user uses them), then where it runs (machine, project, path, worktree), and a last line pointing at `$ghostex-agents`. The title is the stored one, without the glyphs the daemon's display title carries. The internal fields (display title, sidebar and routing ids, kind, status, activity, terminal title, detail, parent project, last active) are gone even with Show debug UI controls on, which an earlier version of this decision allowed; `ghostex sessions --json --full` has them.
pub(crate) fn session_details_text(row: &SessionRow, group: &DetailsGroup<'_>) -> String {
    let facts = &row.menu_facts;
    let title = first_non_empty(&[
        Some(row.alias.as_str()),
        facts.primary_title.as_deref(),
        facts.raw_display_title.as_deref(),
    ]);
    let mut lines: Vec<String> = vec!["Ghostex Session".to_string()];
    let mut line = |label: &str, value: Option<&str>| {
        if let Some(value) = value {
            let value = js_trim(value);
            if !value.is_empty() {
                lines.push(format!("{label}: {value}"));
            }
        }
    };
    let agent = row.agent_icon.as_deref().map(format_identifier);
    line("Agent", agent.as_deref());
    line("Title", Some(&title));
    /*
    CDXC:Cli 2026-09-24 DECISION:
    The user asked that the copied block carry the one id every `ghostex` verb takes, so an agent
    handed it can read or message that thread without guessing: `<server>:<project>:<session>`.
    */
    let global_ref = match (group.server_id, row.key.as_ref()) {
        (Some(server_id), Some(key)) => {
            Some(format!("{server_id}:{}:{}", key.project_id, key.session_id))
        }
        _ => None,
    };
    line("Global Ref", global_ref.as_deref());
    line("Agent Session ID", facts.agent_session_id.as_deref());
    let persistence_name = facts
        .session_persistence_name
        .as_deref()
        .filter(|name| !name.is_empty());
    let persistence_provider = facts
        .session_persistence_provider
        .as_deref()
        .filter(|provider| !provider.is_empty())
        .unwrap_or("zmx");
    line(persistence_provider, persistence_name);
    line("Remote Machine", group.remote_machine_name);
    line("Project", Some(group.title));
    line("Project Path", group.project_path);
    line("Worktree", group.worktree_name);
    line("Worktree Branch", group.worktree_branch);
    lines.push("More details: use $ghostex-agents".to_string());
    lines.join("\n")
}

/// `pickFirstNonEmpty`: the first value with non-whitespace in it, trimmed, else `Session`.
fn first_non_empty(values: &[Option<&str>]) -> String {
    for value in values {
        if let Some(value) = value {
            let trimmed = js_trim(value);
            if !trimmed.is_empty() {
                return trimmed.to_string();
            }
        }
    }
    "Session".to_string()
}
