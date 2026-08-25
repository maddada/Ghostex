use std::time::Duration;

use gpui::AnyElement;
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
use gpui_component::scroll::ScrollbarShow;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;
use gpui_component::v_flex;

use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn build_gpui_titlebar_extensions_popup_menu(&self, menu: PopupMenu) -> PopupMenu {
        let mut extensions = self
            .extensions_snapshot
            .installed
            .values()
            .filter(|extension| extension.enabled)
            .cloned()
            .collect::<Vec<_>>();
        extensions.sort_by(|left, right| {
            left.title
                .to_ascii_lowercase()
                .cmp(&right.title.to_ascii_lowercase())
                .then_with(|| left.id.cmp(&right.id))
        });
        let mut menu = menu
            .min_w(px(TITLEBAR_POPUP_EXTENSIONS_WIDTH))
            .max_w(px(TITLEBAR_POPUP_EXTENSIONS_WIDTH))
            .max_h(px(
                TITLEBAR_POPUP_MENU_MAX_HEIGHT - TITLEBAR_POPUP_MENU_BORDER_CHROME
            ))
            .items_padding_bottom(px(0.0))
            .scrollable(true)
            .scrollbar_thickness(px(TITLEBAR_DROPDOWN_SCROLLBAR_WIDTH))
            .scrollbar_show(ScrollbarShow::Scrolling)
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

    pub(crate) fn render_titlebar_extensions_button(
        &self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        self.render_titlebar_native_popup_button(
            GpuiTitlebarPopupKind::Extensions,
            TITLEBAR_ICON_EXTENSIONS,
            TITLEBAR_EXTENSIONS_TOOLTIP,
            false,
            window,
            cx,
        )
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
            .filter(|extension| extension.enabled && extension.pinned)
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
        let open = self
            .titlebar_extension_popup
            .as_ref()
            .is_some_and(|state| state.id == extension_id);
        let anchor_state = window.use_keyed_state(
            format!("ghostex-gpui-titlebar-extension-{}-anchor", extension.id),
            cx,
            |_, _| GpuiTitlebarPopupAnchorState::default(),
        );
        let anchor_bounds = anchor_state.read(cx).bounds;
        let trigger_bounds = anchor_state
            .read(cx)
            .trigger_bounds_captured
            .then_some(anchor_bounds);
        let badge_lines = extension
            .badge_lines
            .iter()
            .filter(|line| !line.trim().is_empty())
            .take(2)
            .cloned()
            .collect::<Vec<_>>();
        let button_width = if badge_lines.is_empty() {
            TITLEBAR_BUTTON_WIDTH
        } else {
            84.0
        };
        let tooltip = extension.title.clone();
        let icon_data_url = extension.icon_data_url.clone();

        h_flex()
            .id(format!(
                "ghostex-gpui-titlebar-pinned-extension-{}",
                extension.id
            ))
            .relative()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .w(px(button_width))
            .items_center()
            .justify_center()
            .gap(px(5.0))
            .when(cfg!(target_os = "windows"), |this| this.occlude())
            .border_l_1()
            .border_color(titlebar_button_border_color())
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
            .child(img(icon_data_url).size(px(18.0)))
            .when(!badge_lines.is_empty(), |this| {
                this.child(
                    v_flex()
                        .min_w_0()
                        .max_w(px(48.0))
                        .overflow_hidden()
                        .text_size(px(9.0))
                        .line_height(px(10.0))
                        .text_color(titlebar_inactive_text_color())
                        .children(badge_lines.into_iter().map(|line| {
                            div()
                                .overflow_hidden()
                                .whitespace_nowrap()
                                .text_ellipsis()
                                .child(line)
                        })),
                )
            })
            .into_any_element()
    }
}
