//! Claude Code's own `/model` list, the only surface that can change a model without saving a
//! default. Verified live 2026-09-18 against Claude Code 2.1.268: `s` applied the highlighted model
//! and the arrow-set effort with "for this session only", and `~/.claude/settings.json` was
//! byte-identical after.

use super::{
    agent_busy, claude_model_label_matches, collapse_spaces, dialog_mismatch, screen_lines,
    session_not_running, unsupported_selection, CodexPickerPlan, PickerDriver,
    CLAUDE_MODEL_COMMAND, CODEX_SUBMIT,
};
use crate::domain::DomainStateError;

const CLAUDE_MODEL_PICKER_TITLE: &str = "Select model";
/// Proof that this Claude build binds the session-only key. Without it `s` would type into the
/// list's filter instead, so the driver refuses rather than guessing.
const CLAUDE_SESSION_ONLY_FOOTER: &str = "to use this session only";
/// The whole footer while the highlight sits on a row this Claude build cannot pick
/// ("Sonnet 5.5  Update Claude Code to use this model"): Enter and `s` are not offered there.
const CLAUDE_CANCEL_ONLY_FOOTER: &str = "Esc to cancel";
pub(super) const CLAUDE_SESSION_ONLY_KEY: &str = "s";
const CLAUDE_APPLIED_PREFIX: &str = "⎿ Set model to ";
const CLAUDE_SESSION_ONLY_SUFFIX: &str = " for this session only";
/// Option+P (Escape, `p`): Claude's shortcut to the same list, which opens over a draft.
const CLAUDE_MODEL_SHORTCUT: &str = "\u{1b}p";
/// The notice above the input after a pick from the list the shortcut opened:
/// "Model set to opus (claude-opus-5-5) for this session only". No `/model` was echoed, so there
/// is no reply line, and the notice clears itself after a few seconds.
const CLAUDE_SHORTCUT_PICK_PREFIX: &str = "Model set to ";
const CLAUDE_SHORTCUT_DEFAULT_SUFFIX: &str = " and saved as your default for new sessions";
/// The notice sits above the input box, which the status line can push this far from the tail.
const CLAUDE_SHORTCUT_NOTICE_TAIL_LINES: usize = 12;
const CLAUDE_CURSOR: char = '\u{276f}';
/// Scroll hints the cursor column shows on the first painted row (`↑ 3.`) and the last (`↓ 10.`)
/// when the window hides rows beyond them.
const CLAUDE_SCROLL_HINTS: [char; 2] = ['\u{2191}', '\u{2193}'];
/// `… +2 models` under the rows counts every row the window hides, above and below together.
const CLAUDE_HIDDEN_ROWS_PREFIX: &str = "… +";
/// The label of the row that runs the account's default model; its next column names that model.
const CLAUDE_DEFAULT_ROW_PREFIX: &str = "Default (";
/// Marks the model the session is on now, inside the label column.
const CLAUDE_CURRENT_MARKER: char = '\u{2714}';
/// The rail line of a model without effort levels: `○ Effort not supported for <model>`.
const CLAUDE_EFFORT_UNSUPPORTED: &str = "Effort not supported";
const CLAUDE_ARROW_UP: &str = "\u{1b}[A";
const CLAUDE_ARROW_DOWN: &str = "\u{1b}[B";
pub(super) const CLAUDE_ARROW_RIGHT: &str = "\u{1b}[C";
const CLAUDE_ARROW_LEFT: &str = "\u{1b}[D";
/// The effort rail's levels left to right. A model offers a subset in this order, and the rail
/// wraps at both ends (Claude Code 2.1.281: Low, Medium, High, xHigh, Max, Ultracode on Opus).
const CLAUDE_EFFORT_ORDER: [&str; 6] = ["low", "medium", "high", "xhigh", "max", "ultracode"];
/// Every jump paints at least one unread row, so this bounds a list of this many windows. A one-row
/// window on a short chat pane makes it a bound on rows, and Claude Code 2.1.283 lists 22.
const CLAUDE_WINDOW_JUMP_LIMIT: usize = 64;
/// How long a list without its footer must stay unchanged before it counts as clipped by the pane.
/// A clipped list never changes; Claude repaints slowly while a turn runs, so a half-painted frame
/// can hold still for a moment.
const CLAUDE_CLIPPED_SETTLE_MS: u64 = 1_500;
/// The rail wraps, so every supported level is reachable; the bound stops a stuck rail.
pub(super) const CLAUDE_EFFORT_STEP_LIMIT: usize = 8;
/// The footer is the last line the open list draws; a closed one is followed by Claude's
/// acknowledgement and the input box, so the footer sits deeper than this many lines.
pub(super) const CLAUDE_PICKER_TAIL_LINES: usize = 4;

/// One row of Claude's `/model` list, read off the grid's own columns:
/// `❯ 2. Opus (1M context) ✔    Opus 5 with 1M context · Best for everyday, complex tasks`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ClaudeModelRow {
    number: u32,
    label: String,
    selected: bool,
}

/// One complete paint of the list: the rows the window shows, how many it scrolled out of view,
/// and the effort rail under them.
#[derive(Clone, Debug, PartialEq, Eq)]
struct ClaudeModelList {
    rows: Vec<ClaudeModelRow>,
    hidden: u32,
    effort: Option<String>,
    effort_unsupported: bool,
    footer: ClaudeListFooter,
}

/// What the list's last line says about the highlighted row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClaudeListFooter {
    /// Enter, `s` and Esc: the row can be picked for this session only.
    SessionOnly,
    /// Only "Esc to cancel": the row needs a newer Claude Code.
    CancelOnly,
    /// The pane is too short for the footer (and the effort rail) to be painted at all. A frame
    /// caught mid-paint reads the same way, so only a frame that stays put counts as one.
    Clipped,
}

impl ClaudeModelList {
    /// A frame whose footer was painted, which is what every step after opening the list reads.
    fn painted(self) -> Option<Self> {
        (self.footer != ClaudeListFooter::Clipped).then_some(self)
    }

    fn selected(&self) -> Option<u32> {
        self.rows
            .iter()
            .find(|row| row.selected)
            .map(|row| row.number)
    }

    fn first(&self) -> u32 {
        self.rows.first().map_or(1, |row| row.number)
    }

    fn last(&self) -> u32 {
        self.rows.last().map_or(1, |row| row.number)
    }

    /// Rows are numbered from one, so the painted ones plus the hidden ones are the whole list.
    fn total(&self) -> u32 {
        self.rows.len() as u32 + self.hidden
    }

    /// The first painted row whose label names `model`.
    fn row_for(&self, model: &str) -> Option<u32> {
        self.rows
            .iter()
            .find(|row| claude_model_label_matches(&row.label, model))
            .map(|row| row.number)
    }
}

/// ANSI-stripped but not space-collapsed: the run of spaces between the label and the
/// description is what separates them, and `Opus` is a prefix of `Opus (1M context)`.
pub(super) fn screen_lines_spaced(screen: &str) -> Vec<String> {
    screen
        .split('\n')
        .map(crate::session_chat_options::strip_ansi_sgr)
        .collect()
}

fn parse_claude_model_row(line: &str) -> Option<ClaudeModelRow> {
    let trimmed = line.trim_start();
    let selected = trimmed.starts_with(CLAUDE_CURSOR);
    let rest = trimmed
        .strip_prefix(CLAUDE_CURSOR)
        .or_else(|| trimmed.strip_prefix(&CLAUDE_SCROLL_HINTS[..]))
        .map(str::trim_start)
        .unwrap_or(trimmed);
    let dot = rest.find(". ")?;
    let number = rest[..dot].parse::<u32>().ok()?;
    // Two-digit lists pad the single-digit numbers: `3.  Fable 5.1`.
    let mut columns = rest[dot + 1..]
        .trim_start()
        .split("  ")
        .map(str::trim)
        .filter(|column| !column.is_empty());
    let label = columns.next()?;
    // CDXC:AgentProviders 2026-11-09 WHY:
    // Claude Code 2.1.319 lists Fable 5.1 only as `1.  Default (recommended)  Fable 5.1` (and under
    // a collapsed "More models"), so the Default row is read by the model it names: otherwise a
    // pick of `fable` found no row once Fable 5.2 became its own row.
    let label = if label.starts_with(CLAUDE_DEFAULT_ROW_PREFIX) {
        columns.next().unwrap_or(label)
    } else {
        label
    };
    let label = label
        .strip_suffix(CLAUDE_CURRENT_MARKER)
        .map(str::trim_end)
        .unwrap_or(label);
    (!label.is_empty()).then(|| ClaudeModelRow {
        number,
        label: label.to_string(),
        selected,
    })
}

/// `… +2 models` → 2.
fn hidden_row_count(line: &str) -> Option<u32> {
    let count = line.trim().strip_prefix(CLAUDE_HIDDEN_ROWS_PREFIX)?;
    let digits: String = count.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// `◉ xHigh effort ←/→ to adjust` → `xhigh`. The TUI prints the level's own label, which
/// lowercases straight back to the catalog id; `High effort (default)` marks the model's default.
fn effort_label(line: &str) -> Option<String> {
    let line = collapse_spaces(line);
    let head = line.split(" effort").next()?;
    let label = head.split_once(' ')?.1.trim();
    (!label.is_empty() && head != line).then(|| label.to_ascii_lowercase())
}

/// CDXC:SessionChat 2026-09-18 WHY:
/// The capture is the whole scrollback, so every repaint of the list is still in it and a frame
/// caught mid-paint has only its first rows. Anchoring on the footer — the last line the list
/// draws, and the proof this build binds the session-only key — takes the last COMPLETE frame
/// instead. Reading from the last title alone picked up half-drawn lists, which is why choosing
/// Sonnet or Haiku reported no such row while Opus, higher up, was already painted.
///
/// CDXC:SessionChat 2026-09-28 WHY:
/// A highlight on a row this Claude build cannot use ("Update Claude Code to use this model")
/// shortens the footer to "Esc to cancel". Requiring the session-only footer made that frame
/// unreadable, so a search walking a short pane's window past such a row timed out, pressed
/// Escape and retried every five seconds, retyping `/model` into the pane and holding the user's
/// next message behind it. That frame is read too when it is the last line on screen, and so is a
/// list that a pane of about ten rows cuts off above its footer: that one stayed open unseen, so
/// Escape was never sent and every retry found no input box. `footer` records which was read.
fn claude_model_list(screen: &str) -> Option<ClaudeModelList> {
    let lines = screen_lines_spaced(screen);
    let last_painted = lines.iter().rposition(|line| !line.trim().is_empty())?;
    let last_title = lines
        .iter()
        .rposition(|line| line.trim() == CLAUDE_MODEL_PICKER_TITLE)?;
    let session_only_footer = lines
        .iter()
        .rposition(|line| line.contains(CLAUDE_SESSION_ONLY_FOOTER));
    let (footer, kind) = if lines[last_painted].trim() == CLAUDE_CANCEL_ONLY_FOOTER {
        (last_painted, ClaudeListFooter::CancelOnly)
    } else if let Some(footer) = session_only_footer.filter(|footer| *footer > last_title) {
        (footer, ClaudeListFooter::SessionOnly)
    } else {
        (last_painted + 1, ClaudeListFooter::Clipped)
    };
    let title = lines[..footer]
        .iter()
        .rposition(|line| line.trim() == CLAUDE_MODEL_PICKER_TITLE)?;
    let body = &lines[title + 1..footer];
    // A complete older frame inside the body means the title belongs to another list.
    if body
        .iter()
        .any(|line| line.contains(CLAUDE_SESSION_ONLY_FOOTER))
    {
        return None;
    }
    let mut rows = Vec::new();
    let mut last_row = 0;
    for (index, line) in body.iter().enumerate() {
        if let Some(row) = parse_claude_model_row(line) {
            rows.push(row);
            last_row = index;
        }
    }
    // A window paints consecutive rows; anything else is not a list this driver can count.
    let first = rows.first()?.number;
    if rows
        .iter()
        .enumerate()
        .any(|(index, row)| row.number != first + index as u32)
    {
        return None;
    }
    let below = &body[last_row + 1..];
    // A clipped list is the last thing on screen: only its own trailing lines may follow the rows,
    // never Claude's reply or input box after it closed.
    if kind == ClaudeListFooter::Clipped
        && !below.iter().all(|line| {
            let line = line.trim();
            line.is_empty()
                || line.starts_with(CLAUDE_HIDDEN_ROWS_PREFIX)
                || line.starts_with("Use /")
                || line.contains(CLAUDE_EFFORT_UNSUPPORTED)
                || effort_label(line).is_some()
                || line.chars().all(|ch| CLAUDE_SCROLL_HINTS.contains(&ch))
        })
    {
        return None;
    }
    Some(ClaudeModelList {
        rows,
        hidden: below
            .iter()
            .find_map(|line| hidden_row_count(line))
            .unwrap_or(0),
        effort: below.iter().rev().find_map(|line| effort_label(line)),
        effort_unsupported: below
            .iter()
            .any(|line| line.contains(CLAUDE_EFFORT_UNSUPPORTED)),
        footer: kind,
    })
}

/// CDXC:SessionChat 2026-09-19 WHY:
/// The capture is zmx history, so a `Select model` title outlives the list it belonged to. Only the
/// footer still at the tail of the screen proves the list is open now; going by the title made every
/// error-path `cancel_dialog` interrupt an idle Claude session that had used the picker once before.
pub(super) fn claude_model_picker_open(screen: &str) -> bool {
    screen_lines(screen)
        .iter()
        .rev()
        .filter(|line| !line.trim().is_empty())
        .take(CLAUDE_PICKER_TAIL_LINES)
        .any(|line| line.contains(CLAUDE_SESSION_ONLY_FOOTER))
        || claude_model_list(screen)
            .is_some_and(|list| list.footer != ClaudeListFooter::SessionOnly)
}

/// Claude's acknowledgement of a session-only pick, below the `/model` it echoed.
///
/// CDXC:SessionChat 2026-09-18 WHY:
/// The line names the effort only when the picker's rail was moved: leaving an already-correct
/// effort alone prints "Set model to Sonnet 5 for this session only" and nothing more. Requiring
/// the suffix unconditionally made every pick whose effort already matched time out and retry.
fn claude_session_only_applied(screen: &str, model: &str, effort: Option<&str>) -> bool {
    if crate::session_chat_composer::detect_session_chat_composer_readiness(
        Some("claude"),
        screen,
        None,
    )
    .state
        != crate::session_chat_composer::SessionChatComposerState::Ready
    {
        return false;
    }
    let lines = screen_lines(screen);
    let command = format!("❯ {CLAUDE_MODEL_COMMAND}");
    let Some(command_index) = lines.iter().rposition(|line| line == &command) else {
        return false;
    };
    let Some(reply) = lines[command_index + 1..]
        .iter()
        .find(|line| !line.is_empty())
    else {
        return false;
    };
    let Some(rest) = reply.strip_prefix(CLAUDE_APPLIED_PREFIX) else {
        return false;
    };
    let Some((label, tail)) = rest.split_once(CLAUDE_SESSION_ONLY_SUFFIX) else {
        return false;
    };
    claude_model_label_matches(label, model)
        && effort.is_none_or(|effort| tail.trim() == format!("with {effort} effort"))
}

/// Claude's notice of a pick from the list Option+P opened, once the list has closed.
fn claude_shortcut_pick_applied(screen: &str, model: &str, session_only: bool) -> bool {
    if claude_model_picker_open(screen)
        || crate::session_chat_composer::detect_session_chat_composer_readiness(
            Some("claude"),
            screen,
            None,
        )
        .state
            != crate::session_chat_composer::SessionChatComposerState::Ready
    {
        return false;
    }
    let suffix = if session_only {
        CLAUDE_SESSION_ONLY_SUFFIX
    } else {
        CLAUDE_SHORTCUT_DEFAULT_SUFFIX
    };
    screen_lines(screen)
        .iter()
        .rev()
        .filter(|line| !line.trim().is_empty())
        .take(CLAUDE_SHORTCUT_NOTICE_TAIL_LINES)
        .any(|line| {
            let Some(rest) = line
                .split_once(CLAUDE_SHORTCUT_PICK_PREFIX)
                .and_then(|(_, rest)| rest.trim_end().strip_suffix(suffix))
            else {
                return false;
            };
            // The alias Claude took, then the full model id in parentheses.
            let (alias, id) = rest
                .split_once(" (")
                .map_or((rest, ""), |(alias, id)| (alias, id.trim_end_matches(')')));
            alias == model || (!id.is_empty() && claude_model_label_matches(id, model))
        })
}

fn effort_rank(effort: &str) -> Option<usize> {
    CLAUDE_EFFORT_ORDER
        .iter()
        .position(|level| *level == effort)
}

fn cancelled_error() -> DomainStateError {
    agent_busy("The model change was cancelled by another action on this session.")
}

impl PickerDriver<'_> {
    /// The list as painted now, with its highlight.
    ///
    /// A clipped list counts only once two captures a moment apart read the same frame, and then
    /// ends the pick: neither the effort rail nor the proof that `s` picks for this session only
    /// is on screen. The error is final, so the list is not reopened every five seconds.
    async fn claude_model_list(&self, step: &str) -> Result<ClaudeModelList, DomainStateError> {
        let mut clipped: Option<(ClaudeModelList, std::time::Instant)> = None;
        let list = self
            .wait_for(step, |screen| {
                let list = claude_model_list(screen).filter(|list| list.selected().is_some())?;
                if list.footer != ClaudeListFooter::Clipped {
                    return Some(list);
                }
                match &clipped {
                    Some((seen, since))
                        if *seen == list
                            && since.elapsed()
                                >= std::time::Duration::from_millis(CLAUDE_CLIPPED_SETTLE_MS) =>
                    {
                        Some(list)
                    }
                    Some((seen, _)) if *seen == list => None,
                    _ => {
                        clipped = Some((list, std::time::Instant::now()));
                        None
                    }
                }
            })
            .await?;
        if list.footer == ClaudeListFooter::Clipped {
            return Err(unsupported_selection(
                "The terminal is too short to show Claude's model list. Make the pane taller and pick the model again.",
            ));
        }
        Ok(list)
    }

    /// Moves the highlight from where it is straight to row `to` in one burst of arrows, and
    /// returns the frame it landed on. The burst runs the direct way inside the list, so it never
    /// relies on the list wrapping round at its ends.
    async fn jump_claude_highlight(
        &self,
        list: ClaudeModelList,
        to: u32,
    ) -> Result<ClaudeModelList, DomainStateError> {
        let from = list.selected().ok_or_else(|| {
            dialog_mismatch(
                "choose Claude model",
                "Claude's model list highlights no row, so its distance cannot be counted.",
            )
        })?;
        if from == to {
            return Ok(list);
        }
        let (key, presses) = if to > from {
            (CLAUDE_ARROW_DOWN, to - from)
        } else {
            (CLAUDE_ARROW_UP, from - to)
        };
        self.write(&key.repeat(presses as usize)).await?;
        self.wait_for("move Claude model highlight", |screen| {
            claude_model_list(screen)
                .and_then(ClaudeModelList::painted)
                .filter(|list| list.selected() == Some(to))
        })
        .await
    }

    /// Walks the effort rail from `current` to `target`, the short way when both levels are known.
    /// Returns whether the rail moved, which decides how Claude words its acknowledgement.
    async fn set_claude_effort(
        &self,
        mut current: String,
        plan: &CodexPickerPlan,
    ) -> Result<bool, DomainStateError> {
        let target = plan.effort.as_str();
        let mut shown = vec![current.clone()];
        while current != target {
            if (self.cancelled)() {
                return Err(cancelled_error());
            }
            if shown.len() > CLAUDE_EFFORT_STEP_LIMIT {
                return Err(unsupported_selection(format!(
                    "Claude's effort rail does not offer {target} for {}.",
                    plan.model
                )));
            }
            let from = effort_rank(&current);
            let to = effort_rank(target);
            // An unknown level walks right: the rail wraps, so one direction reaches them all.
            let key = match (from, to) {
                (Some(from), Some(to)) if to < from => CLAUDE_ARROW_LEFT,
                _ => CLAUDE_ARROW_RIGHT,
            };
            self.write(key).await?;
            let landed = self
                .wait_for("confirm Claude effort", |screen| {
                    claude_model_list(screen)?
                        .painted()?
                        .effort
                        .filter(|landed| *landed != current)
                })
                .await?;
            // Stepping past the level, or wrapping round to one already shown, means this model
            // does not offer it: final, like a model the list does not show.
            let skipped = match (from, effort_rank(&landed), to) {
                (Some(from), Some(landed), Some(to)) if to > from => {
                    !(from < landed && landed <= to)
                }
                (Some(from), Some(landed), Some(to)) => !(to <= landed && landed < from),
                _ => shown.contains(&landed),
            };
            if landed != target && skipped {
                return Err(unsupported_selection(format!(
                    "Claude's effort rail does not offer {target} for {}.",
                    plan.model
                )));
            }
            shown.push(landed.clone());
            current = landed;
        }
        Ok(shown.len() > 1)
    }

    /// CDXC:SessionChat 2026-09-27 DECISION:
    /// User: switching models must be super fast; use the order of the models in Claude's own list to know which way to go instead of always walking down it. The pick reads the row numbers off the screen and jumps straight to the row in one burst of arrows, then moves the effort rail left or right, whichever reaches the level.
    /// Row digits are not an option: in Claude Code 2.1.281 a digit selects AND confirms its row as the saved default ("Enter to set as default"), which would break the session-only scope (verified live 2026-09-27; it rewrote `model` in `~/.claude/settings.json`).
    /// This supersedes walking to the top and stepping one row at a time with a settle per press, which took seconds and misread the end of a list: the list wraps round at both ends, so a press never "stops moving".
    /// SEE-ALSO: server/src/session_chat_model_selection.rs read_scope (the session-only scope).
    ///
    /// CDXC:SessionChat 2026-09-18 WHY:
    /// `/model <name>` and `/effort <name>` always save the pick as Claude's default for new
    /// sessions, so a session-only pick drives the bare `/model` list instead: move to the row,
    /// set the effort on its rail, and answer `s`.
    ///
    /// CDXC:SessionChat 2026-09-29 DECISION:
    /// User: when Claude Code's terminal input holds text and the user picks the model with the mouse, simulate Option+P and pick the model that way, since typing `/model` is blocked then. Option+P opens this same list over the draft and the draft stays in the input after `s` or Enter (verified live on Claude Code 2.1.284). The default scope comes here only then, with its effort cleared: Enter on the list saves the model but not the effort, so `/effort` still waits for the input. Ultracode waits too: its switch is only on the `/effort` slider.
    pub(super) async fn drive_claude_session_only(
        &self,
        plan: &CodexPickerPlan,
    ) -> Result<(), DomainStateError> {
        let screen = self
            .capture()
            .await
            .ok_or_else(|| session_not_running("The agent's input could not be read."))?;
        let composer = crate::session_chat_composer::detect_session_chat_composer_readiness(
            Some("claude"),
            &screen,
            None,
        );
        if composer.state != crate::session_chat_composer::SessionChatComposerState::Ready {
            return Err(super::input_box_refusal(
                &composer,
                "Waiting for Claude's input box.",
            ));
        }
        let over_draft = crate::session_chat_composer::claude_composer_input_text(&screen)
            .is_some_and(|text| !text.trim().is_empty())
            && self.claude_input_holds_draft().await;
        if !plan.session_only && !over_draft {
            // The draft went: the default scope's own `/model` path takes the pick on the retry.
            return Err(agent_busy("Waiting to apply the model with /model."));
        }
        let (model, effort) = super::claude_effort_slider::claude_live_selection(plan, &screen);
        let applied = |current: Option<&String>, value: &str| {
            value.is_empty() || current.is_some_and(|current| current == value)
        };
        let model_applied = applied(model.as_ref(), &plan.model);
        if model_applied && applied(effort.as_ref(), &plan.effort) {
            return Ok(());
        }
        if over_draft
            && (plan.effort == "ultracode"
                || (!plan.effort.is_empty() && effort.as_deref() == Some("ultracode")))
        {
            return Err(agent_busy(
                "Waiting for the text in Claude's terminal input to be sent or cleared.",
            ));
        }
        if model_applied && !over_draft {
            return self
                .drive_claude_effort_slider(&plan.effort, CLAUDE_SESSION_ONLY_KEY)
                .await;
        }

        if over_draft {
            self.write(CLAUDE_MODEL_SHORTCUT).await?;
        } else {
            self.write(&crate::session_chat_send::build_session_chat_paste_bytes(
                CLAUDE_MODEL_COMMAND,
            ))
            .await?;
            self.wait_for("type Claude model command", |screen| {
                crate::session_chat_composer::claude_composer_input_text(screen)
                    .is_some_and(|text| text.trim() == CLAUDE_MODEL_COMMAND)
                    .then_some(())
            })
            .await?;
            self.write(CODEX_SUBMIT).await?;
        }
        let list = self.claude_model_list("open Claude model list").await?;
        /*
        CDXC:AgentProviders 2026-09-28 WHY:
        Claude Code 2.1.283 lists no "(1M context)" row on accounts where the plain model already runs with a 1M window ("Opus 5.5" reports context_window_size 1000000), so a session-only pick of `opus[1m]` was refused and its effort never applied, leaving the chat on "Opus 5.5 · Medium". Pick the base row instead: on those builds it is the 1M model, and the statusline reading (`claude_statusline_model_choice`) reports the window Claude actually gives. One walk down the list looks for both rows, so the fallback costs no second pass.
        */
        let mut candidates = vec![plan.model.as_str()];
        if let Some(base) = plan.model.strip_suffix("[1m]") {
            candidates.push(base);
        }
        let (list, found) = self.find_claude_model_row(list, &candidates).await?;
        // The row the confirmation will name; the base row when the list has no 1M row.
        let Some(applied_model) = found.map(|index| candidates[index].to_string()) else {
            if (self.cancelled)() {
                return Err(cancelled_error());
            }
            return Err(unsupported_selection(format!(
                "Claude's model list does not offer {} in this session, so it cannot be applied without changing your default.",
                plan.model
            )));
        };
        if list.footer == ClaudeListFooter::CancelOnly {
            return Err(unsupported_selection(format!(
                "Claude Code must be updated before it can use {applied_model}."
            )));
        }

        // Ultracode is a switch beside the effort slider since Claude Code 2.1.284, not a level on
        // this rail: the rail takes Max and the slider below sets the switch.
        let requested_effort = plan.effort.clone();
        let mut rail_plan = plan.clone();
        if rail_plan.effort == "ultracode" {
            rail_plan.effort = "max".to_string();
        }
        let plan = &rail_plan;
        // A model without effort levels (Haiku) takes the pick without one.
        let mut effort_adjusted = false;
        if !plan.effort.is_empty() && !list.effort_unsupported {
            let current = match list.effort.clone() {
                Some(effort) => effort,
                None => {
                    self.wait_for("read Claude effort", |screen| {
                        claude_model_list(screen)?.painted()?.effort
                    })
                    .await?
                }
            };
            effort_adjusted = self.set_claude_effort(current, plan).await?;
        }

        let effort = effort_adjusted.then_some(plan.effort.as_str());
        self.write(if plan.session_only {
            CLAUDE_SESSION_ONLY_KEY
        } else {
            super::claude_effort_slider::CLAUDE_CONFIRM_KEY
        })
        .await?;
        let screen = self
            .await_claude_switch("applied Claude session model", |screen| {
                if over_draft {
                    claude_shortcut_pick_applied(screen, &applied_model, plan.session_only)
                } else {
                    // While a turn runs Claude switches at once but prints no "Set model to …" under
                    // the `/model` it echoed; its own model report then proves the pick, which
                    // differed from it before the key was pressed.
                    claude_session_only_applied(screen, &applied_model, effort)
                        || super::claude_effort_slider::claude_live_selection(plan, screen)
                            .0
                            .is_some_and(|live| {
                                live == applied_model
                                    || live.strip_suffix("[1m]") == Some(applied_model.as_str())
                            })
                }
            })
            .await?;
        // The Ultracode switch survives a model change, so the level alone is not the whole pick.
        // Over a draft Ultracode was ruled out above, and `/effort` would type into the draft.
        let wanted = requested_effort.as_str();
        if !over_draft && !wanted.is_empty() && !list.effort_unsupported {
            let (_, live) = super::claude_effort_slider::claude_live_selection(plan, &screen);
            if live.as_deref() != Some(wanted) {
                return self
                    .drive_claude_effort_slider(wanted, CLAUDE_SESSION_ONLY_KEY)
                    .await;
            }
        }
        Ok(())
    }

    /// CDXC:SessionChat 2026-09-27 WHY:
    /// The list paints only a window of its rows (ten on a tall pane, one on a short chat pane),
    /// and Claude reads labels loosely: "Opus 4.8" further down also names `opus`. So the row
    /// meant is the FIRST match from the top. Starting at row 1 and jumping a whole window at a
    /// time keeps that rule while reading each window once.
    ///
    /// `models` is in order of preference; the answer is the index of the one whose row the
    /// highlight moved to. The walk stops early only once the first choice is on screen.
    async fn find_claude_model_row(
        &self,
        mut list: ClaudeModelList,
        models: &[&str],
    ) -> Result<(ClaudeModelList, Option<usize>), DomainStateError> {
        if list.first() > 1 {
            list = self.jump_claude_highlight(list, 1).await?;
        }
        let mut rows: Vec<Option<u32>> = vec![None; models.len()];
        for _ in 0..CLAUDE_WINDOW_JUMP_LIMIT {
            if (self.cancelled)() {
                return Err(cancelled_error());
            }
            for (row, model) in rows.iter_mut().zip(models) {
                if row.is_none() {
                    *row = list.row_for(model);
                }
            }
            if rows.first().is_some_and(Option::is_some) {
                break;
            }
            let (last, total) = (list.last(), list.total());
            if last >= total {
                break;
            }
            // Moving past the window's bottom scrolls it until the highlight is its last row, so
            // this paints the next window whole.
            let window = list.rows.len() as u32;
            list = self
                .jump_claude_highlight(list, (last + window).min(total))
                .await?;
        }
        let Some((index, row)) = rows
            .iter()
            .enumerate()
            .find_map(|(index, row)| row.map(|row| (index, row)))
        else {
            return Ok((list, None));
        };
        list = self.jump_claude_highlight(list, row).await?;
        Ok((list, Some(index)))
    }
}
