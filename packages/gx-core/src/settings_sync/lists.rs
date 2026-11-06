//! Accounts, Agents and Skills: lists that sync one key per item.
//!
//! Keys: `accounts/account/<id>` (a saved Claude or Codex account: its name, colour, badge, number,
//! Use this account rule and whether its usage is pinned in the sidebar), `agentRoster/agent/<id>`
//! and `agentRoster/order` (Settings > Agents: which agents are on, their name, command with its
//! flags, Accept all and icon, and the list's order), `skills/bundled/<name>` (a bundled Ghostex
//! skill that is installed) and `skills/own/<name>` (one of the user's own skills in
//! `~/.agents/skills`, with its files).
//!
//! CDXC:SettingsSync 2026-11-07 DECISION:
//! User (7 Nov, on Gregg's "agent account copy" request): route it over the Ghostex computer link, not SSH; "i dont think we will have this copy account sheet, let's just do sync for accounts if someone wants this"; Settings > Sync gets an Accounts category, "yes, off by default"; and "in sync i'd like this (sync agents, skills. i dont know about integrations for now maybe later)". Accounts, Agents and Skills are three more Settings > Sync switches, all off by default; integrations stay out.
//!
//! CDXC:SettingsSync 2026-11-07 WHY:
//! An account's login never syncs, only the list: Claude and Codex replace the refresh token on every renewal, so one login used on two computers signs one of them out. An account that arrives where it has no login is saved without a Claude Swap or Codex Swap slot (`synced` says where it came from) and asks to be signed in there; nothing launches or switches to it until then. The account number is each computer's own slot (moving it would move logins between slots), so a computer that has the login keeps its number and only an account still waiting for its sign-in takes the number it arrived with.

use serde_json::{json, Map, Value};

use super::entries::SyncConflict;

pub const ACCOUNT_KEY_PREFIX: &str = "accounts/account/";
pub const AGENT_KEY_PREFIX: &str = "agentRoster/agent/";
pub const AGENT_ORDER_KEY: &str = "agentRoster/order";
pub const BUNDLED_SKILL_KEY_PREFIX: &str = "skills/bundled/";
pub const OWN_SKILL_KEY_PREFIX: &str = "skills/own/";

/// Saved accounts a computer keeps at most (as Settings > Accounts' `register` does).
pub const MAX_SAVED_ACCOUNTS: usize = 50;
/// One of the user's own skills syncs while its files are at most this big together.
pub const MAX_SKILL_BYTES: u64 = 512 * 1024;
pub const MAX_SKILL_FILES: usize = 200;
/// All own skills together; the ones past it (by name) stay on their computer.
pub const MAX_SKILLS_BYTES: u64 = 8 * 1024 * 1024;

// --- Accounts ---------------------------------------------------------------------------------

/// The Use this account rule a saved account (`SavedAccount` as JSON) has: `automatic`, `manual`
/// (Only when I pick it) or `paused`.
fn account_use(saved: &Value) -> &'static str {
    if saved.get("userDisabled").and_then(Value::as_bool) == Some(true) {
        "paused"
    } else if saved.get("eligible").and_then(Value::as_bool) == Some(true) {
        "automatic"
    } else {
        "manual"
    }
}

fn text<'a>(value: &'a Value, key: &str) -> &'a str {
    value.get(key).and_then(Value::as_str).unwrap_or_default()
}

/// What syncs of a saved account. `email` is the login's email as this computer last read it
/// (else what the entry already says, `prior`), so an account that arrives elsewhere can be signed
/// in with it pre-filled. A login gxserver itself turned off after a refusal (`disabled`) keeps
/// the rule the entry has: that refusal belongs to this computer.
pub fn account_sync_value(saved: &Value, email: Option<&str>, prior: Option<&Value>) -> Value {
    let number = match text(saved, "selector") {
        "" => saved
            .pointer("/synced/number")
            .and_then(Value::as_str)
            .unwrap_or_default(),
        selector => selector,
    };
    let email = email
        .filter(|email| !email.is_empty())
        .or_else(|| prior.and_then(|prior| prior.get("email")?.as_str()))
        .or_else(|| saved.pointer("/synced/email").and_then(Value::as_str))
        .unwrap_or_default();
    let refused_here = saved.get("disabled").is_some_and(|value| !value.is_null());
    let rule = match prior.and_then(|prior| prior.get("use")?.as_str()) {
        Some(rule) if refused_here && account_use(saved) == "manual" => rule.to_string(),
        _ => account_use(saved).to_string(),
    };
    json!({
        "provider": text(saved, "provider"),
        "identity": text(saved, "identity"),
        "email": email,
        "name": text(saved, "name"),
        "color": text(saved, "color"),
        "indicator": text(saved, "indicator"),
        "number": number,
        "use": rule,
        "pinned": saved.get("showInTitlebar").and_then(Value::as_bool) == Some(true),
    })
}

/// Makes this computer's saved accounts (`SavedAccount` JSON) say what the synced account `id`
/// says: an account it has takes the name, colour, badge, rule and pin (never its slot or login);
/// one it lacks is added without a login, saying it came from `from`; `None` removes it. Returns
/// whether anything changed. The caller has checked that `id` belongs to the value's identity.
pub fn apply_synced_account(
    accounts: &mut Vec<Value>,
    id: &str,
    value: Option<&Value>,
    from: &str,
) -> bool {
    let index = accounts
        .iter()
        .position(|account| text(account, "id") == id);
    let Some(value) = value else {
        let Some(index) = index else {
            return false;
        };
        accounts.remove(index);
        return true;
    };
    let provider = text(value, "provider");
    if !matches!(provider, "claude" | "codex") || text(value, "identity").is_empty() {
        return false;
    }
    let rule = text(value, "use");
    let index = match index {
        Some(index) => index,
        None => {
            if accounts.len() >= MAX_SAVED_ACCOUNTS {
                return false;
            }
            accounts.push(json!({
                "id": id,
                "provider": provider,
                "selector": "",
                "identity": text(value, "identity"),
                "name": "",
                "color": "neutral",
                "indicator": "",
                "showInTitlebar": false,
                "eligible": false,
                "sharedHistory": true,
                "synced": { "from": from },
            }));
            accounts.len() - 1
        }
    };
    let before = accounts[index].clone();
    let Some(account) = accounts[index].as_object_mut() else {
        return false;
    };
    let refused_here = account
        .get("disabled")
        .is_some_and(|value| !value.is_null());
    let mut set = |key: &str, next: Value| {
        account.insert(key.to_string(), next);
    };
    let name = text(value, "name").trim();
    if !name.is_empty() {
        set("name", json!(name));
    }
    let color = text(value, "color");
    if !color.is_empty() {
        set("color", json!(color));
    }
    set("indicator", json!(text(value, "indicator")));
    set(
        "showInTitlebar",
        json!(value.get("pinned").and_then(Value::as_bool) == Some(true)),
    );
    if matches!(rule, "automatic" | "manual" | "paused") {
        set("userDisabled", json!(rule == "paused"));
        // A login refused here stays out of automatic switching until it is reconnected here.
        if !refused_here {
            set("eligible", json!(rule == "automatic"));
        }
    }
    let waiting = account
        .get("selector")
        .and_then(Value::as_str)
        .is_none_or(str::is_empty);
    if waiting {
        let mut synced = account
            .get("synced")
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_else(|| {
                let mut synced = Map::new();
                synced.insert("from".to_string(), json!(from));
                synced
            });
        synced.insert("number".to_string(), json!(text(value, "number")));
        synced.insert("email".to_string(), json!(text(value, "email")));
        account.insert("synced".to_string(), Value::Object(synced));
    }
    accounts[index] != before
}

// --- Agents -----------------------------------------------------------------------------------

/// The synced keys of Settings > Agents' stored list (`customAgents` as a project row holds it)
/// and its order (`customAgentOrder`).
pub fn agent_roster_values(agents: &[Value], order: &[String]) -> Vec<(String, Value)> {
    let mut values: Vec<(String, Value)> = agents
        .iter()
        .filter_map(|agent| {
            let id = agent.get("agentId")?.as_str()?;
            let mut value = Map::new();
            for key in ["name", "command", "hidden", "icon", "acceptAllMode"] {
                if let Some(field) = agent.get(key).filter(|field| !field.is_null()) {
                    value.insert(key.to_string(), field.clone());
                }
            }
            Some((format!("{AGENT_KEY_PREFIX}{id}"), Value::Object(value)))
        })
        .collect();
    if !order.is_empty() {
        values.push((AGENT_ORDER_KEY.to_string(), json!(order)));
    }
    values
}

/// Applies one synced agent key to the stored list and order; returns whether anything changed.
pub fn apply_agent_roster_entry(
    agents: &mut Vec<Value>,
    order: &mut Vec<String>,
    key: &str,
    value: Option<&Value>,
) -> bool {
    if key == AGENT_ORDER_KEY {
        let next: Vec<String> = value
            .and_then(Value::as_array)
            .map(|ids| {
                ids.iter()
                    .filter_map(|id| id.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        if *order == next {
            return false;
        }
        *order = next;
        return true;
    }
    let Some(id) = key
        .strip_prefix(AGENT_KEY_PREFIX)
        .filter(|id| !id.is_empty())
    else {
        return false;
    };
    let index = agents
        .iter()
        .position(|agent| agent.get("agentId").and_then(Value::as_str) == Some(id));
    let Some(value) = value.and_then(Value::as_object) else {
        return index.map(|index| agents.remove(index)).is_some();
    };
    let mut next = value.clone();
    next.insert("agentId".to_string(), json!(id));
    let next = Value::Object(next);
    match index {
        Some(index) if same_agent(&agents[index], &next) => false,
        Some(index) => {
            agents[index] = next;
            true
        }
        None => {
            agents.push(next);
            true
        }
    }
}

/// The stored agent says what `next` says (the stored row also carries `isDefault`).
fn same_agent(stored: &Value, next: &Value) -> bool {
    ["name", "command", "hidden", "icon", "acceptAllMode"]
        .iter()
        .all(|key| stored.get(*key).filter(|v| !v.is_null()) == next.get(*key))
}

// --- Skills -----------------------------------------------------------------------------------

/// A skill folder name that may sync: letters, digits, dots, dashes and underscores, not hidden.
pub fn skill_name_syncs(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 128
        && !name.starts_with('.')
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// A file path inside a skill that may be written: relative, `/`-separated, no `..`, no hidden
/// or empty part.
pub fn skill_file_path_safe(path: &str) -> bool {
    !path.is_empty()
        && path.len() <= 512
        && !path.starts_with('/')
        && !path.contains('\\')
        && !path.contains(':')
        && path
            .split('/')
            .all(|part| !part.is_empty() && part != "." && part != ".." && !part.starts_with('.'))
}

/// A short fingerprint of a skill's files (FNV-1a over path, mode and bytes, by path), so two
/// computers see the same folder as the same version.
pub fn skill_hash<'a>(files: impl IntoIterator<Item = (&'a str, &'a [u8], bool)>) -> String {
    let mut files: Vec<(&str, &[u8], bool)> = files.into_iter().collect();
    files.sort_by(|left, right| left.0.cmp(right.0));
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for (path, bytes, executable) in files {
        let mode: &[u8] = if executable { b"x" } else { b"-" };
        for byte in path
            .bytes()
            .chain([0])
            .chain(mode.iter().copied())
            .chain(bytes.iter().copied())
            .chain([0])
        {
            hash ^= u64::from(byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    format!("{hash:016x}")
}

/// Whether a skill with these files (path, size) can sync on its own.
pub fn skill_fits(files: &[(&str, u64)]) -> bool {
    !files.is_empty()
        && files.len() <= MAX_SKILL_FILES
        && files.iter().any(|(path, _)| *path == "SKILL.md")
        && files.iter().all(|(path, _)| skill_file_path_safe(path))
        && files.iter().map(|(_, size)| size).sum::<u64>() <= MAX_SKILL_BYTES
}

// --- Notes ------------------------------------------------------------------------------------

/// One plain sentence about a conflict on an account, an agent or a skill.
pub fn describe_list_key(conflict: &SyncConflict) -> Option<String> {
    let kept = &conflict.kept;
    let lost = &conflict.lost;
    let key = kept.key.as_str();
    let name = |fallback: &str| {
        kept.value
            .as_ref()
            .or(lost.value.as_ref())
            .and_then(|value| value.get("name")?.as_str().map(str::to_string))
            .filter(|name| !name.is_empty())
            .unwrap_or_else(|| fallback.to_string())
    };
    let what = if key.starts_with(ACCOUNT_KEY_PREFIX) {
        format!("The account {}", name("account"))
    } else if let Some(id) = key.strip_prefix(AGENT_KEY_PREFIX) {
        format!("The agent {}", name(id))
    } else if key == AGENT_ORDER_KEY {
        "The order of your agents".to_string()
    } else if let Some(skill) = key
        .strip_prefix(OWN_SKILL_KEY_PREFIX)
        .or_else(|| key.strip_prefix(BUNDLED_SKILL_KEY_PREFIX))
    {
        format!("The skill {skill}")
    } else {
        return None;
    };
    Some(match &kept.value {
        Some(_) => format!(
            "{what} is the edit from {}, replacing the edit from {}.",
            kept.origin_name, lost.origin_name
        ),
        None => format!(
            "{what} was removed on {}, replacing the edit from {}.",
            kept.origin_name, lost.origin_name
        ),
    })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::*;
    use crate::settings_sync::{local_edit, merge_entries, SyncEntry};

    fn saved(selector: &str) -> Value {
        json!({
            "id": "claude-abc",
            "provider": "claude",
            "selector": selector,
            "identity": "me@example.com:org-1",
            "name": "Work",
            "color": "blue",
            "indicator": "W",
            "showInTitlebar": true,
            "eligible": false,
            "sharedHistory": true,
            "userDisabled": false,
        })
    }

    #[test]
    fn an_account_syncs_its_list_details_and_never_its_login() {
        let value = account_sync_value(&saved("3"), Some("me@example.com"), None);
        assert_eq!(value["name"], "Work");
        assert_eq!(value["number"], "3");
        assert_eq!(value["use"], "manual");
        assert_eq!(value["pinned"], true);
        assert_eq!(value["email"], "me@example.com");
        let keys: Vec<&String> = value.as_object().unwrap().keys().collect();
        for secret in [
            "token",
            "refreshToken",
            "accessToken",
            "credentials",
            "selector",
        ] {
            assert!(
                !keys.iter().any(|key| key.as_str() == secret),
                "{secret} synced"
            );
        }
    }

    #[test]
    fn an_account_arrives_without_a_login_and_keeps_its_number_to_sign_in_with() {
        let mut accounts = Vec::new();
        let value = account_sync_value(&saved("3"), Some("me@example.com"), None);
        assert!(apply_synced_account(
            &mut accounts,
            "claude-abc",
            Some(&value),
            "Studio Mac"
        ));
        let arrived = accounts[0].clone();
        assert_eq!(arrived["selector"], "", "no Claude Swap slot here");
        assert_eq!(arrived["eligible"], false, "never picked automatically");
        assert_eq!(arrived["synced"]["from"], "Studio Mac");
        assert_eq!(arrived["synced"]["number"], "3");
        assert_eq!(arrived["synced"]["email"], "me@example.com");
        assert_eq!(arrived["name"], "Work");
        assert_eq!(arrived["showInTitlebar"], true);
        // Applying the same value again changes nothing.
        assert!(!apply_synced_account(
            &mut accounts,
            "claude-abc",
            Some(&value),
            "Studio Mac"
        ));
        // What it syncs back is what it got, so it never echoes a new number.
        assert_eq!(
            account_sync_value(&arrived, None, Some(&value))["number"],
            "3"
        );
    }

    #[test]
    fn an_account_with_a_login_here_keeps_its_own_slot() {
        let mut accounts = vec![saved("1")];
        let mut value = account_sync_value(&saved("4"), None, None);
        value["name"] = json!("Personal");
        value["use"] = json!("paused");
        assert!(apply_synced_account(
            &mut accounts,
            "claude-abc",
            Some(&value),
            "Laptop"
        ));
        assert_eq!(accounts[0]["selector"], "1");
        assert_eq!(accounts[0]["name"], "Personal");
        assert_eq!(accounts[0]["userDisabled"], true);
        assert!(accounts[0].get("synced").is_none());
        assert!(apply_synced_account(
            &mut accounts,
            "claude-abc",
            None,
            "Laptop"
        ));
        assert!(accounts.is_empty());
    }

    #[test]
    fn a_login_refused_here_is_not_switched_to_automatically_by_sync() {
        let mut refused = saved("2");
        refused["disabled"] = json!({ "reason": "No subscription", "at": "x" });
        let mut accounts = vec![refused.clone()];
        let mut value = account_sync_value(&saved("2"), None, None);
        value["use"] = json!("automatic");
        apply_synced_account(&mut accounts, "claude-abc", Some(&value), "Laptop");
        assert_eq!(accounts[0]["eligible"], false);
        // Nor does that refusal turn the account to Only when I pick it elsewhere.
        assert_eq!(
            account_sync_value(&refused, None, Some(&value))["use"],
            "automatic"
        );
    }

    #[test]
    fn agents_sync_on_off_commands_and_order() {
        let stored = vec![
            json!({"agentId": "claude", "name": "Claude", "command": "claude --verbose", "hidden": false, "isDefault": true}),
            json!({"agentId": "custom-x", "name": "X", "command": "x run", "hidden": true, "isDefault": false}),
        ];
        let order = vec!["custom-x".to_string(), "claude".to_string()];
        let values = agent_roster_values(&stored, &order);
        assert_eq!(values.len(), 3);
        let mut agents = Vec::new();
        let mut next_order = Vec::new();
        for (key, value) in &values {
            assert!(apply_agent_roster_entry(
                &mut agents,
                &mut next_order,
                key,
                Some(value)
            ));
        }
        assert_eq!(next_order, order);
        assert_eq!(agents[0]["command"], "claude --verbose");
        assert_eq!(agents[1]["hidden"], true);
        // The stored rows (with `isDefault`) read as unchanged.
        let mut stored_again = stored.clone();
        for (key, value) in &values {
            assert!(!apply_agent_roster_entry(
                &mut stored_again,
                &mut next_order,
                key,
                Some(value)
            ));
        }
        assert!(apply_agent_roster_entry(
            &mut agents,
            &mut next_order,
            "agentRoster/agent/custom-x",
            None
        ));
        assert_eq!(agents.len(), 1);
    }

    #[test]
    fn skill_paths_and_sizes_are_checked() {
        assert!(skill_name_syncs("my-skill_2.0"));
        for bad in ["", ".hidden", "a/b", "..", "a b"] {
            assert!(!skill_name_syncs(bad), "{bad}");
        }
        assert!(skill_file_path_safe("SKILL.md"));
        assert!(skill_file_path_safe("scripts/run.sh"));
        for bad in [
            "/etc/passwd",
            "../x",
            "a/../b",
            "a//b",
            ".git/config",
            "C:\\x",
            "a\\b",
        ] {
            assert!(!skill_file_path_safe(bad), "{bad}");
        }
        assert!(skill_fits(&[("SKILL.md", 10), ("refs/a.md", 20)]));
        assert!(!skill_fits(&[("refs/a.md", 20)]), "needs SKILL.md");
        assert!(!skill_fits(&[("SKILL.md", MAX_SKILL_BYTES + 1)]));
        let one = skill_hash([("SKILL.md", b"a".as_slice(), false)]);
        let same = skill_hash([("SKILL.md", b"a".as_slice(), false)]);
        let exec = skill_hash([("SKILL.md", b"a".as_slice(), true)]);
        assert_eq!(one, same);
        assert_ne!(one, exec);
    }

    #[test]
    fn a_skill_edited_on_two_computers_apart_keeps_the_later_edit_with_a_note() {
        let key = format!("{OWN_SKILL_KEY_PREFIX}notes");
        let store = BTreeMap::new();
        let older = local_edit(
            &store,
            &key,
            Some(json!({"hash": "1"})),
            "0000000000001-000000-a".into(),
            "a",
            "Laptop",
        )
        .unwrap();
        let newer = local_edit(
            &store,
            &key,
            Some(json!({"hash": "2"})),
            "0000000000002-000000-b".into(),
            "b",
            "Studio Mac",
        )
        .unwrap();
        let mut here: BTreeMap<String, SyncEntry> = BTreeMap::new();
        here.insert(key.clone(), older);
        let outcome = merge_entries(&mut here, [newer]);
        assert_eq!(here[&key].value, Some(json!({"hash": "2"})));
        let text = describe_list_key(&outcome.conflicts[0]).unwrap();
        assert_eq!(
            text,
            "The skill notes is the edit from Studio Mac, replacing the edit from Laptop."
        );
    }
}
