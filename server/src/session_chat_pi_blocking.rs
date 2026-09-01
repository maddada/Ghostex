/*
CDXC:SessionChatPiBlockingScreens 2026-08-31:
Pi TUI audit against earendil-works/pi 853a80d2. Pi does not have a built-in
command/edit approval layer. Instead, ordinary prompt input becomes unavailable
whenever `interactive-mode.ts` replaces `editorContainer` with one of these
focused components:

  * extension select/confirm/input/editor prompts, including project-trust and
    provider-owned authentication prompts;
  * built-in settings and submenus, model/thinking/theme/image configuration,
    project trust, session resume/delete/rename, fork/tree/label, and branch
    summary prompts;
  * OAuth/API-key/device/ambient authentication screens;
  * pre-interactive first-run, project-trust, missing-cwd, resume-session, and
    config selectors;
  * the shipped llama.cpp extension's manager, confirmation, and search views.

All concrete Pi-owned replacements render inside `DynamicBorder` rows. The
ordinary editor uses the same borders, so liveness is determined from the
NEWEST complete bordered frame and then from the contents inside that frame.
This also excludes the editor autocomplete list, which Pi renders after the
editor's closing border while ordinary prompt input remains live. Standard
extension dialogs share stable action words and selection markers even when
their titles and option labels are extension-defined.

The public `ui.custom` extension contract is intentionally arbitrary. Its
shipped llama.cpp implementation follows the same framed selection grammar;
third-party custom components without Pi's borders or control hints cannot be
identified safely from terminal text alone.
*/

use crate::session_chat_options::{normalize_spaces, strip_ansi_sgr};

const PI_BLOCKING_SCAN_LINES: usize = 160;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PiBlockingScreen {
    pub title: &'static str,
    pub detail: &'static str,
}

fn scan_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for raw in text.lines().rev() {
        let line = normalize_spaces(&strip_ansi_sgr(raw))
            .trim_end()
            .to_string();
        if line.trim().is_empty() {
            continue;
        }
        lines.push(line);
        if lines.len() >= PI_BLOCKING_SCAN_LINES {
            break;
        }
    }
    lines.reverse();
    lines
}

fn is_dynamic_border(line: &str) -> bool {
    let line = line.trim();
    line.chars().count() >= 6 && line.chars().all(|character| character == '\u{2500}')
}

fn active_frame(lines: &[String]) -> Option<&[String]> {
    let closing = lines.iter().rposition(|line| is_dynamic_border(line))?;
    let opening = lines[..closing]
        .iter()
        .rposition(|line| is_dynamic_border(line))?;
    Some(&lines[opening + 1..closing])
}

fn folded_frame(frame: &[String]) -> String {
    frame
        .iter()
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .collect::<Vec<_>>()
        .join(" ")
        .to_ascii_lowercase()
}

fn has_selected_row(frame: &[String]) -> bool {
    frame.iter().any(|line| {
        let line = line.trim_start();
        line.starts_with("\u{2192} ") || line.starts_with("\u{203a} ")
    })
}

fn has_all_words(text: &str, words: &[&str]) -> bool {
    words.iter().all(|word| text.contains(word))
}

/// Classify a live Pi screen whose focused component has replaced the ordinary
/// prompt editor. `None` means the newest complete frame is the editor itself
/// or the screen has no source-stable evidence of a blocking Pi component.
pub fn detect_pi_blocking_screen(text: &str) -> Option<PiBlockingScreen> {
    let lines = scan_lines(text);
    let frame = active_frame(&lines)?;
    let folded = folded_frame(frame);

    if folded.contains("trust project folder?") || folded.contains("project trust") {
        return Some(PiBlockingScreen {
            title: "Pi is waiting for project trust",
            detail: "Choose how Pi may use this project and whether to remember that decision in the terminal before sending a message.",
        });
    }

    if folded.contains("welcome to pi, the minimal coding agent.")
        || folded.contains("opt-in to anonymous usage data sharing?")
    {
        return Some(PiBlockingScreen {
            title: "Pi setup is waiting for input",
            detail: "Finish or skip Pi's first-run theme and analytics setup in the terminal before sending a message.",
        });
    }

    if folded.contains("select authentication method:")
        || folded.contains("select provider to configure:")
        || folded.contains("select provider to logout:")
        || folded.contains("waiting for authentication...")
        || folded.contains("enter code:")
        || folded.contains("click to open")
        || frame.iter().any(|line| {
            let line = line.trim_start().to_ascii_lowercase();
            line.starts_with("login to ")
        })
    {
        return Some(PiBlockingScreen {
            title: "Pi is waiting for authentication",
            detail: "Complete the provider choice, browser or device-code sign-in, credential prompt, or logout flow in the terminal.",
        });
    }

    if folded.contains("delete session?") {
        return Some(PiBlockingScreen {
            title: "Pi is waiting for confirmation",
            detail:
                "Confirm or cancel the session deletion in the terminal before sending a message.",
        });
    }

    if folded.contains("resume session (current folder)") || folded.contains("resume session (all)")
    {
        return Some(PiBlockingScreen {
            title: "Pi is waiting for a session choice",
            detail: "Select, rename, delete, or cancel the Pi session picker in the terminal before sending a message.",
        });
    }

    if folded.contains("global resources") || folded.contains("project local resources") {
        return Some(PiBlockingScreen {
            title: "Pi configuration is waiting for input",
            detail: "Finish or close Pi's resource configuration screen in the terminal before sending a message.",
        });
    }

    if folded.contains("is configured outside pi") && folded.contains("to close") {
        return Some(PiBlockingScreen {
            title: "Pi is waiting for a setup dialog to close",
            detail: "Review and close the provider setup information in the terminal before sending a message.",
        });
    }

    if has_all_words(&folded, &["submit", "cancel"]) || has_all_words(&folded, &["save", "cancel"])
    {
        return Some(PiBlockingScreen {
            title: "Pi is waiting for text input",
            detail: "A Pi text prompt or editor has replaced the ordinary composer. Submit or cancel it in the terminal before sending a message.",
        });
    }

    if has_selected_row(frame)
        || has_all_words(&folded, &["navigate", "select", "cancel"])
        || folded.contains("no matching models")
        || folded.contains("model configuration")
    {
        return Some(PiBlockingScreen {
            title: "Pi is waiting for a choice",
            detail: "A Pi menu has replaced the prompt editor. Make or cancel the selection in the terminal before sending a message.",
        });
    }

    None
}
