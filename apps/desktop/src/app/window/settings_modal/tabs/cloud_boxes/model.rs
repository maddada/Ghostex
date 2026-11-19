//! What the Cloud Boxes page reads from gxserver's `/api/agentbox` (`status` and `list`), turned
//! into the rows it draws. The JSON shapes are wire contract 3 of
//! docs/2026-10-01/agentbox/PLAN.md; every field is optional here so an older or newer gxserver
//! never blanks the page.
use serde_json::Value;

/// How a provider row is set up and what its buttons run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ProviderKind {
    /// Docker on this computer: `Set Up` (`agentbox install -p docker`).
    Local,
    /// A cloud provider: `Log In` then a one-time `Prepare`.
    Cloud,
    /// A registered remote Docker server (`docker:<alias>`): `Check`.
    RemoteDocker,
    /// The "Your own server (SSH)" row that adds one.
    RemoteDockerSetup,
}

#[derive(Clone, Debug)]
pub(super) struct ProviderRow {
    /// `docker`, `hetzner`, ..., `docker:<alias>`, or `remote-docker` for the add row.
    pub(super) id: String,
    pub(super) label: String,
    pub(super) description: String,
    pub(super) kind: ProviderKind,
    /// `None` while the status is unknown (checking, or gxserver could not answer).
    pub(super) ready: Option<bool>,
    pub(super) configured: Option<bool>,
    pub(super) prepared: Option<bool>,
    /// agentbox doctor's own words about this provider, when it is not ready.
    pub(super) detail: Option<String>,
}

impl ProviderRow {
    /// The `runLocation` / `agentboxDefaultLocation` value that picks this provider.
    pub(super) fn location(&self) -> Option<String> {
        match self.kind {
            ProviderKind::RemoteDockerSetup => None,
            _ => Some(format!("agentbox:{}", self.id)),
        }
    }

    /// The registered alias of a remote Docker row.
    pub(super) fn alias(&self) -> Option<&str> {
        self.id.strip_prefix("docker:")
    }
}

/// The built-in providers, in the order the page and the New Thread picker list them.
const KNOWN_PROVIDERS: [(&str, &str, &str, ProviderKind); 6] = [
    (
        "docker",
        "Docker",
        "On this computer. Free, and needs Docker running.",
        ProviderKind::Local,
    ),
    (
        "hetzner",
        "Hetzner",
        "A cloud server for each box.",
        ProviderKind::Cloud,
    ),
    (
        "vercel",
        "Vercel",
        "A cloud sandbox with a public preview link.",
        ProviderKind::Cloud,
    ),
    (
        "daytona",
        "Daytona",
        "A cloud sandbox.",
        ProviderKind::Cloud,
    ),
    (
        "e2b",
        "E2B",
        "A cloud sandbox with a public preview link.",
        ProviderKind::Cloud,
    ),
    (
        "digitalocean",
        "DigitalOcean",
        "A cloud server for each box.",
        ProviderKind::Cloud,
    ),
];

fn text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|text| !text.is_empty())
        .map(str::to_string)
}

fn flag(value: &Value, key: &str) -> Option<bool> {
    value.get(key).and_then(Value::as_bool)
}

/// The provider rows: the built-in ones (with the status's readiness when it has them), every
/// registered remote Docker server, and the "Your own server (SSH)" row that adds one.
pub(super) fn provider_rows(status: Option<&Value>) -> Vec<ProviderRow> {
    let reported: Vec<&Value> = status
        .and_then(|status| status.get("providers"))
        .and_then(Value::as_array)
        .map(|providers| providers.iter().collect())
        .unwrap_or_default();
    let find = |id: &str| reported.iter().copied().find(|row| row["id"] == id);
    let mut rows: Vec<ProviderRow> = KNOWN_PROVIDERS
        .iter()
        .map(|(id, label, description, kind)| {
            let row = find(id);
            ProviderRow {
                id: id.to_string(),
                label: label.to_string(),
                description: description.to_string(),
                kind: *kind,
                ready: row.and_then(|row| flag(row, "ready")),
                configured: row.and_then(|row| flag(row, "configured")),
                prepared: row.and_then(|row| flag(row, "prepared")),
                detail: row.and_then(|row| text(row, "detail").or_else(|| text(row, "hint"))),
            }
        })
        .collect();
    for row in &reported {
        let Some(id) = text(row, "id") else {
            continue;
        };
        let Some(alias) = id.strip_prefix("docker:") else {
            continue;
        };
        rows.push(ProviderRow {
            label: text(row, "label").unwrap_or_else(|| alias.to_string()),
            description: "Your server over SSH.".to_string(),
            kind: ProviderKind::RemoteDocker,
            ready: flag(row, "ready"),
            configured: flag(row, "configured"),
            prepared: flag(row, "prepared"),
            detail: text(row, "detail").or_else(|| text(row, "hint")),
            id,
        });
    }
    rows.push(ProviderRow {
        id: "remote-docker".to_string(),
        label: "Your own server (SSH)".to_string(),
        description: "Docker on a server you reach over SSH. Add it once by name.".to_string(),
        kind: ProviderKind::RemoteDockerSetup,
        ready: None,
        configured: None,
        prepared: None,
        detail: None,
    });
    rows
}

/// The `agentboxDefaultLocation` choices: this computer, every provider, each registered server,
/// and the saved value when it names a server this status does not list.
pub(super) fn location_options(rows: &[ProviderRow], saved: &str) -> Vec<(String, String)> {
    let computer = crate::host_name::local_computer_name();
    let mut options = vec![(computer.clone(), "local".to_string())];
    for row in rows {
        let Some(location) = row.location() else {
            continue;
        };
        let label = match (row.kind, row.ready) {
            (ProviderKind::Local, _) => format!("Docker on {computer}"),
            (ProviderKind::RemoteDocker, _) => format!("{} (your server)", row.label),
            _ => row.label.clone(),
        };
        let label = if row.ready == Some(false) {
            format!("{label} (not set up)")
        } else {
            label
        };
        options.push((label, location));
    }
    if !options.iter().any(|(_, value)| value == saved) {
        let label = saved
            .strip_prefix("agentbox:docker:")
            .map(|alias| format!("{alias} (your server)"))
            .unwrap_or_else(|| saved.to_string());
        options.push((label, saved.to_string()));
    }
    options
}

/// What the status card says about agentbox itself.
pub(super) struct StatusSummary {
    pub(super) supported: bool,
    pub(super) installed: Option<bool>,
    pub(super) version: Option<String>,
    pub(super) docker_ready: Option<bool>,
    pub(super) portless_installed: Option<bool>,
    pub(super) claude_signed_in: Option<bool>,
    pub(super) codex_signed_in: Option<bool>,
    /// The status's own `error` (agentbox doctor failed while agentbox is installed).
    pub(super) error: Option<String>,
}

pub(super) fn status_summary(status: Option<&Value>) -> StatusSummary {
    let Some(status) = status else {
        return StatusSummary {
            supported: true,
            installed: None,
            version: None,
            docker_ready: None,
            portless_installed: None,
            claude_signed_in: None,
            codex_signed_in: None,
            error: None,
        };
    };
    let sign_ins = status.get("agentSignIns").unwrap_or(&Value::Null);
    StatusSummary {
        supported: flag(status, "supported") != Some(false),
        installed: flag(status, "installed"),
        version: text(status, "version"),
        docker_ready: flag(status, "dockerReady"),
        portless_installed: flag(status, "portlessInstalled"),
        claude_signed_in: flag(sign_ins, "claude"),
        codex_signed_in: flag(sign_ins, "codex"),
        error: text(status, "error"),
    }
}

/// One box from `/api/agentbox list`.
#[derive(Clone, Debug)]
pub(super) struct BoxRow {
    pub(super) name: String,
    pub(super) agent: Option<String>,
    pub(super) provider: Option<String>,
    pub(super) state: Option<String>,
    pub(super) web_url: Option<String>,
    pub(super) session_id: Option<String>,
    pub(super) project_id: Option<String>,
    pub(super) session_title: Option<String>,
}

pub(super) fn box_rows(list: &Value) -> Vec<BoxRow> {
    list.get("boxes")
        .and_then(Value::as_array)
        .map(|boxes| {
            boxes
                .iter()
                .filter_map(|row| {
                    Some(BoxRow {
                        name: text(row, "name")?,
                        agent: text(row, "agent"),
                        provider: text(row, "provider"),
                        state: text(row, "state"),
                        web_url: text(row, "webUrl"),
                        session_id: text(row, "sessionId"),
                        project_id: text(row, "projectId"),
                        session_title: text(row, "sessionTitle"),
                    })
                })
                .collect()
        })
        .unwrap_or_default()
}

pub(super) fn agent_label(agent: &str) -> String {
    match agent {
        "claude" => "Claude".to_string(),
        "codex" => "Codex".to_string(),
        "opencode" => "OpenCode".to_string(),
        "pi" => "Pi".to_string(),
        other => other.to_string(),
    }
}

pub(super) fn provider_label(provider: &str) -> String {
    if let Some((_, label, _, _)) = KNOWN_PROVIDERS.iter().find(|(id, ..)| *id == provider) {
        return label.to_string();
    }
    match provider {
        "remote-docker" => "Your server".to_string(),
        other => other
            .strip_prefix("docker:")
            .map(str::to_string)
            .unwrap_or_else(|| other.to_string()),
    }
}

pub(super) fn state_label(state: &str) -> String {
    let mut label = state.replace(['-', '_'], " ");
    if let Some(first) = label.get_mut(..1) {
        first.make_ascii_uppercase();
    }
    label
}

/// A remote Docker alias agentbox accepts: letters, digits, `.`, `_` and `-`.
pub(super) fn valid_alias(alias: &str) -> bool {
    !alias.is_empty()
        && alias.len() <= 64
        && alias
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-'))
}

/// An `~/.ssh/config` name or `[user@]host[:port]`.
pub(super) fn valid_ssh_target(ssh: &str) -> bool {
    !ssh.is_empty()
        && ssh.len() <= 253
        && !ssh.starts_with('-')
        && ssh
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '.' | '_' | '-' | '@' | ':'))
}
