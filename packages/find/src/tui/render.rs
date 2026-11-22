use std::io::Write;

use crate::agent::ALL_AGENTS;
use crate::scan::{project_display_name, Record, Usage};
use crate::unicode as uni;

use super::*;

impl<'a> Tui<'a> {
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

    pub(super) fn write_prompt_line(&self, b: &mut String) {
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

    pub(super) fn write_query_with_cursor(&self, b: &mut String, max: usize) {
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
        b.push_str("\r\n\x1b[90m↑/↓ or ^p/^n move · Enter/Space toggle · 1-7 quick toggle · Esc close\x1b[0m\r\n");
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

    pub(super) fn render(
        &mut self,
        out: &mut impl Write,
        stdin: &std::io::Stdin,
    ) -> std::io::Result<()> {
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
