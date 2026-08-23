// C1 wave-4 re-cluster: further split out of app/render.rs (~7,340
// lines, itself moved verbatim out of main.rs) into descriptively named
// modules; pure move, no logic changes. Cluster: browser workspace top-level render entry, pane split/leaf recursion, find bar, and media-permission bar/button.

use gpui::AnyElement;
use gpui::FontWeight;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::KeyDownEvent;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::MouseUpEvent;
use gpui::ParentElement as _;
use gpui::Styled as _;
use gpui::Window;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui::relative;
use gpui::rgb;
use gpui_component::Sizable as _;
use gpui_component::Size as ComponentSize;
use gpui_component::h_flex;
use gpui_component::input::Input;
use gpui_component::v_flex;

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn render_browser_workspace(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        self.sync_browser_address_inputs(window, cx);
        self.sync_browser_find_inputs(window, cx);
        v_flex()
            .flex_1()
            .w_full()
            .h_full()
            .min_w_0()
            .min_h_0()
            .bg(browser_toolbar_background())
            .child(self.render_browser_node(&self.browser_tabs.root, window, cx))
            .into_any_element()
    }

    pub(crate) fn render_browser_node(
        &self,
        node: &BrowserNode,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        match node {
            BrowserNode::Split(split) => self.render_browser_split(split, window, cx),
            BrowserNode::Leaf(leaf) => self.render_browser_leaf(leaf, window, cx),
        }
    }

    pub(crate) fn render_browser_split(
        &self,
        split: &BrowserSplit,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let split_id = split.id;
        let axis = split.axis;
        let ratio = workspace_split_ratio(split.ratio);
        let first = div()
            .id(format!("ghostex-gpui-browser-split-{}-first", split_id.0))
            .flex()
            .flex_col()
            .min_w_0()
            .min_h_0()
            .when(axis == WorkspaceSplitAxis::Horizontal, |this| this.h_full())
            .flex_grow(ratio)
            .flex_shrink_1()
            .flex_basis(relative(0.0))
            .child(self.render_browser_node(&split.first, window, cx));
        let second = div()
            .id(format!("ghostex-gpui-browser-split-{}-second", split_id.0))
            .flex()
            .flex_col()
            .min_w_0()
            .min_h_0()
            .when(axis == WorkspaceSplitAxis::Horizontal, |this| this.h_full())
            .flex_grow(1.0 - ratio)
            .flex_shrink_1()
            .flex_basis(relative(0.0))
            .child(self.render_browser_node(&split.second, window, cx));

        /*
        CDXC:GPUIBrowserSplits 2026-06-22-09:02:
        Browser split panes are normal non-overlapping layout siblings. Split creation and persistence stay shell-owned while rendered leaves may attach existing tab-owned CEF bodies without adding overlays or hidden hit regions.

        CDXC:GPUIBrowserSplitResize 2026-06-22-09:05:
        Browser split containers report first/handle/second child bounds from normal GPUI layout before resize starts. The visible handle is the actual drag target, matching Agents workspace and command-pane split behavior without transparent overlays, root hit-test routing, or hidden drag regions.
        */
        match split.axis {
            WorkspaceSplitAxis::Horizontal => {
                let view = cx.entity().clone();
                h_flex()
                    .on_children_prepainted(move |child_bounds, _window, cx| {
                        let _ = view.update(cx, |this, _cx| {
                            this.record_browser_split_layout_metrics(split_id, axis, &child_bounds);
                        });
                    })
                    .id(format!("ghostex-gpui-browser-split-{}", split_id.0))
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .items_start()
                    .overflow_hidden()
                    .child(first)
                    .child(self.render_browser_split_handle(split, cx))
                    .child(second)
                    .into_any_element()
            }
            WorkspaceSplitAxis::Vertical => {
                let view = cx.entity().clone();
                v_flex()
                    .on_children_prepainted(move |child_bounds, _window, cx| {
                        let _ = view.update(cx, |this, _cx| {
                            this.record_browser_split_layout_metrics(split_id, axis, &child_bounds);
                        });
                    })
                    .id(format!("ghostex-gpui-browser-split-{}", split_id.0))
                    .flex_1()
                    .min_w_0()
                    .min_h_0()
                    .overflow_hidden()
                    .child(first)
                    .child(self.render_browser_split_handle(split, cx))
                    .child(second)
                    .into_any_element()
            }
        }
    }

    pub(crate) fn render_browser_split_handle(
        &self,
        split: &BrowserSplit,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let split_id = split.id;
        let axis = split.axis;
        match split.axis {
            WorkspaceSplitAxis::Horizontal => div()
                .id(format!("ghostex-gpui-browser-split-handle-{}", split_id.0))
                .flex()
                .flex_shrink_0()
                .h_full()
                .w(px(WORKSPACE_SPLIT_HANDLE_THICKNESS))
                .items_center()
                .justify_center()
                .cursor_ew_resize()
                .bg(workspace_split_handle_color())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        this.handle_browser_split_handle_mouse_down(
                            split_id, axis, event, window, cx,
                        );
                    }),
                )
                .child(
                    div()
                        .h_full()
                        .w(px(WORKSPACE_SPLIT_SEPARATOR_THICKNESS))
                        .cursor_ew_resize()
                        .bg(browser_split_separator_color()),
                )
                .into_any_element(),
            WorkspaceSplitAxis::Vertical => div()
                .id(format!("ghostex-gpui-browser-split-handle-{}", split_id.0))
                .flex()
                .flex_shrink_0()
                .h(px(WORKSPACE_SPLIT_HANDLE_THICKNESS))
                .w_full()
                .items_center()
                .justify_center()
                .cursor_ns_resize()
                .bg(workspace_split_handle_color())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                        this.handle_browser_split_handle_mouse_down(
                            split_id, axis, event, window, cx,
                        );
                    }),
                )
                .child(
                    div()
                        .h(px(WORKSPACE_SPLIT_SEPARATOR_THICKNESS))
                        .w_full()
                        .cursor_ns_resize()
                        .bg(browser_split_separator_color()),
                )
                .into_any_element(),
        }
    }

    pub(crate) fn render_browser_leaf(
        &self,
        leaf: &BrowserLeaf,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let pane_id = leaf.pane_id;
        let border_state = self.browser_leaf_border_state(leaf, window);
        let view = cx.entity().clone();

        /*
        CDXC:GPUIBrowserKeyboardFocus 2026-06-22-09:24:
        Browser split leaf panes report focus geometry from their actual rendered tab-strip/body children. Directional keyboard focus must treat Browser split placeholders and existing visible Browser CEF bodies as real panes, so geometry stays runtime-only and comes from normal layout rather than overlays or hit-test routing.
        */
        v_flex()
            .on_children_prepainted(move |child_bounds, _window, cx| {
                let _ = view.update(cx, |this, _cx| {
                    this.record_browser_leaf_layout_bounds(pane_id, &child_bounds);
                });
            })
            .id(format!("ghostex-gpui-browser-pane-{}", pane_id.0))
            .flex_1()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .border_1()
            .border_color(workspace_pane_border_color_for_state(border_state))
            .bg(workspace_terminal_placeholder_color())
            .child(self.render_browser_toolbar(pane_id, cx))
            .when_some(
                self.render_browser_media_permission_bar(leaf, cx),
                |this, permission_bar| this.child(permission_bar),
            )
            .when_some(self.render_browser_find_bar(leaf, cx), |this, find_bar| {
                this.child(find_bar)
            })
            .child(self.render_browser_body(leaf, cx))
            .into_any_element()
    }

    pub(crate) fn render_browser_find_bar(
        &self,
        leaf: &BrowserLeaf,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        let tab_id = leaf.tab_group.active_tab_id()?;
        let find = self.browser_find_states.get(&tab_id)?;
        let input = self.browser_find_inputs.get(&tab_id).cloned();
        let count_label = browser_find_count_label(find);
        let element_id_suffix = format!("{}-{}", leaf.pane_id.0, tab_id.0);

        Some(
            h_flex()
                .id(format!("ghostex-gpui-browser-find-row-{element_id_suffix}"))
                .flex_shrink_0()
                .w_full()
                .h(px(FIND_BAR_HEIGHT))
                .items_center()
                .justify_end()
                .pl(px(8.0))
                .bg(terminal_search_bar_row_color())
                .border_b_1()
                .border_color(terminal_search_bar_divider_color())
                .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                    match event.keystroke.key.as_str() {
                        "escape" => {
                            cx.stop_propagation();
                            this.close_browser_find(tab_id, window, cx);
                        }
                        "up" => {
                            cx.stop_propagation();
                            let _ = this.perform_browser_find_navigation(tab_id, false, cx);
                        }
                        "down" => {
                            cx.stop_propagation();
                            let _ = this.perform_browser_find_navigation(tab_id, true, cx);
                        }
                        _ => {}
                    }
                }))
                .child(
                    h_flex()
                        .id(format!("ghostex-gpui-browser-find-bar-{element_id_suffix}"))
                        .w(px(300.0))
                        .max_w_full()
                        .h_full()
                        .items_center()
                        .gap(px(4.0))
                        .border_l_1()
                        .border_color(terminal_search_bar_border_color())
                        .bg(terminal_search_bar_background_color())
                        .pl(px(9.0))
                        .when_some(input, |this, input| {
                            this.child(
                                div().flex_1().min_w_0().overflow_hidden().child(
                                    Input::new(&input)
                                        .with_size(ComponentSize::XSmall)
                                        .appearance(false)
                                        .bordered(false)
                                        .focus_bordered(false)
                                        .w_full()
                                        .px(px(0.0))
                                        .py(px(0.0))
                                        .text_size(px(13.0))
                                        .text_color(terminal_search_bar_text_color()),
                                ),
                            )
                        })
                        .when(!count_label.is_empty(), |this| {
                            this.child(
                                div()
                                    .flex_shrink_0()
                                    .text_size(px(11.0))
                                    .text_color(terminal_search_bar_count_color())
                                    .child(count_label),
                            )
                        })
                        .child(
                            h_flex()
                                .h_full()
                                .flex_shrink_0()
                                .child(self.render_terminal_search_button(
                                    format!("ghostex-gpui-browser-find-prev-{element_id_suffix}"),
                                    "↑",
                                    FIND_BAR_NAV_BUTTON_WIDTH,
                                    move |this, _window, cx| {
                                        let _ =
                                            this.perform_browser_find_navigation(tab_id, false, cx);
                                    },
                                    cx,
                                ))
                                .child(self.render_terminal_search_button(
                                    format!("ghostex-gpui-browser-find-next-{element_id_suffix}"),
                                    "↓",
                                    FIND_BAR_NAV_BUTTON_WIDTH,
                                    move |this, _window, cx| {
                                        let _ =
                                            this.perform_browser_find_navigation(tab_id, true, cx);
                                    },
                                    cx,
                                ))
                                .child(self.render_terminal_search_button(
                                    format!("ghostex-gpui-browser-find-close-{element_id_suffix}"),
                                    "✕",
                                    FIND_BAR_CLOSE_BUTTON_WIDTH,
                                    move |this, window, cx| {
                                        this.close_browser_find(tab_id, window, cx);
                                    },
                                    cx,
                                )),
                        ),
                )
                .into_any_element(),
        )
    }

    pub(crate) fn render_browser_media_permission_bar(
        &self,
        leaf: &BrowserLeaf,
        cx: &mut gpui::Context<Self>,
    ) -> Option<AnyElement> {
        /*
        CDXC:GPUIBrowserMediaPermissions 2026-07-27:
        The prompt is a real chrome row owned by the pane's normal layout, like
        the find bar: it shrinks the CEF child view instead of floating over it,
        so nothing overlaps the page and no hit-test routing is involved.
        */
        let tab_id = leaf.tab_group.active_tab_id()?;
        let prompt = self.browser_media_permission_prompt_for_tab(tab_id)?;
        let element_id_suffix = format!("{}-{}", leaf.pane_id.0, tab_id.0);
        let icon = if prompt.pending.microphone {
            BROWSER_ICON_MICROPHONE
        } else {
            BROWSER_ICON_CAMERA
        };
        let message = format!(
            "{} wants to use {}",
            gpui_browser_media_permission_display_origin(&prompt.origin),
            gpui_browser_media_permission_kinds_label(prompt.pending),
        );

        Some(
            h_flex()
                .id(format!(
                    "ghostex-gpui-browser-media-permission-bar-{element_id_suffix}"
                ))
                .flex_shrink_0()
                .w_full()
                .h(px(BROWSER_MEDIA_PERMISSION_BAR_HEIGHT))
                .items_center()
                .gap(px(8.0))
                .px(px(BROWSER_TOOLBAR_HORIZONTAL_PADDING))
                .bg(browser_toolbar_background())
                .border_b_1()
                .border_color(rgb(0x252525))
                .child(titlebar_svg_icon(
                    icon,
                    15.0,
                    browser_toolbar_button_icon_color(),
                ))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(12.5))
                        .text_color(rgb(0xffffff).opacity(0.88))
                        .child(message),
                )
                .child(self.render_browser_media_permission_button(
                    format!("ghostex-gpui-browser-media-permission-block-{element_id_suffix}"),
                    "Block",
                    false,
                    move |this, _window, cx| {
                        this.resolve_browser_media_permission_prompt(tab_id, false, cx);
                    },
                    cx,
                ))
                .child(self.render_browser_media_permission_button(
                    format!("ghostex-gpui-browser-media-permission-allow-{element_id_suffix}"),
                    "Allow",
                    true,
                    move |this, _window, cx| {
                        this.resolve_browser_media_permission_prompt(tab_id, true, cx);
                    },
                    cx,
                ))
                .into_any_element(),
        )
    }

    pub(crate) fn render_browser_media_permission_button(
        &self,
        id: String,
        label: &'static str,
        primary: bool,
        on_click: impl Fn(&mut Self, &mut Window, &mut gpui::Context<Self>) + 'static,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let (background, hover_background, border, text) = if primary {
            (0.16, 0.22, 0.28, 0.95)
        } else {
            (0.06, 0.11, 0.16, 0.8)
        };

        div()
            .id(id)
            .flex()
            .flex_shrink_0()
            .h(px(24.0))
            .items_center()
            .justify_center()
            .rounded(px(5.0))
            .border_1()
            .border_color(rgb(0xffffff).opacity(border))
            .bg(rgb(0xffffff).opacity(background))
            .px(px(12.0))
            .text_size(px(12.0))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgb(0xffffff).opacity(text))
            .cursor_pointer()
            .hover(|this| this.bg(rgb(0xffffff).opacity(hover_background)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseUpEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    on_click(this, window, cx);
                }),
            )
            .child(label)
            .into_any_element()
    }

}
