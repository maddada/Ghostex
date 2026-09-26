// C1 wave-4 deferred split: apps/desktop/src/app/titlebar.rs (~3.9k lines)
// further divided into responsibility-scoped submodules, pure move (the
// only edit from the original app/titlebar.rs body is wrapping each group
// of `impl GhostexGpuiApp` methods in its own impl block; multiple impl
// blocks for the same type across files is the established pattern used by
// every sibling file in apps/desktop/src/app/). This file holds the browser toolbar renderer.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: titlebar menus, popups, actions, and titlebar render_* builders

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::ParentElement as _;
use gpui::Styled as _;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui::rgb;
use gpui_component::h_flex;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn render_browser_toolbar(
        &self,
        pane_id: BrowserPaneId,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        /*
        CDXC:Browser 2026-06-14-17:42:
        The GPUI browser pane needs the same address toolbar as the macOS app, implemented only with GPUI chrome: a black row, stateless Back/Forward/Reload controls with no Back/Forward history toggles, a lock-or-globe address field that restores the current URL on empty commits, and the browser right-control group while preserving non-empty address commits inside the embedded CEF browser. Toolbar actions use the same full-height segmented button chrome as tab-bar actions.

        CDXC:Browser 2026-06-15-01:52:
        GitHub disallows the injected feedback tool, so the GPUI toolbar must render the feedback button disabled on github.com pages and expose the tooltip "This site disallows using this tool" instead of letting the user start an unsupported tool action.

        CDXC:Browser 2026-06-22-08:08:
        Browser Back and Forward controls must read their enabled state from the selected loaded tab's existing CEF surface and must no-op when that surface cannot navigate. Reload must call CEF `reload()` on the selected loaded surface instead of loading the shell URL again, so Chromium keeps ownership of history, POST/cache behavior, and address-only placeholder tabs remain unloaded.

        CDXC:Browser 2026-06-22-11:50:
        The right-side Browser controls follow current macOS parity: zoom reset appears only when the active CEF surface is zoomed, the feedback button launches Agentation, History uses the app-modal host and Profile uses the shared GPUI popup window, DevTools toggles through the active CEF surface, and the removed Appearance control does not reserve toolbar space or hit area.

        CDXC:Browser 2026-06-23-11:04:
        The Browser feedback toolbar starts Agentation through CEF main-frame JavaScript injection. Keep github.com and *.github.com disabled before injection, and keep the toolbar surface status private by showing only bounded page-data-free notifications for missing CEF surfaces or frames.
        */
        let address_value = self.browser_tabs.address_value_for_pane(pane_id);
        let remote_machine_name = self
            .browser_tabs
            .active_tab_for_pane(pane_id)
            .and_then(|tab| tab.remote_machine_id.as_deref())
            .map(|id| {
                gpui_remote_machine_name_from_settings(id)
                    .unwrap_or_else(|| "Remote computer".into())
            });
        let feedback_tool_unavailable = browser_feedback_tool_unavailable_url(&address_value);
        let feedback_tooltip = feedback_tool_unavailable
            .then_some(BROWSER_FEEDBACK_TOOL_UNAVAILABLE_TOOLTIP)
            .unwrap_or(BROWSER_FEEDBACK_TOOL_AGENTATION_LABEL);
        let (is_loading, runtime_can_go_back, runtime_can_go_forward) = self
            .browser_tabs
            .active_tab_for_pane(pane_id)
            .map(|tab| {
                (
                    tab.runtime_is_loading,
                    tab.runtime_can_go_back,
                    tab.runtime_can_go_forward,
                )
            })
            .unwrap_or((false, false, false));
        let active_browser_surface = self.browser_surface_for_pane(pane_id);
        let can_go_back = active_browser_surface
            .as_ref()
            .is_some_and(|surface| runtime_can_go_back || surface.read(cx).can_go_back());
        let can_go_forward = active_browser_surface
            .as_ref()
            .is_some_and(|surface| runtime_can_go_forward || surface.read(cx).can_go_forward());
        let can_reload = active_browser_surface.is_some();
        let reload_action = if is_loading {
            BrowserToolbarAction::StopLoading
        } else {
            BrowserToolbarAction::Reload
        };
        let is_page_zoomed = active_browser_surface
            .as_ref()
            .is_some_and(|surface| surface.read(cx).is_zoomed());
        let zoom_reset_tooltip = active_browser_surface.as_ref().map(|surface| {
            gpui::SharedString::from(format!(
                "Reset Page Zoom ({}%)",
                (1.2_f64.powf(surface.read(cx).zoom_level()) * 100.0).round() as i32
            ))
        });
        /*
        CDXC:Browser 2026-07-27:
        A remembered Block would otherwise be unrecoverable: the page just
        fails and no prompt returns. Show a reset control exactly while the
        active tab's origin has a stored microphone/camera answer, so the site
        can be asked again.
        */
        let media_permission_reset_tooltip = self
            .browser_media_permission_reset_target(pane_id)
            .map(|(_, origin)| {
                gpui::SharedString::from(format!(
                    "Reset Microphone and Camera Access ({})",
                    gpui_browser_media_permission_display_origin(&origin)
                ))
            });
        h_flex()
            .id(format!("ghostex-gpui-browser-toolbar-{}", pane_id.0))
            .flex_shrink_0()
            .h(px(BROWSER_TOOLBAR_HEIGHT))
            .w_full()
            .items_center()
            .pl(px(BROWSER_TOOLBAR_LEADING_PADDING))
            // Lined up with the panel toggles in the strip above (WORKAREA_HEADER_PINNED_GAP).
            .pr(px(WORKAREA_HEADER_EDGE_PADDING + WORKAREA_HEADER_PINNED_GAP))
            .bg(browser_toolbar_background())
            .border_b_1()
            .border_color(chrome_color(0x252525, 0xd4d4d4))
            .when_some(remote_machine_name, |bar, name| {
                bar.child(
                    h_flex()
                        .max_w(px(160.0))
                        .flex_shrink_0()
                        .px(px(9.0))
                        .gap(px(5.0))
                        .items_center()
                        .child(
                            svg()
                                .path(BROWSER_ICON_WORLD)
                                .size(px(13.0))
                                .text_color(rgb(0x7acb9d)),
                        )
                        .child(
                            div()
                                .min_w_0()
                                .truncate()
                                .text_size(px(11.0))
                                .text_color(chrome_color(0xb7b7b7, 0x525252))
                                .child(name),
                        ),
                )
            })
            .child(
                h_flex()
                    .items_center()
                    .gap(px(BROWSER_TOOLBAR_ITEM_GAP))
                    .child(self.render_browser_toolbar_button(
                        "back",
                        TITLEBAR_ICON_CHEVRON_LEFT,
                        can_go_back,
                        None,
                        BrowserToolbarAction::Back,
                        pane_id,
                        cx,
                    ))
                    .child(self.render_browser_toolbar_button(
                        "forward",
                        BROWSER_ICON_CHEVRON_RIGHT,
                        can_go_forward,
                        None,
                        BrowserToolbarAction::Forward,
                        pane_id,
                        cx,
                    ))
                    .child(self.render_browser_toolbar_button(
                        "reload",
                        BROWSER_ICON_RELOAD,
                        can_reload,
                        None,
                        reload_action,
                        pane_id,
                        cx,
                    ))
                    .child(self.render_browser_toolbar_button(
                        "home",
                        BROWSER_ICON_HOME,
                        true,
                        None,
                        BrowserToolbarAction::Home,
                        pane_id,
                        cx,
                    )),
            )
            .child(div().flex_shrink_0().w(px(BROWSER_TOOLBAR_ADDRESS_GAP)))
            .child(self.render_browser_address_field(pane_id, cx))
            .child(
                div()
                    .flex_shrink_0()
                    .w(px(BROWSER_TOOLBAR_ADDRESS_RIGHT_GAP)),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap(px(BROWSER_TOOLBAR_ITEM_GAP))
                    .when(is_page_zoomed, |this| {
                        this.child(self.render_browser_toolbar_button(
                            "reset-zoom",
                            BROWSER_ICON_SEARCH,
                            true,
                            zoom_reset_tooltip,
                            BrowserToolbarAction::ResetZoom,
                            pane_id,
                            cx,
                        ))
                    })
                    .when_some(media_permission_reset_tooltip, |this, tooltip| {
                        this.child(self.render_browser_toolbar_button(
                            "reset-media-permissions",
                            BROWSER_ICON_MICROPHONE,
                            true,
                            Some(tooltip),
                            BrowserToolbarAction::ResetMediaPermissions,
                            pane_id,
                            cx,
                        ))
                    })
                    .child(self.render_browser_toolbar_button(
                        "agentation",
                        BROWSER_ICON_POINTER,
                        !feedback_tool_unavailable,
                        Some(feedback_tooltip.into()),
                        BrowserToolbarAction::FeedbackTool,
                        pane_id,
                        cx,
                    ))
                    .child(self.render_browser_toolbar_button(
                        "history",
                        BROWSER_ICON_HISTORY,
                        true,
                        Some("History".into()),
                        BrowserToolbarAction::HistoryMenu,
                        pane_id,
                        cx,
                    ))
                    .child(self.render_browser_toolbar_button(
                        "profile",
                        BROWSER_ICON_USER_CIRCLE,
                        true,
                        Some("Browser Profile".into()),
                        BrowserToolbarAction::ProfileMenu,
                        pane_id,
                        cx,
                    ))
                    .child(self.render_browser_toolbar_button(
                        "devtools",
                        BROWSER_ICON_TOOLS,
                        true,
                        Some("Toggle DevTools".into()),
                        BrowserToolbarAction::DevTools,
                        pane_id,
                        cx,
                    ))
                    .child(self.render_browser_toolbar_overflow_button(pane_id, cx)),
            )
    }
}
