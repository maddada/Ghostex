//! One set of box operations over every provider: Instabox and Boat through their HTTP APIs
//! (`ghostex_gx_cloud`), `local` through Docker. Every method blocks; call from `spawn_blocking`.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use ghostex_gx_cloud::http::ProviderError;
use ghostex_gx_cloud::metadata::BoxMetadata;
use ghostex_gx_cloud::provider::{ExecOutput, ProviderMachine, ProviderSnapshot};
use ghostex_gx_cloud::{boat, instabox, local};
use ghostex_gx_protocol::cloud_box::{CloudBoxState, CloudProviderId};

use super::http::call;
use super::local_provider;
use crate::paths::GxserverPaths;

/// How long a create or resume may take before Ghostex gives up on it.
const MACHINE_WAIT: Duration = Duration::from_secs(600);

/// Which box an operation acts on.
#[derive(Clone, Debug)]
pub(super) struct Target {
    pub(super) box_id: String,
    pub(super) provider_box_id: String,
}

/// A provider with its key, ready to call.
#[derive(Clone)]
pub(super) enum Ops {
    Local { home: PathBuf },
    Instabox { key: String },
    Boat { key: String },
}

fn err(error: ProviderError) -> String {
    error.message
}

fn idempotency(box_id: &str, action: &str) -> String {
    let mut entropy = [0u8; 8];
    rand::Rng::fill_bytes(&mut rand::rng(), &mut entropy);
    let suffix: String = entropy.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("ghostex-{box_id}-{action}-{suffix}")
}

impl Ops {
    /// The provider's operations with its saved key (`local` needs none).
    pub(super) fn for_provider(
        paths: &GxserverPaths,
        provider: &CloudProviderId,
    ) -> Result<Ops, crate::domain::DomainStateError> {
        match provider {
            CloudProviderId::Local => Ok(Ops::Local {
                home: paths.home_dir.clone(),
            }),
            CloudProviderId::Instabox => Ok(Ops::Instabox {
                key: super::connect::provider_key(paths, provider)?,
            }),
            CloudProviderId::Boat => Ok(Ops::Boat {
                key: super::connect::provider_key(paths, provider)?,
            }),
            CloudProviderId::Other(other) => Err(crate::domain::DomainStateError::bad_request(
                format!("\"{other}\" is not a Cloud Box provider."),
            )),
        }
    }

    pub(super) fn provider(&self) -> CloudProviderId {
        match self {
            Ops::Local { .. } => CloudProviderId::Local,
            Ops::Instabox { .. } => CloudProviderId::Instabox,
            Ops::Boat { .. } => CloudProviderId::Boat,
        }
    }

    /// Every Ghostex box at the provider, and (Boat) this month's dollars of deleted ones.
    pub(super) fn list(&self) -> Result<(Vec<ProviderMachine>, f64), String> {
        match self {
            Ops::Local { home } => Ok((
                local_provider::list_containers(home)?
                    .iter()
                    .map(local_machine)
                    .collect(),
                0.0,
            )),
            Ops::Instabox { key } => {
                let mut all = Vec::new();
                let mut cursor: Option<String> = None;
                loop {
                    let (page, next) = call(
                        instabox::list_machines_request(key, cursor.as_deref()),
                        instabox::parse_machine_list,
                    )
                    .map_err(err)?;
                    all.extend(page);
                    match next {
                        Some(next) if all.len() < 2000 => cursor = Some(next),
                        _ => return Ok((all, 0.0)),
                    }
                }
            }
            Ops::Boat { key } => {
                let mut all = Vec::new();
                let mut deleted = 0.0;
                let mut cursor: Option<String> = None;
                loop {
                    let (page, next, deleted_page) = call(
                        boat::list_sandboxes_request(key, cursor.as_deref()),
                        boat::parse_sandbox_list,
                    )
                    .map_err(err)?;
                    all.extend(page);
                    deleted += deleted_page;
                    match next {
                        Some(next) if all.len() < 2000 => cursor = Some(next),
                        _ => return Ok((all, deleted)),
                    }
                }
            }
        }
    }

    /// One box as the provider sees it now; `None` when it is gone.
    pub(super) fn get(&self, target: &Target) -> Result<Option<ProviderMachine>, String> {
        let gone = |error: &ProviderError| error.status == 404;
        match self {
            Ops::Local { home } => Ok(local_provider::find_container(home, &target.box_id)?
                .as_ref()
                .map(local_machine)),
            Ops::Instabox { key } => match call(
                instabox::get_machine_request(key, &target.provider_box_id),
                instabox::parse_machine,
            ) {
                Ok(machine) => Ok(Some(machine)),
                Err(error) if gone(&error) => Ok(None),
                Err(error) => Err(err(error)),
            },
            Ops::Boat { key } => match call(
                boat::get_sandbox_request(key, &target.provider_box_id),
                boat::parse_sandbox,
            ) {
                Ok(machine) => Ok(Some(machine)),
                Err(error) if gone(&error) => Ok(None),
                Err(error) => Err(err(error)),
            },
        }
    }

    /// Makes the machine and waits until it runs. Returns its provider id. `idempotency_key` makes a
    /// retried create return the same machine.
    pub(super) fn create(
        &self,
        meta: &BoxMetadata,
        provider_snapshot_id: Option<&str>,
        size_id: &str,
        vcpus: u64,
        mem_mib: u64,
        idle_minutes: u64,
        idempotency_key: &str,
        package: Option<&Path>,
        base_image: &str,
    ) -> Result<String, String> {
        match self {
            Ops::Local { home } => {
                let image = provider_snapshot_id
                    .map(str::to_string)
                    .unwrap_or_else(|| base_image.to_string());
                let id = local_provider::create_container(home, meta, &image, vcpus, mem_mib)?;
                if let Some(package) = package {
                    local_provider::install_package(home, &meta.box_id, package)?;
                }
                Ok(id)
            }
            Ops::Instabox { key } => {
                let operation = call(
                    instabox::create_machine_request(
                        key,
                        idempotency_key,
                        &instabox::CreateSpec {
                            metadata: meta,
                            provider_snapshot_id,
                            vcpus,
                            mem_mib,
                            idle_minutes,
                        },
                    ),
                    instabox::parse_operation,
                )
                .map_err(err)?;
                let machine_id = operation
                    .machine_id
                    .clone()
                    .ok_or("Instabox didn't say which machine it made.")?;
                self.wait_operation(key, operation)?;
                let target = Target {
                    box_id: meta.box_id.clone(),
                    provider_box_id: machine_id.clone(),
                };
                // A create from a snapshot with storage stays stopped; start it.
                if self
                    .get(&target)?
                    .is_some_and(|m| m.state == CloudBoxState::Stopped)
                {
                    self.resume(&target, &CloudBoxState::Stopped)?;
                }
                Ok(machine_id)
            }
            Ops::Boat { key } => {
                let created = call(
                    boat::create_sandbox_request(
                        key,
                        idempotency_key,
                        meta,
                        provider_snapshot_id,
                        size_id,
                    ),
                    boat::parse_sandbox,
                )
                .map_err(err)?;
                let target = Target {
                    box_id: meta.box_id.clone(),
                    provider_box_id: created.provider_box_id.clone(),
                };
                self.wait_running(&target)?;
                Ok(created.provider_box_id)
            }
        }
    }

    fn wait_operation(&self, key: &str, mut operation: instabox::Operation) -> Result<(), String> {
        let started = Instant::now();
        while !operation.is_done() {
            if started.elapsed() > MACHINE_WAIT {
                return Err(format!(
                    "Instabox is still {} after {} minutes.",
                    operation.phase.as_deref().unwrap_or("working"),
                    MACHINE_WAIT.as_secs() / 60
                ));
            }
            operation = call(
                instabox::operation_request(key, &operation.id),
                instabox::parse_operation,
            )
            .map_err(err)?;
        }
        match operation.state.as_str() {
            "succeeded" => Ok(()),
            _ => Err(operation
                .error
                .unwrap_or_else(|| "The Instabox operation failed.".to_string())),
        }
    }

    /// Polls until the box runs (Boat and `local`, whose calls answer before it is up).
    fn wait_running(&self, target: &Target) -> Result<(), String> {
        let started = Instant::now();
        loop {
            match self.get(target)? {
                Some(machine) if machine.state == CloudBoxState::Running => return Ok(()),
                Some(machine) if machine.state == CloudBoxState::Failed => {
                    return Err(machine
                        .error
                        .unwrap_or_else(|| "The box failed to start.".to_string()))
                }
                None => return Err("The provider no longer has this box.".to_string()),
                _ if started.elapsed() > MACHINE_WAIT => {
                    return Err(format!(
                        "The box did not start within {} minutes.",
                        MACHINE_WAIT.as_secs() / 60
                    ))
                }
                _ => std::thread::sleep(Duration::from_secs(2)),
            }
        }
    }

    /// Wakes a paused or stopped box and waits until it runs.
    pub(super) fn resume(&self, target: &Target, state: &CloudBoxState) -> Result<(), String> {
        match self {
            Ops::Local { home } => {
                local_provider::start(home, &target.box_id)?;
                self.wait_running(target)
            }
            Ops::Instabox { key } => {
                let action = if *state == CloudBoxState::Paused {
                    "resume"
                } else {
                    "start"
                };
                let operation = call(
                    instabox::machine_action_request(
                        key,
                        &target.provider_box_id,
                        action,
                        &idempotency(&target.box_id, action),
                    ),
                    instabox::parse_operation,
                )
                .map_err(err)?;
                self.wait_operation(key, operation)
            }
            Ops::Boat { key } => {
                call(
                    boat::sandbox_action_request(key, &target.provider_box_id, "resume"),
                    |response| ghostex_gx_cloud::http::json_or_error(response, "Boat"),
                )
                .map_err(err)?;
                self.wait_running(target)
            }
        }
    }

    /// Stops the box (programs end, the disk is kept; Boat archives it).
    pub(super) fn stop(&self, target: &Target) -> Result<(), String> {
        match self {
            Ops::Local { home } => local_provider::stop(home, &target.box_id),
            Ops::Instabox { key } => {
                let operation = call(
                    instabox::machine_action_request(
                        key,
                        &target.provider_box_id,
                        "stop",
                        &idempotency(&target.box_id, "stop"),
                    ),
                    instabox::parse_operation,
                )
                .map_err(err)?;
                self.wait_operation(key, operation)
            }
            Ops::Boat { key } => call(
                boat::sandbox_action_request(key, &target.provider_box_id, "stop"),
                |response| ghostex_gx_cloud::http::json_or_error(response, "Boat"),
            )
            .map(|_| ())
            .map_err(err),
        }
    }

    pub(super) fn delete(&self, target: &Target) -> Result<(), String> {
        let gone_ok = |result: Result<serde_json::Value, ProviderError>| match result {
            Ok(_) => Ok(()),
            Err(error) if error.status == 404 => Ok(()),
            Err(error) => Err(err(error)),
        };
        match self {
            Ops::Local { home } => local_provider::delete(home, &target.box_id),
            Ops::Instabox { key } => gone_ok(call(
                instabox::delete_machine_request(
                    key,
                    &target.provider_box_id,
                    &idempotency(&target.box_id, "delete"),
                ),
                |response| ghostex_gx_cloud::http::json_or_error(response, "Instabox"),
            )),
            Ops::Boat { key } => gone_ok(call(
                boat::delete_sandbox_request(key, &target.provider_box_id),
                |response| ghostex_gx_cloud::http::json_or_error(response, "Boat"),
            )),
        }
    }

    /// Runs argv on the box (no shell on Instabox and `local`; Boat quotes it for its shell).
    pub(super) fn exec(
        &self,
        target: &Target,
        argv: &[&str],
        stdin: Option<&str>,
        timeout_seconds: u64,
    ) -> Result<ExecOutput, String> {
        match self {
            Ops::Local { home } => local_provider::exec(home, &target.box_id, argv, stdin),
            Ops::Instabox { key } => call(
                instabox::exec_request(key, &target.provider_box_id, argv, stdin, timeout_seconds),
                instabox::parse_exec,
            )
            .map_err(err),
            Ops::Boat { key } => call(
                boat::command_request(key, &target.provider_box_id, argv, stdin, timeout_seconds),
                boat::parse_command,
            )
            .map_err(err),
        }
    }

    /// Writes a private file (mode 0600) on the box through the provider's file API.
    pub(super) fn write_file(
        &self,
        target: &Target,
        path: &str,
        bytes: &[u8],
    ) -> Result<(), String> {
        match self {
            Ops::Local { home } => local_provider::write_file(home, &target.box_id, path, bytes),
            Ops::Instabox { key } => call(
                instabox::write_file_request(key, &target.provider_box_id, path, bytes.to_vec()),
                |response| ghostex_gx_cloud::http::json_or_error(response, "Instabox"),
            )
            .map(|_| ())
            .map_err(err),
            Ops::Boat { key } => call(
                boat::write_file_request(key, &target.provider_box_id, path, bytes),
                |response| ghostex_gx_cloud::http::json_or_error(response, "Boat"),
            )
            .map(|_| ())
            .map_err(err),
        }
    }

    /// Starts a snapshot of the box (`pending` until it is saved).
    pub(super) fn create_snapshot(
        &self,
        target: &Target,
        snapshot_id: &str,
        name: &str,
        created_at: &str,
    ) -> Result<ProviderSnapshot, String> {
        match self {
            Ops::Local { home } => {
                local_provider::commit(home, &target.box_id, snapshot_id, name, created_at)?;
                Ok(ProviderSnapshot {
                    provider_snapshot_id: local::snapshot_image(snapshot_id),
                    name: name.to_string(),
                    status: "ready".to_string(),
                    source_box_id: Some(target.box_id.clone()),
                    size_bytes: None,
                    created_at: Some(created_at.to_string()),
                    error: None,
                })
            }
            Ops::Instabox { key } => call(
                instabox::create_snapshot_request(
                    key,
                    &target.provider_box_id,
                    &instabox::machine_name(snapshot_id),
                    &idempotency(snapshot_id, "snapshot"),
                ),
                instabox::parse_snapshot,
            )
            .map_err(err),
            Ops::Boat { key } => call(
                boat::save_snapshot_request(
                    key,
                    &target.provider_box_id,
                    &boat::snapshot_name(snapshot_id),
                ),
                boat::parse_snapshot,
            )
            .map_err(err),
        }
    }

    /// A snapshot's state now; `None` when it is gone.
    pub(super) fn get_snapshot(
        &self,
        provider_snapshot_id: &str,
    ) -> Result<Option<ProviderSnapshot>, String> {
        let found = match self {
            Ops::Local { home } => {
                return local_provider::snapshot_exists(home, provider_snapshot_id)
            }
            Ops::Instabox { key } => call(
                instabox::get_snapshot_request(key, provider_snapshot_id),
                instabox::parse_snapshot,
            ),
            Ops::Boat { key } => call(
                boat::get_snapshot_request(key, provider_snapshot_id),
                boat::parse_snapshot,
            ),
        };
        match found {
            Ok(snapshot) => Ok(Some(snapshot)),
            Err(error) if error.status == 404 => Ok(None),
            Err(error) => Err(err(error)),
        }
    }

    pub(super) fn delete_snapshot(&self, provider_snapshot_id: &str) -> Result<(), String> {
        let result = match self {
            Ops::Local { home } => {
                return local_provider::delete_snapshot_image(home, provider_snapshot_id)
            }
            Ops::Instabox { key } => call(
                instabox::delete_snapshot_request(
                    key,
                    provider_snapshot_id,
                    &idempotency(provider_snapshot_id, "delete"),
                ),
                |response| ghostex_gx_cloud::http::json_or_error(response, "Instabox"),
            ),
            Ops::Boat { key } => call(
                boat::delete_snapshot_request(key, provider_snapshot_id),
                |response| ghostex_gx_cloud::http::json_or_error(response, "Boat"),
            ),
        };
        match result {
            Ok(_) => Ok(()),
            Err(error) if error.status == 404 => Ok(()),
            Err(error) => Err(err(error)),
        }
    }
}

/// A `local` container as a provider machine.
fn local_machine(container: &local::LocalContainer) -> ProviderMachine {
    let state = container.box_state();
    ProviderMachine {
        provider_box_id: container.container_id.chars().take(12).collect(),
        stopped_at: (state == CloudBoxState::Stopped)
            .then(|| container.finished_at.clone())
            .flatten(),
        started_at: (state == CloudBoxState::Running)
            .then(|| container.started_at.clone())
            .flatten(),
        state,
        metadata: Some(container.metadata.clone()),
        created_at: container.created.clone(),
        cost_month_usd: Some(0.0),
        size: container.metadata.size.clone(),
        error: None,
        operation_id: None,
    }
}
