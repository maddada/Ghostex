//! The Tree layout's rules: which section a bead sits in, what each filter keeps, the counts,
//! the epic summaries, the order, and the words on each row.

use std::collections::{HashMap, HashSet};

use ghostex_gx_protocol::beads_view::{BeadsReadiness, BeadsViewRow};

use super::agents::{bead_agent_key, BeadAgent, BeadAgents};
use super::time::{parse_bd_time_ms, relative_age, short_date, BeadsClock};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum BeadsFilter {
    #[default]
    All,
    NeedsDecision,
    Ready,
    InProgress,
    Blocked,
    Deferred,
}

impl BeadsFilter {
    pub const ALL: [BeadsFilter; 6] = [
        BeadsFilter::All,
        BeadsFilter::NeedsDecision,
        BeadsFilter::Ready,
        BeadsFilter::InProgress,
        BeadsFilter::Blocked,
        BeadsFilter::Deferred,
    ];

    pub fn label(self) -> &'static str {
        match self {
            BeadsFilter::All => "All",
            BeadsFilter::NeedsDecision => "Needs a decision",
            BeadsFilter::Ready => "Ready",
            BeadsFilter::InProgress => "In progress",
            BeadsFilter::Blocked => "Blocked",
            BeadsFilter::Deferred => "Deferred",
        }
    }

    /// Ready and Blocked read bd's readiness answer.
    pub fn needs_readiness(self) -> bool {
        matches!(self, BeadsFilter::Ready | BeadsFilter::Blocked)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum BeadsSort {
    #[default]
    Priority,
    LastChanged,
}

impl BeadsSort {
    pub fn label(self) -> &'static str {
        match self {
            BeadsSort::Priority => "Priority",
            BeadsSort::LastChanged => "Last changed",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BeadsTreeOptions {
    pub filter: BeadsFilter,
    pub sort: BeadsSort,
    pub show_done: bool,
    pub query: String,
    /// Sections whose "N done" fold is open.
    pub open_done: HashSet<String>,
    /// Epic sections the user folded shut.
    pub collapsed: HashSet<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BeadGlyph {
    Open,
    InProgress,
    Blocked,
    /// Deferred to a date.
    Snoozed,
    OnIce,
    Review,
    Decision,
    Closed,
    Epic,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BeadMeta {
    /// "Bug", "Task", "Feature".
    Type(String),
    /// "Waiting on <title>" plus how many more.
    WaitingOn {
        title: String,
        more: usize,
    },
    Until(String),
    OnIce,
    Due(String),
    /// A custom or uncommon status by name ("In review", "Backlog").
    Status(String),
    InEpic(String),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BeadRowModel {
    pub id: String,
    pub title: String,
    pub glyph: BeadGlyph,
    pub meta: Vec<BeadMeta>,
    pub labels: Vec<String>,
    pub comment_count: u64,
    /// "P0".."P4".
    pub priority: Option<String>,
    pub changed: String,
    pub agents: Vec<BeadAgent>,
    pub needs_decision: bool,
    pub closed: bool,
    pub claimable: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EpicSummary {
    pub total: usize,
    pub done: usize,
    pub in_progress: usize,
    pub blocked: usize,
    pub ready: usize,
    pub needs_decision: usize,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum BeadsSectionKind {
    NeedsDecision,
    Epic(Box<BeadRowModel>),
    NotInEpic,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BeadsSection {
    pub key: String,
    pub kind: BeadsSectionKind,
    pub title: String,
    /// Open beads listed (the header's count).
    pub count: usize,
    pub summary: Option<EpicSummary>,
    pub rows: Vec<BeadRowModel>,
    /// Done beads, listed only when the fold is open or "Show done" is on.
    pub done_rows: Vec<BeadRowModel>,
    pub done_count: usize,
    pub done_open: bool,
    pub collapsed: bool,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BeadsFilterCounts {
    pub counts: HashMap<BeadsFilter, usize>,
}

impl BeadsFilterCounts {
    pub fn get(&self, filter: BeadsFilter) -> usize {
        self.counts.get(&filter).copied().unwrap_or(0)
    }
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BeadsTreeModel {
    pub sections: Vec<BeadsSection>,
    pub counts: BeadsFilterCounts,
    /// False when bd's readiness answer is missing or paused: Ready, Blocked and "Waiting on"
    /// are not shown.
    pub readiness_ok: bool,
}

fn is_closed(row: &BeadsViewRow) -> bool {
    row.status == "closed"
}

fn is_epic(row: &BeadsViewRow) -> bool {
    row.issue_type == "epic"
}

fn is_blocked(row: &BeadsViewRow) -> bool {
    row.status == "blocked" || row.blocked_by.as_ref().is_some_and(|by| !by.is_empty())
}

pub fn matches_filter(row: &BeadsViewRow, filter: BeadsFilter) -> bool {
    if is_closed(row) {
        return filter == BeadsFilter::All;
    }
    match filter {
        BeadsFilter::All => true,
        BeadsFilter::NeedsDecision => row.needs_decision,
        BeadsFilter::Ready => row.ready == Some(true),
        BeadsFilter::InProgress => row.status == "in_progress",
        BeadsFilter::Blocked => is_blocked(row),
        BeadsFilter::Deferred => row.status == "deferred",
    }
}

fn matches_query(row: &BeadsViewRow, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }
    row.title.to_lowercase().contains(query)
        || row.id.to_lowercase().contains(query)
        || row
            .labels
            .iter()
            .any(|label| label.to_lowercase().contains(query))
}

/// "In progress", "In review", "Backlog": a status name as people say it.
pub fn status_label(status: &str) -> String {
    match status {
        "in_progress" => "In progress".to_string(),
        "review" => "In review".to_string(),
        "test" => "In test".to_string(),
        "open" => "Open".to_string(),
        "closed" => "Done".to_string(),
        "deferred" => "Deferred".to_string(),
        "blocked" => "Blocked".to_string(),
        other => {
            let spaced = other.replace(['_', '-'], " ");
            let mut chars = spaced.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().chain(chars).collect(),
                None => String::new(),
            }
        }
    }
}

pub fn type_label(issue_type: &str) -> String {
    status_label(issue_type)
}

fn glyph(row: &BeadsViewRow) -> BeadGlyph {
    if is_closed(row) {
        return BeadGlyph::Closed;
    }
    if row.needs_decision {
        return BeadGlyph::Decision;
    }
    match row.status.as_str() {
        "in_progress" => BeadGlyph::InProgress,
        "deferred" if row.defer_until.is_some() => BeadGlyph::Snoozed,
        "deferred" => BeadGlyph::OnIce,
        "review" | "test" => BeadGlyph::Review,
        _ if is_blocked(row) => BeadGlyph::Blocked,
        _ if is_epic(row) => BeadGlyph::Epic,
        _ => BeadGlyph::Open,
    }
}

fn priority_tag(row: &BeadsViewRow) -> Option<String> {
    row.priority.map(|priority| format!("P{priority}"))
}

struct Context<'a> {
    by_id: HashMap<&'a str, &'a BeadsViewRow>,
    agents: &'a BeadAgents,
    clock: &'a BeadsClock,
    readiness_ok: bool,
}

impl Context<'_> {
    /// The nearest epic above a bead, following parents.
    fn epic_of(&self, row: &BeadsViewRow) -> Option<String> {
        let mut current = row.parent.as_deref();
        let mut seen = 0;
        while let Some(parent_id) = current {
            let parent = self.by_id.get(parent_id)?;
            if is_epic(parent) {
                return Some(parent.id.clone());
            }
            current = parent.parent.as_deref();
            seen += 1;
            if seen > 32 {
                return None;
            }
        }
        None
    }

    fn row_model(&self, row: &BeadsViewRow, epic_title: Option<&str>) -> BeadRowModel {
        let mut meta = vec![BeadMeta::Type(type_label(&row.issue_type))];
        if let Some(epic_title) = epic_title {
            meta.push(BeadMeta::InEpic(epic_title.to_string()));
        }
        if self.readiness_ok && !is_closed(row) {
            if let Some(blockers) = row.blocked_by.as_ref().filter(|by| !by.is_empty()) {
                let first = &blockers[0];
                let title = self
                    .by_id
                    .get(first.as_str())
                    .map(|blocker| blocker.title.clone())
                    .unwrap_or_else(|| first.clone());
                meta.push(BeadMeta::WaitingOn {
                    title,
                    more: blockers.len() - 1,
                });
            }
        }
        if row.status == "deferred" {
            match row.defer_until.as_deref().and_then(parse_bd_time_ms) {
                Some(until) => meta.push(BeadMeta::Until(short_date(until, self.clock, true))),
                None => meta.push(BeadMeta::OnIce),
            }
        }
        if !is_closed(row) {
            if let Some(due) = row.due_at.as_deref().and_then(parse_bd_time_ms) {
                meta.push(BeadMeta::Due(short_date(due, self.clock, false)));
            }
        }
        if !matches!(
            row.status.as_str(),
            "open" | "in_progress" | "closed" | "deferred" | "blocked"
        ) {
            meta.push(BeadMeta::Status(status_label(&row.status)));
        }
        let changed = parse_bd_time_ms(&row.updated_at)
            .map(|time| relative_age(time, self.clock))
            .unwrap_or_default();
        BeadRowModel {
            id: row.id.clone(),
            title: row.title.clone(),
            glyph: glyph(row),
            meta,
            labels: row
                .labels
                .iter()
                .filter(|label| label.as_str() != "human")
                .cloned()
                .collect(),
            comment_count: row.comment_count,
            priority: priority_tag(row),
            changed,
            agents: self
                .agents
                .get(&bead_agent_key(&row.id))
                .cloned()
                .unwrap_or_default(),
            needs_decision: row.needs_decision,
            closed: is_closed(row),
            claimable: !is_closed(row) && row.assignee.is_none() && !is_epic(row),
        }
    }
}

fn sort_rows(rows: &mut [&BeadsViewRow], sort: BeadsSort) {
    let changed = |row: &BeadsViewRow| parse_bd_time_ms(&row.updated_at).unwrap_or(0);
    rows.sort_by(|a, b| match sort {
        BeadsSort::Priority => a
            .priority
            .unwrap_or(9)
            .cmp(&b.priority.unwrap_or(9))
            .then_with(|| changed(b).cmp(&changed(a)))
            .then_with(|| a.id.cmp(&b.id)),
        BeadsSort::LastChanged => changed(b).cmp(&changed(a)).then_with(|| a.id.cmp(&b.id)),
    });
}

fn summary_text(summary: &EpicSummary) -> String {
    let mut parts = vec![format!("{} of {} done", summary.done, summary.total)];
    if summary.in_progress > 0 {
        parts.push(format!("{} in progress", summary.in_progress));
    }
    if summary.blocked > 0 {
        parts.push(format!("{} blocked", summary.blocked));
    }
    if summary.ready > 0 {
        parts.push(format!("{} ready", summary.ready));
    }
    if summary.needs_decision > 0 {
        parts.push(format!(
            "{} {} a decision",
            summary.needs_decision,
            if summary.needs_decision == 1 {
                "needs"
            } else {
                "need"
            }
        ));
    }
    parts.join(" · ")
}

fn epic_summary(members: &[&BeadsViewRow], readiness_ok: bool) -> EpicSummary {
    let mut summary = EpicSummary {
        total: members.len(),
        done: members.iter().filter(|row| is_closed(row)).count(),
        in_progress: members
            .iter()
            .filter(|row| row.status == "in_progress")
            .count(),
        blocked: if readiness_ok {
            members
                .iter()
                .filter(|row| !is_closed(row) && is_blocked(row))
                .count()
        } else {
            0
        },
        ready: if readiness_ok {
            members
                .iter()
                .filter(|row| !is_closed(row) && row.ready == Some(true))
                .count()
        } else {
            0
        },
        needs_decision: members.iter().filter(|row| row.needs_decision).count(),
        text: String::new(),
    };
    summary.text = summary_text(&summary);
    summary
}

/// One bead as a details page shows it: its row, the epic above it with that epic's progress,
/// and, for an epic, its own progress. `None` when the bead is not in `rows`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BeadDetailsModel {
    pub row: BeadRowModel,
    pub epic: Option<(String, String, EpicSummary)>,
    pub summary: Option<EpicSummary>,
}

pub fn bead_details(
    rows: &[BeadsViewRow],
    readiness: &BeadsReadiness,
    id: &str,
    clock: &BeadsClock,
) -> Option<BeadDetailsModel> {
    let readiness_ok = *readiness == BeadsReadiness::Ok;
    let agents = BeadAgents::new();
    let context = Context {
        by_id: rows.iter().map(|row| (row.id.as_str(), row)).collect(),
        agents: &agents,
        clock,
        readiness_ok,
    };
    let row = *context.by_id.get(id)?;
    let members_of = |epic: &str| -> Vec<&BeadsViewRow> {
        rows.iter()
            .filter(|candidate| !is_epic(candidate))
            .filter(|candidate| context.epic_of(candidate).as_deref() == Some(epic))
            .collect()
    };
    let epic = context.epic_of(row).and_then(|epic_id| {
        let epic = context.by_id.get(epic_id.as_str())?;
        Some((
            epic.id.clone(),
            epic.title.clone(),
            epic_summary(&members_of(&epic.id), readiness_ok),
        ))
    });
    let summary = is_epic(row).then(|| epic_summary(&members_of(&row.id), readiness_ok));
    Some(BeadDetailsModel {
        row: context.row_model(row, None),
        epic,
        summary,
    })
}

/// CDXC:ProjectBoard 2026-11-11 DECISION:
/// User (the Beads view design): every bead sits under its epic, "Not in an epic" last, and beads waiting on a person ("Needs a decision", bd's `human` flag) are pinned on top. A bead that needs a decision is listed only in that pinned group, never twice; its epic still counts it in "2 of 5 done" and says "1 needs a decision". Filters keep the tree's shape, and done beads fold into one "N done" row per epic.
pub fn build_tree(
    rows: &[BeadsViewRow],
    readiness: &BeadsReadiness,
    agents: &BeadAgents,
    options: &BeadsTreeOptions,
    clock: &BeadsClock,
) -> BeadsTreeModel {
    let readiness_ok = *readiness == BeadsReadiness::Ok;
    let context = Context {
        by_id: rows.iter().map(|row| (row.id.as_str(), row)).collect(),
        agents,
        clock,
        readiness_ok,
    };
    let query = options.query.trim().to_lowercase();
    let mut counts = BeadsFilterCounts::default();
    for filter in BeadsFilter::ALL {
        let count = rows
            .iter()
            .filter(|row| !is_epic(row) && !is_closed(row) && matches_filter(row, filter))
            .filter(|row| matches_query(row, &query))
            .count();
        counts.counts.insert(filter, count);
    }
    let filter = if !readiness_ok && options.filter.needs_readiness() {
        BeadsFilter::All
    } else {
        options.filter
    };
    let keep = |row: &BeadsViewRow| matches_query(row, &query) && matches_filter(row, filter);

    let mut epics: Vec<&BeadsViewRow> = rows.iter().filter(|row| is_epic(row)).collect();
    sort_rows(&mut epics, options.sort);
    let mut children: HashMap<String, Vec<&BeadsViewRow>> = HashMap::new();
    let mut loose: Vec<&BeadsViewRow> = Vec::new();
    let mut decisions: Vec<&BeadsViewRow> = Vec::new();
    for row in rows.iter().filter(|row| !is_epic(row)) {
        let epic = context.epic_of(row);
        if row.needs_decision {
            decisions.push(row);
        }
        match epic {
            Some(epic) => children.entry(epic).or_default().push(row),
            None => loose.push(row),
        }
    }

    let mut sections = Vec::new();
    let mut decision_rows: Vec<&BeadsViewRow> =
        decisions.iter().copied().filter(|row| keep(row)).collect();
    sort_rows(&mut decision_rows, options.sort);
    if !decision_rows.is_empty() {
        sections.push(BeadsSection {
            key: "decisions".to_string(),
            kind: BeadsSectionKind::NeedsDecision,
            title: "Needs a decision".to_string(),
            count: decision_rows.len(),
            summary: None,
            rows: decision_rows
                .iter()
                .map(|row| {
                    let epic_title = context
                        .epic_of(row)
                        .and_then(|epic| context.by_id.get(epic.as_str()).map(|e| e.title.clone()));
                    context.row_model(row, epic_title.as_deref())
                })
                .collect(),
            done_rows: Vec::new(),
            done_count: 0,
            done_open: false,
            collapsed: false,
        });
    }

    let build_section = |key: String,
                         kind: BeadsSectionKind,
                         title: String,
                         members: Vec<&BeadsViewRow>,
                         epic_matches: bool| {
        let mut open: Vec<&BeadsViewRow> = members
            .iter()
            .copied()
            .filter(|row| !is_closed(row) && !row.needs_decision)
            .filter(|row| epic_matches || keep(row))
            .filter(|row| matches_filter(row, filter))
            .collect();
        sort_rows(&mut open, options.sort);
        let mut done: Vec<&BeadsViewRow> = members
            .iter()
            .copied()
            .filter(|row| is_closed(row))
            .filter(|row| epic_matches || matches_query(row, &query))
            .collect();
        sort_rows(&mut done, BeadsSort::LastChanged);
        let summary =
            matches!(kind, BeadsSectionKind::Epic(_)).then(|| epic_summary(&members, readiness_ok));
        let show_done_rows = filter == BeadsFilter::All;
        let unfiltered = show_done_rows && (query.is_empty() || epic_matches);
        let is_epic_section = matches!(kind, BeadsSectionKind::Epic(_));
        let visible = !open.is_empty()
            || (unfiltered && !done.is_empty())
            || (unfiltered && is_epic_section && members.is_empty());
        if !visible {
            return None;
        }
        let done_open = options.show_done || options.open_done.contains(&key);
        Some(BeadsSection {
            collapsed: options.collapsed.contains(&key),
            count: open.len(),
            rows: open
                .iter()
                .map(|row| context.row_model(row, None))
                .collect(),
            done_count: if show_done_rows { done.len() } else { 0 },
            done_rows: if show_done_rows && done_open {
                done.iter()
                    .map(|row| context.row_model(row, None))
                    .collect()
            } else {
                Vec::new()
            },
            done_open,
            key,
            kind,
            title,
            summary,
        })
    };

    for epic in epics {
        if is_closed(epic) && !options.show_done {
            continue;
        }
        let epic_matches = !query.is_empty() && matches_query(epic, &query);
        let members = children.remove(&epic.id).unwrap_or_default();
        if let Some(section) = build_section(
            epic.id.clone(),
            BeadsSectionKind::Epic(Box::new(context.row_model(epic, None))),
            epic.title.clone(),
            members,
            epic_matches,
        ) {
            sections.push(section);
        }
    }
    if let Some(section) = build_section(
        "loose".to_string(),
        BeadsSectionKind::NotInEpic,
        "Not in an epic".to_string(),
        loose,
        false,
    ) {
        sections.push(section);
    }
    BeadsTreeModel {
        sections,
        counts,
        readiness_ok,
    }
}
