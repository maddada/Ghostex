/*
CDXC:AnonymousAnalytics 2026-08-26:
The daily `heartbeat`: the one event that answers "how many people use Ghostex,
and what does a typical install look like". Everything in it is a COUNT or an
ENUM read from state gxserver already owns — never a name, a path, or an id.

`agents_used` is the only aggregate: the distinct agent ids across the session
table, with every unknown (i.e. user-authored) id collapsed into a single
`custom` entry, so the array's length can never leak how many custom agents
someone configured beyond "at least one".
*/

use std::collections::BTreeSet;

use serde_json::Value;

use crate::{domain::DomainRepository, paths::GxserverPaths};

use super::{
    gate::SETTINGS_FILE_NAME,
    queue::capture,
    taxonomy::{self, PropertyValue},
};

/// A heartbeat sent at start is only worth sending if the last one is a full
/// working day old; 20h rather than 24h so a user who opens Ghostex at roughly
/// the same time each morning is not skipped by a few minutes of drift.
pub const HEARTBEAT_STALE_HOURS: i64 = 20;

/// Everything the heartbeat reports, resolved before any of it is validated so
/// the collection step has no opinion about the taxonomy.
pub struct HeartbeatSnapshot {
    pub project_count: usize,
    pub session_count: usize,
    pub running_session_count: usize,
    pub agents_used: Vec<&'static str>,
    pub default_agent: &'static str,
    pub preferred_interface: &'static str,
    pub sidebar_version: &'static str,
    pub sidebar_v2_layout: &'static str,
    pub extension_count: usize,
    pub remote_machine_count: usize,
    pub days_since_install: i64,
}

pub fn emit(snapshot: &HeartbeatSnapshot) {
    capture(
        taxonomy::EVENT_HEARTBEAT,
        &[
            (
                "project_count",
                PropertyValue::Number(snapshot.project_count as f64),
            ),
            (
                "session_count",
                PropertyValue::Number(snapshot.session_count as f64),
            ),
            (
                "running_session_count",
                PropertyValue::Number(snapshot.running_session_count as f64),
            ),
            (
                "agents_used",
                PropertyValue::EnumList(snapshot.agents_used.clone()),
            ),
            ("default_agent", PropertyValue::Enum(snapshot.default_agent)),
            (
                "preferred_interface",
                PropertyValue::Enum(snapshot.preferred_interface),
            ),
            (
                "sidebar_version",
                PropertyValue::Enum(snapshot.sidebar_version),
            ),
            (
                "sidebar_v2_layout",
                PropertyValue::Enum(snapshot.sidebar_v2_layout),
            ),
            (
                "extension_count",
                PropertyValue::Number(snapshot.extension_count as f64),
            ),
            (
                "remote_machine_count",
                PropertyValue::Number(snapshot.remote_machine_count as f64),
            ),
            (
                "days_since_install",
                PropertyValue::Number(snapshot.days_since_install as f64),
            ),
        ],
    );
}

/// Counts from the domain tables. Recent projects are the user-visible project
/// list (hidden and carrier rows already excluded by `list_recent_projects`).
pub fn collect_domain_counts(
    repository: &DomainRepository<'_>,
) -> (usize, usize, usize, Vec<&'static str>) {
    let project_count = repository
        .list_recent_projects()
        .map(|projects| projects.len())
        .unwrap_or(0);
    let sessions = repository.list_sessions(None).unwrap_or_default();
    let session_count = sessions.len();
    let running_session_count = sessions
        .iter()
        .filter(|session| {
            session
                .get("lifecycleState")
                .and_then(Value::as_str)
                .map(|state| state == "running")
                .unwrap_or(false)
        })
        .count();
    let mut agents_used: BTreeSet<&'static str> = BTreeSet::new();
    for session in &sessions {
        let Some(agent_id) = session.get("agentId").and_then(Value::as_str) else {
            continue;
        };
        if agent_id.trim().is_empty() {
            continue;
        }
        agents_used.insert(taxonomy::normalize_agent_id(Some(agent_id)));
    }
    (
        project_count,
        session_count,
        running_session_count,
        agents_used.into_iter().collect(),
    )
}

/// The three enum settings plus the remote-machine COUNT, read straight out of
/// the shared settings file. Unknown or missing values normalize to the shipped
/// defaults, which is what the app itself renders in that situation.
pub struct SettingsSnapshot {
    pub preferred_interface: &'static str,
    pub sidebar_version: &'static str,
    pub sidebar_v2_layout: &'static str,
    pub remote_machine_count: usize,
}

pub fn collect_settings(paths: &GxserverPaths) -> SettingsSnapshot {
    let settings = std::fs::read_to_string(paths.app_config_dir.join(SETTINGS_FILE_NAME))
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok());
    let read_enum = |key: &str, table: &'static [&'static str], default: &'static str| {
        settings
            .as_ref()
            .and_then(|settings| settings.get(key))
            .and_then(Value::as_str)
            .and_then(|value| taxonomy::match_enum(table, value))
            .unwrap_or(default)
    };
    SettingsSnapshot {
        preferred_interface: read_enum(
            "preferredAgentInterface",
            taxonomy::INTERFACE_KINDS,
            "chat",
        ),
        sidebar_version: read_enum("sidebarVersion", taxonomy::SIDEBAR_VERSIONS, "v1"),
        sidebar_v2_layout: read_enum("sidebarV2Layout", taxonomy::SIDEBAR_V2_LAYOUTS, "byProject"),
        /*
        The COUNT only. Machine names, hosts, users, ports, and key paths are
        exactly the kind of thing the "never sent" list forbids, and the length
        of the array answers the only question worth asking here.
        */
        remote_machine_count: settings
            .as_ref()
            .and_then(|settings| settings.get("remoteMachines"))
            .and_then(Value::as_array)
            .map(|machines| machines.len())
            .unwrap_or(0),
    }
}

/// Whole days since `identity.json`'s `createdAt`. A coarse install age, which
/// is what retention analysis needs; the exact timestamp never leaves.
pub fn days_since_install(created_at: &str) -> i64 {
    let Ok(created_at) = chrono::DateTime::parse_from_rfc3339(created_at) else {
        return 0;
    };
    chrono::Utc::now()
        .signed_duration_since(created_at.with_timezone(&chrono::Utc))
        .num_days()
        .max(0)
}

/// `defaultPromptAgentId` from the `agents.settings.v1` metadata row,
/// whitelisted through the catalog like every other agent id.
pub fn default_agent_from_settings(settings: Option<&Value>) -> &'static str {
    taxonomy::normalize_agent_id(
        settings
            .and_then(|settings| settings.get("defaultPromptAgentId"))
            .and_then(Value::as_str),
    )
}
