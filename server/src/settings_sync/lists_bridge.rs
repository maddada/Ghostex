//! Accounts, Agents and Skills read for sync and written back from it (rules: gx-core
//! `settings_sync/lists.rs`, CDXC:SettingsSync 2026-11-07).
//!
//! CDXC:SettingsSync 2026-11-07 WHY:
//! Skills install the way Settings > Integrations installs them: a bundled Ghostex skill through `install_bundled_skill` (GitHub main, else the app's copy) and `uninstall_skill`; one of the user's own skills is written into `~/.agents/skills` with `copy_skill_directory_into_place` (staged, then renamed into place), where every agent that reads that folder finds it and Agent Sync links it for the rest. A removed own skill is moved to `<gxserver state>/backups/settings-sync-skills/`, never deleted, and a skill folder that is a link (a pin the user manages) is neither sent nor replaced.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use base64::Engine as _;
use ghostex_gx_core::settings_sync::{
    account_sync_value, agent_roster_values, apply_agent_roster_entry, apply_synced_account,
    category_enabled, skill_file_path_safe, skill_fits, skill_hash, skill_name_syncs,
    ACCOUNT_KEY_PREFIX, AGENT_KEY_PREFIX, BUNDLED_SKILL_KEY_PREFIX, MAX_SKILLS_BYTES,
    MAX_SKILL_FILES, OWN_SKILL_KEY_PREFIX, SYNC_CATEGORIES,
};
use rusqlite::Connection;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::agent_skills::{
    role_only_ghostex_agent_skill, AGENT_SKILL_COPY_EXCLUDED_NAMES, CANONICAL_AGENT_SKILLS_DIR,
    GHOSTEX_AGENT_SKILL_NAMES, MACOS_ONLY_GHOSTEX_AGENT_SKILL_NAMES,
};
use crate::domain::{DomainRepository, DomainStateError};
use crate::server::AppState;

use super::engine::Engine;
use super::store::SyncStore;

/// The categories whose items are read and written here.
pub(super) const LIST_CATEGORIES: [&str; 3] = ["agentRoster", "skills", "accounts"];

/// A bundled skill install that failed is tried again after this long.
const BUNDLED_RETRY_MS: u64 = 10 * 60 * 1000;

impl Engine<'_> {
    /// What this computer has for list category `prefix`, by synced key.
    pub(super) fn list_values(
        &self,
        prefix: &str,
    ) -> Result<BTreeMap<String, Value>, DomainStateError> {
        match prefix {
            "accounts" => self.account_values(),
            "agentRoster" => {
                let projects = DomainRepository::new(self.db, &self.server_id)
                    .list_projects_without_session_history()?;
                let (agents, order) = crate::sidebar_hud::synced_agent_roster(&projects);
                Ok(agent_roster_values(&agents, &order).into_iter().collect())
            }
            "skills" => Ok(skill_values(&self.skills_root())),
            _ => Ok(BTreeMap::new()),
        }
    }

    /// Makes this computer say what `wanted` (key and value, `None` deletes) says for `prefix`.
    pub(super) fn write_list_values(
        &mut self,
        prefix: &str,
        wanted: &[(String, Option<Value>, String)],
    ) -> Result<(), DomainStateError> {
        match prefix {
            "accounts" => self.write_accounts(wanted),
            "agentRoster" => self.write_agent_roster(wanted),
            "skills" => {
                self.write_skills(wanted);
                Ok(())
            }
            _ => Ok(()),
        }
    }

    fn account_values(&self) -> Result<BTreeMap<String, Value>, DomainStateError> {
        let registry = crate::accounts::store::read(self.db)?;
        let snapshot = crate::accounts::runtime::current_snapshot();
        let mut values = BTreeMap::new();
        for account in &registry.accounts {
            let Ok(saved) = serde_json::to_value(account) else {
                continue;
            };
            let key = format!("{ACCOUNT_KEY_PREFIX}{}", account.id);
            let email = snapshot
                .accounts
                .iter()
                .find(|found| {
                    found.provider == account.provider && found.identity == account.identity
                })
                .map(|found| found.email.as_str());
            let prior = self
                .store
                .entries
                .get(&key)
                .and_then(|entry| entry.value.as_ref());
            values.insert(key, account_sync_value(&saved, email, prior));
        }
        Ok(values)
    }

    fn write_accounts(
        &mut self,
        wanted: &[(String, Option<Value>, String)],
    ) -> Result<(), DomainStateError> {
        let mut registry = crate::accounts::store::read(self.db)?;
        let mut accounts: Vec<Value> = registry
            .accounts
            .iter()
            .filter_map(|account| serde_json::to_value(account).ok())
            .collect();
        let mut changed = false;
        for (key, value, from) in wanted {
            let Some(id) = key.strip_prefix(ACCOUNT_KEY_PREFIX) else {
                continue;
            };
            // An account id is its provider and the hash of its identity, so a value can't
            // claim another account's id.
            if let Some(value) = value {
                let provider = value.get("provider").and_then(Value::as_str);
                let identity = value
                    .get("identity")
                    .and_then(Value::as_str)
                    .unwrap_or_default();
                let expected = provider.map(|provider| {
                    format!(
                        "{provider}-{}",
                        hex::encode(Sha256::digest(identity.as_bytes()))
                    )
                });
                if expected.as_deref() != Some(id) {
                    continue;
                }
            }
            changed |= apply_synced_account(&mut accounts, id, value.as_ref(), from);
        }
        if !changed {
            return Ok(());
        }
        let next: Vec<crate::accounts::model::SavedAccount> = accounts
            .into_iter()
            .filter_map(|account| serde_json::from_value(account).ok())
            .collect();
        let ids: HashSet<&str> = next.iter().map(|account| account.id.as_str()).collect();
        // A removed account is no longer anyone's new-session or last-used choice.
        registry
            .new_session_accounts
            .retain(|_, choice| match choice {
                crate::accounts::model::NewSessionAccount::Pinned { id } => {
                    ids.contains(id.as_str())
                }
                _ => true,
            });
        registry
            .last_used_accounts
            .retain(|_, id| ids.contains(id.as_str()));
        registry.accounts = next;
        crate::accounts::store::write(self.db, &registry)?;
        self.state.accounts.invalidate();
        Ok(())
    }

    fn write_agent_roster(
        &mut self,
        wanted: &[(String, Option<Value>, String)],
    ) -> Result<(), DomainStateError> {
        let repository = DomainRepository::new(self.db, &self.server_id);
        let projects = repository.list_projects_without_session_history()?;
        let (mut agents, mut order) = crate::sidebar_hud::synced_agent_roster(&projects);
        let mut changed = false;
        for (key, value, _) in wanted {
            changed |= apply_agent_roster_entry(&mut agents, &mut order, key, value.as_ref());
        }
        if !changed {
            return Ok(());
        }
        // The list lives on every project row; a computer with no project yet takes it once it
        // has one (the next round asks again).
        for update in crate::sidebar_hud::synced_agent_roster_updates(&projects, &agents, &order) {
            let project_id = update
                .get("projectId")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            repository.update_project(&update)?;
            crate::server::schedule_presentation_project_delta(
                self.state,
                self.db,
                &repository,
                &project_id,
                "projectUpdated",
            )?;
        }
        Ok(())
    }

    fn skills_root(&self) -> PathBuf {
        self.state
            .paths
            .agent_config_home_dir()
            .join(CANONICAL_AGENT_SKILLS_DIR)
    }

    fn write_skills(&mut self, wanted: &[(String, Option<Value>, String)]) {
        let root = self.skills_root();
        let home = self.state.paths.agent_config_home_dir().to_path_buf();
        for (key, value, _) in wanted {
            if let Some(name) = key.strip_prefix(BUNDLED_SKILL_KEY_PREFIX) {
                if !bundled_skill_syncs(name) {
                    continue;
                }
                match value {
                    Some(_) => install_bundled_later(self.state, name),
                    None => {
                        let _ = crate::machine_integrations::uninstall_skill(&home, name);
                    }
                }
            } else if let Some(name) = key.strip_prefix(OWN_SKILL_KEY_PREFIX) {
                if !skill_name_syncs(name) || bundled_skill_syncs(name) {
                    continue;
                }
                let destination = root.join(name);
                // A linked skill folder is the user's own pin.
                if fs::symlink_metadata(&destination)
                    .is_ok_and(|metadata| metadata.file_type().is_symlink())
                {
                    continue;
                }
                let _ = match value {
                    Some(value) => write_own_skill(&self.state.paths.root_dir, &destination, value),
                    None => back_up_own_skill(&self.state.paths.root_dir, &destination),
                };
            }
        }
    }
}

/// A bundled skill sync installs and removes: a listed one this system can run, other than the
/// coordinator and GBot playbooks (installed when a coordinator or a GBot chat starts).
fn bundled_skill_syncs(name: &str) -> bool {
    GHOSTEX_AGENT_SKILL_NAMES.contains(&name)
        && !role_only_ghostex_agent_skill(name)
        && (cfg!(target_os = "macos") || !MACOS_ONLY_GHOSTEX_AGENT_SKILL_NAMES.contains(&name))
}

/// Every installed bundled skill and every own skill small enough to sync, by key.
fn skill_values(root: &Path) -> BTreeMap<String, Value> {
    let mut values = BTreeMap::new();
    let Ok(read) = fs::read_dir(root) else {
        return values;
    };
    let mut names: Vec<String> = read
        .flatten()
        .filter_map(|entry| entry.file_name().to_str().map(str::to_string))
        .filter(|name| skill_name_syncs(name))
        .collect();
    names.sort();
    names.dedup();
    let mut total: u64 = 0;
    for name in names {
        let folder = root.join(&name);
        if !folder.join("SKILL.md").is_file() {
            continue;
        }
        if bundled_skill_syncs(&name) {
            values.insert(format!("{BUNDLED_SKILL_KEY_PREFIX}{name}"), json!(true));
            continue;
        }
        if GHOSTEX_AGENT_SKILL_NAMES.contains(&name.as_str())
            || fs::symlink_metadata(&folder).is_ok_and(|meta| meta.file_type().is_symlink())
        {
            continue;
        }
        let Some((value, size)) = own_skill_value(&folder) else {
            continue;
        };
        if total + size > MAX_SKILLS_BYTES {
            continue;
        }
        total += size;
        values.insert(format!("{OWN_SKILL_KEY_PREFIX}{name}"), value);
    }
    values
}

/// The files of one own skill folder (path, size, modified time, executable), or `None` past the
/// limits.
fn skill_listing(folder: &Path) -> Option<Vec<(String, u64, i128, bool)>> {
    let mut files = Vec::new();
    let mut pending = vec![(folder.to_path_buf(), String::new())];
    while let Some((dir, prefix)) = pending.pop() {
        for entry in fs::read_dir(&dir).ok()?.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            if name.starts_with('.') || AGENT_SKILL_COPY_EXCLUDED_NAMES.contains(&name.as_str()) {
                continue;
            }
            let path = if prefix.is_empty() {
                name.clone()
            } else {
                format!("{prefix}/{name}")
            };
            let Ok(metadata) = fs::symlink_metadata(entry.path()) else {
                continue;
            };
            if metadata.is_dir() {
                pending.push((entry.path(), path));
            } else if metadata.is_file() {
                let modified = metadata
                    .modified()
                    .ok()
                    .and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |at| at.as_nanos() as i128);
                files.push((path, metadata.len(), modified, executable(&metadata)));
            }
            if files.len() > MAX_SKILL_FILES {
                return None;
            }
        }
    }
    files.sort();
    let sizes: Vec<(&str, u64)> = files
        .iter()
        .map(|(path, size, _, _)| (path.as_str(), *size))
        .collect();
    skill_fits(&sizes).then_some(files)
}

#[cfg(unix)]
fn executable(metadata: &fs::Metadata) -> bool {
    use std::os::unix::fs::PermissionsExt as _;
    metadata.permissions().mode() & 0o111 != 0
}

#[cfg(not(unix))]
fn executable(_: &fs::Metadata) -> bool {
    false
}

/// One own skill as it syncs (`{hash, files: [{path, data, exec}]}`) and its size, read again only
/// when a file in it changed since the last read.
fn own_skill_value(folder: &Path) -> Option<(Value, u64)> {
    type Cached = (Vec<(String, u64, i128, bool)>, Value, u64);
    static CACHE: OnceLock<Mutex<HashMap<PathBuf, Cached>>> = OnceLock::new();
    let listing = skill_listing(folder)?;
    let cache = CACHE.get_or_init(Mutex::default);
    if let Some((known, value, size)) = cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .get(folder)
    {
        if *known == listing {
            return Some((value.clone(), *size));
        }
    }
    let mut contents = Vec::new();
    for (path, _, _, exec) in &listing {
        contents.push((path.clone(), fs::read(folder.join(path)).ok()?, *exec));
    }
    let size = contents
        .iter()
        .map(|(_, bytes, _)| bytes.len() as u64)
        .sum();
    let hash = skill_hash(
        contents
            .iter()
            .map(|(path, bytes, exec)| (path.as_str(), bytes.as_slice(), *exec)),
    );
    let files: Vec<Value> = contents
        .iter()
        .map(|(path, bytes, exec)| {
            json!({
                "path": path,
                "data": base64::engine::general_purpose::STANDARD.encode(bytes),
                "exec": exec,
            })
        })
        .collect();
    let value = json!({ "hash": hash, "files": files });
    cache
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .insert(folder.to_path_buf(), (listing, value.clone(), size));
    Some((value, size))
}

/// Writes one own skill from its synced value, staged under gxserver's state and renamed into
/// place; a value with an unsafe path or a hash that doesn't match its files is refused whole.
fn write_own_skill(
    state_root: &Path,
    destination: &Path,
    value: &Value,
) -> Result<(), DomainStateError> {
    let refuse = || DomainStateError::bad_request("A synced skill's files don't check out.");
    let files = value
        .get("files")
        .and_then(Value::as_array)
        .ok_or_else(refuse)?;
    let mut decoded: Vec<(String, Vec<u8>, bool)> = Vec::new();
    for file in files {
        let path = file
            .get("path")
            .and_then(Value::as_str)
            .ok_or_else(refuse)?;
        if !skill_file_path_safe(path) {
            return Err(refuse());
        }
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(file.get("data").and_then(Value::as_str).unwrap_or_default())
            .map_err(|_| refuse())?;
        let exec = file.get("exec").and_then(Value::as_bool) == Some(true);
        decoded.push((path.to_string(), bytes, exec));
    }
    let sizes: Vec<(&str, u64)> = decoded
        .iter()
        .map(|(path, bytes, _)| (path.as_str(), bytes.len() as u64))
        .collect();
    let hash = skill_hash(
        decoded
            .iter()
            .map(|(path, bytes, exec)| (path.as_str(), bytes.as_slice(), *exec)),
    );
    if !skill_fits(&sizes) || value.get("hash").and_then(Value::as_str) != Some(hash.as_str()) {
        return Err(refuse());
    }
    let staging = state_root.join("settings-sync-staging").join(format!(
        "{}-{}",
        destination
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
        std::process::id()
    ));
    let _ = fs::remove_dir_all(&staging);
    let written = (|| -> std::io::Result<()> {
        for (path, bytes, exec) in &decoded {
            let target = staging.join(path);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(&target, bytes)?;
            #[cfg(unix)]
            if *exec {
                use std::os::unix::fs::PermissionsExt as _;
                fs::set_permissions(&target, fs::Permissions::from_mode(0o755))?;
            }
            #[cfg(not(unix))]
            let _ = exec;
        }
        Ok(())
    })();
    let result = written
        .map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("Settings sync could not write a skill: {error}"),
        })
        .and_then(|()| {
            crate::agent_skills::copy_skill_directory_into_place(&staging, destination).map(|_| ())
        });
    let _ = fs::remove_dir_all(&staging);
    result
}

/// Moves a skill another computer removed into `<state>/backups/settings-sync-skills/<time>/`.
fn back_up_own_skill(state_root: &Path, folder: &Path) -> Result<(), DomainStateError> {
    if !folder.is_dir() {
        return Ok(());
    }
    let stamp = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let backup = state_root
        .join("backups")
        .join("settings-sync-skills")
        .join(stamp.to_string());
    fs::create_dir_all(&backup)
        .and_then(|()| fs::rename(folder, backup.join(folder.file_name().unwrap_or_default())))
        .map_err(|error| DomainStateError {
            code: "internalError",
            message: format!("Settings sync could not put a removed skill aside: {error}"),
        })
}

/// Installs a bundled skill in the background, once at a time per skill, and not again for a while
/// after it failed.
fn install_bundled_later(state: &AppState, name: &str) {
    static RUNNING: OnceLock<Mutex<HashMap<String, u64>>> = OnceLock::new();
    let running = RUNNING.get_or_init(Mutex::default);
    let now = super::engine::now_ms();
    {
        let mut running = running
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        if running.get(name).is_some_and(|until| *until > now) {
            return;
        }
        // Until it finishes (and, after a failure, for the retry wait).
        running.insert(name.to_string(), u64::MAX);
    }
    let paths = state.paths.clone();
    let name = name.to_string();
    tokio::spawn(async move {
        let result = crate::machine_integrations::install_bundled_skill(&paths, &name).await;
        let mut running = running
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        match result {
            Ok(()) => {
                running.remove(&name);
                super::nudge_settings_sync();
            }
            Err(_) => {
                running.insert(name, super::engine::now_ms() + BUNDLED_RETRY_MS);
            }
        }
    });
}

/// The agents Settings > Agents shows as turned on from another computer: agent id → that
/// computer's name, while this computer syncs Agents.
pub(crate) fn agents_synced_from_elsewhere(
    state: &AppState,
    db: &Connection,
) -> BTreeMap<String, String> {
    let settings = crate::session_lifecycle::read_sidebar_settings(&state.paths);
    let syncs = SYNC_CATEGORIES
        .iter()
        .find(|category| category.prefix == "agentRoster")
        .is_some_and(|category| category_enabled(category, settings.as_ref()));
    if !syncs {
        return BTreeMap::new();
    }
    let Ok(store) = SyncStore::load(db) else {
        return BTreeMap::new();
    };
    store
        .entries
        .range(AGENT_KEY_PREFIX.to_string()..)
        .take_while(|(key, _)| key.starts_with(AGENT_KEY_PREFIX))
        .filter(|(_, entry)| entry.origin != state.metadata.server_id)
        .filter(|(_, entry)| {
            entry
                .value
                .as_ref()
                .is_some_and(|value| value.get("hidden").and_then(Value::as_bool) != Some(true))
        })
        .filter_map(|(key, entry)| {
            Some((
                key.strip_prefix(AGENT_KEY_PREFIX)?.to_string(),
                entry.origin_name.clone(),
            ))
        })
        .collect()
}
