//! What an agent session's browser requests may do: the addresses it may open or navigate to, the
//! agent session the app is told about, and the security-log line of a background open.
//!
//! CDXC:Browser 2026-11-26 DECISION:
//! Coordinator, on the user's behalf (security review of #762, findings B2, B8 and B9): an agent session (`Principal::Session`) opens and navigates to web pages only: `file://` and every other scheme but http, https and about:blank are refused, both for its opens through the app (`openBrowser`, `openBrowserPane`) and for its DevTools navigations (server/src/server/cef_devtools_scope.rs), so a file the upload check refuses cannot be read as a page instead. The agent session the app keeps a page for (`agentSessionId`) is always the caller's own, whatever its payload said, so one agent cannot steer the page another agent's next tool call drives. A background open goes into the security log after the app answered, with the page's host first and its whole normalised address, and the real outcome.
//! Coordinator, on the user's behalf (#762 follow-up): an on-screen open that "Let agents open and switch pages without asking" let through without a toast is logged the same way (`browser.agentOpen`), quietly like the background one (audit/notify.rs), so every page an agent opened without the user's click is in the log.
//! SEE-ALSO: apps/desktop/src/app/helpers/titlebar/urls_and_sessions.rs (`normalize_address`, which `normalized_browser_address` mirrors), apps/desktop/src/app/browser_agent_pages.rs (the agent's page, keyed by `agentSessionId`).

use serde_json::{json, Map, Value};

use crate::security::Principal;
use crate::server::AppState;

/// The longest address the security log keeps in full; the host is always logged first.
const LOGGED_ADDRESS_CHARS: usize = 2_000;

/// The renderer commands whose payload names the agent session a Browser page belongs to.
fn names_agent_page(action: &str) -> bool {
    matches!(
        action,
        "openBrowser" | "openBrowserPane" | "listBrowserPages"
    )
}

/// The address the app loads for `value` typed into or sent to the Browser, as
/// `normalize_address` in the desktop app computes it.
pub(crate) fn normalized_browser_address(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.contains("://") {
        return Some(trimmed.to_string());
    }
    if trimmed == "localhost"
        || trimmed.starts_with("localhost:")
        || trimmed.starts_with("127.0.0.1")
    {
        return Some(format!("http://{trimmed}"));
    }
    if trimmed.contains('.') && !trimmed.contains(' ') {
        return Some(format!("https://{trimmed}"));
    }
    Some(format!(
        "https://www.google.com/search?q={}",
        percent_encoding::utf8_percent_encode(trimmed, percent_encoding::NON_ALPHANUMERIC)
    ))
}

/// Why an agent may not navigate a page to `url` (an exact address, as DevTools takes it), or
/// `None` when it is a web page.
pub(crate) fn agent_navigation_refusal(url: &str) -> Option<String> {
    let trimmed = url.trim();
    if trimmed.eq_ignore_ascii_case("about:blank") {
        return None;
    }
    match url::Url::parse(trimmed) {
        Ok(parsed) if matches!(parsed.scheme(), "http" | "https") => None,
        Ok(parsed) => Some(format!(
            "An agent's browser tools open only http and https pages (and about:blank), not {}: addresses.",
            parsed.scheme()
        )),
        Err(_) => Some(format!(
            "{} is not a full web address; give it with http:// or https://.",
            trimmed.chars().take(200).collect::<String>()
        )),
    }
}

/// Why an agent may not open `url` in the Browser (as the app would normalise it), or `None`.
pub(crate) fn agent_open_refusal(url: &str) -> Option<String> {
    let address = normalized_browser_address(url)?;
    agent_navigation_refusal(&address)
}

/// The caller's own agent session as the app knows it (`server:project:session`), for a session
/// token; `None` for every other caller.
fn caller_session_ref(state: &AppState, caller: &Principal) -> Option<(String, String)> {
    match caller {
        Principal::Session {
            project_id,
            session_id,
            ..
        } => Some((
            project_id.clone(),
            crate::ids::create_global_session_ref(
                state.metadata.server_id.as_str(),
                project_id,
                session_id,
            ),
        )),
        _ => None,
    }
}

/// For a session token: the payload's agent session becomes the caller's own, and its page list
/// is its own project's. Returns why the request is refused, if it is: an open of an address
/// that is not a web page.
pub(crate) fn pin_agent_browser_request(
    state: &AppState,
    caller: &Principal,
    action: &str,
    payload: &mut Map<String, Value>,
) -> Option<String> {
    if !names_agent_page(action) {
        return None;
    }
    let (project_id, session_ref) = caller_session_ref(state, caller)?;
    payload.insert("agentSessionId".to_string(), json!(session_ref));
    if action == "listBrowserPages" {
        for key in ["groupId", "projectName", "projectPath"] {
            payload.remove(key);
        }
        payload.insert("projectId".to_string(), json!(project_id));
        return None;
    }
    let url = payload
        .get("url")
        .and_then(Value::as_str)
        .unwrap_or_default();
    agent_open_refusal(url)
}

/// What the security log names for an opened address: its host, then the whole address.
pub(crate) fn logged_address(url: &str) -> String {
    let Some(address) = normalized_browser_address(url) else {
        return "no address".to_string();
    };
    let host = url::Url::parse(&address)
        .ok()
        .and_then(|parsed| parsed.host_str().map(str::to_string))
        .unwrap_or_else(|| "no host".to_string());
    let shown: String = address.chars().take(LOGGED_ADDRESS_CHARS).collect();
    let more = address.chars().count().saturating_sub(LOGGED_ADDRESS_CHARS);
    if more > 0 {
        format!("{host} ({shown}... {more} more characters)")
    } else {
        format!("{host} ({shown})")
    }
}

/// An agent's page open that no toast asked the user about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AgentOpen {
    /// A background open: the page loads in a tab that is not shown.
    Background,
    /// An open "Let agents open and switch pages without asking" let through: the page is shown.
    OnScreen,
}

impl AgentOpen {
    /// The security log's action id and line for this open.
    fn action(self) -> (&'static str, &'static str) {
        match self {
            Self::Background => (
                "browser.backgroundOpen",
                "Opened a Browser page in the background",
            ),
            Self::OnScreen => ("browser.agentOpen", "Opened a Browser page without asking"),
        }
    }
}

/// Writes an agent's open that no toast asked about into the security log, after the app answered.
pub(crate) fn record_agent_open(kind: AgentOpen, url: &str, answer: Result<&Value, &str>) {
    let outcome = match answer {
        Ok(result) if result.get("ok") == Some(&Value::Bool(false)) => {
            crate::audit::Outcome::Failed(
                result
                    .get("error")
                    .and_then(Value::as_str)
                    .unwrap_or("The page could not be opened.")
                    .to_string(),
            )
        }
        Ok(_) => crate::audit::Outcome::Done,
        Err(message) => crate::audit::Outcome::Failed(message.to_string()),
    };
    let (id, line) = kind.action();
    crate::audit::record(
        crate::audit::Action::new(id, line).target(logged_address(url)),
        crate::audit::Actor::current(),
        outcome,
        None,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agents_open_only_web_pages() {
        for url in [
            "file:///etc/passwd",
            " FILE:///Users/me/.ssh/id_ed25519",
            "chrome://settings",
            "view-source:https://example.com",
            "ghostex://settings",
        ] {
            assert!(agent_open_refusal(url).is_some(), "{url}");
        }
        for url in [
            "https://example.com",
            "example.com/a",
            "localhost:3000",
            "127.0.0.1:8080/x",
            "how to bake bread",
        ] {
            assert_eq!(agent_open_refusal(url), None, "{url}");
        }
    }

    #[test]
    fn the_log_names_the_real_host_first() {
        let padding = "a".repeat(180);
        let tricky = format!("https://accounts.google.com{padding}@evil.example/login");
        let logged = logged_address(&tricky);
        assert!(logged.starts_with("evil.example ("), "{logged}");
        assert!(logged.contains(&padding));
        let long = format!("https://example.com/{}", "b".repeat(3_000));
        let logged = logged_address(&long);
        assert!(logged.starts_with("example.com ("));
        assert!(logged.ends_with("more characters)"));
        assert_eq!(
            logged_address("localhost:3000"),
            "localhost (http://localhost:3000)"
        );
        assert_eq!(logged_address("  "), "no address");
    }

    #[test]
    fn both_kinds_of_open_are_quiet_log_lines() {
        assert_eq!(AgentOpen::Background.action().0, "browser.backgroundOpen");
        assert_eq!(AgentOpen::OnScreen.action().0, "browser.agentOpen");
        for kind in [AgentOpen::Background, AgentOpen::OnScreen] {
            assert!(crate::audit::is_quiet_agent_action(kind.action().0));
        }
    }
}
