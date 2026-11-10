//! The page's root view: the shared native sidebar beside the work area, under a work area header that keeps the desktop's measurements, palette and icons. Chat and terminal switch through the surfaces' own buttons, as on the desktop (the composer's Terminal View, the terminal bar's Chat View); the header carries no toggle.
//!
//! CDXC:WebGpui 2026-09-22 DECISION: User: "just disable some buttons that don't apply for the web version (show them in ui but disabled)", and earlier "you can hide the stuff like Code/Docs/Automate in the web version". So Start, Open, Commit, the more menu and the panel toggles are drawn where the desktop draws them and do nothing, and the view tabs other than Agents are not drawn at all.
use gpui::{
    AnyElement, Context, Hsla, IntoElement, Render, SharedString, Window, div, prelude::*, px, rgb,
};
use gpui_component::ElementExt as _;
use gpui_component::h_flex;
use gpui_component::tooltip::Tooltip;

use crate::GhostexGpuiApp;
use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::native_sidebar::actions::NativeSidebarAction;

const WEB_DISABLED_REASON: &str = "Not available in the browser yet";

impl GhostexGpuiApp {
    fn open_session_titles(&self) -> Option<(String, String)> {
        let key = self.open_session.as_ref()?;
        self.gx_store
            .sidebar_view()
            .groups
            .iter()
            .find_map(|group| {
                group
                    .core
                    .sessions
                    .iter()
                    .find(|session| session.row.key.as_ref() == Some(key))
                    .map(|session| {
                        // A worktree session its parent project lists is the worktree's (gx-core
                        // sidebar_view/worktree_fold.rs), as the desktop header names it.
                        let project = match &session.row.worktree {
                            Some(worktree) if !worktree.name.is_empty() => worktree.name.clone(),
                            _ => group.core.title.clone(),
                        };
                        (project, session.row.display_title.clone())
                    })
            })
    }

    /// A header button the browser build has no behaviour for yet: drawn, dimmed, and explained on hover.
    fn render_disabled_header_button(
        id: &'static str,
        icon: &'static str,
        label: Option<&'static str>,
    ) -> AnyElement {
        let color: Hsla = titlebar_active_text_color().opacity(0.35);
        h_flex()
            .id(id)
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .px(px(7.0))
            .gap(px(5.0))
            .rounded(px(6.0))
            .items_center()
            .text_size(px(12.0))
            .text_color(color)
            .cursor_not_allowed()
            .child(titlebar_svg_icon(icon, 15.0, color))
            .when_some(label, |button, label| button.child(label))
            .tooltip(|window, cx| Tooltip::new(WEB_DISABLED_REASON).build(window, cx))
            .into_any_element()
    }

    /// Built-in extension `id` is on in the settings gxserver gave this page.
    fn web_built_in_enabled(id: &str) -> bool {
        ghostex_settings_catalog::built_in_extensions::enabled(
            crate::shared_settings::shared_sidebar_settings_snapshot().object(),
            id,
        )
    }

    fn render_workarea_header(&self, cx: &mut Context<Self>) -> AnyElement {
        let text = titlebar_active_text_color();
        let titles = self.open_session_titles();
        h_flex()
            .h(px(WORKAREA_HEADER_HEIGHT))
            .w_full()
            .flex_none()
            .px(px(WORKAREA_HEADER_EDGE_PADDING))
            .gap(px(8.0))
            .items_center()
            .border_b_1()
            .border_color(titlebar_popup_menu_border_color())
            .child(
                div()
                    .id("web-sidebar-toggle")
                    .p(px(4.0))
                    .rounded(px(6.0))
                    .cursor_pointer()
                    .hover(|button| button.bg(titlebar_popup_menu_hover_color()))
                    .child(titlebar_svg_icon(TITLEBAR_ICON_LAYOUT_SIDEBAR, 16.0, text))
                    .on_click(cx.listener(|app, _, _, cx| {
                        app.sidebar_collapsed = !app.sidebar_collapsed;
                        cx.notify();
                    })),
            )
            .when_some(titles, |header, (project, session)| {
                header.child(
                    h_flex()
                        .min_w_0()
                        .gap(px(6.0))
                        .text_size(px(12.5))
                        .overflow_hidden()
                        .child(
                            div()
                                .flex_none()
                                .text_color(text.opacity(0.6))
                                .child(SharedString::from(project)),
                        )
                        .child(div().flex_none().text_color(text.opacity(0.35)).child("/"))
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_color(text)
                                .child(SharedString::from(session)),
                        ),
                )
            })
            .child(div().flex_1())
            // Approvals put aside with Escape or Close, as on the desktop (notice_modal_host.rs).
            .children(self.render_approvals_waiting_pill(false, cx))
            // Start and Open follow the Actions and Open In extensions, as the desktop header does.
            .when(
                Self::web_built_in_enabled(ghostex_settings_catalog::built_in_extensions::ACTIONS),
                |header| {
                    header.child(Self::render_disabled_header_button(
                        "web-header-start",
                        TITLEBAR_ICON_PLAYER_PLAY,
                        Some("Start"),
                    ))
                },
            )
            .when(
                Self::web_built_in_enabled(ghostex_settings_catalog::built_in_extensions::OPEN_IN),
                |header| {
                    header.child(Self::render_disabled_header_button(
                        "web-header-open",
                        TITLEBAR_ICON_FOLDER_OPEN,
                        Some("Open"),
                    ))
                },
            )
            .child(self.render_web_git_button(cx))
            .children(self.render_web_session_details_button(cx))
            .child(Self::render_disabled_header_button(
                "web-header-more",
                TITLEBAR_ICON_DOTS,
                None,
            ))
            .child(Self::render_disabled_header_button(
                "web-header-panel-bottom",
                TITLEBAR_ICON_PANEL_BOTTOM,
                None,
            ))
            .child(self.render_web_changes_button(cx))
            .into_any_element()
    }

    /// The Session details button, as the desktop header draws it
    /// (`render/workarea_header/session_details_button.rs`): the open chat's context ring, which
    /// shows and hides that chat's Session details panel.
    fn render_web_session_details_button(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        // The page has no settings file; the handshake says what is on (`builtInFeatures`).
        if !crate::app::read_aloud::computer_feature_on(
            ghostex_settings_catalog::built_in_extensions::SESSION_DETAILS,
        ) {
            return None;
        }
        let session = self.open_session.as_ref()?;
        if self.web_shows_terminal(session) {
            return None;
        }
        let (percent, label) = {
            let view = self.native_chats.get(session)?.1.read(cx);
            if !view.session_details_available() {
                return None;
            }
            view.session_details_ring()
        };
        let open = self
            .session_details_choices
            .is_open(&session.to_sidebar_session_id());
        let key = session.to_sidebar_session_id();
        let bounds = self.session_details_button_bounds.clone();
        let text = titlebar_active_text_color();
        Some(
            div()
                .id("web-header-session-details")
                .flex()
                .flex_none()
                .items_center()
                .gap(px(5.0))
                .px(px(6.0))
                .py(px(3.0))
                .rounded(px(6.0))
                .cursor_pointer()
                .when(open, |button| button.bg(titlebar_popup_menu_hover_color()))
                .hover(|button| button.bg(titlebar_popup_menu_hover_color()))
                .child(
                    crate::app::native_chat::session_details::session_details_ring(
                        percent.unwrap_or(0.0) as f32,
                        titlebar_popup_menu_border_color(),
                        text,
                        14.0,
                    ),
                )
                .child(
                    crate::app::native_chat::session_details::session_details_ring_label(
                        label, text,
                    ),
                )
                .tooltip(move |window, cx| {
                    gpui_component::tooltip::Tooltip::new(if open {
                        "Hide session details"
                    } else {
                        "Show session details"
                    })
                    .build(window, cx)
                })
                .on_click(cx.listener(move |app, _, _, cx| {
                    // The click that just closed the floating card does not reopen it.
                    let open = app.session_details_choices.is_open(&key);
                    let reopening = !open
                        && app
                            .session_details_dismissed_at
                            .is_some_and(|at| at.elapsed() < std::time::Duration::from_millis(350));
                    if !reopening {
                        app.session_details_choices.set(&key, !open);
                        cx.notify();
                    }
                }))
                .on_prepaint(move |painted, _, _| bounds.set(painted))
                .into_any_element(),
        )
    }

    /// The right-panel button shows and hides the Changes view beside the chat; the page has no
    /// view panel, so this panel is where the desktop's Changes tab lives here.
    fn render_web_changes_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let text = titlebar_active_text_color();
        let open = self.changes_open;
        div()
            .id("web-header-panel-right")
            .size(px(TITLEBAR_CONTROL_HEIGHT))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(6.0))
            .cursor_pointer()
            .when(open, |button| button.bg(titlebar_popup_menu_hover_color()))
            .hover(|button| button.bg(titlebar_popup_menu_hover_color()))
            .child(titlebar_svg_icon(TITLEBAR_ICON_PANEL_RIGHT, 16.0, text))
            .tooltip(move |window, cx| {
                gpui_component::tooltip::Tooltip::new(if open {
                    "Hide Changes"
                } else {
                    "Show Changes"
                })
                .build(window, cx)
            })
            .on_click(cx.listener(|app, _, _, cx| app.web_toggle_changes(cx)))
            .into_any_element()
    }

    /// Commit opens the Git menu, as the desktop's Commit split button's main half does (`CDXC:Git 2026-09-20`).
    fn render_web_git_button(&self, cx: &mut Context<Self>) -> AnyElement {
        let text = titlebar_active_text_color();
        let recorded = self.web_host.git_button_bounds.clone();
        h_flex()
            .id("web-header-commit")
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .px(px(7.0))
            .gap(px(5.0))
            .rounded(px(6.0))
            .items_center()
            .text_size(px(12.0))
            .text_color(text)
            .cursor_pointer()
            .hover(|button| button.bg(titlebar_popup_menu_hover_color()))
            .child(titlebar_svg_icon(TITLEBAR_ICON_GIT_COMMIT, 15.0, text))
            .child("Commit")
            .child(
                gpui::canvas(
                    move |element_bounds, _, _| recorded.set(Some(element_bounds)),
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full(),
            )
            .on_click(cx.listener(move |app, _, window, cx| app.web_toggle_git_menu(window, cx)))
            .into_any_element()
    }

    /// Whether the work area draws `session`'s terminal: the page's Chat/Terminal choice, unless
    /// gx-core rules Chat View out for the session (an agentbox box's), which always shows its
    /// terminal.
    pub(crate) fn web_shows_terminal(&self, session: &ghostex_gx_core::SessionKey) -> bool {
        self.show_terminal
            || ghostex_gx_core::session_chat_view_unavailable(&self.gx_store.core, session)
    }

    pub(crate) fn web_show_terminal(&mut self, terminal: bool, cx: &mut Context<Self>) {
        self.show_terminal = terminal;
        if terminal && let Some(session) = self.open_session.clone() {
            self.ensure_terminal(&session, cx);
        }
        cx.notify();
    }
}

impl Render for GhostexGpuiApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if crate::app::collab_guest::guest().is_some() {
            return self.render_collab_guest_shell(window, cx);
        }
        self.web_sync_terminal_zmx_visibility(cx);
        self.web_report_chat_grid_claims(cx);
        let sidebar = (!self.sidebar_collapsed).then(|| self.render_native_sidebar(window, cx));
        let status = self.gx_store.status.clone();
        let open = self.open_session.clone();
        let shows_terminal = open
            .as_ref()
            .is_some_and(|session| self.web_shows_terminal(session));
        // Read aloud's chat on screen: the chat the page shows (contract section 5).
        crate::app::gx_chat::read_aloud_on_screen(
            open.as_ref()
                .filter(|_| !shows_terminal)
                .and_then(|session| self.native_chats.get(session))
                .and_then(|(_, view)| {
                    view.read(cx)
                        .runtime
                        .as_ref()
                        .map(|runtime| runtime.key().to_string())
                }),
        );
        let surface: Option<AnyElement> = open.as_ref().and_then(|session| {
            if shows_terminal {
                // The desktop's Agents pane frame (render/terminal_body_slot.rs): the terminal
                // padding and Terminal width (Full, Custom, Match chat width) from Settings.
                let (horizontal_padding, vertical_padding, width) =
                    crate::shared_settings::shared_sidebar_settings_snapshot()
                        .terminal_pane_layout(true);
                self.terminals.get(session).map(|view| {
                    crate::app::render::terminal_content_layout::terminal_content_frame(
                        div().size_full().child(view.clone()),
                        horizontal_padding,
                        vertical_padding,
                        width,
                    )
                })
            } else {
                self.native_chats
                    .get(session)
                    .map(|(_, view)| view.clone().into_any_element())
            }
        });
        // CDXC:Notifications 2026-11-01 DECISION: User (Q929): the finished-work border is a setting, "disabled by default for new users too", with its own colour; off, the edge stays transparent (the desktop's rule is `show_finished_work_border` in helpers/project/colors.rs). This supersedes the 2026-10-09 Workarea decision that always drew the 1px attention line; when on, the work area's top edge takes the chosen colour while the open session has attention, and the 1px edge is always there, transparent otherwise, so the line never moves the header.
        let settings = crate::shared_settings::shared_sidebar_settings_snapshot();
        let border_rgb = (settings.object().get("showFinishedWorkBorder")
            == Some(&serde_json::Value::Bool(true)))
        .then(|| {
            settings
                .object()
                .get("finishedWorkBorderColor")
                .and_then(serde_json::Value::as_str)
                .map(|value| value.trim().trim_start_matches('#'))
                .filter(|hex| hex.len() == 6)
                .and_then(|hex| u32::from_str_radix(hex, 16).ok())
                .unwrap_or(0x95d7f6)
        });
        let attention = border_rgb.is_some()
            && open.as_ref().is_some_and(|session| {
                self.gx_store
                    .core
                    .presentation()
                    .session(session)
                    .is_some_and(|session| session.activity.as_str() == "attention")
            });
        // The Session details panel belongs to the chat the page shows, while its button is on.
        let anchor = self.session_details_button_bounds.clone();
        for (key, (_, view)) in &self.native_chats {
            let wanted = !shows_terminal
                && open.as_ref() == Some(key)
                && self
                    .session_details_choices
                    .is_open(&key.to_sidebar_session_id());
            view.update(cx, |view, cx| {
                view.set_session_details_wanted(wanted, Some(anchor.clone()), cx)
            });
        }
        let header = self.render_workarea_header(cx);
        let changes = self.render_web_changes_panel(window, cx);
        let side_chat = self.render_web_side_chat_panel(cx);
        let terminal_bar =
            (shows_terminal && surface.is_some()).then(|| self.render_terminal_action_bar(cx));
        div()
            .id("ghostex-web-shell")
            .size_full()
            .relative()
            .flex()
            .bg(titlebar_background())
            .on_action(
                cx.listener(|app, action: &NativeSidebarAction, window, cx| {
                    app.handle_native_sidebar_action(action, window, cx)
                }),
            )
            .when_some(sidebar, |shell, sidebar| {
                shell
                    .child(
                        div()
                            .h_full()
                            .w(px(self.sidebar_width))
                            .flex_none()
                            .child(sidebar),
                    )
                    .child(
                        div()
                            .h_full()
                            .w(px(1.0))
                            .flex_none()
                            .bg(titlebar_popup_menu_border_color()),
                    )
            })
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .border_t_1()
                    .border_color(if let Some(border_rgb) = border_rgb.filter(|_| attention) {
                        rgb(border_rgb).into()
                    } else {
                        gpui::transparent_black()
                    })
                    .child(header)
                    .child(
                        div()
                            .flex_1()
                            .min_h_0()
                            .flex()
                            .child(match surface {
                                Some(surface) => div()
                                    .flex_1()
                                    .min_w_0()
                                    .h_full()
                                    .overflow_hidden()
                                    .child(surface),
                                None => div()
                                    .flex_1()
                                    .min_w_0()
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .text_color(rgb(0x8a8a8a))
                                    .child(
                                        status.unwrap_or_else(|| "Select a session".to_string()),
                                    ),
                            })
                            .children(changes)
                            .children(side_chat),
                    )
                    .children(terminal_bar),
            )
            .children(self.render_web_git_menu(cx))
            .children(self.render_web_toasts())
            .into_any_element()
    }
}
