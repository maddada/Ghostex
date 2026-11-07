//! `cloudKit`: what a paired phone keeps so it can start a box from a project's Default snapshot
//! while this computer is off (plan §2.3; the wire type and the user's decision are in
//! `ghostex_gx_protocol::cloud_box::kit`).
//!
//! CDXC:CloudBoxes 2026-11-02 WHY:
//! The kit carries the provider keys this computer holds (not keys minted for the phone: Boat has
//! no API to mint one, and Instabox mints only from a fresh browser sign-in), the logins allowed on
//! boxes and each project's ticked secret values, so it is handed out only while Settings › Cloud
//! Boxes › "Start boxes from your phone" is on, and only over the authenticated gxserver route a
//! paired phone reaches through its own encrypted link. Turning the switch off stops new kits; the
//! phone drops its copy when its next read is refused.

use ghostex_gx_protocol::cloud_box::{
    error_codes, setting_keys, CloudKit, CloudKitProject, CloudKitProvider, CloudKitSnapshot,
    CloudKitSpend, CloudProviderConnectionState, CloudProviderId, CloudSnapshotStatus,
    ProjectCloudBoxSettings, CLOUD_KIT_VERSION,
};
use serde_json::Value;

use super::{boot_steps, project, snapshots, store};
use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;

fn kit_off() -> DomainStateError {
    DomainStateError {
        code: error_codes::KIT_OFF,
        message:
            "Turn on Settings › Cloud Boxes › Start boxes from your phone on the computer first."
                .to_string(),
    }
}

fn phone_start_enabled(state: &AppState) -> bool {
    crate::shared_settings_file::read_shared_settings(&state.paths)
        .and_then(|settings| {
            settings
                .get(setting_keys::PHONE_START_ENABLED)
                .and_then(Value::as_bool)
        })
        .unwrap_or(false)
}

/// The connected providers (not `local`, which only this computer's Docker runs) with their keys.
fn providers(state: &AppState) -> Vec<CloudKitProvider> {
    [CloudProviderId::Instabox, CloudProviderId::Boat]
        .into_iter()
        .filter_map(|provider| {
            let stored = store::read_provider(&state.paths, &provider).ok()??;
            if stored.row.state != CloudProviderConnectionState::Connected {
                return None;
            }
            let key = store::unseal(&state.paths, stored.key_sealed.as_deref()?).ok()?;
            Some(CloudKitProvider {
                provider,
                key,
                plan: stored.row.plan.clone(),
            })
        })
        .collect()
}

/// Every project with a ready Default snapshot on a provider the kit carries.
fn projects(state: &AppState, providers: &[CloudKitProvider]) -> Vec<CloudKitProject> {
    let Ok(db) = crate::storage::open_gxserver_database(&state.paths) else {
        return Vec::new();
    };
    let repository = DomainRepository::new(&db, state.metadata.server_id.as_str());
    let project_ids: Vec<String> = repository
        .list_projects()
        .unwrap_or_default()
        .iter()
        .filter_map(|project| project.get("projectId").and_then(Value::as_str))
        .map(str::to_string)
        .collect();
    let all_snapshots = snapshots::list(state, None, None).unwrap_or_default();
    project_ids
        .into_iter()
        .filter_map(|project_id| {
            let home = project::read_home_project(state, &project_id).ok()?;
            let default = all_snapshots.iter().find(|snapshot| {
                snapshot.project_key == home.project_key
                    && snapshot.is_default
                    && snapshot.status == CloudSnapshotStatus::Ready
                    && providers
                        .iter()
                        .any(|provider| provider.provider == snapshot.provider)
            })?;
            let secrets = boot_steps::secrets_file(state, home.path.as_deref(), &home.settings);
            let base_branch = home
                .remembered_base_branch()
                .or_else(|| default.base_branch.clone());
            Some(CloudKitProject {
                home_project_id: project_id,
                project_key: home.project_key.clone(),
                name: home.name(),
                repo_url: home.origin_url(),
                base_branch,
                snapshot: CloudKitSnapshot {
                    snapshot_id: default.snapshot_id.clone(),
                    provider: default.provider.clone(),
                    provider_snapshot_id: default.provider_snapshot_id.clone()?,
                    size: default.size.clone().or(home.settings.size.clone()),
                    name: default.name.clone(),
                },
                files: secrets.files,
                vars: secrets.vars,
            })
        })
        .collect()
}

pub(super) fn kit(state: &AppState) -> Result<CloudKit, DomainStateError> {
    if !phone_start_enabled(state) {
        return Err(kit_off());
    }
    let providers = providers(state);
    let projects = projects(state, &providers);
    // The logins are the same for every project; `secrets_file` without a project reads only them.
    let accounts =
        boot_steps::secrets_file(state, None, &ProjectCloudBoxSettings::default()).accounts;
    Ok(CloudKit {
        version: CLOUD_KIT_VERSION,
        home_machine_id: state.metadata.server_id.clone(),
        gxserver_version: crate::constants::GXSERVER_VERSION.to_string(),
        issued_at: super::time::now_iso(),
        providers,
        accounts,
        allow_device_keys: boot_steps::allowed_device_keys(state),
        idle_stop_minutes: boot_steps::idle_stop_minutes(state),
        projects,
        spend: {
            let spend = super::spend::read_spend(state);
            CloudKitSpend {
                month: spend.month,
                total_usd: spend.total_usd,
                cap_usd: spend.cap_usd,
            }
        },
    })
}
