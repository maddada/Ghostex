//! The provider rows of `providers`.

use ghostex_gx_protocol::cloud_box::{
    setting_keys, CloudConnectMethod, CloudProvider, CloudProviderConnectionState, CloudProviderId,
    CloudProviderLimits, LOCAL_PROVIDER_ENV,
};
use serde_json::Value;

use crate::server::AppState;

/// The `local` provider is listed with Show debug UI controls on, or with
/// `GHOSTEX_CLOUD_BOXES_LOCAL_PROVIDER=1` in gxserver's environment.
pub(super) fn local_provider_listed(state: &AppState) -> bool {
    std::env::var(LOCAL_PROVIDER_ENV).is_ok_and(|value| value == "1")
        || crate::shared_settings_file::read_shared_settings(&state.paths).and_then(|settings| {
            settings
                .get(setting_keys::DEBUG_UI)
                .and_then(Value::as_bool)
        }) == Some(true)
}

fn not_connected(id: CloudProviderId) -> CloudProvider {
    CloudProvider {
        label: id.label().to_string(),
        id,
        state: CloudProviderConnectionState::NotConnected,
        connect_methods: vec![CloudConnectMethod::Browser, CloudConnectMethod::Key],
        available: false,
        unavailable_reason: Some("Not connected.".to_string()),
        account_label: None,
        account_email: None,
        org_id: None,
        plan: None,
        limits: None,
        connected_at: None,
        error: None,
        debug_only: false,
    }
}

/// The `local` row: available when Docker runs and this Ghostex has a Linux package for it.
/// Blocking (it runs `docker version`).
pub(super) fn local_row(home: &std::path::Path) -> CloudProvider {
    let readiness = super::local_provider::docker_arch(home).and_then(|docker_arch| {
        let arch = ghostex_gx_cloud::local::package_arch(&docker_arch).ok_or_else(|| {
            format!("Docker runs {docker_arch}, which has no Linux gxserver package.")
        })?;
        super::local_provider::package_dir(arch).map(|_| ())
    });
    CloudProvider {
        label: CloudProviderId::Local.label().to_string(),
        id: CloudProviderId::Local,
        state: CloudProviderConnectionState::Connected,
        connect_methods: vec![CloudConnectMethod::NotNeeded],
        available: readiness.is_ok(),
        unavailable_reason: readiness.err(),
        account_label: Some("This computer".to_string()),
        account_email: None,
        org_id: None,
        plan: Some("free".to_string()),
        limits: Some(CloudProviderLimits::default()),
        connected_at: None,
        error: None,
        debug_only: true,
    }
}

/// Every provider row: the connected ones from the registry, the rest as not connected, and
/// `local` when listed. Blocking.
pub(super) fn provider_rows(state: &AppState) -> Vec<CloudProvider> {
    let mut rows: Vec<CloudProvider> = [CloudProviderId::Instabox, CloudProviderId::Boat]
        .into_iter()
        .map(|provider| {
            super::store::read_provider(&state.paths, &provider)
                .ok()
                .flatten()
                .map(|stored| stored.row)
                .unwrap_or_else(|| not_connected(provider))
        })
        .collect();
    if local_provider_listed(state) {
        rows.push(local_row(&state.paths.home_dir));
    }
    rows
}
