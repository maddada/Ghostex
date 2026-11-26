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
use gpui_component::h_flex;
use gpui_component::tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement};

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
        The right-side Browser controls follow current macOS parity: zoom reset appears only when the active CEF surface is zoomed, the feedback button launches Agentation, History uses the app-modal host and Profile uses the shared GPUI popup window, and the removed Appearance control does not reserve toolbar space or hit area.

        CDXC:Browser 2026-06-23-11:04:
        The Browser feedback toolbar starts Agentation through CEF main-frame JavaScript injection. Keep github.com and *.github.com disabled before injection, and keep the toolbar surface status private by showing only bounded page-data-free notifications for missing CEF surfaces or frames.
        */
        // The link page draws this toolbar for a pane id no Browser pane has (CDXC:Browser
        // 2026-11-22 in app/custom_links/page.rs): its page state comes from there.
        let link_page = pane_id == crate::app::custom_links::CUSTOM_LINK_PAGE_PANE;
        let link_state = link_page.then(|| self.custom_link_toolbar_page());
        let address_value = match &link_state {
            Some((address, ..)) => address.clone(),
            None => self.browser_tabs.address_value_for_pane(pane_id),
        };
        /*
        CDXC:Browser 2026-11-15 DECISION:
        User (of the "From this computer" chip): "please just show the icon of the machine here,
        don't show this text. tooltip should say the machine's name". A remote project's Browser
        shows only the logo of the computer the page loads from (the remote's while the page goes
        through it or has no page yet, this computer's while it opens from here, helpers/remote/
        browser_route.rs), a cloud for a Cloud Box, and the computer's name as its tooltip.
        */
        let route_mark = (!link_page)
            .then(|| self.browser_tabs.active_tab_for_pane(pane_id))
            .flatten()
            .and_then(|tab| Some((tab.remote_machine_id.as_deref()?, tab.url.as_str())))
            .map(|(id, url)| {
                let from_this_computer = gpui_remote_browser_route_for_machine(id)
                    .url_goes_through_machine(url)
                    == Some(false);
                self.browser_source_mark(if from_this_computer { None } else { Some(id) })
            });
        let feedback_tool_unavailable = browser_feedback_tool_unavailable_url(&address_value);
        let feedback_tooltip: gpui::SharedString = if feedback_tool_unavailable {
            BROWSER_FEEDBACK_TOOL_UNAVAILABLE_TOOLTIP.into()
        } else {
            // The Annotate hotkey toggles it too (CDXC:Docs 2026-11-14 DECISION in
            // packages/settings-catalog/src/hotkey_definitions.rs).
            titlebar_tooltip_label(BROWSER_FEEDBACK_TOOL_AGENTATION_LABEL, "toggleAnnotate")
        };
        let (is_loading, runtime_can_go_back, runtime_can_go_forward) = match &link_state {
            Some((_, loading, back, forward)) => (*loading, *back, *forward),
            None => self
                .browser_tabs
                .active_tab_for_pane(pane_id)
                .map(|tab| {
                    (
                        tab.runtime_is_loading,
                        tab.runtime_can_go_back,
                        tab.runtime_can_go_forward,
                    )
                })
                .unwrap_or((false, false, false)),
        };
        let active_browser_surface = if link_page {
            self.custom_link_surface()
        } else {
            self.browser_surface_for_pane(pane_id)
        };
        let can_go_back = active_browser_surface
            .as_ref()
            .is_some_and(|surface| runtime_can_go_back || surface.read(cx).can_go_back());
        let can_go_forward = active_browser_surface
            .as_ref()
            .is_some_and(|surface| runtime_can_go_forward || surface.read(cx).can_go_forward());
        let can_reload = active_browser_surface.is_some();
        let can_open_externally = self.browser_pane_can_open_externally(pane_id);
        let open_externally_tooltip: gpui::SharedString = if can_open_externally {
            self.view_pop_out_label(TitlebarMode::Browser, false).into()
        } else {
            "This view has no page to pop out yet".into()
        };
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
        let media_permission_reset_tooltip = (!link_page)
            .then(|| self.browser_media_permission_reset_target(pane_id))
            .flatten()
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
            .when_some(route_mark, |bar, mark| {
                let tooltip: gpui::SharedString = mark.name.into();
                bar.child(
                    h_flex()
                        .id(format!("ghostex-gpui-browser-route-chip-{}", pane_id.0))
                        .managed_tooltip_with_placement(
                            ManagedTooltipPlacement::Below,
                            move |window, cx| titlebar_tooltip(tooltip.clone(), window, cx),
                        )
                        .flex_shrink_0()
                        .px(px(9.0))
                        .items_center()
                        .child(
                            svg()
                                .path(mark.asset)
                                .size(px(14.0))
                                .text_color(chrome_color(0xb7b7b7, 0x525252)),
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
            .child(if link_page {
                self.render_custom_link_address_field(cx)
            } else {
                self.render_browser_address_field(pane_id, cx)
            })
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
                    .child(self.render_browser_device_view_button(pane_id, cx))
                    .child(self.render_browser_toolbar_button(
                        "agentation",
                        BROWSER_ICON_POINTER,
                        !feedback_tool_unavailable,
                        Some(feedback_tooltip),
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
                    // CDXC:Browser 2026-11-27 DECISION:
                    // User: "replace this button with the open page externally button please" (of the toolbar's DevTools button) and "move devtools inside the 3 dots pls". Open Externally takes that spot and leaves the view tab strip for the Browser, so it is not shown twice; DevTools is a row of the ⋯ menu (browser_menu.rs).
                    .child(self.render_browser_toolbar_button(
                        "open-externally",
                        TITLEBAR_ICON_EXTERNAL_LINK,
                        can_open_externally,
                        Some(open_externally_tooltip),
                        BrowserToolbarAction::OpenExternally,
                        pane_id,
                        cx,
                    ))
                    .child(self.render_browser_toolbar_overflow_button(pane_id, cx))
                    // The link page keeps its Close (app/custom_links/page.rs).
                    .when(link_page, |this| {
                        this.child(self.render_custom_link_close_button(cx))
                    }),
            )
    }
}
