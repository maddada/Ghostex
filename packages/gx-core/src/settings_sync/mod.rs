//! Settings sync between the user's own computers, peer to peer: every synced setting is a key
//! with a clock, the newest write of a key wins, and a deletion is kept as a tombstone so it
//! spreads instead of coming back. gxserver stores the entries and trades them over the peer link
//! the desktop app already opens between two connected computers; the phone runs the same rules.
//!
//! CDXC:SettingsSync 2026-11-06 DECISION:
//! User (Q1370 to Q1378): settings sync only between the user's own computers when they are connected to each other, never through a server ("peer to peer only, not centralized"), over the links Ghostex already has (Easy Connect and Tailscale, "why are u talking about iroh"). Merge is our own per-key last-writer-wins; when two computers changed the same thing apart, the later edit wins and the other gets a "changed on X · Undo" note; a deleted Space is deleted everywhere; the computers in the group are the ones already paired with each other, and a removed computer stops syncing; the phone is a full peer; a Settings > Sync toggle list where only Spaces is on by default; no per-computer overrides; sign-ins and secrets never sync.
//!
//! SEE-ALSO: server/src/settings_sync/ (storage, the exchange and the Spaces bridge), apps/desktop/src/app/window/settings_modal/tabs/sync.rs (Settings > Sync).

use serde_json::Value;

mod entries;
mod hlc;
mod lists;
mod settings_keys;
mod spaces;
mod spaces_repair;

pub use entries::{
    entries_digest, local_edit, merge_entries, MergeOutcome, SyncConflict, SyncEntry,
};
pub use hlc::{hlc_wall_ms, next_hlc, seed_hlc};
pub use lists::{
    account_sync_value, agent_roster_values, apply_agent_roster_entry, apply_synced_account,
    describe_list_key, skill_file_path_safe, skill_fits, skill_hash, skill_name_syncs,
    ACCOUNT_KEY_PREFIX, AGENT_KEY_PREFIX, AGENT_ORDER_KEY, BUNDLED_SKILL_KEY_PREFIX,
    MAX_SAVED_ACCOUNTS, MAX_SKILLS_BYTES, MAX_SKILL_BYTES, MAX_SKILL_FILES, OWN_SKILL_KEY_PREFIX,
};
pub use settings_keys::{
    category_setting_keys, category_setting_values, describe_setting_key, setting_of_sync_key,
    setting_sync_key,
};
pub use spaces::{
    apply_spaces_entries, derive_spaces_edits, describe_spaces_key, group_deleted_edit,
    merge_spaces_by_name, spaces_named_apart, LocalSpaces, SpacesApplied,
};
pub use spaces_repair::{repair_folded_spaces, spaces_moves, SpacesMove};

/// One thing a computer can sync, as Settings > Sync lists it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SyncCategory {
    /// The first segment of its keys.
    pub prefix: &'static str,
    /// Its switch in the settings file (one per computer).
    pub setting_key: &'static str,
    pub label: &'static str,
    pub default_on: bool,
    /// Built: its switch can be turned on; one that isn't shows greyed.
    pub available: bool,
}

/// The categories a computer can sync, in Settings > Sync order.
///
/// `group` (who is in the sync group) always syncs; it is not a user choice.
pub const SYNC_CATEGORIES: &[SyncCategory] = &[
    SyncCategory {
        prefix: "spaces",
        setting_key: "syncSpaces",
        label: "Spaces",
        default_on: true,
        available: true,
    },
    SyncCategory {
        prefix: "appearance",
        setting_key: "syncAppearance",
        label: "Appearance",
        default_on: false,
        available: true,
    },
    SyncCategory {
        prefix: "agents",
        setting_key: "syncAgentDefaults",
        label: "Agent defaults",
        default_on: false,
        available: true,
    },
    // CDXC:SettingsSync 2026-11-07 DECISION in lists.rs: Agents, Skills and Accounts, off by default.
    SyncCategory {
        prefix: "agentRoster",
        setting_key: "syncAgents",
        label: "Agents",
        default_on: false,
        available: true,
    },
    SyncCategory {
        prefix: "skills",
        setting_key: "syncSkills",
        label: "Skills",
        default_on: false,
        available: true,
    },
    SyncCategory {
        prefix: "accounts",
        setting_key: "syncAccounts",
        label: "Accounts",
        default_on: false,
        available: true,
    },
    SyncCategory {
        prefix: "sounds",
        setting_key: "syncSoundsAndNotifications",
        label: "Sounds and notifications",
        default_on: false,
        available: true,
    },
    SyncCategory {
        prefix: "keybindings",
        setting_key: "syncKeybindings",
        label: "Keybindings",
        default_on: false,
        available: true,
    },
    SyncCategory {
        prefix: "coordinator",
        setting_key: "syncCoordinator",
        label: "Coordinator",
        default_on: false,
        available: true,
    },
];

/// Whether `category` syncs on a computer whose settings file holds `settings`.
pub fn category_enabled(category: &SyncCategory, settings: Option<&Value>) -> bool {
    category.available
        && settings
            .and_then(|settings| settings.get(category.setting_key))
            .and_then(Value::as_bool)
            .unwrap_or(category.default_on)
}

/// The key prefix that always syncs: the members of the sync group and the removed computers.
pub const GROUP_CATEGORY: &str = "group";

/// The sync group's id for a paired phone (`phone-<id>`), from the id the phone made for itself:
/// 4 to 64 letters, digits or dashes.
///
/// CDXC:SettingsSync 2026-11-06 DECISION:
/// User (Q1375): the phone is "a full member: holds a copy, edits offline, carries changes". It joins the group under its own id, can be removed like a computer, and keeps every entry it gets so it carries changes between computers that never meet; each computer it reaches does the merging (apps/mobile/app/src/settingsSync/).
pub fn phone_member_id(phone_id: &str) -> Option<String> {
    let valid = (4..=64).contains(&phone_id.len())
        && phone_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-');
    valid.then(|| format!("phone-{phone_id}"))
}

/// A sync group member that is a phone rather than a computer.
pub fn is_phone_member(member_id: &str) -> bool {
    member_id
        .strip_prefix("phone-")
        .is_some_and(|id| phone_member_id(id).is_some())
}

/// The category a key belongs to: its first path segment.
pub fn key_category(key: &str) -> &str {
    key.split('/').next().unwrap_or_default()
}

/// `group/member/<serverId>`: a computer in the sync group (value `{"name": …}`).
pub fn member_key(server_id: &str) -> String {
    format!("group/member/{server_id}")
}

/// `group/removed/<serverId>`: a computer the user removed (value `{"by": …}`); a tombstone here
/// means it was added back.
pub fn removed_key(server_id: &str) -> String {
    format!("group/removed/{server_id}")
}
