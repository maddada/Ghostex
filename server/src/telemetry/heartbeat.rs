/*
CDXC:Telemetry 2026-08-26:
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
    extensions::{EnabledExtensions, ExtensionRef},
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
    /// Executables of the user's custom agents that map to no catalog agent.
    /// Sorted, deduplicated, and capped, so the list can never be a fingerprint
    /// of how many custom agents someone has.
    pub custom_agent_executables: Vec<String>,
    pub default_agent: &'static str,
    pub preferred_interface: &'static str,
    pub extension_count: usize,
    /// Built-in extensions that are switched on (settings-catalog ids).
    pub built_in_extensions_on: Vec<&'static str>,
    /// Enabled installed extensions: official Store ids, and a count of the rest.
    pub enabled_extensions: EnabledExtensions,
    pub remote_machine_count: usize,
    pub days_since_install: i64,
}

/*
CDXC:Telemetry 2026-08-27 (addendum v2, §2):
The person properties. `$set` (overwrite), never `$set_once`, so the person page
always shows what the install looks like TODAY rather than on the day it first
reported.

It is attached to the heartbeat and nothing else, on purpose: every other event
would rewrite the same record with the same numbers, and putting `$set` on a
high-frequency event is how person properties end up churning.

Two deliberate omissions:
- `agents_used` — an array is a poor person property (no breakdown, no cohort
  filter that behaves), so it stays an event property.
- `country_free` from the addendum's list — Ghostex never sends location. PostHog
  derives country from the sender IP through its own GeoIP transformation, and
  the base plan's "never sent" list forbids us shipping one ourselves, so there
  is no value here to set that would not be invented.

`os` and `arch` go through `match_enum` rather than being sent verbatim: an
unrecognised target would fail validation for the WHOLE heartbeat, and a
platform we have not enumerated is not worth losing the event over.
*/
fn person_properties(snapshot: &HeartbeatSnapshot) -> PropertyValue {
    let mut person: Vec<(&'static str, PropertyValue)> = Vec::with_capacity(14);
    if let Some(os) = taxonomy::match_enum(taxonomy::PLATFORMS, std::env::consts::OS) {
        person.push(("os", PropertyValue::Enum(os)));
    }
    if let Some(arch) = taxonomy::match_enum(taxonomy::ARCHES, std::env::consts::ARCH) {
        person.push(("arch", PropertyValue::Enum(arch)));
    }
    person.push((
        "server_version",
        PropertyValue::Version(super::base::SERVER_MARKETING_VERSION.to_string()),
    ));
    person.push((
        "interface",
        PropertyValue::Enum(snapshot.preferred_interface),
    ));
    person.push(("default_agent", PropertyValue::Enum(snapshot.default_agent)));
    person.push((
        "project_count",
        PropertyValue::Number(snapshot.project_count as f64),
    ));
    person.push((
        "session_count",
        PropertyValue::Number(snapshot.session_count as f64),
    ));
    person.push((
        "extension_count",
        PropertyValue::Number(snapshot.extension_count as f64),
    ));
    person.push((
        "remote_machine_count",
        PropertyValue::Number(snapshot.remote_machine_count as f64),
    ));
    person.push((
        "days_since_install",
        PropertyValue::Number(snapshot.days_since_install as f64),
    ));
    person.push((
        "project_bucket",
        PropertyValue::Enum(taxonomy::project_bucket(snapshot.project_count)),
    ));
    if let Some(identity_source) = super::queue::identity_source() {
        person.push(("identity_source", PropertyValue::Enum(identity_source)));
    }
    PropertyValue::PersonSet(person)
}

pub fn emit(snapshot: &HeartbeatSnapshot) {
    capture(taxonomy::EVENT_HEARTBEAT, &heartbeat_properties(snapshot));
}

/*
CDXC:Telemetry 2026-10-27:
User: "i wanna track please what extensions people have enabled so i know what
they're using in the app". The daily heartbeat is the snapshot: which built-in
extensions are switched on (`built_in_extensions_on`), which official Store
extensions are installed and enabled (`store_extensions_enabled`), and how many
other enabled extensions there are (`custom_extensions_enabled`), never their
names. See `telemetry::extensions` for the whitelist.
*/
pub fn heartbeat_properties(snapshot: &HeartbeatSnapshot) -> Vec<(&'static str, PropertyValue)> {
    vec![
        (taxonomy::PERSON_SET_KEY, person_properties(snapshot)),
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
        (
            "custom_agent_executables",
            PropertyValue::TokenList(snapshot.custom_agent_executables.clone()),
        ),
        ("default_agent", PropertyValue::Enum(snapshot.default_agent)),
        (
            "preferred_interface",
            PropertyValue::Enum(snapshot.preferred_interface),
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
        (
            "built_in_extensions_on",
            PropertyValue::ExtensionList(
                snapshot
                    .built_in_extensions_on
                    .iter()
                    .map(|id| ExtensionRef::BuiltIn(id))
                    .collect(),
            ),
        ),
        (
            "store_extensions_enabled",
            PropertyValue::ExtensionList(
                snapshot
                    .enabled_extensions
                    .store
                    .iter()
                    .map(|id| ExtensionRef::Store(id.clone()))
                    .collect(),
            ),
        ),
        (
            "custom_extensions_enabled",
            PropertyValue::Number(snapshot.enabled_extensions.custom_count as f64),
        ),
    ]
}

/// Counts from the domain tables. Recent projects are the user-visible project
/// list (hidden and carrier rows already excluded by `list_recent_projects`).
pub struct DomainCounts {
    pub project_count: usize,
    pub session_count: usize,
    pub running_session_count: usize,
    pub agents_used: Vec<&'static str>,
    pub custom_agent_executables: Vec<String>,
}

pub fn collect_domain_counts(repository: &DomainRepository<'_>) -> DomainCounts {
    let project_count = repository
        .list_recent_projects()
        .map(|projects| projects.len())
        .unwrap_or(0);
    // Three columns per row answer every count, so the registry is not hydrated.
    let sessions = repository.list_session_agent_facts().unwrap_or_default();
    let session_count = sessions.len();
    let running_session_count = sessions
        .iter()
        .filter(|session| session.lifecycle_state == "running")
        .count();
    let mut agents_used: BTreeSet<&'static str> = BTreeSet::new();
    let mut custom_agent_executables: BTreeSet<String> = BTreeSet::new();
    for session in &sessions {
        let Some(agent_id) = session.agent_id.as_deref() else {
            continue;
        };
        if agent_id.trim().is_empty() {
            continue;
        }
        // The two keys `session_agent_report` reads.
        let report = super::capture::session_agent_report(&serde_json::json!({
            "agentId": agent_id,
            "runtimeSettings": { "agentCommand": session.agent_command },
        }));
        agents_used.insert(report.agent);
        if let Some(executable) = report.executable {
            custom_agent_executables.insert(executable);
        }
    }
    DomainCounts {
        project_count,
        session_count,
        running_session_count,
        agents_used: agents_used.into_iter().collect(),
        custom_agent_executables: custom_agent_executables
            .into_iter()
            .take(taxonomy::MAX_TOKEN_LIST_LENGTH)
            .collect(),
    }
}

/// The interface setting plus the remote-machine COUNT, read straight out of
/// the shared settings file. Unknown or missing values normalize to the shipped
/// defaults, which is what the app itself renders in that situation.
pub struct SettingsSnapshot {
    pub preferred_interface: &'static str,
    pub remote_machine_count: usize,
    pub built_in_extensions_on: Vec<&'static str>,
}

pub fn collect_settings(paths: &GxserverPaths) -> SettingsSnapshot {
    let settings = crate::shared_settings_file::read_shared_settings(paths);
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
        built_in_extensions_on: super::extensions::built_in_extensions_on(settings.as_ref()),
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
