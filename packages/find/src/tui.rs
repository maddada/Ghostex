//! The interactive picker (port of zehn's `src/tui.zig`).
//!
//! CDXC:PromptSearch 2026-08-20:
//! Two hotkeys moved so the terminal picker and the Find GUI can share one key
//! map. `^t` (agents) and `^r` (projects) are unusable in the GUI: browsers
//! reserve Ctrl+T for a new tab and Ctrl+R for reload, and a page cannot take
//! them back. They are now `^g` (a-g-ents) and `^j` (pro-j-ect) in both surfaces.
//! Because `^j` is byte 10, Enter is CR-only here — raw mode already clears
//! ICRNL, so Enter always arrives as byte 13.

use std::io::{Read, Write};

use crate::agent::{Agent, ALL_AGENTS};
use crate::index::{day_key, Hit, QueryOptions, SearchIndex, SECONDS_PER_DAY, UNKNOWN_DAY_KEY};
use crate::scan::{civil_from_day_key, project_display_name, Record, Usage};
use crate::unicode as uni;

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

const ENTER_TUI_SEQUENCE: &str = "\x1b[?1049h\x1b[?25l\x1b[?1006h\x1b[?1003h";
const LEAVE_TUI_SEQUENCE: &str = "\x1b[?1003l\x1b[?1006l\x1b[?25h\x1b[?1049l";

const SELECTED_RESULT_STYLE: &str = "\x1b[48;2;42;42;42m\x1b[38;2;202;160;66m";
const MUTED_RESULT_STYLE: &str = "\x1b[90m";
const RESET_STYLE: &str = "\x1b[0m";
const RESULT_LEAD_COLS: usize = 4; // indicator + favorite slot + spacing
const RESULT_AGENT_COLS: usize = 8;
const RESULT_GAP_COLS: usize = 1;

#[derive(Clone, Copy)]
enum ViewRow {
    Day(i64),
    Hit(usize),
}

struct MouseEvent {
    button: usize,
    _x: usize,
    _y: usize,
}

// ---------------------------------------------------------------------------
// terminal plumbing
// ---------------------------------------------------------------------------

#[cfg(unix)]
mod term {
    use std::os::fd::AsRawFd;

    // Saved terminal state for restoration from a signal handler (which cannot
    // take arguments). Set while the TUI owns the terminal.
    static mut ORIGINAL: Option<libc::termios> = None;
    static ACTIVE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

    pub fn restore() {
        use std::sync::atomic::Ordering;
        if !ACTIVE.swap(false, Ordering::SeqCst) {
            return;
        }
        let seq = super::LEAVE_TUI_SEQUENCE.as_bytes();
        unsafe {
            libc::write(1, seq.as_ptr() as *const libc::c_void, seq.len());
            #[allow(static_mut_refs)]
            if let Some(orig) = ORIGINAL {
                libc::tcsetattr(0, libc::TCSAFLUSH, &orig);
            }
        }
    }

    extern "C" fn on_signal(sig: libc::c_int) {
        restore();
        unsafe {
            libc::signal(sig, libc::SIG_DFL);
            libc::raise(sig);
        }
    }

    pub fn install_signal_handlers() {
        unsafe {
            for sig in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP, libc::SIGQUIT] {
                libc::signal(sig, on_signal as libc::sighandler_t);
            }
        }
    }

    pub fn enter_raw() -> Result<(), String> {
        unsafe {
            let mut orig: libc::termios = std::mem::zeroed();
            if libc::tcgetattr(0, &mut orig) != 0 {
                return Err("not a terminal".to_string());
            }
            ORIGINAL = Some(orig);
            let mut t = orig;
            t.c_lflag &= !(libc::ICANON | libc::ECHO | libc::ISIG | libc::IEXTEN);
            t.c_iflag &= !(libc::IXON | libc::ICRNL);
            if libc::tcsetattr(0, libc::TCSAFLUSH, &t) != 0 {
                return Err("failed to enter raw mode".to_string());
            }
        }
        ACTIVE.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    pub fn leave_raw() {
        use std::sync::atomic::Ordering;
        ACTIVE.store(false, Ordering::SeqCst);
        unsafe {
            #[allow(static_mut_refs)]
            if let Some(orig) = ORIGINAL {
                libc::tcsetattr(0, libc::TCSAFLUSH, &orig);
            }
        }
    }

    /// Terminal size, rejecting implausible values. A bad ioctl can leave the
    /// struct uninitialised (0xAAAA has been seen on macOS), and a multi-thousand
    /// row "terminal" makes the renderer emit a giant blank frame that scrolls
    /// all real output off-screen.
    pub fn winsize(stdin: &std::io::Stdin) -> Option<(u16, u16)> {
        unsafe {
            let mut ws: libc::winsize = std::mem::zeroed();
            if libc::ioctl(stdin.as_raw_fd(), libc::TIOCGWINSZ, &mut ws) != 0 {
                return None;
            }
            if ws.ws_row == 0 || ws.ws_col == 0 || ws.ws_row > 4096 || ws.ws_col > 4096 {
                return None;
            }
            Some((ws.ws_row, ws.ws_col))
        }
    }
}

#[cfg(not(unix))]
mod term {
    pub fn restore() {}
    pub fn install_signal_handlers() {}
    pub fn enter_raw() -> Result<(), String> {
        Err("the zehn picker requires a POSIX terminal".to_string())
    }
    pub fn leave_raw() {}
    pub fn winsize(_stdin: &std::io::Stdin) -> Option<(u16, u16)> {
        None
    }
}

// ---------------------------------------------------------------------------
// picker
// ---------------------------------------------------------------------------

pub struct Tui<'a> {
    index: &'a mut SearchIndex,
    query: Vec<u8>,
    query_cursor: usize,
    hits: Vec<Hit>,
    view_rows: Vec<ViewRow>,
    sel: usize,
    top: usize,
    preview_scroll: usize,
    result_scroll: usize,
    preview_focus: bool,
    wrap_preview: bool,
    fullscreen_preview: bool,
    // CDXC:PromptSearch 2026-06-16-18:16:
    // Search results start as a flat relevance list; day grouping stays opt-in
    // through ^d. Recomputing after query edits, filter changes, or grouping
    // toggles returns to the first visible result instead of a stale scroll.
    group_by_day: bool,
    /// Bit mask of selected agents. 0 means no filter, so all agents show.
    agent_filter_mask: u8,
    rows: u16,
    cols: u16,
    /// When set, the picker is in "fork into which agent?" mode and digit keys
    /// choose the target instead of typing into the query.
    forking: bool,
    filtering_agent: bool,
    filtering_project: bool,
    filter_sel: usize,
    project_filter: Option<String>,
    project_sel: usize,
    project_query: Vec<u8>,
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

    fn records(&self) -> &[Record] {
        &self.index.records
    }

    fn selected_record(&self) -> Option<&Record> {
        self.hits
            .get(self.sel)
            .map(|h| &self.index.records[h.index])
    }

    fn refresh_winsize(&mut self, stdin: &std::io::Stdin) {
        if let Some((rows, cols)) = term::winsize(stdin) {
            self.rows = rows;
            self.cols = cols;
        }
    }

    fn recompute(&mut self) {
        let agents: Vec<Agent> = ALL_AGENTS
            .into_iter()
            .filter(|a| self.agent_filter_mask != 0 && (self.agent_filter_mask & a.bit()) != 0)
            .collect();
        let options = QueryOptions {
            query: String::from_utf8_lossy(&self.query).to_string(),
            agents,
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

    fn list_height(&self) -> usize {
        // 1 prompt line + 1 separator + bottom pane.
        let reserved = 1 + 1 + self.bottom_pane_height();
        let rows = self.rows as usize;
        if rows <= reserved + 1 {
            return 1;
        }
        rows - reserved
    }

    fn bottom_rows_after_list(&self, list_height: usize) -> usize {
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

    fn clamp_scroll(&mut self) {
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

    // -----------------------------------------------------------------------
    // rendering
    // -----------------------------------------------------------------------

    fn result_line_cols(&self) -> usize {
        (self.cols as usize).saturating_sub(1)
    }

    fn result_prompt_cols(&self) -> usize {
        let used = RESULT_LEAD_COLS + RESULT_AGENT_COLS + RESULT_GAP_COLS;
        self.result_line_cols().saturating_sub(used)
    }

    /// Render `text` on a single line: UTF-8 aware, truncated to `max` display
    /// columns, with matched bytes highlighted. Highlight positions are byte
    /// offsets; a codepoint is highlighted when its first byte is a match.
    fn write_highlighted(
        &self,
        b: &mut String,
        text: &str,
        positions: &[u16],
        max: usize,
        selected: bool,
    ) {
        let bytes = text.as_bytes();
        let scroll = if selected { self.result_scroll } else { 0 };
        let mut pi = 0usize;
        let mut i = scroll.min(bytes.len());
        while i < bytes.len() && (bytes[i] & 0xC0) == 0x80 {
            i += 1;
        }
        if i > 0 {
            b.push('…');
        }
        let mut used = 0usize;
        while i < bytes.len() {
            let (cp, len) = uni::decode(&bytes[i..]);
            let is_ctrl = uni::is_control(cp);
            let cw = if is_ctrl { 1 } else { uni::char_width(cp) };
            if used + cw > max {
                if max > 0 && used < max {
                    b.push('…');
                }
                return;
            }
            while pi < positions.len() && (positions[pi] as usize) < i {
                pi += 1;
            }
            let hl = pi < positions.len() && positions[pi] as usize == i;
            if hl {
                pi += 1;
                b.push_str(if selected { "\x1b[1m" } else { "\x1b[1;33m" });
            }
            if is_ctrl {
                b.push(' ');
            } else {
                b.push_str(&text[i..i + len]);
            }
            if hl {
                b.push_str(RESET_STYLE);
                if selected {
                    b.push_str(SELECTED_RESULT_STYLE);
                }
            }
            used += cw;
            i += len;
        }
    }

    fn append_plain_truncated(&self, b: &mut String, text: &str, max_cols: usize) {
        let bytes = text.as_bytes();
        let mut used = 0usize;
        let mut i = 0usize;
        while i < bytes.len() {
            let (cp, len) = uni::decode(&bytes[i..]);
            let is_ctrl = uni::is_control(cp);
            let cw = if is_ctrl { 1 } else { uni::char_width(cp) };
            if used + cw > max_cols {
                if max_cols > 0 && used < max_cols {
                    b.push('…');
                }
                return;
            }
            if is_ctrl {
                b.push(' ');
            } else {
                b.push_str(&text[i..i + len]);
            }
            used += cw;
            i += len;
        }
    }

    fn write_result_lead(&self, b: &mut String, selected: bool, marker: bool, favorite: bool) {
        if selected {
            b.push_str(SELECTED_RESULT_STYLE);
        }
        if selected && marker {
            b.push_str("› ");
        } else {
            b.push_str("  ");
        }
        if favorite {
            if selected {
                b.push('★');
            } else {
                b.push_str("\x1b[1;33m★");
                b.push_str(RESET_STYLE);
            }
        } else {
            b.push(' ');
        }
        b.push(' ');
    }

    fn finish_result_line(&self, b: &mut String, selected: bool) {
        if selected {
            b.push_str("\x1b[K");
        }
        b.push_str(RESET_STYLE);
        b.push_str("\r\n");
    }

    fn write_inline_session_summary(&self, b: &mut String, rec: &Record, selected: bool) {
        let summary = format!("{} • {}", rec.display_title(), rec.project_display_name());
        if !selected {
            b.push_str(MUTED_RESULT_STYLE);
        }
        self.append_plain_truncated(b, &summary, self.result_prompt_cols());
        if !selected {
            b.push_str(RESET_STYLE);
        }
    }

    fn write_result_row(
        &self,
        b: &mut String,
        hit_idx: usize,
        now: i64,
        max_lines: usize,
    ) -> usize {
        if max_lines == 0 {
            return 0;
        }
        let hit = &self.hits[hit_idx];
        let rec = &self.index.records[hit.index];
        let selected = hit_idx == self.sel;

        self.write_result_lead(b, selected, true, hit.favorite);
        if !selected {
            b.push_str(rec.agent.ansi_color());
        }
        b.push_str(&format!("{:<8}", rec.agent.label()));
        if !selected {
            b.push_str(RESET_STYLE);
        }
        b.push(' ');
        self.write_highlighted(
            b,
            &rec.text,
            &hit.positions,
            self.result_prompt_cols(),
            selected,
        );
        self.finish_result_line(b, selected);
        if max_lines == 1 {
            return 1;
        }

        self.write_result_lead(b, selected, false, false);
        let compact = format_last_active_compact(rec.ts, now);
        if !selected {
            b.push_str(MUTED_RESULT_STYLE);
        }
        b.push_str(&format!("{compact:<8}"));
        if !selected {
            b.push_str(RESET_STYLE);
        }
        b.push(' ');
        self.write_inline_session_summary(b, rec, selected);
        self.finish_result_line(b, selected);
        if max_lines == 2 {
            return 2;
        }

        b.push_str("\r\n");
        3
    }

    fn write_day_header_row(&self, b: &mut String, day: i64, now: i64) {
        b.push_str(&format!(
            "  \x1b[1;90m{}\x1b[0m\r\n",
            format_day_header(day, now)
        ));
    }

    fn count_digits(n: usize) -> usize {
        let mut x = n;
        let mut digits = 1;
        while x >= 10 {
            digits += 1;
            x /= 10;
        }
        digits
    }

    fn write_prompt_line(&self, b: &mut String) {
        let prefix_cols = 2usize;
        let counts =
            2 + Self::count_digits(self.hits.len()) + 1 + Self::count_digits(self.records().len());
        let status_cols = if self.cols >= 96 {
            counts + 72
        } else if self.cols >= 64 {
            counts + 25
        } else {
            counts
        };
        // Keep one column spare before CRLF. Many terminals auto-wrap as soon as
        // the cursor reaches the last column, which adds a physical line and can
        // scroll the sticky prompt off the top of the alt screen.
        let line_cols = 1.max((self.cols as usize).saturating_sub(1));
        let query_max = 1.max(line_cols.saturating_sub(prefix_cols + status_cols));

        b.push_str("\x1b[1;36m❯ \x1b[0m");
        self.write_query_with_cursor(b, query_max);
        if self.cols >= 96 {
            b.push_str(&format!(
                "  \x1b[90m{}/{}  ·  ^d days  ^g agents  ^j projects  ^f fav  ^e view  ^y copy  ^o fork\x1b[0m",
                self.hits.len(),
                self.records().len()
            ));
        } else if self.cols >= 64 {
            b.push_str(&format!(
                "  \x1b[90m{}/{}  ·  ^d ^g ^j ^f ^e ^y ^o\x1b[0m",
                self.hits.len(),
                self.records().len()
            ));
        } else {
            b.push_str(&format!(
                "  \x1b[90m{}/{}\x1b[0m",
                self.hits.len(),
                self.records().len()
            ));
        }
        b.push_str("\r\n");
    }

    fn write_query_with_cursor(&self, b: &mut String, max: usize) {
        let q = &self.query;
        let mut start = 0usize;
        if self.query_cursor > max / 2 {
            start = self.query_cursor - max / 2;
        }
        while start < q.len() && (q[start] & 0xC0) == 0x80 {
            start += 1;
        }
        let mut end = q.len().min(start + max);
        while end < q.len() && (q[end] & 0xC0) == 0x80 {
            end -= 1;
        }
        if self.query_cursor >= end && end < q.len() {
            end = uni::next_char(q, self.query_cursor);
            start = end.saturating_sub(max);
            while start < q.len() && (q[start] & 0xC0) == 0x80 {
                start += 1;
            }
        }

        if start > 0 {
            b.push('…');
        }
        b.push_str(&String::from_utf8_lossy(&q[start..self.query_cursor]));
        b.push_str("\x1b[7m");
        if self.query_cursor < q.len() {
            let (_, len) = uni::decode(&q[self.query_cursor..]);
            b.push_str(&String::from_utf8_lossy(
                &q[self.query_cursor..self.query_cursor + len],
            ));
            b.push_str("\x1b[0m");
            b.push_str(&String::from_utf8_lossy(&q[self.query_cursor + len..end]));
        } else {
            b.push(' ');
            b.push_str("\x1b[0m");
        }
        if end < q.len() {
            b.push('…');
        }
    }

    fn git_branch(&self, project: &str) -> Option<String> {
        if project.is_empty() {
            return None;
        }
        let data = std::fs::read_to_string(std::path::Path::new(project).join(".git/HEAD")).ok()?;
        let trimmed = data.trim();
        trimmed.strip_prefix("ref: refs/heads/").map(str::to_string)
    }

    fn write_project_line(&self, b: &mut String, rec: &Record) {
        let max_cols = 1.max((self.cols as usize).saturating_sub(1));
        let mut line = String::new();
        line.push_str(if rec.project.is_empty() {
            "-"
        } else {
            &rec.project
        });
        if let Some(branch) = self.git_branch(&rec.project) {
            line.push_str(&format!(" ({branch})"));
        }
        let pos = if self.hits.is_empty() {
            0
        } else {
            self.sel + 1
        };
        line.push_str(&format!("  {pos}/{}", self.hits.len()));
        self.append_agent_filter_status(&mut line);

        b.push_str(MUTED_RESULT_STYLE);
        self.append_plain_truncated(b, &line, max_cols);
        b.push_str("\x1b[0m\r\n\r\n");
    }

    fn write_usage_status(&self, b: &mut String, u: &Usage) {
        if u.input > 0 {
            b.push_str(&format!("↑{} ", u.input));
        }
        if u.output > 0 {
            b.push_str(&format!("↓{} ", u.output));
        }
        if u.cache_read > 0 {
            b.push_str(&format!("R{} ", u.cache_read));
        }
        if u.cache_write > 0 {
            b.push_str(&format!("W{} ", u.cache_write));
        }
        if u.cost > 0.0 {
            b.push_str(&format!("${:.3} ", u.cost));
        }
    }

    fn write_metadata_line(&self, b: &mut String, rec: &Record) {
        b.push_str(MUTED_RESULT_STYLE);
        b.push_str(&format!("{} ", format_last_active_full(rec.ts)));
        self.write_usage_status(b, &rec.meta.usage);
        if !rec.meta.plan.is_empty() {
            b.push_str(&format!("({}) ", rec.meta.plan));
        }
        if rec.meta.usage.rate_percent > 0.0 {
            b.push_str(&format!("{:.1}%", rec.meta.usage.rate_percent));
        }
        if rec.meta.usage.context_window > 0 {
            b.push_str(&format!("/{} ", rec.meta.usage.context_window));
        }
        if !rec.meta.provider.is_empty() {
            b.push_str(&format!("({})", rec.meta.provider));
        }
        if !rec.meta.model.is_empty() {
            if !rec.meta.provider.is_empty() {
                b.push(' ');
            }
            b.push_str(&rec.meta.model);
        }
        if !rec.meta.thinking.is_empty() {
            b.push_str(&format!(" • {}", rec.meta.thinking));
        }
        b.push_str("\x1b[0m\r\n");
    }

    fn append_agent_filter_status(&self, out: &mut String) {
        if self.agent_filter_mask == 0 {
            return;
        }
        out.push_str("  agents:");
        let mut first = true;
        for agent in ALL_AGENTS {
            if (self.agent_filter_mask & agent.bit()) == 0 {
                continue;
            }
            if !first {
                out.push(',');
            }
            out.push_str(agent.label());
            first = false;
        }
    }

    fn write_agent_filter_picker(&self, b: &mut String, max_rows: usize) {
        b.push_str("\r\n");
        let rows = max_rows.min(ALL_AGENTS.len());
        for (idx, agent) in ALL_AGENTS.iter().take(rows).enumerate() {
            let focused = idx == self.filter_sel;
            let selected = (self.agent_filter_mask & agent.bit()) != 0;
            if focused {
                b.push_str(&format!("\x1b[1;36m→ {}\x1b[0m", agent.label()));
            } else {
                b.push_str(&format!("  {}", agent.label()));
            }
            if selected {
                b.push_str(" \x1b[1;32m✓\x1b[0m");
            }
            b.push_str("\r\n");
        }
        b.push_str("\r\n\x1b[90mSelect none to show all agents.\x1b[0m\r\n");
        b.push_str("\r\n\x1b[90m↑/↓ or ^p/^n move · Enter/Space toggle · 1-6 quick toggle · Esc close\x1b[0m\r\n");
    }

    fn write_project_filter_picker(&self, b: &mut String, max_rows: usize) {
        b.push_str("\r\n\x1b[1;36m> \x1b[0m");
        b.push_str(&String::from_utf8_lossy(&self.project_query));
        b.push_str("\r\n\r\n");
        let projects = self.filtered_projects();
        let count = projects.len();
        // Budget for: blank, search line, blank, optional scroll info, blank, hint.
        let rows_avail = if max_rows > 6 { max_rows - 6 } else { 1 };
        let visible = rows_avail.min(count);
        // Keep the highlighted row near the middle, pinning only at the ends.
        let start = if count <= visible {
            0
        } else {
            self.project_sel
                .saturating_sub(visible / 2)
                .min(count - visible)
        };
        for shown in 0..visible {
            let idx = start + shown;
            let path = projects.get(idx).copied();
            let focused = idx == self.project_sel;
            let label = path.map(project_display_name).unwrap_or("-");
            if focused {
                b.push_str(&format!("\x1b[1;36m→ {label}\x1b[0m"));
            } else {
                b.push_str(&format!("  {label}"));
            }
            let selected = match path {
                Some(p) => self.project_filter.as_deref() == Some(p),
                None => self.project_filter.is_none(),
            };
            if selected {
                b.push_str(" \x1b[1;32m✓\x1b[0m");
            }
            b.push_str("\r\n");
        }
        if count > visible {
            b.push_str(&format!(
                "  \x1b[90m({}/{count})\x1b[0m\r\n",
                self.project_sel + 1
            ));
        }
        if count == 0 {
            b.push_str("  \x1b[90mNo matching projects\x1b[0m\r\n");
        }
        b.push_str("\r\n\x1b[90mType to search · ↑/↓ or ^p/^n move · Enter select · Space toggles/clears · Esc close\x1b[0m\r\n");
    }

    fn render(&mut self, out: &mut impl Write, stdin: &std::io::Stdin) -> std::io::Result<()> {
        self.refresh_winsize(stdin); // pick up live terminal resizes
        self.clamp_scroll();
        let mut b = String::with_capacity(16 * 1024);
        b.push_str("\x1b[2J\x1b[H"); // clear + home

        self.write_prompt_line(&mut b);

        let h = self.list_height();
        let now = now_seconds();
        let mut row = 0usize;
        let mut row_idx = self.top;
        while row < h {
            let Some(view_row) = self.view_rows.get(row_idx).copied() else {
                b.push_str("\r\n");
                row += 1;
                continue;
            };
            match view_row {
                ViewRow::Day(day) => {
                    self.write_day_header_row(&mut b, day, now);
                    row += 1;
                }
                ViewRow::Hit(hit_idx) => {
                    row += self.write_result_row(&mut b, hit_idx, now, h - row);
                }
            }
            row_idx += 1;
        }

        b.push_str(MUTED_RESULT_STYLE);
        let sep_cols = 1.max((self.cols as usize).saturating_sub(1));
        for _ in 0..sep_cols {
            b.push('─');
        }
        b.push_str("\x1b[0m\r\n");

        if self.filtering_project {
            self.write_project_filter_picker(&mut b, self.bottom_rows_after_list(h));
            return flush_frame(out, b);
        }
        if self.filtering_agent {
            self.write_agent_filter_picker(&mut b, self.bottom_rows_after_list(h));
            return flush_frame(out, b);
        }
        if self.forking {
            b.push_str("\x1b[1;36mfork prompt into:\x1b[0m  ");
            b.push_str("\x1b[1m1\x1b[0m claude  \x1b[1m2\x1b[0m codex  \x1b[1m3\x1b[0m pi  \x1b[1m4\x1b[0m opencode  \x1b[1m5\x1b[0m cursor  \x1b[1m6\x1b[0m grok");
            b.push_str("  \x1b[90m(esc cancels)\x1b[0m\r\n");
            return flush_frame(out, b);
        }

        if self.sel < self.hits.len() {
            let rec = self.index.records[self.hits[self.sel].index].clone();
            let bottom_rows = self.bottom_rows_after_list(h);
            self.write_project_line(&mut b, &rec);
            let has_preview_title = self.preview_focus && bottom_rows > 4;
            // project line + blank + optional title + metadata line
            let fixed_rows = 3 + usize::from(has_preview_title);
            let preview_lines = if bottom_rows > fixed_rows {
                bottom_rows - fixed_rows
            } else {
                1
            };
            let preview_cols = 1.max((self.cols as usize).saturating_sub(1));
            if has_preview_title {
                b.push_str("\x1b[1;36mpreview\x1b[0m\r\n");
            }
            self.write_preview(&mut b, &rec, preview_lines, preview_cols);
            self.write_metadata_line(&mut b, &rec);
        }

        flush_frame(out, b)
    }

    fn write_preview(
        &self,
        b: &mut String,
        rec: &Record,
        preview_lines: usize,
        preview_cols: usize,
    ) {
        let text = rec.text.as_bytes();
        let mut i = 0usize;
        let mut skipped = 0usize;
        while i < text.len() && skipped < self.preview_scroll {
            while i < text.len() && text[i] != b'\n' {
                i += 1;
            }
            if i < text.len() && text[i] == b'\n' {
                i += 1;
            }
            skipped += 1;
        }
        let mut line_lines = 0usize;
        while i < text.len() && line_lines < preview_lines {
            let mut used = 0usize;
            let line_start = i;
            let mut filled = false; // a wrap already consumed the last budgeted row
            while i < text.len() {
                if text[i] == b'\n' {
                    break;
                }
                let (cp, len) = uni::decode(&text[i..]);
                let is_ctrl = uni::is_control(cp);
                let cw = if is_ctrl { 1 } else { uni::char_width(cp) };
                if used + cw > preview_cols {
                    if !self.wrap_preview {
                        break;
                    }
                    b.push_str("\r\n");
                    line_lines += 1;
                    used = 0;
                    if line_lines >= preview_lines {
                        filled = true;
                        break;
                    }
                }
                if is_ctrl {
                    b.push(' ');
                } else {
                    b.push_str(&rec.text[i..i + len]);
                }
                used += cw;
                i += len;
            }
            // The wrap above already emitted this row's newline and counted it.
            // Emitting the line terminator again would make the frame one line
            // taller than the terminal, scrolling the prompt off the alt screen.
            if filled {
                break;
            }
            b.push_str("\r\n");
            if i < text.len() && text[i] == b'\n' {
                i += 1;
            }
            line_lines += 1;
            if i == line_start && used == 0 {
                i += 1; // guarantee progress
            }
        }
    }

    // -----------------------------------------------------------------------
    // input
    // -----------------------------------------------------------------------

    /// Returns the chosen Action, or None if cancelled.
    pub fn run(&mut self) -> Result<Option<Action>, String> {
        let stdin = std::io::stdin();
        self.refresh_winsize(&stdin);
        term::install_signal_handlers();
        term::enter_raw()?;
        let mut stdout = std::io::stdout();
        let _ = stdout.write_all(ENTER_TUI_SEQUENCE.as_bytes());
        let _ = stdout.flush();

        let outcome = self.event_loop(&stdin, &mut stdout);

        let _ = stdout.write_all(LEAVE_TUI_SEQUENCE.as_bytes());
        let _ = stdout.flush();
        term::leave_raw();
        outcome
    }

    fn event_loop(
        &mut self,
        stdin: &std::io::Stdin,
        stdout: &mut std::io::Stdout,
    ) -> Result<Option<Action>, String> {
        self.recompute();
        let mut handle = stdin.lock();
        let mut ibuf = [0u8; 256];
        loop {
            self.render(stdout, stdin).map_err(|e| e.to_string())?;
            let n = match handle.read(&mut ibuf) {
                Ok(0) | Err(_) => return Ok(None),
                Ok(n) => n,
            };
            let mut i = 0usize;
            while i < n {
                let c = ibuf[i];

                if self.filtering_project {
                    if c == 27 {
                        if let Some(consumed) = self.handle_escape_sequence(&ibuf[i..n]) {
                            i += consumed;
                            continue;
                        }
                        self.filtering_project = false;
                        i += 1;
                        continue;
                    }
                    match c {
                        13 => {
                            self.apply_project_filter_selection();
                            self.filtering_project = false;
                        }
                        b' ' => self.toggle_project_filter_selection(),
                        127 | 8 => self.backspace_project_query(),
                        14 => self.move_project_selection(1),
                        16 => self.move_project_selection(-1),
                        _ => {
                            if (32..127).contains(&c) || c >= 128 {
                                self.insert_project_query_byte(c);
                            }
                        }
                    }
                    i += 1;
                    continue;
                }

                // While picking an agent filter, digits choose the filter and any
                // other key (esc included) cancels back to normal browsing.
                if self.filtering_agent {
                    if c == 27 {
                        if let Some(consumed) = self.handle_escape_sequence(&ibuf[i..n]) {
                            i += consumed;
                            continue;
                        }
                        self.filtering_agent = false;
                        i += 1;
                        continue;
                    }
                    match c {
                        13 | b' ' => self.toggle_filter_selection(),
                        b'1'..=b'6' => {
                            self.filter_sel = (c - b'1') as usize;
                            self.toggle_filter_selection();
                        }
                        14 => self.move_filter_selection(1),
                        16 => self.move_filter_selection(-1),
                        _ => {}
                    }
                    i += 1;
                    continue;
                }

                // While picking a fork target, digits choose the agent and any
                // other key (esc included) cancels back to normal browsing.
                if self.forking {
                    self.forking = false;
                    if self.sel < self.hits.len() {
                        let agent = match c {
                            b'1' => Some(Agent::Claude),
                            b'2' => Some(Agent::Codex),
                            b'3' => Some(Agent::Pi),
                            b'4' => Some(Agent::Opencode),
                            b'5' => Some(Agent::Cursor),
                            b'6' => Some(Agent::Grok),
                            _ => None,
                        };
                        if let Some(fork_agent) = agent {
                            return Ok(Some(Action {
                                index: self.hits[self.sel].index,
                                kind: ActionKind::Fork,
                                fork_agent,
                            }));
                        }
                    }
                    i += 1;
                    continue;
                }

                if c == 27 {
                    if let Some(consumed) = self.handle_escape_sequence(&ibuf[i..n]) {
                        i += consumed;
                        continue;
                    }
                    return Ok(None); // bare ESC quits
                }
                match c {
                    3 => return Ok(None),            // ctrl-c
                    4 => self.toggle_day_grouping(), // ctrl-d
                    13 => {
                        // Enter. CR only: ^j (byte 10) is the project picker, and
                        // raw mode clears ICRNL so Enter always arrives as CR.
                        if self.sel < self.hits.len() {
                            return Ok(Some(Action {
                                index: self.hits[self.sel].index,
                                kind: ActionKind::ResumeSession,
                                fork_agent: Agent::Claude,
                            }));
                        }
                        return Ok(None);
                    }
                    6 => {
                        // ctrl-f
                        if self.preview_focus {
                            self.fullscreen_preview = !self.fullscreen_preview;
                        } else {
                            self.toggle_favorite();
                        }
                    }
                    5 => {
                        // ctrl-e: open selected prompt in $EDITOR
                        if self.sel < self.hits.len() {
                            return Ok(Some(Action {
                                index: self.hits[self.sel].index,
                                kind: ActionKind::View,
                                fork_agent: Agent::Claude,
                            }));
                        }
                    }
                    25 => {
                        // ctrl-y: copy selected to clipboard
                        if self.sel < self.hits.len() {
                            return Ok(Some(Action {
                                index: self.hits[self.sel].index,
                                kind: ActionKind::Copy,
                                fork_agent: Agent::Claude,
                            }));
                        }
                    }
                    9 => self.preview_focus = !self.preview_focus, // tab
                    15 => {
                        // ctrl-o: fork into another agent
                        if self.sel < self.hits.len() {
                            self.forking = true;
                        }
                    }
                    11 => self.kill_to_end(),                // ctrl-k
                    10 => self.open_project_filter_picker(), // ctrl-j
                    7 => self.open_agent_filter_picker(),    // ctrl-g
                    21 => self.kill_to_beginning(),          // ctrl-u
                    b'w' | b'W' => {
                        if self.preview_focus {
                            self.wrap_preview = !self.wrap_preview;
                        } else {
                            self.insert_query_byte(c);
                        }
                    }
                    b'f' | b'F' => {
                        if self.preview_focus {
                            self.fullscreen_preview = !self.fullscreen_preview;
                        } else {
                            self.insert_query_byte(c);
                        }
                    }
                    127 | 8 => self.backspace(),
                    14 => self.move_down(), // ctrl-n
                    16 => self.move_up(),   // ctrl-p
                    _ => {
                        // accept printable ASCII and any UTF-8 lead/continuation
                        // byte (>=128), but not DEL
                        if (32..127).contains(&c) || c >= 128 {
                            self.insert_query_byte(c);
                        }
                    }
                }
                i += 1;
            }
        }
    }

    fn handle_escape_sequence(&mut self, bytes: &[u8]) -> Option<usize> {
        if bytes.len() < 3 || bytes[0] != 27 || bytes[1] != b'[' {
            return None;
        }
        if bytes[2] == b'<' {
            if let Some((event, consumed)) = parse_sgr_mouse(bytes) {
                self.handle_mouse_event(event);
                return Some(consumed);
            }
            return Some(bytes.len());
        }
        if bytes[2] == b'5' && bytes.len() >= 4 && bytes[3] == b'~' {
            if self.preview_focus {
                self.scroll_preview(-1);
            } else if self.group_by_day {
                self.jump_day(-1);
            }
            return Some(4);
        }
        if bytes[2] == b'6' && bytes.len() >= 4 && bytes[3] == b'~' {
            if self.preview_focus {
                self.scroll_preview(1);
            } else if self.group_by_day {
                self.jump_day(1);
            }
            return Some(4);
        }
        if bytes[2] == b'3'
            && bytes.len() >= 6
            && bytes[3] == b';'
            && bytes[4] == b'5'
            && bytes[5] == b'~'
        {
            self.delete_word_forward(); // ctrl-delete
            return Some(6);
        }
        if bytes.len() >= 8 && &bytes[2..8] == b"127;5u" {
            self.delete_word_backward(); // ctrl-backspace (CSI u)
            return Some(8);
        }
        if bytes.len() >= 6 && &bytes[2..6] == b"8;5u" {
            self.delete_word_backward(); // ctrl-backspace (CSI u)
            return Some(6);
        }
        if bytes.len() >= 6 && bytes[2] == b'1' && bytes[3] == b';' && bytes[4] == b'5' {
            match bytes[5] {
                b'A' => {
                    if self.filtering_project {
                        self.move_project_selection(-1);
                    } else if self.filtering_agent {
                        self.move_filter_selection(-1);
                    } else if self.group_by_day {
                        self.jump_day(-1);
                    } else {
                        self.move_up();
                    }
                }
                b'B' => {
                    if self.filtering_project {
                        self.move_project_selection(1);
                    } else if self.filtering_agent {
                        self.move_filter_selection(1);
                    } else if self.group_by_day {
                        self.jump_day(1);
                    } else {
                        self.move_down();
                    }
                }
                b'C' => {
                    if self.preview_focus {
                        self.scroll_result_to_end();
                    } else {
                        self.move_word_right();
                    }
                }
                b'D' => {
                    if self.preview_focus {
                        self.result_scroll = 0;
                    } else {
                        self.move_word_left();
                    }
                }
                _ => {}
            }
            return Some(6);
        }
        match bytes[2] {
            b'A' => {
                if self.filtering_project {
                    self.move_project_selection(-1);
                } else if self.filtering_agent {
                    self.move_filter_selection(-1);
                } else {
                    self.move_up();
                }
            }
            b'B' => {
                if self.filtering_project {
                    self.move_project_selection(1);
                } else if self.filtering_agent {
                    self.move_filter_selection(1);
                } else {
                    self.move_down();
                }
            }
            b'C' => {
                if self.preview_focus {
                    self.scroll_result(8);
                } else {
                    self.move_right();
                }
            }
            b'D' => {
                if self.preview_focus {
                    self.scroll_result(-8);
                } else {
                    self.move_left();
                }
            }
            _ => {}
        }
        Some(3)
    }

    fn handle_mouse_event(&mut self, ev: MouseEvent) {
        if self.filtering_agent || self.filtering_project || self.forking {
            return;
        }
        // Wheel input moves the keyboard selection; clicks never select or resume.
        if (ev.button & 64) != 0 {
            match ev.button & 3 {
                0 => self.move_up(),
                1 => self.move_down(),
                _ => {}
            }
        }
    }

    // -----------------------------------------------------------------------
    // state transitions
    // -----------------------------------------------------------------------

    /// Toggle the favorite flag of the selected prompt, persist, and re-rank.
    /// Selection follows the same record across the re-sort so the cursor does
    /// not jump after starring/unstarring.
    fn toggle_favorite(&mut self) {
        let Some(hit) = self.hits.get(self.sel) else {
            return;
        };
        let rec_idx = hit.index;
        let rec = &self.index.records[rec_idx];
        let (agent, text) = (rec.agent, rec.text.clone());
        self.index.toggle_favorite(agent, &text);
        self.recompute();
        if let Some(j) = self.hits.iter().position(|h| h.index == rec_idx) {
            self.sel = j;
        }
    }

    fn toggle_day_grouping(&mut self) {
        self.group_by_day = !self.group_by_day;
        self.recompute();
    }

    fn select_hit(&mut self, hit_idx: usize) {
        if hit_idx >= self.hits.len() || self.sel == hit_idx {
            return;
        }
        self.sel = hit_idx;
        self.preview_scroll = 0;
        self.result_scroll = 0;
    }

    fn jump_day(&mut self, delta: isize) {
        if self.hits.is_empty() || self.sel >= self.hits.len() {
            return;
        }
        let current_day = day_key(self.index.records[self.hits[self.sel].index].ts);
        if delta > 0 {
            for i in (self.sel + 1)..self.hits.len() {
                if day_key(self.index.records[self.hits[i].index].ts) != current_day {
                    self.select_hit(i);
                    return;
                }
            }
            return;
        }
        let mut i = self.sel;
        while i > 0 {
            i -= 1;
            let candidate_day = day_key(self.index.records[self.hits[i].index].ts);
            if candidate_day == current_day {
                continue;
            }
            while i > 0 && day_key(self.index.records[self.hits[i - 1].index].ts) == candidate_day {
                i -= 1;
            }
            self.select_hit(i);
            return;
        }
    }

    fn open_agent_filter_picker(&mut self) {
        self.filtering_agent = true;
        self.filter_sel = 0;
    }

    fn move_filter_selection(&mut self, delta: isize) {
        if delta < 0 {
            self.filter_sel = if self.filter_sel == 0 {
                ALL_AGENTS.len() - 1
            } else {
                self.filter_sel - 1
            };
        } else {
            self.filter_sel = (self.filter_sel + 1) % ALL_AGENTS.len();
        }
    }

    fn toggle_filter_selection(&mut self) {
        self.agent_filter_mask ^= ALL_AGENTS[self.filter_sel].bit();
        self.recompute();
    }

    fn project_matches(&self, project: &str) -> bool {
        if self.project_query.is_empty() {
            return true;
        }
        let q = String::from_utf8_lossy(&self.project_query).to_lowercase();
        let base = project_display_name(project).to_lowercase();
        base.contains(&q) || project.to_lowercase().contains(&q)
    }

    fn filtered_projects(&self) -> Vec<&str> {
        self.index
            .projects()
            .into_iter()
            .filter(|p| self.project_matches(p))
            .collect()
    }

    fn open_project_filter_picker(&mut self) {
        self.filtering_project = true;
        self.project_sel = 0;
        self.project_query.clear();
    }

    fn move_project_selection(&mut self, delta: isize) {
        let count = self.filtered_projects().len();
        if count == 0 {
            return;
        }
        if delta < 0 {
            self.project_sel = if self.project_sel == 0 {
                count - 1
            } else {
                self.project_sel - 1
            };
        } else {
            self.project_sel = (self.project_sel + 1) % count;
        }
    }

    fn apply_project_filter_selection(&mut self) {
        self.project_filter = self
            .filtered_projects()
            .get(self.project_sel)
            .map(|p| p.to_string());
        self.recompute();
    }

    fn toggle_project_filter_selection(&mut self) {
        let picked = self
            .filtered_projects()
            .get(self.project_sel)
            .map(|p| p.to_string());
        self.project_filter = match (picked, self.project_filter.clone()) {
            (None, _) => None,
            (Some(p), Some(cur)) if cur == p => None,
            (Some(p), _) => Some(p),
        };
        self.recompute();
    }

    fn insert_project_query_byte(&mut self, c: u8) {
        self.project_query.push(c);
        self.project_sel = 0;
    }

    fn backspace_project_query(&mut self) {
        if self.project_query.is_empty() {
            return;
        }
        self.project_query.pop();
        while !self.project_query.is_empty()
            && (self.project_query[self.project_query.len() - 1] & 0xC0) == 0x80
        {
            self.project_query.pop();
        }
        self.project_sel = self
            .project_sel
            .min(self.filtered_projects().len().saturating_sub(1));
    }

    fn insert_query_byte(&mut self, c: u8) {
        self.query.insert(self.query_cursor, c);
        self.query_cursor += 1;
        self.recompute();
    }

    fn delete_range(&mut self, start: usize, end: usize) {
        if end <= start {
            return;
        }
        self.query.drain(start..end);
        self.query_cursor = start;
        self.recompute();
    }

    fn backspace(&mut self) {
        if self.query_cursor == 0 {
            return;
        }
        self.delete_range(
            uni::prev_char(&self.query, self.query_cursor),
            self.query_cursor,
        );
    }

    fn kill_to_end(&mut self) {
        self.delete_range(self.query_cursor, self.query.len());
    }

    fn kill_to_beginning(&mut self) {
        self.delete_range(0, self.query_cursor);
    }

    fn move_left(&mut self) {
        self.query_cursor = uni::prev_char(&self.query, self.query_cursor);
    }

    fn move_right(&mut self) {
        self.query_cursor = uni::next_char(&self.query, self.query_cursor);
    }

    fn move_word_left(&mut self) {
        let mut p = self.query_cursor;
        while p > 0 && !is_word_byte(self.query[uni::prev_char(&self.query, p)]) {
            p = uni::prev_char(&self.query, p);
        }
        while p > 0 && is_word_byte(self.query[uni::prev_char(&self.query, p)]) {
            p = uni::prev_char(&self.query, p);
        }
        self.query_cursor = p;
    }

    fn move_word_right(&mut self) {
        let mut p = self.query_cursor;
        while p < self.query.len() && !is_word_byte(self.query[p]) {
            p = uni::next_char(&self.query, p);
        }
        while p < self.query.len() && is_word_byte(self.query[p]) {
            p = uni::next_char(&self.query, p);
        }
        self.query_cursor = p;
    }

    fn delete_word_forward(&mut self) {
        let start = self.query_cursor;
        self.move_word_right();
        self.delete_range(start, self.query_cursor);
    }

    fn delete_word_backward(&mut self) {
        let end = self.query_cursor;
        self.move_word_left();
        self.delete_range(self.query_cursor, end);
    }

    fn scroll_result(&mut self, delta: isize) {
        if delta < 0 {
            self.result_scroll = self.result_scroll.saturating_sub((-delta) as usize);
        } else {
            self.result_scroll += delta as usize;
        }
    }

    fn scroll_result_to_end(&mut self) {
        if let Some(rec) = self.selected_record() {
            self.result_scroll = rec.text.len();
        }
    }

    fn scroll_preview(&mut self, delta: isize) {
        if delta < 0 {
            self.preview_scroll = self.preview_scroll.saturating_sub((-delta) as usize);
        } else {
            self.preview_scroll += delta as usize;
        }
    }

    fn move_down(&mut self) {
        if self.hits.is_empty() {
            return;
        }
        if self.sel + 1 < self.hits.len() {
            self.sel += 1;
            self.preview_scroll = 0;
            self.result_scroll = 0;
        }
    }

    fn move_up(&mut self) {
        if self.sel > 0 {
            self.sel -= 1;
            self.preview_scroll = 0;
            self.result_scroll = 0;
        }
    }
}

fn is_word_byte(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// Write the assembled frame, dropping a single trailing newline first. The
/// frame is sized to fill the terminal exactly, so a final CRLF would push the
/// cursor one row below the bottom and scroll the whole alt screen up — sweeping
/// the sticky `❯` prompt off the top.
fn flush_frame(out: &mut impl Write, frame: String) -> std::io::Result<()> {
    let trimmed = frame.strip_suffix("\r\n").unwrap_or(&frame);
    out.write_all(trimmed.as_bytes())?;
    out.flush()
}

fn parse_sgr_mouse(bytes: &[u8]) -> Option<(MouseEvent, usize)> {
    if bytes.len() < 6 || bytes[0] != 27 || bytes[1] != b'[' || bytes[2] != b'<' {
        return None;
    }
    let mut i = 3usize;
    let button = parse_mouse_number(bytes, &mut i)?;
    if i >= bytes.len() || bytes[i] != b';' {
        return None;
    }
    i += 1;
    let x = parse_mouse_number(bytes, &mut i)?;
    if i >= bytes.len() || bytes[i] != b';' {
        return None;
    }
    i += 1;
    let y = parse_mouse_number(bytes, &mut i)?;
    if i >= bytes.len() || (bytes[i] != b'M' && bytes[i] != b'm') {
        return None;
    }
    Some((
        MouseEvent {
            button,
            _x: x,
            _y: y,
        },
        i + 1,
    ))
}

fn parse_mouse_number(bytes: &[u8], index: &mut usize) -> Option<usize> {
    if *index >= bytes.len() || !bytes[*index].is_ascii_digit() {
        return None;
    }
    let mut value = 0usize;
    while *index < bytes.len() && bytes[*index].is_ascii_digit() {
        value = value * 10 + (bytes[*index] - b'0') as usize;
        *index += 1;
    }
    Some(value)
}

pub fn now_seconds() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn month_name(month: u32) -> &'static str {
    match month {
        1 => "Jan",
        2 => "Feb",
        3 => "Mar",
        4 => "Apr",
        5 => "May",
        6 => "Jun",
        7 => "Jul",
        8 => "Aug",
        9 => "Sep",
        10 => "Oct",
        11 => "Nov",
        12 => "Dec",
        _ => "???",
    }
}

pub fn format_last_active_compact(ts: i64, now: i64) -> String {
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
    let date = civil_from_day_key(day_key(ts));
    format!("{} {}", month_name(date.month), date.day)
}

pub fn format_day_header(day: i64, now: i64) -> String {
    if day == UNKNOWN_DAY_KEY {
        return "Unknown day".to_string();
    }
    let today = day_key(now);
    if day == today {
        return "Today".to_string();
    }
    if day == today - 1 {
        return "Yesterday".to_string();
    }
    if day > today - 7 && day < today {
        return format!("{} days ago", today - day);
    }
    let date = civil_from_day_key(day);
    let now_date = civil_from_day_key(today);
    if date.year == now_date.year {
        return format!("{} {}", month_name(date.month), date.day);
    }
    format!("{} {}, {}", month_name(date.month), date.day, date.year)
}

pub fn format_last_active_full(ts: i64) -> String {
    if ts <= 0 {
        return "last active unknown".to_string();
    }
    let date = civil_from_day_key(day_key(ts));
    let seconds = ts.rem_euclid(SECONDS_PER_DAY);
    let hour = seconds / 3_600;
    let minute = (seconds % 3_600) / 60;
    format!(
        "last active {} {} {:02}:{:02} UTC",
        month_name(date.month),
        date.day,
        hour,
        minute
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn visible_cols_no_ansi(s: &str) -> usize {
        let bytes = s.as_bytes();
        let mut cols = 0usize;
        let mut i = 0usize;
        while i < bytes.len() {
            if bytes[i] == 0x1b {
                i += 1;
                if i < bytes.len() && bytes[i] == b'[' {
                    i += 1;
                    while i < bytes.len() && (bytes[i] < b'@' || bytes[i] > b'~') {
                        i += 1;
                    }
                    if i < bytes.len() {
                        i += 1;
                    }
                }
                continue;
            }
            if bytes[i] == b'\r' || bytes[i] == b'\n' {
                i += 1;
                continue;
            }
            let (cp, len) = uni::decode(&bytes[i..]);
            cols += uni::char_width(cp);
            i += len;
        }
        cols
    }

    fn empty_index() -> SearchIndex {
        SearchIndex::build(
            "/nonexistent-home",
            std::path::Path::new("/nonexistent-cache"),
            "/nonexistent/favorites".into(),
        )
    }

    #[test]
    fn query_render_keeps_typed_text_visible_with_cursor() {
        let mut index = empty_index();
        let mut tui = Tui::new(&mut index);
        tui.cols = 24;
        tui.query = b"abcdef".to_vec();
        tui.query_cursor = tui.query.len();
        let mut out = String::new();
        tui.write_query_with_cursor(&mut out, 20);
        assert!(out.contains("abcdef"));
        assert!(out.contains("\x1b[7m \x1b[0m"));
    }

    #[test]
    fn prompt_line_keeps_a_spare_column_to_avoid_autowrap() {
        for cols in [20u16, 64, 80, 120] {
            let mut index = empty_index();
            let mut tui = Tui::new(&mut index);
            tui.cols = cols;
            tui.query = b"a deliberately long search that used to push the help text past the edge"
                .to_vec();
            tui.query_cursor = tui.query.len();
            let mut out = String::new();
            tui.write_prompt_line(&mut out);
            assert!(
                visible_cols_no_ansi(&out) < cols as usize,
                "prompt line overflowed at {cols} cols"
            );
            assert!(out.ends_with("\r\n"));
        }
    }

    #[test]
    fn prompt_line_advertises_the_remapped_hotkeys() {
        let mut index = empty_index();
        let mut tui = Tui::new(&mut index);
        tui.cols = 120;
        let mut out = String::new();
        tui.write_prompt_line(&mut out);
        assert!(out.contains("^g agents"));
        assert!(out.contains("^j projects"));
        assert!(!out.contains("^t agents"));
        assert!(!out.contains("^r projects"));
    }

    #[test]
    fn query_supports_readline_style_editing() {
        let mut index = empty_index();
        let mut tui = Tui::new(&mut index);
        tui.query = b"hello world".to_vec();
        tui.query_cursor = tui.query.len();

        tui.move_left();
        tui.move_left();
        assert_eq!(tui.query_cursor, 9);
        tui.insert_query_byte(b'!');
        assert_eq!(tui.query, b"hello wor!ld");

        tui.backspace();
        assert_eq!(tui.query, b"hello world");
        assert_eq!(tui.query_cursor, 9);

        tui.kill_to_beginning();
        assert_eq!(tui.query, b"ld");
        assert_eq!(tui.query_cursor, 0);

        tui.kill_to_end();
        assert!(tui.query.is_empty());
    }

    #[test]
    fn query_supports_word_movement_and_word_deletion() {
        let mut index = empty_index();
        let mut tui = Tui::new(&mut index);
        tui.query = b"alpha beta gamma".to_vec();
        tui.query_cursor = tui.query.len();

        tui.move_word_left();
        assert_eq!(tui.query_cursor, 11);
        tui.move_word_left();
        assert_eq!(tui.query_cursor, 6);
        tui.move_word_right();
        assert_eq!(tui.query_cursor, 10);

        tui.query_cursor = tui.query.len();
        tui.delete_word_backward();
        assert_eq!(tui.query, b"alpha beta ");
    }

    #[test]
    fn day_and_last_active_labels_match_the_original_wording() {
        let now = 20_000 * SECONDS_PER_DAY;
        assert_eq!(format_day_header(day_key(now), now), "Today");
        assert_eq!(format_day_header(day_key(now) - 1, now), "Yesterday");
        assert_eq!(format_day_header(day_key(now) - 3, now), "3 days ago");
        assert_eq!(format_day_header(UNKNOWN_DAY_KEY, now), "Unknown day");

        assert_eq!(format_last_active_compact(0, now), "unknown");
        assert_eq!(format_last_active_compact(now - 5, now), "now");
        assert_eq!(format_last_active_compact(now - 120, now), "2m ago");
        assert_eq!(format_last_active_compact(now - 7_200, now), "2h ago");
        assert_eq!(
            format_last_active_compact(now - 2 * SECONDS_PER_DAY, now),
            "2d ago"
        );
        assert!(format_last_active_full(now).starts_with("last active "));
    }

    #[test]
    fn sgr_mouse_wheel_events_parse() {
        let (ev, consumed) = parse_sgr_mouse(b"\x1b[<65;10;20M").expect("wheel event");
        assert_eq!(consumed, 12);
        assert_eq!(ev.button & 64, 64);
        assert_eq!(ev.button & 3, 1);
        assert!(parse_sgr_mouse(b"\x1b[<65;10").is_none());
    }
}
