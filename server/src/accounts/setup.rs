use super::{
    codex_blockers, endpoint, helpers,
    model::Provider,
    setup_terminal::{LoginTerminal, COLS, ROWS},
    store,
};
use crate::{domain::DomainStateError, server::AppState};
use portable_pty::{native_pty_system, CommandBuilder, PtySize};
use serde_json::{json, Map, Value};
use std::{
    collections::HashMap,
    io::{Read, Write},
    sync::{Arc, Mutex},
};

pub(crate) struct SetupJob {
    view: Value,
    process_id: Option<u32>,
    writer: Option<Box<dyn Write + Send>>,
    killer: Option<Box<dyn portable_pty::ChildKiller + Send + Sync>>,
}
pub(crate) type SetupJobs = Mutex<HashMap<String, Arc<Mutex<SetupJob>>>>;

impl SetupJob {
    fn cancel(&mut self) -> Result<(), DomainStateError> {
        if self.killer.is_none() {
            return Ok(());
        }
        // portable-pty starts a new session, so this group contains only this login and its CLI children.
        #[cfg(unix)]
        if let Some(pid) = self.process_id {
            let result = unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
            if result != 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH) {
                return Err(store::error(std::io::Error::last_os_error()));
            }
        }
        #[cfg(not(unix))]
        if let Some(killer) = &mut self.killer {
            killer.kill().map_err(store::error)?;
        }
        self.writer = None;
        Ok(())
    }
}
pub(crate) fn cancel_all(state: &AppState) {
    if let Ok(jobs) = state.accounts.setup_jobs.lock() {
        for job in jobs.values() {
            if let Ok(mut job) = job.lock() {
                let _ = job.cancel();
            }
        }
    }
}

fn response(jobs: Vec<Value>) -> Value {
    json!({"accounts":[],"helpers":[],"defaults":{"claude":{},"codex":{}},"defaultAccounts":{},"newSessionAccounts":{},"setupJobs":jobs})
}
fn field<'a>(params: &'a Map<String, Value>, key: &str) -> Result<&'a str, DomainStateError> {
    params
        .get(key)
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty() && s.len() < 512)
        .ok_or_else(|| DomainStateError::bad_request(format!("{key} is required")))
}
/// CDXC:AgentProviders 2026-09-08 DECISION:
/// Settings owns account login from start to finish, including browser/code prompts, registration and returning to the new account. Completion follows helper exit and its saved identity, never a matching success phrase in terminal output.
pub(crate) fn dispatch(
    state: &AppState,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let operation = field(params, "operation")?;
    let owner = field(params, "owner")?;
    if !matches!(
        operation,
        "setupStart" | "setupStatus" | "setupInput" | "setupCancel" | "setupAcknowledge"
    ) {
        return Err(DomainStateError::bad_request(
            "Unknown account login operation.",
        ));
    }
    if operation == "setupStart" {
        let provider: Provider =
            serde_json::from_value(params.get("provider").cloned().unwrap_or_default())
                .map_err(store::error)?;
        let email = params
            .get("email")
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|email| !email.is_empty());
        if email.is_some_and(|email| {
            email.len() >= 512 || !email.contains('@') || email.chars().any(char::is_whitespace)
        }) {
            return Err(DomainStateError::bad_request("Enter the account email."));
        }
        if params.get("shareHistory").and_then(Value::as_bool) != Some(true) {
            return Err(DomainStateError::bad_request(
                "Allow shared conversations to add an account.",
            ));
        }
        let reconnect = params.get("accountId").and_then(Value::as_str);
        let db = crate::storage::open_gxserver_database(&state.paths).map_err(store::error)?;
        let registry = store::read(&db)?;
        let selected = reconnect
            .map(|id| {
                registry
                    .accounts
                    .iter()
                    .find(|a| a.id == id && a.provider == provider)
                    .ok_or_else(|| {
                        DomainStateError::bad_request(
                            "The selected account is no longer registered.",
                        )
                    })
            })
            .transpose()?;
        let repair_selector = params
            .get("selector")
            .and_then(Value::as_str)
            .filter(|slot| slot.parse::<u32>().is_ok_and(|number| number > 0));
        // An account synced from another computer has no slot here yet: it signs in as a new
        // login (pre-filled with its email), and `register` matches it back by identity.
        let selector = selected
            .map(|account| account.selector.as_str())
            .filter(|selector| !selector.is_empty())
            .or(repair_selector);
        let synced_email = selected
            .and_then(|account| account.synced.as_ref())
            .map(|synced| synced.email.clone())
            .filter(|email| !email.is_empty());
        let email = email.or(synced_email.as_deref());
        let binary = helpers::executable(
            &state.paths.home_dir,
            if provider == Provider::Claude {
                "ghostex"
            } else {
                "xswap"
            },
        )
        .ok_or_else(|| {
            DomainStateError::bad_request(format!(
                "Install {} in Settings > Accounts first.",
                helpers::tool_label(provider.helper())
            ))
        })?;
        let mut command = CommandBuilder::new(binary);
        let result_dir = if provider == Provider::Claude {
            Some(
                tempfile::Builder::new()
                    .prefix("ghostex-account-login-")
                    .tempdir()
                    .map_err(store::error)?,
            )
        } else {
            None
        };
        if let Some(result_dir) = &result_dir {
            command.args(["account-login", "claude", "--json", "--result-file"]);
            command.arg(result_dir.path().join(RESULT_FILE));
            if let Some(selector) = selector {
                // The CLI pre-fills the browser with the saved account's own email.
                command.args(["--account", selector]);
            } else if let Some(email) = email {
                command.args(["--email", email]);
            }
        } else if let Some(selector) = selector {
            command.args(["login", selector]);
        } else if let Some(email) = email {
            command.args([
                "add",
                "--login",
                "--share-history",
                "--email",
                email,
                "--json",
            ]);
        } else {
            // CDXC:AgentProviders 2026-11-05 DECISION:
            // User picked "No email, like Claude" for adding a Codex account: Codex Swap 0.3.6 saves whichever account the browser signs in to (`--any-account`) and still refuses one that is already saved. Settings updates an older Codex Swap first (`codexSwapNeedsUpdate` on the helper row). Supersedes the 2026-10-14 rule that Add account asked for a new Codex account's email.
            if super::helper_tools::codex_swap_needs_update_to_add(&state.paths.home_dir) {
                return Err(DomainStateError::bad_request(
                    "Update Codex Swap to add a Codex account, then try again.",
                ));
            }
            command.args([
                "add",
                "--login",
                "--share-history",
                "--any-account",
                "--json",
            ]);
        }
        command.cwd(&state.paths.home_dir);
        command.env("TERM", "dumb");
        if provider == Provider::Codex {
            let stop_codex = params.get("stopCodex").and_then(Value::as_bool) == Some(true);
            if stop_codex {
                let sessions = params
                    .get("sleepSessions")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                codex_blockers::sleep_sessions(state, &sessions)?;
            }
            // The login runs in a terminal, so xswap would otherwise ask there; Settings offers its own button instead.
            command.env(
                "XSWAP_STOP_CODEX",
                if stop_codex { "always" } else { "never" },
            );
        }
        let pair = native_pty_system()
            .openpty(PtySize {
                rows: ROWS,
                cols: COLS,
                pixel_width: 0,
                pixel_height: 0,
            })
            .map_err(store::error)?;
        let reader = pair.master.try_clone_reader().map_err(store::error)?;
        let writer = pair.master.take_writer().map_err(store::error)?;
        let id = uuid::Uuid::new_v4().to_string();
        let mut child = pair.slave.spawn_command(command).map_err(store::error)?;
        drop(pair.slave);
        let job = Arc::new(Mutex::new(SetupJob {
            view: json!({"id":id,"owner":owner,"provider":provider,"email":email,"status":"signingIn","output":"","accountId":reconnect,"acknowledged":false,"createdAt":chrono::Utc::now().timestamp_millis()}),
            process_id: child.process_id(),
            writer: Some(writer),
            killer: Some(child.clone_killer()),
        }));
        {
            let mut jobs = state.accounts.setup_jobs.lock().map_err(store::error)?;
            jobs.retain(|_, job| {
                job.lock().is_ok_and(|job| {
                    job.view["acknowledged"] != true && job.view["status"] != "failed"
                })
            });
            if jobs.len() >= 20 {
                if let Ok(mut job) = job.lock() {
                    let _ = job.cancel();
                }
                let _ = child.wait();
                return Err(DomainStateError::bad_request(
                    "Finish an existing account login first.",
                ));
            }
            jobs.insert(id.clone(), job.clone());
        }
        let state = state.clone();
        let selector = selector.map(str::to_string);
        // GitHub #410: `register` updates sessions, and a session update starts the chat follower with `tokio::spawn`, which panics on a plain thread (poisoning `accounts.mutations`), so the login thread runs inside the caller's runtime.
        let runtime = tokio::runtime::Handle::try_current().ok();
        std::thread::spawn(move || {
            let _runtime = runtime.as_ref().map(|runtime| runtime.enter());
            let output_job = job.clone();
            let output = std::thread::spawn(move || read_output(reader, output_job));
            let exit = child.wait();
            let exited_ok = exit.as_ref().is_ok_and(|exit| exit.success());
            let exit_code = exit.as_ref().ok().map(|exit| exit.exit_code());
            if let Ok(mut job) = job.lock() {
                job.writer = None;
                job.killer = None;
                job.process_id = None;
            }
            drop(pair.master);
            let transcript = output.join().unwrap_or_default();
            let report = result_dir.as_ref().and_then(|dir| {
                serde_json::from_slice::<Value>(&std::fs::read(dir.path().join(RESULT_FILE)).ok()?)
                    .ok()
            });
            drop(result_dir);
            let result = match exit {
                Err(error) => Err(store::error(error)),
                Ok(_) if provider == Provider::Claude => {
                    claude_outcome(exited_ok, report.as_ref(), selector.as_deref())
                        .map_err(DomainStateError::bad_request)
                }
                Ok(_) if !exited_ok => Err(DomainStateError::bad_request(LOGIN_INCOMPLETE)),
                Ok(_) => selector
                    .clone()
                    .or_else(|| transcript_account_number(&transcript))
                    .ok_or_else(|| DomainStateError::bad_request(HELPER_SILENT)),
            };
            let failure_text = format!(
                "{transcript}\n{}",
                report
                    .as_ref()
                    .and_then(|report| report["error"].as_str())
                    .unwrap_or_default()
            );
            let wrong_account = result
                .as_ref()
                .err()
                .and_then(|_| wrong_account(&failure_text));
            let blockers = (provider == Provider::Codex && result.is_err())
                .then(|| codex_blockers::pids_in_refusal(&transcript))
                .filter(|pids| !pids.is_empty())
                .map(|pids| codex_blockers::describe(&state, &pids));
            let result = result.and_then(|selector| {
                if let Ok(mut job) = job.lock() { job.view["status"] = json!("saving"); }
                // Registering needs the account read as ready, which a session's expired-login mark would hide.
                let db = crate::storage::open_gxserver_database(&state.paths).map_err(store::error)?;
                super::login_expired::signed_in(&state, &db, provider, &selector)?;
                state.accounts.invalidate();
                let registered = endpoint::dispatch(&state, json!({"operation":"register","provider":provider,"selector":selector,"shareHistory":true}).as_object().unwrap())?;
                registered["accounts"].as_array().and_then(|rows| rows.iter().find(|a| a["selector"] == selector && a["provider"] == json!(provider) && a["registered"] == true)).and_then(|a| a["id"].as_str()).map(str::to_string).ok_or_else(|| DomainStateError::bad_request("The connected account could not be registered."))
            });
            if let Ok(mut job) = job.lock() {
                job.writer = None;
                job.killer = None;
                match result {
                    Ok(id) => {
                        job.view["status"] = json!("complete");
                        job.view["accountId"] = json!(id);
                        job.view["output"] = json!("");
                        job.view["url"] = Value::Null;
                    }
                    Err(error) => {
                        job.view["status"] = json!("failed");
                        if let Some(blockers) = blockers {
                            job.view["error"] = json!("Codex is still running and has to stop before this login can finish.");
                            job.view["blockers"] = blockers;
                        } else if let Some(signed_in) = wrong_account {
                            job.view["error"] =
                                json!("The browser signed in to a different account.");
                            job.view["wrongAccount"] = json!({ "signedIn": signed_in });
                        } else {
                            job.view["error"] = json!(error.message);
                        }
                        // A login that failed after cswap saved part of it must not leave the list stale.
                        state.accounts.invalidate();
                        log_failure(
                            &state,
                            provider,
                            selector.as_deref(),
                            exit_code,
                            job.view["error"].as_str().unwrap_or_default(),
                            &transcript,
                        );
                    }
                }
            }
        });
    } else if matches!(operation, "setupInput" | "setupCancel" | "setupAcknowledge") {
        let id = field(params, "jobId")?;
        let jobs = state.accounts.setup_jobs.lock().map_err(store::error)?;
        let mut job = jobs
            .get(id)
            .ok_or_else(|| DomainStateError::not_found("Login job not found."))?
            .lock()
            .map_err(store::error)?;
        if job.view["owner"] != owner {
            return Err(DomainStateError::bad_request(
                "This login belongs to another window.",
            ));
        }
        if operation == "setupInput" {
            let input = field(params, "input")?;
            job.writer
                .as_mut()
                .ok_or_else(|| DomainStateError::bad_request("This login has finished."))?
                .write_all(format!("{}\r", input.trim()).as_bytes())
                .map_err(store::error)?;
        } else if operation == "setupCancel" {
            job.cancel()?;
        } else {
            job.view["acknowledged"] = json!(true);
            job.view["output"] = json!("");
            job.view["url"] = Value::Null;
        }
    }
    let jobs = state.accounts.setup_jobs.lock().map_err(store::error)?;
    let mut views: Vec<_> = jobs
        .values()
        .filter_map(|job| {
            job.lock()
                .ok()
                .filter(|job| job.view["owner"] == owner)
                .map(|job| job.view.clone())
        })
        .collect();
    views.sort_by_key(|view| view["createdAt"].as_i64().unwrap_or_default());
    Ok(response(views))
}
/// The login ended on another account than the one asked for: the email the browser signed in to
/// (empty when the helper did not say), from `ghostex account-login`'s "signed in to <email>
/// instead of" or xswap's "signed in to a different email".
fn wrong_account(transcript: &str) -> Option<String> {
    const CLAUDE: &str = "The browser signed in to ";
    if let Some(start) = transcript.find(CLAUDE) {
        let rest = &transcript[start + CLAUDE.len()..];
        let email = rest
            .split_whitespace()
            .next()
            .filter(|word| word.contains('@'))
            .unwrap_or_default();
        return Some(email.to_string());
    }
    let lower = transcript.to_lowercase();
    (lower.contains("signed in to a different email")
        || lower.contains("now signed into another account"))
    .then(String::new)
}
/// Where `ghostex account-login --result-file` reports, inside the job's own temporary folder.
const RESULT_FILE: &str = "result.json";
const LOGIN_INCOMPLETE: &str = "Login did not complete. Check the terminal details and try again.";
const HELPER_SILENT: &str =
    "The sign-in finished, but the account wasn't saved. Update it in Settings > Accounts and try again.";

/// A Claude login's outcome from the helper's result file: the saved slot, or the reason it
/// failed. The terminal screen is never read for it.
/// `run` in server/src/ghostex_cli/account_login.rs writes the file.
fn claude_outcome(
    exited_ok: bool,
    report: Option<&Value>,
    selector: Option<&str>,
) -> Result<String, String> {
    if let Some(error) = report
        .and_then(|report| report["error"].as_str())
        .map(str::trim)
        .filter(|error| !error.is_empty())
    {
        return Err(error.to_string());
    }
    let reported = report
        .and_then(|report| report.pointer("/account/number"))
        .and_then(Value::as_u64)
        .map(|number| number.to_string());
    if !exited_ok && reported.is_none() {
        // Cancelled, killed, or the helper crashed before it could say why.
        return Err(LOGIN_INCOMPLETE.to_string());
    }
    selector
        .map(str::to_string)
        .or(reported)
        .ok_or_else(|| HELPER_SILENT.to_string())
}

/// The saved slot an `xswap ... --json` login printed on its terminal: the first JSON object on
/// screen with `account.number`, whatever output follows it.
fn transcript_account_number(transcript: &str) -> Option<String> {
    transcript
        .char_indices()
        .filter(|(_, c)| *c == '{')
        .filter_map(|(index, _)| {
            serde_json::Deserializer::from_str(&transcript[index..])
                .into_iter::<Value>()
                .next()?
                .ok()
        })
        .find_map(|value| {
            value
                .pointer("/account/number")
                .and_then(Value::as_u64)
                .map(|number| number.to_string())
        })
}

/// Every failed Settings login is logged with the reason shown and the end of its terminal, so a
/// banner can be traced without a reproduction (#347 left no gxserver entry at all).
fn log_failure(
    state: &AppState,
    provider: Provider,
    selector: Option<&str>,
    exit_code: Option<u32>,
    error: &str,
    transcript: &str,
) {
    let lines: Vec<&str> = transcript
        .lines()
        .map(str::trim_end)
        .filter(|line| !line.is_empty())
        .collect();
    let tail = lines[lines.len().saturating_sub(12)..].join("\n");
    let _ = state.logger.log(crate::logging::GxserverLogInput {
        level: crate::logging::LogLevel::Warn,
        event: "accounts.loginFailed".to_string(),
        server_id: None,
        request_id: None,
        client: None,
        duration_ms: None,
        error: Some(error.to_string()),
        details: Some(json!({
            "provider": provider,
            "selector": selector,
            "exitCode": exit_code,
            "terminalTail": tail,
        })),
    });
}

fn read_output(mut reader: Box<dyn Read + Send>, job: Arc<Mutex<SetupJob>>) -> String {
    let mut terminal = LoginTerminal::new();
    let mut bytes = [0; 4096];
    loop {
        let Ok(n) = reader.read(&mut bytes) else {
            break;
        };
        if n == 0 {
            break;
        }
        let replies = terminal.process(&bytes[..n]);
        let output = terminal.text();
        if let Ok(mut job) = job.lock() {
            // A login that already exited or was cancelled has no one left to answer.
            if let Some(writer) = job.writer.as_mut().filter(|_| !replies.is_empty()) {
                let _ = writer.write_all(&replies);
            }
            job.view["output"] = json!(output);
            for word in output.split_whitespace() {
                if let Some(start) = word.find("https://") {
                    let candidate = word[start..].trim_end_matches(['\'', '"', '>']);
                    if let Ok(url) = url::Url::parse(candidate) {
                        if matches!(
                            url.host_str(),
                            Some(
                                "claude.com"
                                    | "claude.ai"
                                    | "platform.claude.com"
                                    | "auth.openai.com"
                                    | "chatgpt.com"
                                    | "auth0.openai.com"
                            )
                        ) {
                            job.view["url"] = json!(url.as_str());
                        }
                    }
                }
            }
        }
    }
    terminal.text()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The Details terminal from #347 (Windows 11, ConPTY): Claude's "Login successful.", then
    /// only the first line of the helper's JSON before the pseudo console closed.
    fn reporter_screen() -> String {
        let mut terminal = LoginTerminal::new();
        terminal.process(
            b"Reading saved Claude accounts...\r\n\
              Choose the requested account in the browser.\r\n\
              Login successful.\r\n\
              {\r\n",
        );
        terminal.text()
    }

    #[test]
    fn saved_account_completes_although_the_screen_lost_the_result() {
        let screen = reporter_screen();
        assert!(screen.contains("Login successful.\n{"));
        // The screen alone never says which slot was saved: the old completion check failed here.
        assert_eq!(transcript_account_number(&screen), None);
        let report = json!({"schemaVersion":1,"account":{"number":2,"email":"second@example.com"}});
        assert_eq!(claude_outcome(true, Some(&report), None), Ok("2".into()));
    }

    #[test]
    fn saved_account_completes_when_the_helper_exits_with_an_error() {
        let report = json!({"schemaVersion":1,"account":{"number":2,"email":"second@example.com"}});
        assert_eq!(claude_outcome(false, Some(&report), None), Ok("2".into()));
    }

    #[test]
    fn failed_login_shows_the_helpers_reason() {
        let report = json!({"ok":false,"error":"Claude Swap could not save this login. Check the message above and try again."});
        assert_eq!(
            claude_outcome(false, Some(&report), None),
            Err(
                "Claude Swap could not save this login. Check the message above and try again."
                    .into()
            )
        );
    }

    #[test]
    fn login_without_a_report_says_what_is_known() {
        assert_eq!(
            claude_outcome(false, None, None),
            Err(LOGIN_INCOMPLETE.into())
        );
        assert_eq!(claude_outcome(true, None, None), Err(HELPER_SILENT.into()));
        assert_eq!(claude_outcome(true, None, Some("3")), Ok("3".into()));
    }

    #[test]
    fn wrong_account_is_read_from_the_reported_error() {
        let error = "The browser signed in to other@example.com instead of second@example.com. Run login again and choose the requested account. No saved account was changed.";
        let failure_text = format!("{}\n{error}", reporter_screen());
        assert_eq!(
            wrong_account(&failure_text),
            Some("other@example.com".into())
        );
    }

    #[test]
    fn codex_result_is_found_with_crlf_and_output_after_it() {
        let mut terminal = LoginTerminal::new();
        terminal.process(
            b"Login successful.\r\n{\"schemaVersion\":1,\"account\":{\"number\":3,\"email\":\"c@example.com\"}}\r\nDone.\r\n",
        );
        assert_eq!(
            transcript_account_number(&terminal.text()),
            Some("3".into())
        );
    }
}
