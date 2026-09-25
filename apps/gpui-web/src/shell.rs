//! The page's root view: the shared native sidebar beside the work area, under a work area header that keeps the desktop's measurements, palette and icons. Chat and terminal switch through the surfaces' own buttons, as on the desktop (the composer's Terminal View, the terminal bar's Chat View); the header carries no toggle.
//!
//! CDXC:WebGpui 2026-09-22 DECISION: User: "just disable some buttons that don't apply for the web version (show them in ui but disabled)", and earlier "you can hide the stuff like Code/Docs/Automate in the web version". So Start, Open, Commit, the more menu and the panel toggles are drawn where the desktop draws them and do nothing, and the view tabs other than Agents are not drawn at all.
use gpui::{
    AnyElement, Context, Hsla, IntoElement, Render, SharedString, Window, div, prelude::*, px, rgb,
};
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
        self.gx_store.sidebar_view().groups.iter().find_map(|group| {
            group
                .core
                .sessions
                .iter()
                .find(|session| session.row.key.as_ref() == Some(key))
                .map(|session| (group.core.title.clone(), session.row.display_title.clone()))
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
                        .child(div().flex_none().text_color(text.opacity(0.6)).child(SharedString::from(project)))
                        .child(div().flex_none().text_color(text.opacity(0.35)).child("/"))
                        .child(div().min_w_0().truncate().text_color(text).child(SharedString::from(session))),
                )
            })
            .child(div().flex_1())
            .child(Self::render_disabled_header_button("web-header-start", TITLEBAR_ICON_PLAYER_PLAY, Some("Start")))
            .child(Self::render_disabled_header_button("web-header-open", TITLEBAR_ICON_FOLDER_OPEN, Some("Open")))
            .child(self.render_web_git_button(cx))
            .child(Self::render_disabled_header_button("web-header-more", TITLEBAR_ICON_DOTS, None))
            .child(Self::render_disabled_header_button("web-header-panel-bottom", TITLEBAR_ICON_PANEL_BOTTOM, None))
            .child(Self::render_disabled_header_button("web-header-panel-right", TITLEBAR_ICON_PANEL_RIGHT, None))
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
                gpui::canvas(move |element_bounds, _, _| recorded.set(Some(element_bounds)), |_, _, _, _| {})
                    .absolute()
                    .size_full(),
            )
            .on_click(cx.listener(move |app, _, window, cx| app.web_toggle_git_menu(window, cx)))
            .into_any_element()
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
        let sidebar = (!self.sidebar_collapsed).then(|| self.render_native_sidebar(window, cx));
        let status = self.gx_store.status.clone();
        let open = self.open_session.clone();
        let surface: Option<AnyElement> = open.as_ref().and_then(|session| {
            if self.show_terminal {
                self.terminals.get(session).map(|view| view.clone().into_any_element())
            } else {
                self.native_chats.get(session).map(|(_, view)| view.clone().into_any_element())
            }
        });
        let header = self.render_workarea_header(cx);
        let terminal_bar = (self.show_terminal && surface.is_some())
            .then(|| self.render_terminal_action_bar(cx));
        div()
            .id("ghostex-web-shell")
            .size_full()
            .relative()
            .flex()
            .bg(titlebar_background())
            .on_action(cx.listener(|app, action: &NativeSidebarAction, window, cx| {
                app.handle_native_sidebar_action(action, window, cx)
            }))
            .when_some(sidebar, |shell, sidebar| {
                shell
                    .child(div().h_full().w(px(self.sidebar_width)).flex_none().child(sidebar))
                    .child(div().h_full().w(px(1.0)).flex_none().bg(titlebar_popup_menu_border_color()))
            })
            .child(
                div()
                    .flex_1()
                    .h_full()
                    .min_w_0()
                    .flex()
                    .flex_col()
                    .child(header)
                    .child(match surface {
                        Some(surface) => div().flex_1().min_h_0().overflow_hidden().child(surface),
                        None => div()
                            .flex_1()
                            .flex()
                            .items_center()
                            .justify_center()
                            .text_color(rgb(0x8a8a8a))
                            .child(status.unwrap_or_else(|| "Select a session".to_string())),
                    })
                    .children(terminal_bar),
            )
            .children(self.render_web_git_menu(cx))
            .children(self.render_web_toasts())
    }
}
