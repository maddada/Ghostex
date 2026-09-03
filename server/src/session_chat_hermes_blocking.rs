/*
CDXC:AgentScreenDetection 2026-08-31:
Hermes Agent audit against nousresearch/hermes-agent a9c783f2 (v0.20.6).
Hermes ships two independent terminal interfaces, and both replace or hide the
ordinary composer while they own input:

  * the default prompt_toolkit REPL has approval, clarify, sudo, secret,
    destructive-confirmation, model-picker, and command-palette states;
  * the optional Ink TUI's `$isBlocked` aggregate covers approval, clarify,
    confirm, sudo, secret, billing, subscription, sessions, model, pet, skills,
    plugins, pager, agents, journey, and modal-widget overlays.

The pre-interactive setup wizard and provider device-login flows also own stdin
before either composer exists. Named screen anchors and source-stable control
hints cover those states. Generic labels such as Sessions, Pets, and Journey
are never evidence by themselves: they must be paired with that surface's
control footer or enclosed in its live modal frame. A later normal Hermes
rule-sandwich composer retires an earlier match so a dismissed picker left in
scrollback does not keep chat blocked. We deliberately do not classify every
missing composer: the classic REPL also changes its marker while an agent or
slash command is running, when steering or queued input can still be valid.

Ink's public modal-widget SDK permits completely arbitrary text. Standard
widgets use the shipped Overlay/Dialog close grammar and are covered by the
modal control hints below. A third-party widget that paints no standard title,
border, or control hint cannot be identified safely from terminal text alone;
the existing positive composer-readiness gate still prevents delivery into it.
*/

use crate::session_chat_options::{normalize_spaces, strip_ansi_sgr};

const HERMES_BLOCKING_SCAN_LINES: usize = 180;
const HERMES_BLOCKING_PAIR_RADIUS: usize = 96;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct HermesBlockingScreen {
    pub title: &'static str,
    pub detail: &'static str,
}

fn collapse_spaces(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn scan_lines(text: &str) -> Vec<String> {
    let mut lines = Vec::new();
    for raw in text.lines().rev() {
        let line = collapse_spaces(normalize_spaces(&strip_ansi_sgr(raw)).trim());
        if line.is_empty() {
            continue;
        }
        lines.push(line);
        if lines.len() >= HERMES_BLOCKING_SCAN_LINES {
            break;
        }
    }
    lines.reverse();
    lines
}

fn strip_box_border(line: &str) -> &str {
    const BORDERS: &[char] = &[
        '\u{2502}', '\u{2503}', '\u{2506}', '\u{250a}', '\u{250c}', '\u{2510}', '\u{2514}',
        '\u{2518}', '\u{256d}', '\u{256e}', '\u{256f}', '\u{2570}', '|',
    ];
    line.trim()
        .trim_start_matches(BORDERS)
        .trim_end_matches(BORDERS)
        .trim()
}

fn is_horizontal_rule(line: &str) -> bool {
    let line = line.trim();
    line.chars().count() >= 8
        && line
            .chars()
            .all(|character| matches!(character, '\u{2500}' | '\u{2501}' | '-' | '_'))
}

fn is_numbered_choice(rest: &str) -> bool {
    let rest = rest.trim_start();
    let rest = rest
        .strip_prefix("[ ]")
        .or_else(|| rest.strip_prefix("[x]"))
        .unwrap_or(rest)
        .trim_start();
    let digits = rest.chars().take_while(char::is_ascii_digit).count();
    digits > 0 && matches!(rest.chars().nth(digits), Some('.' | ')'))
}

fn is_normal_composer_line(line: &str) -> bool {
    let line = strip_box_border(line);
    let mut words = line.split_whitespace();
    let Some(first) = words.next() else {
        return false;
    };
    if first == "\u{276f}" {
        return !is_numbered_choice(line.trim_start_matches('\u{276f}'));
    }
    if words.next() != Some("\u{276f}") {
        return false;
    }
    // Hermes profile names are one CLI token. More words before the marker
    // would admit prose quoting the glyph rather than a composer row.
    let after_marker = line
        .split_once('\u{276f}')
        .map(|(_, rest)| rest)
        .unwrap_or_default();
    !is_numbered_choice(after_marker)
}

fn composer_after(lines: &[String], index: usize) -> bool {
    ((index + 1)..lines.len()).any(|line_index| {
        if !is_normal_composer_line(&lines[line_index]) {
            return false;
        }
        let before_is_rule = line_index
            .checked_sub(1)
            .is_some_and(|before| is_horizontal_rule(&lines[before]));
        let after_is_rule = lines
            .get(line_index + 1)
            .is_some_and(|line| is_horizontal_rule(line));
        before_is_rule || after_is_rule
    })
}

fn latest_line_containing(lines: &[String], needles: &[&str]) -> Option<usize> {
    lines.iter().rposition(|line| {
        let line = line.to_ascii_lowercase();
        needles
            .iter()
            .any(|needle| line.contains(&needle.to_ascii_lowercase()))
    })
}

fn line_contains_any(line: &str, needles: &[&str]) -> bool {
    let line = line.to_ascii_lowercase();
    needles
        .iter()
        .any(|needle| line.contains(&needle.to_ascii_lowercase()))
}

fn latest_paired_evidence(
    lines: &[String],
    anchors: &[&str],
    companions: &[&str],
) -> Option<usize> {
    for anchor in (0..lines.len()).rev() {
        if !line_contains_any(&lines[anchor], anchors) {
            continue;
        }
        let end = (anchor + HERMES_BLOCKING_PAIR_RADIUS + 1).min(lines.len());
        if let Some(companion) = (anchor..end)
            .rev()
            .find(|index| line_contains_any(&lines[*index], companions))
        {
            return Some(anchor.max(companion));
        }
    }
    None
}

fn live_paired_screen(
    lines: &[String],
    anchors: &[&str],
    companions: &[&str],
    title: &'static str,
    detail: &'static str,
) -> Option<HermesBlockingScreen> {
    let index = latest_paired_evidence(lines, anchors, companions)?;
    (!composer_after(lines, index)).then_some(HermesBlockingScreen { title, detail })
}

fn is_selected_numbered_row(line: &str) -> bool {
    let line = strip_box_border(line);
    let Some(rest) = line
        .strip_prefix("\u{25b8} ")
        .or_else(|| line.strip_prefix("\u{276f} "))
    else {
        return false;
    };
    is_numbered_choice(rest)
}

fn latest_controlled_menu(lines: &[String], controls: &[&str]) -> Option<usize> {
    for control in (0..lines.len()).rev() {
        if !line_contains_any(&lines[control], controls) {
            continue;
        }
        let start = control.saturating_sub(HERMES_BLOCKING_PAIR_RADIUS);
        if (start..control).any(|index| is_selected_numbered_row(&lines[index])) {
            return Some(control);
        }
    }
    None
}

fn live_controlled_menu(
    lines: &[String],
    controls: &[&str],
    title: &'static str,
    detail: &'static str,
) -> Option<HermesBlockingScreen> {
    let index = latest_controlled_menu(lines, controls)?;
    (!composer_after(lines, index)).then_some(HermesBlockingScreen { title, detail })
}

fn is_frame_top(line: &str) -> bool {
    let line = line.trim();
    matches!(line.chars().next(), Some('\u{256d}' | '\u{2554}'))
        && matches!(line.chars().last(), Some('\u{256e}' | '\u{2557}'))
}

fn is_frame_bottom(line: &str) -> bool {
    let line = line.trim();
    matches!(line.chars().next(), Some('\u{2570}' | '\u{255a}'))
        && matches!(line.chars().last(), Some('\u{256f}' | '\u{255d}'))
}

fn latest_framed_control(lines: &[String], controls: &[&str]) -> Option<usize> {
    for control in (0..lines.len()).rev() {
        if !line_contains_any(&lines[control], controls) {
            continue;
        }
        let start = control.saturating_sub(HERMES_BLOCKING_PAIR_RADIUS);
        let end = (control + HERMES_BLOCKING_PAIR_RADIUS + 1).min(lines.len());
        let has_top = lines[start..=control].iter().any(|line| is_frame_top(line));
        let bottom = ((control + 1)..end).find(|index| is_frame_bottom(&lines[*index]));
        if has_top {
            return Some(bottom.unwrap_or(control));
        }
    }
    None
}

fn live_framed_control(
    lines: &[String],
    controls: &[&str],
    title: &'static str,
    detail: &'static str,
) -> Option<HermesBlockingScreen> {
    let index = latest_framed_control(lines, controls)?;
    (!composer_after(lines, index)).then_some(HermesBlockingScreen { title, detail })
}

fn latest_masked_prompt(lines: &[String]) -> Option<usize> {
    for anchor in (0..lines.len()).rev() {
        let line = strip_box_border(&lines[anchor]);
        if !line.starts_with('\u{1f510}') && !line.starts_with('\u{1f511}') {
            continue;
        }
        let end = (anchor + 5).min(lines.len());
        if lines[anchor + 1..end]
            .iter()
            .any(|line| strip_box_border(line).starts_with('>'))
        {
            return Some(anchor);
        }
    }
    None
}

fn live_named_screen(
    lines: &[String],
    needles: &[&str],
    title: &'static str,
    detail: &'static str,
) -> Option<HermesBlockingScreen> {
    let index = latest_line_containing(lines, needles)?;
    (!composer_after(lines, index)).then_some(HermesBlockingScreen { title, detail })
}

/// Classify a live Hermes screen that owns terminal input in place of the
/// ordinary prompt. `None` means either normal input is available, the only
/// evidence is stale scrollback, or no source-stable terminal text identifies
/// the blocking surface.
pub fn detect_hermes_blocking_screen(text: &str) -> Option<HermesBlockingScreen> {
    let lines = scan_lines(text);
    if lines.is_empty() {
        return None;
    }

    if let Some(screen) = live_paired_screen(
        &lines,
        &["Dangerous Command", "approval required"],
        &[
            "\u{2191}/\u{2193} to select, Enter to confirm",
            "quick pick · Esc/Ctrl+C deny",
        ],
        "Hermes is waiting for approval",
        "Approve, deny, or change the command or tool decision in the terminal before sending a message.",
    ) {
        return Some(screen);
    }

    if let Some(screen) = live_paired_screen(
        &lines,
        &[
            "Sudo Password Required",
            "sudo password required",
            "Skill Setup Required",
        ],
        &[
            "password hidden · Enter to skip",
            "secret hidden · Enter to skip",
        ],
        "Hermes is waiting for a protected value",
        "Enter or skip the requested password or secret in the terminal before sending a message.",
    ) {
        return Some(screen);
    }
    if let Some(index) = latest_masked_prompt(&lines) {
        if !composer_after(&lines, index) {
            return Some(HermesBlockingScreen {
                title: "Hermes is waiting for a protected value",
                detail: "Enter or skip the requested password or secret in the terminal before sending a message.",
            });
        }
    }

    if let Some(screen) = live_named_screen(
        &lines,
        &[
            "Log in to Nous Portal now?",
            "Open this URL to authorize Hermes",
            "Waiting for approval (polling every",
            "Waiting for sign-in... (press Ctrl+C to cancel)",
        ],
        "Hermes is waiting for authentication",
        "Complete the API-key, browser, or device-code sign-in flow in the terminal before sending a message.",
    ) {
        return Some(screen);
    }

    if let Some(screen) = live_named_screen(
        &lines,
        &[
            "Hermes isn't configured yet",
            "Run setup now? [Y/n]",
            "Hermes Agent Setup Wizard",
            "Hermes Setup \u{2014}",
        ],
        "Hermes setup is waiting for input",
        "Finish or exit Hermes first-run and provider setup in the terminal before sending a message.",
    ) {
        return Some(screen);
    }

    if let Some(screen) = live_named_screen(
        &lines,
        &[
            "Hermes needs your input",
            "Tab/Shift+Tab switch question",
            "Enter confirm and continue",
            "Enter lock answer",
            "Enter send · Esc back",
            "Enter send · Esc cancel",
        ],
        "Hermes needs your input",
        "Answer or cancel Hermes's question in the terminal before sending another message.",
    ) {
        return Some(screen);
    }

    if let Some(screen) = live_named_screen(
        &lines,
        &[
            "type 1/2/3, or use",
            "Y/N quick · Esc cancel",
            "Use this model for this invocation? [y/N]",
            "y/Enter confirm · n/Esc cancel",
        ],
        "Hermes is waiting for confirmation",
        "Confirm or cancel the pending Hermes action in the terminal before sending a message.",
    ) {
        return Some(screen);
    }

    if let Some(screen) = live_paired_screen(
        &lines,
        &[
            "Top up · balance",
            "Allow Remote Spending",
            "Remote Spending allowed",
            "Waiting for your browser",
            "One-time setup",
            "Add funds",
            "Auto-reload",
            "Change plan",
            "Confirm cancellation",
            "Confirm plan change",
            "Confirm purchase",
            "Team subscription",
        ],
        &[
            "quick pick · Enter confirm · Esc close",
            "quick pick · Enter confirm · Esc back",
            "Enter confirm · Esc back",
            "Enter confirm · Y/N quick · Esc back",
            "Enter resume · Esc cancel",
            "Enter next/confirm · Esc back",
            "Enter preview · Esc back",
            "Waiting for approval\u{2026} · Esc to cancel",
            "Enter/Esc close",
        ],
        "Hermes billing is waiting for input",
        "Finish the billing, subscription, or browser-approval step in the terminal before sending a message.",
    ) {
        return Some(screen);
    }
    if let Some(screen) = live_controlled_menu(
        &lines,
        &[
            "quick pick · Enter confirm · Esc close",
            "\u{2191}/\u{2193} select · Enter confirm · Esc close",
            "\u{2191}/\u{2193} select · Enter confirm · Esc back",
            "\u{2191}/\u{2193} select · Enter · Esc close",
        ],
        "Hermes billing is waiting for input",
        "Finish the billing or subscription menu in the terminal before sending a message.",
    ) {
        return Some(screen);
    }

    if let Some(screen) = live_paired_screen(
        &lines,
        &[
            "Model Picker",
            "Select provider (step 1/2)",
            "Select model (step 2/2)",
            "Configure ",
            "Disconnect ",
        ],
        &[
            "Current:",
            "Select a model (",
            "type to filter · \u{2191}/\u{2193} select",
            "Enter save · Ctrl+U clear · Esc back",
            "Enter choose · ^d disconnect",
            "Enter switch · Esc clear/back · q close",
            "y/Enter confirm · n/Esc cancel",
        ],
        "Hermes is waiting for a model choice",
        "Finish or close the provider, model, credential, or disconnect picker in the terminal before sending a message.",
    ) {
        return Some(screen);
    }
    if let Some(screen) = live_named_screen(
        &lines,
        &["loading models\u{2026}"],
        "Hermes is waiting for the model picker",
        "Wait for the model picker, then finish or close it in the terminal before sending a message.",
    ) {
        return Some(screen);
    }

    if let Some(screen) = live_paired_screen(
        &lines,
        &["Command Palette"],
        &["Enter inserts, Esc cancels"],
        "Hermes is waiting in the command palette",
        "Insert a command or close the palette in the terminal before sending a message.",
    ) {
        return Some(screen);
    }

    if let Some(screen) = live_paired_screen(
        &lines,
        &["Sessions"],
        &["Ctrl+N new · Ctrl+R refresh · Esc close"],
        "Hermes is waiting for a session choice",
        "Select, create, close, resume, or dismiss the Hermes session picker in the terminal.",
    ) {
        return Some(screen);
    }
    if let Some(screen) = live_named_screen(
        &lines,
        &["loading sessions\u{2026}"],
        "Hermes is waiting for the session picker",
        "Wait for the session picker, then finish or close it in the terminal before sending a message.",
    ) {
        return Some(screen);
    }

    if let Some(screen) = live_paired_screen(
        &lines,
        &["Pets"],
        &["Enter adopt · type to filter · Esc cancel"],
        "Hermes is waiting for a pet choice",
        "Choose a pet or close the picker in the terminal before sending a message.",
    ) {
        return Some(screen);
    }
    if let Some(screen) = live_named_screen(
        &lines,
        &["loading pets\u{2026}"],
        "Hermes is waiting for the pet picker",
        "Wait for the pet picker, then choose a pet or close it in the terminal.",
    ) {
        return Some(screen);
    }

    if let Some(screen) = live_paired_screen(
        &lines,
        &["Skills Hub"],
        &[
            "Enter open · 1-9,0 quick · Esc/q cancel",
            "i reinspect · x reinstall · Enter/Esc back · q close",
        ],
        "Hermes is waiting in the Skills Hub",
        "Finish or close the skill browser in the terminal before sending a message.",
    ) {
        return Some(screen);
    }
    if let Some(screen) = live_named_screen(
        &lines,
        &["loading skills\u{2026}"],
        "Hermes is waiting for the Skills Hub",
        "Wait for the skill browser, then finish or close it in the terminal before sending a message.",
    ) {
        return Some(screen);
    }

    if let Some(screen) = live_paired_screen(
        &lines,
        &["Plugins Hub"],
        &[
            "Enter/Space toggle · Tab user/all",
            "install: hermes plugins install owner/repo",
        ],
        "Hermes is waiting in the Plugins Hub",
        "Finish or close the plugin browser in the terminal before sending a message.",
    ) {
        return Some(screen);
    }
    if let Some(screen) = live_named_screen(
        &lines,
        &["loading plugins\u{2026}"],
        "Hermes is waiting for the Plugins Hub",
        "Wait for the plugin browser, then finish or close it in the terminal before sending a message.",
    ) {
        return Some(screen);
    }

    if let Some(screen) = live_paired_screen(
        &lines,
        &["Spawn tree", "Last turn · finished"],
        &["Enter/\u{2192} open detail", "Esc/\u{2190} back to list"],
        "Hermes is showing the agents view",
        "Close the full-screen agents view in the terminal before sending a message.",
    ) {
        return Some(screen);
    }
    if let Some(screen) = live_paired_screen(
        &lines,
        &["Replay diff"],
        &["baseline vs candidate · esc/q close"],
        "Hermes is showing the agents view",
        "Close the full-screen agents view in the terminal before sending a message.",
    ) {
        return Some(screen);
    }
    if let Some(screen) = live_named_screen(
        &lines,
        &["No subagents this turn. Trigger delegate_task to populate the tree."],
        "Hermes is showing the agents view",
        "Close the full-screen agents view in the terminal before sending a message.",
    ) {
        return Some(screen);
    }

    if let Some(screen) = live_paired_screen(
        &lines,
        &["Journey"],
        &[
            "learned skills & memories over time",
            "g/G top/bottom · q close",
            "Esc/q close",
        ],
        "Hermes is showing the Journey view",
        "Close the full-screen Journey view in the terminal before sending a message.",
    ) {
        return Some(screen);
    }

    if let Some(screen) = live_paired_screen(
        &lines,
        &["error:", "no skills available", "no pets available"],
        &["Esc/q cancel", "Esc/q close", "Esc cancel"],
        "Hermes is waiting in a picker",
        "Close the failed or empty Hermes picker in the terminal before sending a message.",
    ) {
        return Some(screen);
    }

    if let Some(screen) = live_framed_control(
        &lines,
        &[
            "Enter/Space/PgDn page",
            "PgUp back · g/G top/bottom · Esc/q close",
            "Esc/q close (",
            "Esc/q close",
        ],
        "Hermes is showing a modal view",
        "Read, act on, or close the Hermes pager or modal in the terminal before sending a message.",
    ) {
        return Some(screen);
    }

    None
}
