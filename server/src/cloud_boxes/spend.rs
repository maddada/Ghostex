//! This month's spend and the monthly cap. The scheduler reads the providers every few minutes
//! (`refresh`); `spend` and the cap check answer from the last read.

use ghostex_gx_cloud::provider::ProviderMachine;
use ghostex_gx_cloud::{boat, instabox};
use ghostex_gx_protocol::cloud_box::{
    error_codes, setting_keys, CloudBoxState, CloudProviderId, CloudSpend, CloudSpendLine,
    CloudSpendScope, CloudSpendState,
};
use serde_json::Value;

use super::http::call;
use super::ops::Ops;
use super::{events, registry, store};
use crate::domain::DomainStateError;
use crate::server::AppState;

/// The cap in dollars from Settings; `None` when off (absent, `null`, or not a positive number).
pub(super) fn monthly_cap_usd(state: &AppState) -> Option<f64> {
    crate::shared_settings_file::read_shared_settings(&state.paths)
        .and_then(|settings| {
            settings
                .get(setting_keys::MONTHLY_CAP_USD)
                .and_then(Value::as_f64)
        })
        .filter(|cap| cap.is_finite() && *cap > 0.0)
}

/// This month's spend from the last read of each provider.
pub(super) fn read_spend(state: &AppState) -> CloudSpend {
    let month = super::time::this_month();
    let providers = store::read_spend(&state.paths, &month).unwrap_or_default();
    let total_usd = providers.iter().map(|line| line.usd).sum();
    let cap_usd = monthly_cap_usd(state);
    CloudSpend {
        month,
        state: CloudSpend::state_for(total_usd, cap_usd),
        providers,
        total_usd,
        cap_usd,
    }
}

/// Refuses a new box once the cap is reached.
///
/// CDXC:CloudBoxes 2026-11-02 DECISION: the user chose one monthly cap across providers that counts Instabox's month total (all use on that account) plus Ghostex-made Boat boxes (D8, "instabox month plus boat boxes"); Ghostex warns at 80% and refuses new boxes at 100%, while running boxes keep running, so Ghostex never sets Instabox's own `spendCapUSD` (Instabox stops running boxes at it).
pub(super) fn check_cap(state: &AppState) -> Result<CloudSpend, DomainStateError> {
    let spend = read_spend(state);
    if spend.state == CloudSpendState::Reached {
        return Err(DomainStateError {
            code: error_codes::CAP_REACHED,
            message: format!(
                "This month's Cloud Box spend (${:.2}) reached your ${:.2} cap. Raise the cap in Settings › Cloud Boxes to start new boxes; running boxes keep running.",
                spend.total_usd,
                spend.cap_usd.unwrap_or_default()
            ),
        });
    }
    Ok(spend)
}

fn line(
    provider: CloudProviderId,
    usd: f64,
    scope: CloudSpendScope,
    estimated: bool,
    error: Option<String>,
) -> CloudSpendLine {
    CloudSpendLine {
        provider,
        usd,
        scope,
        read_at: Some(super::time::now_iso()),
        estimated,
        error,
    }
}

/// Instabox: the org's month total (scope `account`), and each box's estimate from its running
/// seconds and the price table (Instabox reports seconds per box, not dollars; R6).
fn refresh_instabox(state: &AppState, key: &str) -> CloudSpendLine {
    let month_start = super::time::month_start_iso();
    for stored in store::list_boxes(&state.paths).unwrap_or_default() {
        let row = &stored.row;
        let (Some(machine_id), CloudProviderId::Instabox) =
            (row.provider_box_id.as_deref(), &row.provider)
        else {
            continue;
        };
        if row.state == CloudBoxState::Deleted {
            continue;
        }
        let rate = row.usd_per_hour.unwrap_or(0.0);
        let from_created = row
            .created_at
            .clone()
            .unwrap_or_else(|| month_start.clone());
        let total = call(
            instabox::machine_usage_request(key, machine_id, &from_created),
            instabox::parse_running_seconds,
        );
        let month_from = if from_created > month_start {
            from_created.clone()
        } else {
            month_start.clone()
        };
        let month = call(
            instabox::machine_usage_request(key, machine_id, &month_from),
            instabox::parse_running_seconds,
        );
        if let (Ok(total), Ok(month)) = (total, month) {
            let _ = registry::update(&state.paths, &row.box_id, |stored| {
                stored.row.cost_total_usd =
                    Some(ghostex_gx_cloud::prices::estimate_usd(total, rate));
                stored.row.cost_month_usd =
                    Some(ghostex_gx_cloud::prices::estimate_usd(month, rate));
                stored.row.cost_estimated = true;
            });
        }
    }
    match call(instabox::usage_request(key), instabox::parse_usage) {
        Ok(account) => line(
            CloudProviderId::Instabox,
            account.month_usd.unwrap_or(0.0),
            CloudSpendScope::Account,
            false,
            None,
        ),
        Err(error) => {
            let last = store::read_spend(&state.paths, &super::time::this_month())
                .unwrap_or_default()
                .into_iter()
                .find(|line| line.provider == CloudProviderId::Instabox)
                .map(|line| line.usd)
                .unwrap_or(0.0);
            line(
                CloudProviderId::Instabox,
                last,
                CloudSpendScope::Account,
                false,
                Some(error.message),
            )
        }
    }
}

/// Boat: the sum of Ghostex boxes' dollars this month (its list carries them) plus deleted boxes',
/// and each box's whole-life dollars.
fn refresh_boat(
    state: &AppState,
    key: &str,
    machines: &[ProviderMachine],
    deleted_usd: f64,
) -> CloudSpendLine {
    for stored in store::list_boxes(&state.paths).unwrap_or_default() {
        let row = &stored.row;
        let (Some(sandbox_id), CloudProviderId::Boat) =
            (row.provider_box_id.as_deref(), &row.provider)
        else {
            continue;
        };
        if row.state == CloudBoxState::Deleted {
            continue;
        }
        if let Ok(total) = call(
            boat::usage_request(key, sandbox_id),
            boat::parse_usage_dollars,
        ) {
            let _ = registry::update(&state.paths, &row.box_id, |stored| {
                stored.row.cost_total_usd = Some(total);
                stored.row.cost_estimated = false;
            });
        }
    }
    let month: f64 = machines
        .iter()
        .filter_map(|machine| machine.cost_month_usd)
        .sum();
    line(
        CloudProviderId::Boat,
        month + deleted_usd,
        CloudSpendScope::GhostexBoxes,
        false,
        None,
    )
}

/// Reads every connected provider's spend and saves it; tells clients when the cap state moved.
/// Blocking.
pub(super) fn refresh(state: &AppState) {
    let month = super::time::this_month();
    let before = read_spend(state).state;
    for provider in super::reconcile::providers_to_check(state) {
        let spend_line = match &provider {
            CloudProviderId::Local => line(
                CloudProviderId::Local,
                0.0,
                CloudSpendScope::GhostexBoxes,
                false,
                None,
            ),
            CloudProviderId::Instabox => {
                match super::connect::provider_key(&state.paths, &provider) {
                    Ok(key) => refresh_instabox(state, &key),
                    Err(_) => continue,
                }
            }
            CloudProviderId::Boat => {
                let Ok(key) = super::connect::provider_key(&state.paths, &provider) else {
                    continue;
                };
                let listed = Ops::Boat { key: key.clone() }.list();
                match listed {
                    Ok((machines, deleted)) => refresh_boat(state, &key, &machines, deleted),
                    Err(message) => line(
                        CloudProviderId::Boat,
                        0.0,
                        CloudSpendScope::GhostexBoxes,
                        false,
                        Some(message),
                    ),
                }
            }
            CloudProviderId::Other(_) => continue,
        };
        let _ = store::save_spend(&state.paths, &month, &spend_line);
    }
    let after = read_spend(state).state;
    events::broadcast_scope(
        state,
        if after != before {
            &["spend", "boxes"]
        } else {
            &["spend"]
        },
    );
}
