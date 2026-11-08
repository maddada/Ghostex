//! Which Linear workspace each Linear team's tickets live in, learned from the Linear issue URLs
//! gxserver sees, so a ticket linked by its ID alone (`ghostex link add THE-346`) still opens.
//!
//! CDXC:SessionLinks 2026-11-08 WHY:
//! A Linear ticket's page is `https://linear.app/<workspace>/issue/<ID>`, and only Linear knows the workspace. A ticket linked by its ID in a project with no Linear key (or whose key belongs to another workspace) was never read, so it had no URL and its row and sidebar chip did nothing when clicked. The workspace is learned per team from every Linear issue URL gxserver meets (Linear's own reads, a link added by URL, URLs in the user's or the agent's chat), and a ticket with no URL of its own gets one built from it. The sync pass stores the built URL with the link (`fill_linear_urls`), so it survives a restart.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

/// Linear team key (`THE`) to workspace URL key (`the-code-uae`).
fn workspaces() -> &'static Mutex<HashMap<String, String>> {
    static WORKSPACES: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    WORKSPACES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// The workspace and ticket ID of a Linear issue URL
/// (`https://linear.app/<workspace>/issue/GHX-142/some-title`).
pub(crate) fn linear_url_parts(text: &str) -> Option<(String, String)> {
    let text = text.trim();
    let rest = text
        .strip_prefix("https://linear.app/")
        .or_else(|| text.strip_prefix("http://linear.app/"))?;
    let mut parts = rest.split('/');
    let workspace = parts.next()?.to_ascii_lowercase();
    if parts.next()? != "issue" {
        return None;
    }
    let identifier = crate::work_mode::normalize_linear_identifier(parts.next()?)?;
    let valid = !workspace.is_empty()
        && workspace
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_');
    valid.then_some((workspace, identifier))
}

fn team_of(identifier: &str) -> &str {
    identifier.split('-').next().unwrap_or_default()
}

/// Learns the workspace of the team a Linear issue URL names; anything else is ignored.
pub(crate) fn remember_linear_workspace(url: &str) {
    let Some((workspace, identifier)) = linear_url_parts(url) else {
        return;
    };
    if let Ok(mut workspaces) = workspaces().lock() {
        workspaces.insert(team_of(&identifier).to_string(), workspace);
    }
}

/// The page of a Linear ticket whose team's workspace is known.
pub(crate) fn linear_issue_url(identifier: &str) -> Option<String> {
    let workspace = workspaces()
        .lock()
        .ok()?
        .get(team_of(identifier))
        .cloned()?;
    Some(format!("https://linear.app/{workspace}/issue/{identifier}"))
}
