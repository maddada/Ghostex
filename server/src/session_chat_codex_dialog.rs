//! Codex-owned dialogs projected into the shared chat notice card.

use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};

use crate::domain::DomainStateError;
use crate::session_chat_options::{normalize_spaces, strip_ansi_sgr};
use crate::session_chat_send::{
    capture_session_terminal_text, execute_session_chat_send, SessionChatSendStep,
    SessionChatSendTarget, SESSION_CHAT_INTERRUPT,
};

use crate::session_chat_terminal_dialog::{TerminalDialog, TerminalDialogRow};

fn clean(line: &str) -> &str {
    line.trim().trim_matches(['│', '┃', '▌']).trim()
}

fn left_column(line: &str) -> &str {
    line.split("          ").next().unwrap_or_default().trim()
}

/// Onboarding screens (sign-in, Amazon Bedrock setup) mark the highlighted row with an ASCII `>`
/// where every other Codex list uses `›`.
const ROW_MARKERS: [char; 2] = ['›', '>'];

fn row(line: &str) -> Option<TerminalDialogRow> {
    let line = clean(line);
    let selected = line.starts_with(ROW_MARKERS);
    let line = line.strip_prefix(ROW_MARKERS).unwrap_or(line).trim_start();
    let (number, label) = line.split_once(". ")?;
    let number = number.parse::<u32>().ok()?;
    if number == 0 || label.trim().is_empty() {
        return None;
    }
    let (label, description) = label
        .split_once("  ")
        .map(|(label, detail)| {
            (
                label,
                Some(detail.split_whitespace().collect::<Vec<_>>().join(" ")),
            )
        })
        .unwrap_or((label, None));
    Some(TerminalDialogRow {
        number,
        label: label.trim().to_string(),
        description,
        selected,
    })
}

/// One option of an unnumbered picker: `› Reset all memories  Delete local memory files…`.
fn unnumbered_row(line: &str, number: u32) -> TerminalDialogRow {
    let line = clean(line);
    let selected = line.starts_with('›');
    let text = line.trim_start_matches('›').trim_start();
    let (label, description) = text
        .split_once("  ")
        .map(|(label, detail)| {
            (
                label.trim(),
                Some(detail.split_whitespace().collect::<Vec<_>>().join(" ")),
            )
        })
        .unwrap_or((text, None));
    TerminalDialogRow {
        number,
        label: label.to_string(),
        description,
        selected,
    }
}

/*
CDXC:AgentScreenDetection 2026-09-27 WHY:
Codex's safety-buffering menu ("Giving this request a little extra thought"), its precaution stop and pause screens, and similar pickers print no key-hint footer, so the footed-dialog parser skipped them and chat could only say "Codex is waiting for a choice".
A menu without a footer is the highlighted row near the bottom with no Codex input box on screen, the same evidence the blocking fallback (`detect_codex_blocking_screen`) accepts; at most a short note may follow its rows.
*/
fn footerless_menu(text: &str, lines: &[String], end: usize) -> Option<()> {
    let selected = (end.saturating_sub(8)..=end)
        .rev()
        .find(|&i| row(&lines[i]).is_some_and(|row| row.selected))?;
    let composer_follows = lines[selected + 1..=end]
        .iter()
        .any(|line| clean(line).starts_with(['›', '»']) && row(line).is_none());
    if composer_follows
        || crate::session_chat_composer::session_chat_composer_input("codex", text).is_some()
    {
        return None;
    }
    let last_row = (selected..=end).rev().find(|&i| row(&lines[i]).is_some())?;
    let rows = lines[selected.saturating_sub(12)..=last_row]
        .iter()
        .filter(|line| row(line).is_some())
        .count();
    let note_lines = lines[last_row + 1..=end]
        .iter()
        .filter(|line| !line.trim().is_empty())
        .count();
    (rows >= 2 && note_lines <= 3).then_some(())
}

const DIRECTORY_TRUST_ID_PREFIX: &str = "codex-directory-trust:";
/// Codex 0.156's "Folder access" trust dialog. A separate prefix because it
/// is answered differently from the onboarding one (see `payload`).
const FOLDER_ACCESS_TRUST_ID_PREFIX: &str = "codex-folder-access-trust:";
const UPDATE_PROMPT_ID_PREFIX: &str = "codex-update-prompt:";

/// Codex's "Update available" prompt as Codex drew it: the running version, the newest one, and the
/// command Codex's own updater would run.
pub(crate) struct CodexUpdatePrompt {
    pub current: String,
    pub latest: String,
    pub command: String,
}

/// The install method (an `agent_cli` catalog method id) Codex's own updater would use, read from the
/// command it prints (Codex tui/src/update_action.rs).
pub(crate) fn codex_update_method(command: &str) -> Option<&'static str> {
    if command.contains("brew ") {
        Some("brew")
    } else if command.contains("pnpm ") {
        Some("pnpm")
    } else if command.contains("npm ") {
        Some("npm")
    } else if command.contains("bun ") {
        Some("bun")
    } else if command.contains("vp ") {
        Some("vp")
    } else if command.contains("install.sh") || command.contains("install.ps1") {
        Some("native")
    } else {
        None
    }
}

fn update_prompt_parts(dialog: &TerminalDialog) -> Option<CodexUpdatePrompt> {
    // "✨ Update available! 0.1.0 -> 0.2.0" before Codex 0.156, "Update available · 0.156.0 → 0.156.1" since.
    let (_, versions) = dialog
        .title
        .split_once("Update available!")
        .or_else(|| dialog.title.split_once("Update available ·"))?;
    let (current, latest) = versions
        .split_once("->")
        .or_else(|| versions.split_once('→'))
        .map(|(current, latest)| (current.trim(), latest.trim()))
        .filter(|(current, latest)| !current.is_empty() && !latest.is_empty())?;
    let update_row = dialog
        .rows
        .first()
        .filter(|row| row.label.starts_with("Update now"))?;
    if dialog.rows.len() < 2
        || dialog.rows[1..]
            .iter()
            .any(|row| !row.label.starts_with("Skip"))
    {
        return None;
    }
    // A narrow screen wraps the command into the row's continuation lines.
    let command = format!(
        "{} {}",
        update_row.label,
        update_row.description.as_deref().unwrap_or_default()
    )
    .split_whitespace()
    .collect::<Vec<_>>()
    .join(" ");
    // "Update now (runs `pnpm add -g @openai/codex`)"
    let command = match (command.find('`'), command.rfind('`')) {
        (Some(start), Some(end)) if end > start => command[start + 1..end].to_string(),
        _ => command
            .strip_prefix("Update now")
            .unwrap_or(&command)
            .trim()
            .to_string(),
    };
    Some(CodexUpdatePrompt {
        current: current.to_string(),
        latest: latest.to_string(),
        command,
    })
}

/// The update prompt on this screen, before the card rewords it.
pub(crate) fn detect_codex_update_prompt(text: &str) -> Option<CodexUpdatePrompt> {
    detect_codex_dialog_as_drawn(text).and_then(|dialog| update_prompt_parts(&dialog))
}

/// CDXC:AgentScreenDetection 2026-10-13 DECISION:
/// User (Q39): "Update now" updates the copy sessions actually run, using Ghostex's own updater. The card names both versions and says Ghostex updates the copy this session runs and restarts Codex; it never shows the raw install command, which stays readable under the card's terminal output, since the notice keeps the real screen for this dialog. Supersedes the 2026-09-11 wording that described Codex's own installer, which on a mise-managed Codex installed through pnpm into a copy no session ran, so the prompt came back on every launch (session_chat_codex_update.rs).
/// SEE-ALSO: Codex tui/src/update_prompt.rs (the modal) and tui/src/update_action.rs (the commands it prints); server/src/session_chat_notice/classify.rs keeps the screen tail.
fn update_prompt_dialog(mut dialog: TerminalDialog) -> TerminalDialog {
    let Some(CodexUpdatePrompt {
        current, latest, ..
    }) = update_prompt_parts(&dialog)
    else {
        return dialog;
    };
    // CDXC:AgentScreenDetection 2026-09-23 WHY:
    // Chat/terminal resizing reflows this prompt without changing its choices; raw screen hashes rejected valid answers. Retain the versions, full installer command and numbered choices in the identity, while the fresh capture supplies the current highlight.
    let choices: Vec<_> = dialog
        .rows
        .iter()
        .map(|row| {
            (
                row.number,
                format!(
                    "{} {}",
                    row.label,
                    row.description.as_deref().unwrap_or_default()
                )
                .split_whitespace()
                .collect::<Vec<_>>()
                .join(" "),
            )
        })
        .collect();
    let identity = json!([current, latest, choices]).to_string();
    dialog.id = format!(
        "{UPDATE_PROMPT_ID_PREFIX}{}",
        hex::encode(Sha256::digest(identity.as_bytes()))
    );
    dialog.title = format!("Update Codex to {latest}?");
    dialog.body = format!(
        "This session runs Codex {current}. Update now has Ghostex update the copy of Codex this session runs to {latest}, with the tool that installed it, then restart Codex here on the same conversation."
    );
    dialog.footer = "Choose an option to continue.".to_string();
    dialog.actions.clear();
    dialog.rows[0].label = "Update now".to_string();
    dialog.rows[0].description = None;
    if dialog.rows[1].label == "Skip" {
        dialog.rows[1].label = "Skip for now".to_string();
    }
    dialog
}

/// CDXC:AgentScreenDetection 2026-09-09 WHY:
/// Codex's extended-thinking menu has an informational footer instead of the usual Escape/Enter hints, so the generic dialog parser misses its choices.
/// The request keeps running; expose its two rows without adding a cancel action that would interrupt it.
fn extended_thinking_dialog(lines: &[String]) -> Option<TerminalDialog> {
    let heading = lines
        .iter()
        .rposition(|line| clean(line).starts_with("Our systems are thinking"))?;
    let content = &lines[heading..];
    let first_row = content.iter().position(|line| row(line).is_some())?;
    let title = content[..first_row]
        .iter()
        .map(|line| clean(line))
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if title != "Our systems are thinking a bit more about this request before responding." {
        return None;
    }
    let rows: Vec<_> = content.iter().filter_map(|line| row(line)).collect();
    if rows.len() != 2
        || rows[0].number != 1
        || rows[0].label != "Dismiss and keep waiting"
        || rows[1].number != 2
        || rows[1].label != "Learn more"
        || rows.iter().filter(|row| row.selected).count() != 1
    {
        return None;
    }
    let last_row = content.iter().rposition(|line| row(line).is_some())?;
    let remaining = &content[last_row + 1..];
    // A returned composer means this menu is only old terminal output.
    if remaining.iter().any(|line| clean(line).starts_with('›')) {
        return None;
    }
    let footer = remaining
        .iter()
        .map(|line| clean(line))
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if !footer.starts_with("No action is required. Codex will keep waiting") {
        return None;
    }
    let identity = serde_json::to_string(&(&title, &rows, &footer)).ok()?;
    Some(TerminalDialog {
        id: hex::encode(Sha256::digest(identity.as_bytes())),
        title,
        body: String::new(),
        footer,
        rows,
        input: None,
        input_value: String::new(),
        actions: Vec::new(),
        side_question: None,
        blocks: None,
        picker: None,
    })
}

/// CDXC:AgentScreenDetection 2026-09-06 WHY:
/// Codex redraws the directory heading into scrollback during onboarding, so only the final heading belongs to the live trust dialog.
/// Its trust shortcut selects Yes but requires Enter to commit; treating it like the ordinary numbered menus leaves the dialog open.
fn directory_trust_dialog(content: &[&str]) -> Option<TerminalDialog> {
    let heading = content.iter().position(|line| !clean(line).is_empty())?;
    let path_start = clean(content[heading]).strip_prefix("> You are in ")?;
    let path_end = (heading + 1..content.len()).find(|&i| clean(content[i]).is_empty())?;
    let folder = std::iter::once(path_start)
        .chain(
            content[heading + 1..path_end]
                .iter()
                .map(|line| clean(line)),
        )
        .collect::<String>();
    let first_row = content.iter().position(|line| row(line).is_some())?;
    let mut rows: Vec<_> = content.iter().filter_map(|line| row(line)).collect();
    if rows.len() != 2
        || rows[0].number != 1
        || rows[0].label != "Yes, continue"
        || rows[1].number != 2
        || rows[1].label != "No, quit"
        || rows.iter().filter(|row| row.selected).count() != 1
        || path_end >= first_row
    {
        return None;
    }
    let context = content[path_end..first_row]
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    if !context.contains("Do you trust the contents of this directory?") {
        return None;
    }
    let context = context.replace(
        "Do you trust the contents of this directory? Working with untrusted contents comes with higher risk of prompt injection. Trusting the directory allows project-local config, hooks, and exec policies to load.",
        "Only continue if you trust this folder's contents. Codex will load its local configuration, hooks, and execution policies. Untrusted content can contain instructions that manipulate the agent.",
    );
    let last_row = content.iter().rposition(|line| row(line).is_some())?;
    let error = content[last_row + 1..]
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    let identity = serde_json::to_string(&(&folder, &context, &rows, &error)).ok()?;
    rows[0].label = "Trust and continue".to_string();
    rows[1].label = "Quit Codex".to_string();
    let name = folder
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()?;
    Some(TerminalDialog {
        id: format!(
            "{DIRECTORY_TRUST_ID_PREFIX}{}",
            hex::encode(Sha256::digest(identity.as_bytes()))
        ),
        title: format!("Trust folder \"{name}\"?"),
        body: format!("{folder}\n\n{context}\n\n{error}")
            .trim()
            .to_string(),
        footer: "Choose an option to continue.".to_string(),
        rows,
        input: None,
        input_value: String::new(),
        actions: Vec::new(),
        side_question: None,
        blocks: None,
        picker: None,
    })
}

/// CDXC:AgentScreenDetection 2026-09-26 WHY:
/// Codex 0.156 replaced the onboarding trust screen with a "Folder access" list (a folder path, a "Trust this folder?" sentence, rows "Trust and continue" and "Quit", footer "enter continue · esc quit"). Unrecognized, it surfaced as a generic Codex dialog, so chat never treated it as a trust prompt.
/// Its list wraps, so the onboarding answer (Up, then Enter) would land on Quit: rows are reached by moving from the captured highlight like the other numbered menus.
fn folder_access_trust_dialog(content: &[&str]) -> Option<TerminalDialog> {
    let heading = content.iter().position(|line| !clean(line).is_empty())?;
    if clean(content[heading]) != "Folder access" {
        return None;
    }
    let path_end = (heading + 1..content.len()).find(|&i| clean(content[i]).is_empty())?;
    let folder = content[heading + 1..path_end]
        .iter()
        .map(|line| clean(line))
        .collect::<String>();
    if folder.is_empty() {
        return None;
    }
    let first_row = content.iter().position(|line| row(line).is_some())?;
    let mut rows: Vec<_> = content.iter().filter_map(|line| row(line)).collect();
    if rows.len() != 2
        || rows[0].number != 1
        || rows[0].label != "Trust and continue"
        || rows[1].number != 2
        || rows[1].label != "Quit"
        || rows.iter().filter(|row| row.selected).count() != 1
        || path_end >= first_row
    {
        return None;
    }
    let context = content[path_end..first_row]
        .iter()
        .map(|line| clean(line))
        .collect::<Vec<_>>()
        .join(" ")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    // In a subfolder of a repository Codex adds a note that trust applies to
    // the repository root before this sentence.
    if !context.contains("Trust this folder?") {
        return None;
    }
    let identity = serde_json::to_string(&(&folder, &context, &rows)).ok()?;
    rows[1].label = "Quit Codex".to_string();
    let name = folder
        .trim_end_matches(['/', '\\'])
        .rsplit(['/', '\\'])
        .next()?;
    Some(TerminalDialog {
        id: format!(
            "{FOLDER_ACCESS_TRUST_ID_PREFIX}{}",
            hex::encode(Sha256::digest(identity.as_bytes()))
        ),
        title: format!("Trust folder \"{name}\"?"),
        body: format!("{folder}\n\n{context}"),
        footer: "Choose an option to continue.".to_string(),
        rows,
        input: None,
        input_value: String::new(),
        actions: Vec::new(),
        side_question: None,
        blocks: None,
        picker: None,
    })
}

/// CDXC:AgentScreenDetection 2026-10-17 WHY:
/// Codex 0.163 draws its confirmations (`SelectionViewParams::confirmation()`: /permissions' "Enable full access?", goal, worktree, plugin and daemon confirmations) as a dialog at most 72 columns wide centred over the screen, while its composer stays drawn below it (tui/src/bottom_pane/view_stack.rs `dialog_area`). The footed-dialog parser looks for the footer at the bottom of the screen, so these never reached chat and Codex waited in the terminal. The dialog's own columns are cut out and parsed like any other dialog: Codex clears only its rectangle, so transcript text either side of it stays out, and the climb stops at the first line that is not the dialog's.
fn centered_dialog_text(text: &str) -> Option<String> {
    const DIALOG_WIDTH: usize = 72;
    let lines: Vec<Vec<char>> = text
        .lines()
        .rev()
        .take(160)
        .map(|line| {
            normalize_spaces(&strip_ansi_sgr(line))
                .trim_end()
                .chars()
                .collect()
        })
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    let slice = |line: &[char], from: usize| -> String {
        line.get(from..line.len().min(from + DIALOG_WIDTH))
            .map(|cells| cells.iter().collect::<String>().trim_end().to_string())
            .unwrap_or_default()
    };
    // The footer's hint starts two columns in from the dialog's left edge, past an indent that a
    // bottom-pane view (column 2) never has.
    let (footer, left) = (lines.len().saturating_sub(40)..lines.len())
        .rev()
        .find_map(|index| {
            let line = &lines[index];
            (4..line.len())
                .find(|&column| {
                    line[column] != ' '
                        && line[column - 1] == ' '
                        && line[column - 2] == ' '
                        && crate::session_chat_codex_blocking::is_codex_modal_footer(&slice(
                            line, column,
                        ))
                })
                .map(|column| (index, column - 2))
        })?;
    // Only an overlay leaves Codex's composer drawn below its footer.
    if !lines[footer + 1..].iter().any(|line| {
        let line: String = line.iter().collect();
        let line = clean(&line);
        line.starts_with('›') && row(line).is_none()
    }) {
        return None;
    }
    // A dialog line starts at the edge only with its selection marker; anything else crossing the
    // edge is transcript text beside the dialog.
    let in_dialog =
        |line: &str| line.is_empty() || line.starts_with("  ") || line.starts_with(ROW_MARKERS);
    let top = (0..footer)
        .rev()
        .find(|&index| !in_dialog(&slice(&lines[index], left)))
        .map_or(0, |index| index + 1);
    let mut inner = vec![String::new(), String::new()];
    inner.extend((top..=footer).map(|index| slice(&lines[index], left)));
    (inner
        .iter()
        .filter(|line| row(line).is_some_and(|row| row.selected))
        .count()
        == 1)
        .then(|| inner.join("\n"))
}

/// CDXC:AgentScreenDetection 2026-09-05 DECISION:
/// User: expose the "Implement this plan?" picker in chat so switching to the terminal is unnecessary.
/// User: audit every Codex command and make its messages and interactions usable in chat, using the existing UX and improving it where needed.
/// Numbered selectors, searchable menus, checkbox settings, and text forms retain their own selection, navigation, toggle, save, and cancel actions.
/// SEE-ALSO: apps/desktop/src/app/native_chat/terminal_dialog.rs and Codex tui/src/bottom_pane/list_selection_view.rs.
pub fn detect_codex_dialog(text: &str) -> Option<TerminalDialog> {
    // The async question card answers Codex's question editor (session_chat_codex_async_answer.rs).
    if crate::session_chat_codex_async_answer::question_editor_on_screen(text) {
        return None;
    }
    detect_codex_dialog_as_drawn(text).map(update_prompt_dialog)
}

fn detect_codex_dialog_as_drawn(text: &str) -> Option<TerminalDialog> {
    if let Some(pager) = crate::session_chat_codex_pager::detect_codex_transcript_pager(text) {
        return Some(pager);
    }
    if let Some(dialog) =
        centered_dialog_text(text).and_then(|inner| detect_codex_dialog_as_drawn(&inner))
    {
        return Some(dialog);
    }
    let mut lines: Vec<String> = text
        .lines()
        .rev()
        .take(160)
        .map(|line| {
            normalize_spaces(&strip_ansi_sgr(line))
                .trim_end()
                .to_string()
        })
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();
    if let Some(dialog) = extended_thinking_dialog(&lines) {
        return Some(dialog);
    }
    let end = lines.iter().rposition(|line| !line.trim().is_empty())?;
    let footer_index = match (end.saturating_sub(3)..=end).rev().find(|&i| {
        let line = clean(&lines[i]).to_ascii_lowercase();
        crate::session_chat_codex_blocking::is_codex_modal_footer(&line)
            || line == "q to quit"
            || line.starts_with("press enter to continue")
            || line.starts_with("press space to select or enter to save")
            // Model migration, /import and app-link views: "Use ↑/↓ to move, press enter to confirm".
            || (line.starts_with("use ") && line.contains(" to move"))
    }) {
        Some(index) => index,
        None => {
            footerless_menu(text, &lines, end)?;
            // An empty stand-in footer, so the menu parses like every footed dialog.
            lines.truncate(end + 1);
            lines.push(String::new());
            end + 1
        }
    };
    if lines[footer_index + 1..]
        .iter()
        .any(|line| clean(line).starts_with('›'))
    {
        return None;
    }
    // A dialog is separated from scrollback by an empty band. Preserve blank
    // lines inside it, but stop at the last double-blank boundary before it.
    // Blank padding under a short form (an MCP server form) sits between its
    // rows and its footer, so the search starts at the last painted line.
    let painted_end = (0..footer_index)
        .rev()
        .find(|&i| !lines[i].trim().is_empty())
        .unwrap_or(0);
    let band_above = |limit: usize| {
        (1..limit)
            .rev()
            .find(|&i| lines[i].trim().is_empty() && lines[i - 1].trim().is_empty())
            .map(|i| i + 1)
            .unwrap_or(0)
    };
    let mut start = band_above(painted_end);
    /*
    CDXC:AgentScreenDetection 2026-09-27 WHY:
    Codex's command, network and permission approvals (Codex 0.157's "Would you like to run the following command?" with its Environment and Reason lines and the command) print two blank lines above their rows, so the band above the rows was read as the dialog's top edge: the card lost its heading and every row, and a command approval fell back to a generic card with no command.
    A dialog whose first line is a row reaches one band further up for its heading, as long as that stretch is a heading's few lines rather than scrollback.
    */
    let first_painted = (start..footer_index).find(|&i| !lines[i].trim().is_empty());
    if start >= 2 && first_painted.is_some_and(|i| row(&lines[i]).is_some()) {
        let heading_start = band_above(start - 2);
        if start - heading_start <= 12 {
            start = heading_start;
        }
    }
    // Onboarding and text-entry views use a single empty line above their
    // heading, unlike the standard list selection view's two-line band.
    // Blank paragraphs in a textarea are content, not a boundary before its title.
    let named_start = if clean(&lines[footer_index]).contains("submit") {
        (0..footer_index)
            .rev()
            .find(|&i| crate::session_chat_codex_blocking::is_codex_modal_footer(clean(&lines[i])))
            .map_or(0, |i| i + 1)
    } else {
        start
    };
    if let Some(named) = (named_start..footer_index).rev().find(|&i| {
        clean(&lines[i]).starts_with("Tell us more (")
            || matches!(
                clean(&lines[i]),
                "Name thread"
                    | "Rename thread"
                    | "Edit goal"
                    | "Save conversation"
                    | "Add marketplace"
                    | "Remap Shortcut"
                    | "Choose an import source"
                    | "Choose what to import"
                    | "Custom review instructions"
                    | "Export filename"
                    | "Save transcript"
                    | "Resume a previous session"
                    | "Fork a previous session"
            )
    }) {
        start = named;
    }
    if clean(&lines[footer_index])
        .to_ascii_lowercase()
        .starts_with("press enter to continue")
    {
        if let Some(trust_heading) = lines[..footer_index]
            .iter()
            .rposition(|line| clean(line).starts_with("> You are in "))
        {
            if let Some(dialog) = directory_trust_dialog(
                &lines[trust_heading..footer_index]
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
            ) {
                return Some(dialog);
            }
        }
    }
    if clean(&lines[footer_index])
        .to_ascii_lowercase()
        .starts_with("enter continue")
    {
        if let Some(trust_heading) = lines[..footer_index]
            .iter()
            .rposition(|line| clean(line) == "Folder access")
        {
            if let Some(dialog) = folder_access_trust_dialog(
                &lines[trust_heading..footer_index]
                    .iter()
                    .map(String::as_str)
                    .collect::<Vec<_>>(),
            ) {
                return Some(dialog);
            }
        }
    }
    if clean(&lines[footer_index]) == "q to quit" {
        start = 0;
    } else if let Some(fullscreen) = lines[..footer_index].iter().rposition(|line| {
        matches!(
            clean(line),
            "Resume a previous session" | "Fork a previous session"
        )
    }) {
        start = fullscreen;
    }
    let content: Vec<&str> = lines[start..footer_index]
        .iter()
        .map(|line| line.as_str())
        .collect();
    let mut heading = content.iter().position(|line| !clean(line).is_empty())?;
    // An MCP server form heads each field with its step ("Field 1/3"); the field's own prompt is
    // what the card should ask, with the step kept only when there is more than one field.
    let field_step = clean(content[heading])
        .strip_prefix("Field ")
        // "Field 1/1 (1 required unanswered)"
        .and_then(|step| step.split_whitespace().next())
        .and_then(|step| step.split_once('/'))
        .filter(|(at, of)| {
            !at.is_empty()
                && !of.is_empty()
                && at.chars().chain(of.chars()).all(|ch| ch.is_ascii_digit())
        })
        .map(|(at, of)| (at.to_string(), of.to_string()));
    if field_step.is_some() {
        heading = (heading + 1..content.len()).find(|&i| !clean(content[i]).is_empty())?;
    }
    let title = if clean(&lines[footer_index]) == "q to quit" {
        let heading = content[heading]
            .split('/')
            .find(|part| !part.trim().is_empty())
            .unwrap_or_default()
            .trim();
        match heading.replace(' ', "").as_str() {
            "DIFF" => "Git diff".to_string(),
            "TRANSCRIPT" => "Conversation transcript".to_string(),
            _ => heading.to_string(),
        }
    } else {
        // Onboarding headings carry the same `>` its highlighted rows do ("> Import setup").
        let heading = clean(left_column(content[heading]));
        let heading = heading.strip_prefix("> ").unwrap_or(heading);
        match &field_step {
            Some((at, of)) if of != "1" => format!("{heading} (field {at} of {of})"),
            _ => heading.to_string(),
        }
    };
    if title.starts_with("Question ")
        || content
            .iter()
            .any(|line| clean(line).starts_with("Question ") && line.contains('/'))
    {
        return None;
    }
    if title.len() > 200 {
        return None;
    }
    /*
    CDXC:AgentScreenDetection 2026-09-27 WHY:
    Some Codex confirmations (the memories reset) print their options without numbers: the highlighted one behind `›`, the others indented to its label. They are answered by arrows from the highlight like every other row, so the number is only their position.
    */
    let footer_hint = clean(&lines[footer_index]).to_ascii_lowercase();
    let unnumbered = (!content[heading + 1..]
        .iter()
        .any(|line| row(line).is_some())
        && (footer_hint.contains("enter to confirm") || footer_hint.contains("enter to select")))
    .then(|| {
        let is_option = |line: &str| {
            line.starts_with("› ") || (line.starts_with("  ") && !line[2..].starts_with(' '))
        };
        let selected = (heading + 1..content.len()).find(|&i| content[i].starts_with("› "))?;
        let first = (heading + 1..=selected)
            .rev()
            .take_while(|&i| is_option(content[i]))
            .last()?;
        let last = (selected..content.len())
            .take_while(|&i| is_option(content[i]))
            .last()?;
        (2..=9)
            .contains(&(last + 1 - first))
            .then_some(first..=last)
    })
    .flatten();
    let mut rows: Vec<TerminalDialogRow> = Vec::new();
    let mut body = Vec::new();
    let mut in_rows = false;
    for (offset, line) in content[heading + 1..].iter().enumerate() {
        let index = heading + 1 + offset;
        let parsed = row(line).or_else(|| {
            let range = unnumbered.as_ref().filter(|range| range.contains(&index))?;
            Some(unnumbered_row(line, (index + 1 - range.start()) as u32))
        });
        if let Some(parsed) = parsed {
            in_rows = true;
            rows.push(parsed);
        } else if in_rows && !line.trim().is_empty() && line.starts_with("     ") {
            if let Some(last) = rows.last_mut() {
                let detail = last.description.get_or_insert_with(String::new);
                if !detail.is_empty() {
                    detail.push(' ');
                }
                detail.push_str(clean(line));
            }
        } else if !in_rows || !line.trim().is_empty() {
            body.push(*line);
        }
    }
    let numbered = !rows.is_empty() && rows.iter().filter(|r| r.selected).count() == 1;
    let body = if clean(&lines[footer_index]) == "q to quit" {
        content[heading + 1..]
            .iter()
            .filter(|line| line.trim() != "~" && !line.contains("pgup/pgdn to page"))
            .copied()
            .collect::<Vec<_>>()
            .join("\n")
    } else if numbered {
        body.join("\n")
    } else {
        content[heading + 1..].join("\n")
    };
    if !numbered {
        rows.clear();
    }
    let footer = lines[footer_index
        .saturating_sub(usize::from(clean(&lines[footer_index]) == "q to quit"))
        ..=end]
        .iter()
        .map(|line| clean(line))
        .collect::<Vec<_>>()
        .join("\n");
    let lower = format!("{title}\n{body}\n{footer}").to_ascii_lowercase();
    let footer_lower = footer.to_ascii_lowercase();
    let search_placeholder = content.iter().position(|line| {
        let line = left_column(line).to_ascii_lowercase();
        line.starts_with("type to search") || line.starts_with("type to filter")
    });
    let searchable_title = matches!(
        title.as_str(),
        "Select Syntax Theme"
            | "Select Pet"
            | "Keymap"
            | "Select a base branch"
            | "Select a commit to review"
            | "Auto-review Denials"
            | "Apps"
            | "Plugins"
            | "Resume a previous session"
            | "Fork a previous session"
    );
    let search_index = search_placeholder.or_else(|| {
        if !searchable_title {
            return None;
        }
        let gap = (heading + 1..content.len()).find(|&i| left_column(content[i]).is_empty())?;
        let mut index = (gap + 1..content.len()).find(|&i| !left_column(content[i]).is_empty())?;
        if title == "Keymap" && clean(content[index]).starts_with('[') {
            index = (index + 1..content.len()).find(|&i| !left_column(content[i]).is_empty())?;
        }
        Some(index)
    });
    // CDXC:AgentScreenDetection 2026-09-25 WHY:
    // Native Windows Codex renders text dialogs with a composer chevron instead of the block gutter. Restrict that marker to text-dialog headings so a selected list row cannot become an editable field.
    let chevron_input = matches!(
        title.as_str(),
        "Name thread"
            | "Rename thread"
            | "Edit goal"
            | "Save conversation"
            | "Add marketplace"
            | "Custom review instructions"
            | "Export filename"
            | "Save transcript"
    ) || title.starts_with("Tell us more (")
        // An MCP server form's free-text field ("› Type your answer").
        || (field_step.is_some() && rows.is_empty());
    let chevron_input = chevron_input
        .then(|| {
            content
                .iter()
                .position(|line| line.trim_start().starts_with('›'))
        })
        .flatten();
    let input = if title == "Remap Shortcut" {
        Some("key".to_string())
    } else if search_index.is_some() {
        Some("search".to_string())
    } else if chevron_input.is_some()
        || content.iter().any(|line| line.trim().starts_with('▌'))
        || lower.contains("type a name")
    {
        Some("text".to_string())
    } else {
        None
    };
    let input_value = if let Some(index) = search_index {
        if search_placeholder.is_some() {
            content
                .get(index + 1)
                .and_then(|line| clean(line).strip_prefix('>'))
                .unwrap_or_default()
                .trim_start()
                .to_string()
        } else {
            left_column(content[index]).trim().to_string()
        }
    } else if let Some(index) = chevron_input {
        let value = content[index..]
            .iter()
            .enumerate()
            .map(|(line_index, line)| {
                if line_index == 0 {
                    line.trim_start()
                        .strip_prefix('›')
                        .unwrap_or(line)
                        .trim_start()
                } else {
                    line.trim()
                }
            })
            .collect::<Vec<_>>()
            .join("\n")
            .trim_end()
            .to_string();
        // The MCP form's empty field shows its placeholder in the box.
        if field_step.is_some() && value.starts_with("Type your answer") {
            String::new()
        } else {
            value
        }
    } else if input.as_deref() == Some("text") {
        let input_start = content
            .iter()
            .rposition(|line| line.trim() == "▌")
            .map(|i| i + 1)
            .unwrap_or(heading + 1);
        content[input_start..]
            .iter()
            .filter(|line| line.trim().starts_with('▌'))
            .map(|line| clean(line))
            .filter(|line| {
                !line.starts_with("Type ")
                    && !line.starts_with("(optional)")
                    && !line.starts_with("owner/repo, git URL")
                    && !line.is_empty()
            })
            .collect::<Vec<_>>()
            .join("\n")
    } else {
        String::new()
    };
    let mut actions = Vec::new();
    if rows.is_empty() && !matches!(input.as_deref(), Some("text" | "key")) {
        actions.extend(["up", "down"].map(str::to_string));
        if footer_lower.contains("left/right") || footer_lower.contains("←/→") {
            actions.extend(["left", "right"].map(str::to_string));
        }
        if footer_lower.contains("tab") {
            actions.push("tab".to_string());
        }
        if footer_lower.contains("space") {
            actions.push("toggle".to_string());
        }
    }
    if footer_lower.contains("page") || footer_lower.contains("browse") {
        actions.extend(["pageUp", "pageDown", "home", "end"].map(str::to_string));
    }
    if footer.to_ascii_lowercase().contains("enter") {
        actions.push("confirm".to_string());
    }
    actions.push("cancel".to_string());
    let identity = content.join("\n") + &footer;
    let id = hex::encode(Sha256::digest(identity.as_bytes()));
    let mut dialog = TerminalDialog {
        id,
        title,
        body: body.trim().to_string(),
        footer,
        rows,
        input,
        input_value,
        actions,
        side_question: None,
        blocks: None,
        picker: None,
    };
    dialog.picker = crate::session_chat_resume_picker::codex_resume_picker(&dialog);
    Some(dialog)
}

impl TerminalDialog {
    pub(crate) fn is_codex_directory_trust(&self) -> bool {
        self.id.starts_with(DIRECTORY_TRUST_ID_PREFIX)
            || self.id.starts_with(FOLDER_ACCESS_TRUST_ID_PREFIX)
    }

    pub(crate) fn is_codex_update_prompt(&self) -> bool {
        self.id.starts_with(UPDATE_PROMPT_ID_PREFIX)
    }

    pub(crate) fn payload(&self, params: &Map<String, Value>) -> Result<String, DomainStateError> {
        let invalid = || DomainStateError {
            code: "invalidParams",
            message: "That action is not offered by this Codex dialog.".to_string(),
        };
        if let Some(index) = params.get("choiceIndex").and_then(Value::as_u64) {
            let row = self.rows.get(index as usize).ok_or_else(invalid)?;
            if self.id.starts_with(DIRECTORY_TRUST_ID_PREFIX) {
                return Ok(match row.number {
                    1 => "\x1b[A\r",
                    2 => "2",
                    _ => return Err(invalid()),
                }
                .to_string());
            }
            // CDXC:AgentScreenDetection 2026-09-07 WHY:
            // Codex's sensitive rows (including hook trust) only highlight on a digit, while ordinary rows commit immediately and searchable lists insert digits into the query.
            // Navigate from the freshly captured highlight and confirm once so all three select the clicked row without submitting into the next dialog.
            let selected = self
                .rows
                .iter()
                .position(|r| r.selected)
                .ok_or_else(invalid)?;
            let target = index as usize;
            let arrow = if target >= selected {
                "\x1b[B"
            } else {
                "\x1b[A"
            };
            return Ok(arrow.repeat(target.abs_diff(selected)) + "\r");
        }
        let action = params
            .get("dialogAction")
            .and_then(Value::as_str)
            .ok_or_else(invalid)?;
        if (action == "text" && self.input.as_deref() == Some("search"))
            || (action == "submit" && self.input.as_deref() == Some("text"))
        {
            let text = params
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(invalid)?;
            let search = self.input.as_deref() == Some("search");
            if text.len() > if search { 512 } else { 8192 }
                || text
                    .chars()
                    .any(|c| c.is_control() && (search || !matches!(c, '\n' | '\t')))
            {
                return Err(invalid());
            }
            return Ok(if self.input.as_deref() == Some("search") {
                "\x7f".repeat(self.input_value.chars().count())
                    + &crate::session_chat_send::sanitize_bracketed_paste_text(text)
            } else {
                crate::session_chat_send::wrap_terminal_bracketed_paste_text(text)
            });
        }
        if action == "key" && self.input.as_deref() == Some("key") {
            let key = params
                .get("text")
                .and_then(Value::as_str)
                .ok_or_else(invalid)?;
            let modifiers = params
                .get("keyModifiers")
                .and_then(Value::as_u64)
                .filter(|m| *m < 16)
                .ok_or_else(invalid)?
                + 1;
            let code = match key {
                "Enter" => 13,
                "Tab" => 9,
                "Backspace" => 127,
                "Escape" => 27,
                "ArrowUp" | "ArrowDown" | "ArrowRight" | "ArrowLeft" | "Home" | "End" => {
                    let suffix = match key {
                        "ArrowUp" => 'A',
                        "ArrowDown" => 'B',
                        "ArrowRight" => 'C',
                        "ArrowLeft" => 'D',
                        "Home" => 'H',
                        _ => 'F',
                    };
                    return Ok(format!("\x1b[1;{modifiers}{suffix}"));
                }
                "Insert" | "Delete" | "PageUp" | "PageDown" => {
                    let code = match key {
                        "Insert" => 2,
                        "Delete" => 3,
                        "PageUp" => 5,
                        _ => 6,
                    };
                    return Ok(format!("\x1b[{code};{modifiers}~"));
                }
                "F1" | "F2" | "F3" | "F4" => {
                    let suffix = match key {
                        "F1" => 'P',
                        "F2" => 'Q',
                        "F3" => 'R',
                        _ => 'S',
                    };
                    return Ok(format!("\x1b[1;{modifiers}{suffix}"));
                }
                "F5" | "F6" | "F7" | "F8" | "F9" | "F10" | "F11" | "F12" => {
                    let code = match key {
                        "F5" => 15,
                        "F6" => 17,
                        "F7" => 18,
                        "F8" => 19,
                        "F9" => 20,
                        "F10" => 21,
                        "F11" => 23,
                        _ => 24,
                    };
                    return Ok(format!("\x1b[{code};{modifiers}~"));
                }
                _ if key.chars().count() == 1 => key.chars().next().ok_or_else(invalid)? as u32,
                _ => return Err(invalid()),
            };
            return Ok(format!("\x1b[{code};{modifiers}u"));
        }
        if !self.actions.iter().any(|a| a == action) {
            return Err(invalid());
        }
        Ok(match action {
            "up" => "\x1b[A",
            "down" => "\x1b[B",
            "left" => "\x1b[D",
            "right" => "\x1b[C",
            "tab" => "\t",
            "toggle" => " ",
            "confirm" => "\r",
            "pageUp" => "\x1b[5~",
            "pageDown" => "\x1b[6~",
            "home" => "\x1b[H",
            "end" => "\x1b[F",
            "cancel" if self.footer.starts_with("Browsing transcript · ") => "\x1b[27u",
            "cancel"
                if self.id == crate::session_chat_codex_pager::CODEX_TRANSCRIPT_PAGER_ID
                    || self.footer.contains("q to quit") =>
            {
                "q"
            }
            "cancel" => SESSION_CHAT_INTERRUPT,
            _ => return Err(invalid()),
        }
        .to_string())
    }
}

pub(crate) async fn answer_codex_dialog(
    state: &crate::server::AppState,
    target: &SessionChatSendTarget,
    params: &Map<String, Value>,
) -> Result<Value, DomainStateError> {
    let stale = || DomainStateError {
        code: "invalidState",
        message: "Codex's dialog changed. Review the current choices and try again.".to_string(),
    };
    let screen = capture_session_terminal_text(&target.zmx_name)
        .await
        .ok_or_else(stale)?;
    let dialog = detect_codex_dialog(&screen).ok_or_else(stale)?;
    if params.get("dialogId").and_then(Value::as_str) != Some(dialog.id.as_str()) {
        return Err(stale());
    }
    // "Update now" runs Ghostex's updater on the copy this session runs; Codex itself only skips
    // the prompt (session_chat_codex_update.rs).
    let mut update = None;
    let skip_params: Map<String, Value>;
    let mut params = params;
    if dialog.is_codex_update_prompt()
        && params.get("choiceIndex").and_then(Value::as_u64) == Some(0)
    {
        update = crate::session_chat_codex_update::plan_for_prompt(state, &screen).await?;
        if update.is_some() {
            let skip = dialog
                .rows
                .iter()
                .position(|row| row.label == "Skip for now" || row.label == "Skip")
                .ok_or_else(stale)?;
            let mut with_skip = params.clone();
            with_skip.insert("choiceIndex".to_string(), json!(skip));
            skip_params = with_skip;
            params = &skip_params;
        }
    }
    let payload = dialog.payload(params)?;
    let submitting_text = params.get("dialogAction").and_then(Value::as_str) == Some("submit")
        && dialog.input.as_deref() == Some("text");
    let mut steps = vec![
        SessionChatSendStep::BeginLocalCommandOutput {
            agent: Some("codex".to_string()),
            command: dialog.title.clone(),
            durable_id: None,
        },
        SessionChatSendStep::VerifyTerminalDialog {
            agent: "codex".to_string(),
            id: dialog.id.clone(),
        },
    ];
    if submitting_text {
        // CDXC:SessionChat 2026-09-25 WHY:
        // Codex's paste detector swallowed Enter into the rename value when clear, paste and submit shared one write. Text dialogs need the same separate bursts as the main composer.
        let clear =
            crate::session_chat_send::build_agent_tui_clear_input_for_text(&dialog.input_value)
                .replace('\u{15}', "\x1b[117;5u")
                .replace('\u{b}', "\x1b[107;5u");
        steps.push(SessionChatSendStep::Write(clear));
        steps.push(SessionChatSendStep::SleepMs(100));
    }
    steps.push(SessionChatSendStep::Write(payload));
    steps.push(SessionChatSendStep::SleepMs(150));
    if submitting_text {
        steps.push(SessionChatSendStep::Write("\r".to_string()));
        steps.push(SessionChatSendStep::SleepMs(150));
    }
    steps.push(SessionChatSendStep::FinishLocalCommandOutput);
    execute_session_chat_send(
        &target.project_id,
        &target.session_id,
        &target.zmx_name,
        "session-chat-dialog",
        steps,
    )
    .await
    .map_err(|error| DomainStateError {
        code: "invalidState",
        message: error.message,
    })?;
    let completes = params.contains_key("choiceIndex")
        || matches!(
            params.get("dialogAction").and_then(Value::as_str),
            Some("submit" | "confirm" | "cancel")
        );
    if completes
        && capture_session_terminal_text(&target.zmx_name)
            .await
            .and_then(|screen| detect_codex_dialog(&screen))
            .is_some_and(|current| {
                current.id == dialog.id || (submitting_text && current.title == dialog.title)
            })
    {
        return Err(DomainStateError {
            code: "invalidState",
            message: "Codex kept this dialog open. Review its message and try again.".to_string(),
        });
    }
    if let Some(update) = update {
        update.spawn(state, target);
    }
    Ok(json!({"queued": true}))
}

/// Local commands print results outside Codex's conversation JSONL.
/// CDXC:AgentScreenDetection 2026-09-13 SEE-ALSO:
/// Compaction is intentionally excluded: session_chat_terminal_activity/ owns its live card, and ContextCompaction records its result.
pub(crate) fn command_has_local_output(text: &str) -> bool {
    matches!(
        text.split_whitespace().next().unwrap_or_default(),
        "/status"
            | "/pwd"
            | "/cwd"
            | "/cd"
            | "/usage"
            | "/ps"
            | "/stop"
            | "/clean"
            | "/mcp"
            | "/ide"
            | "/approve"
            | "/diff"
            | "/debug-config"
            | "/copy"
            | "/export"
            | "/raw"
            | "/vim"
            | "/personality"
            | "/goal"
            | "/plan"
            | "/model"
            | "/skills"
            | "/hooks"
            | "/apps"
            | "/plugins"
            | "/memories"
            | "/keymap"
            | "/theme"
            | "/title"
            | "/statusline"
            | "/pets"
            | "/pet"
            | "/permissions"
            | "/experimental"
            | "/import"
            | "/rename"
            | "/setup-default-sandbox"
            | "/sandbox-add-read-dir"
            | "/rollout"
            | "/agents"
            | "/subagents"
            | "/resume"
            | "/review"
            | "/feedback"
            | "/new"
            | "/clear"
            | "/fork"
            | "/init"
            | "/recap"
            | "/side"
            | "/btw"
            | "/app"
            | "/archive"
            | "/delete"
            | "/fast"
            | "/effort"
    )
}

fn history_without_composer(text: &str) -> Vec<String> {
    let mut lines: Vec<String> = text
        .lines()
        .map(|line| {
            normalize_spaces(&strip_ansi_sgr(line))
                .trim_end()
                .to_string()
        })
        .collect();
    if let Some(dialog) = detect_codex_dialog(text) {
        if let Some(heading) = lines
            .iter()
            .rposition(|line| clean(left_column(line)) == dialog.title)
        {
            lines.truncate(heading);
        }
    } else if let Some(composer) = lines.iter().rposition(|line| {
        let line = clean(line);
        line.starts_with('›') && row(line).is_none()
    }) {
        lines.truncate(composer);
    }
    while lines
        .last()
        .is_some_and(|line| line.trim().is_empty() || is_composer_gap_line(line))
    {
        lines.pop();
    }
    lines
}

/// CDXC:AgentScreenDetection 2026-09-28 WHY:
/// Codex's fullscreen view (the default since 0.157) draws a centred turn tip, the "↓ Back to bottom" pill and copy feedback in the gap above its composer, and moves them below every new output. Left in the history, the pre-send anchor ended on the tip, the only suffix still found after `/status` was the tip itself, now under the result, and the command's output came back empty. They are chrome, never output, so they are dropped with the composer. The indent test keeps a transcript line of the agent's own ("  Tip: …" in an answer) as text.
fn is_composer_gap_line(line: &str) -> bool {
    let text = line.trim();
    line.len() - line.trim_start().len() >= 3
        && [
            "Tip:",
            "↓",
            "New activity",
            "New ·",
            "Copied",
            "Copying",
            "Copy ",
        ]
        .iter()
        .any(|prefix| text.starts_with(prefix))
}

/// Only newly printed command output, excluding the previous conversation and
/// the returned composer. A live dialog has its own card instead.
pub fn codex_command_output(command: &str, before: &str, after: &str) -> Option<String> {
    if let Some(output) = crate::session_chat_codex_pager::codex_static_pager_output(after) {
        return Some(output.chars().take(24_000).collect());
    }
    if detect_codex_dialog(after).is_some() {
        return None;
    }
    let before = history_without_composer(before);
    let after = history_without_composer(after);
    /*
    CDXC:AgentScreenDetection 2026-09-28 WHY:
    Codex prints the command it ran (`/status`) on its own line above the result. When the pre-send tail is gone from a short grid (chat view parks fullscreen Codex at about 24 rows), the latest echo still attributes what follows it, the way Claude's echo does. With neither anchor there is no result: Codex keeps the newest read, so a guessed cut-off would overwrite a good one.
    */
    let command = command.trim();
    let echo = after
        .iter()
        .rposition(|line| !command.is_empty() && clean(line) == command);
    /*
    CDXC:AgentScreenDetection 2026-10-16 WHY:
    A repeated `/status` prints a block identical to the one above it, so the pre-send tail matched the new copy as well, the anchor search picked the copy nearest its old row (the new one, on a scrolled grid) and the second `/status` showed no output. The newest echo on the screen is this command's own, because every chat send stops the previous command's capture: once the screen has changed and the pre-send tail is still found above that echo, the echo starts the result. An unchanged screen keeps the tail anchor, so an older echo is never read as this command's before Codex prints.
    */
    let printed = echo
        .filter(|&echo| {
            before != after
                && crate::session_chat_local_command::newly_printed_lines(&before, &after[..echo])
                    .is_some()
        })
        .map(|echo| after[echo..].to_vec())
        .or_else(|| crate::session_chat_local_command::newly_printed_lines(&before, &after))
        .or_else(|| Some(after[echo?..].to_vec()))?;
    let output = printed.join("\n");
    // A submitted terminal prompt belongs to the JSONL transcript, not this local command.
    let output = output
        .lines()
        .take_while(|line| !clean(line).starts_with('›'))
        .collect::<Vec<_>>()
        .join("\n");
    if output.trim().is_empty() {
        None
    } else {
        Some(output.trim().chars().take(24_000).collect())
    }
}
