//! The categories that sync values rather than documents: Appearance, Agent defaults, Agents,
//! Skills, Accounts, Sounds and notifications, Keybindings and Coordinator (which keys: gx-core
//! `settings_sync/settings_keys.rs` and `settings_sync/lists.rs`; the lists' reads and writes:
//! lists_bridge.rs).
//!
//! CDXC:SettingsSync 2026-11-06 WHY:
//! Settings-file values reach this computer through the running desktop app's `updateSettingsPatch`, like every settings write (CDXC:Settings 2026-09-09 in ghostex_cli/settings.rs), so the app normalizes and applies them at once; while the app is closed they wait for the next round. Turning a category on (or its first sync) takes this computer's own values in with `seed_hlc`, older than any real edit and without the ones still at their default, so it never overrides what another computer already synced; it only fills in what nobody did.

use std::collections::{BTreeMap, BTreeSet};

use ghostex_gx_core::settings_sync::{
    category_setting_values, key_category, seed_hlc, setting_of_sync_key, SyncEntry,
};
use serde_json::{Map, Value};

use crate::domain::DomainStateError;

use super::engine::Engine;
use super::lists_bridge::LIST_CATEGORIES;
use super::spaces_bridge::project_repo_keys;

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
const PLAYBOOK_PROJECT_PREFIX: &str = "coordinator/playbook/project/";
const PLAYBOOK_OVERRIDES: &str = "coordinator/playbook/overrides";

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
                let repos = project_repo_keys(self.db, &self.server_id, &self.store.entries)?;
                for (project_id, copy) in projects {
                    if let Some((_, repo)) = repos.iter().find(|(id, _)| *id == project_id) {
                        values.insert(format!("{PLAYBOOK_PROJECT_PREFIX}{repo}"), copy);
                    }
                }
            }
            prefix if LIST_CATEGORIES.contains(&prefix) => values.extend(self.list_values(prefix)?),
            _ => {}
        }
        Ok((values, defaults.into_iter().collect()))
    }

    /// Takes what changed in each value category since the last sync into the entries.
    pub(super) fn export_values(&mut self) -> Result<(), DomainStateError> {
        for prefix in VALUE_CATEGORIES {
            if !self.enabled(&format!("{prefix}/")) {
                // Turned off: the next time it is on, this computer's values seed again.
                self.store.value_baselines.remove(prefix);
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
            if !self.enabled(&format!("{prefix}/")) {
                continue;
            }
            let (current, _) = self.category_values(prefix)?;
            let wanted: Vec<(String, Option<Value>, String)> = self
                .store
                .entries
                .values()
                .filter(|entry| key_category(&entry.key) == prefix)
                .filter(|entry| current.get(&entry.key) != entry.value.as_ref())
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
            let repos = project_repo_keys(self.db, &self.server_id, &self.store.entries)?;
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
                } else if let Some(repo) = key.strip_prefix(PLAYBOOK_PROJECT_PREFIX) {
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

#[cfg(test)]
mod instruction_tests {
    #[test]
    fn playbooks_skills_and_agent_commands_carry_instructions() {
        for key in [
            "coordinator/playbook/all",
            "coordinator/playbook/project/github.com/a/b",
            "skills/own/review",
            "agentRoster/agent/claude",
        ] {
            assert!(super::carries_agent_instructions(key), "{key}");
        }
        for key in [
            "appearance/theme",
            "skills/bundled/ghostex-help",
            "agentRoster/order",
        ] {
            assert!(!super::carries_agent_instructions(key), "{key}");
        }
    }
}
