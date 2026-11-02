//! `ghostex secret`: an agent asks the user for a secret through a private card in the chat
//! (`request`), and the user's secret providers are listed and changed (`providers`).

use std::time::{Duration, Instant};

use serde_json::{json, Map, Value};

use crate::ghostex_cli::args::{parse_args, Flags, ParsedArgs};
use crate::ghostex_cli::output::{is_failed_cli_result, print_json};
use crate::ghostex_cli::rpc::{call_gxserver_rpc, CliError, CliResult};
use crate::ghostex_cli::{notify, selector, set_exit_code, skills, usage};

const DEFAULT_MINUTES: f64 = 10.0;
const MAX_MINUTES: f64 = 60.0;
/// What one wait asks gxserver to hold the call for, and the HTTP limit around it.
const WAIT_MS: u64 = 20_000;
const WAIT_HTTP_TIMEOUT_MS: &str = "40000";

pub fn secret_command(args: &[String]) -> CliResult<()> {
    let subcommand = args.first().map(String::as_str).unwrap_or("help");
    let rest = args.get(1..).unwrap_or_default();
    match subcommand {
        "help" | "-h" | "--help" => {
            println!("{}", usage::secret_usage());
            Ok(())
        }
        "request" | "ask" => request_command(rest),
        "providers" | "provider" => providers_command(rest),
        "install-skill" => skills::install_secrets_skill_command(rest),
        other => Err(CliError::Other(format!(
            "Unknown secret command: {other}\n\n{}",
            usage::secret_usage()
        ))),
    }
}

fn wants_help(args: &[String]) -> bool {
    args.iter()
        .any(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
}

fn flag_text(parsed: &ParsedArgs, key: &str) -> Option<String> {
    parsed
        .flags
        .string_value(key)
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// The session the card goes to: `--session-id`, else the session this command runs in.
fn request_session(parsed: &ParsedArgs) -> CliResult<(String, String)> {
    if let Some(selector_text) =
        flag_text(parsed, "sessionId").or_else(|| flag_text(parsed, "session"))
    {
        let session = selector::resolve_cli_session_selector(&selector_text, &parsed.flags)?;
        let text = |key: &str| session.get(key).and_then(Value::as_str).map(str::to_string);
        return match (text("projectId"), text("sessionId")) {
            (Some(project_id), Some(session_id)) => Ok((project_id, session_id)),
            _ => Err(CliError::Other(format!(
                "Session \"{selector_text}\" does not report a project id."
            ))),
        };
    }
    notify::session_reference_from_environment().ok_or_else(|| {
        CliError::Other(
            "secret request must run inside a Ghostex session, where the user sees the card."
                .to_string(),
        )
    })
}

fn wait_flags(flags: &Flags) -> Flags {
    let mut wait = flags.clone();
    wait.0.remove("timeoutMs");
    wait.insert_text("timeout", WAIT_HTTP_TIMEOUT_MS);
    wait
}

fn request_command(args: &[String]) -> CliResult<()> {
    if wants_help(args) {
        println!("{}", usage::secret_usage());
        return Ok(());
    }
    let parsed = parse_args(args);
    let label = flag_text(&parsed, "label").unwrap_or_default();
    let reason = flag_text(&parsed, "reason").unwrap_or_default();
    if label.is_empty() || reason.is_empty() {
        return Err(CliError::Other(format!(
            "secret request needs --label and --reason.\n\n{}",
            usage::secret_usage()
        )));
    }
    let minutes = parsed
        .flags
        .number("timeoutMinutes")
        .filter(|minutes| minutes.is_finite() && *minutes > 0.0)
        .unwrap_or(DEFAULT_MINUTES)
        .min(MAX_MINUTES);
    let (project_id, session_id) = request_session(&parsed)?;
    // `--timeout` is the CLI's own HTTP limit for every other verb; this verb's wait is
    // `--timeout-minutes`, so neither is mistaken for the other.
    let mut rpc_flags = parsed.flags.clone();
    rpc_flags.0.remove("timeout");
    rpc_flags.0.remove("timeoutMs");
    let mut payload = Map::new();
    payload.insert("projectId".to_string(), json!(project_id));
    payload.insert("sessionId".to_string(), json!(session_id));
    payload.insert("label".to_string(), json!(label));
    payload.insert("reason".to_string(), json!(reason));
    if let Some(placeholder) = flag_text(&parsed, "placeholder") {
        payload.insert("placeholder".to_string(), json!(placeholder));
    }
    payload.insert(
        "timeoutMs".to_string(),
        json!((minutes * 60_000.0).round() as u64),
    );
    let created = call_gxserver_rpc(
        "/api/createSecretRequest",
        &Value::Object(payload),
        &rpc_flags,
    )?;
    let Some(request_id) = created
        .get("requestId")
        .and_then(Value::as_str)
        .map(str::to_string)
    else {
        print_json(&created);
        set_exit_code(1);
        return Ok(());
    };
    eprintln!("Waiting for the user to answer the secret request in the chat…");
    let deadline = Instant::now() + Duration::from_secs_f64(minutes * 60.0 + 30.0);
    let flags = wait_flags(&rpc_flags);
    let outcome = loop {
        let result = call_gxserver_rpc(
            "/api/waitSecretRequest",
            &json!({ "requestId": request_id, "waitMs": WAIT_MS }),
            &flags,
        );
        match result {
            Ok(result) => {
                let status = result
                    .get("status")
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_string();
                if status != "pending" {
                    break result;
                }
            }
            // A dropped connection or a gxserver restart: the request lived in that gxserver.
            Err(error) if Instant::now() >= deadline => return Err(error),
            Err(CliError::Rpc { .. }) => return Err(CliError::Other(
                "The secret request ended before it was answered (Ghostex restarted). Ask again."
                    .to_string(),
            )),
            Err(_) => std::thread::sleep(Duration::from_secs(1)),
        }
        if Instant::now() >= deadline {
            let _ = call_gxserver_rpc(
                "/api/cancelSecretRequest",
                &json!({ "requestId": request_id }),
                &rpc_flags,
            );
            break json!({ "ok": true, "status": "timedOut" });
        }
    };
    print_json(&describe(&outcome));
    if outcome.get("status").and_then(Value::as_str) != Some("saved") {
        set_exit_code(1);
    }
    Ok(())
}

/// The result an agent reads: the status, the file for a saved secret, and what to do next.
fn describe(outcome: &Value) -> Value {
    let status = outcome
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("unknown");
    let next = match status {
        "saved" => "The secret is in this owner-only file (deleted after 24 hours). Read it only inside the command that needs it, e.g. TOKEN=\"$(cat '<file>')\" some-cli …, or `sudo -k -S -p '' <command> < '<file>'` after `sudo -n <command>` failed for lack of a password. Never print, echo, copy or commit it, and never repeat it in chat.",
        "declined" => "The user chose not to give it. Do not ask again unless they say so; carry on without it or ask how they want to proceed.",
        "timedOut" => "The user did not answer in time and the card closed. Ask again only if it is still needed.",
        "cancelled" => "The request ended before it was answered.",
        _ => "The request ended without an answer.",
    };
    let mut result = json!({ "status": status, "next": next });
    if let Some(file) = outcome.get("file").and_then(Value::as_str) {
        result["file"] = json!(file);
        result["next"] = json!(next.replace("<file>", file));
    }
    result
}

fn providers_command(args: &[String]) -> CliResult<()> {
    let action = args.first().map(String::as_str).unwrap_or("list");
    let rest = args.get(1..).unwrap_or_default();
    if wants_help(args) {
        println!("{}", usage::secret_usage());
        return Ok(());
    }
    let parsed = parse_args(rest);
    let flags = &parsed.flags;
    let result = match action {
        "list" | "ls" => call_gxserver_rpc("/api/readSecretProviders", &json!({}), flags)?,
        "add" => {
            let label = flag_text(&parsed, "name")
                .or_else(|| flag_text(&parsed, "label"))
                .ok_or_else(|| CliError::Other("providers add needs --name.".to_string()))?;
            let account = flag_text(&parsed, "account").ok_or_else(|| {
                CliError::Other(
                    "providers add needs --account (the 1Password sign-in address, email or account ID; `op account list` shows them).".to_string(),
                )
            })?;
            call_gxserver_rpc(
                "/api/saveSecretProvider",
                &json!({ "kind": "onePassword", "label": label, "account": account }),
                flags,
            )?
        }
        "remove" | "rm" => {
            let id = parsed
                .rest
                .first()
                .cloned()
                .or_else(|| flag_text(&parsed, "id"))
                .ok_or_else(|| {
                    CliError::Other("providers remove needs the provider id.".to_string())
                })?;
            call_gxserver_rpc("/api/removeSecretProvider", &json!({ "id": id }), flags)?
        }
        "check" => {
            let provider = flag_text(&parsed, "provider").ok_or_else(|| {
                CliError::Other("providers check needs --provider <id>.".to_string())
            })?;
            let reference = flag_text(&parsed, "reference").ok_or_else(|| {
                CliError::Other(
                    "providers check needs --reference op://vault/item/field.".to_string(),
                )
            })?;
            let mut check_flags = flags.clone();
            // 1Password may wait for the user to unlock it.
            check_flags.insert_text("timeout", "60000");
            call_gxserver_rpc(
                "/api/checkSecretReference",
                &json!({ "provider": provider, "reference": reference }),
                &check_flags,
            )?
        }
        other => {
            return Err(CliError::Other(format!(
                "Unknown secret providers command: {other}\n\n{}",
                usage::secret_usage()
            )))
        }
    };
    print_json(&result);
    if is_failed_cli_result(&result) || result.get("readable") == Some(&Value::Bool(false)) {
        set_exit_code(1);
    }
    Ok(())
}
