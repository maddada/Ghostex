//! The native Docs view: the document on the left and the files list on the right, docked on a
//! wide view and floating on a narrow one.

use std::cell::Cell;

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Bounds, Context, InteractiveElement as _, IntoElement, KeyDownEvent, MouseButton,
    ParentElement as _, Pixels, StatefulInteractiveElement as _, Styled as _, Window, div, px,
};

use super::palette::DocsPalette;
use super::picture_zoom::DocsPictureHost as _;
use super::state::DocsProjectKey;
use crate::GhostexGpuiApp;
use crate::app::model::TitlebarMode;
use crate::app::native_chat::zoom::ZoomStep;

/// CDXC:Docs 2026-09-25 DECISION:
/// User: "please make the set width for it 260px". The Docs files list is not resizable and keeps this one width (it was 292px since the 2026-09-19 decision that replaced the resizable 230-560px range).
pub(crate) const SIDEBAR_WIDTH: f32 = 260.0;
/// CDXC:Docs 2026-09-06 DECISION:
/// User: below 800px of Docs viewport width, overlay the files list instead of pushing the file content; supersedes the 690px breakpoint.
pub(crate) const FLOATING_SIDEBAR_MAX_WIDTH: f32 = 800.0;

thread_local! {
    /// The Docs view's bounds as last laid out, read by the next draw to pick docked or
    /// floating and by the pointer handler that holds a peek open.
    static VIEW_BOUNDS: Cell<Bounds<Pixels>> = Cell::new(Bounds::default());
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

impl DocsPalette {
    /// The palette for the window's glass and the chat's resolved appearance.
    pub(crate) fn current(window: &Window) -> Self {
        let chat =
            crate::app::native_chat::appearance::ChatAppearance::current(&serde_json::Value::Null);
        Self::resolve(
            crate::app::helpers::window_glass_active_in(window),
            chat.light,
            chat.font.clone(),
            chat.background,
        )
    }
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

    /// Something the palette is drawn from changed in settings (the theme colour, a custom tint,
    /// Colourfulness, the chat theme or font): the next draw rebuilds it and restyles the live
    /// editors, as a light/dark or glass change does.
    pub(crate) fn native_docs_notify_appearance(&mut self, cx: &mut Context<Self>) {
        self.native_docs.appearance_signature = None;
        self.native_docs_notify(cx);
    }

    /// Runs in the app's render. `None` when the context has no project, which keeps the
    /// placeholder.
    ///
    /// CDXC:Docs 2026-09-29 DECISION:
    /// User: the native Files view is the main version; HTML files and Excalidraw drawings stay on CEF (the browser area). Supersedes the 2026-09-24 decision that kept the React Docs page as the default behind the `GHOSTEX_NATIVE_DOCS` / `~/.config/ghostex/native-docs` switch; the switch is gone.
    pub(crate) fn render_native_docs(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let project = self.native_docs_project()?;
        self.native_docs_sync_project(&project, window, cx);
        if std::mem::take(&mut self.native_docs.open_file_prompt) {
            self.native_docs_show_search(window, cx);
        }
        self.native_docs_ensure_watch(cx);
        self.native_docs_materialize_editors(window, cx);
        self.native_docs_apply_search_reveal(window, cx);
        if self.native_docs.active != self.native_docs.highlighted_path {
            self.native_docs.highlighted_path = self.native_docs.active.clone();
            self.native_docs.highlights_stale = true;
        }
        self.native_docs_refresh_highlights(cx);
        self.native_docs_sync_browser_area(cx);
        crate::ui_fonts::sync_files_zoom(
            crate::shared_settings::shared_sidebar_settings_snapshot().object(),
        );
        let p = self.native_docs_palette(window, cx);
        self.native_docs_build_data_views(&p, window, cx);
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
        // The open file tabs sit over the document column only, as an editor's tabs do, never
        // over the files list (`open_file_tabs.rs`).
        let document = crate::app::render::view_instance_tabs::with_view_instance_tab_row(
            self.render_native_docs_tab_row(cx),
            document,
        );
        let docked_frame = self.native_docs_sample_docked_motion(layout, window, cx);
        let docked_files = (layout.docked || docked_frame.animating).then(|| {
            let list = self.render_native_docs_files_list(&p, layout, false, window, cx);
            if docked_frame.animating {
                crate::app::panel_motion::clip_panel_horizontally(docked_frame, true, list)
                    .into_any_element()
            } else {
                list
            }
        });
        // A floating list draws in a child window of its own (`drawer.rs`). A peek that was just
        // pinned stays up over the docked list while that grows in beneath it, then goes at once,
        // so the list never jumps.
        let view = VIEW_BOUNDS.with(|cell| cell.get());
        let pinning = layout.docked
            && docked_frame.animating
            && docked_frame.opening
            && self.native_docs_drawer_shown();
        self.native_docs_sync_drawer(layout.overlay || pinning, !layout.docked, view, window, cx);
        let header_bottom = super::document_view::HEADER_BOUNDS
            .with(|cell| cell.get())
            .bottom();
        let toolbar = self.render_native_docs_selection_toolbar(&p, header_bottom, window, cx);
        let composer = self.render_native_docs_composer(&p, window, cx);
        let notes_list = self.render_native_docs_notes_list(&p, window, cx);
        let note_preview = self.render_native_docs_note_preview(&p, window, cx);
        let rename_dialog = self.render_native_docs_rename_dialog(window, cx);

        Some(
            div()
                .id("native-docs")
                .track_focus(&focus)
                .key_context("NativeDocs")
                .on_key_down(cx.listener(Self::native_docs_key_down))
                .on_action(cx.listener(
                    |this, _: &crate::app::actions::FindInFocusedTerminal, window, cx| {
                        this.native_docs_find_shortcut(window, cx);
                    },
                ))
                // Search in Files (Cmd+Shift+F) shares its chord with Find Prompts; the root lets it
                // through, and here, below the root, Files claims the chord (CDXC:Docs 2026-10-25
                // DECISION in packages/settings-catalog/src/hotkey_definitions.rs).
                .on_action(cx.listener(Self::native_docs_claim_hotkey))
                // CDXC:Docs 2026-10-24 WHY: Cmd+= / Cmd+- / Cmd+0 (Ctrl on Windows and Linux) are bound app-wide to zoom the focused surface (main.rs), and a matched binding is dispatched before any key-down listener, so an open picture, or a document shown as text (`text_zoom.rs`), answers those actions here, below the root, and lets them go on otherwise.
                .on_action(cx.listener(
                    |this, _: &crate::app::actions::ZoomInFocusedSurface, _, cx| {
                        if !this.native_docs_zoom_picture(
                            super::picture_zoom::PictureZoomCommand::In,
                            cx,
                        ) && !this.native_docs_step_text_zoom(ZoomStep::In, cx)
                        {
                            cx.propagate();
                        }
                    },
                ))
                .on_action(cx.listener(
                    |this, _: &crate::app::actions::ZoomOutFocusedSurface, _, cx| {
                        if !this.native_docs_zoom_picture(
                            super::picture_zoom::PictureZoomCommand::Out,
                            cx,
                        ) && !this.native_docs_step_text_zoom(ZoomStep::Out, cx)
                        {
                            cx.propagate();
                        }
                    },
                ))
                .on_action(cx.listener(
                    |this, _: &crate::app::actions::ResetFocusedSurfaceZoom, _, cx| {
                        if !this.native_docs_zoom_picture(
                            super::picture_zoom::PictureZoomCommand::Opening,
                            cx,
                        ) && !this.native_docs_step_text_zoom(ZoomStep::Reset, cx)
                        {
                            cx.propagate();
                        }
                    },
                ))
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
                    this.native_docs_pointer_moved(event.position, sidebar_left, cx);
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
                .children(toolbar)
                .children(notes_list)
                .children(note_preview)
                .children(composer)
                .children(rename_dialog)
                .into_any_element(),
        )
    }

    /// Moves the lines at the focused Files editor's selection up or down (`move_lines.rs`). False
    /// when the key is not for an editor here (none focused, or a read-only file), so it goes on.
    fn native_docs_move_lines(
        &mut self,
        down: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(document) = self.native_docs.active_document() else {
            return false;
        };
        if document.read_only {
            return false;
        }
        if let Some(live) = document.live.clone() {
            if !gpui::Focusable::focus_handle(live.read(cx), cx).is_focused(window) {
                return false;
            }
            live.update(cx, |editor, cx| {
                super::move_lines::move_lines_in_live(editor, down, cx)
            });
            return true;
        }
        let Some(code) = document.editor.clone() else {
            return false;
        };
        if !gpui::Focusable::focus_handle(code.read(cx), cx).is_focused(window) {
            return false;
        }
        code.update(cx, |state, cx| {
            super::move_lines::move_lines_in_code(state, down, window, cx)
        })
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
        // Option+Up / Option+Down (Alt on Windows and Linux) move the editor's lines (`move_lines.rs`).
        if let Some(down) = super::move_lines::move_lines_key(event)
            && self.native_docs_move_lines(down, window, cx)
        {
            cx.stop_propagation();
            return;
        }
        // F3 and Shift+F3 step through the open document's matches while its find bar is up.
        if event.keystroke.key == "f3"
            && !modifiers.control
            && !modifiers.platform
            && !modifiers.alt
            && self.native_docs_find_visible()
        {
            self.native_docs_step_find(if modifiers.shift { -1 } else { 1 }, window, cx);
            cx.stop_propagation();
            return;
        }
        // F4 and Shift+F4 step through the Search view's results from anywhere in Files.
        if event.keystroke.key == "f4"
            && !modifiers.control
            && !modifiers.platform
            && !modifiers.alt
            && self.native_docs_has_search_results()
        {
            self.native_docs_search_step(if modifiers.shift { -1 } else { 1 }, window, cx);
            cx.stop_propagation();
            return;
        }
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
                self.native_docs_send_notes(cx);
                cx.stop_propagation();
            }
            "s" => {
                self.native_docs_save_active(cx);
                cx.stop_propagation();
            }
            "f" => {
                self.native_docs_find_shortcut(window, cx);
                cx.stop_propagation();
            }
            _ => {}
        }
    }

    /// Cmd+F (Ctrl+F on Windows and Linux) in the Files view. An open Markdown, code or text file
    /// claims it for the find panel (`find.rs`); otherwise it shows the files search.
    ///
    /// CDXC:Docs 2026-10-09 WHY:
    /// The app binds Cmd/Ctrl+F globally to its find action (`FindInFocusedTerminal`, main.rs), and GPUI dispatches a matched binding before any key-down listener. The root's handler finds in a browser, a chat or a terminal and otherwise swallows the key, so the Files view's own Ctrl+F handler never ran ("cmd/ctrl + f does nothing"). The Files view therefore answers the action itself, below the root; the key-down path stays for the floating files list's window, which has no handler for the action.
    pub(crate) fn native_docs_find_shortcut(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let findable = self
            .native_docs
            .active_document()
            .is_some_and(|document| document.live.is_some())
            || self.native_docs_code_find_editor().is_some();
        if findable && !self.native_docs_search_focused(window, cx) {
            self.native_docs_show_find(window, cx);
        } else {
            self.native_docs_show_search(window, cx);
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

    /// The header's Show files button, shown while the list is hidden: hovering peeks, clicking
    /// shows it.
    pub(super) fn render_native_docs_restore_button(
        &mut self,
        p: &DocsPalette,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let hover = p.control_hover;
        div()
            .id("native-docs-restore")
            .relative()
            .flex_none()
            .w(px(32.0))
            .h(px(27.0))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(7.0))
            .cursor_pointer()
            .hover(move |style| style.bg(hover))
            .child(crate::app::helpers::titlebar_svg_icon(
                "files-view/t-layout-sidebar-right-expand-2.svg",
                16.0,
                p.toolbar_icon,
            ))
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
