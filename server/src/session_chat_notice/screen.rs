use super::*;

// ---------------------------------------------------------------------------
// Screen preparation
// ---------------------------------------------------------------------------

/// Banner-class window: an inline error/limit line only counts while it is
/// still on the live screen.
const NOTICE_BANNER_SCAN_LINES: usize = 15;
/// Dialog-class window: roughly one visible screen, never the whole scrollback.
const NOTICE_DIALOG_SCAN_LINES: usize = 60;
/// Exit signatures must be at the very bottom, right above the shell prompt.
pub(super) const NOTICE_EXIT_SCAN_LINES: usize = 10;
/// Evidence attached to a notice.
///
/// CDXC:SessionChat 2026-10-27 WHY: A card that answers a prompt opens its terminal output on the
/// lines above that prompt (the chat core's questions/notice_tail.rs, which shows at most 12 of
/// them). A Codex approval alone fills a dozen lines, so a 12-line tail held nothing above it and
/// the card lost its terminal output; the tail now reaches far enough up for both.
const NOTICE_SCREEN_TAIL_LINES: usize = 40;
const NOTICE_SCREEN_TAIL_MAX_CHARS: usize = 6000;
const NOTICE_LIVE_TAIL_LINES: usize = 24;
const NOTICE_LIVE_TAIL_MAX_CHARS: usize = 4000;
const NOTICE_EVIDENCE_MAX_CHARS: usize = 240;

/// One prepared capture: parallel display/folded views of the same tail lines,
/// oldest first.
pub(super) struct NoticeScreen {
    /// ANSI-stripped, whitespace-folded, trailing space removed — what the user
    /// sees, used for `screenTail` and quoted evidence.
    pub(super) display: Vec<String>,
    /// Additionally space-collapsed and apostrophe-folded — what patterns run
    /// against.
    pub(super) folded: Vec<String>,
    /// Flattened windows (`folded` joined by one space) for wrap tolerance.
    banner: String,
    dialog: String,
}

/// Claude renders `’`, codex renders `'`; both mean the same word.
fn fold_typographic(line: &str) -> String {
    line.chars()
        .map(|ch| match ch {
            '\u{2018}' | '\u{2019}' => '\'',
            '\u{201c}' | '\u{201d}' => '"',
            other => other,
        })
        .collect()
}

/// Collapses the space runs that folding NBSP/tabs produces, so one literal
/// matches however the TUI padded the line.
fn collapse_spaces(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut pending_space = false;
    for ch in line.chars() {
        if ch == ' ' {
            pending_space = !out.is_empty();
            continue;
        }
        if pending_space {
            out.push(' ');
            pending_space = false;
        }
        out.push(ch);
    }
    out
}

impl NoticeScreen {
    pub(super) fn new(text: &str) -> Self {
        let mut display: Vec<String> = Vec::new();
        let mut folded: Vec<String> = Vec::new();
        for raw in text.lines().rev() {
            let line = normalize_spaces(&strip_ansi_sgr(raw))
                .trim_end()
                .to_string();
            if line.trim().is_empty() {
                continue;
            }
            folded.push(collapse_spaces(&fold_typographic(&line)));
            display.push(line);
            if display.len() >= NOTICE_DIALOG_SCAN_LINES {
                break;
            }
        }
        display.reverse();
        folded.reverse();
        let banner = flatten_tail(&folded, NOTICE_BANNER_SCAN_LINES);
        let dialog = flatten_tail(&folded, NOTICE_DIALOG_SCAN_LINES);
        Self {
            display,
            folded,
            banner,
            dialog,
        }
    }

    pub(super) fn window(&self, scope: NoticeScope) -> &str {
        match scope {
            NoticeScope::Banner => &self.banner,
            NoticeScope::Dialog => &self.dialog,
            NoticeScope::Exit => &self.banner,
        }
    }

    /// A Codex update chooser is stale once a later bare composer row exists.
    /// The selected update row also starts with `›`, so numbered rows do not
    /// count as composer evidence.
    pub(super) fn has_codex_composer_after(&self, needle: &str) -> bool {
        let Some(notice_index) = self.folded.iter().rposition(|line| line.contains(needle)) else {
            return false;
        };
        self.display
            .iter()
            .skip(notice_index + 1)
            .any(|line| is_codex_composer_line(line))
    }

    /// CDXC:AgentScreenDetection 2026-09-24 WHY:
    /// Claude's dialogs replace its input box, so a ready composer painted below the newest line carrying a dialog phrase proves that line is transcript text: an agent reply quoting "Do you trust the files in this folder?" refused every send with a hidden folder-trust notice.
    pub(super) fn has_claude_composer_after(&self, needle: &str) -> bool {
        let Some(index) = self.folded.iter().rposition(|line| line.contains(needle)) else {
            return false;
        };
        crate::session_chat_composer::detect_session_chat_composer_ready(
            Some("claude"),
            &self.display[index + 1..].join("\n"),
        )
        .state
            == crate::session_chat_composer::SessionChatComposerState::Ready
    }

    /// CDXC:AgentScreenDetection 2026-09-05 WHY:
    /// A successful `/model` command can leave the previous model's quota error within the banner window indefinitely.
    /// Its confirmation followed by a normal composer supersedes that evidence, but says nothing about the new model's quota; any later limit still counts.
    pub(super) fn has_claude_model_switch_after(&self, signature: &NoticeSignature) -> bool {
        let Some(command_index) = self.folded.windows(2).rposition(|pair| {
            pair[0]
                .trim_start()
                .strip_prefix("❯ /model")
                .is_some_and(|rest| rest.is_empty() || rest.starts_with(' '))
                && pair[1].trim_start().starts_with("⎿ Set model to ")
        }) else {
            return false;
        };
        let after_switch = &self.folded[command_index + 1..];
        !matches_parts(&after_switch.join(" "), signature.parts)
            && crate::session_chat_composer::detect_session_chat_composer_ready(
                Some("claude"),
                &after_switch.join("\n"),
            )
            .state
                == crate::session_chat_composer::SessionChatComposerState::Ready
    }

    /// CDXC:AgentScreenDetection 2026-09-08 DECISION:
    /// User: a later "Login successful" clears Claude's earlier login-expired warning, including a compaction failure, even while that error remains in terminal scrollback.
    pub(super) fn has_claude_login_success_after(&self, signature: &NoticeSignature) -> bool {
        let Some(success_index) = self
            .folded
            .iter()
            .rposition(|line| line.trim_start_matches('⎿').trim() == "Login successful")
        else {
            return false;
        };
        !matches_parts(&self.folded[success_index + 1..].join(" "), signature.parts)
    }

    /// The newest displayed line carrying `needle`, capped. Absent when the
    /// phrase only matched across a wrap.
    pub(super) fn evidence(&self, needle: &str) -> Option<String> {
        let index = self.folded.iter().rposition(|line| line.contains(needle))?;
        let line = self.display.get(index)?.trim();
        (!line.is_empty()).then(|| cap_chars_from_end(line, NOTICE_EVIDENCE_MAX_CHARS))
    }

    /*
    CDXC:AgentScreenDetection 2026-08-19:
    Agent TUIs frame their footers with full-width rules that carry no
    information but eat the whole card width. A rule row becomes a blank line —
    the break it was drawing is kept, the wall of glyphs is not — and it does
    not spend the content budget, so the tail still carries
    `NOTICE_SCREEN_TAIL_LINES` real lines.
    */
    pub(super) fn screen_tail(&self) -> Option<String> {
        self.tail(NOTICE_SCREEN_TAIL_LINES, NOTICE_SCREEN_TAIL_MAX_CHARS)
    }

    /// The tail a pass-through card draws live: a whole picker, not a few lines of evidence.
    pub(super) fn live_tail(&self) -> Option<String> {
        self.tail(NOTICE_LIVE_TAIL_LINES, NOTICE_LIVE_TAIL_MAX_CHARS)
    }

    fn tail(&self, max_lines: usize, max_chars: usize) -> Option<String> {
        let mut newest_first: Vec<&str> = Vec::new();
        let mut content_lines = 0usize;
        for line in self.display.iter().rev() {
            if content_lines >= max_lines {
                break;
            }
            let line = line.trim_end();
            if is_decoration_line(line) {
                newest_first.push("");
                continue;
            }
            content_lines += 1;
            newest_first.push(line);
        }
        let mut tail_lines: Vec<&str> = Vec::new();
        for line in newest_first.into_iter().rev() {
            // Collapse runs of blanks (original or rule-produced) and drop the
            // leading ones outright.
            if line.is_empty() && tail_lines.last().is_none_or(|last| last.is_empty()) {
                continue;
            }
            tail_lines.push(line);
        }
        while tail_lines.last().is_some_and(|line| line.is_empty()) {
            tail_lines.pop();
        }
        let tail = tail_lines.join("\n");
        (!tail.trim().is_empty()).then(|| cap_chars_from_end(&tail, max_chars))
    }

    /// A bare shell prompt as the bottom line is what distinguishes "the agent
    /// exited" from "the agent printed something that looks like an exit".
    pub(super) fn ends_with_shell_prompt(&self) -> bool {
        let Some(line) = self.folded.last().map(|line| line.trim()) else {
            return false;
        };
        // `›` is codex's own composer marker: an alive TUI, never a shell.
        if line.starts_with('\u{203a}') || line.len() > 200 {
            return false;
        }
        // PowerShell's default prompt ends in `>`, which is also common in
        // agent prose. Require the PowerShell prefix and a filesystem path.
        if let Some(path) = line
            .strip_prefix("PS ")
            .and_then(|line| line.strip_suffix('>'))
        {
            let path = path.trim();
            if path.starts_with("\\\\")
                || (path.as_bytes().first().is_some_and(u8::is_ascii_alphabetic)
                    && path.as_bytes().get(1) == Some(&b':')
                    && matches!(path.as_bytes().get(2), Some(b'\\' | b'/')))
            {
                return true;
            }
        }
        matches!(
            line.chars().last(),
            Some('$') | Some('%') | Some('#') | Some('\u{276f}') | Some('\u{279c}')
        )
    }
}

fn is_codex_composer_line(line: &str) -> bool {
    let Some(rest) = line.trim().strip_prefix('›') else {
        return false;
    };
    let mut chars = rest.trim_start().chars();
    let mut digits = 0usize;
    for character in chars.by_ref() {
        if character.is_ascii_digit() {
            digits += 1;
            continue;
        }
        return digits == 0 || !matches!(character, '.' | ')');
    }
    digits == 0
}

pub(super) fn flatten_tail(folded: &[String], lines: usize) -> String {
    let start = folded.len().saturating_sub(lines);
    folded[start..].join(" ")
}

/// The ASCII/typographic rule characters TUIs pad separators with. Box drawing
/// (U+2500–U+257F) and block elements (U+2580–U+259F) are one contiguous range,
/// so they are matched separately.
const NOTICE_DECORATION_CHARS: &[char] = &[
    '-', '=', '_', '*', '~', '#', '+', '.', '\u{00b7}', '\u{2022}', '\u{2014}', '\u{2013}',
];

fn is_decoration_char(ch: char) -> bool {
    matches!(ch, '\u{2500}'..='\u{259f}') || NOTICE_DECORATION_CHARS.contains(&ch)
}

/// A separator row: non-empty, and every non-whitespace character is
/// decoration. `──── session ──` mixes in real text, so it is not one.
fn is_decoration_line(line: &str) -> bool {
    let mut saw_decoration = false;
    for ch in line.chars() {
        if ch.is_whitespace() {
            continue;
        }
        if !is_decoration_char(ch) {
            return false;
        }
        saw_decoration = true;
    }
    saw_decoration
}

/// Keeps the NEWEST characters: the bottom of the screen is the evidence.
fn cap_chars_from_end(text: &str, max_chars: usize) -> String {
    let count = text.chars().count();
    if count <= max_chars {
        return text.to_string();
    }
    text.chars().skip(count - max_chars).collect()
}

// ---------------------------------------------------------------------------
// Phrase matcher (no regex dependency — see the module header)
// ---------------------------------------------------------------------------

pub(super) enum NoticePart {
    /// Literal, matched against the space-collapsed window.
    Text(&'static str),
    /// Up to N arbitrary characters: separators, names, reasons, wrap padding.
    Gap(usize),
    /// One ASCII digit (versions, retry counters).
    Digit,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum NoticeScope {
    /// Last few non-blank lines only.
    Banner,
    /// Anywhere on the visible screen.
    Dialog,
    /// Last few lines AND a shell prompt at the bottom.
    Exit,
}

pub(super) struct NoticeSignature {
    pub(super) scope: NoticeScope,
    /// Always starts with a `Text` part: that literal seeds the scan.
    pub(super) parts: &'static [NoticePart],
    /// Any-of literals that must also be in the same window. Empty ⇒ the
    /// phrase stands alone.
    pub(super) corroborators: &'static [&'static str],
}

fn matches_from(hay: &str, at: usize, parts: &[NoticePart]) -> bool {
    let Some((part, rest)) = parts.split_first() else {
        return true;
    };
    match part {
        NoticePart::Text(text) => {
            hay[at..].starts_with(text) && matches_from(hay, at + text.len(), rest)
        }
        NoticePart::Digit => {
            hay[at..]
                .chars()
                .next()
                .is_some_and(|ch| ch.is_ascii_digit())
                && matches_from(hay, at + 1, rest)
        }
        NoticePart::Gap(max) => {
            let mut cursor = at;
            for _ in 0..=*max {
                if matches_from(hay, cursor, rest) {
                    return true;
                }
                let Some(ch) = hay[cursor..].chars().next() else {
                    return false;
                };
                cursor += ch.len_utf8();
            }
            false
        }
    }
}

pub(super) fn matches_parts(hay: &str, parts: &[NoticePart]) -> bool {
    let Some(NoticePart::Text(first)) = parts.first() else {
        return false;
    };
    hay.match_indices(first)
        .any(|(index, _)| matches_from(hay, index + first.len(), &parts[1..]))
}

/// The signature's leading literal, used to quote the line it matched on.
pub(super) fn signature_needle(signature: &NoticeSignature) -> Option<&'static str> {
    match signature.parts.first() {
        Some(NoticePart::Text(text)) => Some(text),
        _ => None,
    }
}
