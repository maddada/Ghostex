//! The categories that sync values rather than documents: Appearance, Agent defaults, Agents,
//! Skills, Accounts, Sounds and notifications, Keybindings and Coordinator (which keys: gx-core
//! `settings_sync/settings_keys.rs` and `settings_sync/lists.rs`; the lists' reads and writes:
//! lists_bridge.rs).
//!
//! CDXC:SettingsSync 2026-11-06 WHY:
//! Settings-file values reach this computer through the running desktop app's `updateSettingsPatch`, like every settings write (CDXC:Settings 2026-09-09 in ghostex_cli/settings.rs), so the app normalizes and applies them at once; while the app is closed they wait for the next round. Turning a category on (or its first sync) takes this computer's own values in with `seed_hlc`, older than any real edit and without the ones still at their default, so it never overrides what another computer already synced; it only fills in what nobody did.

use std::collections::{BTreeMap, BTreeSet};

use ghostex_gx_core::settings_sync::{
    category_setting_values, hlc_wall_ms, key_category, seed_hlc, setting_of_sync_key, SyncEntry,
};
use serde_json::{Map, Value};

use crate::domain::DomainStateError;

use super::engine::Engine;
use super::lists_bridge::LIST_CATEGORIES;
use super::spaces_bridge::{project_repo_keys, KeyBy};

/// Every category but Spaces, in Settings > Sync order.
pub(super) const VALUE_CATEGORIES: [&str; 8] = [
    "appearance",
    "agents",
    "agentRoster",
    "skills",
    "accounts",
    "sounds",
    "keybindings",
    "coordinator",
];

const MODEL_PIN_PREFIX: &str = "agents/modelPin/";
const PLAYBOOK_ALL: &str = "coordinator/playbook/all";
/// A project's playbook keyed by its repository alone, before #488's follow-up: every folder of
/// one monorepo shared it. Kept in the entries for computers that still read it, never written or
/// deleted by a computer that keys by folder.
const PLAYBOOK_PROJECT_PREFIX: &str = "coordinator/playbook/project/";
/// A project's playbook keyed by its repository and its folder inside it
/// (`unified_sidebar::project_repo_key`), as Spaces are.
const PLAYBOOK_FOLDER_PREFIX: &str = "coordinator/playbook/folder/";
const PLAYBOOK_OVERRIDES: &str = "coordinator/playbook/overrides";

/// The prefix project playbooks sync under, and project id → key under it.
type PlaybookKeys = (&'static str, Vec<(String, String)>);

/// Keys that carry what agents are told or run: coordinator playbooks, the user's own skills and
/// the agents' commands.
fn carries_agent_instructions(key: &str) -> bool {
    key.starts_with("coordinator/playbook/")
        || key.starts_with(ghostex_gx_core::settings_sync::OWN_SKILL_KEY_PREFIX)
        || key.starts_with(ghostex_gx_core::settings_sync::AGENT_KEY_PREFIX)
}

/// CDXC:Security 2026-11-08 WHY: a playbook, a skill or an agent's command that arrives from another computer becomes what this computer's agents are told or run, so it is a way to put words into a coordinator from elsewhere (the design review, GitHub #495, section 5). These categories sync only when the user turns them on (CDXC:SettingsSync 2026-11-07 DECISION in gx-core lists.rs); each such change is recorded in the security log with its notification, so the user sees it when it happens. Holding them for the owner's approval is the approvals gate's part.
fn log_incoming_instructions(wanted: &[(String, Option<Value>, String)]) {
    for (key, _, origin) in wanted {
        if !carries_agent_instructions(key) {
            continue;
        }
        let what = if key.starts_with("coordinator/playbook/") {
            "A coordinator playbook"
        } else if key.starts_with(ghostex_gx_core::settings_sync::OWN_SKILL_KEY_PREFIX) {
            "A skill"
        } else {
            "An agent's command"
        };
        crate::audit::record(
            crate::audit::Action::new(
                "settingsSync.instructions",
                format!("{what} changed by settings sync from {origin}"),
            )
            .target(key.clone()),
            crate::audit::Actor::SettingsSync {
                computer: origin.clone(),
            },
            crate::audit::Outcome::Done,
            None,
        );
    }
}

impl Engine<'_> {
    /// What this computer has for category `prefix`, by synced key, and the keys still at their
    /// default (a fresh computer's defaults never seed).
    fn category_values(
        &self,
        prefix: &str,
    ) -> Result<(BTreeMap<String, Value>, BTreeSet<String>), DomainStateError> {
        let (mut values, defaults) = category_setting_values(prefix, self.settings());
        match prefix {
            "agents" => {
                for (family, pin) in crate::agent_model_pins::default_pins() {
                    values.insert(format!("{MODEL_PIN_PREFIX}{family}"), pin);
                }
            }
            "coordinator" => {
                let (all, projects) = crate::coordinators::synced_playbooks(self.db)?;
                if let Some(overrides) = crate::coordinators::synced_playbook_overrides(self.db)? {
                    values.insert(PLAYBOOK_OVERRIDES.to_string(), overrides);
                }
                if let Some(all) = all {
                    values.insert(PLAYBOOK_ALL.to_string(), all);
                }
                let (key_prefix, repos) = self.playbook_keys()?;
                for (project_id, copy) in projects {
                    if let Some((_, repo)) = repos.iter().find(|(id, _)| *id == project_id) {
                        values.insert(format!("{key_prefix}{repo}"), copy);
                    }
                }
            }
            prefix if LIST_CATEGORIES.contains(&prefix) => values.extend(self.list_values(prefix)?),
            _ => {}
        }
        Ok((values, defaults.into_iter().collect()))
    }

    /// The prefix project playbooks sync under here and each project's key under it.
    fn playbook_keys(&self) -> Result<PlaybookKeys, DomainStateError> {
        let (prefix, key_by) = if self.store.playbooks_keyed_by_folder {
            (PLAYBOOK_FOLDER_PREFIX, KeyBy::RepositoryAndFolder)
        } else {
            (PLAYBOOK_PROJECT_PREFIX, KeyBy::Repository)
        };
        let keys = project_repo_keys(self.db, &self.server_id, &self.store.entries, key_by)?;
        Ok((prefix, keys.repos))
    }

    /// A Coordinator entry this computer neither takes in nor tells about: a repository-only
    /// playbook once playbooks are keyed by folder here, and a folder playbook another computer
    /// only seeded where this one already has that project's playbook.
    ///
    /// CDXC:SettingsSync 2026-11-08 WHY:
    /// #488's follow-up: keyed by repository alone, every folder project of one monorepo shared one playbook key, so another computer gave only the first of them a playbook. Playbooks now sync under `coordinator/playbook/folder/<repository#folder>` (the Spaces rule). Switching keys must not delete or overwrite anything elsewhere: the old `…/project/` entries stay as they are for computers that still read them (this one stops reading them, since it can't tell which folder one was for), and each computer seeds its folder playbooks (`seed_hlc`, older than any real edit), which only fill a project that has no playbook on another computer and never replace one it has.
    fn playbook_entry_ignored(&self, entry: &SyncEntry, current: &BTreeMap<String, Value>) -> bool {
        (self.store.playbooks_keyed_by_folder && entry.key.starts_with(PLAYBOOK_PROJECT_PREFIX))
            || (entry.key.starts_with(PLAYBOOK_FOLDER_PREFIX)
                && hlc_wall_ms(&entry.hlc) == 0
                && current.contains_key(&entry.key))
    }

    /// Once, when every project's repository root is known: project playbooks move to keys by
    /// repository and folder, seeding this computer's under the new keys (CDXC:SettingsSync
    /// 2026-11-08 above) and forgetting the old keys without deleting them anywhere.
    fn key_playbooks_by_folder(&mut self) -> Result<(), DomainStateError> {
        if self.store.playbooks_keyed_by_folder || !self.enabled("coordinator/") {
            return Ok(());
        }
        let keys = project_repo_keys(
            self.db,
            &self.server_id,
            &self.store.entries,
            KeyBy::RepositoryAndFolder,
        )?;
        if !keys.remotes_probed {
            return Ok(());
        }
        self.store.playbooks_keyed_by_folder = true;
        // Before the first sync of this category everything seeds anyway.
        if !self.store.value_baselines.contains_key("coordinator") {
            return Ok(());
        }
        let (current, _) = self.category_values("coordinator")?;
        let seeds: Vec<(String, Value)> = current
            .into_iter()
            .filter(|(key, _)| key.starts_with(PLAYBOOK_FOLDER_PREFIX))
            .collect();
        for (key, value) in &seeds {
            if !self.store.entries.contains_key(key) {
                self.store.entries.insert(
                    key.clone(),
                    SyncEntry {
                        key: key.clone(),
                        value: Some(value.clone()),
                        hlc: seed_hlc(&self.server_id),
                        parent: None,
                        origin: self.server_id.clone(),
                        origin_name: self.name.clone(),
                    },
                );
            }
        }
        if let Some(baseline) = self.store.value_baselines.get_mut("coordinator") {
            baseline.retain(|key, _| !key.starts_with(PLAYBOOK_PROJECT_PREFIX));
            baseline.extend(seeds);
        }
        Ok(())
    }

    /// Whether category `prefix` can sync this step. Coordinator waits until every project's git
    /// origin and repository root are probed (a moment after gxserver starts).
    ///
    /// CDXC:SettingsSync 2026-11-08 WHY:
    /// Until then a project's playbook key falls back to its folder name (`dir:trading`), so the first step after every start deleted each repository's playbook key on every computer and wrote `dir:` copies; found proving #488's follow-up on two throwaway gxservers.
    fn category_ready(&self, prefix: &str) -> Result<bool, DomainStateError> {
        if prefix != "coordinator" {
            return Ok(true);
        }
        let keys = project_repo_keys(
            self.db,
            &self.server_id,
            &self.store.entries,
            KeyBy::Repository,
        )?;
        Ok(keys.remotes_probed)
    }

    /// Takes what changed in each value category since the last sync into the entries.
    pub(super) fn export_values(&mut self) -> Result<(), DomainStateError> {
        self.key_playbooks_by_folder()?;
        for prefix in VALUE_CATEGORIES {
            if !self.enabled(&format!("{prefix}/")) {
                // Turned off: the next time it is on, this computer's values seed again.
                self.store.value_baselines.remove(prefix);
                continue;
            }
            if !self.category_ready(prefix)? {
                continue;
            }
            let (current, defaults) = self.category_values(prefix)?;
            match self.store.value_baselines.get(prefix).cloned() {
                None => {
                    for (key, value) in &current {
                        if defaults.contains(key) || self.store.entries.contains_key(key) {
                            continue;
                        }
                        self.store.entries.insert(
                            key.clone(),
                            SyncEntry {
                                key: key.clone(),
                                value: Some(value.clone()),
                                hlc: seed_hlc(&self.server_id),
                                parent: None,
                                origin: self.server_id.clone(),
                                origin_name: self.name.clone(),
                            },
                        );
                    }
                }
                Some(baseline) => {
                    for (key, value) in &current {
                        if baseline.get(key) != Some(value) {
                            self.edit(key, Some(value.clone()));
                        }
                    }
                    for key in baseline.keys().filter(|key| !current.contains_key(*key)) {
                        self.edit(key, None);
                    }
                }
            }
            self.store
                .value_baselines
                .insert(prefix.to_string(), current);
        }
        Ok(())
    }

    /// Makes this computer say what the entries say: remembered models and playbooks are written
    /// here; the settings-file values are returned as one patch for the app to save.
    pub(super) fn import_values(&mut self) -> Result<Map<String, Value>, DomainStateError> {
        let mut patch = Map::new();
        for prefix in VALUE_CATEGORIES {
            if !self.enabled(&format!("{prefix}/")) || !self.category_ready(prefix)? {
                continue;
            }
            let (current, _) = self.category_values(prefix)?;
            let wanted: Vec<(String, Option<Value>, String)> = self
                .store
                .entries
                .values()
                .filter(|entry| key_category(&entry.key) == prefix)
                .filter(|entry| current.get(&entry.key) != entry.value.as_ref())
                .filter(|entry| !self.playbook_entry_ignored(entry, &current))
                .map(|entry| {
                    (
                        entry.key.clone(),
                        entry.value.clone(),
                        entry.origin_name.clone(),
                    )
                })
                .collect();
            if wanted.is_empty() {
                continue;
            }
            log_incoming_instructions(&wanted);
            if LIST_CATEGORIES.contains(&prefix) {
                self.write_list_values(prefix, &wanted)?;
            }
            let mut playbook_all: Option<Option<&Value>> = None;
            let mut playbook_projects: Vec<(String, Option<&Value>)> = Vec::new();
            let (playbook_prefix, repos) = self.playbook_keys()?;
            for (key, value, _) in &wanted {
                if let Some(setting) = setting_of_sync_key(key) {
                    let value = value.clone().or_else(|| {
                        ghostex_settings_catalog::default_value(setting).map(|default| {
                            ghostex_settings_catalog::ToJson::to_json(default).to_value()
                        })
                    });
                    if let Some(value) = value.filter(|value| {
                        ghostex_gx_core::settings_patch_value_allowed(setting, value)
                    }) {
                        patch.insert(setting.to_string(), value);
                    }
                } else if let Some(family) = key.strip_prefix(MODEL_PIN_PREFIX) {
                    crate::agent_model_pins::set_synced_default(family, value.as_ref());
                } else if key == PLAYBOOK_OVERRIDES {
                    crate::coordinators::write_synced_playbook_overrides(self.db, value.as_ref())?;
                } else if key == PLAYBOOK_ALL {
                    playbook_all = Some(value.as_ref());
                } else if let Some(repo) = key.strip_prefix(playbook_prefix) {
                    if let Some((project_id, _)) = repos.iter().find(|(_, known)| known == repo) {
                        playbook_projects.push((project_id.clone(), value.as_ref()));
                    }
                }
            }
            if playbook_all.is_some() || !playbook_projects.is_empty() {
                crate::coordinators::write_synced_playbooks(
                    self.db,
                    playbook_all,
                    &playbook_projects,
                )?;
            }
            // What was written here is what this computer has now, not an edit made here.
            let (written, _) = self.category_values(prefix)?;
            let baseline = self
                .store
                .value_baselines
                .entry(prefix.to_string())
                .or_default();
            for (key, value) in written {
                if setting_of_sync_key(&key).is_none() {
                    baseline.insert(key, value);
                }
            }
        }
        Ok(patch)
    }
}
