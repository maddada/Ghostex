//! Table machinery: WYSIWYG table geometry, hit-testing, editing commands, and
//! painting — split from `lib.rs`.

use super::*;

impl EditorState {
    /// The table-affordance region the pointer is in — `(table index, 0 = in the
    /// hover zone / 1 = on the below "+" strip / 2 = on the right "+" strip)`, or
    /// `None` off every table. Drives `on_mouse_move`'s repaint-on-change.
    pub(crate) fn table_hover_region_at(&self, pos: Point<Pixels>) -> Option<(usize, u8)> {
        let i = self
            .table_hover_zones
            .iter()
            .position(|(z, _)| z.contains(&pos))?;
        let strip = if self
            .table_row_add_rects
            .iter()
            .any(|(b, _)| b.contains(&pos))
        {
            1
        } else if self
            .table_col_add_rects
            .iter()
            .any(|(b, _, _)| b.contains(&pos))
        {
            2
        } else {
            0
        };
        Some((i, strip))
    }

    /// The table cell `(row, col)` the pointer is over, or `None` off any table —
    /// The table's content LEFT edge in window space, horizontal scroll
    /// applied — THE single source for every x mapping on a table (paint,
    /// hit-tests, caret, selection, affordances).
    pub(crate) fn table_left(&self, t: &TableRow, row: usize, bounds: &Bounds<Pixels>) -> Pixels {
        let total: Pixels = t.col_widths.iter().copied().sum();
        let sx = self.table_sx(
            table_header_row(t, row),
            total,
            bounds.size.width - px(TABLE_GUTTER),
        );
        table_left_x(bounds.origin.x, bounds.size.width, total, sx, t.rtl)
    }

    /// The clamped horizontal scroll of the table headed at `header_row`.
    pub(crate) fn table_sx(&self, header_row: usize, total: Pixels, avail: Pixels) -> Pixels {
        let max = f32::from((total - avail).max(px(0.)));
        px(self
            .table_scroll_x
            .get(&header_row)
            .copied()
            .unwrap_or(0.)
            .clamp(0., max))
    }

    /// Horizontal wheel/trackpad over a wide table scrolls IT, not the page —
    /// vertical deltas fall through to the outer scroll container untouched.
    pub(crate) fn on_scroll_wheel(
        &mut self,
        event: &gpui::ScrollWheelEvent,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Vertical scrolling is the overwhelmingly common case — bail before
        // the O(lines) row walk when there's no horizontal delta.
        let dx = f32::from(event.delta.pixel_delta(px(20.)).x);
        if std::env::var_os("ZORITE_WHEEL_DEBUG").is_some() {
            eprintln!("wheel: delta={:?} dx={dx}", event.delta);
        }
        if dx == 0. {
            return;
        }
        let dbg = std::env::var_os("ZORITE_WHEEL_DEBUG").is_some();
        let Some(bounds) = self.last_bounds else {
            return;
        };
        // Hit-test the PAINTED table zone (grid + margin, from the last
        // paint), not the text row model — table paint runs taller than its
        // logical rows (top gutter, cell padding), which left the lower rows
        // of a tall table unresponsive to the wheel.
        let Some(&(_, header)) = self
            .table_hover_zones
            .iter()
            .find(|(z, _)| z.contains(&event.position))
        else {
            if dbg {
                eprintln!("wheel: not over a table zone");
            }
            return;
        };
        // A horizontally-dominant wheel over a table belongs to the table —
        // consume it even at the scroll ends, so leftover deltas don't spill
        // into the page. Vertical-dominant scrolling keeps feeding the page.
        if dx.abs() >= f32::from(event.delta.pixel_delta(px(20.)).y).abs() {
            cx.stop_propagation();
        }
        let Some(t) = self.table_rows.get(header).and_then(Option::as_ref) else {
            return;
        };
        if t.col_widths.is_empty() {
            return;
        }
        let total: Pixels = t.col_widths.iter().copied().sum();
        let avail = bounds.size.width - px(TABLE_GUTTER);
        if total <= avail {
            if dbg {
                eprintln!("wheel: fits total={total:?} avail={avail:?}");
            }
            return;
        }
        let max = f32::from(total - avail);
        let cur = self.table_scroll_x.get(&header).copied().unwrap_or(0.);
        // An RTL table's scroll offset runs the other way (see `table_left_x`),
        // so the same physical swipe moves its content the same physical way.
        let new = if t.rtl { cur + dx } else { cur - dx }.clamp(0., max);
        if new != cur {
            self.table_scroll_x.insert(header, new);
            cx.notify();
        }
    }

    /// drives the delete-handle repaint + reveal.
    pub(crate) fn hovered_table_cell(&self, pos: Point<Pixels>) -> Option<(usize, usize)> {
        let bounds = self.last_bounds.as_ref()?;
        // Hover bands start at the left gutter and extend a header's band up into the
        // top gutter, so moving onto a delete handle keeps its cell "hovered".
        let gutter_left = bounds.left();
        if pos.x < gutter_left {
            return None;
        }
        let rel_y = pos.y - bounds.top();
        let g = px(TABLE_GUTTER);
        let row = (0..self.wrapped.len()).find(|&i| {
            let Some(t) = self.table_rows.get(i).and_then(Option::as_ref) else {
                return false;
            };
            if t.is_separator {
                return false;
            }
            let h = self.line_h(i) * self.row_span(i) as f32;
            let lo = if t.is_header {
                self.line_tops[i] - g
            } else {
                self.line_tops[i]
            };
            rel_y >= lo && rel_y < self.line_tops[i] + h
        })?;
        let t = self.table_rows.get(row).and_then(Option::as_ref)?;
        if t.col_widths.is_empty() {
            return None;
        }
        let table_left = self.table_left(t, row, bounds);
        let table_w: Pixels = t.col_widths.iter().copied().sum();
        if pos.x >= table_left + table_w {
            return None;
        }
        let rel_x = (pos.x - table_left).max(px(0.));
        let mut colx = px(0.);
        for (col, &cw) in t.col_widths.iter().enumerate() {
            if rel_x < colx + cw {
                return Some((row, col));
            }
            colx += cw;
        }
        Some((row, t.col_widths.len() - 1))
    }

    /// Hit-test the hovered row's border "+" → the row a new row lands after.
    pub(crate) fn table_add_row_at(&self, position: Point<Pixels>) -> Option<usize> {
        self.table_row_add_rects
            .iter()
            .find_map(|&(rect, row)| rect.contains(&position).then_some(row))
    }

    /// Hit-test the hovered column's border "+" → `(header row, column)` — a new
    /// column lands right of it.
    pub(crate) fn table_add_col_at(&self, position: Point<Pixels>) -> Option<(usize, usize)> {
        self.table_col_add_rects
            .iter()
            .find_map(|&(rect, row, col)| rect.contains(&position).then_some((row, col)))
    }

    /// Source offset at the start of `cell`'s content in table `row` (last paint).
    pub(crate) fn cell_start_offset(&self, row: usize, cell: usize) -> Option<usize> {
        let t = self.table_rows.get(row).and_then(Option::as_ref)?;
        Some(self.line_starts()[row] + t.cell_ranges.get(cell)?.start)
    }

    /// If `position` lands on a table grid row (not the separator), the source
    /// byte offset of the closest cell-content position — so a click puts the
    /// caret inside the cell rather than in the raw `| … |` source. `None`
    /// otherwise (the caller falls back to [`Self::index_for_mouse_position`]).
    pub(crate) fn table_offset_at(
        &self,
        position: Point<Pixels>,
        window: &mut Window,
    ) -> Option<usize> {
        if self.wrapped.is_empty() || self.table_rows.iter().all(Option::is_none) {
            return None;
        }
        let bounds = self.last_bounds.as_ref()?;
        let rel = point(
            position.x - bounds.left() - px(TABLE_GUTTER),
            position.y - bounds.top(),
        );
        let mut row = self.wrapped.len() - 1;
        for i in 0..self.wrapped.len() {
            let h = self.line_h(i) * self.row_span(i) as f32;
            if rel.y < self.line_tops[i] + h {
                row = i;
                break;
            }
        }
        let t = self.table_rows.get(row).and_then(Option::as_ref)?;
        if t.is_separator || t.col_widths.is_empty() {
            return None;
        }
        // A scrolled wide table: the pointer maps to content shifted left.
        let rel = point(position.x - self.table_left(t, row, bounds), rel.y);
        // Measure at the last paint's font + size — the style stack is unwound
        // during event dispatch, so `text_style()` here reports the root style.
        let font = self
            .paint_font
            .clone()
            .unwrap_or_else(|| window.text_style().font());
        let font_size = self.font_size;
        let pad = px(TABLE_CELL_PAD);
        // Column the click is in, and its left x — through the same mirrored
        // mapping the cells were painted with, or a click lands in a different
        // cell than the one under the pointer (on an RTL table the columns run
        // the other way, so accumulating left-to-right picks the mirror image).
        let total: Pixels = t.col_widths.iter().copied().sum();
        // Columns tile the table exactly, so a clamped x always lands in one.
        let hit_x = rel.x.max(px(0.)).min(total - px(0.01));
        let mut cell = t.col_widths.len() - 1;
        for c in 0..t.col_widths.len() {
            let x = col_offset(&t.col_widths, t.cells.len(), c, t.rtl);
            if hit_x >= x && hit_x < x + cell_span_width(&t.col_widths, t.cells.len(), c) {
                cell = c;
                break;
            }
        }
        // A click in a spanned trailing column lands in the short row's last cell.
        let cell = cell.min(t.cells.len().saturating_sub(1));
        let cx = col_offset(&t.col_widths, t.cells.len(), cell, t.rtl);
        let content = t.cells.get(cell)?;
        let cw = cell_span_width(&t.col_widths, t.cells.len(), cell);
        let cf = cell_font(&font, t.is_header);
        let full_w = measure_width(window, content, &cf, font_size);
        let avail = (cw - pad * 2.).max(px(8.));
        // Alignment shifts only unwrapped content (a wrapped cell fills its width).
        let align_off = if full_w > avail {
            px(0.)
        } else {
            match t.aligns.get(cell) {
                Some(markdown_syntax::Align::Center) => (avail - full_w).max(px(0.)) / 2.,
                Some(markdown_syntax::Align::Right) => (avail - full_w).max(px(0.)),
                // Unaligned cells follow the table's direction — same rule the
                // paint uses, so the hit-test matches what's on screen.
                _ if t.rtl => (avail - full_w).max(px(0.)),
                _ => px(0.),
            }
        };
        // In-cell y: from the row's top, minus the cell's constant 6px top pad
        // (`table_row_h` = line height + 12, text centered → 6 above).
        let pad_y = px(6.);
        let target = point(
            (rel.x - cx - pad - align_off).max(px(0.)),
            rel.y - self.line_tops[row] - pad_y,
        );
        let in_cell = cell_offset_for_point(content, target, avail, &cf, font_size, window);
        Some(self.line_starts()[row] + t.cell_ranges.get(cell)?.start + in_cell)
    }

    /// Whether the caret is currently inside an editable table cell (not the
    /// separator) — so Tab navigates cells instead of indenting.
    pub(crate) fn caret_in_table(&self) -> bool {
        let (row, _) = self.row_col(self.cursor_offset());
        self.table_rows
            .get(row)
            .and_then(Option::as_ref)
            .is_some_and(|t| !t.is_separator)
    }

    /// Cell-aware vertical caret move inside a table: keep the same column (cell +
    /// the offset within that cell) on the adjacent row, skipping the `|---|`
    /// separator. `None` when the caret isn't in a table cell, or the move would
    /// leave the table — the caller then does a normal vertical move (exiting it).
    pub(crate) fn table_move_vertical(&self, dir: i32) -> Option<usize> {
        let (row, col) = self.row_col(self.cursor_offset());
        let t = self.table_rows.get(row).and_then(Option::as_ref)?;
        if t.is_separator || t.cell_ranges.is_empty() {
            return None;
        }
        let cell = table_cell_at(t, col);
        let intra = col.saturating_sub(t.cell_ranges[cell].start);
        let starts = self.line_starts();
        let mut r = row as isize + dir as isize;
        loop {
            if r < 0 {
                return None;
            }
            let ru = r as usize;
            match self.table_rows.get(ru) {
                Some(Some(nt)) if !nt.is_separator && !nt.cell_ranges.is_empty() => {
                    let tc = cell.min(nt.cell_ranges.len() - 1);
                    let cr = &nt.cell_ranges[tc];
                    return Some(starts[ru] + cr.start + intra.min(cr.end - cr.start));
                }
                Some(Some(_)) => r += dir as isize, // separator — skip past it
                // A non-table row next to the table: exit onto it at the same byte
                // column (clamped to a char boundary). Done here rather than via
                // `move_vertical`, whose handling of the table's top gutter would
                // otherwise trap an upward exit back onto the header row.
                Some(None) => {
                    let end = self.line_end(ru);
                    // Skip the table's own `<!-- table:STYLE -->` style-marker line
                    // (a hidden directive) so an upward exit lands on real content,
                    // the way a downward move already skips its zero-height row.
                    if markdown_syntax::table_style_marker(&self.content[starts[ru]..end]).is_some()
                    {
                        r += dir as isize;
                        continue;
                    }
                    let mut target = starts[ru] + col.min(end - starts[ru]);
                    while !self.content.is_char_boundary(target) {
                        target -= 1;
                    }
                    return Some(target);
                }
                None => return None, // past the document edge — let move_vertical exit
            }
        }
    }

    /// Cell-aware horizontal caret move inside a table: step a character within the
    /// cell, hopping to the adjacent cell (the next/previous row's edge cell at a
    /// row boundary) so the caret never has to cross the hidden `|`/padding.
    /// `None` when the caret isn't in a table cell or the move would leave it.
    pub(crate) fn table_move_horizontal(&self, dir: i32) -> Option<usize> {
        let (row, col) = self.row_col(self.cursor_offset());
        let t = self.table_rows.get(row).and_then(Option::as_ref)?;
        if t.is_separator || t.cell_ranges.is_empty() {
            return None;
        }
        let cell = table_cell_at(t, col);
        let starts = self.line_starts();
        let cur = self.cursor_offset();
        let cell_start = starts[row] + t.cell_ranges[cell].start;
        let cell_end = starts[row] + t.cell_ranges[cell].end;
        if dir > 0 {
            if cur < cell_end {
                return Some(self.next_boundary(cur).min(cell_end));
            }
            if cell + 1 < t.cell_ranges.len() {
                return Some(starts[row] + t.cell_ranges[cell + 1].start);
            }
            // Last cell of the row → first cell of the next table row, else exit.
            for (r, slot) in self.table_rows.iter().enumerate().skip(row + 1) {
                match slot.as_ref() {
                    Some(nt) if !nt.is_separator && !nt.cell_ranges.is_empty() => {
                        return Some(starts[r] + nt.cell_ranges[0].start);
                    }
                    Some(_) => continue,
                    None => break,
                }
            }
            None
        } else {
            if cur > cell_start {
                return Some(self.previous_boundary(cur).max(cell_start));
            }
            if cell > 0 {
                return Some(starts[row] + t.cell_ranges[cell - 1].end);
            }
            // First cell of the row → last cell of the previous table row, else exit.
            for (r, slot) in self.table_rows.iter().enumerate().take(row).rev() {
                match slot.as_ref() {
                    Some(pt) if !pt.is_separator && !pt.cell_ranges.is_empty() => {
                        return Some(starts[r] + pt.cell_ranges[pt.cell_ranges.len() - 1].end);
                    }
                    Some(_) => continue,
                    None => break,
                }
            }
            None
        }
    }

    /// Target source offset to move the caret to the next (`forward`) / previous
    /// table cell, crossing rows (skipping the separator). Stays put at the table's
    /// final/first cell. `None` when the caret isn't in a table cell.
    pub(crate) fn table_cell_nav(&self, forward: bool) -> Option<usize> {
        let (row, col) = self.row_col(self.cursor_offset());
        let t = self.table_rows.get(row).and_then(Option::as_ref)?;
        if t.is_separator {
            return None;
        }
        let cell = table_cell_at(t, col);
        let starts = self.line_starts();
        let usable = |tr: &TableRow| !tr.is_separator && !tr.cell_ranges.is_empty();
        if forward {
            if cell + 1 < t.cell_ranges.len() {
                return Some(starts[row] + t.cell_ranges[cell + 1].start);
            }
            for (r, slot) in self.table_rows.iter().enumerate().skip(row + 1) {
                match slot.as_ref() {
                    Some(nt) if usable(nt) => return Some(starts[r] + nt.cell_ranges[0].start),
                    Some(_) => continue,
                    None => break,
                }
            }
        } else {
            if cell > 0 {
                return Some(starts[row] + t.cell_ranges[cell - 1].start);
            }
            for (r, slot) in self.table_rows.iter().enumerate().take(row).rev() {
                match slot.as_ref() {
                    Some(pt) if usable(pt) => {
                        return Some(starts[r] + pt.cell_ranges[pt.cell_ranges.len() - 1].start);
                    }
                    Some(_) => continue,
                    None => break,
                }
            }
        }
        Some(self.cursor_offset()) // at the boundary — no-op move (don't indent)
    }

    /// The alignment of the table column the caret sits in — but only while the
    /// caret is in the table's HEADER row (the toolbar lives there; alignment is a
    /// per-column property, set once from the header). `None` otherwise. Read from
    /// the current content, since the painted `table_rows` lag a frame right after
    /// a separator rewrite (which would highlight the just-changed-from button).
    pub fn caret_table_align(&self) -> Option<CellAlign> {
        let (row, col) = self.row_col(self.cursor_offset());
        // Fast-reject via the paint: only a header row gets the toolbar.
        let t = self.table_rows.get(row).and_then(Option::as_ref)?;
        if !t.is_header {
            return None;
        }
        let cell = table_cell_at(t, col);
        let scan = self.scan_data();
        let region = scan.tables.iter().find(|r| r.lines.contains(&row))?;
        Some(match region.aligns.get(cell) {
            Some(markdown_syntax::Align::Center) => CellAlign::Center,
            Some(markdown_syntax::Align::Right) => CellAlign::Right,
            _ => CellAlign::Left,
        })
    }

    /// Set the alignment of the caret's table column by rewriting that table's
    /// `|---|` separator row; the caret stays put. No-op outside a table cell.
    pub fn set_caret_table_align(&mut self, align: CellAlign, cx: &mut Context<Self>) {
        let (row, col) = self.row_col(self.cursor_offset());
        let Some(t) = self.table_rows.get(row).and_then(Option::as_ref) else {
            return;
        };
        if t.is_separator {
            return;
        }
        let cell = table_cell_at(t, col);
        // Read the table's columns from the current content (fresh), so repeated
        // clicks build on the latest alignment, not a stale painted snapshot.
        let scan = self.scan_data();
        let Some(region) = scan.tables.iter().find(|r| r.lines.contains(&row)) else {
            return;
        };
        let mut aligns = region.aligns.clone();
        if cell >= aligns.len() {
            return;
        }
        aligns[cell] = match align {
            CellAlign::Left => markdown_syntax::Align::Left,
            CellAlign::Center => markdown_syntax::Align::Center,
            CellAlign::Right => markdown_syntax::Align::Right,
        };
        let sep_row = region.lines.start + 1;
        let mut new_sep = String::from("|");
        for a in &aligns {
            new_sep.push_str(match a {
                markdown_syntax::Align::Left => " :-- |",
                markdown_syntax::Align::Center => " :-: |",
                markdown_syntax::Align::Right => " --: |",
            });
        }
        let starts = self.line_starts();
        let sep_start = starts[sep_row];
        let sep_end = starts
            .get(sep_row + 1)
            .map_or(self.content.len(), |&s| s - 1);
        let old_caret = self.cursor_offset();
        let range = sep_start..sep_end;
        self.record_edit(&range, &new_sep);
        self.content = self.content[..sep_start].to_owned() + &new_sep + &self.content[sep_end..];
        let delta = new_sep.len() as isize - (sep_end - sep_start) as isize;
        let caret = if old_caret >= sep_end {
            (old_caret as isize + delta).max(0) as usize
        } else {
            old_caret
        };
        self.selected_range = caret..caret;
        self.remap_diagnostics(&range, new_sep.len());
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }

    /// The caret's table block as `(header_line, separator_line, end_exclusive,
    /// columns)`, or `None` outside a table.
    fn caret_table_block(&self) -> Option<(usize, usize, usize, usize)> {
        let (row, _) = self.row_col(self.cursor_offset());
        let scan = self.scan_data();
        let region = scan.tables.iter().find(|r| r.lines.contains(&row))?;
        Some((
            region.lines.start,
            region.lines.start + 1,
            region.lines.end,
            region.aligns.len().max(1),
        ))
    }

    /// Insert an empty row above/below the caret's row (Word-style); the caret
    /// moves into the new row's first cell. No-op outside a table.
    pub fn insert_table_row(&mut self, below: bool, cx: &mut Context<Self>) {
        let (row, _) = self.row_col(self.cursor_offset());
        let Some((header, sep, _end, cols)) = self.caret_table_block() else {
            return;
        };
        // From the header a new row always lands below the separator (the first
        // body row); above/below a body row is literal.
        let after = if row == header {
            sep
        } else if below {
            row
        } else {
            (row - 1).max(sep)
        };
        let new_row = format!("\n|{}", "  |".repeat(cols));
        let pos = self.line_end(after);
        let range = pos..pos;
        self.record_edit(&range, &new_row);
        self.content = self.content[..pos].to_owned() + &new_row + &self.content[pos..];
        self.remap_diagnostics(&range, new_row.len());
        self.selected_range = (pos + 3)..(pos + 3); // first cell, after "\n| "
        self.table_menu = None;
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }

    /// Delete the caret's table row (body rows only — the header + separator stay).
    /// The caret keeps its cell + in-cell offset, landing on the row that takes the
    /// deleted row's place. No-op outside a table.
    pub fn delete_table_row(&mut self, cx: &mut Context<Self>) {
        let Some((row, cell, in_cell)) = self.caret_table_cell_pos() else {
            return;
        };
        let Some((header, sep, end, _cols)) = self.caret_table_block() else {
            return;
        };
        if row == header || row == sep {
            return;
        }
        let start = self.line_starts()[row];
        let line_end = self.line_end(row);
        // Remove the line + its trailing newline; for the last line, eat the
        // preceding newline instead so no blank line is left behind.
        let (del_start, del_end) = if line_end < self.content.len() {
            (start, line_end + 1)
        } else {
            (start.saturating_sub(1), line_end)
        };
        let range = del_start..del_end;
        self.record_edit(&range, "");
        self.content = self.content[..del_start].to_owned() + &self.content[del_end..];
        self.remap_diagnostics(&range, 0);
        // Stay at the same cell/offset, on the row now at this position (shifted
        // up), or the header if no body rows remain.
        let target = if end <= sep + 2 {
            header
        } else {
            row.min(end - 2)
        };
        let caret = self.caret_pos_for_cell(target, cell, in_cell);
        self.selected_range = caret..caret;
        self.table_menu = None;
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }

    /// Delete the whole table the caret is in — its grid lines plus an optional
    /// `<!-- table:STYLE -->` marker line directly above — joining the surrounding
    /// text. The caret lands where the table was.
    pub fn delete_table(&mut self, cx: &mut Context<Self>) {
        let Some((header, _sep, end, _cols)) = self.caret_table_block() else {
            return;
        };
        let starts = self.line_starts();
        let mut first = header;
        if first > 0
            && markdown_syntax::table_style_marker(
                &self.content[starts[first - 1]..starts[first] - 1],
            )
            .is_some()
        {
            first -= 1;
        }
        let line_end_last = self.line_end(end - 1);
        // Remove the table's lines + the trailing newline; at the document end, eat
        // the preceding newline instead so no blank line is left behind.
        let (del_start, del_end) = if line_end_last < self.content.len() {
            (starts[first], line_end_last + 1)
        } else {
            (starts[first].saturating_sub(1), line_end_last)
        };
        let range = del_start..del_end;
        self.record_edit(&range, "");
        self.content = self.content[..del_start].to_owned() + &self.content[del_end..];
        self.remap_diagnostics(&range, 0);
        let caret = del_start.min(self.content.len());
        self.selected_range = caret..caret;
        self.selection_reversed = false;
        self.goal_x = None;
        self.table_menu = None;
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }

    /// The caret row's table region, when inside one.
    pub(crate) fn caret_table_region(&self) -> Option<markdown_syntax::TableRegion> {
        let (row, _) = self.row_col(self.cursor_offset());
        self.scan_data()
            .tables
            .iter()
            .find(|r| r.lines.contains(&row))
            .cloned()
    }

    /// Duplicate the caret's row below itself (the header duplicates as the
    /// first body row). The caret lands in the copy, same cell + offset.
    pub fn duplicate_table_row(&mut self, cx: &mut Context<Self>) {
        let Some((row, cell, in_cell)) = self.caret_table_cell_pos() else {
            return;
        };
        let Some((_header, sep, _end, _cols)) = self.caret_table_block() else {
            return;
        };
        if row == sep {
            return;
        }
        let starts = self.line_starts();
        let line = self.content[starts[row]..self.line_end(row)].to_string();
        // A duplicated header lands below the separator (as a body row).
        let after = if row + 1 == sep { sep } else { row };
        let insert = format!("\n{line}");
        let pos = self.line_end(after);
        let range = pos..pos;
        self.record_edit(&range, &insert);
        self.content = self.content[..pos].to_owned() + &insert + &self.content[pos..];
        self.remap_diagnostics(&range, insert.len());
        let caret = self.caret_pos_for_cell(after + 1, cell, in_cell);
        self.selected_range = caret..caret;
        self.table_menu = None;
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }

    /// Copy the caret's table — its grid source plus any `<!-- table:STYLE -->`
    /// marker — to the clipboard (markdown, pasteable anywhere).
    pub fn copy_table(&mut self, cx: &mut Context<Self>) {
        let Some(region) = self.caret_table_region() else {
            return;
        };
        let starts = self.line_starts();
        let first = region.marker_line.unwrap_or(region.lines.start);
        let Some(&start) = starts.get(first) else {
            return;
        };
        let end = self.line_end(region.lines.end - 1);
        let text = self.content[start..end].to_string();
        self.table_menu = None;
        self.write_clipboard(text, cx);
        cx.notify();
    }

    /// Set the caret table's visual style by rewriting its
    /// `<!-- table:STYLE -->` marker line: `Some(name)` writes/replaces the
    /// marker, `None` (Grid, the default) removes it. One undo step.
    /// Rewrite (or insert/remove) the marker line above `region` to say
    /// `marker` (`None` = no marker needed). The caret keeps its cell. One
    /// undo step; shared by the style menu and drag-to-resize.
    fn rewrite_table_marker(
        &mut self,
        region: &markdown_syntax::TableRegion,
        marker: Option<String>,
        cx: &mut Context<Self>,
    ) {
        let starts = self.line_starts();
        let (range, new) = match (region.marker_line, marker) {
            // Marker present → replace its line, or remove it (with the newline).
            (Some(m), Some(text)) => (starts[m]..self.line_end(m), text),
            (Some(m), None) => (starts[m]..self.line_end(m) + 1, String::new()),
            // No marker → insert one above the header.
            (None, Some(text)) => (
                starts[region.lines.start]..starts[region.lines.start],
                format!("{text}\n"),
            ),
            (None, None) => return,
        };
        let cell_pos = self.caret_table_cell_pos();
        let old_caret = self.selected_range.start;
        // The marker edit adds/removes one line ABOVE the table (or replaces in
        // place) — shift the caret's row index to keep it in its cell.
        let row_shift: isize = match (region.marker_line, new.is_empty()) {
            (Some(_), true) => -1,
            (None, false) => 1,
            _ => 0,
        };
        self.record_edit(&range, &new);
        self.content.replace_range(range.clone(), &new);
        self.remap_diagnostics(&range, new.len());
        let caret = match cell_pos {
            Some((row, cell, in_cell)) => {
                let same_row = (row as isize + row_shift).max(0) as usize;
                self.caret_pos_for_cell(same_row, cell, in_cell)
            }
            // Caret wasn't in this table (e.g. a border drag from elsewhere):
            // keep it where it was, shifted by the edit's byte delta — seating
            // it "in a cell" would land it on the marker line and reveal it.
            None => {
                let delta = new.len() as isize - (range.end - range.start) as isize;
                if old_caret >= range.end {
                    (old_caret as isize + delta).max(0) as usize
                } else if old_caret > range.start {
                    range.start + new.len()
                } else {
                    old_caret
                }
            }
        };
        let caret = caret.min(self.content.len());
        self.selected_range = caret..caret;
        self.table_menu = None;
        cx.emit(EditorEvent::Changed);
        cx.notify();
    }

    /// Set the caret table's visual style (`None` = Grid, the default),
    /// preserving any drag-resized `cols=` widths in the marker.
    pub fn set_table_style(&mut self, name: Option<&'static str>, cx: &mut Context<Self>) {
        let Some(region) = self.caret_table_region() else {
            return;
        };
        let style = zorite_markdown::syntax::TableStyle::from_name(name.unwrap_or("grid"))
            .unwrap_or_default();
        let marker =
            zorite_markdown::syntax::table_marker_text(style, region.col_widths_attr.as_deref());
        self.rewrite_table_marker(&region, marker, cx);
    }

    /// Persist a finished column-border drag: every column's current display
    /// width goes into the marker's `cols=` list (style preserved).
    /// Double-click on a column border: auto-fit the column to its widest
    /// content (Excel's AutoFit). Measures the region with explicit widths
    /// stripped, then persists the natural width through the same marker path
    /// as a drag release.
    pub(crate) fn autofit_table_col(
        &mut self,
        header_row: usize,
        col: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(mut region) = self
            .scan_data()
            .tables
            .iter()
            .find(|r| r.lines.start == header_row)
            .cloned()
        else {
            return;
        };
        region.col_widths_attr = None; // natural, content-measured widths
        let content = self.content.clone();
        let lines: Vec<&str> = content.split('\n').collect();
        // Measure with the PAINT font — the shaping that lays the table out
        // uses it, and the root font event dispatch reports has different
        // metrics (autofit widths were off by the family drift).
        let font = self
            .paint_font
            .clone()
            .unwrap_or_else(|| window.text_style().font());
        let natural = table_column_widths(
            &lines,
            &region,
            window,
            &font,
            self.font_size,
            Hsla::default(),
            None,
        );
        let Some(&w) = natural.get(col) else {
            return;
        };
        // Ceil: the marker serializes widths as whole px (`w.round()`), and a
        // fraction-of-a-pixel shortfall wraps the widest cell's last word.
        self.commit_table_col_widths(
            TableColResize {
                header_row,
                col,
                start_x: px(0.),
                orig: 0.,
                width: f32::from(w).ceil(),
            },
            cx,
        );
        cx.notify();
    }

    pub(crate) fn commit_table_col_widths(
        &mut self,
        resize: TableColResize,
        cx: &mut Context<Self>,
    ) {
        let Some(region) = self
            .scan_data()
            .tables
            .iter()
            .find(|r| r.lines.start == resize.header_row)
            .cloned()
        else {
            return;
        };
        // The header row's committed display widths already carry the live drag.
        let Some(t) = self
            .table_rows
            .get(resize.header_row)
            .and_then(Option::as_ref)
        else {
            return;
        };
        let mut widths: Vec<f32> = t.col_widths.iter().map(|w| f32::from(*w)).collect();
        if let Some(w) = widths.get_mut(resize.col) {
            *w = resize.width.max(24.);
        }
        let marker = zorite_markdown::syntax::table_marker_text(region.style, Some(&widths));
        self.rewrite_table_marker(&region, marker, cx);
    }

    /// The caret's table position as `(row, cell_index, offset_within_cell)`, or
    /// `None` outside a table. Lets structural edits keep the caret put.
    pub(crate) fn caret_table_cell_pos(&self) -> Option<(usize, usize, usize)> {
        let (row, _) = self.row_col(self.cursor_offset());
        self.caret_table_block()?;
        let starts = self.line_starts();
        let row_start = starts[row];
        let line = &self.content[row_start..self.line_end(row)];
        let line_col = self.cursor_offset() - row_start;
        let ranges = markdown_syntax::table_cell_ranges(line);
        let cell = ranges
            .iter()
            .position(|r| line_col <= r.end)
            .unwrap_or(ranges.len().saturating_sub(1));
        let in_cell = ranges
            .get(cell)
            .map_or(0, |r| line_col.saturating_sub(r.start).min(r.len()));
        Some((row, cell, in_cell))
    }

    /// Byte offset of `(row, cell, offset_within_cell)` in the current content,
    /// clamping the cell + offset to what that row actually has.
    pub(crate) fn caret_pos_for_cell(&self, row: usize, cell: usize, in_cell: usize) -> usize {
        let starts = self.line_starts();
        let Some(&row_start) = starts.get(row) else {
            return self.content.len();
        };
        let line = &self.content[row_start..self.line_end(row)];
        let ranges = markdown_syntax::table_cell_ranges(line);
        if ranges.is_empty() {
            return row_start;
        }
        let r = &ranges[cell.min(ranges.len() - 1)];
        // Keep the caret strictly inside the cell, before its closing pipe — an
        // empty cell's trimmed range collapses onto that pipe (the line end for the
        // last cell), which would drop the caret out of the rendered table.
        let bytes = line.as_bytes();
        let close = (r.end..bytes.len())
            .find(|&i| bytes[i] == b'|')
            .unwrap_or(bytes.len());
        row_start + (r.start + in_cell).min(close.saturating_sub(1))
    }

    /// Insert an empty column left/right of the caret's column (a cell added to
    /// every row; the separator gets a default-left marker). The caret stays in its
    /// cell. No-op outside a table.
    pub fn insert_table_column(&mut self, right: bool, cx: &mut Context<Self>) {
        let Some((row, cell, in_cell)) = self.caret_table_cell_pos() else {
            return;
        };
        let at = if right { cell + 1 } else { cell };
        if self.rewrite_table_columns(ColEdit::Insert(at)) {
            // Inserting to the left shifts the caret's cell one column right.
            let new_cell = if right { cell } else { cell + 1 };
            let caret = self.caret_pos_for_cell(row, new_cell, in_cell);
            self.selected_range = caret..caret;
            self.table_menu = None;
            cx.emit(EditorEvent::Changed);
            cx.notify();
        }
    }

    /// Delete the caret's column from every row; the caret stays near where the
    /// column was. No-op outside a table, or on the last remaining column.
    pub fn delete_table_column(&mut self, cx: &mut Context<Self>) {
        let Some((row, cell, in_cell)) = self.caret_table_cell_pos() else {
            return;
        };
        if self.rewrite_table_columns(ColEdit::Delete(cell)) {
            let caret = self.caret_pos_for_cell(row, cell, in_cell);
            self.selected_range = caret..caret;
            self.table_menu = None;
            cx.emit(EditorEvent::Changed);
            cx.notify();
        }
    }

    /// Rewrite every row of the caret's table to insert/delete a cell, normalizing
    /// cell spacing. Returns `false` (no edit) outside a table or when a delete
    /// would remove the last column; the caller restores the caret.
    fn rewrite_table_columns(&mut self, edit: ColEdit) -> bool {
        let Some((header, sep, end, _cols)) = self.caret_table_block() else {
            return false;
        };
        let lines: Vec<&str> = self.content.split('\n').collect();
        let mut new_rows: Vec<String> = Vec::with_capacity(end - header);
        for (i, &line) in lines[header..end].iter().enumerate() {
            let is_sep = header + i == sep;
            let mut cells: Vec<String> = markdown_syntax::table_cells(line)
                .iter()
                .map(|s| (*s).to_string())
                .collect();
            match edit {
                ColEdit::Insert(at) => cells.insert(
                    at.min(cells.len()),
                    if is_sep { "---".into() } else { String::new() },
                ),
                ColEdit::Delete(c) => {
                    if cells.len() <= 1 || c >= cells.len() {
                        return false; // never delete the last column
                    }
                    cells.remove(c);
                }
            }
            new_rows.push(format!("| {} |", cells.join(" | ")));
        }
        let starts = self.line_starts();
        let block_start = starts[header];
        let block_end = self.line_end(end - 1);
        let new_block = new_rows.join("\n");
        let range = block_start..block_end;
        self.record_edit(&range, &new_block);
        self.content =
            self.content[..block_start].to_owned() + &new_block + &self.content[block_end..];
        self.remap_diagnostics(&range, new_block.len());
        true
    }
}

/// Content-fit column widths for a table region (W4c): each column sized to its
/// widest cell (header measured bold) + padding, with a minimum. A table wider
/// than the viewport is NOT scaled down — it scrolls horizontally in place.
#[allow(clippy::too_many_arguments)]
pub(crate) fn table_column_widths(
    lines: &[&str],
    region: &markdown_syntax::TableRegion,
    window: &mut Window,
    base_font: &Font,
    font_size: Pixels,
    color: Hsla,
    col_resize: Option<TableColResize>,
) -> Vec<Pixels> {
    // Reader parity: a row with MORE cells than the header widens the grid —
    // extra columns render (the short rows' last cells span the remainder)
    // instead of hiding the excess.
    let cols = region
        .lines
        .clone()
        .filter(|&li| li != region.lines.start + 1)
        .map(|li| markdown_syntax::table_cells(lines[li]).len())
        .max()
        .unwrap_or(0)
        .max(region.aligns.len())
        .max(1);
    let pad = px(TABLE_CELL_PAD);
    let mut widths = vec![px(0.); cols];
    for li in region.lines.clone() {
        if li == region.lines.start + 1 {
            continue; // skip the |---| separator
        }
        let header = li == region.lines.start;
        for (c, cell) in markdown_syntax::table_cells(lines[li])
            .iter()
            .enumerate()
            .take(cols)
        {
            if cell.is_empty() {
                continue;
            }
            let mut font = base_font.clone();
            if header {
                font.weight = gpui::FontWeight::BOLD;
            }
            let run = TextRun {
                len: cell.len(),
                font,
                color,
                background_color: None,
                underline: None,
                strikethrough: None,
            };
            let w = window
                .text_system()
                .shape_line(
                    SharedString::from(cell.to_string()),
                    font_size,
                    &[run],
                    None,
                )
                .width();
            widths[c] = widths[c].max(w + pad * 2. + px(2.)); // Local change: slack so rounding never wraps a cell that fits.
        }
    }
    for w in &mut widths {
        *w = (*w).max(px(48.));
    }
    // Explicit widths — the marker's `cols=` list (drag-to-resize persisted),
    // then the live drag — override the measurement (floored so a column can't
    // vanish). Content-measured columns keep the 48px floor above.
    if let Some(attr) = &region.col_widths_attr {
        for (c, w) in attr.iter().enumerate().take(cols) {
            widths[c] = px(w.max(24.));
        }
    }
    if let Some(r) = col_resize.filter(|r| r.header_row == region.lines.start)
        && r.col < cols
    {
        widths[r.col] = px(r.width.max(24.));
    }
    // No scale-to-fit: a table wider than the viewport keeps its natural
    // columns and scrolls horizontally in place (Cditor-style) — see
    // `EditorState::table_scroll_x`.
    widths
}

/// Horizontal inset (px) of a table cell's text from its column's left edge.
pub(crate) const TABLE_CELL_PAD: f32 = 10.;
/// Left indent for tables, so per-row delete "−" handles sit in a gutter beside the
/// grid instead of over the first cell (issue #16).
pub(crate) const TABLE_GUTTER: f32 = 22.;

/// The font a table cell is rendered with — bold in the header row.
fn cell_font(font: &Font, is_header: bool) -> Font {
    let mut f = font.clone();
    if is_header {
        f.weight = gpui::FontWeight::BOLD;
    }
    f
}

/// Shape a table cell's `content` into a single (unwrapped) line, for exact
/// caret / hit-test geometry that matches the kerned glyphs `paint_table_row`
/// renders (measuring prefixes in isolation drifts by their kerning).
fn shape_cell(
    window: &mut Window,
    content: &str,
    font: &Font,
    font_size: Pixels,
    wrap: Option<Pixels>,
    color: Hsla,
) -> Option<WrappedLine> {
    let run = TextRun {
        len: content.len(),
        font: font.clone(),
        color,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let runs: &[TextRun] = if content.is_empty() {
        &[]
    } else {
        std::slice::from_ref(&run)
    };
    window
        .text_system()
        .shape_text(
            SharedString::from(content.to_string()),
            font_size,
            runs,
            wrap,
            None,
        )
        .ok()?
        .into_iter()
        .next()
}

/// A wide table's scroll thumb — its visual rect, (padded) grab rect, and
/// the mapping from thumb travel to content scroll. Built in prepaint (with
/// a hand-cursor hitbox), drawn in paint, committed for drag hit-tests.
#[derive(Clone, Copy)]
pub(crate) struct TableThumb {
    pub(crate) rect: Bounds<Pixels>,
    pub(crate) grab: Bounds<Pixels>,
    /// Header row — the table's `table_scroll_x` key.
    pub(crate) header: usize,
    /// Content px scrolled per px of thumb travel.
    pub(crate) factor: f32,
    pub(crate) color: Hsla,
}

/// The table content's LEFT x, given the note column's `origin`/`width`, the
/// table's `total` width and its already-clamped horizontal scroll `sx`.
///
/// LTR sits `TABLE_GUTTER` in from the left edge (the row handles live in that
/// gutter) and scrolls leftward. An RTL table mirrors it (#66): the grid hugs
/// the RIGHT edge with the gutter on the other side — the reader's
/// `.mr(22).ml_auto()` — and its scroll reveals content to the left. Either
/// way the visible band is `TABLE_GUTTER` narrower than the note column, so
/// [`EditorState::table_sx`]'s clamp is direction-agnostic.
///
/// The single source for every x on a table: paint, hit-tests, the caret,
/// the resize bands and the add/delete affordances all derive from it, so
/// they cannot drift apart.
pub(crate) fn table_left_x(
    origin: Pixels,
    width: Pixels,
    total: Pixels,
    sx: Pixels,
    rtl: bool,
) -> Pixels {
    if rtl {
        origin + width - px(TABLE_GUTTER) - total + sx
    } else {
        origin + px(TABLE_GUTTER) - sx
    }
}

/// The x band a table is visible in: the note column less its gutter, on the
/// side [`table_left_x`] put the gutter. Clamps the scroll thumb's track and a
/// wide table's selection highlight.
pub(crate) fn table_visible_band(origin: Pixels, width: Pixels, rtl: bool) -> (Pixels, Pixels) {
    if rtl {
        (origin, origin + width - px(TABLE_GUTTER))
    } else {
        (origin + px(TABLE_GUTTER), origin + width)
    }
}

/// The header row of the table that `t` (the grid row at line `row`) belongs
/// to — the key of its horizontal-scroll entry.
fn table_header_row(t: &TableRow, row: usize) -> usize {
    match t.body_index {
        Some(b) => row.saturating_sub(b + 2),
        None if t.is_separator => row.saturating_sub(1),
        None => row,
    }
}

/// The width available to cell `c` of a row with `cells_len` cells: a short
/// row's LAST cell spans the remaining columns (reader parity — e.g. a
/// two-cell header over a wider body), otherwise its own column.
/// Distance from the table's LEFT edge to column `c`'s left edge.
///
/// Mirrored for an RTL table, so column 0 sits at the right — matching the
/// reader's `flex_row_reverse`. Every place that turns a column into an x goes
/// through this: paint, caret, selection bands, resize grips. They have to
/// agree, or clicking lands in a different cell than the one drawn there.
pub(crate) fn col_offset(col_widths: &[Pixels], cells_len: usize, c: usize, rtl: bool) -> Pixels {
    let before: Pixels = col_widths[..c.min(col_widths.len())].iter().copied().sum();
    if !rtl {
        return before;
    }
    let total: Pixels = col_widths.iter().copied().sum();
    total - before - cell_span_width(col_widths, cells_len, c)
}

pub(crate) fn cell_span_width(col_widths: &[Pixels], cells_len: usize, c: usize) -> Pixels {
    if c + 1 == cells_len && cells_len < col_widths.len() {
        col_widths[c..].iter().copied().sum()
    } else {
        col_widths.get(c).copied().unwrap_or(px(0.))
    }
}

pub(crate) fn table_row_wrap_rows(
    window: &mut Window,
    cells: &[SharedString],
    col_widths: &[Pixels],
    is_header: bool,
    font: &Font,
    font_size: Pixels,
) -> usize {
    let pad = px(TABLE_CELL_PAD);
    let cf = cell_font(font, is_header);
    let mut rows = 1;
    for (c, cell) in cells.iter().enumerate() {
        if cell.is_empty() {
            continue;
        }
        let cw = cell_span_width(col_widths, cells.len(), c);
        if cw == px(0.) {
            continue;
        }
        let avail = (cw - pad * 2.).max(px(8.));
        if let Some(wl) = shape_cell(window, cell, &cf, font_size, Some(avail), Hsla::default()) {
            rows = rows.max(wl.wrap_boundaries().len() + 1);
        }
    }
    rows
}

/// The cell a source column `col` (line-local) falls in for a table row — clamped
/// to the nearest cell when `col` is in a pipe/space between cells.
fn table_cell_at(t: &TableRow, col: usize) -> usize {
    t.cell_ranges
        .iter()
        .position(|r| col <= r.end)
        .unwrap_or(t.cell_ranges.len().saturating_sub(1))
}

/// Screen position of the caret at source column `col` (line-local) inside a
/// table row's rendered cells: `(x, cell_index, in_cell_offset)`. Mirrors
/// `paint_table_row`'s layout (cumulative column widths + pad + alignment).
pub(crate) fn table_caret_pos(
    t: &TableRow,
    col: usize,
    left: Pixels,
    font: &Font,
    font_size: Pixels,
    window: &mut Window,
) -> Option<(Pixels, Pixels, usize, usize)> {
    if t.cell_ranges.is_empty() {
        return None;
    }
    let pad = px(TABLE_CELL_PAD);
    let cell = table_cell_at(t, col);
    let range = t.cell_ranges.get(cell)?;
    let content = t.cells.get(cell)?;
    let in_cell = col.saturating_sub(range.start).min(content.len());
    let cell_x = left + col_offset(&t.col_widths, t.cell_ranges.len(), cell, t.rtl);
    let cw = cell_span_width(&t.col_widths, t.cell_ranges.len(), cell);
    // The header is bold, so shape with the bold font or the caret lands left of
    // the (wider) bold glyphs; position_for_index gives the exact kerned x — and,
    // shaped at the cell's width, the wrap row's y for drag-narrowed columns.
    let cf = cell_font(font, t.is_header);
    let line_h = font_size * LINE_HEIGHT_RATIO;
    let avail = (cw - pad * 2.).max(px(8.));
    let wl = shape_cell(
        window,
        content,
        &cf,
        font_size,
        Some(avail),
        Hsla::default(),
    )?;
    let mut pos = wl.position_for_index(in_cell, line_h).unwrap_or_default();
    let wrapped = !wl.wrap_boundaries().is_empty();
    // A cell holding right-to-left text needs the same index↔x map the rest of
    // the editor uses: gpui's `position_for_index` collapses every offset in an
    // RTL run onto one x, which is a caret that won't move through the word.
    // Only an unwrapped cell can use it — a map's x's run along the single
    // pre-wrap line (same limit as elsewhere).
    if !wrapped && zorite_markdown::syntax::contains_rtl(content) {
        let map = gpui_bidi::shaped::map_of_wrapped(&wl, content.len());
        pos.x = px(map.x_for_index(in_cell));
    }
    let full_w = wl.width();
    // Alignment shifts only unwrapped content (a wrapped cell fills its width).
    let align_off = if wrapped {
        px(0.)
    } else {
        match t.aligns.get(cell) {
            Some(markdown_syntax::Align::Center) => (avail - full_w).max(px(0.)) / 2.,
            Some(markdown_syntax::Align::Right) => (avail - full_w).max(px(0.)),
            _ => px(0.),
        }
    };
    Some((cell_x + pad + align_off + pos.x, pos.y, cell, in_cell))
}

/// The byte offset within `content` whose rendered x (from the text's left edge)
/// is closest to `target` — hit-tests a click inside a table cell, using the
/// shaped line so it matches the kerned glyphs.
fn cell_offset_for_point(
    content: &str,
    target: Point<Pixels>,
    wrap: Pixels,
    font: &Font,
    font_size: Pixels,
    window: &mut Window,
) -> usize {
    let line_h = font_size * LINE_HEIGHT_RATIO;
    let Some(wl) = shape_cell(
        window,
        content,
        font,
        font_size,
        Some(wrap),
        Hsla::default(),
    ) else {
        return 0;
    };
    if wl.wrap_boundaries().is_empty() && zorite_markdown::syntax::contains_rtl(content) {
        // Same reason as the caret above, in reverse.
        let map = gpui_bidi::shaped::map_of_wrapped(&wl, content.len());
        return map.index_for_x(f32::from(target.x));
    }
    match wl.closest_index_for_position(
        point(
            target.x,
            target.y.max(px(0.)).min(wl.size(line_h).height - px(1.)),
        ),
        line_h,
    ) {
        Ok(i) | Err(i) => i,
    }
}

/// Paint a table row as a grid (W4c): a top border (+ bottom on the last row),
/// a left border per column + a right outer border, and each cell's text aligned
/// within its (content-fit) column. A separator row is a single horizontal rule.
#[allow(clippy::too_many_arguments)]
pub(crate) fn paint_table_row(
    t: &TableRow,
    origin: Point<Pixels>,
    row_h: Pixels,
    font: &Font,
    font_size: Pixels,
    line_h: Pixels,
    color: Hsla,
    window: &mut Window,
    cx: &mut App,
) {
    use markdown_syntax::TableStyle;
    let thick = px(1.);
    // The collapsed `|---|` separator draws nothing — the outer box + the next
    // row's top divider form the header/body border.
    if t.is_separator {
        return;
    }
    let style = t.style;
    let vlines = matches!(style, TableStyle::Grid);
    // A single rule under the header (Striped/Minimal) vs a divider under every
    // row (Grid) vs none (Header).
    let header_rule = matches!(style, TableStyle::Striped | TableStyle::Minimal);
    let table_w = t.col_widths.iter().sum();
    // Row shading (painted first, behind everything): the header for the Header
    // style; alternate body rows for Striped.
    let shaded = match style {
        TableStyle::Header => t.is_header,
        TableStyle::Striped => t.body_index.is_some_and(|b| b % 2 == 1),
        _ => false,
    };
    if shaded {
        window.paint_quad(fill(Bounds::new(origin, size(table_w, row_h)), t.shade));
    }
    // Horizontal divider at the row's top: under every row (Grid, header excepted —
    // the box covers it), or just under the header (Striped/Minimal: the first body
    // row's top), or never (Header).
    let top_divider = if matches!(style, TableStyle::Grid) {
        !t.is_header
    } else {
        header_rule && t.body_index == Some(0)
    };
    if top_divider {
        window.paint_quad(fill(Bounds::new(origin, size(table_w, thick)), t.border));
    }
    let pad = px(TABLE_CELL_PAD);
    let mut cell_font = font.clone();
    if t.is_header {
        cell_font.weight = gpui::FontWeight::BOLD;
    }
    for (c, &cw) in t.col_widths.iter().enumerate() {
        let x = origin.x + col_offset(&t.col_widths, t.cells.len(), c, t.rtl);
        // Inner cell separator at the left of every cell except the first (Grid
        // only; the other styles drop vertical lines). A short row draws no
        // dividers past its last cell — that cell spans the remaining columns.
        if vlines && c > 0 && c < t.cells.len() {
            // The divider goes on the column's LEADING edge — its right on an
            // RTL table, where reading starts.
            let dx = if t.rtl { x + cw - thick } else { x };
            window.paint_quad(fill(
                Bounds::new(point(dx, origin.y), size(thick, row_h)),
                t.border,
            ));
        }
        if let Some(cell) = t.cells.get(c).filter(|s| !s.is_empty()) {
            // Shaped at the cell's width so a drag-narrowed column word-wraps
            // (the row height already reserves the wrap rows). Top-anchored at
            // the single-line pad, not centered — wraps grow downward. A short
            // row's last cell spans the remaining columns.
            let cw = cell_span_width(&t.col_widths, t.cells.len(), c);
            let avail = (cw - pad * 2.).max(px(8.));
            if let Some(shaped) =
                shape_cell(window, cell, &cell_font, font_size, Some(avail), color)
            {
                let align = match t.aligns.get(c) {
                    Some(markdown_syntax::Align::Center) => gpui::TextAlign::Center,
                    Some(markdown_syntax::Align::Right) => gpui::TextAlign::Right,
                    // No explicit alignment: follow the table's direction, so
                    // RTL cells start at the right like their text does.
                    _ if t.rtl => gpui::TextAlign::Right,
                    _ => gpui::TextAlign::Left,
                };
                let _ = shaped.paint(
                    point(x + pad, origin.y + px(6.)),
                    line_h,
                    align,
                    Some(Bounds::new(
                        point(x + pad, origin.y + px(6.)),
                        size(avail, (row_h - px(12.)).max(line_h)),
                    )),
                    window,
                    cx,
                );
            }
        }
    }
}

/// Paint a table add-row / add-column affordance: a thin strip with a centered
/// "+". Subtle by default; on hover a faint fill + a brighter glyph.
/// Paint a row/column delete handle: a small rounded button with a "−". Filled on
/// hover, a muted glyph otherwise.
/// An accent border (no fill) around a hovered row/column or the caret's cell
/// (issue #16, Cditor-style).
pub(crate) fn paint_table_outline(bounds: Bounds<Pixels>, accent: Hsla, window: &mut Window) {
    window.paint_quad(PaintQuad {
        bounds,
        corner_radii: Corners::all(px(2.)),
        background: hsla(0., 0., 0., 0.).into(),
        border_widths: Edges::all(px(1.5)),
        border_color: accent,
        border_style: BorderStyle::Solid,
    });
}

/// The hovered row/column's border pill: an accent-filled capsule holding "+"
/// and "−" halves (white glyphs; the hovered half full-strength). `sep_h` =
/// true when the two halves sit side-by-side (a column's top-border pill).
pub(crate) fn paint_table_pill(
    a: &TableAffordance,
    sep_h: bool,
    mouse: Point<Pixels>,
    window: &mut Window,
) {
    let pill = a.plus.union(&a.minus);
    window.paint_quad(fill(pill, a.accent).corner_radii(Corners::all(px(6.))));
    let white = hsla(0., 0., 1., 1.);
    let arm = px(4.);
    let th = px(1.5);
    let glyph = |b: &Bounds<Pixels>, plus: bool, window: &mut Window| {
        let mut c = white;
        c.a = if b.contains(&mouse) { 1.0 } else { 0.75 };
        let cx = b.origin.x + b.size.width / 2.;
        let cy = b.origin.y + b.size.height / 2.;
        window.paint_quad(fill(
            Bounds::new(point(cx - arm, cy - th / 2.), size(arm * 2., th)),
            c,
        ));
        if plus {
            window.paint_quad(fill(
                Bounds::new(point(cx - th / 2., cy - arm), size(th, arm * 2.)),
                c,
            ));
        }
    };
    glyph(&a.plus, true, window);
    glyph(&a.minus, false, window);
    // A hairline between the halves so the two targets read separately.
    let mut div_c = white;
    div_c.a = 0.35;
    if sep_h {
        window.paint_quad(fill(
            Bounds::new(
                point(a.minus.origin.x, pill.origin.y + px(3.)),
                size(px(1.), pill.size.height - px(6.)),
            ),
            div_c,
        ));
    } else {
        window.paint_quad(fill(
            Bounds::new(
                point(pill.origin.x + px(3.), a.minus.origin.y),
                size(pill.size.width - px(6.), px(1.)),
            ),
            div_c,
        ));
    }
}

/// An in-progress table column-border drag (issue #16): identifies the column
/// by its table's header line + index, and carries the live width the shaping
/// applies each frame; release writes it into the marker's `cols=` list.
#[derive(Clone, Copy)]
pub(crate) struct TableColResize {
    pub(crate) header_row: usize,
    pub(crate) col: usize,
    pub(crate) start_x: Pixels,
    pub(crate) orig: f32,
    pub(crate) width: f32,
}

/// A column-resize grip: a slim band over one column's right border in the
/// hovered table (issue #16). Dragging it resizes that column live.
pub(crate) struct ColResizeGrip {
    pub(crate) band: Bounds<Pixels>,
    pub(crate) hit: Hitbox,
    pub(crate) header_row: usize,
    pub(crate) col: usize,
    pub(crate) width: f32,
    /// The border's x + the table's vertical extent, for painting the accent
    /// line while hovered/dragged.
    pub(crate) x: Pixels,
    pub(crate) top: Pixels,
    pub(crate) bottom: Pixels,
    pub(crate) accent: Hsla,
}

/// The hovered row's / column's affordance (issue #16, Cditor-style): an accent
/// OUTLINE around it (no fill) and a small pill sitting ON the table border —
/// "+" (insert after) and "−" (delete) — instead of outside strips/handles.
pub(crate) struct TableAffordance {
    /// The whole row's / column's rect, outlined in the accent color.
    pub(crate) outline: Bounds<Pixels>,
    pub(crate) plus: Bounds<Pixels>,
    pub(crate) minus: Bounds<Pixels>,
    pub(crate) plus_hit: Hitbox,
    pub(crate) minus_hit: Hitbox,
    /// The hovered body row (rows) / the table's header line (columns) — where
    /// the caret seats to run the caret-driven insert/delete APIs.
    pub(crate) row: usize,
    pub(crate) col: usize,
    pub(crate) accent: Hsla,
}
