use std::{
    path::{Path, PathBuf},
    process::Stdio,
    time::{Duration, Instant},
};

use serde_json::Value;

use crate::session_chat_send::{SessionChatSendError, SessionChatSendFailure};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BackgroundSession {
    cli: PathBuf,
    config_dir: PathBuf,
    conversation_id: String,
    id: String,
    process_id: i64,
}

pub(super) fn resolve(
    home: &Path,
    process_id: i64,
    conversation_id: &str,
) -> Option<BackgroundSession> {
    let config_dir = super::process_login::configured_home(process_id, "CLAUDE_CONFIG_DIR")?;
    let cli = super::helpers::probe_executable(home, "claude")?;
    let mut command = std::process::Command::new(&cli);
    command
        .args(["agents", "--json"])
        .env("HOME", home)
        .env("CLAUDE_CONFIG_DIR", &config_dir);
    // Older Claude versions without background agents only need their terminal client interrupted.
    let output = crate::agent_hooks::probing::run_command_stdout_with_timeout(
        command,
        Duration::from_secs(3),
    )?;
    let roster: Value = serde_json::from_str(&output).ok()?;
    let row = roster.as_array()?.iter().find(|row| {
        row["kind"] == "background" && row["sessionId"].as_str() == Some(conversation_id)
    })?;
    Some(BackgroundSession {
        cli,
        config_dir,
        conversation_id: conversation_id.to_string(),
        id: row["id"]
            .as_str()
            .filter(|id| !id.is_empty() && !id.starts_with('-'))?
            .to_string(),
        process_id: row["pid"].as_i64().filter(|pid| *pid > 0)?,
    })
}

fn failure(message: impl Into<String>) -> SessionChatSendError {
    SessionChatSendError {
        failure: SessionChatSendFailure::ComposerNotReady,
        message: message.into(),
    }
}

fn check_cancelled(
    cancelled: &(dyn Fn() -> bool + Send + Sync),
) -> Result<(), SessionChatSendError> {
    if cancelled() {
        Err(SessionChatSendError {
            failure: SessionChatSendFailure::NotAttempted,
            message: "The account switch was cancelled.".to_string(),
        })
    } else {
        Ok(())
    }
}

impl BackgroundSession {
    async fn run(&self, home: &Path, args: &[&str]) -> Result<Vec<u8>, SessionChatSendError> {
        let mut command = tokio::process::Command::new(&self.cli);
        command
            .args(args)
            .env("HOME", home)
            .env("CLAUDE_CONFIG_DIR", &self.config_dir)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);
        crate::platform::process::NoConsoleWindow::no_console_window(&mut command);
        let output = tokio::time::timeout(Duration::from_secs(8), command.output())
            .await
            .map_err(|_| {
                failure("Claude did not finish checking or stopping the background conversation.")
            })?
            .map_err(|error| {
                failure(format!(
                    "Could not run Claude's background-session command: {error}"
                ))
            })?;
        if !output.status.success() {
            return Err(failure("Claude could not check or stop the background conversation. Check the terminal before retrying."));
        }
        Ok(output.stdout)
    }

    async fn running(&self, home: &Path) -> Result<bool, SessionChatSendError> {
        let output = self.run(home, &["agents", "--json"]).await?;
        let roster: Value = serde_json::from_slice(&output)
            .map_err(|_| failure("Claude returned an unreadable background-session list."))?;
        let rows = roster
            .as_array()
            .ok_or_else(|| failure("Claude returned an invalid background-session list."))?;
        let Some(row) = rows.iter().find(|row| row["id"].as_str() == Some(&self.id)) else {
            return Ok(false);
        };
        if row["kind"] != "background"
            || row["sessionId"].as_str() != Some(&self.conversation_id)
            || row["pid"].as_i64() != Some(self.process_id)
        {
            return Err(failure(
                "The Claude background conversation changed before switching. Retry the switch.",
            ));
        }
        Ok(true)
    }
}

/// Stops the independently running conversation before the caller interrupts its attached client.
/// The caller has already captured the exact resume command, since stopping can emit a dashboard hook with a different conversation id.
pub(crate) async fn stop(
    background: &BackgroundSession,
    home: &Path,
    cancelled: &(dyn Fn() -> bool + Send + Sync),
) -> Result<(), SessionChatSendError> {
    check_cancelled(cancelled)?;
    if background.running(home).await? {
        check_cancelled(cancelled)?;
        background.run(home, &["stop", &background.id]).await?;
        let deadline = Instant::now() + Duration::from_secs(8);
        while background.running(home).await? {
            check_cancelled(cancelled)?;
            if Instant::now() >= deadline {
                return Err(failure(
                    "The Claude background conversation has not stopped yet.",
                ));
            }
            tokio::time::sleep(Duration::from_millis(150)).await;
        }
    }
    Ok(())
}
