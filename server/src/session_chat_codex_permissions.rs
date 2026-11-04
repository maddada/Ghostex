//! Codex's permission mode, changed by typing its own `/permissions <profile>` into the session.

use super::selection_options::ready;
use super::*;
use crate::session_chat_options::{
    codex_permissions_choice, codex_permissions_command, codex_permissions_on_screen,
    SessionChatOptionEvidence,
};

const FULL_ACCESS_TITLE: &str = "Enable full access?";
const FULL_ACCESS_ACCEPT_ROW: &str = "1. Yes, continue anyway";
const PERMISSIONS_MENU_TITLE: &str = "Update Model Permissions";
const PERMISSIONS_MENU_COMMAND: &str = "/permissions";
const CURRENT_ROW_MARKER: &str = " (current)";
/// How long Codex may stay quiet after taking the command before its menu is asked.
const SILENT_SETTLE: Duration = Duration::from_millis(1_500);

enum Applied {
    Confirmed,
    /// Codex took the command and printed nothing, which it does when the mode is already the
    /// requested one, and also when it printed a refusal.
    Silent,
}

/// Whether Codex's composer still holds `command`, so it has not been submitted yet.
fn command_staged(screen: &str, command: &str) -> bool {
    screen_lines(screen).iter().any(|line| {
        line.trim_start()
            .strip_prefix(CODEX_CURSOR)
            .is_some_and(|text| text.trim() == command)
    })
}

/// The lines of the Codex dialog titled `title` as the dialog itself draws them, newest dialog
/// first found from the bottom.
///
/// CDXC:SessionChat 2026-10-27 WHY: In a wide Ghostex terminal Codex draws its `/permissions` menu and its "Enable full access?" box over the right of the screen, beside the conversation's older lines, so a row such as `› 1. Yes, continue anyway` starts mid-line. Reading rows only at the start of a line never saw the box, and a Full Access pick from the chat stayed open in Codex's confirmation (found testing the desktop app). Each line is read from the dialog's own left edge, two columns left of its title, where Codex puts its row cursor.
fn dialog_lines(screen: &str, title: &str) -> Option<Vec<String>> {
    // Columns matter here, so the lines keep their spacing until the dialog's part is cut out.
    let lines: Vec<String> = screen
        .split('\n')
        .map(crate::session_chat_options::strip_ansi_sgr)
        .collect();
    let (index, column) = lines.iter().enumerate().rev().find_map(|(index, line)| {
        let byte = line.find(title)?;
        Some((index, line[..byte].chars().count().saturating_sub(2)))
    })?;
    Some(
        lines[index..]
            .iter()
            .map(|line| collapse_spaces(&line.chars().skip(column).collect::<String>()))
            .collect(),
    )
}

/// Codex's "Enable full access?" confirmation, open with its accept row selected.
pub(super) fn full_access_confirmation_open(screen: &str) -> bool {
    dialog_lines(screen, FULL_ACCESS_TITLE).is_some_and(|lines| {
        lines.iter().any(|line| {
            line.trim_start()
                .strip_prefix(CODEX_CURSOR)
                .is_some_and(|row| row.trim_start().starts_with(FULL_ACCESS_ACCEPT_ROW))
        })
    })
}

/// The numbered rows of Codex's `/permissions` menu, once it has loaded them.
fn permissions_menu_rows(screen: &str) -> Option<Vec<String>> {
    let lines = dialog_lines(screen, PERMISSIONS_MENU_TITLE)?;
    let rows: Vec<String> = lines[1..]
        .iter()
        .filter_map(|line| {
            let row = line.trim_start();
            let row = row.strip_prefix(CODEX_CURSOR).unwrap_or(row).trim_start();
            let (number, rest) = row.split_once(". ")?;
            (!number.is_empty() && number.chars().all(|ch| ch.is_ascii_digit()))
                .then(|| rest.to_string())
        })
        .collect();
    (!rows.is_empty()).then_some(rows)
}

/// The chat value of the row Codex's `/permissions` menu marks `(current)`, `None` when no row
/// is (a mix of settings none of its rows describe). The outer `None` is a menu not yet drawn.
pub(super) fn permissions_menu_current(screen: &str) -> Option<Option<String>> {
    let rows = permissions_menu_rows(screen)?;
    Some(rows.iter().find_map(|row| {
        let (label, _) = row.split_once(CURRENT_ROW_MARKER)?;
        codex_permissions_choice(label, SessionChatOptionEvidence::Terminal)
            .map(|choice| choice.value)
    }))
}

/// The refusal Codex printed (`■ …`) as the newest line above its composer.
fn newest_refusal(screen: &str) -> Option<String> {
    let lines = screen_lines(screen);
    let composer = lines
        .iter()
        .rposition(|line| line.trim_start().starts_with(CODEX_CURSOR))?;
    lines[..composer]
        .iter()
        .rev()
        .map(|line| line.trim())
        .find(|line| !line.is_empty())
        .and_then(|line| line.strip_prefix("■ "))
        .map(str::to_string)
}

impl PickerDriver<'_> {
    /*
    CDXC:SessionChat 2026-10-27 WHY:
    Codex 0.164 changes a running session's permissions with `/permissions <profile>` (`:ask-for-approval`, `:approve-for-me`, `:danger-full-access`, `:read-only`), the same selection its `/permissions` menu makes, and it is accepted while a turn runs. Full Access always opens Codex's "Enable full access?" confirmation, which is answered with its "Yes, continue anyway" row because the user just chose Full Access in the chat. Codex confirms a change with `• Permission selection requested: <name>` but prints nothing when the mode is already the one asked for, and a refusal is a `■` line, so a quiet answer is settled by reading the `(current)` row of Codex's own menu, never by assuming the change landed.
    SEE-ALSO: `codex_permissions_on_screen` in server/src/session_chat_options/agent_matchers.rs, `codex_permissions` in packages/gx-chat-core/src/menus/option_catalog.rs.
    */
    pub(super) async fn drive_permissions_option(
        &self,
        plan: &CodexPickerPlan,
        agent: SessionChatOptionAgent,
    ) -> Result<(), DomainStateError> {
        let Some(target) = plan.options.permissions.as_deref() else {
            return Ok(());
        };
        let command = codex_permissions_command(target)
            .filter(|_| agent == SessionChatOptionAgent::Codex)
            .ok_or_else(|| {
                unsupported_selection("This agent does not offer that permission mode.")
            })?;
        let screen = self.option_screen(agent).await?;
        if codex_permissions_on_screen(&screen).is_some_and(|current| current.value == target) {
            return Ok(());
        }
        self.type_option_command(agent, &command).await?;
        let mut answered_confirmation = false;
        let mut quiet_since: Option<std::time::Instant> = None;
        let applied = loop {
            let step = self
                .wait_for("permission change", |screen| {
                    if codex_permissions_on_screen(screen)
                        .is_some_and(|current| current.value == target)
                    {
                        return Some(Some(Applied::Confirmed));
                    }
                    if !answered_confirmation && full_access_confirmation_open(screen) {
                        return Some(None);
                    }
                    if ready(screen, agent)
                        && !command_staged(screen, &command)
                        && !screen.contains(FULL_ACCESS_TITLE)
                    {
                        let since = *quiet_since.get_or_insert_with(std::time::Instant::now);
                        if since.elapsed() >= SILENT_SETTLE {
                            return Some(Some(Applied::Silent));
                        }
                    } else {
                        quiet_since = None;
                    }
                    None
                })
                .await?;
            match step {
                Some(applied) => break applied,
                None => {
                    answered_confirmation = true;
                    self.write(CODEX_SUBMIT).await?;
                }
            }
        };
        if matches!(applied, Applied::Confirmed) {
            return Ok(());
        }
        let refusal = self.capture().await.as_deref().and_then(newest_refusal);
        if self.permissions_menu_current(agent).await?.as_deref() == Some(target) {
            return Ok(());
        }
        let label = codex_permissions_choice(target, SessionChatOptionEvidence::Terminal)
            .map(|choice| choice.label)
            .unwrap_or_default();
        log_picker(
            LogLevel::Warn,
            "sessionChatCodexPermissionsNotApplied",
            json!({
                "projectId": self.project_id,
                "sessionId": self.session_id,
                "target": target,
                "confirmedFullAccess": answered_confirmation,
                "refusal": refusal,
            }),
            None,
        );
        match refusal {
            Some(reason) if reason.starts_with("Wait for") => Err(agent_busy(reason)),
            Some(reason) => Err(unsupported_selection(reason)),
            None => Err(agent_busy(format!(
                "Codex did not switch to {label}; the choice remains queued."
            ))),
        }
    }

    /// Opens Codex's `/permissions` menu, reads its `(current)` row and closes it again.
    async fn permissions_menu_current(
        &self,
        agent: SessionChatOptionAgent,
    ) -> Result<Option<String>, DomainStateError> {
        self.type_option_command(agent, PERMISSIONS_MENU_COMMAND)
            .await?;
        let current = self
            .wait_for("read Codex permissions menu", permissions_menu_current)
            .await;
        self.write(claude_effort_slider::CLAUDE_ESCAPE).await?;
        self.wait_for("close Codex permissions menu", |screen| {
            (ready(screen, agent) && !screen.contains(PERMISSIONS_MENU_TITLE)).then_some(())
        })
        .await?;
        current
    }
}
