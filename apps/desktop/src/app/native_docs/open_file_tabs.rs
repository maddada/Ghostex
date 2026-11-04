//! The open files as a tab row above the Files view (`docsOpenFilesAsTabs`), drawn by the side
//! panel's shared view row (`render/view_instance_tabs.rs`).

use gpui::{
    AnyElement, Context, InteractiveElement as _, IntoElement as _, ParentElement as _, Pixels,
    Point, Styled as _, Window, div,
};
use serde_json::json;

use super::actions::action;
use crate::GhostexGpuiApp;
use crate::app::context_menu::GpuiContextMenu;
use crate::app::render::view_instance_tabs::{ViewInstance, ViewInstanceRow, ViewInstanceTab};

impl GhostexGpuiApp {
    /// A tab per open file, in the order they were opened, with the shown file's tab selected, an
    /// unsaved file's dot, and the file's path as its tooltip. With no file open the row is just
    /// its `+` (Open File).
    ///
    /// CDXC:Docs 2026-10-25 WHY:
    /// The Files view's open files can show as tabs: a setting, off by default (`docsOpenFilesAsTabs`), shows them as a row above the document using the same row as VS Code's windows, and while it is on the files sidebar hides its Open Files section and keeps Project Docs. Every file Files opens (from the list, Quick Access or a chat link) is a tab, so opening another file adds a tab instead of replacing the one on screen, and each file keeps its own draft, unsaved mark and notes as before. Supersedes, while the setting is on, the 2026-09-15 vertical Open Files list in files_list.rs.
    pub(crate) fn render_native_docs_tab_row(
        &mut self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !self.docs_open_files_as_tabs {
            return None;
        }
        let state = &self.native_docs;
        let active = state.active.as_deref();
        let mut shown = None;
        let tabs = state.documents.iter().enumerate().map(|(index, document)| {
            let showing = active == Some(document.path.as_str());
            if showing {
                shown = Some(index);
            }
            ViewInstanceTab {
                instance: ViewInstance::OpenFile(document.path.clone()),
                label: document.name.clone(),
                tooltip: document.display_path.clone(),
                showing,
                closable: true,
                unsaved: document.dirty,
                icon: None,
                asleep: false,
                pinned: false,
                drag: None,
            }
        });
        let row = self.render_view_instance_tab_row(
            ViewInstanceRow::OpenFiles,
            tabs,
            &state.tab_scroll,
            cx,
        );
        // The keys the Open Files list answers (`native_docs_open_files_key_down`): arrows step
        // between the files, Delete closes the shown one, Enter goes to the document.
        let keys = self.native_docs_open_files_focus_handle(cx);
        let row = div()
            .id("native-docs-open-file-tab-keys")
            .flex_shrink_0()
            .track_focus(&keys)
            .on_key_down(cx.listener(Self::native_docs_open_files_key_down))
            .child(row)
            .into_any_element();
        // Scroll to the shown file when it changes or its tab moves (a file before it closed); the
        // row applies the request when it lays out, and otherwise keeps a scroll made by hand.
        let state = &mut self.native_docs;
        let selected = state.active.as_deref().zip(shown);
        let revealed = state
            .tab_revealed
            .as_ref()
            .map(|(path, index)| (path.as_str(), *index));
        if selected != revealed {
            if let Some((_, index)) = selected {
                state.tab_scroll.scroll_to_item(index);
            }
            state.tab_revealed = selected.map(|(path, index)| (path.to_string(), index));
        }
        Some(row)
    }

    /// A click on a file's tab shows it and gives the row the keyboard, as a click on an Open Files
    /// row does, so the arrows step on from there.
    pub(crate) fn native_docs_select_tab(
        &mut self,
        path: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.native_docs_open_files_focus_handle(cx)
            .focus(window, cx);
        self.native_docs_select(path, cx);
    }

    /// A file tab's right-click menu: Close, and Close All, which the hidden Open Files list
    /// otherwise offers.
    pub(crate) fn show_native_docs_tab_menu(
        &mut self,
        path: &str,
        position: Point<Pixels>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        GpuiContextMenu::new()
            .menu("Close", action(json!({ "type": "close", "path": path })))
            .menu("Close All", action(json!({ "type": "closeAll" })))
            .show(position, window, cx);
    }
}
