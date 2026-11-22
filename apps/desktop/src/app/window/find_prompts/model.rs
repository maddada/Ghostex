//! Search by Prompt's data: the gxserver rows as the window keeps them, the labels the rows and the
//! footer print, the flattened prompt line with its match ranges, and the one key map.
//!
//! Every function here is a port of the React Find page (find-prompts-format.ts, find-prompt-highlight.ts,
//! find-prompts-hotkeys.ts and `buildViewRows` in find-prompts-view.tsx; deleted 2026-10-01, in git
//! history). The phone's native Find screens print the same labels and highlights, so keep them reading the same.
//! SEE-ALSO: apps/mobile/app/src/find/findFormat.ts and promptSearch.ts (the phone's labels and wire types), server/src/agent_prompt_search.rs.
use gpui::{Keystroke, Rgba, SharedString};
use serde::Deserialize;
use std::ops::Range;

/// `FIND_PROMPT_AGENTS`: the fork and filter order.
pub(crate) const FIND_PROMPT_AGENTS: [&str; 13] = [
    "claude",
    "codex",
    "pi",
    "opencode",
    "cursor",
    "grok",
    "empryo",
    "antigravity",
    "hermes",
    "copilot",
    "droid",
    "omp",
    "zcode",
];
/// The fork targets are the leading agents of `FIND_PROMPT_AGENTS`: Empryo's terminal app takes
/// no starting prompt, and the agents after it (added 2026-10-17) filter and resume but are not
/// numbered fork targets, so it is never forked into.
pub(crate) const FIND_PROMPT_FORK_AGENT_COUNT: usize = 6;
/// Rows fetched per page (`FIND_PROMPTS_PAGE_SIZE`).
pub(crate) const FIND_PROMPTS_PAGE_SIZE: usize = 120;
/// The window is 1080px wide, so a row shows about 130 characters of its prompt. Longer lines
/// are cut before they are shaped; the ellipsis lands at the same place either way.
const ROW_LINE_MAX_CHARS: usize = 480;
const SECONDS_PER_DAY: i64 = 86_400;
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct WireUsage {
    pub(crate) cache_read: f64,
    pub(crate) cache_write: f64,
    pub(crate) context_window: f64,
    pub(crate) cost: f64,
    pub(crate) input: f64,
    pub(crate) output: f64,
    pub(crate) rate_percent: f64,
}

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct WireMeta {
    pub(crate) model: String,
    pub(crate) plan: String,
    pub(crate) provider: String,
    pub(crate) thinking: String,
    pub(crate) usage: WireUsage,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct WireRow {
    agent: String,
    agent_color: String,
    day_key: Option<f64>,
    favorite: bool,
    highlights: Vec<u32>,
    interrupted: bool,
    key: String,
    kind: String,
    meta: WireMeta,
    project: String,
    project_name: String,
    text: String,
    title: String,
    truncated: bool,
    ts: f64,
}

#[derive(Clone, Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub(crate) struct ProjectFacet {
    pub(crate) name: String,
    pub(crate) path: String,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct WireAgentFacet {
    agent: String,
    color: String,
}

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct WireSearchResult {
    agents: Option<Vec<WireAgentFacet>>,
    matched: usize,
    offset: usize,
    opencode_error: Option<String>,
    empryo_error: Option<String>,
    projects: Option<Vec<ProjectFacet>>,
    rows: Vec<WireRow>,
    total: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum FindSearchScope {
    #[default]
    All,
    Prompts,
    Answers,
}

impl FindSearchScope {
    pub(crate) fn cycle(self) -> Self {
        match self {
            Self::All => Self::Prompts,
            Self::Prompts => Self::Answers,
            Self::Answers => Self::All,
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Prompts => "Prompts",
            Self::Answers => "Answers",
        }
    }

    pub(crate) fn tooltip(self) -> &'static str {
        match self {
            Self::All => "Searching your prompts and agent closing answers (click to filter)",
            Self::Prompts => "Searching your prompts only (click for agent answers)",
            Self::Answers => "Searching agent closing answers only (click for all)",
        }
    }

    pub(crate) fn kinds(self) -> &'static [&'static str] {
        match self {
            Self::All => &["user", "agent_answer"],
            Self::Prompts => &["user"],
            Self::Answers => &["agent_answer"],
        }
    }
}

/// One result, prepared once per page so a frame only lays out what it shows.
pub(crate) struct FindRow {
    pub(crate) key: String,
    pub(crate) agent: SharedString,
    pub(crate) agent_color: Rgba,
    pub(crate) is_agent_answer: bool,
    pub(crate) interrupted: bool,
    /// `None` is the server's unknown day (a prompt without a session time).
    pub(crate) day_key: Option<i64>,
    pub(crate) favorite: bool,
    /// The row's text as sent (possibly capped; `truncated` says so).
    pub(crate) text: SharedString,
    /// The first line as a row shows it: whitespace runs flattened, cut to what fits.
    pub(crate) line: SharedString,
    /// Byte ranges of `line` that matched the query.
    pub(crate) line_highlights: Vec<Range<usize>>,
    pub(crate) project: SharedString,
    pub(crate) project_name: SharedString,
    pub(crate) title: SharedString,
    pub(crate) ts: i64,
    pub(crate) truncated: bool,
    /// "last active …" plus the usage and model line, for the footer.
    pub(crate) footer: SharedString,
}

pub(crate) struct SearchPage {
    pub(crate) rows: Vec<FindRow>,
    pub(crate) offset: usize,
    pub(crate) matched: usize,
    pub(crate) total: usize,
    pub(crate) projects: Option<Vec<ProjectFacet>>,
    /// Facet colors in `FIND_PROMPT_AGENTS` order; `None` where the server sent none.
    pub(crate) agent_colors: Option<[Option<Rgba>; FIND_PROMPT_AGENTS.len()]>,
    pub(crate) opencode_error: Option<String>,
    pub(crate) empryo_error: Option<String>,
}

/// Parses `/api/searchAgentPrompts` and prepares every row (flattened line, match ranges, footer).
/// Runs off the UI thread.
pub(crate) fn parse_search_page(value: serde_json::Value) -> Result<SearchPage, String> {
    let wire: WireSearchResult = serde_json::from_value(value)
        .map_err(|error| format!("gxserver returned an unexpected search result: {error}"))?;
    let fallback = Rgba {
        r: 0.5,
        g: 0.5,
        b: 0.5,
        a: 1.0,
    };
    let agent_colors = wire.agents.map(|facets| {
        let mut colors = [None; FIND_PROMPT_AGENTS.len()];
        for facet in facets {
            if let Some(index) = FIND_PROMPT_AGENTS
                .iter()
                .position(|agent| *agent == facet.agent)
            {
                colors[index] = Some(crate::app::window::quick_access::palette::parse_css_color(
                    &facet.color,
                    fallback,
                ));
            }
        }
        colors
    });
    let rows = wire
        .rows
        .into_iter()
        .map(|row| {
            let (line, line_highlights) = flatten_line_with_ranges(&row.text, &row.highlights);
            let ts = if row.ts.is_finite() { row.ts as i64 } else { 0 };
            let meta = format_prompt_meta_line(&row.meta);
            let full = format_last_active_full(ts);
            FindRow {
                agent_color: crate::app::window::quick_access::palette::parse_css_color(
                    &row.agent_color,
                    fallback,
                ),
                agent: row.agent.into(),
                is_agent_answer: row.kind == "agent_answer",
                interrupted: row.interrupted,
                // The server sends i64::MIN for an unknown day; anything past f64's exact range is that.
                day_key: row
                    .day_key
                    .filter(|day| day.is_finite() && day.abs() < 9.0e15)
                    .map(|day| day as i64),
                favorite: row.favorite,
                key: row.key,
                line: line.into(),
                line_highlights,
                project: row.project.into(),
                project_name: row.project_name.into(),
                title: row.title.into(),
                text: row.text.into(),
                ts,
                truncated: row.truncated,
                footer: if meta.is_empty() {
                    full.into()
                } else {
                    format!("{full} {meta}").into()
                },
            }
        })
        .collect();
    Ok(SearchPage {
        rows,
        offset: wire.offset,
        matched: wire.matched,
        total: wire.total,
        projects: wire.projects,
        agent_colors,
        opencode_error: wire.opencode_error.filter(|error| !error.is_empty()),
        empryo_error: wire.empryo_error.filter(|error| !error.is_empty()),
    })
}

/// `flattenPromptLineWithOffsets` + `splitHighlightedSegments`: `\r\n\t` runs become one space, and
/// a character is highlighted when its first byte was one of the matcher's byte offsets. Returns
/// the merged highlighted byte ranges of the flattened (and capped) line.
pub(crate) fn flatten_line_with_ranges(
    text: &str,
    byte_offsets: &[u32],
) -> (String, Vec<Range<usize>>) {
    let mut marks: Vec<u32> = byte_offsets.to_vec();
    marks.sort_unstable();
    marks.dedup();
    let marked = |offset: usize| marks.binary_search(&(offset as u32)).is_ok();
    let mut out = String::with_capacity(text.len().min(ROW_LINE_MAX_CHARS * 2));
    let mut ranges: Vec<Range<usize>> = Vec::new();
    let mut pending_whitespace = false;
    let mut chars = 0usize;
    for (offset, character) in text.char_indices() {
        if chars >= ROW_LINE_MAX_CHARS {
            break;
        }
        if matches!(character, '\n' | '\r' | '\t') {
            if !pending_whitespace {
                out.push(' ');
                chars += 1;
                pending_whitespace = true;
            }
            continue;
        }
        pending_whitespace = false;
        let start = out.len();
        out.push(character);
        chars += 1;
        if !marks.is_empty() && marked(offset) {
            match ranges.last_mut() {
                Some(last) if last.end == start => last.end = out.len(),
                _ => ranges.push(start..out.len()),
            }
        }
    }
    (out, ranges)
}

fn utc_parts(ts: i64) -> Option<(usize, u32, u32, u32, i32)> {
    use chrono::{Datelike as _, TimeZone as _, Timelike as _};
    let date = chrono::Utc.timestamp_opt(ts, 0).single()?;
    Some((
        date.month0() as usize,
        date.day(),
        date.hour(),
        date.minute(),
        date.year(),
    ))
}

/// `formatLastActiveCompact`: the time under each result.
pub(crate) fn format_last_active_compact(ts: i64, now: i64) -> String {
    if ts <= 0 {
        return "unknown".to_string();
    }
    let delta = (now - ts).max(0);
    if delta < 60 {
        return "now".to_string();
    }
    if delta < 3_600 {
        return format!("{}m ago", delta / 60);
    }
    if delta < SECONDS_PER_DAY {
        return format!("{}h ago", delta / 3_600);
    }
    if delta < 7 * SECONDS_PER_DAY {
        return format!("{}d ago", delta / SECONDS_PER_DAY);
    }
    match utc_parts(ts) {
        Some((month, day, ..)) => format!("{} {day}", MONTHS[month]),
        None => "unknown".to_string(),
    }
}

/// `formatDayHeader`: a day group's heading.
pub(crate) fn format_day_header(day_key: Option<i64>, now: i64) -> String {
    let Some(day_key) = day_key else {
        return "Unknown day".to_string();
    };
    let today = now.div_euclid(SECONDS_PER_DAY);
    if day_key == today {
        return "Today".to_string();
    }
    if day_key == today - 1 {
        return "Yesterday".to_string();
    }
    if day_key > today - 7 && day_key < today {
        return format!("{} days ago", today - day_key);
    }
    let (Some(date), Some(now_date)) = (
        utc_parts(day_key.saturating_mul(SECONDS_PER_DAY)),
        utc_parts(today * SECONDS_PER_DAY),
    ) else {
        return "Unknown day".to_string();
    };
    if date.4 == now_date.4 {
        format!("{} {}", MONTHS[date.0], date.1)
    } else {
        format!("{} {}, {}", MONTHS[date.0], date.1, date.4)
    }
}

/// `formatLastActiveFull`: the footer's "last active …".
pub(crate) fn format_last_active_full(ts: i64) -> String {
    match (ts > 0).then(|| utc_parts(ts)).flatten() {
        Some((month, day, hour, minute, _)) => {
            format!(
                "last active {} {day} {hour:02}:{minute:02} UTC",
                MONTHS[month]
            )
        }
        None => "last active unknown".to_string(),
    }
}

/// JavaScript's `String(number)` for the whole counts the server sends.
fn js_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

/// `formatPromptMetaLine`: usage, plan, rate and model after "last active …".
pub(crate) fn format_prompt_meta_line(meta: &WireMeta) -> String {
    let usage = &meta.usage;
    let mut parts: Vec<String> = Vec::new();
    if usage.input > 0.0 {
        parts.push(format!("↑{}", js_number(usage.input)));
    }
    if usage.output > 0.0 {
        parts.push(format!("↓{}", js_number(usage.output)));
    }
    if usage.cache_read > 0.0 {
        parts.push(format!("R{}", js_number(usage.cache_read)));
    }
    if usage.cache_write > 0.0 {
        parts.push(format!("W{}", js_number(usage.cache_write)));
    }
    if usage.cost > 0.0 {
        parts.push(format!("${:.3}", usage.cost));
    }
    if !meta.plan.is_empty() {
        parts.push(format!("({})", meta.plan));
    }
    if usage.rate_percent > 0.0 {
        parts.push(if usage.context_window > 0.0 {
            format!(
                "{:.1}%/{}",
                usage.rate_percent,
                js_number(usage.context_window)
            )
        } else {
            format!("{:.1}%", usage.rate_percent)
        });
    } else if usage.context_window > 0.0 {
        parts.push(format!("/{}", js_number(usage.context_window)));
    }
    let model = [
        (!meta.provider.is_empty()).then(|| format!("({})", meta.provider)),
        (!meta.model.is_empty()).then(|| meta.model.clone()),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" ");
    if !model.is_empty() {
        parts.push(model);
    }
    if !meta.thinking.is_empty() {
        parts.push(format!("• {}", meta.thinking));
    }
    parts.join(" ")
}

/// A list entry: a day heading (grouping on) or the result at `rows[index]`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ViewRow {
    Day(Option<i64>),
    Row(usize),
}

/// `buildViewRows`.
pub(crate) fn build_view_rows(rows: &[FindRow], group_by_day: bool) -> Vec<ViewRow> {
    let mut out = Vec::with_capacity(rows.len() + if group_by_day { 16 } else { 0 });
    let mut last_day: Option<Option<i64>> = None;
    for (index, row) in rows.iter().enumerate() {
        if group_by_day && last_day != Some(row.day_key) {
            out.push(ViewRow::Day(row.day_key));
            last_day = Some(row.day_key);
        }
        out.push(ViewRow::Row(index));
    }
    out
}

/// Word characters for the query line's word deletes (`[\p{L}\p{N}_]`).
fn is_word(character: char) -> bool {
    character.is_alphanumeric() || character == '_'
}

/// Ctrl+Backspace: `head.replace(/[^\p{L}\p{N}_]*[\p{L}\p{N}_]*$/u, '')`.
pub(crate) fn delete_word_backward(head: &str) -> &str {
    let mut end = head.len();
    for (offset, character) in head.char_indices().rev() {
        if !is_word(character) {
            break;
        }
        end = offset;
    }
    for (offset, character) in head[..end].char_indices().rev() {
        if is_word(character) {
            break;
        }
        end = offset;
    }
    &head[..end]
}

/// Ctrl+Delete: `tail.replace(/^[^\p{L}\p{N}_]*[\p{L}\p{N}_]*/u, '')`.
pub(crate) fn delete_word_forward(tail: &str) -> &str {
    let mut start = tail.len();
    let mut seen_word = false;
    for (offset, character) in tail.char_indices() {
        if is_word(character) {
            seen_word = true;
        } else if seen_word {
            start = offset;
            break;
        }
    }
    &tail[start..]
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum FindMode {
    List,
    Preview,
    ForkPicker,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum FindAction {
    CancelOverlay,
    Close,
    CopyPrompt,
    DeleteWordBackward,
    DeleteWordForward,
    ForkPicker,
    JumpDay(isize),
    KillToEnd,
    KillToStart,
    Move(isize),
    OpenAgentPicker,
    OpenProjectPicker,
    PickIndex(usize),
    ResumePrompt,
    ScrollPreview(isize),
    ToggleDayGrouping,
    ToggleFavorite,
    ToggleFullscreenPreview,
    TogglePreviewFocus,
    ToggleWrap,
    ViewPrompt,
}

/// `resolveFindPromptsAction` for the list, preview and fork modes (the filter menus own their
/// keys in the window). `key` is gpui's layout-independent key name and `key_char` the character
/// the keystroke types.
pub(crate) fn resolve_find_action(keystroke: &Keystroke, mode: FindMode) -> Option<FindAction> {
    let modifiers = keystroke.modifiers;
    let key = keystroke.key.as_str();
    let typed = keystroke.key_char.as_deref().unwrap_or(key);
    let plain = !modifiers.control && !modifiers.platform && !modifiers.alt;
    if mode == FindMode::ForkPicker {
        // Any key leaves fork mode; a digit also picks the target agent.
        return Some(match digit_index(typed) {
            Some(index) if plain => FindAction::PickIndex(index),
            _ => FindAction::CancelOverlay,
        });
    }
    let preview_focused = mode == FindMode::Preview;
    if key == "escape" {
        return Some(FindAction::Close);
    }
    if modifiers.control && key == "c" {
        return Some(FindAction::Close);
    }
    if key == "enter" && !modifiers.shift {
        return Some(FindAction::ResumePrompt);
    }
    if key == "tab" {
        return Some(FindAction::TogglePreviewFocus);
    }
    if modifiers.control && !modifiers.alt {
        let action = match key {
            "d" => Some(FindAction::ToggleDayGrouping),
            "g" => Some(FindAction::OpenAgentPicker),
            "j" => Some(FindAction::OpenProjectPicker),
            // `^f` favorites a result, and toggles the big preview while the preview owns focus.
            "f" if preview_focused => Some(FindAction::ToggleFullscreenPreview),
            "f" => Some(FindAction::ToggleFavorite),
            "e" => Some(FindAction::ViewPrompt),
            "y" => Some(FindAction::CopyPrompt),
            "o" => Some(FindAction::ForkPicker),
            "k" => Some(FindAction::KillToEnd),
            "u" => Some(FindAction::KillToStart),
            "n" => Some(FindAction::Move(1)),
            "p" => Some(FindAction::Move(-1)),
            "backspace" => Some(FindAction::DeleteWordBackward),
            "delete" => Some(FindAction::DeleteWordForward),
            "up" => Some(FindAction::JumpDay(-1)),
            "down" => Some(FindAction::JumpDay(1)),
            _ => None,
        };
        if action.is_some() {
            return action;
        }
    }
    if key == "down" && plain {
        return Some(FindAction::Move(1));
    }
    if key == "up" && plain {
        return Some(FindAction::Move(-1));
    }
    if key == "pagedown" {
        return Some(if preview_focused {
            FindAction::ScrollPreview(1)
        } else {
            FindAction::JumpDay(1)
        });
    }
    if key == "pageup" {
        return Some(if preview_focused {
            FindAction::ScrollPreview(-1)
        } else {
            FindAction::JumpDay(-1)
        });
    }
    // `W` and `F` are preview controls only; in the list they are query text.
    if preview_focused && plain {
        if typed.eq_ignore_ascii_case("w") {
            return Some(FindAction::ToggleWrap);
        }
        if typed.eq_ignore_ascii_case("f") {
            return Some(FindAction::ToggleFullscreenPreview);
        }
    }
    None
}

/// Digits 1-6 pick an agent in the fork overlay.
fn digit_index(typed: &str) -> Option<usize> {
    match typed {
        "1" | "2" | "3" | "4" | "5" | "6" => typed.parse::<usize>().ok().map(|digit| digit - 1),
        _ => None,
    }
}
