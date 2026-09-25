//! The native Docs view: the document on the left and the files list on the right, docked on a
//! wide view and floating on a narrow one.

use std::cell::Cell;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Bounds, Context, InteractiveElement as _, IntoElement, KeyDownEvent, MouseButton,
    ParentElement as _, Pixels, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};

use super::palette::DocsPalette;
use super::state::DocsProjectKey;
use crate::GhostexGpuiApp;
use crate::app::model::TitlebarMode;

/// CDXC:Docs 2026-09-19 DECISION:
/// User: the Docs files list is not resizable and keeps this one width, which replaced the resizable 230-560px range.
pub(crate) const SIDEBAR_WIDTH: f32 = 292.0;
/// CDXC:Docs 2026-09-06 DECISION:
/// User: below 800px of Docs viewport width, overlay the files list instead of pushing the file content; supersedes the 690px breakpoint.
pub(crate) const FLOATING_SIDEBAR_MAX_WIDTH: f32 = 800.0;

thread_local! {
    /// The Docs view's bounds as last laid out, read by the next draw to pick docked or
    /// floating and by the pointer handlers for the edge band.
    static VIEW_BOUNDS: Cell<Bounds<Pixels>> = Cell::new(Bounds::default());
    /// The corner restore button, so leaving it re-arms the edge band.
    pub(crate) static RESTORE_BOUNDS: Cell<Bounds<Pixels>> = Cell::new(Bounds::default());
}

/// The Docs view's width as last laid out; wide until the first layout.
pub(crate) fn view_width() -> f32 {
    let width = VIEW_BOUNDS.with(|cell| cell.get()).size.width;
    if width == px(0.0) {
        f32::MAX
    } else {
        f32::from(width)
    }
}

/// Whether the temporary switch that draws Docs natively is on.
///
/// CDXC:Docs 2026-09-24 DECISION:
/// User: make Docs fully GPUI like Kanban and Automate, with the files list GPUI too, and swap it in all at once when everything is finished. Until then the native view is drawn only with `GHOSTEX_NATIVE_DOCS=1` or a `native-docs` file in the Ghostex config folder (`~/.config/ghostex/`, which an app opened from Finder can see), and the Docs page stays the default; the switch goes away with the swap.
pub(crate) fn native_docs_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        if std::env::var("GHOSTEX_NATIVE_DOCS").as_deref() == Ok("1") {
            return true;
        }
        let config = std::env::var_os("XDG_CONFIG_HOME")
            .map(std::path::PathBuf::from)
            .filter(|path| path.is_absolute())
            .or_else(|| {
                std::env::var_os("HOME").map(|home| std::path::PathBuf::from(home).join(".config"))
            });
        config.is_some_and(|config| config.join("ghostex/native-docs").exists())
    })
}

impl GhostexGpuiApp {
    /// The current project's Docs identity, or `None` when this context has no project folder.
    pub(crate) fn native_docs_project(&self) -> Option<DocsProjectKey> {
        let snapshot = self.latest_sidebar_project_snapshot.as_ref()?;
        let project_id = snapshot.active_project_id.as_ref()?.0.clone();
        let project_path = snapshot.in_memory_project_path.clone()?;
        Some(DocsProjectKey {
            project_id,
            project_path,
        })
    }

    fn native_docs_palette(&mut self, window: &Window, cx: &mut Context<Self>) -> DocsPalette {
        let signature = (
            crate::app::helpers::window_glass_active_in(window),
            crate::app::helpers::CHROME_LIGHT_APPEARANCE.load(std::sync::atomic::Ordering::Relaxed),
        );
        let changed = self.native_docs.appearance_signature != Some(signature);
        if changed {
            self.native_docs.appearance_signature = Some(signature);
            self.native_docs.palette = None;
        }
        let palette = self
            .native_docs
            .palette
            .get_or_insert_with(|| DocsPalette::current(window))
            .clone();
        if changed {
            self.native_docs_restyle_live_editors(cx);
        }
        palette
    }

    /// Runs in the app's render. `None` when native Docs is off or the context has no project,
    /// which keeps the Docs page or its placeholder.
    pub(crate) fn render_native_docs(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if !native_docs_enabled() {
            return None;
        }
        let project = self.native_docs_project()?;
        self.native_docs_sync_project(&project, window, cx);
        self.native_docs_ensure_watch(cx);
        self.native_docs_materialize_editors(window, cx);
        if self.native_docs.active != self.native_docs.highlighted_path {
            self.native_docs.highlighted_path = self.native_docs.active.clone();
            self.native_docs.highlights_stale = true;
        }
        self.native_docs_refresh_highlights(cx);
        self.native_docs_sync_browser_area(cx);
        let p = self.native_docs_palette(window, cx);
        let focus = self.native_docs.focus.clone()?;

        let layout = self.native_docs_sidebar_layout();

        let probe = gpui::canvas(
            |bounds, window, _| {
                let before = VIEW_BOUNDS.with(|cell| cell.replace(bounds));
                let was_narrow = f32::from(before.size.width) < FLOATING_SIDEBAR_MAX_WIDTH;
                let narrow = f32::from(bounds.size.width) < FLOATING_SIDEBAR_MAX_WIDTH;
                if was_narrow != narrow || before.size.width == px(0.0) {
                    window.refresh();
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();

        let document = self.render_native_docs_document(&p, layout, window, cx);
        let docked_files = layout
            .docked
            .then(|| self.render_native_docs_files_list(&p, layout, false, window, cx));
        // A floating list draws in a child window of its own (`drawer.rs`).
        let view = VIEW_BOUNDS.with(|cell| cell.get());
        self.native_docs_sync_drawer(layout.overlay, view, cx);
        let restore = (!layout.visible()).then(|| self.render_native_docs_restore_button(&p, cx));
        let header_bottom = super::document_view::HEADER_BOUNDS
            .with(|cell| cell.get())
            .bottom();
        let toolbar = self.render_native_docs_selection_toolbar(&p, header_bottom, window, cx);
        let composer = self.render_native_docs_composer(&p, window, cx);
        let notes_list = self.render_native_docs_notes_list(&p, window, cx);

        Some(
            div()
                .id("native-docs")
                .track_focus(&focus)
                .key_context("NativeDocs")
                .on_action(cx.listener(Self::handle_native_docs_action))
                .on_key_down(cx.listener(Self::native_docs_key_down))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, window, cx| {
                        // A click on the document closes a drawer or a peek.
                        this.native_docs_close_transient(cx);
                        this.focus_project_editor_surface(TitlebarMode::Manage, window, cx);
                    }),
                )
                .on_mouse_move(cx.listener(|this, event: &gpui::MouseMoveEvent, _, cx| {
                    let bounds = VIEW_BOUNDS.with(|cell| cell.get());
                    let sidebar_left = this
                        .native_docs_sidebar_layout()
                        .overlay
                        .then(|| bounds.right() - px(SIDEBAR_WIDTH));
                    let over_restore = RESTORE_BOUNDS
                        .with(|cell| cell.get())
                        .contains(&event.position);
                    this.native_docs_pointer_moved(
                        event.position,
                        event.pressed_button.is_some(),
                        bounds.right(),
                        sidebar_left,
                        over_restore,
                        cx,
                    );
                }))
                .relative()
                .size_full()
                .min_w_0()
                .min_h_0()
                .flex()
                .overflow_hidden()
                .font_family(p.font.clone())
                .text_size(px(13.0))
                .text_color(p.text)
                .when(!p.glass, |this| this.bg(p.page))
                .child(probe)
                .child(div().flex_1().min_w_0().h_full().child(document))
                .children(docked_files)
                .children(restore)
                .children(toolbar)
                .children(notes_list)
                .children(composer)
                .into_any_element(),
        )
    }

    /// Cmd+S saves; Cmd+F (Ctrl+F on Windows and Linux) shows the files search.
    ///
    /// CDXC:Docs 2026-09-12 DECISION:
    /// User: Cmd+F, and Ctrl+F on Windows, show the Docs search and put the caret in it. macOS binds Ctrl+F to move the caret forward inside text fields, so claiming it there would break typing; any modifier beyond the platform's primary one means this is not the Docs find shortcut.
    pub(crate) fn native_docs_key_down(
        &mut self,
        event: &KeyDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let modifiers = event.keystroke.modifiers;
        if event.keystroke.key == "escape" && !modifiers.modified() {
            if self.native_docs_find_visible() {
                self.native_docs_hide_find(window, cx);
                cx.stop_propagation();
            } else if self.native_docs_search_focused(window, cx)
                && !self.native_docs.search_query.is_empty()
            {
                self.native_docs_clear_search(window, cx);
                cx.stop_propagation();
            } else if self.native_docs.transient.is_some() {
                self.native_docs_close_transient(cx);
                cx.stop_propagation();
            }
            return;
        }
        let primary = if cfg!(target_os = "macos") {
            modifiers.platform && !modifiers.control
        } else {
            modifiers.control && !modifiers.platform
        };
        if !primary || modifiers.alt || modifiers.shift {
            return;
        }
        match event.keystroke.key.as_str() {
            "enter"
                if self.native_docs.composer.is_none()
                    && !self.native_docs_active_notes().is_empty() =>
            {
                self.native_docs_send_notes(false, cx);
                cx.stop_propagation();
            }
            "s" => {
                self.native_docs_save_active(cx);
                cx.stop_propagation();
            }
            "f" => {
                // An open Markdown document claims the shortcut for its own Find and Replace;
                // otherwise it shows the files search.
                let markdown = self
                    .native_docs
                    .active_document()
                    .is_some_and(|document| document.live.is_some());
                if markdown && !self.native_docs_search_focused(window, cx) {
                    self.native_docs_show_find(window, cx);
                } else {
                    self.native_docs_show_search(window, cx);
                }
                cx.stop_propagation();
            }
            _ => {}
        }
    }

    /// Cmd+F without a document search: shows the list (docked or as a drawer) and focuses
    /// the search field with its text selected.
    ///
    /// CDXC:Docs 2026-09-12 DECISION:
    /// User: the find shortcut shows the Docs search and focuses it. Revealing the list follows the ordinary show path, which docks it on a wide view and opens the drawer on a narrow one.
    pub(crate) fn native_docs_show_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.native_docs_sidebar_layout().visible() || self.native_docs_sidebar_layout().narrow
        {
            self.native_docs_show_sidebar(cx);
        }
        if let Some(search) = self.native_docs.search.clone() {
            self.native_docs_focus_list_input(&search, window, cx);
        }
        self.native_docs_notify(cx);
    }

    /// The corner button shown while the list is hidden: hovering peeks, clicking shows it.
    fn render_native_docs_restore_button(
        &mut self,
        p: &DocsPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hover = p.control_hover;
        div()
            .id("native-docs-restore")
            .absolute()
            .top(px((35.0 - 27.0) / 2.0))
            .right(px(9.0))
            .w(px(32.0))
            .h(px(27.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(7.0))
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .child(crate::app::helpers::titlebar_svg_icon(
                "docs/t-layout-sidebar-right-expand-2.svg",
                16.0,
                p.toolbar_icon,
            ))
            .child(
                gpui::canvas(
                    |bounds, _, _| RESTORE_BOUNDS.with(|cell| cell.set(bounds)),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .tooltip(|window, cx| crate::app::helpers::titlebar_tooltip("Show files", window, cx))
            .on_hover(cx.listener(|this, hovered: &bool, _, cx| {
                if *hovered {
                    this.native_docs_schedule_peek(cx);
                } else {
                    this.native_docs_cancel_peek_open();
                }
            }))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _, _, cx| this.native_docs_show_sidebar(cx)))
            .into_any_element()
    }
}
