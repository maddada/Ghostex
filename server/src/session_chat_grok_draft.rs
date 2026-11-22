use super::*;
use crate::session_chat_composer::{
    detect_session_chat_composer_ready, grok_composer_input_text, SessionChatComposerState,
};

/// CDXC:Drafts 2026-09-07 WHY:
/// Grok 1.0.21 ignores Ctrl+P on its welcome screen, so the old single-write Ctrl+P/editor/Enter capture submitted `editor` as the first user prompt during startup's automatic switch to Chat.
/// Empty or unavailable drafts need no editor invocation; nonempty drafts require proof of the palette and its filtered editor action before text or Enter can reach the terminal.
pub(super) fn has_capturable_draft(screen: &str) -> bool {
    detect_session_chat_composer_ready(Some("grok"), screen).state
        == SessionChatComposerState::Ready
        && crate::session_chat_grok_blocking::detect_grok_blocking_screen(screen).is_none()
        && grok_composer_input_text(screen).is_some()
}

fn palette_ready(screen: &str, filtered: bool) -> bool {
    let lines: Vec<_> = screen.lines().collect();
    let Some(top) = lines.iter().rposition(|line| {
        line.contains("┌─ Commands ─")
            && (line.contains("[✗]") || line.contains("[x]"))
            && line.contains('┐')
    }) else {
        return false;
    };
    let Some(bottom) = lines[top + 1..]
        .iter()
        .position(|line| line.contains("└─") && line.contains('┘'))
    else {
        return false;
    };
    let cells: Vec<_> = lines[top + 1..top + 1 + bottom]
        .iter()
        .flat_map(|line| {
            let parts: Vec<_> = line.split('│').collect();
            if parts.len() < 3 {
                return Vec::new();
            }
            parts[1..parts.len() - 1]
                .iter()
                .map(|part| part.trim())
                .collect::<Vec<_>>()
        })
        .collect();
    let query = if filtered {
        "search: editor"
    } else {
        "search:"
    };
    if !cells.contains(&query) {
        return false;
    }
    if !filtered {
        return true;
    }
    let entries: Vec<_> = cells
        .iter()
        .filter_map(|cell| cell.strip_prefix("◆ "))
        .collect();
    entries.len() == 1 && entries[0].starts_with("Edit Prompt in External Editor ")
}

pub(super) async fn open_editor(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    cancellation: Option<(&AtomicU64, u64)>,
) -> Result<(), String> {
    for (input, postcondition) in [
        ("\u{10}", Some(false)),
        ("editor", Some(true)),
        ("\r", None),
    ] {
        let cancelled = || {
            cancellation
                .is_some_and(|(generation, expected)| generation.load(Ordering::SeqCst) != expected)
        };
        if cancelled() {
            return Err("The terminal draft capture was superseded.".to_string());
        }
        write_session_chat_payload(project_id, session_id, zmx_name, "grok-draft-editor", input)
            .await?;
        let Some(filtered) = postcondition else { break };
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            if cancelled() {
                return Err("The terminal draft capture was superseded.".to_string());
            }
            if capture_session_terminal_text(zmx_name)
                .await
                .as_deref()
                .is_some_and(|screen| palette_ready(screen, filtered))
            {
                break;
            }
            if std::time::Instant::now() >= deadline {
                return Err("Grok did not show its prompt-editor command in the command palette. The terminal draft was not submitted.".to_string());
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const WELCOME: &str = "Grok Build 1.0.21\nNew worktree ctrl+w\nResume session ctrl+r\n╭──────────────────╮\n│ ❯                │\n╰──── Grok 4.6 ─────╯";
    const EDITOR_PALETTE: &str = "transcript ┌─ Commands ────────── [✗] ─┐\ntranscript │ search: editor            │\ntranscript │ Model & Input ─────────── │\ntranscript │ ◆ Edit Prompt in External Editor  /edit-prompt │\ntranscript └───────────────────────────┘\n│ ❯ draft │";

    #[test]
    fn grok_empty_startup_draft_needs_no_terminal_input() {
        assert!(!has_capturable_draft(WELCOME));
        assert!(!has_capturable_draft("shell still starting grok"));
        assert!(!has_capturable_draft(""));
        assert!(!palette_ready(WELCOME, false));
        assert!(!palette_ready(WELCOME, true));
    }

    #[test]
    fn grok_nonempty_composer_can_be_captured() {
        assert!(has_capturable_draft(
            "╭──────────────────╮\n│ ❯ draft          │\n╰──── Grok 4.6 ─────╯"
        ));
    }

    #[test]
    fn grok_editor_requires_filtered_palette_with_one_action() {
        assert!(palette_ready(EDITOR_PALETTE, true));
        assert!(!palette_ready(EDITOR_PALETTE, false));
        let unfiltered = EDITOR_PALETTE.replace("search: editor", "search:");
        assert!(palette_ready(&unfiltered, false));
        assert!(!palette_ready(&unfiltered, true));
        assert!(!palette_ready(
            &EDITOR_PALETTE.replace("Commands", "Settings"),
            true
        ));
        assert!(!palette_ready(&EDITOR_PALETTE.replace("└", " "), true));
        assert!(!palette_ready(
            &EDITOR_PALETTE.replace(
                "│ Model & Input ─────────── │",
                "│ ◆ Another editor action │"
            ),
            true
        ));
        assert!(!palette_ready(
            "search: editor\n◆ Edit Prompt in External Editor /edit-prompt",
            true
        ));
    }
}
