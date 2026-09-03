/*
CDXC:Telemetry 2026-08-27 (addendum v2, §3):
The profile properties that ride EVERY event: `interface`,
`default_agent`, `project_bucket`, `identity_source` — so the PostHog UI can
break any event down by "what kind of install is this?" without a HogQL join.

The whole design constraint here is COST. `capture` is called from inside domain
writes, HTTP handlers, and prompt dispatch; it must stay a gate check plus a
push. So the two halves of the profile are refreshed on completely different
clocks, and neither of them is "per capture":

- Settings-derived (`interface`) comes back from the gate's
  existing mtime-keyed read (`gate::evaluate`). The capture path was already
  doing exactly one `fs::metadata` on that file to answer the opt-out question;
  these fields ride along on the same cached parse, so the syscall count per
  capture is unchanged.
- DB-derived (`project_bucket`, `default_agent`) live in this `ProfileSnapshot`
  behind the `Telemetry` mutex, refreshed once at startup and then once per
  heartbeat run. Being up to a day stale is explicitly acceptable: neither field
  changes often, and the alternative is a SQLite query per captured event.
- `identity_source` is fixed for the process lifetime (`telemetry::identity`),
  so it is simply carried on the `Telemetry` struct.

Every field is an `Option`. A missing one is OMITTED, never defaulted: a wrong
`default_agent` on a few thousand events is worse than a null, because a null is
visibly a null in the UI and a wrong enum is not.
*/

use super::{gate::SettingsProfile, taxonomy::PropertyValue};

/// The DB-derived half of the profile. Held behind the `Telemetry` lock and
/// replaced wholesale on refresh, so a reader never sees a half-updated pair.
#[derive(Clone, Copy, Debug, Default)]
pub struct ProfileSnapshot {
    pub project_bucket: Option<&'static str>,
    pub default_agent: Option<&'static str>,
}

impl ProfileSnapshot {
    /// Build from the two raw values the heartbeat already collects, so the
    /// refresh and the heartbeat can never disagree about what they mean.
    pub fn from_counts(project_count: usize, default_agent: &'static str) -> Self {
        Self {
            project_bucket: Some(super::taxonomy::project_bucket(project_count)),
            default_agent: Some(default_agent),
        }
    }
}

/// Assemble the property list merged onto every captured event. Returns pairs
/// in the same shape a call site uses, so they go through
/// `taxonomy::validate` unchanged — there is no bypass for profile fields.
pub fn profile_properties(
    settings: &SettingsProfile,
    snapshot: &ProfileSnapshot,
    identity_source: &'static str,
) -> Vec<(&'static str, PropertyValue)> {
    let mut properties: Vec<(&'static str, PropertyValue)> = Vec::with_capacity(5);
    if let Some(interface) = settings.interface {
        properties.push(("interface", PropertyValue::Enum(interface)));
    }
    if let Some(default_agent) = snapshot.default_agent {
        properties.push(("default_agent", PropertyValue::Enum(default_agent)));
    }
    if let Some(project_bucket) = snapshot.project_bucket {
        properties.push(("project_bucket", PropertyValue::Enum(project_bucket)));
    }
    properties.push(("identity_source", PropertyValue::Enum(identity_source)));
    properties
}
