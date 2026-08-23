// C1 wave-4 re-cluster: further split out of app/render.rs (~7,340
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: browser pane body rendering (including placeholders), browser tab strip/tab element, drop target/insertion marker, tab icon, drop-zone/feedback, and tab action cluster.

use gpui::AnyElement;
use gpui::AppContext as _;
use gpui::FontWeight;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::MouseUpEvent;
use gpui::ParentElement as _;
use gpui::StatefulInteractiveElement as _;
use gpui::Styled as _;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui::rgb;
use gpui_component::h_flex;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;
use gpui_component::tooltip::Tooltip;
use gpui_component::v_flex;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn render_browser_body(&self, leaf: &BrowserLeaf, cx: &mut gpui::Context<Self>) -> AnyElement {
        let pane_id = leaf.pane_id;
        let placeholder = self.browser_body_placeholder_for_leaf(leaf);
        let active_browser_surface = self.browser_surface_for_rendered_leaf(leaf);
        let render_empty_body = active_browser_surface.is_none();
        div()
            .id(format!("ghostex-gpui-browser-body-{}", pane_id.0))
            .relative()
            .flex_1()
            .w_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .bg(workspace_terminal_placeholder_color())
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.focus_browser_pane(pane_id, window, cx);
                }),
            )
            .on_drag_move::<DraggedBrowserTab>(cx.listener(
                move |this, event: &gpui::DragMoveEvent<DraggedBrowserTab>, _window, cx| {
                    this.update_browser_pane_drag_feedback(event, pane_id, cx);
                },
            ))
            .can_drop(|value, _window, _cx| value.is::<DraggedBrowserTab>())
            .on_drop(
                cx.listener(move |this, dragged: &DraggedBrowserTab, window, cx| {
                    this.handle_browser_pane_body_drop(pane_id, dragged, window, cx);
                }),
            )
            .when_some(active_browser_surface, |this, browser| this.child(browser))
            .when(render_empty_body, |this| {
                this.child(self.render_browser_placeholder_body(pane_id, placeholder))
            })
            .when_some(self.browser_pane_drop_zone(pane_id), |this, zone| {
                this.child(self.render_browser_pane_drop_feedback(pane_id, zone))
            })
            .into_any_element()
    }

    pub(crate) fn browser_body_placeholder_for_leaf(&self, leaf: &BrowserLeaf) -> BrowserBodyPlaceholder {
        leaf.tab_group
            .active_tab_id()
            .and_then(|tab_id| self.browser_tabs.tab(tab_id))
            .map(|tab| {
                BrowserBodyPlaceholder::from_tab(tab, self.browser_surfaces.contains_key(&tab.id))
            })
            .unwrap_or_else(BrowserBodyPlaceholder::blank)
    }

    pub(crate) fn render_browser_placeholder_body(
        &self,
        pane_id: BrowserPaneId,
        placeholder: BrowserBodyPlaceholder,
    ) -> AnyElement {
        /*
        CDXC:GPUIBrowserTabs 2026-06-22-06:59:
        Address-only Browser tabs are real shell tabs but not real page surfaces yet. Render an empty black GPUI body for those tabs so creating or selecting a new tab never exposes the previous tab's CEF page.

        CDXC:GPUIBrowserSplits 2026-06-22-09:02:
        Browser split panes remain visible shell panes even when their active loaded tab has no existing CEF entity. Render those loaded bodies as restored/sleeping placeholders while preserving their tab groups and selected tab ids for later focused activation or wake materialization.

        CDXC:GPUIBrowserRestoredPlaceholder 2026-06-22-13:38:
        Restored loaded Browser tabs are shell metadata until focus materializes their tab-owned CEF surface. Render a visible sleeping placeholder from sanitized shell URL state only: no CEF creation from render, runtime page titles, query strings, fragments, credentials, local paths, tokens, cookies, or user content.
        */
        if placeholder.state == BrowserTabState::Loaded && !placeholder.has_cef_surface {
            return self.render_browser_restored_placeholder_body(pane_id, placeholder);
        }

        div()
            .id(format!(
                "ghostex-gpui-browser-placeholder-body-{}",
                pane_id.0
            ))
            .size_full()
            .bg(workspace_terminal_placeholder_color())
            .into_any_element()
    }

    pub(crate) fn render_browser_restored_placeholder_body(
        &self,
        pane_id: BrowserPaneId,
        placeholder: BrowserBodyPlaceholder,
    ) -> AnyElement {
        let title = placeholder
            .safe_title
            .unwrap_or_else(|| "Restored Browser tab".to_string());

        v_flex()
            .id(format!(
                "ghostex-gpui-browser-restored-placeholder-body-{}",
                pane_id.0
            ))
            .size_full()
            .items_center()
            .justify_center()
            .border_1()
            .border_color(rgb(0x1f1f1f))
            .bg(rgb(0x000000))
            .px(px(32.0))
            .child(
                v_flex()
                    .w_full()
                    .max_w(px(640.0))
                    .min_w_0()
                    .items_center()
                    .justify_center()
                    .child(
                        div()
                            .max_w(px(540.0))
                            .min_w_0()
                            .overflow_hidden()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(24.0))
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_color(rgb(0xf2f2f2))
                            .child(title),
                    )
                    .child(
                        div()
                            .mt(px(8.0))
                            .text_size(px(12.5))
                            .text_color(rgb(0x8f8f8f))
                            .child("Click to load tab"),
                    ),
            )
            .into_any_element()
    }

    #[allow(dead_code)] // no caller: the browser tab strip is drawn by the CEF browser chrome; this native gpui strip (and everything it calls) is the superseded implementation
    pub(crate) fn render_browser_tab_strip(
        &self,
        leaf: &BrowserLeaf,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        /*
        CDXC:GPUIBrowserTabs 2026-06-22-05:56:
        Browser mode needs native-style tabs above the address toolbar while GPUI owns Browser tab identity. Render the strip as a real top chrome row in normal layout, keep tab overflow horizontally scrollable, and reserve only an in-memory new-tab control in this slice.

        CDXC:GPUIBrowserTabs 2026-06-22-06:59:
        Each rendered Browser pane's active loaded tab selects which tab-owned CEF entity may occupy that pane's body. Inactive tabs retain their shell identity and any runtime CEF entity, but their native views are hidden instead of being stacked under another tab.

        CDXC:GPUIBrowserDragDrop 2026-06-22-07:41:
        Browser tabs need Agents/command-style typed GPUI drag within this single tab strip only. Render a visible insertion marker at the computed tab boundary plus a real end-of-strip drop target, and leave Browser body edge drops and cross-pane Browser splitting out of this slice.

        CDXC:GPUIBrowserSplits 2026-06-22-09:02:
        Browser tab strips now belong to Browser panes, not one flat workspace strip. Each pane owns its tab order and active selection, while the shared toolbar follows the focused pane's active tab and CEF ownership stays keyed by BrowserTabId.
        */
        let pane_id = leaf.pane_id;
        let scroll_handle = self.browser_tab_scroll_handle(pane_id);

        h_flex()
            .id(format!("ghostex-gpui-browser-tabbar-{}", pane_id.0))
            .flex_shrink_0()
            .h(px(BROWSER_TAB_BAR_HEIGHT))
            .w_full()
            .items_center()
            .overflow_hidden()
            .border_b_1()
            .border_color(browser_tab_separator_color())
            .bg(browser_tab_bar_color())
            .child(
                h_flex()
                    .id(format!("ghostex-gpui-browser-tabstrip-{}", pane_id.0))
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .items_center()
                    .gap(px(BROWSER_TAB_GAP))
                    .overflow_x_scroll()
                    .track_scroll(&scroll_handle)
                    .children(
                        leaf.tab_group
                            .tabs
                            .iter()
                            .enumerate()
                            .map(|(tab_index, tab)| {
                                self.render_browser_tab(
                                    pane_id,
                                    &leaf.tab_group,
                                    tab_index,
                                    tab,
                                    cx,
                                )
                            }),
                    )
                    .child(self.render_browser_tab_strip_end_drop_target(
                        pane_id,
                        leaf.tab_group.tabs.len(),
                        cx,
                    )),
            )
            .child(self.render_browser_tab_action_cluster(pane_id, cx))
            .into_any_element()
    }

    pub(crate) fn render_browser_tab(
        &self,
        pane_id: BrowserPaneId,
        tab_group: &BrowserTabGroup,
        tab_index: usize,
        pane_tab: &BrowserPaneTab,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let tab_id = pane_tab.tab_id;
        let tab = self.browser_tabs.tab(tab_id);
        let state = tab
            .map(|tab| tab.state)
            .unwrap_or(BrowserTabState::AddressOnly);
        let has_cef_surface = self.browser_surfaces.contains_key(&tab_id);
        let chrome_signature =
            browser_tab_chrome_signature(tab_group, tab_id, state, has_cef_surface);
        let is_active = chrome_signature.active_in_tab_group;
        let state = chrome_signature.state;
        let chrome_status = chrome_signature.chrome_status;
        let display_title = tab
            .map(BrowserTab::display_title)
            .unwrap_or_else(|| "New Tab".to_string());
        let runtime_favicon_url = tab.and_then(|tab| tab.runtime_favicon_url.as_deref());
        let runtime_favicon_image = tab.and_then(|tab| tab.runtime_favicon_image.clone());
        let runtime_favicon_fetch = tab.and_then(|tab| tab.runtime_favicon_fetch.clone());
        let profile_id = tab
            .map(|tab| tab.profile_id)
            .unwrap_or_else(BrowserProfileId::default_profile);
        let dragged_tab = DraggedBrowserTab {
            source_pane_id: pane_id,
            tab_id,
            profile_id,
            title: display_title.clone(),
            runtime_favicon_url: runtime_favicon_url.map(str::to_string),
            runtime_favicon_image: runtime_favicon_image.clone(),
            runtime_favicon_fetch: runtime_favicon_fetch.clone(),
            state,
            chrome_status,
        };
        let view = cx.entity().clone();
        let show_insertion_marker = self.browser_tab_drop_feedback
            == Some(BrowserDropFeedback {
                pane_id,
                target: BrowserTabDropTarget::TabStrip(tab_index),
            });
        let tab_hover_key = BrowserHoverTab { pane_id, tab_id };
        let is_tab_hovered = self.hovered_browser_tab == Some(tab_hover_key);
        let can_close = state != BrowserTabState::AddressOnly || tab_group.tabs.len() > 1;
        let show_close_button = can_close && is_tab_hovered;
        let tab_tooltip = display_title.clone();

        div()
            .id(format!(
                "ghostex-gpui-browser-tab-{}-{}",
                pane_id.0, tab_id.0
            ))
            .relative()
            .flex()
            .flex_shrink_1()
            .h_full()
            .w(px(BROWSER_TAB_MAX_WIDTH))
            .min_w(px(BROWSER_TAB_MIN_WIDTH))
            .max_w(px(BROWSER_TAB_MAX_WIDTH))
            .items_center()
            .overflow_hidden()
            .pl(px(8.0))
            .pr(px(4.0))
            .text_size(px(13.0))
            .font_weight(FontWeight::NORMAL)
            .text_color(browser_tab_text_color(state, is_active))
            .cursor_default()
            .bg(if is_active {
                browser_tab_active_color()
            } else {
                browser_tab_inactive_color()
            })
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Right, move |window, cx| {
                Tooltip::new(tab_tooltip.clone()).build(window, cx)
            })
            .when(show_insertion_marker, |this| {
                this.child(self.render_browser_tab_insertion_marker(pane_id, tab_index, "before"))
            })
            .on_hover(cx.listener(move |this, hovered, _, cx| {
                this.set_browser_tab_hovered(tab_hover_key, *hovered, cx);
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.select_browser_tab_in_pane(pane_id, tab_id, window, cx);
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
                    if can_close {
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
            .on_drag_move::<DraggedBrowserTab>(cx.listener(
                move |this, event: &gpui::DragMoveEvent<DraggedBrowserTab>, _window, cx| {
                    this.update_browser_tab_drag_feedback(event, pane_id, tab_index, cx);
                },
            ))
            .can_drop(move |value, _window, _cx| {
                value
                    .downcast_ref::<DraggedBrowserTab>()
                    .is_some_and(|dragged| dragged.source_pane_id == pane_id)
            })
            .on_drop(
                cx.listener(move |this, dragged: &DraggedBrowserTab, window, cx| {
                    this.handle_browser_tab_strip_drop(pane_id, tab_index, dragged, window, cx);
                }),
            )
            .child(self.render_browser_tab_icon(
                profile_id,
                chrome_status,
                runtime_favicon_url,
                runtime_favicon_image.as_ref(),
                runtime_favicon_fetch.as_ref(),
            ))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .text_ellipsis()
                    .ml(px(5.0))
                    .child(display_title),
            )
            .when(show_close_button, |this| {
                this.child(self.render_browser_tab_close_button(tab_id, cx))
            })
            .into_any_element()
    }

    pub(crate) fn render_browser_tab_strip_end_drop_target(
        &self,
        pane_id: BrowserPaneId,
        insertion_index: usize,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let show_insertion_marker = self.browser_tab_drop_feedback
            == Some(BrowserDropFeedback {
                pane_id,
                target: BrowserTabDropTarget::TabStrip(insertion_index),
            });

        div()
            .id(format!(
                "ghostex-gpui-browser-tabstrip-end-drop-{}",
                pane_id.0
            ))
            .relative()
            .h_full()
            .flex_grow_1()
            .min_w(px(20.0))
            .when(show_insertion_marker, |this| {
                this.child(self.render_browser_tab_insertion_marker(
                    pane_id,
                    insertion_index,
                    "end",
                ))
            })
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseUpEvent, window, cx| {
                    if event.click_count < 2 {
                        return;
                    }
                    window.prevent_default();
                    cx.stop_propagation();
                    this.browser_tabs.focus_pane(pane_id);
                    this.add_browser_tab(window, cx);
                }),
            )
            .on_drag_move::<DraggedBrowserTab>(cx.listener(
                move |this, event: &gpui::DragMoveEvent<DraggedBrowserTab>, _window, cx| {
                    this.update_browser_tab_end_drag_feedback(event, pane_id, insertion_index, cx);
                },
            ))
            .can_drop(move |value, _window, _cx| {
                value
                    .downcast_ref::<DraggedBrowserTab>()
                    .is_some_and(|dragged| dragged.source_pane_id == pane_id)
            })
            .on_drop(
                cx.listener(move |this, dragged: &DraggedBrowserTab, window, cx| {
                    this.handle_browser_tab_strip_drop(
                        pane_id,
                        insertion_index,
                        dragged,
                        window,
                        cx,
                    );
                }),
            )
            .into_any_element()
    }

    pub(crate) fn render_browser_tab_insertion_marker(
        &self,
        pane_id: BrowserPaneId,
        insertion_index: usize,
        marker_kind: &'static str,
    ) -> AnyElement {
        div()
            .id(format!(
                "ghostex-gpui-browser-tab-drop-marker-{}-{}-{marker_kind}",
                pane_id.0, insertion_index
            ))
            .absolute()
            .left_0()
            .top(px(4.0))
            .h(px(BROWSER_TAB_BAR_HEIGHT - 8.0))
            .w(px(2.0))
            .rounded_full()
            .bg(workspace_drop_feedback_border_color())
            .into_any_element()
    }

    pub(crate) fn render_browser_tab_icon(
        &self,
        profile_id: BrowserProfileId,
        chrome_status: BrowserTabChromeStatus,
        runtime_favicon_url: Option<&str>,
        runtime_favicon_image: Option<&BrowserFaviconImage>,
        runtime_favicon_fetch: Option<&BrowserFaviconFetchSource>,
    ) -> AnyElement {
        browser_tab_icon_element(
            profile_id,
            chrome_status,
            runtime_favicon_url,
            runtime_favicon_image,
            runtime_favicon_fetch,
        )
    }

    pub(crate) fn browser_pane_drop_zone(&self, pane_id: BrowserPaneId) -> Option<WorkspaceDropZone> {
        match self.browser_tab_drop_feedback {
            Some(BrowserDropFeedback {
                pane_id: feedback_pane_id,
                target: BrowserTabDropTarget::PaneBody(zone),
            }) if feedback_pane_id == pane_id => Some(zone),
            _ => None,
        }
    }

    pub(crate) fn render_browser_pane_drop_feedback(
        &self,
        pane_id: BrowserPaneId,
        zone: WorkspaceDropZone,
    ) -> AnyElement {
        /*
        CDXC:GPUIBrowserSplits 2026-06-22-09:02:
        Browser body drop feedback must distinguish center grouping from edge split intent while staying non-interactive. Render the indication as a normal child inside the Browser pane body so native CEF views stay hidden by the drag visibility gate instead of relying on transparent overlays or hit-test rerouting.
        */
        let label = match zone {
            WorkspaceDropZone::Center => "Group",
            WorkspaceDropZone::Left => "Split left",
            WorkspaceDropZone::Right => "Split right",
            WorkspaceDropZone::Top => "Split top",
            WorkspaceDropZone::Bottom => "Split bottom",
        };

        let feedback = div()
            .id(format!(
                "ghostex-gpui-browser-pane-drop-feedback-{}",
                pane_id.0
            ))
            .absolute()
            .top_0()
            .left_0()
            .size_full();

        match zone {
            WorkspaceDropZone::Center => feedback
                .flex()
                .items_center()
                .justify_center()
                .border_2()
                .border_color(workspace_drop_feedback_border_color())
                .bg(workspace_drop_group_feedback_color())
                .child(self.render_workspace_drop_feedback_label(label, zone))
                .into_any_element(),
            WorkspaceDropZone::Left => feedback
                .child(
                    self.render_workspace_drop_edge_band(label, zone)
                        .left_0()
                        .top_0()
                        .bottom_0(),
                )
                .into_any_element(),
            WorkspaceDropZone::Right => feedback
                .child(
                    self.render_workspace_drop_edge_band(label, zone)
                        .right_0()
                        .top_0()
                        .bottom_0(),
                )
                .into_any_element(),
            WorkspaceDropZone::Top => feedback
                .child(
                    self.render_workspace_drop_edge_band(label, zone)
                        .top_0()
                        .left_0()
                        .right_0(),
                )
                .into_any_element(),
            WorkspaceDropZone::Bottom => feedback
                .child(
                    self.render_workspace_drop_edge_band(label, zone)
                        .bottom_0()
                        .left_0()
                        .right_0(),
                )
                .into_any_element(),
        }
    }

    pub(crate) fn render_browser_tab_action_cluster(
        &self,
        pane_id: BrowserPaneId,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        h_flex()
            .id(format!("ghostex-gpui-browser-tab-actions-{}", pane_id.0))
            .flex_shrink_0()
            .h_full()
            .w(px(BROWSER_TAB_ACTION_CLUSTER_WIDTH))
            .items_center()
            .bg(browser_tab_action_cluster_color())
            .child(
                div()
                    .id(format!("ghostex-gpui-browser-tab-action-new-{}", pane_id.0))
                    .flex()
                    .h_full()
                    .w(px(BROWSER_TAB_ACTION_BUTTON_SIZE))
                    .items_center()
                    .justify_center()
                    .rounded(px(0.0))
                    .border_l_1()
                    .border_color(browser_tab_separator_color())
                    .bg(browser_tab_action_cluster_color())
                    .cursor_default()
                    .hover(|this| this.bg(browser_tab_action_hover_color()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.browser_tabs.focus_pane(pane_id);
                            this.add_browser_tab(window, cx);
                        }),
                    )
                    .managed_tooltip_with_placement(ManagedTooltipPlacement::Left, |window, cx| {
                        Tooltip::new("New browser tab").build(window, cx)
                    })
                    .child(self.render_browser_tab_new_icon(17.0)),
            )
            .child(
                div()
                    .id(format!(
                        "ghostex-gpui-browser-tab-action-overflow-{}",
                        pane_id.0
                    ))
                    .flex()
                    .h_full()
                    .w(px(BROWSER_TAB_ACTION_BUTTON_SIZE))
                    .items_center()
                    .justify_center()
                    .rounded(px(0.0))
                    .border_l_1()
                    .border_color(browser_tab_separator_color())
                    .bg(browser_tab_action_cluster_color())
                    .cursor_default()
                    .hover(|this| this.bg(browser_tab_action_hover_color()))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |_this, _event: &MouseDownEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                        }),
                    )
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseUpEvent, window, cx| {
                            window.prevent_default();
                            cx.stop_propagation();
                            this.show_browser_pane_actions_menu(
                                pane_id,
                                event.position,
                                window,
                                cx,
                            );
                        }),
                    )
                    .managed_tooltip_with_placement(ManagedTooltipPlacement::Left, |window, cx| {
                        Tooltip::new("Browser pane actions menu").build(window, cx)
                    })
                    .child(self.render_browser_tab_overflow_icon()),
            )
            .into_any_element()
    }

    pub(crate) fn render_browser_tab_new_icon(&self, size: f32) -> AnyElement {
        let arm_length = size - 2.0;
        let arm_offset = (size - 1.0) / 2.0;
        div()
            .relative()
            .size(px(size))
            .child(
                div()
                    .absolute()
                    .left(px(arm_offset))
                    .top(px(1.0))
                    .w(px(1.0))
                    .h(px(arm_length))
                    .bg(browser_tab_action_icon_color()),
            )
            .child(
                div()
                    .absolute()
                    .left(px(1.0))
                    .top(px(arm_offset))
                    .w(px(arm_length))
                    .h(px(1.0))
                    .bg(browser_tab_action_icon_color()),
            )
            .into_any_element()
    }

    pub(crate) fn render_browser_tab_overflow_icon(&self) -> AnyElement {
        h_flex()
            .size(px(14.0))
            .items_center()
            .justify_center()
            .gap(px(2.0))
            .child(
                div()
                    .size(px(3.0))
                    .rounded_full()
                    .bg(browser_tab_action_icon_color()),
            )
            .child(
                div()
                    .size(px(3.0))
                    .rounded_full()
                    .bg(browser_tab_action_icon_color()),
            )
            .child(
                div()
                    .size(px(3.0))
                    .rounded_full()
                    .bg(browser_tab_action_icon_color()),
            )
            .into_any_element()
    }

    pub(crate) fn render_browser_tab_close_button(
        &self,
        tab_id: BrowserTabId,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        div()
            .id(format!("ghostex-gpui-browser-tab-close-{}", tab_id.0))
            .flex()
            .flex_shrink_0()
            .size(px(BROWSER_TAB_CLOSE_SIZE))
            .ml(px(5.0))
            .items_center()
            .justify_center()
            .rounded(px(0.0))
            .bg(browser_tab_close_background_color())
            .cursor_default()
            .hover(|this| this.bg(browser_tab_close_hover_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.close_browser_tab(tab_id, window, cx);
                }),
            )
            .child(titlebar_svg_icon(
                BROWSER_ICON_STOP,
                8.5,
                browser_tab_close_color(),
            ))
            .into_any_element()
    }
}
