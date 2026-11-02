//! The `local` provider: a box is a Docker container on this computer running the Linux gxserver
//! package, so every stream can be proven without an account or money. This module builds the
//! `docker` command lines and reads their output; gxserver runs them.
//!
//! Mapping: create = `docker run -d` (labels carry [`BoxMetadata`]), stop = `docker stop`,
//! resume = `docker start` (programs end, like a Boat resume), delete = `docker rm -f`, snapshot
//! = `docker commit` to a `ghostex-cloudbox-snapshot:<snapshotId>` image, exec = `docker exec`,
//! file writes = `docker exec -i … sh -c 'cat > file'`. The box's gxserver listens on the
//! container's loopback; `ghostex box relay` forwards [`RELAY_PORT`] to it and Docker publishes
//! that port on this computer's loopback only, which `connectInfo` returns as a `direct` base URL.

use std::collections::BTreeMap;

use ghostex_gx_protocol::cloud_box::{CloudBoxState, BOX_PACKAGE_DIR};
use serde::Deserialize;

use crate::metadata::{self, BoxMetadata};

/// The base image of a blank local box (overridable with [`BASE_IMAGE_ENV`]). Debian rather than
/// Alpine so the agent CLIs W2 installs find glibc.
///
/// CDXC:CloudBoxes 2026-11-02 WHY: `debian:bookworm-slim` has no git, so every blank local box
/// failed at "Fetching the code and starting the agent" ("Can't run git"). The official
/// `buildpack-deps:bookworm-scm` is the same Debian with git, curl, ca-certificates and procps.
pub const BASE_IMAGE: &str = "buildpack-deps:bookworm-scm";
pub const BASE_IMAGE_ENV: &str = "GHOSTEX_CLOUD_BOXES_LOCAL_IMAGE";
/// Override for the Linux gxserver package folder copied into each box.
pub const PACKAGE_DIR_ENV: &str = "GHOSTEX_CLOUD_BOXES_LOCAL_PACKAGE";
/// The repository of snapshot images.
pub const SNAPSHOT_REPOSITORY: &str = "ghostex-cloudbox-snapshot";
/// Image labels of a snapshot (besides the box labels the commit inherits).
pub const SNAPSHOT_ID_LABEL: &str = "ghostex.snapshotId";
pub const SNAPSHOT_NAME_LABEL: &str = "ghostex.snapshotName";
pub const SNAPSHOT_CREATED_AT_LABEL: &str = "ghostex.snapshotCreatedAt";
pub const SNAPSHOT_SOURCE_BOX_LABEL: &str = "ghostex.snapshotSourceBox";
/// The in-box port `ghostex box relay` listens on (forwarding to gxserver's loopback port).
pub const RELAY_PORT: u16 = 58790;

/// Every metadata key, so a container made from a snapshot image never inherits the source box's
/// values (an empty label reads as absent).
const ALL_METADATA_KEYS: [&str; 11] = [
    metadata::MARKER_KEY,
    metadata::BOX_ID_KEY,
    metadata::PROJECT_KEY_KEY,
    metadata::HOME_PROJECT_KEY,
    metadata::HOME_MACHINE_KEY,
    metadata::SNAPSHOT_KEY,
    metadata::SIZE_KEY,
    metadata::IDEMPOTENCY_KEY,
    metadata::CREATED_BY_KEY,
    metadata::CREATED_AT_KEY,
    SNAPSHOT_ID_LABEL,
];

pub fn container_name(box_id: &str) -> String {
    format!("ghostex-box-{box_id}")
}

pub fn snapshot_image(snapshot_id: &str) -> String {
    format!("{SNAPSHOT_REPOSITORY}:{snapshot_id}")
}

fn args(items: &[&str]) -> Vec<String> {
    items.iter().map(|item| item.to_string()).collect()
}

/// `docker version`: the server's architecture (`arm64`, `amd64`), which picks the package.
pub fn server_arch_args() -> Vec<String> {
    args(&["version", "--format", "{{.Server.Arch}}"])
}

/// The Linux package arch name for Docker's server arch.
pub fn package_arch(docker_arch: &str) -> Option<&'static str> {
    match docker_arch.trim() {
        "arm64" | "aarch64" => Some("arm64"),
        "amd64" | "x86_64" => Some("x64"),
        _ => None,
    }
}

/// `docker run -d …`: the box container, idling until boot.
pub fn create_args(meta: &BoxMetadata, image: &str, vcpus: u64, mem_mib: u64) -> Vec<String> {
    let mut out = args(&["run", "-d", "--init"]);
    out.push("--name".into());
    out.push(container_name(&meta.box_id));
    out.push("--hostname".into());
    out.push(crate::ids::box_display_name(&meta.box_id));
    let written: BTreeMap<&str, String> = meta.to_pairs().into_iter().collect();
    for key in ALL_METADATA_KEYS {
        out.push("--label".into());
        out.push(format!(
            "{key}={}",
            written.get(key).cloned().unwrap_or_default()
        ));
    }
    out.push("--cpus".into());
    out.push(vcpus.to_string());
    out.push("--memory".into());
    out.push(format!("{mem_mib}m"));
    // The private folder for logins and secrets: tmpfs, never in a commit.
    out.push("--tmpfs".into());
    out.push("/run/ghostex:rw,mode=0700".into());
    out.push("--publish".into());
    out.push(format!("127.0.0.1::{RELAY_PORT}"));
    out.push(image.to_string());
    out.extend(args(&["sleep", "infinity"]));
    out
}

/// `docker cp <package>/. <container>:/opt/ghostex`: installs the Linux gxserver package.
pub fn install_package_args(box_id: &str, package_dir: &str) -> Vec<String> {
    vec![
        "cp".into(),
        format!("{}/.", package_dir.trim_end_matches('/')),
        format!("{}:{BOX_PACKAGE_DIR}", container_name(box_id)),
    ]
}

/// `docker exec <container> <argv…>`.
pub fn exec_args(box_id: &str, argv: &[&str]) -> Vec<String> {
    let mut out = vec!["exec".to_string(), container_name(box_id)];
    out.extend(argv.iter().map(|arg| arg.to_string()));
    out
}

/// `docker exec -i <container> sh -c 'umask 077; mkdir -p <dir> && cat > <path>'`: write stdin to
/// a file (the provider file API of `local`). `path` must be absolute and shell-safe.
pub fn write_file_args(box_id: &str, path: &str) -> Option<Vec<String>> {
    if !path.starts_with('/')
        || !path
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b"/._-".contains(&b))
    {
        return None;
    }
    let dir = path
        .rsplit_once('/')
        .map(|(dir, _)| dir)
        .filter(|dir| !dir.is_empty())
        .unwrap_or("/");
    Some(vec![
        "exec".into(),
        "-i".into(),
        container_name(box_id),
        "sh".into(),
        "-c".into(),
        format!("umask 077; mkdir -p {dir} && cat > {path}"),
    ])
}

pub fn start_args(box_id: &str) -> Vec<String> {
    vec!["start".into(), container_name(box_id)]
}

pub fn stop_args(box_id: &str) -> Vec<String> {
    vec![
        "stop".into(),
        "--time".into(),
        "10".into(),
        container_name(box_id),
    ]
}

pub fn delete_args(box_id: &str) -> Vec<String> {
    vec!["rm".into(), "-f".into(), container_name(box_id)]
}

/// `docker ps -a -q --filter label=ghostex=1`: ids of every Ghostex box container.
pub fn list_ids_args() -> Vec<String> {
    args(&[
        "ps",
        "-a",
        "-q",
        "--no-trunc",
        "--filter",
        "label=ghostex=1",
    ])
}

/// `docker inspect <ids…>` (JSON array).
pub fn inspect_args(ids: &[String]) -> Vec<String> {
    let mut out = vec!["inspect".to_string()];
    out.extend(ids.iter().cloned());
    out
}

/// `docker commit` of a box into a snapshot image.
pub fn commit_args(box_id: &str, snapshot_id: &str, name: &str, created_at: &str) -> Vec<String> {
    let label = |key: &str, value: &str| format!("LABEL {key}=\"{}\"", value.replace('"', "'"));
    vec![
        "commit".into(),
        "--change".into(),
        label(SNAPSHOT_ID_LABEL, snapshot_id),
        "--change".into(),
        label(SNAPSHOT_NAME_LABEL, name),
        "--change".into(),
        label(SNAPSHOT_CREATED_AT_LABEL, created_at),
        "--change".into(),
        label(SNAPSHOT_SOURCE_BOX_LABEL, box_id),
        // The image runs nothing by itself; `create_args` always passes the command.
        "--change".into(),
        "CMD [\"sleep\", \"infinity\"]".into(),
        container_name(box_id),
        snapshot_image(snapshot_id),
    ]
}

/// `docker images -q --filter label=ghostex.snapshotId`: every snapshot image id.
pub fn list_snapshot_images_args() -> Vec<String> {
    vec![
        "images".into(),
        "-q".into(),
        "--no-trunc".into(),
        "--filter".into(),
        format!("label={SNAPSHOT_ID_LABEL}"),
    ]
}

pub fn delete_snapshot_args(snapshot_id: &str) -> Vec<String> {
    vec!["rmi".into(), snapshot_image(snapshot_id)]
}

/// One container of `docker inspect`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalContainer {
    pub container_id: String,
    pub metadata: BoxMetadata,
    /// Docker's state word: `created`, `running`, `paused`, `restarting`, `exited`, `dead`.
    pub docker_state: String,
    pub started_at: Option<String>,
    pub finished_at: Option<String>,
    pub created: Option<String>,
    /// The loopback port Docker published for [`RELAY_PORT`], while running.
    pub relay_host_port: Option<u16>,
    pub image: String,
}

impl LocalContainer {
    /// The registry state the container's Docker state implies. A running container whose boot
    /// has not finished is the caller's to mark `booting`.
    pub fn box_state(&self) -> CloudBoxState {
        match self.docker_state.as_str() {
            "running" => CloudBoxState::Running,
            "paused" => CloudBoxState::Paused,
            "created" => CloudBoxState::Creating,
            "restarting" => CloudBoxState::Booting,
            "removing" => CloudBoxState::Deleting,
            "dead" => CloudBoxState::Failed,
            _ => CloudBoxState::Stopped,
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct InspectRow {
    id: String,
    #[serde(default)]
    created: Option<String>,
    #[serde(default)]
    state: InspectState,
    #[serde(default)]
    config: InspectConfig,
    #[serde(default)]
    network_settings: InspectNetwork,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct InspectState {
    #[serde(default)]
    status: String,
    #[serde(default)]
    started_at: Option<String>,
    #[serde(default)]
    finished_at: Option<String>,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct InspectConfig {
    #[serde(default)]
    labels: Option<BTreeMap<String, String>>,
    #[serde(default)]
    image: String,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct InspectNetwork {
    #[serde(default)]
    ports: Option<BTreeMap<String, Option<Vec<InspectBinding>>>>,
}

#[derive(Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct InspectBinding {
    #[serde(default)]
    host_ip: String,
    #[serde(default)]
    host_port: String,
}

/// Docker reports a never-set time as `0001-01-01T00:00:00Z`.
fn real_time(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.is_empty() && !value.starts_with("0001-"))
}

/// Reads `docker inspect` output; containers without Ghostex labels are skipped.
pub fn parse_inspect(json: &str) -> Result<Vec<LocalContainer>, String> {
    let rows: Vec<InspectRow> = serde_json::from_str(json.trim())
        .map_err(|error| format!("docker inspect printed something unexpected: {error}"))?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let labels = row.config.labels.unwrap_or_default();
            let metadata = BoxMetadata::from_map(&labels)?;
            let relay_host_port = row
                .network_settings
                .ports
                .and_then(|ports| ports.get(&format!("{RELAY_PORT}/tcp")).cloned().flatten())
                .and_then(|bindings| {
                    bindings.into_iter().find(|binding| {
                        binding.host_ip == "127.0.0.1" || binding.host_ip.is_empty()
                    })
                })
                .and_then(|binding| binding.host_port.parse().ok());
            Some(LocalContainer {
                container_id: row.id,
                metadata,
                docker_state: row.state.status,
                started_at: real_time(row.state.started_at),
                finished_at: real_time(row.state.finished_at),
                created: real_time(row.created),
                relay_host_port,
                image: row.config.image,
            })
        })
        .collect())
}

/// One snapshot image of `docker image inspect`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalSnapshotImage {
    pub image_id: String,
    pub snapshot_id: String,
    pub name: String,
    pub project_key: String,
    pub source_box_id: Option<String>,
    pub created_at: Option<String>,
    pub size_bytes: Option<u64>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct ImageRow {
    id: String,
    #[serde(default)]
    size: Option<u64>,
    #[serde(default)]
    config: InspectConfig,
}

/// Reads `docker image inspect` output of snapshot images.
pub fn parse_snapshot_images(json: &str) -> Result<Vec<LocalSnapshotImage>, String> {
    let rows: Vec<ImageRow> = serde_json::from_str(json.trim())
        .map_err(|error| format!("docker image inspect printed something unexpected: {error}"))?;
    Ok(rows
        .into_iter()
        .filter_map(|row| {
            let labels = row.config.labels.unwrap_or_default();
            let text = |key: &str| labels.get(key).filter(|value| !value.is_empty()).cloned();
            Some(LocalSnapshotImage {
                image_id: row.id,
                snapshot_id: text(SNAPSHOT_ID_LABEL)?,
                name: text(SNAPSHOT_NAME_LABEL).unwrap_or_else(|| "Snapshot".into()),
                project_key: text(metadata::PROJECT_KEY_KEY).unwrap_or_default(),
                source_box_id: text(SNAPSHOT_SOURCE_BOX_LABEL),
                created_at: text(SNAPSHOT_CREATED_AT_LABEL),
                size_bytes: row.size,
            })
        })
        .collect())
}
