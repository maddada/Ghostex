//! What a session's own window draws (its header and the session), and the card its pane shows
//! meanwhile.

use gpui::prelude::FluentBuilder as _;
use gpui::{
    AnyElement, Context, Entity, FontWeight, InteractiveElement as _, IntoElement, MouseButton,
    MouseDownEvent, ParentElement as _, Render, SharedString, StatefulInteractiveElement as _,
    Styled as _, Subscription, WeakEntity, Window, div, px,
};

use super::pin_supported;
use crate::GhostexGpuiApp;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::render::sleeping_card::{view_card_button, view_card_frame};
use crate::app::render::terminal_content_layout::terminal_content_frame;
#[cfg(target_os = "macos")]
use crate::app::render::window_drag_region::window_drag_region;

/// The header row's height: the work area header's, so the traffic lights sit on its centreline.
const HEADER_HEIGHT: f32 = 38.0;
/// Room for the traffic lights on macOS; elsewhere the system titlebar sits above the header.
#[cfg(target_os = "macos")]
const HEADER_LEADING: f32 = 80.0;
#[cfg(not(target_os = "macos"))]
const HEADER_LEADING: f32 = 12.0;

/// The window's root: it draws the app's session and redraws when what it shows changes.
pub(crate) struct SessionPopupRoot {
    app: WeakEntity<GhostexGpuiApp>,
    row_id: String,
    /// What the window last drew from; see `popup_signature`.
    drawn: Option<PopupSignature>,
    /// The window's own focus, so its keys reach the handlers below before the chat or the
    /// terminal has been clicked.
    focus: gpui::FocusHandle,
    _subscriptions: Vec<Subscription>,
}

/// What the window's own drawing depends on. The chat and the terminal are views of their own that
/// redraw themselves, so the window ignores the app's other notifications (terminal output, chat
/// streaming, indicator ticks), as the board's window does (CDXC:ProjectBoard 2026-10-13).
type PopupSignature = (
    Option<TerminalSessionId>,
    bool,
    Option<gpui::EntityId>,
    Option<gpui::EntityId>,
    Option<TerminalSessionPresentationState>,
    String,
    bool,
    bool,
    bool,
);

fn popup_signature(app: &GhostexGpuiApp, row_id: &str) -> Option<PopupSignature> {
    let popup = app.session_popups.get(row_id)?;
    let shell = app.session_popup_shell_session(&popup.key);
    let session = shell.and_then(|id| app.agents_workspace.session(id));
    Some((
        shell,
        shell.is_some_and(|id| app.agents_chat_mode_sessions.contains(&id)),
        shell
            .and_then(|id| app.native_chat_views.get(&id))
            .map(Entity::entity_id),
        shell
            .and_then(|id| app.agents_gpui_engine_terminals.get(&id))
            .map(|record| record.view.entity_id()),
        session.map(|session| session.presentation_state),
        session
            .map(|session| session.title.clone())
            .unwrap_or_default(),
        popup.pinned,
        popup.expanded,
        app.gx_store_sidebar_list_ready(),
    ))
}

impl SessionPopupRoot {
    pub(crate) fn new(
        app: Entity<GhostexGpuiApp>,
        row_id: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let observe_row = row_id.clone();
        let bounds_row = row_id.clone();
        let observed = cx.observe(&app, move |root: &mut Self, app, cx| {
            let signature = popup_signature(app.read(cx), &observe_row);
            if root.drawn != signature {
                root.drawn = signature;
                cx.notify();
            }
        });
        let bounds_app = app.downgrade();
        let bounds = cx.observe_window_bounds(window, move |_, window, cx| {
            let frame = window.bounds();
            let display = window.display(cx).map(|display| display.id());
            let row_id = bounds_row.clone();
            let _ = bounds_app.update(cx, |app, _| {
                app.session_popup_note_bounds(&row_id, frame, display)
            });
        });
        Self {
            drawn: popup_signature(app.read(cx), &row_id),
            app: app.downgrade(),
            row_id,
            focus: {
                let focus = cx.focus_handle();
                window.focus(&focus, cx);
                focus
            },
            _subscriptions: vec![observed, bounds],
        }
    }

    /// Cmd+W (and the menu's Close Pane) in this window: the window closes and the session goes
    /// back to its pane, as its close button does.
    fn close_window(&mut self, cx: &mut Context<Self>) {
        let Some(app) = self.app.upgrade() else {
            return;
        };
        let row_id = self.row_id.clone();
        app.update(cx, |app, cx| app.close_session_popup(&row_id, cx));
    }

    /// The Ghostex hotkeys pressed in this window. Pop Out Pane brings the session back; the rest
    /// run in the main window, as the floating sidebar's do (floating_reveal/window.rs).
    fn run_hotkey(
        &mut self,
        action: &crate::app::hotkeys::RunConfiguredGhostexHotkey,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(app) = self.app.upgrade() else {
            return;
        };
        let row_id = self.row_id.clone();
        match action.action_id.as_str() {
            "popOutPane" => {
                app.update(cx, |app, cx| app.bring_back_session_popup(&row_id, cx));
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
        let row_id = self.row_id.clone();
        let body = app.update(cx, |app, cx| {
            app.render_session_popup_window(&row_id, window, cx)
        });
        div()
            .size_full()
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::run_hotkey))
            .on_action(cx.listener(
                |root, _: &crate::app::actions::CloseFocusedSurface, _, cx| root.close_window(cx),
            ))
            .on_action(cx.listener(
                |root, _: &crate::app::actions::CloseFocusedSurfaceMenuOnly, _, cx| {
                    root.close_window(cx)
                },
            ))
            .child(body)
            .into_any_element()
    }
}

impl GhostexGpuiApp {
    /// The window's header and the session under it.
    fn render_session_popup_window(
        &mut self,
        row_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(popup) = self.session_popups.get(row_id) else {
            return div().into_any_element();
        };
        let key = popup.key.clone();
        let (expanded, pinned) = (popup.expanded, popup.pinned);
        let shell = self.session_popup_shell_session(&key);
        let title = self.session_popup_title(&key);
        let agent_icon = shell
            .and_then(|id| self.agents_workspace.session(id))
            .and_then(|session| session.agent_icon);
        let chat = shell.is_some_and(|id| self.agents_chat_mode_sessions.contains(&id));
        window.set_window_title(&title);
        let body = match shell {
            Some(session_id) if chat => self.render_session_chat_surface_content(session_id),
            Some(session_id) => self.render_session_popup_terminal(row_id, session_id, cx),
            None if !self.gx_store_sidebar_list_ready() => self.render_session_chat_skeleton(),
            None => self.render_session_popup_card(
                row_id,
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
            .id("session-popup-window")
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(workspace_background_color())
            .child(
                self.render_session_popup_header(row_id, &title, agent_icon, expanded, pinned, cx),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .min_w_0()
                    .relative()
                    .bg(background)
                    .child(body),
            )
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
                format!("{} is asleep", self.session_popup_title_for(row_id)),
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

    fn session_popup_title_for(&self, row_id: &str) -> String {
        self.session_popups
            .get(row_id)
            .map(|popup| self.session_popup_title(&popup.key))
            .unwrap_or_default()
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
                                .id("session-popup-card-button")
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

    /// The window's header: the agent and title, then Pin, Bring Back and Expand or Shrink. On
    /// macOS it is the titlebar (the traffic lights sit at its left and a drag of it moves the
    /// window); elsewhere it sits under the system titlebar.
    fn render_session_popup_header(
        &self,
        row_id: &str,
        title: &str,
        agent_icon: Option<&'static str>,
        expanded: bool,
        pinned: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let text = titlebar_project_text_color();
        let icon = titlebar_icon_color();
        let pin_row = row_id.to_string();
        let back_row = row_id.to_string();
        let size_row = row_id.to_string();
        let row = div()
            .id("session-popup-header")
            .h(px(HEADER_HEIGHT))
            .w_full()
            .flex_shrink_0()
            .flex()
            .items_center()
            .gap(px(6.0))
            .pl(px(HEADER_LEADING))
            .pr(px(8.0))
            .border_b_1()
            .border_color(chrome_ink().opacity(0.08))
            .when_some(
                agent_icon.and_then(workspace_tab_agent_icon_path),
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
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(13.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(text)
                    .child(title.to_string()),
            )
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
                            app.toggle_session_popup_pin(&pin_row, cx);
                        }),
                    ),
                )
            })
            .child(
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
                        app.toggle_session_popup_expanded(&size_row, window, cx);
                    }),
                ),
            );
        #[cfg(target_os = "macos")]
        let row = window_drag_region(row);
        row.into_any_element()
    }

    /// The pane's card while its session is in its own window, in the board's style
    /// (`render_native_kanban_detached_card`).
    pub(crate) fn render_session_popup_placeholder(
        &self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let ink = chrome_ink();
        let row_id = self
            .session_popup_for_shell(session_id)
            .map(|popup| popup.key.to_sidebar_session_id())
            .unwrap_or_default();
        let show_row = row_id.clone();
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
                            .child(SharedString::from(format!(
                                "{title} is open in its own window"
                            ))),
                    )
                    .child(
                        div()
                            .mt(px(6.0))
                            .text_size(px(12.5))
                            .line_height(px(18.0))
                            .text_color(ink.opacity(0.55))
                            .child("Closing that window brings it back here."),
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
                                        if let Some(key) = app
                                            .session_popups
                                            .get(&show_row)
                                            .map(|popup| popup.key.clone())
                                        {
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
    /// The work area header's Open in New Window button, for the focused pane's session.
    pub(crate) fn render_workarea_header_session_popup_button(
        &self,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let session_id = self.session_popup_focused_target()?;
        let session = self.agents_workspace.session(session_id)?;
        if session.agent_icon == Some("browser") {
            return None;
        }
        let tooltip = titlebar_tooltip_label("Open in New Window", "popOutPane");
        let color = titlebar_icon_color();
        Some(
            div()
                .id("ghostex-gpui-workarea-header-session-popup")
                .role(gpui::Role::Button)
                .aria_label("Open in New Window")
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
                .child(titlebar_svg_icon("titlebar/external-link.svg", 14.0, color))
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
