use super::*;

/// CDXC:Security 2026-11-21 DECISION:
/// Fable's security review of the Claude limit resets (finding 1, proven live): any agent could spend one of the user's banked limit resets with `operation: "redeemReset"`, since this route took no caller and the account and grant ids are in `ghostex accounts --json`. A reset is the user's to spend: only their own computer (the desktop app's owner session, or the token file the app and their terminal use) or a paired phone may redeem one. An agent's session token, another computer, an extension or a Collab guest is refused with 403 and the refusal goes to the security log. Autopilot's own claims run inside gxserver and never come through here.
const REDEEM_ONLY_YOU: &str = "Only the user can use a limit reset: from the account's usage panel in Ghostex or on their phone. Tell the user a reset is available; never use it for them.";

const HUB_ONLY_YOU: &str = "Only the user can manage CLIProxyAPI hubs and their accounts: in Settings > Accounts, on their phone, or from their own terminal outside Ghostex.";

const ACCOUNTS_ONLY_YOU: &str = "Only the user can change accounts, sign in, or manage account helpers: in Settings > Accounts or on their phone.";

/// Operations on `/api/agentAccounts` restricted to the user's own devices (owner session,
/// paired phone, or the token file from a terminal outside a Ghostex session).
fn user_only_gate(operation: &str) -> Option<(&'static str, &'static str)> {
    if operation == "redeemReset" {
        return Some(("Use a limit reset", REDEEM_ONLY_YOU));
    }
    if operation.starts_with("hub") {
        return Some(("Manage a CLIProxyAPI hub", HUB_ONLY_YOU));
    }
    if operation.starts_with("setup")
        || operation.starts_with("usageSignIn")
        || matches!(
            operation,
            "helperAction"
                | "register"
                | "update"
                | "remove"
                | "setEnabled"
                | "setTitlebar"
                | "setSignInIgnored"
                | "swapSlots"
                | "defaultAccount"
                | "defaults"
        )
    {
        return Some(("Manage Ghostex accounts", ACCOUNTS_ONLY_YOU));
    }
    None
}

/// Safe, non-secret target identifier for security-log entries (never includes keys or inputs).
fn audit_target(params: &Map<String, Value>) -> String {
    for key in ["hubId", "accountId", "id", "provider", "action"] {
        if let Some(value) = params
            .get(key)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|v| !v.is_empty())
        {
            return value.chars().take(120).collect();
        }
    }
    String::new()
}

pub(crate) async fn handle_accounts_http(
    state: &AppState,
    caller: &crate::security::Principal,
    endpoint_path: String,
    request_id: String,
    body: &Value,
) -> RoutedResponse {
    let params = match read_domain_rpc_params(body) {
        Ok(p) => p,
        Err(e) => return domain_error_response(endpoint_path, request_id, e),
    };
    // The token file from a Ghostex session pane is an agent that dropped its session token
    // (`env -u GHOSTEX_SESSION_TOKEN …`, Fable's re-check), as autopilot_http.rs `is_user` reads it.
    let user = caller.is_owner_device()
        && !(matches!(caller, crate::security::Principal::LocalAgent)
            && crate::audit::token_file_names_session());
    if let Some(operation) = params.get("operation").and_then(Value::as_str) {
        if let Some((summary, message)) = user_only_gate(operation) {
            if !user {
                crate::audit::record(
                    crate::audit::Action::new(format!("accounts.{operation}"), summary)
                        .target(audit_target(&params)),
                    crate::audit::Actor::current(),
                    crate::audit::Outcome::Refused(message.to_string()),
                    None,
                );
                return domain_error_response(
                    endpoint_path,
                    request_id,
                    DomainStateError {
                        code: "forbidden",
                        message: message.to_string(),
                    },
                );
            }
        }
    }
    let owned = state.clone();
    match tokio::task::spawn_blocking(move || crate::accounts::endpoint::dispatch(&owned, &params))
        .await
    {
        Ok(Ok(result)) => routed_json(
            Some(endpoint_path),
            StatusCode::OK,
            rpc_success(request_id, result),
        ),
        Ok(Err(error)) => domain_error_response(endpoint_path, request_id, error),
        Err(error) => domain_error_response(
            endpoint_path,
            request_id,
            crate::accounts::store::error(error),
        ),
    }
}
