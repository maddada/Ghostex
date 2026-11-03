use super::{
    codex_identity::codex_account_part,
    endpoint::{account_id, new_saved_account},
    model::{DiscoveredAccount, NewSessionAccount, Provider, Registry, SavedAccount},
};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::Value;
use std::collections::HashSet;

/// Saved data that names accounts by id outside the registry: the reset warnings already shown (reset_watch.rs).
const ACCOUNT_KEYED_METADATA: &[&str] = &["agents.accounts.resetWarnings.v1"];

/// CDXC:AgentProviders 2026-10-26 WHY:
/// Codex accounts saved before members were told apart (see codex_identity.rs) carry the workspace-only identity, and their id is its hash. Once Codex Swap reports the member (`userId`), each saved account moves to its new identity and id with its name, color, indicator, switches and place in the list, and every reference follows: session bindings (`runtimeSettings.accountId` and the switch progress), the pinned new-session account, the Same as last session account and the reset warnings already shown. When one old account now matches several logins (members of one Business workspace), the slot the saved account launches (`selector`) keeps everything, else the lowest slot; each other login is added fresh, as Settings > Accounts adds one, when it is signed in and shares conversations. Rows Codex Swap lists without a member keep the old identity and are left alone. It runs on every registry read, so a later Codex Swap update that starts reporting `userId` migrates the same way.
pub(super) fn apply(db: &Connection, registry: &mut Registry) {
    if !registry.accounts.iter().any(is_legacy) {
        return;
    }
    let snapshot = super::runtime::current_snapshot();
    let mut next = registry.clone();
    let remaps = plan(&mut next, &snapshot.accounts);
    if remaps.is_empty() {
        return;
    }
    // A failed write leaves the registry as it was; the next read tries again.
    if persist(db, &next, &remaps).is_ok() {
        *registry = next;
    }
}

/// Moves legacy Codex accounts in `registry` to the identities `discovered` reports and returns each `(old id, new id)`.
pub(super) fn plan(
    registry: &mut Registry,
    discovered: &[DiscoveredAccount],
) -> Vec<(String, String)> {
    if !registry.accounts.iter().any(is_legacy) {
        return Vec::new();
    }
    let saved_ids: HashSet<String> = registry.accounts.iter().map(|a| a.id.clone()).collect();
    let mut remaps = Vec::new();
    let mut accounts = Vec::with_capacity(registry.accounts.len());
    let total = registry.accounts.len();
    for (index, saved) in std::mem::take(&mut registry.accounts)
        .into_iter()
        .enumerate()
    {
        if !is_legacy(&saved) {
            accounts.push(saved);
            continue;
        }
        let mut members: Vec<&DiscoveredAccount> = discovered
            .iter()
            .filter(|found| {
                found.provider == Provider::Codex
                    && found.identity.contains(':')
                    && codex_account_part(&found.identity) == saved.identity
            })
            .collect();
        if members.is_empty() {
            accounts.push(saved);
            continue;
        }
        members.sort_by_key(|found| {
            (
                found.selector != saved.selector,
                found.selector.parse::<u64>().unwrap_or(u64::MAX),
                found.selector.clone(),
            )
        });
        let keeper = members[0];
        let new_id = account_id(keeper);
        remaps.push((saved.id.clone(), new_id.clone()));
        if !saved_ids.contains(&new_id) {
            accounts.push(SavedAccount {
                id: new_id,
                identity: keeper.identity.clone(),
                selector: keeper.selector.clone(),
                ..saved
            });
        }
        for other in &members[1..] {
            let id = account_id(other);
            let remaining = total - index - 1;
            if saved_ids.contains(&id)
                || accounts.iter().any(|a: &SavedAccount| a.id == id)
                || other.status != "ready"
                || !other.shared_history
                || accounts.len() + remaining >= 50
            {
                continue;
            }
            accounts.push(new_saved_account(other));
        }
    }
    registry.accounts = accounts;
    for (old, new) in &remaps {
        for rule in registry.new_session_accounts.values_mut() {
            if let NewSessionAccount::Pinned { id } = rule {
                if id == old {
                    *id = new.clone();
                }
            }
        }
        for id in registry.last_used_accounts.values_mut() {
            if id == old {
                *id = new.clone();
            }
        }
    }
    remaps
}

fn is_legacy(account: &SavedAccount) -> bool {
    account.provider == Provider::Codex
        && !account.identity.is_empty()
        && !account.identity.contains(':')
}

fn persist(
    db: &Connection,
    registry: &Registry,
    remaps: &[(String, String)],
) -> Result<(), String> {
    db.execute_batch("SAVEPOINT codex_identity_migration")
        .map_err(|e| e.to_string())?;
    let result = (|| {
        super::store::write(db, registry).map_err(|e| e.message)?;
        remap_sessions(db, remaps)?;
        remap_metadata(db, remaps)
    })();
    let end = if result.is_ok() {
        "RELEASE codex_identity_migration"
    } else {
        "ROLLBACK TO codex_identity_migration; RELEASE codex_identity_migration"
    };
    db.execute_batch(end).map_err(|e| e.to_string())?;
    result
}

fn remap_sessions(db: &Connection, remaps: &[(String, String)]) -> Result<(), String> {
    for (old, new) in remaps {
        let rows = {
            let mut statement = db
                .prepare(
                    "SELECT projectId, sessionId, runtimeSettingsJson FROM sessions WHERE instr(runtimeSettingsJson, ?1) > 0",
                )
                .map_err(|e| e.to_string())?;
            let rows = statement
                .query_map([old], |row| {
                    Ok((
                        row.get::<_, String>(0)?,
                        row.get::<_, String>(1)?,
                        row.get::<_, String>(2)?,
                    ))
                })
                .map_err(|e| e.to_string())?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|e| e.to_string())?;
            rows
        };
        for (project_id, session_id, raw) in rows {
            let Ok(mut runtime) = serde_json::from_str::<Value>(&raw) else {
                continue;
            };
            if replace_id(&mut runtime, old, new) {
                db.execute(
                    "UPDATE sessions SET runtimeSettingsJson = ?1 WHERE projectId = ?2 AND sessionId = ?3",
                    params![runtime.to_string(), project_id, session_id],
                )
                .map_err(|e| e.to_string())?;
            }
        }
    }
    Ok(())
}

fn remap_metadata(db: &Connection, remaps: &[(String, String)]) -> Result<(), String> {
    for key in ACCOUNT_KEYED_METADATA {
        let Some(raw) = db
            .query_row("SELECT value FROM metadata WHERE key=?1", [key], |r| {
                r.get::<_, String>(0)
            })
            .optional()
            .map_err(|e| e.to_string())?
        else {
            continue;
        };
        let Ok(mut value) = serde_json::from_str::<Value>(&raw) else {
            continue;
        };
        let changed = remaps.iter().fold(false, |changed, (old, new)| {
            replace_id(&mut value, old, new) || changed
        });
        if changed {
            db.execute(
                "UPDATE metadata SET value=?1, updatedAt=?2 WHERE key=?3",
                params![value.to_string(), chrono::Utc::now().to_rfc3339(), key],
            )
            .map_err(|e| e.to_string())?;
        }
    }
    Ok(())
}

/// Replaces every string equal to `old` anywhere in `value`. Account ids are a provider prefix and a SHA-256, so no other value can equal one.
fn replace_id(value: &mut Value, old: &str, new: &str) -> bool {
    match value {
        Value::String(text) if text == old => {
            *text = new.to_string();
            true
        }
        Value::Array(items) => replace_all(items.iter_mut(), old, new),
        Value::Object(map) => replace_all(map.values_mut(), old, new),
        _ => false,
    }
}

fn replace_all<'a>(items: impl Iterator<Item = &'a mut Value>, old: &str, new: &str) -> bool {
    let mut changed = false;
    for item in items {
        changed |= replace_id(item, old, new);
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::accounts::codex_identity::codex_identity;
    use serde_json::json;

    fn found(selector: &str, account: &str, user: Option<&str>, name: &str) -> DiscoveredAccount {
        DiscoveredAccount {
            provider: Provider::Codex,
            selector: selector.into(),
            identity: codex_identity(account, user),
            name: name.into(),
            email: name.into(),
            status: "ready".into(),
            shared_history: true,
            usage: vec![],
            reset_credits: None,
            reset_credit_details: None,
            reset_credits_error: None,
            usage_updated_at: None,
            usage_error: None,
            usage_stale: false,
        }
    }

    fn saved(found: &DiscoveredAccount, name: &str) -> SavedAccount {
        SavedAccount {
            id: account_id(found),
            provider: found.provider,
            selector: found.selector.clone(),
            identity: found.identity.clone(),
            name: name.into(),
            color: "coral".into(),
            indicator: "W".into(),
            show_in_titlebar: true,
            eligible: false,
            shared_history: true,
            disabled: None,
            user_disabled: false,
            login_expired_at: None,
            login_confirmed_at: None,
        }
    }

    fn claude() -> SavedAccount {
        SavedAccount {
            id: "claude-abc".into(),
            provider: Provider::Claude,
            selector: "1".into(),
            identity: "a@b.c:org".into(),
            name: "Claude".into(),
            color: "sky".into(),
            indicator: String::new(),
            show_in_titlebar: false,
            eligible: true,
            shared_history: true,
            disabled: None,
            user_disabled: false,
            login_expired_at: None,
            login_confirmed_at: None,
        }
    }

    fn database() -> Connection {
        let db = Connection::open_in_memory().unwrap();
        db.execute_batch(
            "CREATE TABLE metadata (key TEXT PRIMARY KEY, value TEXT NOT NULL, updatedAt TEXT NOT NULL);
             CREATE TABLE sessions (projectId TEXT NOT NULL, sessionId TEXT NOT NULL, runtimeSettingsJson TEXT);",
        )
        .unwrap();
        db
    }

    /// A single-user Codex account saved under its workspace-only identity keeps its name, settings, place, pinned and last-used references, and session bindings under the new id.
    #[test]
    fn a_saved_account_keeps_its_settings_and_sessions_under_the_new_id() {
        let old_personal = found("1", "0e1e7f6e", None, "me@x");
        let old_work = found("2", "da6509fe", None, "work@x");
        let mut registry = Registry {
            accounts: vec![
                claude(),
                saved(&old_personal, "Personal"),
                saved(&old_work, "Work"),
            ],
            ..Default::default()
        };
        registry.new_session_accounts.insert(
            Provider::Codex,
            NewSessionAccount::Pinned {
                id: account_id(&old_work),
            },
        );
        registry
            .last_used_accounts
            .insert(Provider::Codex, account_id(&old_personal));
        registry
            .last_used_accounts
            .insert(Provider::Claude, "claude-abc".into());
        let db = database();
        db.execute(
            "INSERT INTO sessions VALUES ('p', 's1', ?1), ('p', 's2', '{\"accountId\":\"claude-abc\"}')",
            [json!({
                "accountId": account_id(&old_work),
                "accountSwitch": {"fromAccountId": account_id(&old_personal), "toAccountId": account_id(&old_work)}
            })
            .to_string()],
        )
        .unwrap();
        db.execute(
            "INSERT INTO metadata VALUES ('agents.accounts.resetWarnings.v1', ?1, 'then')",
            [json!([{"accountId": account_id(&old_work), "creditId": "c"}]).to_string()],
        )
        .unwrap();
        let personal = found("1", "0e1e7f6e", Some("user-Bp8i"), "me@x");
        let work = found("2", "da6509fe", Some("user-3mAa"), "work@x");
        let discovered = vec![personal.clone(), work.clone()];

        let mut next = registry.clone();
        let remaps = plan(&mut next, &discovered);
        persist(&db, &next, &remaps).unwrap();

        assert_eq!(remaps.len(), 2);
        let ids: Vec<_> = next.accounts.iter().map(|a| a.id.clone()).collect();
        assert_eq!(
            ids,
            [
                "claude-abc".to_string(),
                account_id(&personal),
                account_id(&work)
            ]
        );
        let moved = &next.accounts[2];
        assert_eq!(moved.name, "Work");
        assert_eq!(moved.identity, "da6509fe:user-3mAa");
        assert_eq!(
            (moved.color.as_str(), moved.indicator.as_str()),
            ("coral", "W")
        );
        assert!(moved.show_in_titlebar && !moved.eligible);
        assert_eq!(
            next.new_session_accounts[&Provider::Codex],
            NewSessionAccount::Pinned {
                id: account_id(&work)
            }
        );
        assert_eq!(
            next.last_used_accounts[&Provider::Codex],
            account_id(&personal)
        );
        assert_eq!(next.last_used_accounts[&Provider::Claude], "claude-abc");

        let stored = super::super::store::read(&db).unwrap();
        assert_eq!(stored.accounts.len(), 3);
        let runtime: Value = serde_json::from_str(
            &db.query_row(
                "SELECT runtimeSettingsJson FROM sessions WHERE sessionId='s1'",
                [],
                |r| r.get::<_, String>(0),
            )
            .unwrap(),
        )
        .unwrap();
        assert_eq!(runtime["accountId"], json!(account_id(&work)));
        assert_eq!(
            runtime["accountSwitch"]["fromAccountId"],
            json!(account_id(&personal))
        );
        let untouched: String = db
            .query_row(
                "SELECT runtimeSettingsJson FROM sessions WHERE sessionId='s2'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(untouched, "{\"accountId\":\"claude-abc\"}");
        let warnings: String = db
            .query_row(
                "SELECT value FROM metadata WHERE key='agents.accounts.resetWarnings.v1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert!(warnings.contains(&account_id(&work)));

        // Idempotent: a second pass finds nothing left to move.
        let mut again = next.clone();
        assert!(plan(&mut again, &discovered).is_empty());
    }

    /// The reporter's slots (#312): slots 1 and 3 are two members of one Business workspace and were saved as one account (selector 1); slot 2 is the first member's personal account.
    #[test]
    fn a_shared_workspace_account_splits_and_the_saved_slot_keeps_its_data() {
        let merged = found("1", "89b7ba55", None, "me@team");
        let plus_old = found("2", "a29cdd25", None, "me@plus");
        let mut registry = Registry {
            accounts: vec![saved(&merged, "PA Business"), saved(&plus_old, "Plus")],
            ..Default::default()
        };
        registry
            .last_used_accounts
            .insert(Provider::Codex, account_id(&merged));
        let team_first = found("1", "89b7ba55", Some("user-wgOd"), "me@team");
        let plus = found("2", "a29cdd25", Some("user-wgOd"), "me@plus");
        let team_second = found("3", "89b7ba55", Some("user-2o3K"), "wife@team");
        let discovered = vec![team_first.clone(), plus.clone(), team_second.clone()];

        let remaps = plan(&mut registry, &discovered);

        assert_eq!(
            remaps,
            vec![
                (account_id(&merged), account_id(&team_first)),
                (account_id(&plus_old), account_id(&plus)),
            ]
        );
        let names: Vec<_> = registry
            .accounts
            .iter()
            .map(|a| (a.selector.as_str(), a.name.as_str(), a.color.as_str()))
            .collect();
        assert_eq!(
            names,
            [
                ("1", "PA Business", "coral"),
                ("3", "wife@team", "neutral"),
                ("2", "Plus", "coral")
            ]
        );
        let ids: HashSet<_> = registry.accounts.iter().map(|a| a.id.clone()).collect();
        assert_eq!(ids.len(), 3);
        assert!(registry.accounts[1].eligible && registry.accounts[1].show_in_titlebar);
        assert_eq!(
            registry.last_used_accounts[&Provider::Codex],
            account_id(&team_first)
        );
    }

    /// When the saved slot no longer holds a member of the workspace, the lowest slot keeps the data.
    #[test]
    fn without_the_saved_slot_the_lowest_slot_keeps_the_data() {
        let merged = found("5", "89b7ba55", None, "me@team");
        let mut registry = Registry {
            accounts: vec![saved(&merged, "Team")],
            ..Default::default()
        };
        let low = found("3", "89b7ba55", Some("user-b"), "b@team");
        let high = found("4", "89b7ba55", Some("user-a"), "a@team");
        let remaps = plan(&mut registry, &[high.clone(), low.clone()]);
        assert_eq!(remaps, vec![(account_id(&merged), account_id(&low))]);
        assert_eq!(registry.accounts[0].name, "Team");
        assert_eq!(registry.accounts[0].selector, "3");
        assert_eq!(registry.accounts[1].selector, "4");
    }

    /// Codex Swap before 0.3.2 lists no `userId`: nothing moves.
    #[test]
    fn rows_without_a_member_leave_the_registry_alone() {
        let old = found("1", "0e1e7f6e", None, "me@x");
        let mut registry = Registry {
            accounts: vec![saved(&old, "Personal"), claude()],
            ..Default::default()
        };
        let before = serde_json::to_string(&registry).unwrap();
        assert!(plan(&mut registry, std::slice::from_ref(&old)).is_empty());
        assert!(plan(&mut registry, &[]).is_empty());
        assert_eq!(serde_json::to_string(&registry).unwrap(), before);
    }

    /// An account the user already added under its new identity wins; the legacy entry's references move to it.
    #[test]
    fn an_already_registered_new_identity_keeps_its_own_settings() {
        let old = found("1", "0e1e7f6e", None, "me@x");
        let new = found("1", "0e1e7f6e", Some("user-1"), "me@x");
        let mut registry = Registry {
            accounts: vec![saved(&old, "Old"), saved(&new, "New")],
            ..Default::default()
        };
        let remaps = plan(&mut registry, std::slice::from_ref(&new));
        assert_eq!(remaps, vec![(account_id(&old), account_id(&new))]);
        assert_eq!(registry.accounts.len(), 1);
        assert_eq!(registry.accounts[0].name, "New");
    }
}
