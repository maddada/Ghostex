//! CDXC:AgentProviders 2026-10-13 DECISION:
//! User (Q39): "Update now" on Codex's update prompt updates the copy sessions actually run, using Ghostex's own updater.
//! WHY: Codex picks its updater from how the running binary looks to it, not from which `codex` a terminal resolves. On a Mac whose `codex` is mise's (`~/.local/bin/codex` -> mise shim -> `npm:@openai/codex`), Codex ran `pnpm add -g @openai/codex`, which updated a pnpm copy no session runs, so the prompt came back on every launch. The chat card now asks the agent CLI updater (agent_cli/, the Agents page and `ghostex agent-cli update`) which install a new terminal's `codex` is and updates that one with the method that installed it; Codex itself only skips the prompt, and the session restarts Codex with the sleep and wake the existing restart uses once Codex is idle. Codex's own updater runs only when it provably writes the same binary; otherwise the card says which copy runs and how to update it.
//! SEE-ALSO: session_chat_codex_dialog.rs (the card and its answer), agent_cli/endpoint.rs (`read_agent`, `update_and_wait`), session_chat_agent_restart.rs (the restart cycle).

use serde_json::Value;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use crate::agent_cli::latest::{is_newer, version_in};
use crate::domain::DomainStateError;
use crate::server::AppState;
use crate::session_chat_codex_dialog::{codex_update_method, CodexUpdatePrompt};
use crate::session_chat_send::SessionChatSendTarget;

/// What "Update now" does for the copy of Codex this session runs.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum UpdatePlan {
    /// Ghostex's updater, with the method that installed the copy.
    Ghostex { method_id: String, command: String },
    /// The copy on disk is already the prompt's version (another session updated it): only restart.
    AlreadyCurrent { version: String },
    /// Codex's own updater writes the same binary, so Codex runs it as it would in a terminal.
    CodexOwn,
    /// Nothing Ghostex can run safely: the card shows this.
    Explain(String),
}

/// What runs after Codex has skipped its prompt.
pub(crate) struct PendingUpdate {
    plan: UpdatePlan,
    latest: String,
    command: String,
}

/// Decides from the Agents page state of `codex` (agent_cli `read_agent`). `codex_targets_copy` answers
/// whether Codex's own updater writes the binary at the given path.
pub(crate) fn plan(
    state: &Value,
    prompt: &CodexUpdatePrompt,
    codex_targets_copy: impl FnOnce(&str) -> bool,
) -> UpdatePlan {
    let Some(executable) = state["executablePath"].as_str() else {
        return UpdatePlan::Explain(
            "Ghostex can't find the codex command new terminals run, so it did not update anything. Install Codex from Settings > Agents, then start it again.".to_string(),
        );
    };
    if let Some(version) = state["version"].as_str().and_then(version_in) {
        if !is_newer(&prompt.latest, &version) {
            return UpdatePlan::AlreadyCurrent { version };
        }
    }
    let detected = state["detectedMethodId"]
        .as_str()
        .filter(|method| *method != "manual");
    if let Some(method_id) = detected {
        let method = state["methods"]
            .as_array()
            .and_then(|methods| methods.iter().find(|method| method["id"] == method_id));
        let label = method
            .and_then(|method| method["label"].as_str())
            .unwrap_or(method_id);
        return match (
            method.and_then(|method| method["command"].as_str()),
            method.and_then(|method| method["unavailableReason"].as_str()),
        ) {
            (Some(command), None) => UpdatePlan::Ghostex {
                method_id: method_id.to_string(),
                command: command.to_string(),
            },
            (_, reason) => UpdatePlan::Explain(format!(
                "This session runs Codex from {executable}, installed with {label}, and Ghostex can't update it here{}. Update it with {label}, then start Codex again.",
                reason.map(|reason| format!(": {reason}")).unwrap_or_default()
            )),
        };
    }
    if codex_targets_copy(executable) {
        return UpdatePlan::CodexOwn;
    }
    UpdatePlan::Explain(format!(
        "This session runs Codex from {executable}. Ghostex can't tell which tool installed that copy, and Codex's own update ({}) would change a different copy, so nothing was updated. Update {executable} with the tool that installed it, then start Codex again.",
        prompt.command
    ))
}

/// `None` lets Codex run its own updater (the answer goes through unchanged). `Some` means Codex only
/// skips its prompt and `PendingUpdate::spawn` does the rest. `Err` keeps the card with the reason.
pub(crate) async fn plan_for_prompt(
    state: &AppState,
    screen: &str,
) -> Result<Option<PendingUpdate>, DomainStateError> {
    let invalid = |message: String| DomainStateError {
        code: "invalidState",
        message,
    };
    let prompt =
        crate::session_chat_codex_dialog::detect_codex_update_prompt(screen).ok_or_else(|| {
            invalid("Codex's dialog changed. Review the current choices and try again.".into())
        })?;
    let home = state.paths.agent_config_home_dir().to_path_buf();
    let agent = crate::agent_cli::endpoint::read_agent(&home, "codex").await?;
    let codex_method = codex_update_method(&prompt.command);
    let plan = {
        let agent = agent.clone();
        let latest = prompt.latest.clone();
        let command = prompt.command.clone();
        tokio::task::spawn_blocking(move || {
            let prompt = CodexUpdatePrompt {
                current: String::new(),
                latest,
                command,
            };
            plan(&agent, &prompt, |executable| {
                codex_method.is_some_and(|method| writes_binary(method, executable, &home))
            })
        })
        .await
        .map_err(|error| invalid(error.to_string()))?
    };
    match plan {
        UpdatePlan::CodexOwn => Ok(None),
        UpdatePlan::Explain(message) => Err(invalid(message)),
        plan => Ok(Some(PendingUpdate {
            plan,
            latest: prompt.latest,
            command: prompt.command,
        })),
    }
}

/// Whether `method`'s global install of `codex` is the binary at `executable`. Only the package managers
/// that can say where their global commands go are checked; anything else is not provable.
fn writes_binary(method: &str, executable: &str, home: &Path) -> bool {
    let Some(dir) = global_bin_dir(method, home) else {
        return false;
    };
    let canonical = |path: &Path| std::fs::canonicalize(path).unwrap_or_else(|_| path.into());
    let name = Path::new(executable)
        .file_name()
        .map(PathBuf::from)
        .unwrap_or_else(|| "codex".into());
    canonical(&dir.join(name)) == canonical(Path::new(executable))
}

fn global_bin_dir(method: &str, home: &Path) -> Option<PathBuf> {
    let (tool, args): (&str, &[&str]) = match method {
        "npm" => ("npm", &["prefix", "-g"]),
        "pnpm" => ("pnpm", &["bin", "-g"]),
        "bun" => ("bun", &["pm", "bin", "-g"]),
        "brew" => ("brew", &["--prefix"]),
        _ => return None,
    };
    let binary = crate::agent_cli::process::resolve(tool, home)?;
    let mut command = crate::platform::process::background_command(&binary);
    command.args(args).current_dir(home).env("HOME", home);
    #[cfg(not(windows))]
    command.env(
        "PATH",
        crate::agent_hooks::probing::normalize_gxserver_process_path(
            std::env::var("PATH").ok().as_deref(),
            home,
        ),
    );
    let output = crate::agent_hooks::probing::run_command_stdout_with_timeout(
        command,
        Duration::from_secs(5),
    )?;
    let dir = PathBuf::from(output.trim());
    Some(match method {
        "npm" if !cfg!(windows) => dir.join("bin"),
        "brew" => dir.join("bin"),
        _ => dir,
    })
}

/// How long a second "Update now" waits for an update already running.
const UPDATE_WAIT: Duration = Duration::from_secs(20 * 60);

impl PendingUpdate {
    pub(crate) fn spawn(self, state: &AppState, target: &SessionChatSendTarget) {
        let state = state.clone();
        let project_id = target.project_id.clone();
        let session_id = target.session_id.clone();
        tokio::spawn(async move {
            let title = format!("Update Codex to {}", self.latest);
            let mut row: Option<String> = None;
            let mut show = |output: &str| {
                row = Some(
                    crate::session_chat_app_command::show_session_chat_app_command_output(
                        &project_id,
                        &session_id,
                        row.as_deref(),
                        &title,
                        output,
                    ),
                );
                crate::session_chat_options::session_chat_terminal_notice_publisher(
                    &state,
                    &project_id,
                    &session_id,
                )();
            };
            let home = state.paths.agent_config_home_dir().to_path_buf();
            let PendingUpdate {
                mut plan,
                latest,
                command: prompt_command,
            } = self;
            loop {
                match plan {
                    UpdatePlan::Ghostex { method_id, command } => {
                        /*
                        CDXC:AgentProviders 2026-10-14 DECISION:
                        User (Q74): a second Update now press on another session while an update runs waits for the running update and then re-plans, instead of failing. Once the other update ends, the copy is read again: a finished update leaves only the restart, a failed one is tried again with whatever method now owns the copy.
                        */
                        if crate::agent_cli::endpoint::job_active(&home, "codex") {
                            show("Another update of Codex is running. Waiting for it to finish, then checking this session's copy again.");
                            let started = Instant::now();
                            while crate::agent_cli::endpoint::job_active(&home, "codex") {
                                if started.elapsed() > UPDATE_WAIT {
                                    show("The other update of Codex did not finish within 20 minutes, so nothing was updated here. Choose Update now again later.");
                                    return;
                                }
                                tokio::time::sleep(Duration::from_secs(1)).await;
                            }
                            plan = replan(&home, &latest, &prompt_command).await;
                            continue;
                        }
                        show(&format!(
                            "Updating the copy of Codex this session runs: {command}"
                        ));
                        match crate::agent_cli::endpoint::update_and_wait(
                            &state.paths,
                            "codex",
                            &method_id,
                        )
                        .await
                        {
                            Ok(agent) => {
                                let version = agent["version"].as_str().and_then(version_in);
                                show(&format!(
                                    "Updated Codex to {}. Restarting Codex here once it is idle.",
                                    version.as_deref().unwrap_or(&latest)
                                ));
                                break;
                            }
                            // Another press or the Agents page started its update first.
                            Err(_) if crate::agent_cli::endpoint::job_active(&home, "codex") => {
                                plan = UpdatePlan::Ghostex { method_id, command };
                            }
                            Err(reason) => {
                                show(&format!("The update did not finish, so Codex keeps running its current version.\n{reason}"));
                                return;
                            }
                        }
                    }
                    UpdatePlan::AlreadyCurrent { version } => {
                        show(&format!("Codex {version} is already installed for this session. Restarting Codex here once it is idle so it runs that version."));
                        break;
                    }
                    UpdatePlan::Explain(message) => {
                        show(&message);
                        return;
                    }
                    UpdatePlan::CodexOwn => return,
                }
            }
            let outcome = crate::session_chat_agent_restart::restart_when_idle(
                &state,
                &project_id,
                &session_id,
                "Codex",
                "codexUpdate",
            )
            .await;
            show(&match outcome {
                Ok(()) => "Codex restarted on the new version.".to_string(),
                Err(reason) => format!(
                    "The new version is installed, but Codex was not restarted: {reason}. It runs the new version the next time Codex starts."
                ),
            });
        });
    }
}

/// The plan again from the copy as it is now. Codex's own updater is not offered here: Codex has
/// already skipped its prompt.
async fn replan(home: &Path, latest: &str, command: &str) -> UpdatePlan {
    let prompt = CodexUpdatePrompt {
        current: String::new(),
        latest: latest.to_string(),
        command: command.to_string(),
    };
    match crate::agent_cli::endpoint::read_agent(home, "codex").await {
        Ok(agent) => plan(&agent, &prompt, |_| false),
        Err(error) => UpdatePlan::Explain(format!(
            "Ghostex could not read this session's copy of Codex after the other update: {}",
            error.message
        )),
    }
}
