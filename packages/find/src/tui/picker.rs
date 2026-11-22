use crate::agent::{Agent, ALL_AGENTS};
use crate::index::{day_key, Hit, QueryOptions, SearchIndex};
use crate::scan::Record;

use super::*;

/// What the user chose to do with the selected record. `ResumeSession` is the
/// default (Enter); `Copy` puts the prompt on the clipboard; `View` opens it in
/// `$EDITOR`; `Fork` starts a fresh session with the prompt in `fork_agent`
/// (possibly a different agent than it came from).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActionKind {
    ResumeSession,
    Copy,
    View,
    Fork,
}

#[derive(Clone, Copy, Debug)]
pub struct Action {
    /// Index into `SearchIndex::records`.
    pub index: usize,
    pub kind: ActionKind,
    pub fork_agent: Agent,
}

pub(super) const ENTER_TUI_SEQUENCE: &str = "\x1b[?1049h\x1b[?25l\x1b[?1006h\x1b[?1003h";
pub(super) const LEAVE_TUI_SEQUENCE: &str = "\x1b[?1003l\x1b[?1006l\x1b[?25h\x1b[?1049l";

pub(super) const SELECTED_RESULT_STYLE: &str = "\x1b[48;2;42;42;42m\x1b[38;2;202;160;66m";
pub(super) const MUTED_RESULT_STYLE: &str = "\x1b[90m";
pub(super) const RESET_STYLE: &str = "\x1b[0m";
pub(super) const RESULT_LEAD_COLS: usize = 4; // indicator + favorite slot + spacing
pub(super) const RESULT_AGENT_COLS: usize = 8;
pub(super) const RESULT_GAP_COLS: usize = 1;

#[derive(Clone, Copy)]
pub(super) enum ViewRow {
    Day(i64),
    Hit(usize),
}

pub(super) struct MouseEvent {
    pub(super) button: usize,
    pub(super) _x: usize,
    pub(super) _y: usize,
}

// ---------------------------------------------------------------------------
// picker
// ---------------------------------------------------------------------------

pub struct Tui<'a> {
    pub(super) index: &'a mut SearchIndex,
    pub(super) query: Vec<u8>,
    pub(super) query_cursor: usize,
    pub(super) hits: Vec<Hit>,
    pub(super) view_rows: Vec<ViewRow>,
    pub(super) sel: usize,
    pub(super) top: usize,
    pub(super) preview_scroll: usize,
    pub(super) result_scroll: usize,
    pub(super) preview_focus: bool,
    pub(super) wrap_preview: bool,
    pub(super) fullscreen_preview: bool,
    // CDXC:PromptSearch 2026-06-16-18:16:
    // Search results start as a flat relevance list; day grouping stays opt-in
    // through ^d. Recomputing after query edits, filter changes, or grouping
    // toggles returns to the first visible result instead of a stale scroll.
    pub(super) group_by_day: bool,
    /// Bit mask of selected agents. 0 means no filter, so all agents show.
    pub(super) agent_filter_mask: u16,
    pub(super) rows: u16,
    pub(super) cols: u16,
    /// When set, the picker is in "fork into which agent?" mode and digit keys
    /// choose the target instead of typing into the query.
    pub(super) forking: bool,
    pub(super) filtering_agent: bool,
    pub(super) filtering_project: bool,
    pub(super) filter_sel: usize,
    pub(super) project_filter: Option<String>,
    pub(super) project_sel: usize,
    pub(super) project_query: Vec<u8>,
}

impl<'a> Tui<'a> {
    pub fn new(index: &'a mut SearchIndex) -> Self {
        Self {
            index,
            query: Vec::new(),
            query_cursor: 0,
            hits: Vec::new(),
            view_rows: Vec::new(),
            sel: 0,
            top: 0,
            preview_scroll: 0,
            result_scroll: 0,
            preview_focus: false,
            wrap_preview: true,
            fullscreen_preview: false,
            group_by_day: false,
            agent_filter_mask: 0,
            rows: 24,
            cols: 80,
            forking: false,
            filtering_agent: false,
            filtering_project: false,
            filter_sel: 0,
            project_filter: None,
            project_sel: 0,
            project_query: Vec::new(),
        }
    }

    pub(super) fn records(&self) -> &[Record] {
        &self.index.records
    }

    pub(super) fn selected_record(&self) -> Option<&Record> {
        self.hits
            .get(self.sel)
            .map(|h| &self.index.records[h.index])
    }

    pub(super) fn refresh_winsize(&mut self, stdin: &std::io::Stdin) {
        if let Some((rows, cols)) = term::winsize(stdin) {
            self.rows = rows;
            self.cols = cols;
        }
    }

    pub(super) fn recompute(&mut self) {
        let agents: Vec<Agent> = ALL_AGENTS
            .into_iter()
            .filter(|a| self.agent_filter_mask != 0 && (self.agent_filter_mask & a.bit()) != 0)
            .collect();
        let options = QueryOptions {
            query: String::from_utf8_lossy(&self.query).to_string(),
            agents,
            kinds: Vec::new(),
            project: self.project_filter.clone(),
            group_by_day: self.group_by_day,
            offset: 0,
            limit: 0,
        };
        self.hits = self.index.query(&options).hits;
        self.rebuild_view_rows();
        self.sel = 0;
        self.top = 0;
        self.preview_scroll = 0;
        self.result_scroll = 0;
    }

    fn rebuild_view_rows(&mut self) {
        self.view_rows.clear();
        if !self.group_by_day {
            self.view_rows
                .extend((0..self.hits.len()).map(ViewRow::Hit));
            return;
        }
        let mut last_day: Option<i64> = None;
        for (i, hit) in self.hits.iter().enumerate() {
            let d = day_key(self.index.records[hit.index].ts);
            if last_day != Some(d) {
                self.view_rows.push(ViewRow::Day(d));
                last_day = Some(d);
            }
            self.view_rows.push(ViewRow::Hit(i));
        }
    }

    fn bottom_pane_height(&self) -> usize {
        if self.filtering_agent || self.filtering_project {
            // Pi-style selectors keep up to ~10 visible items plus search/hints.
            // Give the picker more room and shrink the results list instead of
            // overflowing the terminal when the window is short.
            return 13.min((self.rows as usize).saturating_sub(3));
        }
        if self.fullscreen_preview {
            (self.rows as usize).saturating_sub(2)
        } else {
            7
        }
    }

    pub(super) fn list_height(&self) -> usize {
        // 1 prompt line + 1 separator + bottom pane.
        let reserved = 1 + 1 + self.bottom_pane_height();
        let rows = self.rows as usize;
        if rows <= reserved + 1 {
            return 1;
        }
        rows - reserved
    }

    pub(super) fn bottom_rows_after_list(&self, list_height: usize) -> usize {
        (self.rows as usize).saturating_sub(1 + list_height + 1)
    }

    fn selected_view_row(&self) -> usize {
        for (i, row) in self.view_rows.iter().enumerate() {
            if let ViewRow::Hit(hit_idx) = row {
                if *hit_idx == self.sel {
                    return i;
                }
            }
        }
        0
    }

    fn view_row_height(row: ViewRow) -> usize {
        match row {
            ViewRow::Day(_) => 1,
            ViewRow::Hit(_) => 3,
        }
    }

    fn visual_offset_for_view_row(&self, target: usize) -> usize {
        self.view_rows
            .iter()
            .take(target.min(self.view_rows.len()))
            .map(|row| Self::view_row_height(*row))
            .sum()
    }

    pub(super) fn clamp_scroll(&mut self) {
        let h = self.list_height();
        if self.view_rows.is_empty() {
            self.top = 0;
            return;
        }
        let selected_row = self.selected_view_row();
        if self.top >= self.view_rows.len() {
            self.top = self.view_rows.len() - 1;
        }
        let selected_offset = self.visual_offset_for_view_row(selected_row);
        let selected_height = Self::view_row_height(self.view_rows[selected_row]);
        let top_offset = self.visual_offset_for_view_row(self.top);
        if selected_offset < top_offset {
            self.top = selected_row;
            return;
        }
        if selected_offset + selected_height <= top_offset + h {
            return;
        }
        self.top = selected_row;
        let mut visible = selected_height;
        while self.top > 0 {
            let prev_height = Self::view_row_height(self.view_rows[self.top - 1]);
            if visible + prev_height > h {
                break;
            }
            self.top -= 1;
            visible += prev_height;
        }
    }
}
