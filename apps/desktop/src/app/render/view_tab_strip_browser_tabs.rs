//! The Browser view's own tabs, drawn in the view panel's tab strip beside the view tabs.

use gpui::AnyElement;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::MouseUpEvent;
use gpui::ParentElement as _;
use gpui::StatefulInteractiveElement as _;
use gpui::Styled as _;
use gpui::Window;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::render::view_tab_strip::ViewStripTabSlot;
use crate::app::render::view_tab_strip::view_tab_strip_tooltip_text;
use crate::*;

const VIEW_STRIP_BROWSER_TAB_GROUP: &str = "ghostex-gpui-view-strip-browser-tab";

/// One browser tab as the strip lists it, with the pane that owns it.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) struct ViewStripBrowserTab {
    pub(crate) pane_id: BrowserPaneId,
    pub(crate) tab_id: BrowserTabId,
}

impl GhostexGpuiApp {
    /// CDXC:Browser 2026-09-20 DECISION:
    /// User: browser tabs are no longer sidebar rows. They live at the top of the view panel, in
    /// the same strip as the view tabs (since 2026-09-21 in one order with them, draggable anywhere
    /// in the row, with the `+` after every tab), so a browser tab reads as a peer of the
    /// Browser, Code and Docs tabs rather than as a session. They stay listed while another view is
    /// on screen, and clicking one brings the Browser view back with that tab selected. Docs is
    /// meant to list its open files the same way later, which is why the strip takes a second group
    /// rather than the Browser view owning a tab bar of its own.
    /// SEE-ALSO: apps/desktop/src/app/render/view_tab_strip.rs (the strip these join),
    /// apps/desktop/src/app/gx_store/sidebar_list_inputs.rs (the sidebar projection that stopped
    /// listing them).
    pub(crate) fn view_strip_browser_tabs(&self) -> Vec<ViewStripBrowserTab> {
        if !self.open_view_tabs().contains(&TitlebarMode::Browser) {
            return Vec::new();
        }
        // A tab earns its place in the strip once it has a page, which is the rule the sidebar rows
        // followed. The address-only "New Tab" placeholder is the exception every project carries
        // even when it has never opened the Browser, so it is listed only while the Browser view is
        // the one on screen and the user is looking at it.
        let browser_is_open_view = self.active_mode == TitlebarMode::Browser;
        let mut tabs = Vec::new();
        for pane_id in self.browser_tabs.rendered_leaf_order() {
            let Some(leaf) = self.browser_tabs.find_leaf(pane_id) else {
                continue;
            };
            for pane_tab in &leaf.tab_group.tabs {
                let Some(tab) = self.browser_tabs.tab(pane_tab.tab_id) else {
                    continue;
                };
                if browser_is_open_view || tab.state == BrowserTabState::Loaded {
                    tabs.push(ViewStripBrowserTab {
                        pane_id,
                        tab_id: tab.id,
                    });
                }
            }
        }
        tabs
    }

    pub(crate) fn render_view_strip_browser_tab(
        &self,
        entry: ViewStripBrowserTab,
        slot: ViewStripTabSlot,
        showing_tab_id: Option<BrowserTabId>,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let pinned = slot.pinned;
        let ViewStripBrowserTab { pane_id, tab_id } = entry;
        let tab = self.browser_tabs.tab(tab_id);
        let state = tab
            .map(|tab| tab.state)
            .unwrap_or(BrowserTabState::AddressOnly);
        let has_cef_surface = self.browser_surfaces.contains_key(&tab_id);
        let chrome_status = BrowserTabChromeStatus::from_state(state, has_cef_surface);
        let is_showing = showing_tab_id == Some(tab_id);
        // A loaded tab with no page of its own is the dimmed "this is asleep" tab, the same reading
        // a sleeping view tab gets.
        let asleep = chrome_status == BrowserTabChromeStatus::RestoredPlaceholder;
        let title = tab
            .map(BrowserTab::display_title)
            .unwrap_or_else(|| "New Tab".to_string());
        let profile_id = tab
            .map(|tab| tab.profile_id)
            .unwrap_or_else(BrowserProfileId::default_profile);
        let runtime_favicon_url = tab.and_then(|tab| tab.runtime_favicon_url.as_deref());
        let runtime_favicon_image = tab.and_then(|tab| tab.runtime_favicon_image.clone());
        let runtime_favicon_fetch = tab.and_then(|tab| tab.runtime_favicon_fetch.clone());
        let dragged_tab = DraggedBrowserTab {
            source_pane_id: pane_id,
            tab_id,
            profile_id,
            title: title.clone(),
            runtime_favicon_url: runtime_favicon_url.map(str::to_string),
            runtime_favicon_image: runtime_favicon_image.clone(),
            runtime_favicon_fetch: runtime_favicon_fetch.clone(),
            state,
            chrome_status,
        };
        let view = cx.entity().clone();
        let tooltip_title = title.clone();
        let tab_frame = Self::view_strip_tab_frame(
            format!(
                "ghostex-gpui-view-strip-browser-tab-{}-{}",
                pane_id.0, tab_id.0
            ),
            VIEW_STRIP_BROWSER_TAB_GROUP,
            WORKAREA_VIEW_TAB_WIDTH,
            slot,
        );
        self.with_view_strip_drop_target(tab_frame, slot.index, cx)
            .when(is_showing, |this| {
                this.bg(titlebar_active_segment_color())
                    .text_color(titlebar_active_text_color())
            })
            .when(!is_showing, |this| {
                this.text_color(titlebar_inactive_text_color())
                    .hover(|this| {
                        this.bg(titlebar_button_hover_color())
                            .text_color(titlebar_active_text_color())
                    })
            })
            .when(asleep && !is_showing, |this| this.opacity(0.72))
            .managed_tooltip_with_placement(
                ManagedTooltipPlacement::WiderSide,
                move |window, cx| {
                    titlebar_tooltip(view_tab_strip_tooltip_text(&tooltip_title), window, cx)
                },
            )
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.open_browser_tab_from_view_strip(pane_id, tab_id, window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.show_browser_tab_context_menu(pane_id, tab_id, event.position, window, cx);
                }),
            )
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(move |this, _event: &MouseUpEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    // A pinned tab closes from its menu only, never by a stray middle click.
                    if !pinned {
                        this.close_browser_tab(tab_id, window, cx);
                    }
                }),
            )
            .on_drag(dragged_tab, move |dragged, _offset, _window, cx| {
                let _ = view.update(cx, |this, cx| {
                    this.begin_browser_tab_drag(cx);
                });
                cx.new(|_| BrowserTabDragPreview {
                    profile_id: dragged.profile_id,
                    title: dragged.title.clone(),
                    runtime_favicon_url: dragged.runtime_favicon_url.clone(),
                    runtime_favicon_image: dragged.runtime_favicon_image.clone(),
                    runtime_favicon_fetch: dragged.runtime_favicon_fetch.clone(),
                    state: dragged.state,
                    chrome_status: dragged.chrome_status,
                })
            })
            .child(self.render_browser_tab_icon(
                profile_id,
                chrome_status,
                runtime_favicon_image.as_ref(),
                runtime_favicon_fetch.as_ref(),
            ))
            .when(!pinned, |this| {
                this.child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(title),
                )
            })
            // Every tab closes like a view tab, the empty "New Tab" placeholder included: closing
            // the Browser's last tab closes the Browser view and brings back the view picker.
            .when(!pinned, |this| {
                this.child(self.render_view_strip_browser_tab_close_button(tab_id, is_showing, cx))
            })
            .into_any_element()
    }

    /// The close control, drawn on the showing tab and on whichever tab the pointer is over, so a
    /// tab never changes width when it is hovered. The same rule the view tabs beside it follow.
    fn render_view_strip_browser_tab_close_button(
        &self,
        tab_id: BrowserTabId,
        is_showing: bool,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        div()
            .id(format!(
                "ghostex-gpui-view-strip-browser-tab-close-{}",
                tab_id.0
            ))
            .flex()
            .flex_shrink_0()
            .size(px(WORKAREA_VIEW_TAB_CLOSE_SIZE))
            .items_center()
            .justify_center()
            .rounded(px(4.0))
            .cursor_default()
            .when(!is_showing, |this| {
                this.opacity(0.0)
                    .group_hover(VIEW_STRIP_BROWSER_TAB_GROUP, |this| this.opacity(1.0))
            })
            .hover(|this| this.bg(titlebar_button_hover_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.close_browser_tab(tab_id, window, cx);
                }),
            )
            .child(titlebar_svg_icon(
                TITLEBAR_ICON_X,
                11.0,
                titlebar_icon_color(),
            ))
    }

    /// Clicking a browser tab in the strip. The Browser view comes back first when another view is
    /// on screen, because the tab the user just clicked has to be the thing they end up looking at.
    pub(crate) fn open_browser_tab_from_view_strip(
        &mut self,
        pane_id: BrowserPaneId,
        tab_id: BrowserTabId,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.titlebar_mode_available(TitlebarMode::Browser) {
            return;
        }
        if self.active_mode != TitlebarMode::Browser {
            self.open_view_tab(TitlebarMode::Browser, window, cx);
        }
        self.select_browser_tab_in_pane(pane_id, tab_id, window, cx);
    }

    /// The `+` menu's Browser row: always a new tab, never "switch to the Browser".
    ///
    /// A project whose Browser has only its address-only placeholder gets that placeholder opened
    /// rather than a second tab beside it, because opening the Browser view is what turns the
    /// placeholder into the project's first page; adding a tab as well would leave two.
    pub(crate) fn open_new_browser_tab_from_view_menu(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.titlebar_mode_available(TitlebarMode::Browser) {
            return;
        }
        let only_placeholder = self.browser_tabs.tabs.len() == 1
            && self.browser_tabs.tabs[0].state == BrowserTabState::AddressOnly;
        if self.active_mode != TitlebarMode::Browser {
            self.open_view_tab(TitlebarMode::Browser, window, cx);
            if only_placeholder {
                return;
            }
        }
        self.add_browser_tab(window, cx);
    }
}
