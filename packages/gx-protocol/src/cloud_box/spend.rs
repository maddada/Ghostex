//! This month's spend and the monthly cap (Q1000, Q1001, D8).

use serde::{Deserialize, Serialize};

use super::enums::{CloudProviderId, CloudSpendScope, CloudSpendState};

/// The fraction of the cap at which Ghostex warns (toast + chip tint).
pub const CLOUD_SPEND_WARN_FRACTION: f64 = 0.8;

/// Result of `spend`: "This month: Instabox $1.42 · Boat $0.30 · total $1.72 of $20 cap".
///
/// D8: one cap across providers, summing Instabox's month total (all use on that account) and the
/// cost of Ghostex-made Boat boxes. Ghostex never sets Instabox's own `spendCapUSD`, because
/// Instabox stops running boxes at it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSpend {
    /// `YYYY-MM`, UTC.
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub month: String,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub providers: Vec<CloudSpendLine>,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub total_usd: f64,
    /// The cap (`cloudBoxesMonthlyCapUsd`); absent when off.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cap_usd: Option<f64>,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub state: CloudSpendState,
}

impl CloudSpend {
    /// Where `total_usd` stands against `cap_usd`.
    pub fn state_for(total_usd: f64, cap_usd: Option<f64>) -> CloudSpendState {
        match cap_usd {
            None => CloudSpendState::Off,
            Some(cap) if total_usd >= cap => CloudSpendState::Reached,
            Some(cap) if total_usd >= cap * CLOUD_SPEND_WARN_FRACTION => CloudSpendState::Warn,
            Some(_) => CloudSpendState::Ok,
        }
    }
}

/// One provider's part of this month's spend.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CloudSpendLine {
    pub provider: CloudProviderId,
    #[serde(default, deserialize_with = "crate::de::null_as_default")]
    pub usd: f64,
    pub scope: CloudSpendScope,
    /// When the figure was read from the provider (ISO-8601).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_at: Option<String>,
    /// Ghostex's estimate rather than the provider's own dollars.
    #[serde(
        default,
        deserialize_with = "crate::de::null_as_default",
        skip_serializing_if = "std::ops::Not::not"
    )]
    pub estimated: bool,
    /// The read failed; `usd` is the last figure Ghostex had.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}
