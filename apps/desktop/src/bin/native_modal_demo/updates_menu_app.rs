//! Stand-ins for the few `crate::app` items the top bar's Updates rows use
//! (app/titlebar/updates_menu_rows.rs) and the Start menu's rows use
//! (app/titlebar/actions_menu_rows.rs), so the demo draws those rows from the app's own files.
//! Colours are the app's dark-theme values.
#[path = "../../app/element/count_badge.rs"]
pub(crate) mod element;

/// The chat host's computer icons (`app/gx_chat/session_directory.rs`): the preview has no
/// computers, so every one wears the plain computer.
pub(crate) mod gx_chat {
    pub(crate) fn current_model_catalog()
    -> Option<ghostex_gx_chat_core::menus::catalog::AgentModelCatalog> {
        None
    }

    /// Settings > Autopilot's supervisor pickers star models; the preview keeps none.
    pub(crate) fn model_favorites() -> Vec<String> {
        Vec::new()
    }

    pub(crate) fn toggle_model_favorite(_key: &str) -> Vec<String> {
        Vec::new()
    }

    pub(crate) mod session_directory {
        pub(crate) fn machine_icon(_machine_id: &str) -> &'static str {
            "titlebar/device-desktop.svg"
        }
    }
}

pub(crate) mod actions {
    #[derive(Clone, Copy, Debug, PartialEq, Eq, gpui::Action)]
    #[action(namespace = ghostex_gpui, no_json)]
    pub(crate) struct RunGpuiTitlebarUpdates {
        pub(crate) update_all: bool,
    }

    #[derive(Clone, Debug, PartialEq, Eq, gpui::Action)]
    #[action(namespace = ghostex_gpui, no_json)]
    pub(crate) struct RunGpuiTitlebarUpdateItem {
        pub(crate) id: String,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, gpui::Action)]
    #[action(namespace = ghostex_gpui, no_json)]
    pub(crate) struct RunGpuiTitlebarAction {
        pub(crate) action_index: u64,
    }

    #[derive(Clone, Debug, PartialEq, Eq, gpui::Action)]
    #[action(namespace = ghostex_gpui, no_json)]
    pub(crate) struct RunGpuiTitlebarGlobalAction {
        pub(crate) command_id: String,
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq, gpui::Action)]
    #[action(namespace = ghostex_gpui, no_json)]
    pub(crate) struct ConfigureGpuiTitlebarActions;

    #[derive(Clone, Copy, Debug, PartialEq, Eq, gpui::Action)]
    #[action(namespace = ghostex_gpui, no_json)]
    pub(crate) struct CopyGpuiTitlebarGitBranch;

    #[derive(Clone, Debug, PartialEq, Eq, gpui::Action)]
    #[action(namespace = ghostex_gpui, no_json)]
    pub(crate) struct DismissGpuiTitlebarUpdateFailure {
        pub(crate) id: String,
        pub(crate) error: String,
    }
}

pub(crate) mod consts {
    pub(crate) const TITLEBAR_ICON_DOWNLOAD: &str = "titlebar/download.svg";
    pub(crate) const TITLEBAR_ICON_SETTINGS: &str = "titlebar/settings.svg";
    pub(crate) const TITLEBAR_POPUP_ACTION_ROW_HEIGHT: f32 = 44.0;
    pub(crate) const TITLEBAR_POPUP_ACTION_PREVIEW_TEXT_SIZE: f32 = 11.0;
    pub(crate) const TITLEBAR_POPUP_GIT_SECTION_LABEL_HEIGHT: f32 = 22.0;
    pub(crate) const TITLEBAR_POPUP_MENU_ITEM_PADDING_X: f32 = 10.0;
    pub(crate) const TITLEBAR_POPUP_MENU_ROW_HEIGHT: f32 = 34.0;
    pub(crate) const TITLEBAR_POPUP_MENU_ROW_ICON_SIZE: f32 = 14.0;
    pub(crate) const TITLEBAR_POPUP_MENU_ROW_TEXT_SIZE: f32 = 13.0;
    pub(crate) const TITLEBAR_UPDATE_ICON_SIZE: f32 = 14.0;
}

pub(crate) mod helpers {
    use gpui::{FontWeight, Hsla, IntoElement, ParentElement as _, Styled as _, div, px, rgb, svg};
    use gpui_component::h_flex;

    /// An agent logo's brand tone (app/helpers): the preview draws every logo white.
    pub(crate) fn workspace_tab_agent_icon_accent_color(_icon: &str) -> u32 {
        0xffffff
    }

    use super::consts::*;

    /// The app's spinner rate (app/helpers/indicator_animation.rs).
    pub(crate) const INDICATOR_MAX_FPS: f32 = 30.0;

    /// The demo paints no theme artwork, so Settings never draws its frosted glass over one
    /// (app/helpers/window_art.rs `window_art_frosted` and `frosted_glass_backdrop`).
    pub(crate) fn window_art_frosted() -> Option<()> {
        None
    }

    /// app/helpers/remote/ssh_connection_share.rs: whether a saved computer's last connect asked
    /// for an SSH sign-in in a terminal. The demo says so when `GHOSTEX_DEMO_SSH_SIGN_IN` is set.
    pub(crate) fn gpui_remote_ssh_sign_in_needed(_remote_machine_id: &str) -> bool {
        std::env::var_os("GHOSTEX_DEMO_SSH_SIGN_IN").is_some()
    }

    pub(crate) fn frosted_glass_backdrop(
        _radii: gpui::Corners<gpui::Pixels>,
        _light: bool,
    ) -> Option<gpui::AnyElement> {
        None
    }

    /// The app's theme roles (app/helpers/theme_roles.rs): backed by real catalog resolution so
    /// `native-modal-demo` can preview any built-in theme with its exact tokens.
    struct DemoRoles {
        dark: Vec<(
            &'static str,
            ghostex_settings_catalog::theme_roles::RoleValue,
        )>,
        light: Vec<(
            &'static str,
            ghostex_settings_catalog::theme_roles::RoleValue,
        )>,
        dark_base: u32,
        light_base: u32,
        dark_accent: u32,
        light_accent: u32,
        dark_modal_surface: u32,
        light_modal_surface: u32,
    }

    static DEMO_ROLES: std::sync::RwLock<DemoRoles> = std::sync::RwLock::new(DemoRoles {
        dark: Vec::new(),
        light: Vec::new(),
        dark_base: 0x0d0d0d,
        light_base: 0xf7f7f7,
        dark_accent: 0x95d7f6,
        light_accent: 0x0c74a6,
        dark_modal_surface: 0x13161b,
        light_modal_surface: 0xfefdf9,
    });

    pub(crate) fn apply_demo_theme_roles(settings: &serde_json::Map<String, serde_json::Value>) {
        use ghostex_settings_catalog::theme_colours::{
            resolved_custom_sidebar_titlebar_background_for_variant,
            sidebar_titlebar_menu_background_for_chrome, theme_accent_for_variant,
            work_area_background_for_variant,
        };
        use ghostex_settings_catalog::theme_roles::roles_for;
        let dark_chrome = resolved_custom_sidebar_titlebar_background_for_variant(settings, false);
        let light_chrome = resolved_custom_sidebar_titlebar_background_for_variant(settings, true);
        let roles = DemoRoles {
            dark: roles_for(settings, false),
            light: roles_for(settings, true),
            dark_base: work_area_background_for_variant(settings, false),
            light_base: work_area_background_for_variant(settings, true),
            dark_accent: theme_accent_for_variant(settings, false),
            light_accent: theme_accent_for_variant(settings, true),
            dark_modal_surface: sidebar_titlebar_menu_background_for_chrome(dark_chrome),
            light_modal_surface: sidebar_titlebar_menu_background_for_chrome(light_chrome),
        };
        if let Ok(mut saved) = DEMO_ROLES.write() {
            *saved = roles;
        }
    }

    pub(crate) fn theme_role_rgba(id: &str, light: bool) -> Option<gpui::Rgba> {
        let roles = DEMO_ROLES.read().ok()?;
        let (list, base) = if light {
            (&roles.light, roles.light_base)
        } else {
            (&roles.dark, roles.dark_base)
        };
        let [r, g, b, a] = list
            .iter()
            .find(|(role, _)| *role == id)?
            .1
            .rgba_on_base(light, base);
        Some(gpui::Rgba { r, g, b, a })
    }

    pub(crate) fn theme_role_or(id: &str, light: bool, fallback: Hsla) -> Hsla {
        theme_role_rgba(id, light).map_or(fallback, Hsla::from)
    }

    pub(crate) fn theme_role_or_rgba(id: &str, light: bool, fallback: gpui::Rgba) -> gpui::Rgba {
        theme_role_rgba(id, light).unwrap_or(fallback)
    }

    pub(crate) fn theme_accent_rgba(light: bool) -> gpui::Rgba {
        if let Some(accent) = theme_role_rgba("accent", light) {
            return gpui::Rgba { a: 1.0, ..accent };
        }
        let fallback =
            DEMO_ROLES
                .read()
                .ok()
                .map_or(if light { 0x0c74a6 } else { 0x95d7f6 }, |roles| {
                    if light {
                        roles.light_accent
                    } else {
                        roles.dark_accent
                    }
                });
        rgb(fallback)
    }

    pub(crate) fn demo_modal_surface_rgba(light: bool) -> gpui::Rgba {
        let base_rgb =
            DEMO_ROLES
                .read()
                .ok()
                .map_or(if light { 0xfefdf9 } else { 0x13161b }, |roles| {
                    if light {
                        roles.light_modal_surface
                    } else {
                        roles.dark_modal_surface
                    }
                });
        let derived: Hsla = rgb(base_rgb).into();
        let blended =
            theme_role_rgba("menu", light).map_or(derived, |menu| derived.blend(Hsla::from(menu)));
        blended.alpha(1.0).into()
    }

    pub(crate) fn titlebar_active_text_color() -> Hsla {
        rgb(0xffffff).into()
    }
    pub(crate) fn titlebar_inactive_text_color() -> Hsla {
        rgb(0xffffff).opacity(0.68).into()
    }
    pub(crate) fn titlebar_update_available_color() -> Hsla {
        titlebar_active_text_color()
    }
    pub(crate) fn titlebar_button_hover_color() -> Hsla {
        rgb(0xffffff).opacity(0.08).into()
    }
    pub(crate) fn titlebar_popup_menu_foreground() -> Hsla {
        rgb(0xfcfcfc).into()
    }
    pub(crate) fn titlebar_popup_menu_destructive() -> Hsla {
        rgb(0xf87171).into()
    }
    pub(crate) fn titlebar_popup_menu_hover_color() -> Hsla {
        rgb(0xffffff).opacity(0.08).into()
    }
    pub(crate) fn titlebar_popup_menu_border_color() -> Hsla {
        rgb(0xffffff).opacity(0.12).into()
    }
    pub(crate) fn titlebar_popup_menu_background() -> Hsla {
        rgb(0x2b2c30).into()
    }
    pub(crate) fn titlebar_tooltip(
        text: impl Into<gpui_component::text::Text>,
        window: &mut gpui::Window,
        cx: &mut gpui::App,
    ) -> gpui::AnyView {
        gpui_component::tooltip::Tooltip::new(text).build(window, cx)
    }
    /// The app's `titlebar_popup_standard_menu_row` (helpers/titlebar/popup_rows.rs).
    pub(crate) fn titlebar_popup_standard_menu_row(
        icon_path: &'static str,
        icon_size: f32,
        label: String,
        _disabled: bool,
    ) -> impl IntoElement {
        h_flex()
            .min_w_0()
            .flex_1()
            .min_h(px(TITLEBAR_POPUP_MENU_ROW_HEIGHT))
            .items_center()
            .gap(px(8.0))
            .text_size(px(TITLEBAR_POPUP_MENU_ROW_TEXT_SIZE))
            .font_weight(FontWeight::NORMAL)
            .text_color(titlebar_popup_menu_foreground())
            .child(
                div()
                    .flex()
                    .w(px(icon_size.max(TITLEBAR_POPUP_MENU_ROW_ICON_SIZE)))
                    .items_center()
                    .justify_center()
                    .child(
                        svg()
                            .size(px(icon_size))
                            .path(icon_path)
                            .text_color(titlebar_popup_menu_foreground()),
                    ),
            )
            .child(div().child(label))
    }

    /// The app's `GpuiTitlebarAction`, cut to what the Start menu's rows read.
    #[derive(Clone, Debug)]
    pub(crate) struct GpuiTitlebarAction {
        pub(crate) command_id: String,
        pub(crate) name: String,
        pub(crate) icon_path: &'static str,
        /// The command or URL; empty for an Action with neither yet.
        pub(crate) preview: String,
    }

    /// The app's `titlebar_popup_empty_menu_row`.
    pub(crate) fn titlebar_popup_empty_menu_row(label: String) -> impl IntoElement {
        h_flex()
            .flex_1()
            .min_h(px(TITLEBAR_POPUP_MENU_ROW_HEIGHT))
            .items_center()
            .text_size(px(TITLEBAR_POPUP_MENU_ROW_TEXT_SIZE))
            .text_color(rgb(0xffffff).opacity(0.42))
            .child(label)
    }

    /// The app's `titlebar_popup_git_section` (a disabled heading row).
    pub(crate) fn titlebar_popup_git_section(
        menu: gpui_component::menu::PopupMenu,
        label: impl Into<gpui::SharedString>,
    ) -> gpui_component::menu::PopupMenu {
        let label = label.into();
        menu.menu_element_with_disabled(
            Box::new(super::actions::CopyGpuiTitlebarGitBranch),
            true,
            move |_, _| {
                h_flex()
                    .w_full()
                    .min_h(px(TITLEBAR_POPUP_GIT_SECTION_LABEL_HEIGHT))
                    .items_center()
                    .text_size(px(11.0))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgb(0xffffff).opacity(0.55))
                    .child(label.clone())
            },
        )
    }

    /// The app's `titlebar_popup_action_menu_row`: icon, name, and the command or URL below it.
    pub(crate) fn titlebar_popup_action_menu_row(action: GpuiTitlebarAction) -> impl IntoElement {
        use gpui::prelude::FluentBuilder as _;
        let unconfigured = action.preview.is_empty();
        let preview = if unconfigured {
            "Set the command".to_string()
        } else {
            action.preview.clone()
        };
        h_flex()
            .min_w_0()
            .flex_1()
            .overflow_hidden()
            .min_h(px(TITLEBAR_POPUP_ACTION_ROW_HEIGHT))
            .items_start()
            .gap(px(10.0))
            .py(px(6.0))
            .text_color(titlebar_popup_menu_foreground())
            .child(
                div()
                    .flex()
                    .w(px(TITLEBAR_POPUP_MENU_ROW_ICON_SIZE))
                    .pt(px(1.0))
                    .justify_center()
                    .child(
                        svg()
                            .size(px(TITLEBAR_POPUP_MENU_ROW_ICON_SIZE))
                            .path(action.icon_path)
                            .text_color(rgb(0xffffff).opacity(0.84)),
                    ),
            )
            .child(
                gpui_component::v_flex()
                    .min_w(px(0.0))
                    .flex_1()
                    .overflow_hidden()
                    .gap(px(1.0))
                    .child(
                        div()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(TITLEBAR_POPUP_MENU_ROW_TEXT_SIZE))
                            .line_height(px(16.0))
                            .child(action.name.clone()),
                    )
                    .child(
                        div()
                            .whitespace_nowrap()
                            .text_ellipsis()
                            .text_size(px(TITLEBAR_POPUP_ACTION_PREVIEW_TEXT_SIZE))
                            .line_height(px(14.0))
                            .text_color(rgb(0xffffff).opacity(0.48))
                            .when(unconfigured, |this| this.italic())
                            .child(preview),
                    ),
            )
    }
}

pub(crate) mod window {
    pub(crate) use crate::native_modal_kit;
    pub(crate) use crate::round_close_button;

    /// The Agents Hub's key interceptor asks the app's text-field menu (app/window/text_field_menu.rs)
    /// whether a key is the menu's. The demo draws no such menu, so no key is.
    pub(crate) mod text_field_menu {
        pub(crate) fn takes_keystroke(_keystroke: &gpui::Keystroke) -> bool {
            false
        }
    }
}

/// The sidebar items Settings reads (app/native_sidebar): the Ask Ghostex AI flower icon
/// (Settings > Extensions) and the 4pt corner dot an account with an issue wears (Settings >
/// Accounts). The demo already compiles the chief row's own file, so that one is the app's.
pub(crate) mod native_sidebar {
    pub(crate) use crate::chief_row_view;

    pub(crate) mod status {
        use gpui::{Hsla, ParentElement as _, SharedString, Styled as _, div, px};

        /// Stand-in for app/native_sidebar/status.rs `corner_dot`: the same 4pt dot, 3pt in from
        /// the top-right corner, with a 1pt ring of what it sits on; the preview has no hover.
        pub(crate) fn corner_dot(
            color: Hsla,
            scale: f32,
            ring: Hsla,
            _hover: Option<(Hsla, SharedString)>,
        ) -> gpui::Div {
            let offset = 2.0 * scale;
            div()
                .absolute()
                .top(px(offset))
                .right(px(offset))
                .size(px(6.0 * scale))
                .rounded_full()
                .bg(ring)
                .p(px(scale))
                .child(div().size(px(4.0 * scale)).rounded_full().bg(color))
        }
    }
}
