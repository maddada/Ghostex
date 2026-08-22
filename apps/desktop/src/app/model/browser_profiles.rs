// C1 wave-3 re-cluster: the BrowserProfileId/BrowserProfileModel browser-profile types and their shell-state persistence, moved verbatim out of the
// types1.rs..types6.rs chunk split (docs/2026-08-22/repo-restructure/SPLITS.md
// C1) into this descriptively named module per its FOLLOW-UPS.md note (pure
// move, no logic changes).

use crate::*;


#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct BrowserProfileId(pub(crate) u64);


impl BrowserProfileId {
    pub(crate) fn default_profile() -> Self {
        Self(BROWSER_PROFILE_DEFAULT_ID)
    }

    pub(crate) fn display_label(self) -> String {
        format!("Profile {}", self.0)
    }

    pub(crate) fn display_number(self) -> Option<u64> {
        (self != Self::default_profile()).then_some(self.0)
    }

    pub(crate) fn cef_profile_string(self) -> String {
        if self == Self::default_profile() {
            BROWSER_PROFILE_DEFAULT_CEF_ID.to_string()
        } else {
            format!("profile-{}", self.0)
        }
    }
}


pub(crate) struct BrowserProfileModel {
    pub(crate) profiles: Vec<BrowserProfileId>,
    pub(crate) active_profile: BrowserProfileId,
    pub(crate) next_profile_id: u64,
}


impl BrowserProfileModel {
    pub(crate) fn shell_default() -> Self {
        /*
        CDXC:GPUIBrowserProfiles 2026-06-23-11:14:
        GPUI Browser profile parity is shell-owned and generated-only for this slice: keep a built-in Default profile plus app-generated Profile N ids, persist only those stable ids and the active id, and avoid user-entered names, profile paths, cookies, credentials, history, page titles, URLs, command text, or local paths.
        */
        Self {
            profiles: vec![BrowserProfileId::default_profile()],
            active_profile: BrowserProfileId::default_profile(),
            next_profile_id: BROWSER_PROFILE_FIRST_GENERATED_ID,
        }
    }

    pub(crate) fn contains_profile(&self, profile_id: BrowserProfileId) -> bool {
        self.profiles.contains(&profile_id)
    }

    pub(crate) fn active_profile_id(&self) -> BrowserProfileId {
        if self.contains_profile(self.active_profile) {
            self.active_profile
        } else {
            BrowserProfileId::default_profile()
        }
    }

    pub(crate) fn profile_ids(&self) -> impl Iterator<Item = BrowserProfileId> + '_ {
        self.profiles.iter().copied()
    }

    pub(crate) fn select_profile(&mut self, profile_id: BrowserProfileId) -> bool {
        if !self.contains_profile(profile_id) || self.active_profile_id() == profile_id {
            return false;
        }

        self.active_profile = profile_id;
        true
    }

    pub(crate) fn create_generated_profile(&mut self) -> Option<BrowserProfileId> {
        if self.profiles.len() >= BROWSER_PROFILE_MAX_PROFILES {
            return None;
        }

        let mut next_id = self.next_profile_id.max(BROWSER_PROFILE_FIRST_GENERATED_ID);
        while self.contains_profile(BrowserProfileId(next_id)) {
            if next_id == u64::MAX {
                return None;
            }
            next_id = next_id.saturating_add(1);
        }

        let profile_id = BrowserProfileId(next_id);
        self.profiles.push(profile_id);
        self.active_profile = profile_id;
        self.next_profile_id = next_id.saturating_add(1);
        Some(profile_id)
    }
}


pub(crate) fn browser_profile_model_to_shell_state_json(model: &BrowserProfileModel) -> serde_json::Value {
    /*
    CDXC:GPUIBrowserProfiles 2026-06-23-11:14:
    Browser profile shell-state serialization is sanitized at the writer boundary: persist only generated numeric profile ids, the active generated id, and the next generated id. Never persist profile display names from user input, filesystem paths, CEF cache directories, imported data choices, cookies, credentials, history, URLs, page titles, command text, or terminal content.
    */
    serde_json::json!({
        "profiles": model
            .profile_ids()
            .map(|profile_id| serde_json::json!(profile_id.0))
            .collect::<Vec<_>>(),
        "activeProfileId": model.active_profile_id().0,
        "nextProfileId": model.next_profile_id.max(BROWSER_PROFILE_FIRST_GENERATED_ID),
    })
}


pub(crate) fn browser_profile_model_from_shell_state(
    value: &serde_json::Value,
) -> Option<BrowserProfileModel> {
    let object = value.as_object()?;
    let mut profiles = json_array_field(object, "profiles")?
        .iter()
        .map(json_u64_value)
        .collect::<Option<Vec<_>>>()?
        .into_iter()
        .filter(|profile_id| *profile_id >= BROWSER_PROFILE_DEFAULT_ID)
        .map(BrowserProfileId)
        .collect::<Vec<_>>();

    if !profiles.contains(&BrowserProfileId::default_profile()) {
        profiles.push(BrowserProfileId::default_profile());
    }
    profiles.sort_by_key(|profile_id| {
        if *profile_id == BrowserProfileId::default_profile() {
            0
        } else {
            profile_id.0
        }
    });
    profiles.dedup();
    if profiles.is_empty()
        || profiles.len() > BROWSER_PROFILE_MAX_PROFILES
        || has_duplicate_u64(
            &profiles
                .iter()
                .map(|profile_id| profile_id.0)
                .collect::<Vec<_>>(),
        )
    {
        return None;
    }

    let active_profile = json_u64_field(object, "activeProfileId")
        .map(BrowserProfileId)
        .filter(|profile_id| profiles.contains(profile_id))
        .unwrap_or_else(BrowserProfileId::default_profile);
    let max_profile_id = profiles
        .iter()
        .map(|profile_id| profile_id.0)
        .max()
        .unwrap_or(BROWSER_PROFILE_DEFAULT_ID);
    let next_profile_id = json_u64_field(object, "nextProfileId")
        .unwrap_or(BROWSER_PROFILE_FIRST_GENERATED_ID)
        .max(BROWSER_PROFILE_FIRST_GENERATED_ID)
        .max(max_profile_id.saturating_add(1));

    Some(BrowserProfileModel {
        profiles,
        active_profile,
        next_profile_id,
    })
}
