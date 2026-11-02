//! A side chat's tab in the view panel's strip, its right-click menu, and the panel body that
//! shows its chat.

use gpui::AnyElement;
use gpui::AnyView;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::MouseUpEvent;
use gpui::ParentElement as _;
use gpui::Pixels;
use gpui::StatefulInteractiveElement as _;
use gpui::Styled as _;
use gpui::Window;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;

use crate::app::actions::*;
use crate::app::consts::*;
use crate::app::context_menu::GpuiContextMenu;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::render::view_tab_strip::ViewStripTabSlot;
use crate::app::render::view_tab_strip::view_tab_strip_tooltip_text;
use crate::*;

const VIEW_STRIP_SIDE_CHAT_TAB_GROUP: &str = "ghostex-gpui-view-strip-side-chat-tab";

impl GhostexGpuiApp {
    pub(crate) fn render_view_strip_side_chat_tab(
        &self,
        tab_id: SideChatTabId,
        slot: ViewStripTabSlot,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let pinned = slot.pinned;
        let is_showing = self.side_chat_tab_on_screen() == Some(tab_id);
        let title = self.side_chat_tab_title(tab_id);
        let icon = TitlebarMode::SideChat.tab_icon();
        let tooltip_title = title.clone();
        let preview_label = title.clone();
        let tab = Self::view_strip_tab_frame(
            format!("ghostex-gpui-view-strip-side-chat-tab-{}", tab_id.0),
            VIEW_STRIP_SIDE_CHAT_TAB_GROUP,
            WORKAREA_VIEW_TAB_WIDTH,
            slot,
        );
        self.with_view_strip_drop_target(tab, slot.index, cx)
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
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Below, move |window, cx| {
                titlebar_tooltip(view_tab_strip_tooltip_text(&tooltip_title), window, cx)
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.show_side_chat_tab(tab_id, window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.show_side_chat_tab_context_menu(tab_id, event.position, window, cx);
                }),
            )
            .on_mouse_up(
                MouseButton::Middle,
                cx.listener(move |this, _event: &MouseUpEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    // A pinned tab closes from its menu only, never by a stray middle click.
                    if !pinned {
                        this.close_side_chat_tab(tab_id, window, cx);
                    }
                }),
            )
            .on_drag(
                DraggedSideChatTab { tab_id },
                move |_dragged, _offset, _window, cx| {
                    cx.new(|_| ViewTabDragPreview {
                        icon,
                        label: preview_label.clone(),
                    })
                },
            )
            .child(titlebar_svg_icon(
                icon,
                WORKAREA_VIEW_TAB_ICON_SIZE,
                if is_showing {
                    titlebar_active_text_color()
                } else {
                    titlebar_icon_color()
                },
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
                .child(self.render_view_strip_side_chat_tab_close_button(tab_id, is_showing, cx))
            })
            .into_any_element()
    }

    /// Drawn on the showing tab and on the hovered one, like every tab of the strip.
    fn render_view_strip_side_chat_tab_close_button(
        &self,
        tab_id: SideChatTabId,
        is_showing: bool,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        div()
            .id(format!(
                "ghostex-gpui-view-strip-side-chat-tab-close-{}",
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
                    .group_hover(VIEW_STRIP_SIDE_CHAT_TAB_GROUP, |this| this.opacity(1.0))
            })
            .hover(|this| this.bg(titlebar_button_hover_color()))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.close_side_chat_tab(tab_id, window, cx);
                }),
            )
            .child(titlebar_svg_icon(
                TITLEBAR_ICON_X,
                11.0,
                titlebar_icon_color(),
            ))
    }

    /// Bring back and Delete act on the side chat itself (its own chat makes the call); Close tab
    /// only takes it out of the strip.
    fn show_side_chat_tab_context_menu(
        &mut self,
        tab_id: SideChatTabId,
        position: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        GpuiContextMenu::new()
            .menu(
                "Bring back",
                Box::new(BringBackGpuiSideChatTab { tab_id: tab_id.0 }),
            )
            .menu(
                "Delete",
                Box::new(DeleteGpuiSideChatTab { tab_id: tab_id.0 }),
            )
            .separator()
            .menu(
                "Close tab",
                Box::new(CloseGpuiSideChatTab { tab_id: tab_id.0 }),
            )
            .show(position, window, cx);
    }

    /// The panel body while side chats are on screen: the chat of the tab that is showing.
    pub(crate) fn render_side_chat_panel_surface(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let Some(tab_id) = self.side_chat_tab_on_screen() else {
            return div().into_any_element();
        };
        let Some(view) = self.ensure_side_panel_chat(tab_id, cx) else {
            return self.render_session_chat_skeleton();
        };
        // Drawn before the reconcile that marks it shown (a tab picked this frame): ask for it, so
        // its runtime is not left paused.
        if !self.side_panel_chat_marked_shown(tab_id) {
            let app = cx.entity().downgrade();
            cx.defer(move |cx| {
                let _ = app.update(cx, |app, cx| app.reconcile_side_panel_chats(cx));
            });
        }
        div()
            .id(format!("ghostex-gpui-side-chat-panel-{}", tab_id.0))
            .size_full()
            .min_w_0()
            .min_h_0()
            .overflow_hidden()
            .child(AnyView::from(view).cached(gpui::StyleRefinement::default().size_full()))
            .into_any_element()
    }
}
