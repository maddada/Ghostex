//! This month's spend and the monthly cap.

use ghostex_gx_protocol::cloud_box::{
    error_codes, setting_keys, CloudProviderId, CloudSpend, CloudSpendLine, CloudSpendScope,
    CloudSpendState,
};
use serde_json::Value;

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

/// This month's spend across providers. `local` never bills; the Instabox and Boat lines are read
/// from the providers once they are connected.
pub(super) fn read_spend(state: &AppState, month: &str) -> CloudSpend {
    let providers = vec![CloudSpendLine {
        provider: CloudProviderId::Local,
        usd: 0.0,
        scope: CloudSpendScope::GhostexBoxes,
        read_at: Some(crate::domain::params::now_iso()),
        estimated: false,
        error: None,
    }];
    let total_usd = providers.iter().map(|line| line.usd).sum();
    let cap_usd = monthly_cap_usd(state);
    CloudSpend {
        month: month.to_string(),
        state: CloudSpend::state_for(total_usd, cap_usd),
        providers,
        total_usd,
        cap_usd,
    }
}

/// `YYYY-MM` of an ISO-8601 time.
pub(super) fn month_of(iso: &str) -> String {
    iso.chars().take(7).collect()
}

/// Refuses a new box once the cap is reached.
///
/// CDXC:CloudBoxes 2026-11-02 DECISION: the user chose one monthly cap across providers that counts Instabox's month total (all use on that account) plus Ghostex-made Boat boxes (D8, "instabox month plus boat boxes"); Ghostex warns at 80% and refuses new boxes at 100%, while running boxes keep running, so Ghostex never sets Instabox's own `spendCapUSD` (Instabox stops running boxes at it).
pub(super) fn check_cap(state: &AppState) -> Result<CloudSpend, DomainStateError> {
    let spend = read_spend(state, &month_of(&crate::domain::params::now_iso()));
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
