//! The drawing switcher: the Canvas tab shows the open drawing's name with a chevron, and a click
//! on the tab while Canvas is showing opens a menu of this project's drawings.
//!
//! CDXC:Canvas 2026-11-27 DECISION:
//! User, on the switcher page (Q5279 B): the switcher is in the view's tab, "the tab shows the
//! drawing's name with a chevron; clicking the tab when it's already open opens the menu", with no
//! row above the board. Q5280 B: every drawing is a row (the open one ticked), each with its own ⋯
//! holding Rename and Delete (Delete asks first), and New drawing at the bottom. Q5281 A with the
//! note "have include in git part of the menu in the switcher pls": the ⋯ also has an
//! "Include in git" check, off by default, which only edits `.ghostex/.gitignore`.

use std::cell::Cell;

use gpui::{Bounds, Context, IntoElement, Pixels, Styled as _, Window};

use super::{drawing_included_in_git, list_drawings};
use crate::app::context_menu::GpuiContextMenu;
use crate::*;

thread_local! {
    /// Where the Canvas tab was drawn, so its menu opens below it.
    pub(crate) static CANVAS_TAB_ANCHOR: Cell<Bounds<Pixels>> = Cell::new(Bounds::default());
}

/// Records the Canvas tab's bounds as it paints.
pub(crate) fn canvas_tab_anchor() -> impl IntoElement {
    gpui::canvas(
        move |bounds, _, _| CANVAS_TAB_ANCHOR.with(|cell| cell.set(bounds)),
        |_, _, _, _| {},
    )
    .absolute()
    .size_full()
}

impl GhostexGpuiApp {
    /// The switcher menu, below the Canvas tab.
    pub(crate) fn show_canvas_switcher(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(project) = self.canvas_project() else {
            return;
        };
        let open = self.canvas.open.get(&project.id).cloned();
        let app = cx.entity().downgrade();
        let mut menu = GpuiContextMenu::new();
        for drawing in list_drawings(&project) {
            let is_open = open.as_deref() == Some(drawing.file.as_str());
            let show = {
                let app = app.clone();
                let file = drawing.file.clone();
                move |_: &mut Window, cx: &mut gpui::App| {
                    let file = file.clone();
                    let _ = app.update(cx, |this, cx| this.canvas_show_drawing(file, cx));
                }
            };
            let rename = {
                let app = app.clone();
                let file = drawing.file.clone();
                move |window: &mut Window, cx: &mut gpui::App| {
                    let _ = app.update(cx, |this, cx| {
                        this.canvas_open_rename(&file, window, cx);
                    });
                }
            };
            let delete = {
                let app = app.clone();
                let file = drawing.file.clone();
                move |_: &mut Window, cx: &mut gpui::App| {
                    let _ = app.update(cx, |this, cx| this.canvas_open_delete(&file, cx));
                }
            };
            let git = {
                let app = app.clone();
                let file = drawing.file.clone();
                move |_: &mut Window, cx: &mut gpui::App| {
                    let _ = app.update(cx, |this, cx| this.canvas_toggle_git(&file, cx));
                }
            };
            let more = GpuiContextMenu::new()
                .menu_with_icon_check_callback(
                    "Rename…",
                    Some("titlebar/pencil.svg"),
                    false,
                    rename,
                )
                .menu_with_icon_check_callback("Delete…", Some("titlebar/trash.svg"), false, delete)
                .separator()
                .menu_with_icon_check_callback(
                    "Include in git",
                    Some("titlebar/git-branch.svg"),
                    drawing_included_in_git(&project, &drawing.file),
                    git,
                );
            menu = menu
                .menu_with_icon_check_callback(
                    drawing.name.clone(),
                    Some(if is_open { "titlebar/check.svg" } else { "" }),
                    false,
                    show,
                )
                .with_more(more);
        }
        let new_drawing = {
            let app = app.clone();
            move |_: &mut Window, cx: &mut gpui::App| {
                let _ = app.update(cx, |this, cx| this.canvas_new_drawing(cx));
            }
        };
        menu = menu.separator().menu_with_icon_check_callback(
            "New drawing",
            Some("titlebar/plus.svg"),
            false,
            new_drawing,
        );
        let trigger = CANVAS_TAB_ANCHOR.with(|cell| cell.get());
        menu.toggle_below(trigger, window, cx);
    }
}
