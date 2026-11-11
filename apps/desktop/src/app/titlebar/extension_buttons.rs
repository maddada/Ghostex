use std::time::Duration;

use gpui::AnyElement;
use gpui::FontWeight;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::ParentElement as _;
use gpui::Styled as _;
use gpui::Window;
use gpui::div;
use gpui::img;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui_component::ElementExt;
use gpui_component::Side;
use gpui_component::h_flex;
use gpui_component::menu::PopupMenu;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;
use gpui_component::v_flex;

use crate::*;

use super::popup_menu_builders::titlebar_popup_menu_with_scroll_behavior;

pub(crate) struct TitlebarBadgeButton {
    pub id: ExtensionId,
    pub title: String,
    pub icon_image: std::sync::Arc<gpui::Image>,
    pub badge_lines: Vec<String>,
}

impl GhostexGpuiApp {
    pub(crate) fn build_gpui_titlebar_extensions_popup_menu(
        &self,
        menu: PopupMenu,
        width: f32,
        max_height: f32,
        scrollable: bool,
    ) -> PopupMenu {
        let mut extensions = self
            .extensions_snapshot
            .installed
            .values()
            .filter(|extension| {
                extension.enabled
                    && self.view_scope_allows(&extension_view_scope_key(&extension.id))
            })
            .cloned()
            .collect::<Vec<_>>();
        extensions.sort_by(|left, right| {
            left.title
                .to_ascii_lowercase()
                .cmp(&right.title.to_ascii_lowercase())
                .then_with(|| left.id.cmp(&right.id))
        });
        let mut menu =
            titlebar_popup_menu_with_scroll_behavior(menu, width, max_height, scrollable)
                .check_side(Side::Right);
        if extensions.is_empty() {
            menu = menu.menu_element_with_disabled(
                Box::new(BrowseGpuiExtensions),
                true,
                move |_, _| titlebar_popup_empty_menu_row("No extensions installed".to_string()),
            );
        } else {
            for extension in extensions {
                let extension_id = extension.id.clone();
                menu = menu.menu_element(
                    Box::new(LaunchGpuiExtension { extension_id }),
                    move |_, _| titlebar_popup_extension_menu_row(extension.clone()),
                );
            }
        }
        menu.separator()
            .menu_element(Box::new(BrowseGpuiExtensions), move |_, _| {
                titlebar_popup_standard_menu_row(
                    TITLEBAR_ICON_EXTENSIONS,
                    TITLEBAR_POPUP_MENU_ROW_ICON_SIZE,
                    "Browse extensions…".to_string(),
                    false,
                )
            })
    }

    pub(crate) fn render_titlebar_pinned_extension_buttons(
        &self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Vec<AnyElement> {
        let mut extensions = self
            .extensions_snapshot
            .installed
            .values()
            /*
            CDXC:Extensions 2026-09-18 DECISION:
            User: an extension's view scope decides where it is shown, so a pinned titlebar
            button disappears in a project the extension is not scoped to, exactly like its view tab.
            SEE-ALSO: apps/desktop/src/app/view_scopes.rs.
            */
            .filter(|extension| {
                extension.enabled
                    && extension.pinned
                    && self.view_scope_allows(&extension_view_scope_key(&extension.id))
            })
            .cloned()
            .collect::<Vec<_>>();
        extensions.sort_by(|left, right| {
            left.title
                .to_ascii_lowercase()
                .cmp(&right.title.to_ascii_lowercase())
                .then_with(|| left.id.cmp(&right.id))
        });
        extensions
            .into_iter()
            .map(|extension| self.render_titlebar_pinned_extension_button(extension, window, cx))
            .collect()
    }

    fn render_titlebar_pinned_extension_button(
        &self,
        extension: GpuiInstalledExtension,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let Some(extension_id) = ExtensionId::new(&extension.id) else {
            return div().size_0().into_any_element();
        };
        self.render_titlebar_badge_button(
            TitlebarBadgeButton {
                id: extension_id,
                title: extension.title,
                icon_image: extension.icon_image,
                badge_lines: extension.badge_lines,
            },
            window,
            cx,
        )
    }

    pub(crate) fn render_titlebar_badge_button(
        &self,
        button: TitlebarBadgeButton,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let extension_id = button.id;
        let open = self
            .titlebar_extension_popup
            .as_ref()
            .is_some_and(|state| state.id == extension_id && !state.account);
        let anchor_state = window.use_keyed_state(
            format!(
                "ghostex-gpui-titlebar-extension-{}-anchor",
                button.id.as_str()
            ),
            cx,
            |_, _| GpuiTitlebarPopupAnchorState::default(),
        );
        let anchor_bounds = anchor_state.read(cx).bounds;
        let trigger_bounds = anchor_state
            .read(cx)
            .trigger_bounds_captured
            .then_some(anchor_bounds);
        let badge_lines = button
            .badge_lines
            .iter()
            .filter(|line| !line.trim().is_empty())
            .take(2)
            .cloned()
            .collect::<Vec<_>>();
        let show_badge = !badge_lines.is_empty();
        let tooltip = button.title.clone();
        let icon_image = button.icon_image.clone();

        let icon = |size| {
            div()
                .relative()
                .size(px(size))
                .flex()
                .items_center()
                .justify_center()
                .flex_shrink_0()
                .child(
                    img(icon_image.clone())
                        .absolute()
                        .top_0()
                        .left_0()
                        .size(px(size)),
                )
        };
        div()
            .flex_shrink_0()
            .id(format!(
                "ghostex-gpui-titlebar-pinned-extension-{}",
                button.id.as_str()
            ))
            .relative()
            .flex()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .px(px(TITLEBAR_BUTTON_HORIZONTAL_PADDING))
            .items_center()
            .justify_center()
            .rounded(px(TITLEBAR_BUTTON_RADIUS))
            .when(cfg!(target_os = "windows"), |this| this.occlude())
            .cursor_default()
            .when(open, |this| this.bg(titlebar_active_segment_color()))
            .hover(move |this| {
                if open {
                    this.bg(titlebar_active_segment_color())
                } else {
                    this.bg(titlebar_button_hover_color())
                }
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    let Some(trigger_bounds) = trigger_bounds else {
                        window.request_animation_frame();
                        return;
                    };
                    this.close_gpui_titlebar_popup(None, window, cx);
                    this.launch_extension_from_titlebar(
                        extension_id.as_str(),
                        trigger_bounds,
                        window,
                        cx,
                    );
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener({
                    let extension_id = button.id.as_str().to_string();
                    move |this, event: &MouseDownEvent, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.show_gpui_titlebar_pinned_extension_menu(
                            extension_id.clone(),
                            event.position,
                            window,
                            cx,
                        );
                    }
                }),
            )
            .when(!open, |this| {
                this.managed_discrete_tooltip_with_placement(
                    ManagedTooltipPlacement::Left,
                    Duration::from_millis(300),
                    move |window, cx| titlebar_tooltip(tooltip.clone(), window, cx),
                )
            })
            .on_prepaint({
                let anchor_state = anchor_state.clone();
                move |bounds, window, cx| {
                    let request_frame = anchor_state.update(cx, |state, _| {
                        let changed = !state.trigger_bounds_captured || state.bounds != bounds;
                        state.bounds = bounds;
                        state.trigger_bounds_captured = true;
                        changed
                    });
                    if request_frame {
                        window.request_animation_frame();
                    }
                }
            })
            .map(|this| {
                if show_badge {
                    this.child(
                        h_flex().gap(px(4.0)).child(icon(14.0)).child(
                            v_flex()
                                .text_size(px(10.5))
                                .line_height(px(10.5))
                                .font_weight(FontWeight::SEMIBOLD)
                                .text_color(chrome_color(0xb9b9b9, 0x404040))
                                .children(badge_lines),
                        ),
                    )
                } else {
                    this.child(icon(18.0))
                }
            })
            .into_any_element()
    }
}
