//! CSV and TSV files as a read-only table, and the table itself, which other Files formats can
//! fill with their own columns and rows.
//!
//! CDXC:Docs 2026-10-17 DECISION:
//! User (Q219): CSV and TSV open in the Files view as a "native table": a read-only table with a Table / Raw switch in the header (sticky header row, resizable columns, sort by clicking a column, select cells and copy them with Cmd+C). Raw is the ordinary code editor and stays editable; the table reads the editor's text each time it is shown. A file with more rows than `MAX_ROWS` shows the first ones and says so. It is gpui-component's `DataTable`, which draws only the rows on screen.

use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, InteractiveElement as _, IntoElement,
    ParentElement as _, Pixels, SharedString, Styled as _, Window, div, px,
};
use gpui_component::Sizable as _;
use gpui_component::table::{
    Column, ColumnSort, DataTable, TableDelegate, TableSelection, TableState,
};

use super::palette::DocsPalette;

/// The most rows the table reads from a file.
pub(crate) const MAX_ROWS: usize = 50_000;

/// The field separator for a delimited-text file, or `None` when Files shows it as text only.
/// A `.csv` whose first line has more semicolons than commas (a European spreadsheet export) is
/// read with semicolons.
pub(crate) fn delimiter_for(path: &str, text: &str) -> Option<u8> {
    match super::state::file_extension(path).as_str() {
        "tsv" | "tab" => Some(b'\t'),
        "csv" => {
            let first = text.lines().next().unwrap_or_default();
            let (mut commas, mut semicolons, mut quoted) = (0, 0, false);
            for c in first.chars() {
                match c {
                    '"' => quoted = !quoted,
                    ',' if !quoted => commas += 1,
                    ';' if !quoted => semicolons += 1,
                    _ => {}
                }
            }
            Some(if semicolons > commas { b';' } else { b',' })
        }
        _ => None,
    }
}

/// A delimited file read for the table: the first record is the header row.
pub(crate) struct DelimitedTable {
    pub(crate) headers: Vec<String>,
    pub(crate) rows: Vec<Vec<String>>,
    /// Every data row in the file, including the ones past `MAX_ROWS`.
    pub(crate) total_rows: usize,
    /// The first record the reader could not read, when it stopped early.
    pub(crate) error: Option<String>,
}

pub(crate) fn read_delimited(text: &str, delimiter: u8) -> DelimitedTable {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(delimiter)
        .has_headers(false)
        .flexible(true)
        .from_reader(text.as_bytes());
    let mut headers = Vec::new();
    let mut rows = Vec::new();
    let mut total_rows = 0;
    let mut error = None;
    for (ix, record) in reader.records().enumerate() {
        let record = match record {
            Ok(record) => record,
            Err(err) => {
                error = Some(err.to_string());
                break;
            }
        };
        if ix == 0 {
            headers = record.iter().map(str::to_string).collect();
            continue;
        }
        total_rows += 1;
        if rows.len() < MAX_ROWS {
            rows.push(record.iter().map(str::to_string).collect());
        }
    }
    DelimitedTable {
        headers,
        rows,
        total_rows,
        error,
    }
}

/// The table's columns and rows, in the order the sort put them.
pub(crate) struct DocsDataTable {
    headers: Vec<SharedString>,
    rows: Vec<Vec<SharedString>>,
    /// Display position to row.
    order: Vec<usize>,
    widths: Vec<Pixels>,
    /// Set by the Files view before each draw.
    pub(crate) text: gpui::Hsla,
}

impl DocsDataTable {
    pub(crate) fn new(headers: Vec<String>, rows: Vec<Vec<String>>) -> Self {
        let mut table = Self {
            headers: Vec::new(),
            rows: Vec::new(),
            order: Vec::new(),
            widths: Vec::new(),
            text: gpui::black(),
        };
        table.set_rows(headers, rows);
        table
    }

    /// Replaces the columns and rows (call `TableState::refresh` afterwards). Short rows read as
    /// empty cells, and a row longer than the header row adds unnamed columns.
    pub(crate) fn set_rows(&mut self, headers: Vec<String>, rows: Vec<Vec<String>>) {
        let columns = rows
            .iter()
            .map(Vec::len)
            .chain([headers.len()])
            .max()
            .unwrap_or(0);
        self.headers = (0..columns)
            .map(|ix| match headers.get(ix).map(|name| name.trim()) {
                Some(name) if !name.is_empty() => SharedString::from(name.to_string()),
                _ => SharedString::from(format!("Column {}", ix + 1)),
            })
            .collect();
        // Wide enough for the header and the longest of the first rows, within reason.
        self.widths = (0..columns)
            .map(|ix| {
                let longest = rows
                    .iter()
                    .take(200)
                    .filter_map(|row| row.get(ix))
                    .map(|cell| cell.chars().count())
                    .chain([self.headers[ix].chars().count() + 2])
                    .max()
                    .unwrap_or(0);
                px((longest as f32 * 7.6 + 34.0).clamp(64.0, 380.0))
            })
            .collect();
        self.rows = rows
            .into_iter()
            .map(|row| row.into_iter().map(SharedString::from).collect())
            .collect();
        self.order = (0..self.rows.len()).collect();
    }

    fn cell(&self, row_ix: usize, col_ix: usize) -> SharedString {
        self.order
            .get(row_ix)
            .and_then(|row| self.rows.get(*row))
            .and_then(|row| row.get(col_ix))
            .cloned()
            .unwrap_or_default()
    }

    /// The selection as text: one cell as it reads, a row or a column as tab-separated values.
    pub(crate) fn selection_text(&self, selection: TableSelection) -> Option<String> {
        match selection {
            TableSelection::None => None,
            TableSelection::Cell(row, col) => Some(self.cell(row, col).to_string()),
            TableSelection::Row(row) => Some(
                (0..self.headers.len())
                    .map(|col| self.cell(row, col).to_string())
                    .collect::<Vec<_>>()
                    .join("\t"),
            ),
            TableSelection::Column(col) => Some(
                std::iter::once(
                    self.headers
                        .get(col)
                        .cloned()
                        .unwrap_or_default()
                        .to_string(),
                )
                .chain((0..self.order.len()).map(|row| self.cell(row, col).to_string()))
                .collect::<Vec<_>>()
                .join("\n"),
            ),
        }
    }
}

/// Numbers sort as numbers when both cells are numbers, everything else by its text; empty
/// cells go last.
fn compare_cells(a: &str, b: &str) -> std::cmp::Ordering {
    let (a, b) = (a.trim(), b.trim());
    match (a.is_empty(), b.is_empty()) {
        (true, true) => return std::cmp::Ordering::Equal,
        (true, false) => return std::cmp::Ordering::Greater,
        (false, true) => return std::cmp::Ordering::Less,
        _ => {}
    }
    match (a.parse::<f64>(), b.parse::<f64>()) {
        (Ok(x), Ok(y)) => x.partial_cmp(&y).unwrap_or(std::cmp::Ordering::Equal),
        _ => a.to_lowercase().cmp(&b.to_lowercase()),
    }
}

impl TableDelegate for DocsDataTable {
    fn columns_count(&self, _: &App) -> usize {
        self.headers.len()
    }

    fn rows_count(&self, _: &App) -> usize {
        self.order.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        Column::new(
            SharedString::from(col_ix.to_string()),
            self.headers[col_ix].clone(),
        )
        .width(self.widths[col_ix])
        .min_width(px(40.0))
        .movable(false)
        .sortable()
    }

    fn perform_sort(
        &mut self,
        col_ix: usize,
        sort: ColumnSort,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) {
        let mut order: Vec<usize> = (0..self.rows.len()).collect();
        if sort != ColumnSort::Default {
            let rows = &self.rows;
            let text = |row: usize| {
                rows[row]
                    .get(col_ix)
                    .map(|cell| cell.as_ref())
                    .unwrap_or("")
            };
            // A stable sort, so equal cells keep the file's order.
            order.sort_by(|a, b| {
                let ordering = compare_cells(text(*a), text(*b));
                if sort == ColumnSort::Descending {
                    ordering.reverse()
                } else {
                    ordering
                }
            });
        }
        self.order = order;
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _: &mut Window,
        _: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        div()
            .w_full()
            .overflow_hidden()
            .whitespace_nowrap()
            .text_ellipsis()
            .text_color(self.text)
            .child(self.cell(row_ix, col_ix))
    }

    fn cell_text(&self, row_ix: usize, col_ix: usize, _: &App) -> String {
        self.cell(row_ix, col_ix).to_string()
    }
}

/// A table for `headers` and `rows`, with cells selectable and columns sortable and resizable.
pub(crate) fn new_data_table(
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    window: &mut Window,
    cx: &mut App,
) -> Entity<TableState<DocsDataTable>> {
    cx.new(|cx| {
        TableState::new(DocsDataTable::new(headers, rows), window, cx)
            .cell_selectable(true)
            .col_movable(false)
    })
}

/// The table, filling its parent. Cmd+C (Ctrl+C) copies what is selected through `copy`.
pub(crate) fn render_data_table(
    table: &Entity<TableState<DocsDataTable>>,
    p: &DocsPalette,
    copy: std::rc::Rc<dyn Fn(String, &mut App)>,
    cx: &mut App,
) -> AnyElement {
    table.update(cx, |state, _| state.delegate_mut().text = p.text);
    let source = table.clone();
    div()
        .size_full()
        .font_family(p.mono_font.clone())
        .text_size(px(12.0))
        .on_key_down(move |event, _, cx| {
            let keystroke = &event.keystroke;
            if keystroke.key == "c"
                && keystroke.modifiers.secondary(cx)
                && !keystroke.modifiers.shift
            {
                let state = source.read(cx);
                if let Some(text) = state.delegate().selection_text(state.selection()) {
                    cx.stop_propagation();
                    copy(text, cx);
                }
            }
        })
        .child(DataTable::new(table).bordered(false).stripe(false).small())
        .into_any_element()
}

/// The line above a table cut short, or above a file the reader stopped in.
pub(crate) fn render_table_notice(text: String, p: &DocsPalette) -> AnyElement {
    div()
        .flex_none()
        .px(px(12.0))
        .py(px(6.0))
        .border_b_1()
        .border_color(p.border)
        .text_size(px(12.0))
        .text_color(p.muted)
        .child(text)
        .into_any_element()
}

/// The notice for a read CSV or TSV file, when it needs one.
pub(crate) fn delimited_notice(table: &DelimitedTable) -> Option<String> {
    if let Some(error) = &table.error {
        return Some(format!(
            "Only the rows before a malformed line are shown ({error}). Raw shows the whole file."
        ));
    }
    (table.total_rows > table.rows.len()).then(|| {
        format!(
            "Showing the first {} of {} rows. Raw shows the whole file.",
            group_digits(table.rows.len()),
            group_digits(table.total_rows)
        )
    })
}

fn group_digits(count: usize) -> String {
    let digits = count.to_string();
    let mut out = String::new();
    for (ix, c) in digits.chars().enumerate() {
        if ix > 0 && (digits.len() - ix).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// The Kit theme's table colours, set to the Files look: a see-through body (so window glass
/// shows), and the header, rules, hover and selection as washes of the text ink.
///
/// CDXC:Docs 2026-10-17 WHY: gpui-component's `DataTable` paints itself from the theme's table tokens and has no way to take colours per table; the Files table is the only `DataTable` in the app, so `apply_gpui_component_theme` sets the tokens here each time the theme is applied.
pub(crate) fn apply_table_theme(theme: &mut gpui_component::Theme, light: bool) {
    let ink = |alpha: f32| -> gpui::Hsla {
        gpui::Hsla::from(gpui::rgb(if light { 0x000000 } else { 0xffffff })).opacity(alpha)
    };
    let selection: gpui::Hsla = gpui::rgb(0x3b82f6).into();
    for (token, color) in [
        (&mut theme.tokens.table, gpui::transparent_black()),
        (&mut theme.tokens.table_even, ink(0.02)),
        (&mut theme.tokens.table_head, ink(0.04)),
        (&mut theme.tokens.table_head_foreground, ink(0.66)),
        (&mut theme.tokens.table_hover, ink(0.045)),
        (&mut theme.tokens.table_row_border, ink(0.08)),
        (&mut theme.tokens.table_active, selection.opacity(0.16)),
        (
            &mut theme.tokens.table_active_border,
            selection.opacity(0.7),
        ),
    ] {
        *token = color.into();
    }
    theme.table_head_foreground = ink(0.66);
}
