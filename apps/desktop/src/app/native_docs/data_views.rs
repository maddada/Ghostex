//! A data file's second look next to its text: the CSV/TSV table (`data_table.rs`) and the JSON
//! tree (`json_view.rs`), with their header buttons, and JSON's Format and Minify.

use std::rc::Rc;

use gpui::{
    AnyElement, App, AppContext as _, Context, Entity, IntoElement, ParentElement as _,
    StatefulInteractiveElement as _, Styled as _, Window, div,
};
use gpui_component::table::TableState;

use super::data_table::{self, DocsDataTable};
use super::files_list::{header_icon, header_tile};
use super::json_view::{self, DocsJsonTree};
use super::palette::DocsPalette;
use crate::GhostexGpuiApp;
use crate::app::helpers::titlebar_tooltip;

/// What a data file shows instead of its text, built from the editor's text when it is switched on.
pub(crate) enum DocsDataView {
    Table {
        table: Entity<TableState<DocsDataTable>>,
        notice: Option<String>,
    },
    Tree(Entity<DocsJsonTree>),
}

/// Files that open as a table rather than as text.
pub(crate) fn opens_as_table(path: &str) -> bool {
    data_table::delimiter_for(path, "").is_some()
}

fn copy_with_feedback() -> Rc<dyn Fn(String, &mut App)> {
    Rc::new(|text, cx| {
        crate::app::helpers::gpui_copy_to_clipboard(gpui::ClipboardItem::new_string(text), cx);
    })
}

impl GhostexGpuiApp {
    /// Builds the table or tree of every document whose switch is on and whose text has its
    /// editor. Runs from the render, after the editors are made.
    pub(crate) fn native_docs_build_data_views(
        &mut self,
        palette: &DocsPalette,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let wanted: Vec<(String, String)> = self
            .native_docs
            .documents
            .iter()
            .filter(|document| document.data_view_open && document.data_view.is_none())
            .filter_map(|document| {
                let editor = document.editor.as_ref()?;
                Some((
                    document.path.to_string(),
                    editor.read(cx).value().to_string(),
                ))
            })
            .collect();
        for (path, text) in wanted {
            let view =
                if let Some(delimiter) = data_table::delimiter_for(&path, &text) {
                    let read = data_table::read_delimited(&text, delimiter);
                    let notice = data_table::delimited_notice(&read);
                    DocsDataView::Table {
                        table: data_table::new_data_table(read.headers, read.rows, window, cx),
                        notice,
                    }
                } else {
                    let palette = palette.clone();
                    DocsDataView::Tree(cx.new(|cx| {
                        DocsJsonTree::new(&text, palette, copy_with_feedback(), window, cx)
                    }))
                };
            if let Some(document) = self.native_docs.document_mut(&path) {
                document.data_view = Some(view);
            }
        }
    }

    /// The header's Table / Raw or Tree / Raw switch. The view is built again from the text each
    /// time it is shown, so edits made in Raw appear in it.
    pub(crate) fn native_docs_toggle_data_view(&mut self, path: &str, cx: &mut Context<Self>) {
        let Some(document) = self.native_docs.document_mut(path) else {
            return;
        };
        document.data_view_open = !document.data_view_open;
        document.data_view = None;
        self.native_docs_notify(cx);
    }

    /// Format (`pretty`) or Minify: the editor's JSON re-spaced, as an edit Undo takes back.
    pub(crate) fn native_docs_reformat_json(
        &mut self,
        path: &str,
        pretty: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor) = self
            .native_docs
            .document(path)
            .and_then(|document| document.editor.clone())
        else {
            return;
        };
        let text = editor.read(cx).value().to_string();
        match json_view::reformat(&text, pretty) {
            Ok(formatted) if formatted != text => {
                editor.update(cx, |editor, cx| editor.replace_all(formatted, window, cx));
            }
            Ok(_) => {}
            Err(error) => self.dispatch_gpui_workspace_action_toast(
                "error",
                if pretty {
                    "Couldn't format the JSON"
                } else {
                    "Couldn't minify the JSON"
                },
                &error,
                cx,
            ),
        }
    }

    /// The active document's table or tree, when its switch is on and it is built.
    pub(crate) fn render_native_docs_data_view(
        &mut self,
        p: &DocsPalette,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let document = self.native_docs.active_document()?;
        if !document.data_view_open {
            return None;
        }
        match document.data_view.as_ref()? {
            DocsDataView::Table { table, notice } => {
                let (table, notice) = (table.clone(), notice.clone());
                Some(
                    div()
                        .size_full()
                        .flex()
                        .flex_col()
                        .children(notice.map(|notice| data_table::render_table_notice(notice, p)))
                        .child(
                            div()
                                .flex_1()
                                .min_h_0()
                                .child(data_table::render_data_table(
                                    &table,
                                    p,
                                    copy_with_feedback(),
                                    cx,
                                )),
                        )
                        .into_any_element(),
                )
            }
            DocsDataView::Tree(tree) => {
                let tree = tree.clone();
                tree.update(cx, |tree, _| tree.palette = p.clone());
                Some(div().size_full().child(tree).into_any_element())
            }
        }
    }

    /// The header buttons a data file adds: Table or Tree, Format and Minify for strict JSON,
    /// and Open in Code view for a notebook.
    pub(crate) fn render_native_docs_data_tiles(
        &mut self,
        p: &DocsPalette,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(document) = self.native_docs.active_document() else {
            return Vec::new();
        };
        if document.editor.is_none() || document.review_session_title.is_some() {
            return Vec::new();
        }
        let path = document.path.clone();
        let open = document.data_view_open;
        let mut tiles = Vec::new();
        let table = opens_as_table(&path);
        if table || json_view::has_tree(&path) {
            let (icon, tooltip, id) = match (table, open) {
                (true, true) => (
                    "titlebar/code.svg",
                    "Show raw text",
                    "native-docs-table-raw",
                ),
                (true, false) => (
                    "titlebar/table.svg",
                    "Show as table",
                    "native-docs-table-raw",
                ),
                (false, true) => (
                    "titlebar/code.svg",
                    "Show raw JSON",
                    "native-docs-json-tree",
                ),
                (false, false) => (
                    "titlebar/list-details.svg",
                    "Show as tree",
                    "native-docs-json-tree",
                ),
            };
            let toggle_path = path.clone();
            tiles.push(
                header_tile(id, header_icon(icon, false, p), false, false, p)
                    .tooltip(move |window, cx| titlebar_tooltip(tooltip, window, cx))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.native_docs_toggle_data_view(&toggle_path, cx);
                    }))
                    .into_any_element(),
            );
        }
        if json_view::is_strict_json(&path) && !open {
            for (id, icon, tooltip, pretty) in [
                (
                    "native-docs-json-format",
                    "titlebar/wand.svg",
                    "Format JSON",
                    true,
                ),
                (
                    "native-docs-json-minify",
                    "titlebar/minimize.svg",
                    "Minify JSON",
                    false,
                ),
            ] {
                let format_path = path.clone();
                tiles.push(
                    header_tile(id, header_icon(icon, false, p), false, false, p)
                        .tooltip(move |window, cx| titlebar_tooltip(tooltip, window, cx))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.native_docs_reformat_json(&format_path, pretty, window, cx);
                        }))
                        .into_any_element(),
                );
            }
        }
        // CDXC:Docs 2026-10-17 SEE-ALSO: a notebook opened from a link or Quick Access goes to the Code view (`file_open_target_for_path`, entry.rs); one opened from the Files list shows its JSON with this button.
        if super::state::file_extension(&path) == "ipynb" {
            let code_path = path.clone();
            tiles.push(
                header_tile(
                    "native-docs-notebook-code-view",
                    header_icon(crate::app::consts::TITLEBAR_ICON_CODE, false, p),
                    false,
                    false,
                    p,
                )
                .tooltip(|window, cx| titlebar_tooltip("Open in VS Code view", window, cx))
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.native_docs_open_in_code_view(&code_path, cx);
                }))
                .into_any_element(),
            );
        }
        tiles
    }
}
