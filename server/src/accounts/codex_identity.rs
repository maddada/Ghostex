use super::model::Provider;
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use serde_json::Value;

/// CDXC:AgentProviders 2026-10-26 WHY:
/// A Codex login is the ChatGPT account (workspace) plus the member signed in to it. For a ChatGPT Business login `accountId` is the workspace, so two people in one workspace had one identity, one hashed Ghostex account id, one name and one set of settings (GitHub #227, #312). The identity is `accountId:userId` whenever the user is known; codex-swap before 0.3.2 does not report `userId`, and those rows keep the workspace-only identity they always had. `codex_identity_migration.rs` moves saved accounts from the old identity to the new one.
/// SEE-ALSO: helpers.rs (`discover_row`, `codex_request`), launch.rs (`validate_identity`), session_identity.rs (`login_identity`), restart_verification.rs, codex_identity_migration.rs.
pub(crate) fn codex_identity(account_id: &str, user_id: Option<&str>) -> String {
    match usable(user_id) {
        Some(user) if !account_id.is_empty() => format!("{account_id}:{user}"),
        _ => account_id.to_string(),
    }
}

/// The ChatGPT account id of a Codex identity: the part before `:userId`, or the whole legacy identity.
pub(crate) fn codex_account_part(identity: &str) -> &str {
    identity
        .split_once(':')
        .map_or(identity, |(account, _)| account)
}

/// The Codex identity of an `auth.json` document: `tokens.account_id` plus the id token's member claim (`chatgpt_user_id`, else `user_id`, as codex-swap reads it).
pub(crate) fn codex_auth_identity(auth: &Value) -> Option<String> {
    let account_id = auth
        .pointer("/tokens/account_id")?
        .as_str()
        .filter(|id| !id.is_empty())?;
    Some(codex_identity(account_id, codex_auth_user(auth).as_deref()))
}

/// The ChatGPT member the `auth.json` id token was issued to. The claims are read, not verified: they label the login, as in codex-swap.
pub(crate) fn codex_auth_user(auth: &Value) -> Option<String> {
    let token = auth.pointer("/tokens/id_token")?.as_str()?;
    let payload = token.split('.').nth(1)?;
    let claims: Value =
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(payload.trim_end_matches('=')).ok()?)
            .ok()?;
    let auth = &claims["https://api.openai.com/auth"];
    // Same precedence as codex-swap's `userId`: a blank `chatgpt_user_id` is an unknown member, not a reason to read `user_id`.
    usable(
        auth["chatgpt_user_id"]
            .as_str()
            .or_else(|| auth["user_id"].as_str()),
    )
    .map(str::to_string)
}

/// Whether a login read now (`observed`) is the saved account's login (`saved`). Claude compares exactly. A Codex side that does not name the member (a legacy saved identity, or an older reader) can only check the workspace, which is what every check did before members were told apart.
pub(crate) fn identity_matches(provider: Provider, saved: &str, observed: &str) -> bool {
    if saved.is_empty() || observed.is_empty() {
        return false;
    }
    if provider == Provider::Claude || saved == observed {
        return saved == observed;
    }
    match (saved.split_once(':'), observed.split_once(':')) {
        (Some(_), Some(_)) => false,
        _ => codex_account_part(saved) == codex_account_part(observed),
    }
}

fn usable(value: Option<&str>) -> Option<&str> {
    value.filter(|value| !value.trim().is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn id_token(claims: Value) -> String {
        format!(
            "e30.{}.sig",
            URL_SAFE_NO_PAD.encode(serde_json::to_vec(&claims).unwrap())
        )
    }

    /// The reporter's three slots (#312): two members of one Business workspace and one personal account of the first member.
    #[test]
    fn members_of_one_workspace_get_their_own_identity() {
        let team_first = codex_identity("89b7ba55", Some("user-wgOd"));
        let plus_first = codex_identity("a29cdd25", Some("user-wgOd"));
        let team_second = codex_identity("89b7ba55", Some("user-2o3K"));
        assert_eq!(team_first, "89b7ba55:user-wgOd");
        assert_eq!(plus_first, "a29cdd25:user-wgOd");
        assert_eq!(team_second, "89b7ba55:user-2o3K");
        assert_ne!(team_first, team_second);
        assert_ne!(team_first, plus_first);
        assert!(!identity_matches(
            Provider::Codex,
            &team_first,
            &team_second
        ));
        assert!(identity_matches(Provider::Codex, &team_first, &team_first));
    }

    #[test]
    fn a_single_user_account_is_account_and_user() {
        assert_eq!(
            codex_identity("0e1e7f6e", Some("user-Bp8i")),
            "0e1e7f6e:user-Bp8i"
        );
        assert_eq!(codex_account_part("0e1e7f6e:user-Bp8i"), "0e1e7f6e");
    }

    /// codex-swap before 0.3.2 lists no `userId`: the identity stays the workspace id, and such a saved login still matches what `auth.json` reports.
    #[test]
    fn a_row_without_a_user_keeps_the_workspace_identity() {
        assert_eq!(codex_identity("0e1e7f6e", None), "0e1e7f6e");
        assert_eq!(codex_identity("0e1e7f6e", Some("  ")), "0e1e7f6e");
        assert_eq!(codex_identity("", Some("user-1")), "");
        assert!(identity_matches(
            Provider::Codex,
            "0e1e7f6e",
            "0e1e7f6e:user-1"
        ));
        assert!(identity_matches(
            Provider::Codex,
            "0e1e7f6e:user-1",
            "0e1e7f6e"
        ));
        assert!(!identity_matches(
            Provider::Codex,
            "0e1e7f6e",
            "a29cdd25:user-1"
        ));
        assert!(!identity_matches(Provider::Codex, "", ""));
    }

    #[test]
    fn claude_identities_compare_exactly() {
        assert!(identity_matches(Provider::Claude, "a@b.c:org", "a@b.c:org"));
        assert!(!identity_matches(Provider::Claude, "a@b.c:org", "a@b.c"));
    }

    #[test]
    fn auth_json_names_the_member_from_the_id_token() {
        let auth = json!({"tokens": {"account_id": "89b7ba55", "id_token": id_token(json!({
            "https://api.openai.com/auth": {"chatgpt_user_id": "user-2o3K", "user_id": "user-fallback"}
        }))}});
        assert_eq!(
            codex_auth_identity(&auth).as_deref(),
            Some("89b7ba55:user-2o3K")
        );
        let fallback = json!({"tokens": {"account_id": "89b7ba55", "id_token": id_token(json!({
            "https://api.openai.com/auth": {"chatgpt_user_id": null, "user_id": "user-fallback"}
        }))}});
        assert_eq!(
            codex_auth_identity(&fallback).as_deref(),
            Some("89b7ba55:user-fallback")
        );
        let blank = json!({"tokens": {"account_id": "89b7ba55", "id_token": id_token(json!({
            "https://api.openai.com/auth": {"chatgpt_user_id": " ", "user_id": "user-fallback"}
        }))}});
        assert_eq!(codex_auth_identity(&blank).as_deref(), Some("89b7ba55"));
        let unreadable = json!({"tokens": {"account_id": "89b7ba55", "id_token": "not-a-jwt"}});
        assert_eq!(
            codex_auth_identity(&unreadable).as_deref(),
            Some("89b7ba55")
        );
        assert_eq!(codex_auth_identity(&json!({"tokens": {}})), None);
    }
}
