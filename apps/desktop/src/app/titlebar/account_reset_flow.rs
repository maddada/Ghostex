use serde_json::{Value, json};
use std::time::{Duration, Instant};

pub(super) type Rpc<'a> = dyn Fn(&str, &Value) -> Result<Value, String> + 'a;

pub(super) struct ResetSession {
    pub target: Value,
    pub credit_id: String,
}
/// CDXC:AgentProviders 2026-09-11 DECISION:
/// User: run reset redemption in the currently active project's folder and show its terminal in that project's sidebar. This replaces the separate Quick chat workspace.
pub(super) fn create(
    rpc: &Rpc<'_>,
    account_id: &str,
    project_id: &str,
) -> Result<ResetSession, String> {
    if project_id.is_empty() {
        return Err("Open a project before redeeming a reset.".into());
    }
    let plan = rpc(
        "/api/agentAccounts",
        &json!({"operation":"prepareReset","id":account_id}),
    )?;
    if plan["credits"]
        .as_array()
        .is_none_or(|credits| credits.is_empty())
    {
        return Err("This account has no available resets.".into());
    }
    let credit_id = plan["credits"][0]["id"]
        .as_str()
        .ok_or("The reset identifier is unavailable.")?
        .to_string();
    let created = rpc(
        "/api/createAgentSession",
        &json!({
            "projectId":project_id,"agentId":"codex","title":"Codex reset","requireLaunchCommand":true,
            "runtimeSettings":{"accountId":account_id,
                "accountPolicyOverride":{"enabled":false,"atLimit":"wait","priority":"soonestReset","retryErrors":false}}
        }),
    )?;
    let session_id = created["session"]["sessionId"]
        .as_str()
        .ok_or("Could not create the reset terminal.")?;
    let target = json!({"projectId":project_id,"sessionId":session_id});
    rpc("/api/startSessionProvider", &target)?;
    Ok(ResetSession { target, credit_id })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Mode {
    Redeem,
    StopBeforeRedeem,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Stage {
    Starting,
    Usage,
    Credits,
}

/// CDXC:AgentProviders 2026-09-11 DECISION:
/// User: Redeem a reset opens a terminal chat and redeems the soonest-expiring credit for that account. Live testing stops at the final picker without selecting a number or pressing Enter.
/// The final answer uses the current dialog fingerprint, runs once, and is never retried after an uncertain response.
pub(super) fn drive(
    rpc: &Rpc<'_>,
    session: &ResetSession,
    account_id: &str,
    mode: Mode,
) -> Result<(), String> {
    drive_with_wait(rpc, session, account_id, mode, &|| {
        std::thread::sleep(Duration::from_millis(500))
    })
}

fn drive_with_wait(
    rpc: &Rpc<'_>,
    session: &ResetSession,
    account_id: &str,
    mode: Mode,
    wait: &dyn Fn(),
) -> Result<(), String> {
    let target = &session.target;
    let deadline = Instant::now() + Duration::from_secs(90);
    let mut stage = Stage::Starting;
    let mut trusted = false;
    while Instant::now() < deadline {
        let state = rpc("/api/readSessionChat", target)?;
        let dialog = &state["terminalNotice"]["dialog"];
        let title = dialog["title"].as_str().unwrap_or("");
        let rows = dialog["rows"].as_array();
        if stage == Stage::Starting {
            if dialog["id"]
                .as_str()
                .is_some_and(|id| id.starts_with("codex-directory-trust:"))
                && !trusted
            {
                let index = rows
                    .and_then(|rows| rows.iter().position(|r| r["label"] == "Trust and continue"));
                if let Some(index) = index {
                    answer(rpc, target, dialog, index)?;
                    trusted = true;
                    wait();
                    continue;
                }
            }
            if !dialog.is_null() {
                return Err(
                    "Codex needs attention before opening usage. Continue in the reset chat."
                        .into(),
                );
            }
            if state["agent"] == "codex"
                && state["screenProbed"] == true
                && !state["selectedOptions"].is_null()
                && state["working"] != true
            {
                let mut params = target.clone();
                params["text"] = json!("/usage");
                rpc("/api/sendSessionChatMessage", &params)?;
                stage = Stage::Usage;
            }
        } else if stage == Stage::Usage
            && title == "Usage"
            && rows.is_some_and(|rows| !rows.is_empty())
        {
            let index = rows
                .and_then(|rows| {
                    rows.iter()
                        .position(|r| r["label"] == "Redeem usage limit reset")
                })
                .ok_or("Codex did not offer a reset. Review the reset chat.")?;
            answer(rpc, target, dialog, index)?;
            stage = Stage::Credits;
        } else if stage == Stage::Credits
            && title == "Usage limit resets"
            && rows.is_some_and(|rows| !rows.is_empty())
        {
            let plan = rpc(
                "/api/agentAccounts",
                &json!({"operation":"prepareReset","id":account_id}),
            )?;
            let credit = plan["credits"]
                .as_array()
                .and_then(|credits| credits.first())
                .ok_or("No available reset remains.")?;
            if credit["id"] != session.credit_id {
                return Err(
                    "The available resets changed. Review the reset chat before trying again."
                        .into(),
                );
            }
            let index = earliest_choice(dialog, credit)?;
            if mode == Mode::StopBeforeRedeem {
                return Ok(());
            }
            answer(rpc, target, dialog, index)?;
            // Confirmation is read-only. A lost reply must never cause another selection.
            for _ in 0..6 {
                wait();
                let updated = rpc(
                    "/api/agentAccounts",
                    &json!({"operation":"prepareReset","id":account_id}),
                )?;
                if updated["credits"]
                    .as_array()
                    .is_some_and(|credits| !credits.iter().any(|c| c["id"] == credit["id"]))
                {
                    return Ok(());
                }
            }
            return Err("The reset was submitted, but completion is unconfirmed. Check the reset chat before trying again.".into());
        }
        wait();
    }
    Err("Codex did not finish opening the reset picker. Continue in the reset chat.".into())
}

fn answer(rpc: &Rpc<'_>, target: &Value, dialog: &Value, index: usize) -> Result<(), String> {
    let id = dialog["id"]
        .as_str()
        .filter(|id| !id.is_empty())
        .ok_or("The reset dialog changed.")?;
    let mut params = target.clone();
    params["kind"] = json!("terminalDialog");
    params["dialogId"] = json!(id);
    params["choiceIndex"] = json!(index);
    rpc("/api/answerSessionChatPrompt", &params).map(|_| ())
}

fn earliest_choice(dialog: &Value, credit: &Value) -> Result<usize, String> {
    let expiry = credit["pickerExpiry"]
        .as_str()
        .ok_or("The earliest reset has no expiry date. Choose a reset in the chat.")?;
    let rows = dialog["rows"]
        .as_array()
        .ok_or("The reset picker is unavailable.")?;
    // The account API and the CLI must agree about expiry before any credit is selected.
    rows.iter().position(|row| row["label"] == "Full reset" && row["description"] == expiry)
        .ok_or_else(|| "Reset expiry dates changed or the CLI format is unsupported. Choose a reset in the chat.".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn reset_terminal_is_created_under_the_supplied_project() {
        let calls = RefCell::new(Vec::new());
        let rpc = |path: &str, params: &Value| -> Result<Value, String> {
            calls
                .borrow_mut()
                .push(json!({"path":path,"params":params}));
            match path {
                "/api/agentAccounts" => Ok(json!({"credits":[{"id":"first"}]})),
                "/api/createAgentSession" => {
                    assert_eq!(params["projectId"], "active-project");
                    assert_eq!(params["runtimeSettings"]["accountId"], "selected-account");
                    assert!(
                        params.get("cwd").is_none(),
                        "The server inherits the project's folder."
                    );
                    Ok(
                        json!({"session":{"projectId":"active-project","sessionId":"reset-session"}}),
                    )
                }
                "/api/startSessionProvider" => Ok(json!({})),
                _ => panic!("Unexpected reset launch operation: {path}"),
            }
        };
        let session = create(&rpc, "selected-account", "active-project").unwrap();
        assert_eq!(
            session.target,
            json!({"projectId":"active-project","sessionId":"reset-session"})
        );
        assert_eq!(calls.borrow().last().unwrap()["params"], session.target);
        let before = calls.borrow().len();
        assert!(create(&rpc, "selected-account", "").is_err());
        assert_eq!(calls.borrow().len(), before);
    }

    fn run(mode: Mode, fail_final: bool) -> Vec<Value> {
        let calls = RefCell::new(Vec::new());
        let reads = RefCell::new(0);
        let submitted = RefCell::new(false);
        let rpc = |path: &str, params: &Value| -> Result<Value, String> {
            calls
                .borrow_mut()
                .push(json!({"path":path,"params":params}));
            match path {
                "/api/readSessionChat" => {
                    *reads.borrow_mut() += 1;
                    Ok(match *reads.borrow() {
                        1 => {
                            json!({"terminalNotice":{"dialog":{"title":"Trust folder?","id":"codex-directory-trust:test","rows":[{"label":"Trust and continue"},{"label":"Quit Codex"}]}}})
                        }
                        2 => {
                            json!({"agent":"codex","screenProbed":true,"selectedOptions":{},"working":false})
                        }
                        3 => {
                            json!({"terminalNotice":{"dialog":{"title":"Usage","id":"usage","rows":[{"label":"Show usage"},{"label":"Redeem usage limit reset"}]}}})
                        }
                        4 => {
                            json!({"terminalNotice":{"dialog":{"title":"Usage limit resets","id":"loading","rows":[]}}})
                        }
                        _ => {
                            json!({"terminalNotice":{"dialog":{"title":"Usage limit resets","id":"credits","rows":[
                            {"label":"Full reset","description":"Expires 08:18 on 5 Oct 2026."},
                            {"label":"Full reset","description":"Expires 06:02 on 4 Oct 2026."},
                            {"label":"Cancel"}]}}})
                        }
                    })
                }
                "/api/agentAccounts" => Ok(if *submitted.borrow() {
                    json!({"credits":[]})
                } else {
                    json!({"credits":[{"id":"first","pickerExpiry":"Expires 06:02 on 4 Oct 2026."}]})
                }),
                "/api/answerSessionChatPrompt" if params["dialogId"] == "credits" => {
                    *submitted.borrow_mut() = true;
                    if fail_final {
                        Err("Lost response".into())
                    } else {
                        Ok(json!({}))
                    }
                }
                _ => Ok(json!({})),
            }
        };
        let session = ResetSession {
            target: json!({"projectId":"test","sessionId":"test"}),
            credit_id: "first".into(),
        };
        let result = drive_with_wait(&rpc, &session, "account", mode, &|| {});
        assert_eq!(result.is_err(), fail_final);
        calls.into_inner()
    }
    #[test]
    fn dry_run_never_submits_final_credit() {
        let calls = run(Mode::StopBeforeRedeem, false);
        assert_eq!(
            calls
                .iter()
                .filter(|c| c["path"] == "/api/answerSessionChatPrompt")
                .count(),
            2
        );
        assert!(
            calls
                .iter()
                .filter(|c| c["path"] == "/api/answerSessionChatPrompt")
                .all(|c| c["params"]["kind"] == "terminalDialog")
        );
        assert!(!calls.iter().any(|c| c["params"]["dialogId"] == "credits"));
    }
    #[test]
    fn chooses_earliest_expiry_even_if_picker_order_changes() {
        let calls = run(Mode::Redeem, false);
        let final_calls: Vec<_> = calls
            .iter()
            .filter(|c| c["params"]["dialogId"] == "credits")
            .collect();
        assert_eq!(final_calls.len(), 1);
        assert_eq!(final_calls[0]["params"]["choiceIndex"], 1);
    }
    #[test]
    fn lost_final_response_is_never_retried() {
        assert_eq!(
            run(Mode::Redeem, true)
                .iter()
                .filter(|c| c["params"]["dialogId"] == "credits")
                .count(),
            1
        );
    }
    #[test]
    fn changed_or_unknown_expiry_cannot_be_selected() {
        assert!(
            earliest_choice(
                &json!({"rows":[{"label":"Full reset","description":"Unknown"}]}),
                &json!({"pickerExpiry":"Expires 06:02 on 4 Oct 2026."})
            )
            .is_err()
        );
    }
}
