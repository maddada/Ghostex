// The titlebar's trailing "⋯" button and the menu behind it: the surfaces that are
// occasional rather than part of the active project. Each row opens the same native
// child window its own titlebar button used to open, re-anchored to this button.

use gpui::AnyElement;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::ParentElement as _;
use gpui::Styled as _;
use gpui::Window;
use gpui::div;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui_component::ElementExt as _;
use gpui_component::Side;
use gpui_component::menu::PopupMenu;

use super::popup_menu_builders::titlebar_popup_menu_with_scroll_behavior;
use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::window::*;
use crate::*;

/// CDXC:Titlebar 2026-09-20 DECISION:
/// User: Ask Ghostex, Tips & Tricks, Resources, Dev servers and Extensions leave the titlebar for one trailing "⋯" menu, so the titlebar row holds only the active project's own controls.
/// An item switched off in Settings, or scoped away from this project, is not listed at all.
/// CDXC:Titlebar 2026-09-21 DECISION:
/// User: "make these options go back to opening as gpui overlays just like they did before... i dont like that they are opening as full tabs in the right side view panel". Every row opens its overlay anchored to the ⋯ button; this supersedes the 2026-09-20 ruling that made Ask Ghostex, Tips & Tricks and Resources view tabs and sent Dev servers to the Browser view. Ask Ghostex, Tips & Tricks and Resources stay app-wide: a Settings switch each, never scoped away from a project. Since 2026-09-22 this dropdown is the list's only home: the user asked that a blank Browser tab stop showing it, because it appeared unbidden in the side panel and its Open buttons did nothing there.
/// User: "add customize as an option bottom of this, which opens the extensions page in settings". Customize is always listed, so there is a way back after every other row is switched off.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum GpuiTitlebarMoreMenuItem {
    AskGhostex,
    Tips,
    Resources,
    DevServers,
    Extensions,
    Customize,
}

impl GpuiTitlebarMoreMenuItem {
    /// Menu order, top to bottom.
    const ALL: [Self; 6] = [
        Self::AskGhostex,
        Self::Tips,
        Self::Resources,
        Self::DevServers,
        Self::Extensions,
        Self::Customize,
    ];

    pub(crate) fn from_index(index: u64) -> Option<Self> {
        Self::ALL.get(usize::try_from(index).ok()?).copied()
    }

    fn index(self) -> u64 {
        Self::ALL
            .iter()
            .position(|item| *item == self)
            .unwrap_or_default() as u64
    }

    fn label(self) -> &'static str {
        match self {
            Self::AskGhostex => "Ask Ghostex",
            Self::Tips => "Tips & Tricks",
            Self::Resources => "Resources",
            Self::DevServers => "Dev servers",
            Self::Extensions => "Extensions",
            Self::Customize => "Customize",
        }
    }

    fn icon_path(self) -> &'static str {
        match self {
            Self::AskGhostex => TITLEBAR_ICON_HELP,
            Self::Tips => TITLEBAR_ICON_INFO,
            Self::Resources => TITLEBAR_ICON_DEVICE_DESKTOP,
            Self::DevServers => BROWSER_ICON_WORLD,
            Self::Extensions => TITLEBAR_ICON_EXTENSIONS,
            Self::Customize => TITLEBAR_ICON_SETTINGS,
        }
    }

    /// The Settings switch and the view scope key this item's titlebar
    /// button was gated by, so the one gate keeps deciding whether it is offered.
    /// Customize has none: it is the way back to those switches.
    fn visibility_gate(self) -> Option<(&'static str, &'static str)> {
        match self {
            Self::AskGhostex => Some((HELP_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY, "help")),
            Self::Tips => Some((TIPS_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY, "tips")),
            Self::Resources => Some((RESOURCES_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY, "resources")),
            Self::DevServers => Some((
                DEV_SERVERS_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY,
                "devServers",
            )),
            Self::Extensions => Some((
                EXTENSIONS_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY,
                "extensionsButton",
            )),
            Self::Customize => None,
        }
    }

    /// Ask Ghostex, Tips & Tricks and Resources belong to the app rather than to a project, so only
    /// their Settings switch can take them away, never a view scope.
    fn app_wide(self) -> bool {
        matches!(self, Self::AskGhostex | Self::Tips | Self::Resources)
    }

    /// The overlay a row opens; Customize opens Settings instead.
    fn popup_kind(self) -> Option<GpuiTitlebarPopupKind> {
        match self {
            Self::AskGhostex => Some(GpuiTitlebarPopupKind::Help),
            Self::Tips => Some(GpuiTitlebarPopupKind::Tips),
            Self::Resources => Some(GpuiTitlebarPopupKind::Resources),
            Self::DevServers => Some(GpuiTitlebarPopupKind::RemoteSites),
            Self::Extensions => Some(GpuiTitlebarPopupKind::Extensions),
            Self::Customize => None,
        }
    }
}

impl GhostexGpuiApp {
    /// The rows the ⋯ menu offers right now, after the single titlebar-button gate.
    pub(crate) fn titlebar_more_menu_items(&self) -> Vec<GpuiTitlebarMoreMenuItem> {
        GpuiTitlebarMoreMenuItem::ALL
            .into_iter()
            .filter(|item| {
                let Some((settings_key, official_extension_id)) = item.visibility_gate() else {
                    return true;
                };
                if item.app_wide() {
                    !shared_settings::shared_sidebar_settings_snapshot()
                        .object()
                        .get(settings_key)
                        .and_then(serde_json::Value::as_bool)
                        .unwrap_or(false)
                } else {
                    !self.titlebar_button_hidden(settings_key, official_extension_id)
                }
            })
            .collect()
    }

    pub(crate) fn titlebar_more_menu_visible(&self) -> bool {
        !self.titlebar_more_menu_items().is_empty()
    }

    pub(crate) fn titlebar_more_popup_content_height(&self) -> f32 {
        let mut rows = vec![TITLEBAR_POPUP_MENU_ROW_HEIGHT; self.titlebar_more_menu_items().len()];
        rows.push(TITLEBAR_POPUP_MENU_SEPARATOR_HEIGHT);
        titlebar_popup_menu_height_for_rows(&rows)
    }

    pub(crate) fn build_gpui_titlebar_more_popup_menu(
        &self,
        menu: PopupMenu,
        width: f32,
        max_height: f32,
        scrollable: bool,
    ) -> PopupMenu {
        let mut menu =
            titlebar_popup_menu_with_scroll_behavior(menu, width, max_height, scrollable)
                .check_side(Side::Right);
        for item in self.titlebar_more_menu_items() {
            if item == GpuiTitlebarMoreMenuItem::Customize {
                menu = menu.separator();
            }
            menu = menu.menu_element(
                Box::new(OpenGpuiTitlebarMoreMenuItem {
                    item_index: item.index(),
                }),
                move |_, _| {
                    titlebar_popup_standard_menu_row(
                        item.icon_path(),
                        TITLEBAR_POPUP_MENU_ROW_ICON_SIZE,
                        item.label().to_string(),
                        false,
                    )
                },
            );
        }
        menu
    }

    /// Opens one row's overlay anchored to the ⋯ button, or Settings → Extensions for Customize.
    /// The gate is re-checked because the menu row and the click are separated by a frame.
    pub(crate) fn open_titlebar_more_menu_item(
        &mut self,
        item: GpuiTitlebarMoreMenuItem,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.titlebar_more_menu_items().contains(&item) {
            return;
        }
        let Some(kind) = item.popup_kind() else {
            self.open_gpui_settings_extensions_page(Some(window), cx);
            return;
        };
        let trigger_bounds = self.titlebar_more_button_bounds.get();
        self.set_gpui_titlebar_popup_open(kind, true, trigger_bounds, window, cx);
    }

    pub(crate) fn toggle_gpui_titlebar_more_menu(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !self.titlebar_more_menu_visible() {
            return;
        }
        let open = !self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::More);
        let trigger_bounds = self.titlebar_more_button_bounds.get();
        self.set_gpui_titlebar_popup_open(
            GpuiTitlebarPopupKind::More,
            open,
            trigger_bounds,
            window,
            cx,
        );
    }

    /// The trailing ⋯ button. It records its own bounds because the Ghostex Help
    /// hotkey opens the Help panel without going through the menu and still has to
    /// anchor it somewhere real.
    /// CDXC:Titlebar 2026-09-21 DECISION:
    /// User: "please remove the blue dot shown on the 'More' dropdown trigger button". The ⋯ button carries no unread-tips badge.
    pub(crate) fn render_titlebar_more_button(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let open = self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::More);
        let icon_color = if open {
            titlebar_icon_hover_color()
        } else {
            titlebar_icon_color()
        };
        let button_bounds = self.titlebar_more_button_bounds.clone();

        div()
            .id("ghostex-gpui-titlebar-button-more")
            .relative()
            .flex()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .px(px(TITLEBAR_BUTTON_HORIZONTAL_PADDING))
            .items_center()
            .justify_center()
            .rounded(px(TITLEBAR_BUTTON_RADIUS))
            .when(cfg!(target_os = "windows"), |this| this.occlude())
            .text_color(icon_color)
            .cursor_default()
            .when(open, |this| this.bg(titlebar_active_segment_color()))
            .hover(move |this| {
                if open {
                    this.bg(titlebar_active_segment_color())
                        .text_color(titlebar_icon_hover_color())
                } else {
                    this.bg(titlebar_button_hover_color())
                        .text_color(titlebar_icon_hover_color())
                }
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    log_gpui_titlebar_popup_mouse_down(
                        GpuiTitlebarPopupKind::More,
                        "left",
                        "togglePopup",
                        open,
                        this.titlebar_more_button_bounds.get(),
                        event,
                        window,
                    );
                    this.toggle_gpui_titlebar_more_menu(window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    log_gpui_titlebar_popup_mouse_down(
                        GpuiTitlebarPopupKind::More,
                        "right",
                        "togglePopup",
                        open,
                        this.titlebar_more_button_bounds.get(),
                        event,
                        window,
                    );
                    this.toggle_gpui_titlebar_more_menu(window, cx);
                }),
            )
            .on_prepaint(move |bounds, window, _cx| {
                let previous = button_bounds.get();
                let first_capture = previous.is_none();
                let moved = previous != Some(bounds);
                button_bounds.set(Some(bounds));
                if first_capture || moved {
                    log_gpui_titlebar_popup_anchor(
                        GpuiTitlebarPopupKind::More,
                        bounds,
                        first_capture,
                        moved,
                        window,
                    );
                    window.request_animation_frame();
                }
            })
            .child(titlebar_svg_icon(TITLEBAR_ICON_DOTS, 16.0, icon_color))
            .into_any_element()
    }
}
