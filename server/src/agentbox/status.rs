//! `agentbox doctor --json` and `agentbox list -g --json`, read into the `/api/agentbox` shapes.

use std::{
    collections::BTreeMap,
    path::Path,
    sync::{Mutex, OnceLock},
    time::{Duration, Instant},
};

use serde_json::{json, Map, Value};

use super::cli::{resolve_agentbox_binary, run_agentbox, STATUS_TIMEOUT};
use super::location::{is_valid_alias, PROVIDERS, REMOTE_DOCKER_PREFIX};

/// How long a doctor run answers `status` before it runs again (unless `refresh`).
const STATUS_CACHE_TTL: Duration = Duration::from_secs(30);

fn status_cache() -> &'static Mutex<Option<(Value, Instant)>> {
    static CACHE: OnceLock<Mutex<Option<(Value, Instant)>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(None))
}

/// agentbox's own text, with its em dashes turned into the separators Ghostex copy uses.
fn plain_text(value: &str) -> String {
    value
        .replace(" \u{2014} ", ": ")
        .replace('\u{2014}', "-")
        .replace('`', "")
        .trim()
        .to_string()
}

struct DoctorRow {
    label: String,
    status: String,
    detail: Option<String>,
    hint: Option<String>,
}

impl DoctorRow {
    fn ok(&self) -> bool {
        self.status == "ok"
    }
}

fn doctor_groups(doctor: &Value) -> BTreeMap<String, Vec<DoctorRow>> {
    let mut groups = BTreeMap::new();
    for group in doctor
        .get("groups")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(title) = group.get("title").and_then(Value::as_str) else {
            continue;
        };
        let rows = group
            .get("results")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .map(|row| {
                let text = |key: &str| {
                    row.get(key)
                        .and_then(Value::as_str)
                        .map(plain_text)
                        .filter(|value| !value.is_empty())
                };
                DoctorRow {
                    label: text("label").unwrap_or_default(),
                    status: text("status").unwrap_or_default(),
                    detail: text("detail"),
                    hint: text("hint"),
                }
            })
            .collect::<Vec<_>>();
        groups.insert(title.to_string(), rows);
    }
    groups
}

/// The first row that keeps a provider from being ready, ignoring the optional Portless row.
fn blocking_row(rows: &[DoctorRow]) -> Option<&DoctorRow> {
    rows.iter()
        .find(|row| !row.ok() && row.label != "portless" && row.status != "info")
}

fn row<'a>(rows: &'a [DoctorRow], label: &str) -> Option<&'a DoctorRow> {
    rows.iter().find(|row| row.label == label)
}

fn provider_entry(
    id: &str,
    label: &str,
    description: &str,
    kind: &str,
    rows: &[DoctorRow],
    docker_ready: bool,
) -> Value {
    let (configured, prepared, ready, detail) = if id == "docker" {
        let configured = row(rows, "docker cli").is_some_and(DoctorRow::ok);
        let prepared = row(rows, "box image").is_none_or(DoctorRow::ok);
        let detail = row(rows, "docker daemon").and_then(|row| row.detail.clone());
        (configured, prepared, docker_ready, detail)
    } else {
        let configured = row(rows, "credentials").is_some_and(DoctorRow::ok);
        let prepared = rows
            .iter()
            .filter(|row| row.label.starts_with("base") || row.label.contains("snapshot"))
            .all(DoctorRow::ok);
        let detail = blocking_row(rows).and_then(|row| row.detail.clone());
        (configured, prepared, configured && prepared, detail)
    };
    let mut entry = Map::new();
    entry.insert("id".to_string(), json!(id));
    entry.insert("label".to_string(), json!(label));
    entry.insert("description".to_string(), json!(description));
    entry.insert("kind".to_string(), json!(kind));
    entry.insert("ready".to_string(), json!(ready));
    entry.insert("configured".to_string(), json!(configured));
    entry.insert("prepared".to_string(), json!(prepared));
    if let Some(detail) = detail {
        entry.insert("detail".to_string(), json!(detail));
    }
    if let Some(hint) = blocking_row(rows).and_then(|row| row.hint.clone()) {
        entry.insert("hint".to_string(), json!(hint));
    }
    Value::Object(entry)
}

fn read_json_file(path: &Path) -> Option<Value> {
    serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()
}

/// One provider entry per registered remote Docker alias, or a setup entry when there is none.
///
/// CDXC:AgentBox 2026-10-01 WHY: `agentbox remote-docker list` prints only a human table, so the aliases are read from the registry it keeps (`~/.agentbox/remote-docker-hosts.json`, with the baked-image records beside it). The run location of an alias is `agentbox:docker:<alias>`.
fn remote_docker_entries(home: &Path) -> Vec<Value> {
    let hosts = read_json_file(&home.join(".agentbox/remote-docker-hosts.json"));
    let prepared = read_json_file(&home.join(".agentbox/remote-docker-prepared.json"));
    let mut entries = Vec::new();
    for (alias, host) in hosts
        .as_ref()
        .and_then(|hosts| hosts.get("hosts"))
        .and_then(Value::as_object)
        .into_iter()
        .flatten()
    {
        if !is_valid_alias(alias) {
            continue;
        }
        let is_prepared = prepared
            .as_ref()
            .and_then(|prepared| prepared.pointer(&format!("/hosts/{alias}")))
            .is_some();
        let mut entry = json!({
            "id": format!("{REMOTE_DOCKER_PREFIX}{alias}"),
            "label": alias,
            "description": "Your server over SSH",
            "kind": "remoteDocker",
            "ready": true,
            "configured": true,
            "prepared": is_prepared,
        });
        if let Some(ssh) = host.get("ssh").and_then(Value::as_str) {
            entry["detail"] = json!(format!("ssh {ssh}"));
        }
        entries.push(entry);
    }
    if entries.is_empty() {
        entries.push(json!({
            "id": "remote-docker",
            "label": "Your own server (SSH)",
            "description": "Your server over SSH",
            "kind": "remoteDocker",
            "ready": false,
            "configured": false,
            "prepared": false,
            "hint": "agentbox remote-docker add <name> <user@host>",
        }));
    }
    entries
}

fn file_has_content(path: &Path) -> bool {
    std::fs::metadata(path).is_ok_and(|metadata| metadata.is_file() && metadata.len() > 0)
}

/// Whether Claude is signed in for boxes. Claude's box sign-in is agentbox's own (never the
/// computer's Keychain token, whose shared refresh token would sign the computer out).
pub(crate) fn claude_signed_in_for_boxes(home: &Path) -> bool {
    file_has_content(&home.join(".agentbox/claude-credentials.json"))
}

/// Drops the cached `status`, so the next read (the Cloud Boxes page) runs doctor again.
pub(crate) fn invalidate_status_cache() {
    if let Ok(mut cache) = status_cache().lock() {
        *cache = None;
    }
}

/// Which agents are signed in for boxes. Codex reuses the computer's `~/.codex/auth.json` on
/// Docker.
fn agent_sign_ins(home: &Path) -> Value {
    json!({
        "claude": claude_signed_in_for_boxes(home),
        "codex": file_has_content(&home.join(".agentbox/codex-credentials.json"))
            || file_has_content(&ghostex_paths::agent_homes::codex_default_home(home).join("auth.json")),
    })
}

fn status_value(home: &Path, refresh: bool) -> Value {
    let checked_at = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    let unsupported = cfg!(windows);
    let binary = (!unsupported)
        .then(|| resolve_agentbox_binary(home, refresh))
        .flatten();
    let mut error: Option<String> = None;
    let doctor = match binary.as_ref() {
        Some(_) => match run_agentbox(home, &["doctor", "--json"], STATUS_TIMEOUT, None) {
            Ok(output) => {
                let text = output.stdout.trim();
                let start = text.find('{').unwrap_or(0);
                match serde_json::from_str::<Value>(&text[start..]) {
                    Ok(doctor) => Some(doctor),
                    Err(_) => {
                        error = Some(if output.success {
                            "agentbox doctor did not print a report.".to_string()
                        } else {
                            output.failure_message()
                        });
                        None
                    }
                }
            }
            Err(message) => {
                error = Some(message);
                None
            }
        },
        None => None,
    };
    let groups = doctor.as_ref().map(doctor_groups).unwrap_or_default();
    let empty = Vec::new();
    let docker_rows = groups.get("docker").unwrap_or(&empty);
    let docker_ready = row(docker_rows, "docker daemon").is_some_and(DoctorRow::ok);
    let mut providers: Vec<Value> = PROVIDERS
        .iter()
        .map(|spec| {
            provider_entry(
                spec.id,
                spec.label,
                spec.description,
                spec.kind,
                groups.get(spec.id).unwrap_or(&empty),
                docker_ready,
            )
        })
        .collect();
    if binary.is_some() {
        providers.extend(remote_docker_entries(home));
    } else {
        for provider in &mut providers {
            provider["ready"] = json!(false);
        }
    }
    json!({
        "supported": !unsupported,
        "installed": binary.is_some(),
        "version": doctor.as_ref().and_then(|doctor| doctor.get("version")).cloned().unwrap_or(Value::Null),
        "binaryPath": binary,
        "dockerReady": docker_ready,
        "portlessInstalled": doctor
            .as_ref()
            .and_then(|doctor| doctor.pointer("/portless/installed"))
            .and_then(Value::as_bool)
            .unwrap_or(false),
        "agentSignIns": agent_sign_ins(home),
        "providers": providers,
        "checkedAt": checked_at,
        "error": error,
    })
}

/// `{"action":"status"}`. Blocking: call it from `spawn_blocking`.
pub(crate) fn read_status(home: &Path, refresh: bool) -> Value {
    if !super::location::cloud_boxes_enabled() {
        return json!({
            "supported": false,
            "installed": false,
            "turnedOff": true,
            "providers": [],
            "error": ghostex_settings_catalog::built_in_extensions::turned_off_message(
                ghostex_settings_catalog::built_in_extensions::CLOUD_BOXES,
            ),
        });
    }
    if !refresh {
        if let Ok(cache) = status_cache().lock() {
            if let Some((value, at)) = cache.as_ref() {
                if at.elapsed() < STATUS_CACHE_TTL {
                    return value.clone();
                }
            }
        }
    }
    let value = status_value(home, refresh);
    if let Ok(mut cache) = status_cache().lock() {
        *cache = Some((value.clone(), Instant::now()));
    }
    value
}

/// One box from `agentbox list -g --json`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ListedBox {
    pub(crate) name: String,
    pub(crate) agent: String,
    pub(crate) provider: String,
    pub(crate) state: String,
    pub(crate) web_url: Option<String>,
    pub(crate) vnc_url: Option<String>,
    pub(crate) project_root: Option<String>,
    /// agentbox's view of the agent: working, compacting, question, waiting, end-plan, error, idle.
    pub(crate) activity: Option<String>,
    pub(crate) session_title: Option<String>,
}

impl ListedBox {
    pub(crate) fn to_value(&self) -> Value {
        json!({
            "name": self.name,
            "agent": self.agent,
            "provider": self.provider,
            "state": self.state,
            "webUrl": self.web_url,
            "vncUrl": self.vnc_url,
            "projectRoot": self.project_root,
            "activity": self.activity,
            "sessionTitle": self.session_title,
        })
    }
}

pub(crate) fn parse_boxes(stdout: &str) -> Option<Vec<ListedBox>> {
    let text = stdout.trim();
    let start = text.find('[')?;
    let rows: Vec<Value> = serde_json::from_str(&text[start..]).ok()?;
    Some(
        rows.iter()
            .filter_map(|row| {
                let text = |key: &str| {
                    row.get(key)
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string)
                };
                let name = text("name")?;
                let agent = text("agent")
                    .or_else(|| text("lastAgent"))
                    .unwrap_or_default();
                let agent_status = row.pointer(&format!("/agentStatus/{agent}"));
                let status_text = |key: &str| {
                    agent_status
                        .and_then(|status| status.get(key))
                        .and_then(Value::as_str)
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                        .map(str::to_string)
                };
                Some(ListedBox {
                    activity: status_text("state").or_else(|| text(&format!("{agent}Activity"))),
                    session_title: status_text("sessionTitle")
                        .or_else(|| text(&format!("{agent}SessionTitle"))),
                    state: text("state")
                        .or_else(|| text("status"))
                        .unwrap_or_else(|| "unknown".to_string()),
                    provider: text("provider").unwrap_or_else(|| "docker".to_string()),
                    web_url: text("webUrl"),
                    vnc_url: text("vncUrl"),
                    project_root: text("projectRoot"),
                    agent,
                    name,
                })
            })
            .collect(),
    )
}

/// `agentbox list -g --json`. Blocking: call it from `spawn_blocking`.
pub(crate) fn list_boxes(home: &Path) -> Result<Vec<ListedBox>, String> {
    let output = run_agentbox(home, &["list", "-g", "--json"], STATUS_TIMEOUT, None)?;
    if !output.success {
        return Err(output.failure_message());
    }
    parse_boxes(&output.stdout).ok_or_else(|| "agentbox list did not print a box list.".to_string())
}
