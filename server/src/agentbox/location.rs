//! Where a session runs: the `runLocation` create param, the provider catalog, and box names.

use serde_json::{Map, Value};

use crate::domain::DomainStateError;

/// One agentbox backend Ghostex offers as a run location.
pub(crate) struct ProviderSpec {
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
    pub(crate) description: &'static str,
    /// `local` (Docker on this computer) or `cloud`.
    pub(crate) kind: &'static str,
}

/// The fixed providers, in the order the Settings page and the picker list them. Remote Docker hosts
/// are added per registered alias (`docker:<alias>`, see `status.rs`).
pub(crate) const PROVIDERS: &[ProviderSpec] = &[
    ProviderSpec {
        id: "docker",
        label: "Docker",
        description: "On this computer",
        kind: "local",
    },
    ProviderSpec {
        id: "hetzner",
        label: "Hetzner",
        description: "Cloud VPS",
        kind: "cloud",
    },
    ProviderSpec {
        id: "vercel",
        label: "Vercel",
        description: "Cloud sandbox",
        kind: "cloud",
    },
    ProviderSpec {
        id: "daytona",
        label: "Daytona",
        description: "Cloud sandbox",
        kind: "cloud",
    },
    ProviderSpec {
        id: "e2b",
        label: "E2B",
        description: "Cloud sandbox",
        kind: "cloud",
    },
    ProviderSpec {
        id: "digitalocean",
        label: "DigitalOcean",
        description: "Cloud VPS",
        kind: "cloud",
    },
];

/// The agents agentbox can run, by Ghostex agent family id.
pub(crate) const AGENTBOX_AGENTS: &[&str] = &["claude", "codex", "opencode", "pi"];

pub(crate) const REMOTE_DOCKER_PREFIX: &str = "docker:";

pub(crate) fn provider_spec(id: &str) -> Option<&'static ProviderSpec> {
    PROVIDERS.iter().find(|provider| provider.id == id)
}

pub(crate) fn is_cloud_provider(id: &str) -> bool {
    provider_spec(id).is_some_and(|provider| provider.kind == "cloud")
}

/// The remote Docker alias of a `docker:<alias>` provider.
pub(crate) fn remote_docker_alias(provider: &str) -> Option<&str> {
    provider
        .strip_prefix(REMOTE_DOCKER_PREFIX)
        .filter(|alias| is_valid_alias(alias))
}

/// Short label for a provider: the catalog label, or the alias of a remote Docker host.
pub(crate) fn provider_label(provider: &str) -> String {
    if let Some(alias) = remote_docker_alias(provider) {
        return alias.to_string();
    }
    provider_spec(provider)
        .map(|spec| spec.label.to_string())
        .unwrap_or_else(|| provider.to_string())
}

/// A remote Docker host alias as `agentbox remote-docker add` accepts it.
pub(crate) fn is_valid_alias(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && !value.starts_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._-".contains(&byte))
}

/// An SSH target: a `~/.ssh/config` alias or `[user@]host[:port]`.
pub(crate) fn is_valid_ssh_target(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 255
        && !value.starts_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || b"._@:-".contains(&byte))
}

/// A box name as agentbox accepts it: lowercase letters, digits and dashes.
pub(crate) fn is_valid_box_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 63
        && !value.starts_with('-')
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

/// The provider a create's `runLocation` asks for, or `None` for this computer.
///
/// CDXC:AgentBox 2026-10-01 DECISION:
/// User: "when spinning up a thread i should be able to pick to spin it up in one of these clouds". The run location is a property of the session decided here in gxserver, so the desktop, the web build, the `ghostex` CLI and remote clients all create box sessions the same way; absent, `""` or `"local"` keep today's local launch.
pub(crate) fn requested_agentbox_provider(
    params: &Map<String, Value>,
) -> Result<Option<String>, DomainStateError> {
    let value = match params.get("runLocation") {
        None | Some(Value::Null) => return Ok(None),
        Some(Value::String(value)) => value.trim(),
        Some(_) => {
            return Err(DomainStateError::bad_request(
                "runLocation must be a string such as \"local\" or \"agentbox:docker\".",
            ))
        }
    };
    // Claude cloud is not a box (server/src/claude_cloud/create.rs settles it).
    if value.is_empty()
        || value == "local"
        || value == crate::claude_cloud::CLAUDE_CLOUD_RUN_LOCATION
    {
        return Ok(None);
    }
    let provider = value
        .strip_prefix("agentbox:")
        .filter(|provider| provider_spec(provider).is_some() || remote_docker_alias(provider).is_some())
        .ok_or_else(|| {
            DomainStateError::bad_request(format!(
                "\"{value}\" is not a run location. Use local, agentbox:docker, agentbox:hetzner, agentbox:vercel, agentbox:daytona, agentbox:e2b, agentbox:digitalocean or agentbox:docker:<host alias>."
            ))
        })?;
    if cfg!(windows) {
        return Err(DomainStateError::bad_request(
            "AgentBox runs on macOS and Linux only.",
        ));
    }
    if !cloud_boxes_enabled() {
        return Err(DomainStateError::bad_request(
            ghostex_settings_catalog::built_in_extensions::turned_off_message(
                ghostex_settings_catalog::built_in_extensions::CLOUD_BOXES,
            ),
        ));
    }
    Ok(Some(provider.to_string()))
}

/// The CLI shorthand of `ghostex create-agent --run-on`: `local`, a provider id, or `docker:<alias>`.
pub(crate) fn run_location_from_cli(value: &str) -> Result<String, String> {
    let value = value.trim();
    if value.is_empty() || value == "local" {
        return Ok("local".to_string());
    }
    if value == crate::claude_cloud::CLAUDE_CLOUD_RUN_LOCATION {
        return Ok(value.to_string());
    }
    let provider = value.strip_prefix("agentbox:").unwrap_or(value);
    if provider_spec(provider).is_some() || remote_docker_alias(provider).is_some() {
        return Ok(format!("agentbox:{provider}"));
    }
    Err(format!(
        "--run-on {value} is not a run location. Use local, claude-cloud, docker, hetzner, vercel, daytona, e2b, digitalocean or docker:<host alias>."
    ))
}

/// `gx-<project folder slug, at most 16>-<6 lowercase hex>`: unique per session, kept for its life.
///
/// CDXC:AgentBox 2026-10-01 WHY: the name is chosen when the create is normalized, before the repository assigns a session id, so the unique tail is random rather than derived from that id. It is stored in `runtimeSettings.agentbox.boxName` and never regenerated, so restores always reach the same box.
pub(crate) fn new_box_name(project_path: &str) -> String {
    let folder = project_path
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()
        .unwrap_or_default();
    let mut slug = String::new();
    for ch in folder.chars() {
        let ch = ch.to_ascii_lowercase();
        if ch.is_ascii_lowercase() || ch.is_ascii_digit() {
            slug.push(ch);
        } else if !slug.ends_with('-') && !slug.is_empty() {
            slug.push('-');
        }
    }
    let mut slug: String = slug.chars().take(16).collect();
    while slug.ends_with('-') {
        slug.pop();
    }
    if slug.is_empty() {
        slug.push_str("box");
    }
    let tail: String = uuid::Uuid::new_v4()
        .simple()
        .to_string()
        .chars()
        .take(6)
        .collect();
    format!("gx-{slug}-{tail}")
}

/// Whether the Cloud Boxes built-in extension is on (Settings > Extensions; never on Windows). While
/// it is off no client can start a box and every status answers "not supported", so the desktop,
/// the web build and the phone's Run on rows drop their box choices together.
///
/// CDXC:AgentBox 2026-10-06 SEE-ALSO: packages/settings-catalog/src/data/official_extensions.rs (the user decision).
pub(crate) fn cloud_boxes_enabled() -> bool {
    let paths = crate::paths::get_gxserver_paths(None);
    ghostex_settings_catalog::built_in_extensions::enabled_in_value(
        crate::session_lifecycle::read_sidebar_settings(&paths).as_ref(),
        ghostex_settings_catalog::built_in_extensions::CLOUD_BOXES,
    )
}
