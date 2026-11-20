//! What a popup window draws (its header, its tabs and their sessions), and the card a session's
//! pane shows meanwhile.

use ghostex_gx_core::SessionKey;
use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Entity, FontWeight, InteractiveElement as _, IntoElement, MouseButton,
    MouseDownEvent, ParentElement as _, Render, SharedString, StatefulInteractiveElement as _,
    Styled as _, Subscription, WeakEntity, Window, div, px,
};

use super::{SessionPopupId, pin_supported};
use crate::GhostexGpuiApp;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::render::sleeping_card::{view_card_button, view_card_frame};
use crate::app::render::terminal_content_layout::terminal_content_frame;
use crate::app::render::window_drag_region::window_drag_region;

/// The header row's height: the work area header's, so the traffic lights sit on its centreline.
pub(super) const HEADER_HEIGHT: f32 = 38.0;
/// Room for the traffic lights on macOS; elsewhere the header starts at the window's left edge.
#[cfg(target_os = "macos")]
pub(super) const HEADER_LEADING: f32 = 80.0;
#[cfg(not(target_os = "macos"))]
pub(super) const HEADER_LEADING: f32 = 8.0;

/// The window's root: it draws the app's sessions and redraws when what it shows changes.
pub(crate) struct SessionPopupRoot {
    app: WeakEntity<GhostexGpuiApp>,
    id: SessionPopupId,
    /// What the window last drew from; see `popup_signature`.
    drawn: Option<PopupSignature>,
    /// The window's own focus, so its keys reach the handlers below before the chat or the
    /// terminal has been clicked.
    focus: gpui::FocusHandle,
    _subscriptions: Vec<Subscription>,
}

/// What one tab of the window draws from.
type TabSignature = (
    SessionKey,
    Option<TerminalSessionId>,
    bool,
    Option<gpui::EntityId>,
    Option<gpui::EntityId>,
    Option<TerminalSessionPresentationState>,
    String,
    Option<&'static str>,
);

/// What the window's own drawing depends on. The chat and the terminal are views of their own that
/// redraw themselves, so the window ignores the app's other notifications (terminal output, chat
/// streaming, indicator ticks), as the board's window does (CDXC:ProjectBoard 2026-10-13).
/// The last part is each tab's chat's Work panel (has one, shown, how many wait on the user), for
/// the header's Work button.
type PopupSignature = (
    Vec<TabSignature>,
    u64,
    bool,
    bool,
    bool,
    u64,
    Vec<(bool, bool, usize)>,
);

fn popup_signature(
    app: &GhostexGpuiApp,
    id: SessionPopupId,
    cx: &gpui::App,
) -> Option<PopupSignature> {
    let popup = app.session_popups.get(id)?;
    let work = popup
        .keys_in_order()
        .into_iter()
        .filter_map(|key| app.session_popup_shell_session(&key))
        .filter_map(|id| app.native_chat_views.get(&id))
        .map(|chat| {
            let chat = chat.read(cx);
            (
                chat.work_panel_available(),
                chat.work_panel_shown(),
                chat.work_panel_waiting(),
            )
        })
        .collect();
    let tabs = popup
        .keys_in_order()
        .into_iter()
        .map(|key| {
            let shell = app.session_popup_shell_session(&key);
            let session = shell.and_then(|id| app.agents_workspace.session(id));
            (
                shell,
                shell.is_some_and(|id| app.agents_chat_mode_sessions.contains(&id)),
                shell
                    .and_then(|id| app.native_chat_views.get(&id))
                    .map(Entity::entity_id),
                shell
                    .and_then(|id| app.agents_gpui_engine_terminals.get(&id))
                    .map(|record| record.view.entity_id()),
                session.map(|session| session.presentation_state),
                shell
                    .map(|id| app.agents_workspace_tab_display_title(id))
                    .unwrap_or_default(),
                session.and_then(|session| session.agent_icon),
                key,
            )
        })
        .map(
            |(shell, chat, chat_view, terminal, state, title, icon, key)| {
                (key, shell, chat, chat_view, terminal, state, title, icon)
            },
        )
        .collect();
    Some((
        tabs,
        popup.revision,
        popup.pinned,
        popup.expanded,
        app.gx_store_sidebar_list_ready(),
        window_glass_redraw_key(),
        work,
    ))
}

impl SessionPopupRoot {
    pub(crate) fn new(
        app: Entity<GhostexGpuiApp>,
        id: SessionPopupId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let observed = cx.observe(&app, move |root: &mut Self, app, cx| {
            let signature = popup_signature(app.read(cx), id, cx);
            if root.drawn != signature {
                root.drawn = signature;
                cx.notify();
            }
        });
        let bounds_app = app.downgrade();
        let bounds = cx.observe_window_bounds(window, move |_, window, cx| {
            let frame = window.bounds();
            let display = window.display(cx).map(|display| display.id());
            let _ = bounds_app.update(cx, |app, _| {
                app.session_popup_note_bounds(id, frame, display)
            });
        });
        // The popup the user was in last is the one the next popped-out session joins.
        let active_app = app.downgrade();
        let activation = cx.observe_window_activation(window, move |_, window, cx| {
            if window.is_window_active() {
                let _ = active_app.update(cx, |app, _| app.session_popup_note_active(id));
            }
        });
        Self {
            drawn: popup_signature(app.read(cx), id, cx),
            app: app.downgrade(),
            id,
            focus: {
                let focus = cx.focus_handle();
                window.focus(&focus, cx);
                focus
            },
            _subscriptions: vec![observed, bounds, activation],
        }
    }

    /// The session of the window's selected tab, as a sidebar row id.
    fn active_row(&self, cx: &Context<Self>) -> Option<String> {
        let app = self.app.upgrade()?;
        let popup = app.read(cx).session_popups.get(self.id)?;
        popup.active_key().map(|key| key.to_sidebar_session_id())
    }

    /// Cmd+W (and the menu's Close Pane) in this window: the selected tab's session goes back to
    /// its pane, as its tab's close button does; the window closes with its last tab.
    fn close_tab(&mut self, cx: &mut Context<Self>) {
        let (Some(app), Some(row_id)) = (self.app.upgrade(), self.active_row(cx)) else {
            return;
        };
        app.update(cx, |app, cx| {
            app.close_session_popup(&row_id, cx);
        });
    }

    /// The Ghostex hotkeys pressed in this window. Open in a Popup (`popOutPane`) brings the
    /// selected tab's session back; the rest run in the main window, as the floating sidebar's do
    /// (floating_reveal/window.rs).
    fn run_hotkey(
        &mut self,
        action: &crate::app::hotkeys::RunConfiguredGhostexHotkey,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(app) = self.app.upgrade() else {
            return;
        };
        match action.action_id.as_str() {
            "popOutPane" => {
                if let Some(row_id) = self.active_row(cx) {
                    app.update(cx, |app, cx| app.bring_back_session_popup(&row_id, cx));
                }
            }
            _ => {
                let Some(main) = app.read(cx).main_window_handle else {
                    return;
                };
                let app = app.downgrade();
                let action_id = action.action_id.clone();
                cx.defer(move |cx| {
                    let _ = main.update(cx, |_, window, cx| {
                        let _ = app.update(cx, |app, cx| {
                            app.run_configured_ghostex_hotkey(&action_id, window, cx)
                        });
                    });
                });
            }
        }
    }
}

impl Render for SessionPopupRoot {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(app) = self.app.upgrade() else {
            return div().into_any_element();
        };
        sync_own_window_glass(window);
        let id = self.id;
        let body = app.update(cx, |app, cx| {
            app.render_session_popup_window(id, window, cx)
        });
        let drop_app = self.app.clone();
        div()
            .size_full()
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::run_hotkey))
            .on_action(cx.listener(
                |root, _: &crate::app::actions::CloseFocusedSurface, _, cx| root.close_tab(cx),
            ))
            .on_action(cx.listener(
                |root, _: &crate::app::actions::CloseFocusedSurfaceMenuOnly, _, cx| {
                    root.close_tab(cx)
                },
            ))
            // A tab released outside the window leaves for another popup or a popup of its own
            // (tabs.rs); one released inside was handled by what it was dropped on.
            .on_mouse_up_out(MouseButton::Left, move |event, window, cx| {
                let position = event.position;
                // A release inside the window is the drop target's (and `on_mouse_up` below), even
                // where a child kept the root from counting as hovered.
                if !cx.has_active_drag()
                    || gpui::Bounds::new(gpui::Point::default(), window.viewport_size())
                        .contains(&position)
                {
                    return;
                }
                let origin = super::window_global_bounds(window, cx).origin;
                let _ = drop_app.update(cx, |app, cx| {
                    app.session_popup_tab_released(id, Some(origin + position), cx);
                    app.session_popup_split_drag_end(id, cx);
                });
            })
            .on_mouse_up(MouseButton::Left, {
                let app = self.app.clone();
                move |_, _, cx| {
                    let _ = app.update(cx, |app, cx| {
                        app.session_popup_tab_released(id, None, cx);
                        app.session_popup_split_drag_end(id, cx);
                    });
                }
            })
            // A held divider follows the pointer anywhere in the window (panes.rs).
            .on_mouse_move({
                let app = self.app.clone();
                move |event: &gpui::MouseMoveEvent, _, cx| {
                    let _ = app.update(cx, |app, cx| {
                        app.session_popup_split_drag_move(id, event, cx)
                    });
                }
            })
            .child(body)
            .into_any_element()
    }
}

impl GhostexGpuiApp {
    /// The window's header and its sessions under it.
    fn render_session_popup_window(
        &mut self,
        id: SessionPopupId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(popup) = self.session_popups.get(id) else {
            return div().into_any_element();
        };
        let (expanded, pinned) = (popup.expanded, popup.pinned);
        let active = popup.active_key();
        let root = popup.layout.root.clone();
        let title = active
            .as_ref()
            .map(|key| self.session_popup_title(key))
            .unwrap_or_default();
        let agent_icon = active
            .as_ref()
            .and_then(|key| self.session_popup_shell_session(key))
            .and_then(|id| self.agents_workspace.session(id))
            .and_then(|session| session.agent_icon);
        window.set_window_title(&title);
        // The selected session's Work panel, from the popup's own header.
        let work = active
            .as_ref()
            .and_then(|key| self.session_popup_shell_session(key))
            .and_then(|session_id| self.render_session_popup_work_button(session_id, cx));
        // An unsplit popup's tabs sit in its header, even its only tab; a split popup's panes each
        // keep their own bar over them, and the header names the selected session.
        let header_strip = match &root {
            WorkspaceNode::Leaf(leaf) => {
                Some(self.render_session_popup_tab_strip(id, leaf.pane_id, None, true, cx))
            }
            WorkspaceNode::Split(_) => None,
        };
        let body = self.render_session_popup_node(id, &root, window, cx);
        div()
            .id("session-popup-window")
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            // The work area's tint over the glass, as the session has in its pane (opaque without
            // glass).
            .bg(workspace_column_background())
            .child(self.render_session_popup_header(
                id,
                active.map(|key| key.to_sidebar_session_id()),
                header_strip,
                &title,
                agent_icon,
                expanded,
                pinned,
                work,
                window,
                cx,
            ))
            .child(body)
            .into_any_element()
    }

    /// One pane of the window: its tab bar when the window is split (the bar names the pane's
    /// project; an unsplit popup's bar is in its header), then the selected tab's session.
    pub(crate) fn render_session_popup_pane(
        &mut self,
        id: SessionPopupId,
        pane: WorkspacePaneId,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(popup) = self.session_popups.get(id) else {
            return div().into_any_element();
        };
        let Some(leaf) = popup.layout.find_leaf(pane) else {
            return div().into_any_element();
        };
        let split = matches!(popup.layout.root, WorkspaceNode::Split(_));
        let key = leaf
            .tab_group
            .active_session_id()
            .and_then(|tab| popup.keys.get(&tab).cloned());
        let project = split
            .then(|| {
                key.as_ref()
                    .map(|key| self.session_popup_project_title(key))
            })
            .flatten();
        let strip =
            split.then(|| self.render_session_popup_tab_strip(id, pane, project, false, cx));
        let body = key
            .map(|key| self.render_session_popup_session(&key, cx))
            .unwrap_or_else(|| div().into_any_element());
        let body = self.session_popup_pane_drop_target(id, pane, body, cx);
        let view = cx.entity().downgrade();
        div()
            .id(format!("session-popup-pane-{}", pane.0))
            .flex_1()
            .min_h_0()
            .min_w_0()
            .flex()
            .flex_col()
            // A click anywhere in the pane makes it the one the header acts on, without taking
            // the click from the chat or terminal under it.
            .capture_any_mouse_down(move |_: &MouseDownEvent, _, cx| {
                let _ = view.update(cx, |app, cx| app.session_popup_focus_pane(id, pane, cx));
            })
            .children(strip)
            .child(body)
            .into_any_element()
    }

    /// The session `key` in a pane of a popup: its chat or terminal, or the state it is in.
    fn render_session_popup_session(
        &mut self,
        key: &SessionKey,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let row_id = key.to_sidebar_session_id();
        let shell = self.session_popup_shell_session(key);
        let title = self.session_popup_title(key);
        let chat = shell.is_some_and(|id| self.agents_chat_mode_sessions.contains(&id));
        let body = match shell {
            Some(session_id) if chat => self.render_session_chat_surface_content(session_id),
            Some(session_id) => self.render_session_popup_terminal(&row_id, session_id, cx),
            None if !self.gx_store_sidebar_list_ready() => self.render_session_chat_skeleton(),
            None => self.render_session_popup_card(
                &row_id,
                format!("{title} isn't open in the main window"),
                "Ghostex's window shows another project, or the session isn't open there yet.",
                "Show Session",
                cx,
            ),
        };
        let background = if chat {
            glass_clear(gpui_session_chat_background_color())
        } else {
            workspace_terminal_body_color(
                shell
                    .and_then(|id| self.agents_workspace.session(id))
                    .map(|session| session.presentation_state),
            )
        };
        div()
            .flex_1()
            .min_h_0()
            .min_w_0()
            .relative()
            .bg(background)
            .child(body)
            .into_any_element()
    }

    /// The terminal in the window: the pane's own terminal view, or the state it is in.
    fn render_session_popup_terminal(
        &mut self,
        row_id: &str,
        session_id: TerminalSessionId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        if let Some(view) = self
            .agents_gpui_engine_terminals
            .get(&session_id)
            .map(|record| record.view.clone())
        {
            let (horizontal, vertical, width_percent) =
                crate::shared_settings::shared_sidebar_settings_snapshot()
                    .terminal_pane_layout(true);
            let focus_view = view.clone();
            return div()
                .id(format!("session-popup-terminal-{}", session_id.0))
                .size_full()
                .min_w_0()
                .min_h_0()
                // The terminal takes the keyboard of its own window when it is clicked, as it does
                // in its pane (`focus_agents_terminal_mount_slot`).
                .capture_any_mouse_down(move |_: &MouseDownEvent, window, cx| {
                    let handle = gpui::Focusable::focus_handle(focus_view.read(cx), cx);
                    window.focus(&handle, cx);
                })
                .child(terminal_content_frame(
                    div().size_full().child(view),
                    horizontal,
                    vertical,
                    width_percent,
                ))
                .into_any_element();
        }
        let state = self
            .agents_workspace
            .session(session_id)
            .map(|session| session.presentation_state);
        match state {
            Some(TerminalSessionPresentationState::Sleeping) => self.render_session_popup_card(
                row_id,
                format!(
                    "{} is asleep",
                    self.agents_workspace_tab_display_title(session_id)
                ),
                "Wake it to see its terminal here.",
                "Wake",
                cx,
            ),
            Some(TerminalSessionPresentationState::Running) => {
                // Its viewer was let go while nothing showed it; this window shows it now.
                let app = cx.entity().downgrade();
                cx.defer(move |cx| {
                    let _ = app.update(cx, |app, cx| {
                        if app.ensure_agents_gpui_engine_terminal_view(session_id, cx) {
                            cx.notify();
                        }
                    });
                });
                crate::app::view_skeletons::render_view_skeleton(
                    crate::app::view_skeletons::ViewSkeletonKind::Terminal,
                    format!("session-popup-terminal-skeleton-{}", session_id.0),
                    glass_clear(workspace_terminal_placeholder_color()),
                )
            }
            _ => crate::app::view_skeletons::render_view_skeleton(
                crate::app::view_skeletons::ViewSkeletonKind::Terminal,
                format!("session-popup-terminal-skeleton-{}", session_id.0),
                glass_clear(workspace_terminal_placeholder_color()),
            ),
        }
    }

    /// A centred card in the window with one button that shows the session in the main window
    /// (which also wakes it or switches the main window to its project).
    fn render_session_popup_card(
        &self,
        row_id: &str,
        heading: String,
        detail: &'static str,
        button: &'static str,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ink = chrome_ink();
        let row = row_id.to_string();
        div()
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .p(px(16.0))
            .child(
                view_card_frame()
                    .text_center()
                    .child(
                        div()
                            .text_size(px(14.0))
                            .line_height(px(20.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(ink.opacity(0.9))
                            .child(heading),
                    )
                    .child(
                        div()
                            .mt(px(6.0))
                            .text_size(px(12.5))
                            .line_height(px(18.0))
                            .text_color(ink.opacity(0.55))
                            .child(detail),
                    )
                    .child(
                        div().mt(px(14.0)).child(
                            view_card_button(button)
                                .id(SharedString::from(format!("session-popup-card-{row}")))
                                .on_click(cx.listener(move |app, _, _, cx| {
                                    app.gx_store_focus_session_row(
                                        &row,
                                        crate::app::gx_store::RowFocusOptions::default(),
                                        cx,
                                    );
                                })),
                        ),
                    ),
            )
            .into_any_element()
    }

    /// The window's title bar: the tabs (`strip`, an unsplit popup's) or else the selected tab's
    /// agent and title, then Pin, Bring Back and Expand or Shrink. It is the window's own title
    /// band on every platform: the traffic lights sit at its left on macOS, the minimize, maximize
    /// and close buttons at its right on Windows and Linux (where Ghostex draws the caption), and
    /// a drag of its empty space moves the window while a double-click there does what the
    /// platform's title bar does (`window_drag_region`). Its tabs and buttons take their own
    /// presses, so a tab drag stays a tab drag.
    ///
    /// CDXC:SessionPopup 2026-11-18 DECISION:
    /// User: "for the pop ups, we should always show the tabs bar even if just 1 tab is there and i wish we show the tabs bar in the title bar of the pop up itself instead of below the titlebar". An unsplit popup draws its tab bar inside this header, right of the traffic lights, in the band its own buttons use, like a browser's tab strip, and draws it with one tab too. Windows hides its system caption and Linux asks for client decorations (mod.rs), so the header is the title band there as well, ending in the main window's caption buttons. A split popup keeps a bar over each pane, which names the pane's project and takes a tab dropped back onto it to unsplit (panes.rs), and its header keeps the selected session's title. Supersedes the 2026-11-17 rule that a popup showed its tab bar under the header and only from two tabs.
    #[allow(clippy::too_many_arguments)]
    fn render_session_popup_header(
        &self,
        id: SessionPopupId,
        active_row: Option<String>,
        strip: Option<AnyElement>,
        title: &str,
        agent_icon: Option<&'static str>,
        expanded: bool,
        pinned: bool,
        work: Option<AnyElement>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let text = titlebar_project_text_color();
        let icon = titlebar_icon_color();
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        let caption = app_draws_window_caption(window).then(|| {
            self.render_titlebar_window_controls(window, cx)
                .into_any_element()
        });
        #[cfg(not(any(target_os = "windows", target_os = "linux")))]
        let caption: Option<AnyElement> = {
            let _ = window;
            None
        };
        let has_strip = strip.is_some();
        let row = div()
            .id("session-popup-header")
            .h(px(HEADER_HEIGHT))
            .w_full()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(6.0))
            .pl(px(HEADER_LEADING))
            // The caption buttons reach the window's top-right corner, as the main window's do.
            .pr(px(if caption.is_some() { 0.0 } else { 8.0 }))
            .border_b_1()
            .border_color(chrome_ink().opacity(0.08))
            .children(strip)
            .when_some(
                agent_icon
                    .and_then(workspace_tab_agent_icon_path)
                    .filter(|_| !has_strip),
                |row, path| {
                    row.child(titlebar_svg_icon(
                        path,
                        14.0,
                        if agent_icon == Some("claude") {
                            gpui::rgb(0xd97757).into()
                        } else {
                            icon
                        },
                    ))
                },
            )
            .when(!has_strip, |row| {
                row.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_size(px(13.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(text)
                        .child(title.to_string()),
                )
            })
            .children(work)
            .when(pin_supported(), |row| {
                row.child(
                    header_button(
                        "session-popup-pin",
                        if pinned {
                            "titlebar/pin-filled.svg"
                        } else {
                            "titlebar/pin-outline.svg"
                        },
                        if pinned {
                            "Stop Keeping on Top"
                        } else {
                            "Keep on Top"
                        },
                        pinned,
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |app, _, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            app.toggle_session_popup_pin(id, cx);
                        }),
                    ),
                )
            })
            .when_some(active_row, |row, back_row| {
                row.child(
                    header_button(
                        "session-popup-back",
                        "titlebar/arrow-back-up.svg",
                        "Bring Back",
                        false,
                    )
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |app, _, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            app.bring_back_session_popup(&back_row, cx);
                        }),
                    ),
                )
            })
            .child(
                header_button(
                    "session-popup-size",
                    if expanded {
                        "titlebar/arrows-diagonal-minimize.svg"
                    } else {
                        "titlebar/maximize.svg"
                    },
                    if expanded { "Shrink" } else { "Expand" },
                    false,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |app, _, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        app.toggle_session_popup_expanded(id, window, cx);
                    }),
                ),
            )
            .children(caption);
        window_drag_region(row).into_any_element()
    }

    /// The pane's card while its session is in a popup, in the board's style
    /// (`render_native_kanban_detached_card`).
    pub(crate) fn render_session_popup_placeholder(
        &self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ink = chrome_ink();
        let key = self.workspace_terminal_session_key(session_id);
        let row_id = key
            .as_ref()
            .map(SessionKey::to_sidebar_session_id)
            .unwrap_or_default();
        let back_row = row_id;
        let title = self.agents_workspace_tab_display_title(session_id);
        div()
            .id(format!("session-popup-placeholder-{}", session_id.0))
            .flex_1()
            .min_w_0()
            .min_h_0()
            .w_full()
            .flex()
            .items_center()
            .justify_center()
            .p(px(16.0))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |app, _: &MouseDownEvent, _, cx| {
                    app.focus_agents_pane(pane_id, cx);
                }),
            )
            .child(
                view_card_frame()
                    .text_center()
                    .child(titlebar_svg_icon(
                        "titlebar/external-link.svg",
                        30.0,
                        ink.opacity(0.8).into(),
                    ))
                    .child(
                        div()
                            .mt(px(8.0))
                            .text_size(px(15.0))
                            .line_height(px(21.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(ink.opacity(0.9))
                            .child(SharedString::from(format!("{title} is open in a popup"))),
                    )
                    .child(
                        div()
                            .mt(px(6.0))
                            .text_size(px(12.5))
                            .line_height(px(18.0))
                            .text_color(ink.opacity(0.55))
                            .child("Closing it there brings it back here."),
                    )
                    .child(
                        div()
                            .mt(px(16.0))
                            .flex()
                            .gap(px(8.0))
                            .child(
                                view_card_button("Show Window")
                                    .id("session-popup-placeholder-show")
                                    .on_click(cx.listener(move |app, _, _, cx| {
                                        if let Some(key) = key.clone() {
                                            app.open_session_popup(key, cx);
                                        }
                                    })),
                            )
                            .child(
                                view_card_button("Bring Back")
                                    .id("session-popup-placeholder-back")
                                    .on_click(cx.listener(move |app, _, _, cx| {
                                        app.bring_back_session_popup(&back_row, cx);
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }
}

impl GhostexGpuiApp {
    /// The work area header's Open in a Popup button, for the focused pane's session.
    pub(crate) fn render_workarea_header_session_popup_button(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session_id = self.session_popup_focused_target()?;
        let session = self.agents_workspace.session(session_id)?;
        if session.agent_icon == Some("browser") {
            return None;
        }
        let tooltip = titlebar_tooltip_label("Popout", "popOutPane");
        let color = titlebar_icon_color();
        Some(
            div()
                .id("ghostex-gpui-workarea-header-session-popup")
                .role(gpui::Role::Button)
                .aria_label("Popout")
                .flex()
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .h(px(crate::app::consts::TITLEBAR_CONTROL_HEIGHT))
                .px(px(crate::app::consts::TITLEBAR_BUTTON_HORIZONTAL_PADDING))
                .rounded(px(crate::app::consts::TITLEBAR_BUTTON_RADIUS))
                .cursor_default()
                .when(cfg!(target_os = "windows"), |this| this.occlude())
                .hover(|this| this.bg(titlebar_button_hover_color()))
                .child(
                    crate::app::render::workarea_header::header_pins::header_icon(
                        "titlebar/external-link.svg",
                        color,
                    ),
                )
                .on_mouse_down(
                    MouseButton::Right,
                    crate::app::render::workarea_header::header_pins::header_slot_right_click(
                        crate::app::render::workarea_header::header_pins::HeaderSlot::Popout,
                        cx,
                    ),
                )
                .tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(tooltip.clone()).build(window, cx)
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |app, _, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        app.open_session_popup_for_shell(session_id, cx);
                    }),
                )
                .into_any_element(),
        )
    }
}

/// One of the header's icon buttons, with its tooltip.
fn header_button(
    id: &'static str,
    icon: &'static str,
    tooltip: &'static str,
    active: bool,
) -> gpui::Stateful<gpui::Div> {
    let color = if active {
        titlebar_active_text_color()
    } else {
        titlebar_icon_color()
    };
    div()
        .id(id)
        .flex_shrink_0()
        .size(px(26.0))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.0))
        .when(active, |button| button.bg(titlebar_button_hover_color()))
        .hover(|button| button.bg(titlebar_button_hover_color()))
        .child(titlebar_svg_icon(icon, 14.0, color))
        .tooltip(move |window, cx| gpui_component::tooltip::Tooltip::new(tooltip).build(window, cx))
}
