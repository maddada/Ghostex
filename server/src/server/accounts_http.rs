use super::*;

/// CDXC:Security 2026-11-21 DECISION:
/// Fable's security review of the Claude limit resets (finding 1, proven live): any agent could spend one of the user's banked limit resets with `operation: "redeemReset"`, since this route took no caller and the account and grant ids are in `ghostex accounts --json`. A reset is the user's to spend: only their own computer (the desktop app's owner session, or the token file the app and their terminal use) or a paired phone may redeem one. An agent's session token, another computer, an extension or a Collab guest is refused with 403 and the refusal goes to the security log. Autopilot's own claims run inside gxserver and never come through here.
const REDEEM_ONLY_YOU: &str = "Only the user can use a limit reset: from the account's usage panel in Ghostex or on their phone. Tell the user a reset is available; never use it for them.";

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
    if params.get("operation").and_then(Value::as_str) == Some("redeemReset") && !user {
        crate::audit::record(
            crate::audit::Action::new("accounts.redeemReset", "Use a limit reset").target(
                params
                    .get("id")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string(),
            ),
            crate::audit::Actor::current(),
            crate::audit::Outcome::Refused(REDEEM_ONLY_YOU.to_string()),
            None,
        );
        return domain_error_response(
            endpoint_path,
            request_id,
            DomainStateError {
                code: "forbidden",
                message: REDEEM_ONLY_YOU.to_string(),
            },
        );
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
