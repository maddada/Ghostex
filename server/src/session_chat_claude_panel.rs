//! Claude's panels read with their styling: the `/btw` side question and the Settings-style panels.
//!
//! `detect_claude_dialog` reads the plain capture, which loses what makes a panel legible in chat:
//! which words are bold, which are code, which tab is selected. This module reads the VT capture of
//! the same panel and adds what only the styling can tell.

use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::domain::DomainRepository;
use crate::session_chat_notice::SessionChatTerminalNotice;
use crate::session_chat_options::normalize_spaces;
use crate::session_chat_send::{
    capture_session_terminal_text_vt, execute_session_chat_send, write_session_chat_payload,
    SessionChatSendStep,
};

// ---------------------------------------------------------------------------
// Styled rows
// ---------------------------------------------------------------------------

/// The SGR state a run of text was painted with. Only what the readers below ask about is kept.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Pen {
    bold: bool,
    dim: bool,
    italic: bool,
    underline: bool,
    fg: Option<(u8, u8, u8)>,
    fg_indexed: bool,
    bg: bool,
}

impl Pen {
    /// Claude paints secondary text in a neutral grey (or dim), never in a hue.
    fn muted(&self) -> bool {
        self.dim
            || self.fg.is_some_and(|(r, g, b)| {
                r.abs_diff(g) < 12 && g.abs_diff(b) < 12 && r.abs_diff(b) < 12 && r < 200
            })
    }

    /// A coloured run inside prose: Claude's inline code, file paths and similar spans.
    fn coloured(&self) -> bool {
        (self.fg.is_some() || self.fg_indexed) && !self.muted()
    }
}

#[derive(Clone, Debug)]
struct Span {
    text: String,
    pen: Pen,
}

#[derive(Clone, Debug, Default)]
struct Row {
    spans: Vec<Span>,
    plain: String,
}

impl Row {
    fn indent(&self) -> usize {
        self.plain.chars().take_while(|ch| *ch == ' ').count()
    }

    fn trimmed(&self) -> &str {
        self.plain.trim()
    }

    fn is_blank(&self) -> bool {
        self.plain.trim().is_empty()
    }

    /// Every visible character bold (a heading or a bold-only line).
    fn all_bold(&self) -> bool {
        let mut seen = false;
        for span in &self.spans {
            if span.text.trim().is_empty() {
                continue;
            }
            if !span.pen.bold {
                return false;
            }
            seen = true;
        }
        seen
    }

    fn all_muted(&self) -> bool {
        let mut seen = false;
        for span in &self.spans {
            if span.text.trim().is_empty() {
                continue;
            }
            if !span.pen.muted() {
                return false;
            }
            seen = true;
        }
        seen
    }

    fn any_bold(&self) -> bool {
        self.spans
            .iter()
            .any(|span| span.pen.bold && !span.text.trim().is_empty())
    }
}

fn apply_sgr(params: &str, pen: &mut Pen) {
    // `ESC [ > 4 ; 2 m` and its kin set keyboard modes, not the pen.
    if params.starts_with(['<', '=', '>', '?']) {
        return;
    }
    let codes: Vec<u32> = params
        .split([';', ':'])
        .map(|code| code.parse::<u32>().unwrap_or(0))
        .collect();
    let mut index = 0;
    while index < codes.len() {
        match codes[index] {
            0 => *pen = Pen::default(),
            1 => pen.bold = true,
            2 => pen.dim = true,
            3 => pen.italic = true,
            4 => pen.underline = true,
            22 => {
                pen.bold = false;
                pen.dim = false;
            }
            23 => pen.italic = false,
            24 => pen.underline = false,
            7 => pen.bg = true,
            27 => pen.bg = false,
            30..=37 | 90..=97 => {
                pen.fg = None;
                pen.fg_indexed = true;
            }
            39 => {
                pen.fg = None;
                pen.fg_indexed = false;
            }
            40..=47 | 100..=107 => pen.bg = true,
            49 => pen.bg = false,
            code @ (38 | 48) => {
                let (colour, used) = match codes.get(index + 1) {
                    Some(2) => (
                        Some((
                            codes.get(index + 2).copied().unwrap_or(0) as u8,
                            codes.get(index + 3).copied().unwrap_or(0) as u8,
                            codes.get(index + 4).copied().unwrap_or(0) as u8,
                        )),
                        4,
                    ),
                    Some(5) => (None, 2),
                    _ => (None, 0),
                };
                if code == 38 {
                    pen.fg = colour;
                    pen.fg_indexed = colour.is_none() && used == 2;
                } else {
                    pen.bg = true;
                }
                index += used;
            }
            _ => {}
        }
        index += 1;
    }
}

/// One row of a VT capture as painted runs. The pen carries over from row to row.
fn styled_row(line: &str, pen: &mut Pen) -> Row {
    let mut spans: Vec<Span> = Vec::new();
    let mut text = String::new();
    let mut chars = line.chars().peekable();
    let flush = |spans: &mut Vec<Span>, text: &mut String, pen: Pen| {
        if !text.is_empty() {
            spans.push(Span {
                text: normalize_spaces(text),
                pen,
            });
            text.clear();
        }
    };
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            continue;
        }
        if ch != '\u{1b}' {
            text.push(ch);
            continue;
        }
        match chars.next() {
            Some('[') => {
                let mut params = String::new();
                for inner in chars.by_ref() {
                    if ('@'..='~').contains(&inner) {
                        if inner == 'm' {
                            flush(&mut spans, &mut text, *pen);
                            apply_sgr(&params, pen);
                        }
                        // Blank cells the capture skipped, as `strip_ansi_sgr` spells them.
                        if inner == 'C' {
                            text.extend(std::iter::repeat_n(
                                ' ',
                                crate::session_chat_options::cursor_forward_cells(&params),
                            ));
                        }
                        break;
                    }
                    params.push(inner);
                }
            }
            Some(']') => {
                while let Some(inner) = chars.next() {
                    if inner == '\u{7}' {
                        break;
                    }
                    if inner == '\u{1b}' && chars.peek() == Some(&'\\') {
                        chars.next();
                        break;
                    }
                }
            }
            Some('(' | ')' | '*' | '+') => {
                chars.next();
            }
            _ => {}
        }
    }
    flush(&mut spans, &mut text, *pen);
    let plain = spans
        .iter()
        .map(|span| span.text.as_str())
        .collect::<String>()
        .trim_end()
        .to_string();
    Row { spans, plain }
}

/// The rows of the last Claude panel on screen, below its `▔` rule, trailing blanks dropped.
fn panel_rows(styled: &str) -> Option<Vec<Row>> {
    let mut pen = Pen::default();
    let rows: Vec<Row> = styled
        .split('\n')
        .map(|line| styled_row(line, &mut pen))
        .collect();
    let start = rows.iter().rposition(|row| {
        let line = row.trimmed();
        line.chars().count() >= 20 && line.chars().all(|c| c == '▔')
    })? + 1;
    let mut panel = rows[start..].to_vec();
    while panel.last().is_some_and(Row::is_blank) {
        panel.pop();
    }
    (!panel.is_empty()).then_some(panel)
}

fn is_hint(line: &str) -> bool {
    let lower = line.to_ascii_lowercase();
    lower.contains("press y to confirm")
        || lower.contains("esc to")
        || lower.contains("esc cancel")
        || lower.contains("enter to")
        || lower.contains(" to search")
        || lower.contains(" to filter")
        || lower.contains(" to sort")
        || lower.contains("←/→")
        || lower.contains("↑/↓")
        || lower.contains("d to day")
}

// ---------------------------------------------------------------------------
// The `/btw` side question
// ---------------------------------------------------------------------------

/// What the `/btw` panel shows: the selected question and its answer.
struct SideQuestionScreen {
    /// Every question line, in order, so a new question (one more line) is a new identity.
    questions: Vec<String>,
    selected: usize,
    body: Vec<Row>,
    footer: String,
}

impl SideQuestionScreen {
    fn question(&self) -> &str {
        self.questions[self.selected]
            .trim()
            .strip_prefix("/btw")
            .unwrap_or_default()
            .trim()
    }

    fn answering(&self) -> bool {
        let lines: Vec<&str> = self
            .body
            .iter()
            .map(Row::trimmed)
            .filter(|line| !line.is_empty())
            .collect();
        lines.is_empty()
            || (lines.len() == 1
                && (lines[0].ends_with("Answering…") || lines[0].ends_with("Answering...")))
    }

    fn complete(&self) -> bool {
        !self.answering() && self.footer.to_ascii_lowercase().contains("c to copy")
    }

    /// The newest question is the live one; Claude also lists earlier ones for browsing.
    fn is_latest(&self) -> bool {
        self.selected + 1 == self.questions.len()
    }

    fn identity(&self) -> u64 {
        let mut hash = std::collections::hash_map::DefaultHasher::new();
        self.questions.hash(&mut hash);
        self.selected.hash(&mut hash);
        hash.finish()
    }

    fn window(&self) -> Vec<String> {
        self.body.iter().map(|row| row.plain.clone()).collect()
    }
}

/// CDXC:SessionChat 2026-09-27 WHY: Claude lists earlier side questions above the current one (browsed with shift+arrows). The live question is painted with a bold amber `/btw`, a browsed earlier one is bold throughout, and the rest are grey, so bold is what marks the question whose answer is showing.
fn side_question_screen(styled: &str) -> Option<SideQuestionScreen> {
    let rows = panel_rows(styled)?;
    let footer_at = rows.iter().rposition(|row| !row.is_blank())?;
    let footer = rows[footer_at].trimmed().to_string();
    if !footer.to_ascii_lowercase().contains("esc to close") {
        return None;
    }
    let first = rows.iter().position(|row| !row.is_blank())?;
    let mut questions = Vec::new();
    let mut selected = None;
    let mut at = first;
    while at < footer_at {
        let row = &rows[at];
        let line = row.trimmed();
        // A long history starts with a count of the questions scrolled out of the list.
        if line.starts_with("(+") && line.ends_with(" earlier /btw)") {
            at += 1;
            continue;
        }
        if !(line == "/btw" || line.starts_with("/btw ")) {
            break;
        }
        if row.any_bold() {
            selected = Some(questions.len());
        }
        questions.push(line.to_string());
        at += 1;
    }
    if questions.is_empty() {
        return None;
    }
    let selected = selected.unwrap_or(questions.len() - 1);
    let mut body: Vec<Row> = rows[at..footer_at].to_vec();
    while body.first().is_some_and(Row::is_blank) {
        body.remove(0);
    }
    while body.last().is_some_and(Row::is_blank) {
        body.pop();
    }
    Some(SideQuestionScreen {
        questions,
        selected,
        body,
        footer,
    })
}

/// Whether Claude's `/btw` panel is on screen (plain or VT capture), for the send path to close it.
pub(crate) fn side_question_on_screen(screen: &str) -> bool {
    side_question_screen(screen).is_some()
}

// ---------------------------------------------------------------------------
// The answer as Markdown
// ---------------------------------------------------------------------------

fn escape_markdown(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        if matches!(ch, '\\' | '`' | '*' | '_' | '<') {
            out.push('\\');
        }
        out.push(ch);
    }
    out
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Inline {
    Plain,
    Code,
    Bold,
    Italic,
}

/// One row's text with Claude's styling turned back into inline Markdown: bold runs, italics, and
/// the coloured runs Claude paints inline code in. Neighbouring runs of one kind are merged first,
/// so a path painted in two colours is still one code span.
fn inline_markdown(spans: &[Span]) -> String {
    let mut runs: Vec<(Inline, String)> = Vec::new();
    for span in spans {
        let kind = if span.text.trim().is_empty() {
            runs.last().map(|(kind, _)| *kind).unwrap_or(Inline::Plain)
        } else if span.pen.coloured() && !span.pen.bold && !span.pen.underline {
            Inline::Code
        } else if span.pen.bold {
            Inline::Bold
        } else if span.pen.italic {
            Inline::Italic
        } else {
            Inline::Plain
        };
        match runs.last_mut() {
            Some((last, text)) if *last == kind => text.push_str(&span.text),
            _ => runs.push((kind, span.text.clone())),
        }
    }
    let mut out = String::new();
    for (kind, text) in runs {
        let lead = &text[..text.len() - text.trim_start().len()];
        let trail = &text[text.trim_end().len()..];
        let core = text.trim();
        out.push_str(lead);
        if !core.is_empty() {
            match kind {
                Inline::Code => {
                    let fence = if core.contains('`') { "``" } else { "`" };
                    out.push_str(&format!("{fence}{core}{fence}"));
                }
                Inline::Bold => out.push_str(&format!("**{}**", escape_markdown(core))),
                Inline::Italic => out.push_str(&format!("*{}*", escape_markdown(core))),
                Inline::Plain => out.push_str(&escape_markdown(core)),
            }
        }
        out.push_str(trail);
    }
    out
}

/// The list marker a row starts with and how many characters it takes (`- `, `12. `).
fn list_marker(text: &str) -> Option<(String, usize)> {
    for bullet in ["- ", "* ", "• "] {
        if text.starts_with(bullet) {
            return Some(("-".to_string(), bullet.chars().count()));
        }
    }
    let digits: String = text.chars().take_while(char::is_ascii_digit).collect();
    if !digits.is_empty() && digits.len() <= 3 {
        let rest = &text[digits.len()..];
        if rest.starts_with(". ") || rest.starts_with(") ") {
            return Some((format!("{digits}."), digits.len() + 2));
        }
    }
    None
}

fn is_drawing(text: &str) -> bool {
    text.chars().any(|ch| {
        matches!(ch, '\u{2500}'..='\u{257f}' | '\u{2580}'..='\u{259f}' | '\u{2800}'..='\u{28ff}')
    })
}

/// A row's runs without their first `count` characters (its indent, or indent and list marker).
fn spans_after(row: &Row, count: usize) -> Vec<Span> {
    let mut remaining = count;
    let mut spans = Vec::new();
    for span in &row.spans {
        let width = span.text.chars().count();
        if remaining >= width {
            remaining -= width;
            continue;
        }
        spans.push(Span {
            text: span.text.chars().skip(remaining).collect(),
            pen: span.pen,
        });
        remaining = 0;
    }
    spans
}

enum AnswerBlock {
    Paragraph {
        text: String,
        heading: bool,
    },
    Item {
        depth: usize,
        content_indent: usize,
        text: String,
    },
    Fence(Vec<String>),
}

/*
CDXC:SessionChat 2026-09-27 WHY:
Claude never records a side answer in its transcript, and its "c to copy" key writes the Markdown
to the computer's clipboard, so gxserver reads the answer off the panel and rebuilds the Markdown
from the painting: the panel margin goes, the terminal's line breaks are joined back into
sentences and list items, bold lines become bold paragraphs, and the coloured runs Claude paints
code in become inline code. Box-drawn tables and indented blocks stay in a code fence.
*/
fn answer_markdown(body: &[Row]) -> String {
    let margin = body
        .iter()
        .filter(|row| !row.is_blank())
        .map(Row::indent)
        .min()
        .unwrap_or(0);
    let mut blocks: Vec<AnswerBlock> = Vec::new();
    // Indents of the list items open above, outermost first.
    let mut list: Vec<usize> = Vec::new();
    let mut after_blank = true;
    for row in body {
        if row.is_blank() {
            after_blank = true;
            continue;
        }
        let indent = row.indent().saturating_sub(margin);
        let content: String = row.plain.chars().skip(row.indent()).collect();
        let verbatim: String = row.plain.chars().skip(margin).collect();
        if is_drawing(&content) {
            match blocks.last_mut() {
                Some(AnswerBlock::Fence(lines)) if !after_blank => lines.push(verbatim),
                _ => blocks.push(AnswerBlock::Fence(vec![verbatim])),
            }
            list.clear();
            after_blank = false;
            continue;
        }
        if let Some((marker, width)) = list_marker(&content) {
            while list.last().is_some_and(|&open| open > indent) {
                list.pop();
            }
            if list.last() != Some(&indent) {
                list.push(indent);
            }
            let text = inline_markdown(&spans_after(row, row.indent() + width));
            blocks.push(AnswerBlock::Item {
                depth: list.len() - 1,
                content_indent: indent + width,
                text: format!("{marker} {}", text.trim()),
            });
            after_blank = false;
            continue;
        }
        let heading = row.all_bold();
        let text = if heading {
            format!("**{}**", escape_markdown(content.trim()))
        } else {
            inline_markdown(&spans_after(row, row.indent()))
                .trim()
                .to_string()
        };
        match blocks.last_mut() {
            // A hanging-indented row continues the list item above it.
            Some(AnswerBlock::Item {
                content_indent,
                text: item,
                ..
            }) if !after_blank && indent >= *content_indent && indent > 0 => {
                item.push(' ');
                item.push_str(&text);
            }
            Some(AnswerBlock::Fence(lines)) if !after_blank && indent > 0 => lines.push(verbatim),
            // A wrapped paragraph row.
            Some(AnswerBlock::Paragraph {
                text: paragraph,
                heading: false,
            }) if !after_blank && !heading => {
                paragraph.push(' ');
                paragraph.push_str(&text);
            }
            _ if indent >= 2 && after_blank && list.is_empty() => {
                blocks.push(AnswerBlock::Fence(vec![verbatim]));
            }
            _ => {
                if after_blank || heading {
                    list.clear();
                }
                blocks.push(AnswerBlock::Paragraph { text, heading });
            }
        }
        after_blank = false;
    }
    let mut out = String::new();
    let mut previous_item = false;
    for block in blocks {
        let item = matches!(block, AnswerBlock::Item { .. });
        if !out.is_empty() {
            out.push_str(if item && previous_item { "\n" } else { "\n\n" });
        }
        previous_item = item;
        match block {
            AnswerBlock::Paragraph { text, .. } => out.push_str(&text),
            AnswerBlock::Item { depth, text, .. } => {
                out.push_str(&"   ".repeat(depth));
                out.push_str(&text);
            }
            AnswerBlock::Fence(lines) => {
                let common = lines
                    .iter()
                    .filter(|line| !line.trim().is_empty())
                    .map(|line| line.chars().take_while(|ch| *ch == ' ').count())
                    .min()
                    .unwrap_or(0);
                out.push_str("```\n");
                for line in lines {
                    out.push_str(line.chars().skip(common).collect::<String>().trim_end());
                    out.push('\n');
                }
                out.push_str("```");
            }
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Settings-style panels as blocks
// ---------------------------------------------------------------------------

/// A `Label:   value` or `label   value` line, split into its two columns.
fn key_value(text: &str) -> Option<(String, String)> {
    let trimmed = text.trim();
    // Aligned columns, or a `Label: value` whose long label left room for one space only.
    let (key, value) = match trimmed.find("  ") {
        Some(gap) => (trimmed[..gap].trim(), trimmed[gap..].trim()),
        None => {
            let (key, value) = trimmed.split_once(": ")?;
            let words = key.split_whitespace().count();
            if words > 4 || key.contains(['.', ',', '!', '?']) {
                return None;
            }
            (key, value)
        }
    };
    if !key.chars().any(char::is_alphanumeric)
        || value.is_empty()
        || key.chars().count() > 48
        || value.contains("   ")
    {
        return None;
    }
    Some((key.trim_end_matches(':').to_string(), value.to_string()))
}

/// `█████      10% used`: a usage bar and the number beside it.
fn meter(text: &str) -> Option<(f64, String)> {
    if !text.chars().any(|ch| matches!(ch, '\u{2580}'..='\u{259f}')) {
        return None;
    }
    let value: String = text
        .chars()
        .skip_while(|ch| matches!(ch, '\u{2580}'..='\u{259f}') || ch.is_whitespace())
        .collect();
    let value = value.trim().to_string();
    let percent: String = value
        .chars()
        .take_while(|ch| ch.is_ascii_digit() || *ch == '.')
        .collect();
    let percent = percent.parse::<f64>().ok()?;
    value
        .contains('%')
        .then_some((percent.clamp(0.0, 100.0), value))
}

/// The tab strip on a panel's title row (`Settings  Status   Config   Usage   Stats`): the panel
/// name, then each tab, the selected one painted on a background.
fn tabs(row: &Row) -> Option<Value> {
    let mut column = 0;
    let mut selected_columns = None;
    for span in &row.spans {
        let width = span.text.chars().count();
        if span.pen.bg && !span.text.trim().is_empty() {
            selected_columns = Some(column..column + width);
        }
        column += width;
    }
    let selected_columns = selected_columns?;
    let mut labels: Vec<(String, std::ops::Range<usize>)> = Vec::new();
    let chars: Vec<char> = row.plain.chars().collect();
    let mut at = 0;
    while at < chars.len() {
        if chars[at] == ' ' {
            at += 1;
            continue;
        }
        let start = at;
        while at < chars.len() && !(chars[at] == ' ' && chars.get(at + 1).is_none_or(|c| *c == ' '))
        {
            at += 1;
        }
        labels.push((chars[start..at].iter().collect(), start..at));
    }
    if labels.len() < 3 {
        return None;
    }
    let title = labels.remove(0).0;
    let tabs: Vec<Value> = labels
        .iter()
        .map(|(label, range)| {
            json!({
                "label": label,
                "selected": range.start < selected_columns.end && selected_columns.start < range.end,
            })
        })
        .collect();
    tabs.iter()
        .any(|tab| tab["selected"] == true)
        .then(|| json!({ "type": "tabs", "title": title, "tabs": tabs }))
}

/*
CDXC:SessionChat 2026-09-27 DECISION:
User chose to render Claude's other panels as the mockup's screen 6 shows: "Label: value" lines become a table (`/status`), usage bars become meters (`/usage`), the Settings tab strip becomes tabs, and anything not recognised stays in the code font with the shared left margin trimmed and no re-wrapping. The panel's key-hint line gives way to buttons.
*/
fn panel_blocks(rows: &[Row]) -> Option<Vec<Value>> {
    let first = rows.iter().position(|row| !row.is_blank())?;
    let mut blocks: Vec<Value> = Vec::new();
    let mut structured = false;
    let mut body_start = first;
    if let Some(strip) = tabs(&rows[first]) {
        blocks.push(strip);
        structured = true;
    }
    // The title row is the card's title (or the tab strip), never body text.
    body_start += 1;
    let body: Vec<&Row> = rows[body_start..]
        .iter()
        .filter(|row| {
            let line = row.trimmed();
            !is_hint(line)
                && !line.contains('⌕')
                // The search field's empty box; its corners close it on the right.
                && !(line.starts_with(['╭', '╰'])
                    && line
                        .trim_end_matches(['╮', '╯'])
                        .chars()
                        .skip(1)
                        .all(|c| c == '─'))
        })
        .collect();
    let mut pre: Vec<String> = Vec::new();
    let flush_pre = |pre: &mut Vec<String>, blocks: &mut Vec<Value>| {
        if pre.is_empty() {
            return;
        }
        let common = pre
            .iter()
            .filter(|line| !line.trim().is_empty())
            .map(|line| line.chars().take_while(|ch| *ch == ' ').count())
            .min()
            .unwrap_or(0);
        let text = pre
            .iter()
            .map(|line| line.chars().skip(common).collect::<String>())
            .collect::<Vec<_>>()
            .join("\n");
        blocks.push(json!({ "type": "pre", "text": text }));
        pre.clear();
    };
    let mut index = 0;
    while index < body.len() {
        let row = body[index];
        index += 1;
        if row.is_blank() {
            flush_pre(&mut pre, &mut blocks);
            continue;
        }
        let line = row.trimmed();
        if let Some((percent, value)) = meter(line) {
            flush_pre(&mut pre, &mut blocks);
            let label = match blocks.last() {
                Some(block) if block["type"] == "heading" => blocks
                    .pop()
                    .and_then(|block| block["text"].as_str().map(str::to_string)),
                _ => None,
            };
            let detail = body
                .get(index)
                .map(|next| next.trimmed())
                .filter(|next| next.starts_with("Resets") || next.starts_with("Reset "))
                .map(str::to_string);
            if detail.is_some() {
                index += 1;
            }
            blocks.push(json!({
                "type": "meter",
                "label": label,
                "percent": percent,
                "value": value,
                "detail": detail,
            }));
            structured = true;
            continue;
        }
        if is_drawing(line) {
            pre.push(row.plain.clone());
            continue;
        }
        flush_pre(&mut pre, &mut blocks);
        if row.all_bold() {
            blocks.push(json!({ "type": "heading", "text": line }));
            structured = true;
            continue;
        }
        if let Some((key, value)) = key_value(line) {
            let entry = json!({ "key": key, "value": value });
            match blocks.last_mut() {
                Some(block) if block["type"] == "table" => {
                    if let Some(rows) = block["rows"].as_array_mut() {
                        rows.push(entry);
                    }
                }
                _ => blocks.push(json!({ "type": "table", "rows": [entry] })),
            }
            structured = true;
            continue;
        }
        let muted = row.all_muted();
        // A terminal-wrapped sentence continues on the next row.
        if let Some(block) = blocks.last_mut().filter(|block| {
            block["type"] == "text"
                && block["muted"] == muted
                && block["text"].as_str().is_some_and(|text| {
                    !text.ends_with(['.', ':', '!', '?', '…', ')'])
                        && line.starts_with(|ch: char| ch.is_lowercase())
                })
        }) {
            let joined = format!("{} {line}", block["text"].as_str().unwrap_or_default());
            block["text"] = json!(joined);
            continue;
        }
        blocks.push(json!({ "type": "text", "text": line, "muted": muted }));
    }
    flush_pre(&mut pre, &mut blocks);
    if !structured {
        // Nothing recognised: the whole body stays as text in the code font, margin trimmed.
        let lines: Vec<String> = body.iter().map(|row| row.plain.clone()).collect();
        let mut fallback = Vec::new();
        let mut pre = lines;
        while pre.first().is_some_and(|line| line.trim().is_empty()) {
            pre.remove(0);
        }
        flush_pre(&mut pre, &mut fallback);
        return (!fallback.is_empty()).then_some(fallback);
    }
    (!blocks.is_empty()).then_some(blocks)
}

// ---------------------------------------------------------------------------
// Reading a clipped answer whole
// ---------------------------------------------------------------------------

/// Answers read whole by scrolling, by session: the panel identity they belong to and the Markdown.
fn whole_answers() -> &'static Mutex<HashMap<String, (u64, String)>> {
    static STORE: OnceLock<Mutex<HashMap<String, (u64, String)>>> = OnceLock::new();
    STORE.get_or_init(Mutex::default)
}

/// Appends `window` to `acc` over their longest overlap (the panel scrolls a few rows per key).
fn merge_after(acc: &mut Vec<Row>, window: &[Row]) {
    let max = acc.len().min(window.len());
    let overlap = (1..=max)
        .rev()
        .find(|&k| {
            acc[acc.len() - k..]
                .iter()
                .zip(&window[..k])
                .all(|(a, b)| a.plain == b.plain)
        })
        .unwrap_or(0);
    acc.extend_from_slice(&window[overlap..]);
}

fn merge_before(acc: &mut Vec<Row>, window: &[Row]) {
    let max = acc.len().min(window.len());
    let overlap = (1..=max)
        .rev()
        .find(|&k| {
            window[window.len() - k..]
                .iter()
                .zip(&acc[..k])
                .all(|(a, b)| a.plain == b.plain)
        })
        .unwrap_or(0);
    let mut joined = window[..window.len() - overlap].to_vec();
    joined.append(acc);
    *acc = joined;
}

const ARROW_UP: &str = "\x1b[1;1A";
const ARROW_DOWN: &str = "\x1b[1;1B";
const SCROLL_SETTLE: Duration = Duration::from_millis(140);
const SCROLL_LIMIT: usize = 120;

/// One scroll press and the panel it leaves, or `None` when the panel changed into something else.
async fn press(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    key: &str,
    identity: u64,
) -> Option<SideQuestionScreen> {
    write_session_chat_payload(project_id, session_id, zmx_name, "claude-side-answer", key)
        .await
        .ok()?;
    tokio::time::sleep(SCROLL_SETTLE).await;
    let screen = side_question_screen(&capture_session_terminal_text_vt(zmx_name).await?)?;
    (screen.identity() == identity && screen.complete()).then_some(screen)
}

/// Scrolls one way until the window stops moving, merging every window into `acc`. Returns the
/// number of presses that moved it.
async fn scroll_to_end(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    key: &str,
    identity: u64,
    acc: &mut Vec<Row>,
    mut window: Vec<String>,
    cancelled: &(impl Fn() -> bool + Sync),
) -> Option<usize> {
    let mut moved = 0;
    for _ in 0..SCROLL_LIMIT {
        if cancelled() {
            return None;
        }
        let mut screen = press(project_id, session_id, zmx_name, key, identity).await?;
        if screen.window() == window {
            // One more look before concluding the end: a slow repaint reads as "did not move".
            tokio::time::sleep(SCROLL_SETTLE).await;
            screen = side_question_screen(&capture_session_terminal_text_vt(zmx_name).await?)?;
            if screen.identity() != identity || screen.window() == window {
                return Some(moved);
            }
        }
        if key == ARROW_UP {
            merge_before(acc, &screen.body);
        } else {
            merge_after(acc, &screen.body);
        }
        window = screen.window();
        moved += 1;
    }
    Some(moved)
}

/*
CDXC:SessionChat 2026-09-27 WHY:
A short terminal shows only a window of the side answer and gives no sign that more is hidden, so
the card would show half an answer (the reason the user asked for this). The panel scrolls a few
rows per arrow key, so gxserver scrolls to both ends inside the send queue, stitches the windows
over their overlap, and scrolls back to where the reader left it. It runs once per answer.
*/
pub(crate) async fn read_whole_side_answer(
    project_id: &str,
    session_id: &str,
    zmx_name: &str,
    cancelled: &(impl Fn() -> bool + Sync),
) {
    let Some(screen) = capture_session_terminal_text_vt(zmx_name)
        .await
        .and_then(|styled| side_question_screen(&styled))
    else {
        return;
    };
    if !screen.complete() {
        return;
    }
    let identity = screen.identity();
    let mut acc = screen.body.clone();
    let Some(up) = scroll_to_end(
        project_id,
        session_id,
        zmx_name,
        ARROW_UP,
        identity,
        &mut acc,
        screen.window(),
        cancelled,
    )
    .await
    else {
        return;
    };
    let top_window = acc
        .iter()
        .take(screen.body.len())
        .map(|row| row.plain.clone())
        .collect();
    let Some(down) = scroll_to_end(
        project_id, session_id, zmx_name, ARROW_DOWN, identity, &mut acc, top_window, cancelled,
    )
    .await
    else {
        return;
    };
    // Back to where the reader was: `up` presses above the starting window.
    for _ in 0..down.saturating_sub(up) {
        if write_session_chat_payload(
            project_id,
            session_id,
            zmx_name,
            "claude-side-answer",
            ARROW_UP,
        )
        .await
        .is_err()
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(40)).await;
    }
    let markdown = answer_markdown(&acc);
    if let Ok(mut store) = whole_answers().lock() {
        store.insert(
            crate::server::session_observer_key(project_id, session_id),
            (identity, markdown),
        );
    }
    // The follower probes every tick while a side question is open, which publishes it.
}

/// Queues one whole-answer read per panel identity, like the Codex pager close.
fn schedule_whole_answer(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    identity: u64,
) {
    if tokio::runtime::Handle::try_current().is_err() {
        return;
    }
    static PENDING: OnceLock<Mutex<HashMap<(String, u64), Instant>>> = OnceLock::new();
    let store = PENDING.get_or_init(Mutex::default);
    let key = (
        crate::server::session_observer_key(project_id, session_id),
        identity,
    );
    let Ok(mut pending) = store.lock() else {
        return;
    };
    // A failed read (the panel closed mid-scroll) may retry once the probe sees it again.
    pending.retain(|_, at| at.elapsed() < Duration::from_secs(20));
    if pending.contains_key(&key) {
        return;
    }
    let Ok(Some(session)) = repository.get_session(project_id, session_id) else {
        return;
    };
    let Ok(zmx_name) = crate::zmx::provider_zmx_session_name(&session) else {
        return;
    };
    pending.insert(key, Instant::now());
    drop(pending);
    let project_id = project_id.to_string();
    let session_id = session_id.to_string();
    tokio::spawn(async move {
        let _ = execute_session_chat_send(
            &project_id,
            &session_id,
            &zmx_name,
            "claude-side-answer",
            vec![SessionChatSendStep::ReadClaudeSideAnswer],
        )
        .await;
    });
}

/// Side answers already written to the slash-command archive, so each is written once.
fn archived() -> &'static Mutex<HashSet<(String, u64, usize)>> {
    static STORE: OnceLock<Mutex<HashSet<(String, u64, usize)>>> = OnceLock::new();
    STORE.get_or_init(Mutex::default)
}

// ---------------------------------------------------------------------------
// The notice
// ---------------------------------------------------------------------------

/// Adds what the styling says to a Claude dialog notice: the side question with its answer as
/// Markdown, or the structured blocks of a Settings-style panel.
pub(crate) fn enrich_claude_panel_notice(
    repository: &DomainRepository<'_>,
    project_id: &str,
    session_id: &str,
    notice: &mut SessionChatTerminalNotice,
    styled: &str,
) {
    let Some(dialog) = notice.dialog.as_mut() else {
        return;
    };
    // The resume picker is drawn from its own rows (session_chat_resume_picker.rs).
    if dialog.picker.is_some() {
        return;
    }
    // A tabbed panel with option rows (`/plugin`'s install scope) keeps its tabs and details as
    // blocks; the rows themselves are the card's choices, so their lines stay out.
    if !dialog.rows.is_empty() {
        let Some(rows) = panel_rows(styled) else {
            return;
        };
        if rows
            .iter()
            .find(|row| !row.is_blank())
            .and_then(tabs)
            .is_none()
        {
            return;
        }
        let is_option_line = |row: &Row| {
            let line = row.trimmed().trim_start_matches('❯').trim();
            !line.is_empty()
                && dialog.rows.iter().any(|option| {
                    let label = option.label.trim();
                    line.starts_with(label)
                        || line
                            .split_once(". ")
                            .is_some_and(|(_, rest)| rest.starts_with(label))
                        || option
                            .description
                            .as_deref()
                            .is_some_and(|description| description.contains(line))
                })
        };
        let rest: Vec<Row> = rows
            .into_iter()
            .filter(|row| !is_option_line(row))
            .collect();
        dialog.blocks = panel_blocks(&rest);
        return;
    }
    if let Some(screen) = side_question_screen(styled) {
        let identity = screen.identity();
        let complete = screen.complete();
        let whole = whole_answers().lock().ok().and_then(|store| {
            store
                .get(&crate::server::session_observer_key(project_id, session_id))
                .filter(|(id, _)| *id == identity)
                .map(|(_, markdown)| markdown.clone())
        });
        let answer = if screen.answering() {
            String::new()
        } else {
            whole
                .clone()
                .unwrap_or_else(|| answer_markdown(&screen.body))
        };
        let shown = screen.question().to_string();
        let full =
            crate::session_chat_app_command::full_side_question(project_id, session_id, &shown);
        let truncated = full.is_none() && shown.ends_with('…');
        let question = full.unwrap_or(shown);
        if complete && whole.is_none() {
            schedule_whole_answer(repository, project_id, session_id, identity);
        }
        if complete && whole.is_some() {
            let key = (
                crate::server::session_observer_key(project_id, session_id),
                identity,
                answer.len(),
            );
            let fresh = archived().lock().is_ok_and(|mut done| done.insert(key));
            if fresh {
                let session = repository
                    .get_session(project_id, session_id)
                    .ok()
                    .flatten();
                crate::session_chat_app_command::attach_side_answer(
                    project_id,
                    session_id,
                    session.as_ref(),
                    &question,
                    &answer,
                    screen.is_latest(),
                );
            }
        }
        // Claude's first panel line is the oldest question in its history list, so every side
        // question looked like the same notice (title-keyed dismissal cooldown included).
        dialog.title = format!("/btw {question}");
        notice.title = dialog.title.clone();
        dialog.side_question = Some(json!({
            "question": question,
            "questionTruncated": truncated,
            "answer": answer,
            "answering": screen.answering(),
            "complete": complete,
            "whole": whole.is_some(),
            "canFork": screen.footer.to_ascii_lowercase().contains("f to fork"),
        }));
        return;
    }
    if let Some(rows) = panel_rows(styled) {
        dialog.blocks = panel_blocks(&rows);
    }
}

/// The side-question identity folded into the notice identity, so a long-poll wakes when the
/// whole answer replaces the visible window.
pub(crate) fn side_answer_fingerprint(side_question: &Value) -> String {
    let mut hash = std::collections::hash_map::DefaultHasher::new();
    side_question.to_string().hash(&mut hash);
    format!("{:x}", hash.finish())
}
