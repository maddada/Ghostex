//! The header's leading half: the window-control reserve, the panel/navigation controls that used
//! to open the titlebar's project slot, and the project → session breadcrumb.

use gpui::FontWeight;
use gpui::IntoElement;
use gpui::ObjectFit;
use gpui::ParentElement as _;
use gpui::Styled as _;
use gpui::StyledImage as _;
use gpui::div;
use gpui::img;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui_component::h_flex;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::*;

impl GhostexGpuiApp {
    /// CDXC:Titlebar 2026-09-20 DECISION:
    /// User: the breadcrumb names the project and the focused session of the sessions column, as
    /// the 2026-09-19 screens draw it. It is the open view's tab, not the breadcrumb, that says
    /// which view is on the right, so the title comes from the focused Agents pane's active tab.
    /// This supersedes the phase 2 rule that read the sidebar snapshot's focused row, which named
    /// the Browser's page ("Ghostex / Example Domain") as soon as a view took that row.
    ///
    /// CDXC:Titlebar 2026-09-20 WHY:
    /// The pane's title is the sidebar's own projection of that session (`agents_workspace.session`
    /// is written from the sidebar snapshot), so the header and the sidebar still cannot disagree
    /// about what a session is called; they only stop disagreeing about which session is current.
    pub(crate) fn workarea_header_session_title(&self) -> Option<String> {
        self.agents_workspace
            .session(self.workarea_header_session_id()?)
            .map(|session| session.title.clone())
            .filter(|title| !title.trim().is_empty())
    }

    /// The session the breadcrumb names: the focused Agents pane's active tab.
    pub(crate) fn workarea_header_session_id(&self) -> Option<TerminalSessionId> {
        let workspace = &self.agents_workspace;
        let pane_id = workspace
            .focus_mode_pane
            .into_iter()
            .chain(std::iter::once(workspace.focused_pane))
            .chain(workspace.rendered_leaf_order())
            .find(|pane_id| workspace.find_leaf(*pane_id).is_some())?;
        workspace.active_session_in_pane(pane_id)
    }

    /// Whether the chat the breadcrumb names shows its where pill with the project in its pane, so
    /// the breadcrumb leaves "<project> /" out (`set_where_pill_shows_project` in
    /// native_chat/where_pill.rs, where the user's decision lives). Each pane's chat answers for
    /// itself; the header names the focused pane's, so a split pane in terminal view keeps the
    /// project once it takes focus. A chat popped out to its own window is not in the pane.
    pub(crate) fn workarea_header_where_pill_shows_project(&self, cx: &gpui::App) -> bool {
        let Some(session_id) = self.workarea_header_session_id() else {
            return false;
        };
        self.agents_chat_mode_sessions.contains(&session_id)
            && !self.session_popup_shows(session_id)
            && self
                .native_chat_views
                .get(&session_id)
                .is_some_and(|chat| chat.read(cx).where_pill_shows_project)
    }

    /// CDXC:Titlebar 2026-09-10 WHY:
    /// The centered header gives the left region a zero flex basis; auto-sized descendants can retain their minimum measured width and collapse the project label even with space available.
    /// Both the breadcrumb and its label must grow into the width allocated by their parent.
    pub(crate) fn render_workarea_header_breadcrumb(
        &self,
        compact: bool,
        window: &gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let project_icon = self
            .latest_sidebar_project_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.project_icon_data_url.as_deref())
            .and_then(gpui_project_icon_image_from_data_url);
        let session_title = self.workarea_header_session_title();
        // Narrow drops the project half of the breadcrumb, and so does the chat's where pill naming
        // the project, but never leaves the header nameless: with no focused session the project
        // name is all there is to show.
        let show_project_name = (!compact && !self.workarea_header_where_pill_shows_project(cx))
            || session_title.is_none();
        let show_separator = show_project_name && session_title.is_some();
        /*
        CDXC:Titlebar 2026-09-20 WHY:
        With the sidebar expanded the window's top-left belongs to the sidebar's own Search row,
        which reserves the macOS traffic lights there. Collapsed, this header is what sits in that
        corner, so it draws the sidebar toggles at the Search row's `SIDEBAR_TOGGLE_LEADING_X`.
        Windows and Linux have no lights to clear; their caption buttons are trailing children of
        this same header. The reveal's edge zone overlays the workarea and takes no layout, so the
        inset is measured from the window edge. Since 2026-11-06 the macOS lights hide with the
        sidebar, and in full screen macOS draws them only in the title bar that slides down, so in
        both the toggles move left to the x they have on Windows and Linux
        (`window_controls_reserved`, `sidebar_toggle_leading_x`).
        */
        let leading_inset = if self.sidebar_collapsed {
            // Inside the work area frame's card the header starts one gutter plus the card's 1pt
            // border in from the window's edge, so the inset drops by both and the toggles land at
            // the window x `sidebar_toggle_leading_x` gives.
            // The rail stays left of the card when the sidebar hides (app/sidebar_rail/), and the
            // card starts at its edge with no gutter (`work_area_frame_left_gutter`).
            // With the rail on, the macOS lights sit the rail's width further right, over this
            // header while they show (`window_lights_rail_shift`), so it starts past them.
            (sidebar_toggle_leading_x(self.window_controls_reserved())
                + if self.window_controls_reserved() {
                    self.window_lights_rail_shift()
                } else {
                    0.0
                }
                - self.sidebar_rail_width()
                - self.work_area_frame_left_gutter(false)
                - WORK_AREA_FRAME_BORDER)
                .max(WORKAREA_HEADER_EDGE_PADDING)
        } else {
            WORKAREA_HEADER_EDGE_PADDING
        };
        h_flex()
            .pl(px(leading_inset))
            .flex_1()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .max_w(px(620.0))
            .min_w_0()
            .items_center()
            // While docked, the sidebar's Search row draws the toggle (native_sidebar/navigation.rs).
            // The docked Search row puts the button's top at 5pt, while this 36pt row centres the
            // 27pt button at 4.5pt; the half point keeps it from jumping when the sidebar collapses.
            // With the rail on, the bell is here, so it stays in reach while the sidebar is hidden
            // (the rail has no bell by default; CDXC:Sidebar 2026-11-20 in
            // native_sidebar/navigation.rs), and Toggle sidebar is the rail's top button instead
            // (CDXC:Sidebar 2026-11-21 in app/sidebar_rail/render.rs).
            .when(self.sidebar_collapsed, |this| {
                let rail = self.sidebar_rail_shown();
                let toggle = (!rail).then(|| {
                    self.render_sidebar_collapse_button(None, cx)
                        .into_any_element()
                });
                let bell = rail
                    .then(|| self.workarea_header_rail_bell(window, cx))
                    .flatten();
                // The window's traffic lights show here through the floating sidebar, so the
                // toggles under them draw nothing meanwhile (`window_lights_over_header`).
                let under_lights = self.window_lights_over_header();
                this.children(toggle.into_iter().chain(bell).map(|button| {
                    div()
                        .relative()
                        .top(px(0.5))
                        .when(under_lights, |button| button.invisible())
                        .child(button)
                }))
            })
            /*
            CDXC:Navigation 2026-08-19:
            Back/Forward sit LEFT of the project name, next to the sidebar
            toggle. Placing them after the name made them slide horizontally
            every time the active project's title changed length, which is
            exactly the kind of moving target a frequently clicked control must
            not be.
            */
            .child(self.render_titlebar_navigation_history_buttons(cx))
            // CDXC:Titlebar 2026-09-20 DECISION:
            // User: the Update button sits on the left just before the project name, so showing or hiding it only moves the project name and never shifts Back/Forward.
            // This supersedes the 2026-09-19 wording, which placed it after the compact view dropdown; the view panel's tab strip replaced that dropdown.
            .when(
                self.update_available
                    || self.update_downloading
                    || !self.updates_badge_items().is_empty(),
                |this| this.child(self.render_titlebar_update_button(cx)),
            )
            .child(
                h_flex()
                    .h(px(TITLEBAR_CONTROL_HEIGHT))
                    .flex_1()
                    .min_w_0()
                    .items_center()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .px(px(3.0))
                    .ml(px(5.0))
                    // The label's line box is as tall as the icon buttons and Inter's glyphs sit
                    // almost on their centre line; 1pt down lines the title up with the buttons'
                    // visual centre (measured 2026-10-16: with no offset it sat 1pt high, the
                    // old 3pt nudge from before Inter was bundled sat it 3pt low). Windows
                    // already used 1pt (c2acbf508), so every platform now shares it.
                    .relative()
                    .top(px(1.0))
                    .text_size(px(WORKAREA_HEADER_TITLE_TEXT_SIZE))
                    .line_height(px(TITLEBAR_CONTROL_HEIGHT))
                    .when_some(project_icon, |this, image| {
                        this.child(
                            img(image)
                                .size(px(16.0))
                                .mr(px(6.0))
                                .flex_shrink_0()
                                .rounded(px(4.0))
                                .object_fit(ObjectFit::Fill),
                        )
                    })
                    /*
                    CDXC:Titlebar 2026-09-20 DECISION:
                    User: the header names the project and the session as one breadcrumb, and drops
                    the project name and its slash first when the column is narrow. The project icon
                    always stays, because the sidebar no longer names the active project.
                    */
                    .when(show_project_name, |this| {
                        this.child(
                            div()
                                // Capped so a long project name cannot push the session title out
                                // of the breadcrumb, the way the old project slot was capped.
                                .max_w(px(210.0))
                                .flex_shrink_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(titlebar_project_text_color())
                                .child(self.project_name.clone()),
                        )
                    })
                    .when(show_separator, |this| {
                        this.child(
                            h_flex()
                                .flex_shrink_0()
                                .items_center()
                                .px(px(6.0))
                                .text_color(titlebar_disabled_text_color())
                                .child("/"),
                        )
                    })
                    .when_some(session_title, |this, title| {
                        this.child(
                            div()
                                .flex_shrink(1.0)
                                .min_w_0()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .text_color(titlebar_text_color())
                                .child(title),
                        )
                    }),
            )
    }

    /// The sidebar's bell, drawn in the header right of Toggle sidebar while the sidebar is hidden
    /// with the rail on: the sidebar's look at the header's own size (it does not zoom with the
    /// sidebar, like the toggle beside it), and no stop of the sidebar's focus order. None while
    /// the bell is off.
    fn workarea_header_rail_bell(
        &self,
        window: &gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) -> Option<gpui::AnyElement> {
        if !self.titlebar_notification_bell_visible() {
            return None;
        }
        let snapshot = self.native_sidebar.snapshot.as_ref()?;
        let mut appearance = crate::app::native_sidebar::appearance::SidebarAppearance::from_hud(
            &snapshot.hud,
            window,
        );
        appearance.scale = 1.0;
        Some(self.render_sidebar_notification_bell(&appearance, false, cx))
    }
}
