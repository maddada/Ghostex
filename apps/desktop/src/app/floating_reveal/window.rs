//! The native child window the panels ride in, and the one entry point the rest of the app calls.

use gpui::Bounds;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::ParentElement as _;
use gpui::Pixels;
use gpui::Render;
use gpui::Styled as _;
use gpui::Subscription;
use gpui::WeakEntity;
use gpui::Window;
use gpui::div;
use gpui::point;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui::size;
use gpui_component::h_flex;

use super::model::*;
use crate::app::helpers::*;
use crate::app::render::agents_workspace_layout::AgentsWorkspaceLayout;
use crate::app::render::workarea_header::workarea_header_bottom_y;
use crate::*;

/// The root of the floating window. It borrows the app entity and draws the same sidebar and
/// sessions column the docked layout draws, so there is one implementation of each.
pub(crate) struct FloatingRevealWindow {
    pub(crate) app: WeakEntity<GhostexGpuiApp>,
    _subscription: Subscription,
}

impl Render for FloatingRevealWindow {
    fn render(&mut self, window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        let Some(app) = self.app.upgrade() else {
            return div().into_any_element();
        };
        app.update(cx, |app, cx| {
            let Some((content, width)) = app
                .floating_reveal
                .panel
                .as_ref()
                .map(|panel| (panel.content, panel.width))
            else {
                return div().into_any_element();
            };
            // The panels are laid out at the window's full width and pinned to its right edge. While
            // AppKit slides the window the view's size is held (`ghostexSetContentSizeHeld:`), so
            // the viewport, unlike the window's live bounds, keeps the full width and nothing
            // reflows.
            let offset = window.viewport_size().width.as_f32() - width;
            let sidebar_width = app.sidebar_width;
            sync_overlay_window_glass(
                window,
                app.main_window_bounds.origin - app.floating_reveal_frame(width).origin,
            );
            /*
            CDXC:Sidebar 2026-09-23 DECISION:
            User: "the sidebar/chat in this case when they're floating should also have the same glass look they had before", then "make the floating sessions area keep the glass look that it should have from before pls". Under window glass the panel's backdrop is the main window's glass picture rather than a blur of the main window (`sync_overlay_window_glass`), and it paints the same tints the docked panels do: the sidebar its own tint (as does the rail beside it) and the sessions column the workspace column's tint, each straight over that picture, with the sessions column clearing its own fills as it does docked (`window_glass_active_for` counts this window). The opaque window keeps its solid fill.
            */
            div()
                .size_full()
                .overflow_hidden()
                .bg(window_shell_background())
                .on_action(cx.listener(
                    |app,
                     action: &crate::app::native_sidebar::actions::NativeSidebarAction,
                     window,
                     cx| {
                        app.handle_native_sidebar_action(action, window, cx)
                    },
                ))
                .on_action(cx.listener(
                    |app, action: &crate::app::hotkeys::RunConfiguredGhostexHotkey, _, cx| {
                        app.run_floating_reveal_hotkey(action, cx)
                    },
                ))
                .child(
                    h_flex()
                        .w(px(width))
                        .h_full()
                        .ml(px(offset))
                        .items_start()
                        .when(content.sidebar, |this| {
                            this.child(
                                div()
                                    .w(px(sidebar_width))
                                    .flex_shrink_0()
                                    .h_full()
                                    .child(app.render_native_sidebar(window, cx)),
                            )
                        })
                        .when(content.sidebar && content.agents_column, |this| {
                            this.child(
                                // Painted chrome only: no id and no listener, so GPUI gives it no
                                // hitbox and the panel is not a place to resize the split.
                                div()
                                    .relative()
                                    .w(px(FLOATING_REVEAL_RAIL_WIDTH))
                                    .flex_shrink_0()
                                    .h_full()
                                    .when(window_glass_active(), |this| {
                                        this.bg(sidebar_glass_tint())
                                    })
                                    .child(
                                        div()
                                            .absolute()
                                            .inset_0()
                                            .bg(glass_divider(sidebar_divider_background_color())),
                                    ),
                            )
                        })
                        .when(content.agents_column, |this| {
                            this.child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .flex_1()
                                    .h_full()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .when(window_glass_active(), |this| {
                                        this.bg(workspace_column_background())
                                    })
                                    .child(app.render_agents_workspace(
                                        AgentsWorkspaceLayout::Floating,
                                        window,
                                        cx,
                                    )),
                            )
                        }),
                )
                .into_any_element()
        })
    }
}

impl GhostexGpuiApp {
    /// Where the panel sits: down the window's left edge (or just past a docked sidebar), from the
    /// header's bottom to the floor, so the header keeps its window controls and its drag band while
    /// the panel is out.
    pub(crate) fn floating_reveal_frame(&self, width: f32) -> Bounds<Pixels> {
        let top = workarea_header_bottom_y();
        let height = (self.main_window_bounds.size.height.as_f32() - top).max(1.0);
        Bounds::new(
            self.main_window_bounds.origin + point(px(self.floating_reveal_left_inset()), px(top)),
            size(px(width.max(1.0)), px(height)),
        )
    }

    /// The one call the rest of the app makes. `requested` is a reveal asked for by name,
    /// `keep_under_pointer` is a layout change that must not pull the sidebar out from under the
    /// pointer; everything else is the edge strip's doing.
    pub(crate) fn update_floating_reveal(
        &mut self,
        requested: bool,
        keep_under_pointer: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        if requested {
            self.floating_reveal.requested_until = Some(
                std::time::Instant::now()
                    + std::time::Duration::from_secs(FLOATING_REVEAL_REQUEST_GRACE_SECS),
            );
        }
        if !self.floating_reveal_eligible() {
            self.close_floating_reveal(cx);
            return;
        }
        if keep_under_pointer && self.floating_reveal.panel.is_none() {
            self.floating_reveal.want.sidebar = true;
        }
        let content = self.floating_reveal_content();
        if !content.sidebar && !content.agents_column {
            if self.floating_reveal.panel.is_some() {
                self.close_floating_reveal(cx);
            }
            return;
        }
        self.refresh_floating_reveal_panel_shape(cx);
        self.sync_floating_reveal_host(requested, keep_under_pointer, cx);
    }

    /// The 60ms sweep that owns the gesture: it is the only thing that watches the pointer leave.
    /// Returns whether a panel is on screen, which is what shortens the next interval on the
    /// backends that animate the slide themselves.
    pub(crate) fn poll_floating_reveal(
        &mut self,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        #[cfg(target_os = "macos")]
        self.sync_floating_reveal_native_edge();
        if !self.floating_reveal_eligible() {
            self.close_floating_reveal(cx);
            return false;
        }
        // CDXC:Sidebar 2026-09-23 WHY:
        // The sessions reveal takes keyboard focus, and interacting with either floating panel or its chat's child windows can activate another window. Main-window inactivity alone therefore dismissed the Windows reveal on the next poll; keep the panel while its own interaction owns focus.
        #[cfg(not(target_os = "macos"))]
        if !window.is_window_active()
            && !self.floating_reveal_chat_child_active(cx)
            && self.floating_reveal.panel.as_ref().is_some_and(|panel| {
                !panel
                    .window
                    .update(cx, |_, window, _| window.is_window_active())
                    .unwrap_or(false)
            })
        {
            self.close_floating_reveal(cx);
            return false;
        }
        let _ = window;
        self.update_floating_reveal(false, false, cx);
        // An armed edge counts as active too: the panel opens on a deferred task, so the sweep that
        // asked for it has nothing to show yet and the slide would otherwise start a beat late.
        self.floating_reveal.panel.is_some() || self.floating_reveal.edge_hovered
    }

    /// Keep the open panel's content and width current: un-maximising the view panel while it is
    /// out takes the sessions column back, and a window resize re-derives the column's share.
    fn refresh_floating_reveal_panel_shape(&mut self, cx: &mut gpui::Context<Self>) {
        let content = self.floating_reveal_content();
        let width = self.floating_reveal_width_for(content);
        let Some(panel) = self.floating_reveal.panel.as_mut() else {
            return;
        };
        if panel.content == content && (panel.width - width).abs() < 0.5 {
            return;
        }
        let hosted_before = panel.content.agents_column;
        panel.content = content;
        panel.width = width;
        if hosted_before != content.agents_column {
            self.reconcile_agents_pane_surfaces(cx);
            self.update_active_mode_cef_child_visibility(cx);
            self.schedule_floating_reveal_focus(cx);
        }
        self.schedule_floating_reveal_bounds_refresh(cx);
        cx.notify();
    }

    /// CDXC:Sidebar 2026-09-22 WHY:
    /// App hotkeys are answered by the main window's root, and the floating panel is a window of
    /// its own that the root is not under, so with the panel key (a floating chat's composer has
    /// the keyboard) every configured hotkey found no listener and Option+P could not open the
    /// quick model picker. The panel runs them itself, against the main window. It skips the main
    /// root's keyboard-owner gate on purpose: that gate reads who owns the main window's keyboard
    /// (a Source view's editor keeps it while the panel is key) and would hand the chord to a
    /// page that never saw it.
    fn run_floating_reveal_hotkey(
        &self,
        action: &crate::app::hotkeys::RunConfiguredGhostexHotkey,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(main) = self.main_window_handle else {
            return;
        };
        let app = cx.weak_entity();
        let action_id = action.action_id.clone();
        gpui::App::defer(cx, move |cx| {
            let _ = main.update(cx, |_, window, cx| {
                let _ = app.update(cx, |app, cx| {
                    app.run_configured_ghostex_hotkey(&action_id, window, cx)
                });
            });
        });
    }

    /// CDXC:Sidebar 2026-09-23 DECISION:
    /// User: the quick model picker goes away instantly when the floating panel is no longer
    /// hovered and is going away. The picker (and an image preview) is a window of its own over
    /// the chat's pane, so it used to stay up for the whole slide and only closed once the panel
    /// was gone. The hosts call this the moment the panel starts to leave. Since 2026-09-24 that
    /// picker is the composer's model pop-up (session_chat_model_picker.rs), which closes the same way.
    pub(super) fn dismiss_floating_reveal_chat_windows(&mut self, cx: &mut gpui::Context<Self>) {
        if !self
            .floating_reveal
            .panel
            .as_ref()
            .is_some_and(|panel| panel.content.agents_column)
        {
            return;
        }
        // The panel hosts the only Agents Panel on screen, so every visible chat is in it.
        self.dismiss_native_chat_windows_leaving_view(&std::collections::HashSet::new(), cx);
    }

    /// CDXC:Sidebar 2026-09-22 WHY:
    /// GPUI learns a window's new size only from the platform's resize callback, and that callback
    /// re-enters the app through `AsyncApp::update_window`, which gives up when the app is already
    /// borrowed. The hosts resize the panel from inside an app update (a session click in the
    /// floating sidebar, the poll), so the callback failed silently and the window kept laying out
    /// at its old width: the sessions column a click added was clipped off the layout, and its
    /// share of the wider window stayed unpainted black. Re-reading the bounds on a deferred tick,
    /// once the update that resized the window has ended, does what the callback would have done.
    pub(super) fn schedule_floating_reveal_bounds_refresh(&self, cx: &mut gpui::Context<Self>) {
        let Some(handle) = self
            .floating_reveal
            .panel
            .as_ref()
            .map(|panel| panel.window)
        else {
            return;
        };
        gpui::App::defer(cx, move |cx| {
            let _ = handle.update(cx, |_, window, cx| window.bounds_changed(cx));
        });
    }

    /// CDXC:Sidebar 2026-09-09 WHY:
    /// GPUI `open_window` synchronously draws its root, whose render updates this app entity.
    /// Opening it inside the hover handler's own update caused the repeated double-lease panic in
    /// the September 9 crash reports. Defer creation onto `App` itself, outside any app-entity
    /// update, then attach the finished window in a separate update.
    pub(crate) fn open_floating_reveal(&mut self, sticky: bool, cx: &mut gpui::Context<Self>) {
        let app = cx.weak_entity();
        gpui::App::defer(cx, move |cx| {
            let Some(app) = app.upgrade() else {
                return;
            };
            let (options, content, width) = {
                let this = app.read(cx);
                if this.floating_reveal.panel.is_some() || !this.floating_reveal_eligible() {
                    return;
                }
                let content = this.floating_reveal_content();
                let width = this.floating_reveal_width_for(content);
                // AppKit sizes the panel itself from the first frame of its own animation; the
                // other backends start the window at a sliver and grow it.
                let opening_width = if sticky || cfg!(target_os = "macos") {
                    width
                } else {
                    1.0
                };
                let bounds = this.floating_reveal_frame(opening_width);
                (
                    gpui::WindowOptions {
                        window_bounds: Some(gpui::WindowBounds::Windowed(bounds)),
                        display_id: crate::app::window::popup_frame::display_at(
                            bounds.center(),
                            cx,
                        )
                        .or(this.main_window_display_id),
                        focus: false,
                        // The AppKit host orders the panel in once it is a child window; elsewhere
                        // the popup is shown by the platform when it is created.
                        show: !cfg!(target_os = "macos"),
                        kind: gpui::WindowKind::PopUp,
                        // Clear outside its rounded corners when the window is not glass.
                        window_background: if window_glass_active() {
                            gpui::WindowBackgroundAppearance::Blurred
                        } else {
                            gpui::WindowBackgroundAppearance::Transparent
                        },
                        is_movable: false,
                        is_resizable: false,
                        is_minimizable: false,
                        titlebar: None,
                        app_id: crate::gpui_platform_window_app_id(),
                        icon: crate::gpui_platform_window_icon(),
                        ..Default::default()
                    },
                    content,
                    width,
                )
            };
            let observed = app.clone();
            let result = cx.open_window(options, move |window, cx| {
                window.set_background_corner_radius(gpui::px(FLOATING_PANEL_CORNER_RADIUS));
                let view = cx.new(|cx| FloatingRevealWindow {
                    app: observed.downgrade(),
                    _subscription: cx.observe(&observed, |_, _, cx| cx.notify()),
                });
                /*
                CDXC:SessionChat 2026-09-18 WHY:
                gpui-component drives transcript text selection from the window's
                `gpui_component::Root`, so a window that hosts the native chat without one cannot
                select any text. The panel paints its own background, so the Root's surface stays
                clear.
                */
                cx.new(|cx| {
                    gpui_component::Root::new(view, window, cx).bg(gpui::transparent_black())
                })
            });
            let handle = match result {
                Ok(handle) => handle,
                Err(error) => {
                    app.update(cx, |app, cx| {
                        app.dispatch_gpui_app_modal_toast(
                            "warning",
                            "Sidebar unavailable",
                            &error.to_string(),
                            cx,
                        )
                    });
                    return;
                }
            };
            // Read outside the app update: the AppKit host needs the panel's own view, and leasing
            // a second window from inside the app entity's update is what the defer above exists to
            // avoid.
            let native_view = handle
                .update(cx, |_, window, _| cef_parent_native_view(window))
                .ok()
                .and_then(Result::ok);
            if let Some(native_view) = native_view {
                crate::app::native_docs::drawer::round_floating_panel(native_view);
            }
            let anchor = app.read(cx).floating_reveal_frame(width).origin;
            set_floating_reveal_glass_window(Some(handle.into()));
            app.update(cx, |app, cx| {
                if !app.attach_floating_reveal_panel(
                    handle,
                    native_view,
                    anchor,
                    content,
                    width,
                    sticky,
                    cx,
                ) {
                    return;
                }
                if content.agents_column {
                    // The sessions column just came back on screen in a second window, so every
                    // gate that reads `agents_workspace_visible()` is reconciled at this boundary,
                    // exactly as the expand toggle reconciles them.
                    app.reconcile_agents_pane_surfaces(cx);
                    app.update_active_mode_cef_child_visibility(cx);
                    app.schedule_floating_reveal_focus(cx);
                }
                cx.notify();
            });
        });
    }

    /// CDXC:Sidebar 2026-09-21 DECISION:
    /// User: a floating sessions pane opened by a hover, a session click or a new agent takes
    /// typing right away. The panel becomes the key window and the focused pane's terminal or chat
    /// composer takes GPUI focus in it. Deferred onto `App` for the reason `open_floating_reveal`
    /// is: leasing the panel's window from inside this entity's update double-leases it.
    pub(crate) fn schedule_floating_reveal_focus(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(handle) = self
            .floating_reveal
            .panel
            .as_ref()
            .filter(|panel| panel.content.agents_column)
            .map(|panel| panel.window)
        else {
            return;
        };
        let app = cx.weak_entity();
        gpui::App::defer(cx, move |cx| {
            let _ = handle.update(cx, |_, window, cx| {
                window.activate_window();
                if let Some(app) = app.upgrade() {
                    app.update(cx, |app, cx| app.focus_floating_reveal_sessions(window, cx));
                }
            });
        });
    }

    fn focus_floating_reveal_sessions(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let workspace = &self.agents_workspace;
        let Some(pane_id) = workspace
            .focus_mode_pane
            .into_iter()
            .chain(std::iter::once(workspace.focused_pane))
            .find(|pane_id| workspace.find_leaf(*pane_id).is_some())
        else {
            return;
        };
        let Some(session_id) = workspace.active_session_in_pane(pane_id) else {
            return;
        };
        if self.agents_chat_mode_sessions.contains(&session_id)
            && let Some(chat) = self.native_chat_views.get(&session_id).cloned()
        {
            // The terminal branch below claims the pane through its mount slot. A chat that takes
            // the keyboard has to claim it too, or session-scoped hotkeys (Option+P) resolve
            // against whatever the main window focused last and find no session.
            self.set_shell_focus(ShellFocusTarget::AgentsPane(pane_id));
            if let Ok(root) = cef_parent_native_view(window) {
                crate::cef::focus_gpui_root_view(root);
            }
            chat.update(cx, |chat, cx| {
                chat.focus_requested = true;
                cx.notify();
            });
            return;
        }
        self.focus_agents_terminal_mount_slot(
            crate::app::model::AgentsTerminalBodyMountSlotId {
                pane_id,
                session_id,
            },
            window,
            cx,
        );
    }

    pub(crate) fn close_floating_reveal(&mut self, cx: &mut gpui::Context<Self>) {
        let Some(panel) = self.floating_reveal.panel.take() else {
            self.floating_reveal.edge_hovered = false;
            self.floating_reveal.requested_until = None;
            return;
        };
        let hosted_agents_column = panel.content.agents_column;
        set_floating_reveal_glass_window(None);
        self.dispose_floating_reveal_host(&panel);
        let _ = panel
            .window
            .update(cx, |_, window, _| window.remove_window());
        self.floating_reveal.edge_hovered = false;
        self.floating_reveal.requested_until = None;
        self.floating_reveal.want = FloatingRevealContent::default();
        #[cfg(not(target_os = "macos"))]
        {
            self.floating_reveal.outside_since = None;
            self.floating_reveal.slide = FloatingRevealSlide::default();
        }
        if hosted_agents_column {
            self.reconcile_agents_pane_surfaces(cx);
            self.update_active_mode_cef_child_visibility(cx);
        }
        cx.notify();
    }
}
