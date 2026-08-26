// C1 wave-4 deferred split: apps/desktop/src/app/titlebar.rs (~3.9k lines)
// further divided into responsibility-scoped submodules, pure move (the
// only edit from the original app/titlebar.rs body is wrapping each group
// of `impl GhostexGpuiApp` methods in its own impl block; multiple impl
// blocks for the same type across files is the established pattern used by
// every sibling file in apps/desktop/src/app/). This file holds the generic titlebar icon-button renderer and the browser toolbar renderer.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: titlebar menus, popups, actions, and titlebar render_* builders

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::AnyElement;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
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
    pub(crate) fn render_titlebar_icon_button(
        &self,
        id: &'static str,
        icon_path: &'static str,
        icon_size: f32,
        show_badge: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        div()
            .id(format!("ghostex-gpui-titlebar-button-{id}"))
            .relative()
            .flex()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .w(px(if id == "settings" {
                TITLEBAR_SETTINGS_BUTTON_WIDTH
            } else {
                TITLEBAR_BUTTON_WIDTH
            }))
            .items_center()
            .justify_center()
            .border_l_1()
            .border_color(titlebar_button_border_color())
            .text_color(titlebar_icon_color())
            .cursor_default()
            .hover(|this| {
                this.bg(titlebar_button_hover_color())
                    .text_color(titlebar_icon_hover_color())
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    if id == "settings" {
                        /*
                        CDXC:GPUITitlebarAppModalHost 2026-06-24-11:09:
                        The GPUI titlebar Settings glyph owns the app-modal menu for Settings, Hotkeys, and Command Palette. Keep the menu as OS-owned NativeMenu actions that all route to the shared React modal host, rather than leaving Hotkeys or Command Palette without a titlebar path or adding GPUI-local placeholder UI.

                            CDXC:GPUIPreviousSessionsModal 2026-06-24-11:53:
                            The same Settings glyph menu owns Previous Sessions access so the GPUI titlebar opens the production shared modal and its gxserver bridge, not a separate GPUI-local history picker.

                            */
                        this.show_titlebar_settings_menu(event.position, window, cx);
                    } else if id == "keep-awake" {
                        /*
                        CDXC:GPUITitlebarKeepAwake 2026-06-24-13:16:
                        Keep Awake titlebar clicks open the OS-owned duration menu instead of toggling caffeinate directly. Runtime start/stop stays inside menu actions so users can choose the same shared duration semantics as macOS.
                        */
                        this.show_gpui_keep_awake_menu(event.position, window, cx);
                    } else if id == "resources" {
                        /*
                        CDXC:GPUIResourcesTitlebar 2026-07-08:
                        The visible Resources titlebar glyph opens the shared
                        React titlebar-host Resources panel in the app-owned
                        anchored dropdown. React owns live process polling while
                        Rust owns only the panel entity and native action bridge.
                        */
                        this.set_gpui_titlebar_resources_panel_open(
                            !this.titlebar_resources_panel_open,
                            window,
                            cx,
                        );
                    } else if id == "git" {
                        this.show_gpui_titlebar_git_menu(None, window, cx);
                    } else if id == "open-project" {
                        /*
                        CDXC:GPUITitlebarOpenIn 2026-06-24-12:50:
                        Left-click Open In launches the active runtime target for the explicit active project path only. If GPUI does not have `in_memory_project_path` from the sidebar contract, show private-data-free feedback and do not infer a path from display names, ids, labels, filesystem probing, or git metadata.
                        */
                        this.open_active_project_with_active_open_target(window, cx);
                    } else if id == "actions" {
                        /*
                        CDXC:GPUITitlebarActions 2026-06-24-14:24:
                        Left-click Actions runs the selected/last configured sidebar action when GPUI has one, otherwise the first configured action, and opens Settings > Actions when no configured action is available. Browser actions use the GPUI Browser CEF path, terminal actions use command-pane launch payloads, and this handler must not shell out, log private details, or create overlay UI.
                        */
                        this.run_active_gpui_titlebar_action(window, cx);
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    if id == "keep-awake" {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.show_gpui_keep_awake_menu(event.position, window, cx);
                    } else if id == "open-project" {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.show_gpui_open_targets_menu(None, window, cx);
                    } else if id == "resources" {
                        /*
                        CDXC:GPUIResourcesTitlebar 2026-07-08:
                        Right-click uses the same React Resources dropdown as
                        primary click so the full resource controls share one
                        titlebar-host surface and one native action bridge.
                        */
                        window.prevent_default();
                        cx.stop_propagation();
                        this.set_gpui_titlebar_resources_panel_open(
                            !this.titlebar_resources_panel_open,
                            window,
                            cx,
                        );
                    } else if id == "git" {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.show_gpui_titlebar_git_menu(None, window, cx);
                    } else if id == "actions" {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.show_gpui_titlebar_actions_menu(None, window, cx);
                    }
                }),
            )
            .child(titlebar_svg_icon(
                icon_path,
                icon_size,
                titlebar_icon_color(),
            ))
            .when(show_badge, |this| {
                this.child(
                    div()
                        .absolute()
                        .right(px(8.0))
                        .top(px(5.0))
                        .size(px(7.5))
                        .rounded_full()
                        .border_1()
                        .border_color(titlebar_background())
                        .bg(rgb(0x95d7f6)),
                )
            })
            .into_any_element()
    }

    pub(crate) fn render_browser_toolbar(
        &self,
        pane_id: BrowserPaneId,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        /*
        CDXC:GPUIBrowserToolbar 2026-06-14-17:42:
        The GPUI browser pane needs the same address toolbar as the macOS app, implemented only with GPUI chrome: a black row, stateless Back/Forward/Reload controls with no Back/Forward history toggles, a lock-or-globe address field that restores the current URL on empty commits, and the browser right-control group while preserving non-empty address commits inside the embedded CEF browser. Toolbar actions use the same full-height segmented button chrome as tab-bar actions.

        CDXC:GPUIBrowserToolbar 2026-06-15-01:52:
        GitHub disallows the injected feedback tool, so the GPUI toolbar must render the feedback button disabled on github.com pages and expose the tooltip "This site disallows using this tool" instead of letting the user start an unsupported tool action.

        CDXC:GPUIBrowserToolbar 2026-06-22-08:08:
        Browser Back and Forward controls must read their enabled state from the selected loaded tab's existing CEF surface and must no-op when that surface cannot navigate. Reload must call CEF `reload()` on the selected loaded surface instead of loading the shell URL again, so Chromium keeps ownership of history, POST/cache behavior, and address-only placeholder tabs remain unloaded.

        CDXC:GPUIBrowserToolbar 2026-06-22-11:50:
        The right-side Browser controls follow current macOS parity: zoom reset appears only when the active CEF surface is zoomed, the feedback button launches Agentation, History and Profile remain OS NativeMenus, DevTools toggles through the active CEF surface, and the removed Appearance control does not reserve toolbar space or hit area.

        CDXC:GPUIBrowserFeedback 2026-06-23-11:04:
        The Browser feedback toolbar starts Agentation through CEF main-frame JavaScript injection. Keep github.com and *.github.com disabled before injection, and keep the toolbar surface status private by showing only bounded page-data-free notifications for missing CEF surfaces or frames.
        */
        let address_value = self.browser_tabs.address_value_for_pane(pane_id);
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
        let can_show_recent_history = !self
            .browser_tabs
            .pane_history_rows(pane_id, BROWSER_HISTORY_MENU_MAX_ROWS)
            .is_empty();
        /*
        CDXC:GPUIBrowserMediaPermissions 2026-07-27:
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
            .bg(browser_toolbar_background())
            .border_b_1()
            .border_color(rgb(0x252525))
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
                    .child(self.render_browser_toolbar_new_tab_button(pane_id, cx))
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
                        can_show_recent_history,
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
