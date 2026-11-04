//! AgentBox (Cloud Boxes): the rules every client shares about where a new agent session can run.
//!
//! gxserver owns the boxes (`/api/agentbox`, `runLocation` on `/api/createAgentSession`); this
//! module only reads its status answer into the run locations a picker or a menu offers, says
//! which agents a box can run, and spells the `runLocation` a launch carries.
//!
//! CDXC:AgentBox 2026-10-01 WHY:
//! Only providers gxserver reports `ready` are offered, so a pick always names a box that can be
//! created right now; configured-but-not-ready providers are set up in Settings > Cloud Boxes. A
//! status that could not be read offers nothing but this computer, so a gxserver without the
//! endpoint behaves exactly as before.
//!
//! SEE-ALSO: docs/2026-10-01/agentbox/PLAN.md (wire contracts 1-3),
//! apps/desktop/src/app/window/new_thread_picker_run_on.rs (the picker's Run on row),
//! packages/gx-core/src/sidebar_menu/run_in_box.rs (the launcher's Run in a Box pages).

use serde_json::Value;

use crate::core::Core;
use crate::keys::SessionKey;

/// The `runLocation` of a session that runs on this computer.
pub const LOCAL_RUN_LOCATION: &str = "local";

const RUN_LOCATION_PREFIX: &str = "agentbox:";

/// One place a new session can run in a box, as `/api/agentbox status` reported it ready.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentboxLocation {
    /// The provider id: `docker`, `hetzner`, `vercel`, `daytona`, `e2b`, `digitalocean`, or
    /// `docker:<alias>` for a registered remote Docker host.
    pub provider: String,
    /// What a chip or a menu row shows: "Docker", "Hetzner", or a registered SSH host's alias
    /// ("selfhost"); the server glyph says it is reached over SSH.
    pub label: String,
    /// gxserver's one-line description ("On this computer", "Cloud VPS", ...).
    pub description: String,
    /// `local`, `cloud` or `remoteDocker`.
    pub kind: String,
}

impl AgentboxLocation {
    /// The `runLocation` a launch in this box carries.
    pub fn run_location(&self) -> String {
        format!("{RUN_LOCATION_PREFIX}{}", self.provider)
    }

    /// The hover text of a chip or row: `Your server <alias> over SSH` for a registered remote
    /// Docker host, nothing for the others (their label says it all).
    pub fn tooltip(&self) -> Option<String> {
        ghostex_gx_protocol::agentbox::agentbox_location_tooltip(&self.provider)
    }

    /// The menu icon id for this kind of box: a box on this computer, a server over SSH, or the
    /// cloud.
    pub fn icon(&self) -> &'static str {
        ghostex_gx_protocol::agentbox::agentbox_location_icon(&self.kind)
    }
}

/// The box a session's agent runs in (`presentation.agentbox`), as a sidebar row carries it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SessionAgentbox {
    /// The provider id, as in [`AgentboxLocation::provider`].
    pub provider: String,
    /// The agentbox box name every `agentbox … <box>` command takes.
    pub box_name: String,
    /// The provider's short label ("Docker", "Hetzner", or a remote host's alias).
    pub provider_label: String,
}

impl SessionAgentbox {
    /// The label a row badge shows: gxserver's label, else the provider id.
    pub fn badge_label(&self) -> &str {
        if self.provider_label.trim().is_empty() {
            self.provider
                .strip_prefix("docker:")
                .unwrap_or(&self.provider)
        } else {
            self.provider_label.trim()
        }
    }
}

/// What `/api/agentbox status` says about running sessions in boxes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct AgentboxLocations {
    /// `false` where agentbox cannot run at all (native Windows).
    pub supported: bool,
    /// The `agentbox` CLI was found on the login shell's PATH.
    pub installed: bool,
    /// The locations a session can start in right now, in gxserver's order.
    pub ready: Vec<AgentboxLocation>,
}

impl AgentboxLocations {
    /// The ready location a `runLocation` names, if any.
    pub fn find(&self, run_location: &str) -> Option<&AgentboxLocation> {
        self.ready
            .iter()
            .find(|location| location.run_location() == run_location)
    }
}

/// Reads an `/api/agentbox` `{"action":"status"}` answer. A status gxserver could not produce
/// (`error` set, not supported, not installed) offers no ready location.
pub fn agentbox_locations_from_status(status: &Value) -> AgentboxLocations {
    let supported = status.get("supported").and_then(Value::as_bool) == Some(true);
    let installed = status.get("installed").and_then(Value::as_bool) == Some(true);
    let failed = status
        .get("error")
        .is_some_and(|error| !error.is_null() && error.as_str() != Some(""));
    let ready = if supported && installed && !failed {
        status
            .get("providers")
            .and_then(Value::as_array)
            .map(|providers| providers.iter().filter_map(ready_location).collect())
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    AgentboxLocations {
        supported,
        installed,
        ready,
    }
}

fn ready_location(provider: &Value) -> Option<AgentboxLocation> {
    if provider.get("ready").and_then(Value::as_bool) != Some(true) {
        return None;
    }
    let text = |key: &str| {
        provider
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .unwrap_or_default()
            .to_string()
    };
    let id = text("id");
    if !valid_provider_id(&id) {
        return None;
    }
    let kind = text("kind");
    let label = match text("label") {
        label if label.is_empty() => id.strip_prefix("docker:").unwrap_or(&id).to_string(),
        label => label,
    };
    Some(AgentboxLocation {
        provider: id,
        label,
        description: text("description"),
        kind,
    })
}

/// A provider id gxserver can take back in a `runLocation`: a known provider name or
/// `docker:<alias>`, never the `remote-docker` setup entry.
fn valid_provider_id(id: &str) -> bool {
    let allowed = |ch: char| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '@' | '-');
    match id.strip_prefix("docker:") {
        Some(alias) => !alias.is_empty() && alias.len() <= 253 && alias.chars().all(allowed),
        None => matches!(
            id,
            "docker" | "hetzner" | "vercel" | "daytona" | "e2b" | "digitalocean"
        ),
    }
}

/// Whether a session can never show Chat View, whatever its agent's Default Agent View says: an
/// agentbox session's agent runs in its box, so no transcript exists where Ghostex reads one.
///
/// CDXC:AgentBox 2026-10-01 WHY:
/// This is the one rule every client asks before it puts a session on its chat surface (the
/// desktop's chat gate and row focus, the web page's surface choice), so a deep link, a restore,
/// a notification, `ghostex focus`, Quick Access and a row click all land on the terminal alike;
/// do not add per-entry checks for box sessions.
///
/// A draft whose chat Run on row picked a box (`agentbox.pending`) keeps Chat View: nothing runs
/// in its box until its first message, which creates the box and clears `pending`, and that flip
/// is what hands the thread to its terminal on every client.
///
/// A Claude cloud session has no transcript here either: its conversation lives on claude.ai. Its
/// chat takes the first message while it waits for its task; once its claude.ai page exists, a
/// client that cannot show that page in its chat pane (the web build, the phone) shows the
/// session's terminal, which reads the conversation (`session_chat_view_unavailable_showing_cloud_pages`
/// is the desktop's rule).
pub fn session_chat_view_unavailable(core: &Core, session: &SessionKey) -> bool {
    core.presentation().session(session).is_some_and(|session| {
        session
            .claude_cloud
            .as_ref()
            .is_some_and(|cloud| !matches!(cloud.state.as_str(), "draft" | "starting" | "failed"))
            || session
                .agentbox
                .as_ref()
                .is_some_and(|agentbox| !agentbox.pending)
    })
}

/// `session_chat_view_unavailable` for a client whose chat pane shows a Claude cloud session's
/// claude.ai page (the desktop, CDXC:ClaudeCloud in claude_cloud.rs): every cloud session keeps
/// Chat View.
pub fn session_chat_view_unavailable_showing_cloud_pages(
    core: &Core,
    session: &SessionKey,
) -> bool {
    core.presentation().session(session).is_some_and(|session| {
        session.claude_cloud.is_none()
            && session
                .agentbox
                .as_ref()
                .is_some_and(|agentbox| !agentbox.pending)
    })
}

/// Whether a `runLocation` names a box rather than this computer.
pub fn is_agentbox_run_location(run_location: Option<&str>) -> bool {
    run_location
        .map(str::trim)
        .and_then(|location| location.strip_prefix(RUN_LOCATION_PREFIX))
        .is_some_and(|provider| !provider.is_empty())
}

/// What a toast or a label calls a box location named only by its `runLocation` (one the status
/// did not report ready): the provider's name, or the alias of a remote Docker host.
pub fn agentbox_location_label(run_location: &str) -> String {
    let provider = run_location
        .trim()
        .strip_prefix(RUN_LOCATION_PREFIX)
        .unwrap_or(run_location);
    if let Some(alias) = provider.strip_prefix("docker:") {
        return alias.to_string();
    }
    match provider {
        "docker" => "Docker",
        "hetzner" => "Hetzner",
        "vercel" => "Vercel",
        "daytona" => "Daytona",
        "e2b" => "E2B",
        "digitalocean" => "DigitalOcean",
        other => other,
    }
    .to_string()
}

/// The agent agentbox runs for a launcher agent: Claude, Codex, OpenCode and Pi, by the agent's
/// icon (its family) or, without one, its id. `None` for every other agent.
pub fn agentbox_agent_family(agent_id: &str, icon: Option<&str>) -> Option<&'static str> {
    // Claude (cloud) shows Claude's icon but runs on claude.ai, never in a box.
    if crate::claude_cloud::is_claude_cloud_agent(agent_id) {
        return None;
    }
    match icon.unwrap_or(agent_id).trim() {
        "claude" => Some("claude"),
        "codex" => Some("codex"),
        "opencode" => Some("opencode"),
        "pi" => Some("pi"),
        _ => None,
    }
}
