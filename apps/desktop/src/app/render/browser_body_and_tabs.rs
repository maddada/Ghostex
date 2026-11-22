// C1 wave-4 re-cluster: further split out of app/render.rs (~7,340
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: browser pane body rendering (including placeholders), browser tab strip/tab element, drop target/insertion marker, tab icon, drop-zone/feedback, and tab action cluster.

use gpui::AnyElement;
use gpui::FontWeight;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::ParentElement as _;
use gpui::Styled as _;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui_component::h_flex;
use gpui_component::v_flex;

use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn render_browser_body(
        &self,
        leaf: &BrowserLeaf,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let pane_id = leaf.pane_id;
        let placeholder = self.browser_body_placeholder_for_leaf(leaf);
        let active_browser_surface = self.browser_surface_for_rendered_leaf(leaf);
        let render_empty_body = active_browser_surface.is_none();
        let device_view = self
            .browser_tabs
            .active_loaded_tab_id_for_leaf(leaf)
            .and_then(|tab_id| self.browser_tabs.tab(tab_id))
            .map(|tab| tab.device_view)
            .unwrap_or_default();
        div()
            .id(format!("ghostex-gpui-browser-body-{}", pane_id.0))
            .relative()
            .flex_1()
            .w_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            // Under window glass an empty or loading page shows the pane's glass; a loaded page
            // covers it with its own solid view.
            .bg(glass_clear(chrome_color(0x000000, 0xffffff).into()))
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
            .when_some(active_browser_surface, |this, browser| {
                match device_view.frame_size() {
                    Some(frame_size) => this.child(render_browser_device_frame(
                        pane_id,
                        device_view,
                        frame_size,
                        browser.into_any_element(),
                    )),
                    None => this.child(browser),
                }
            })
            .when(render_empty_body, |this| {
                if let Some(signature) =
                    self.web_runtime_placeholder_signature(TitlebarMode::Browser)
                {
                    this.child(self.render_project_editor_placeholder(signature, cx))
                } else if let Some(machine_id) = self
                    .browser_tabs
                    .active_tab_for_pane(pane_id)
                    .and_then(|tab| tab.remote_machine_id.as_deref())
                {
                    this.child(self.render_remote_browser_placeholder(machine_id, cx))
                } else {
                    this.child(self.render_browser_placeholder_body(pane_id, placeholder))
                }
            })
            .when_some(self.browser_pane_drop_zone(pane_id), |this, zone| {
                this.child(self.render_browser_pane_drop_feedback(pane_id, zone))
            })
            .into_any_element()
    }

    pub(crate) fn browser_body_placeholder_for_leaf(
        &self,
        leaf: &BrowserLeaf,
    ) -> BrowserBodyPlaceholder {
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
        CDXC:Browser 2026-06-22-06:59:
        Address-only Browser tabs are real shell tabs but not real page surfaces yet. Render an empty theme-colored GPUI body for those tabs so creating or selecting a new tab never exposes the previous tab's CEF page.

        CDXC:Browser 2026-06-22-09:02:
        Browser split panes remain visible shell panes even when their active loaded tab has no existing CEF entity. Render those loaded bodies as restored/sleeping placeholders while preserving their tab groups and selected tab ids for later focused activation or wake materialization.

        CDXC:Browser 2026-06-22-13:38:
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
            .bg(glass_clear(chrome_color(0x000000, 0xffffff).into()))
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
            .border_color(glass_divider(chrome_color(0x1f1f1f, 0xffffff).into()))
            .bg(glass_clear(chrome_color(0x000000, 0xffffff).into()))
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
                            .text_color(chrome_color(0xf2f2f2, 0x252525))
                            .child(title),
                    )
                    .child(
                        div()
                            .mt(px(8.0))
                            .text_size(px(12.5))
                            .text_color(chrome_color(0x8f8f8f, 0x666666))
                            .child("Click to load tab"),
                    ),
            )
            .into_any_element()
    }

    pub(crate) fn render_browser_tab_icon(
        &self,
        profile_id: BrowserProfileId,
        chrome_status: BrowserTabChromeStatus,
        runtime_favicon_image: Option<&BrowserFaviconImage>,
        runtime_favicon_fetch: Option<&BrowserFaviconFetchSource>,
    ) -> AnyElement {
        browser_tab_icon_element(
            profile_id,
            chrome_status,
            runtime_favicon_image,
            runtime_favicon_fetch,
        )
    }

    pub(crate) fn browser_pane_drop_zone(
        &self,
        pane_id: BrowserPaneId,
    ) -> Option<WorkspaceDropZone> {
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
        CDXC:Browser 2026-06-22-09:02:
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
}

/// CDXC:Browser 2026-10-16 WHY:
/// The phone or tablet preview gives the page's own native view a smaller frame in normal layout, centred under a size caption, instead of scaling or overlaying it, so clicks and focus stay with that one view. A pane smaller than the device shrinks the frame to fit (the caption keeps the nominal size).
pub(crate) fn render_browser_device_frame(
    pane_id: BrowserPaneId,
    device_view: BrowserDeviceView,
    (width, height): (f32, f32),
    page: AnyElement,
) -> AnyElement {
    v_flex()
        .id(format!("ghostex-gpui-browser-device-frame-{}", pane_id.0))
        .size_full()
        .min_h_0()
        .items_center()
        .px(px(16.0))
        .pb(px(16.0))
        // CDXC:Theming 2026-11-20: under glass the frame around the device is an ink wash, not a
        // #161616 slab.
        .bg(glass_card(chrome_color(0x161616, 0xe5e5e5).into(), 2.0))
        .child(
            div()
                .flex_shrink_0()
                .h(px(28.0))
                .flex()
                .items_center()
                .text_size(px(11.0))
                .text_color(chrome_color(0x8f8f8f, 0x666666))
                .children(device_view.caption()),
        )
        .child(
            div()
                .flex()
                .flex_shrink_1()
                .min_h_0()
                .w(px(width))
                .h(px(height))
                .max_w_full()
                .border_1()
                .border_color(chrome_color(0x333333, 0xc4c4c4))
                .child(page),
        )
        .into_any_element()
}
