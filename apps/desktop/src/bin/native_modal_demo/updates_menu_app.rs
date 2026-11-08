//! Stand-ins for the few `crate::app` items the top bar's Updates rows use
//! (app/titlebar/updates_menu_rows.rs) and the Start menu's rows use
//! (app/titlebar/actions_menu_rows.rs), so the demo draws those rows from the app's own files.
//! Colours are the app's dark-theme values.
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
    /// The app's About line for its background helpers (board_gxserver/background_agents.rs).
    #[allow(dead_code)]
    pub(crate) fn gpui_background_helpers_status_line() -> Option<(String, bool)> {
        None
    }
    #[allow(dead_code)]
    pub(crate) fn gpui_open_login_items_settings() {}
    use gpui::{FontWeight, Hsla, IntoElement, ParentElement as _, Styled as _, div, px, rgb, svg};
    use gpui_component::h_flex;

    use super::consts::*;

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
}
