use gpui::{
    FontWeight, InteractiveElement as _, IntoElement, ParentElement as _, Styled as _, div, img,
    prelude::FluentBuilder as _, px, rgb,
};
use gpui_component::{
    h_flex,
    menu::PopupMenu,
    tooltip::{ManagedTooltipExt as _, ManagedTooltipPlacement},
    v_flex,
};

use super::*;
use crate::*;

pub(crate) fn titlebar_popup_standard_menu_row(
    icon_path: &'static str,
    icon_size: f32,
    label: String,
    disabled: bool,
) -> impl IntoElement {
    let text_color = if disabled {
        titlebar_popup_menu_disabled_text_color()
    } else {
        titlebar_popup_menu_foreground()
    };
    let icon_color = if disabled {
        titlebar_popup_menu_disabled_text_color()
    } else {
        titlebar_popup_menu_foreground()
    };

    h_flex()
        .min_w_0()
        .max_w_full()
        .flex_1()
        .overflow_hidden()
        .min_h(px(TITLEBAR_POPUP_MENU_ROW_HEIGHT))
        .items_center()
        .gap(px(8.0))
        .rounded(px(4.0))
        .text_size(px(TITLEBAR_POPUP_MENU_ROW_TEXT_SIZE))
        .font_weight(FontWeight::NORMAL)
        .text_color(text_color)
        .child(
            div()
                .flex()
                .w(px(icon_size.max(TITLEBAR_POPUP_MENU_ROW_ICON_SIZE)))
                .items_center()
                .justify_center()
                .child(titlebar_svg_icon(icon_path, icon_size, icon_color)),
        )
        .child(
            div()
                .min_w_0()
                .max_w_full()
                .flex_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(label),
        )
}

pub(crate) fn titlebar_popup_empty_menu_row(label: String) -> impl IntoElement {
    h_flex()
        .min_w_0()
        .max_w_full()
        .flex_1()
        .overflow_hidden()
        .min_h(px(TITLEBAR_POPUP_MENU_ROW_HEIGHT))
        .items_center()
        .rounded(px(4.0))
        .text_size(px(TITLEBAR_POPUP_MENU_ROW_TEXT_SIZE))
        .font_weight(FontWeight::NORMAL)
        .text_color(titlebar_popup_menu_disabled_text_color())
        .child(
            div()
                .min_w_0()
                .max_w_full()
                .flex_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(label),
        )
}

pub(crate) fn titlebar_popup_action_menu_row(action: GpuiTitlebarAction) -> impl IntoElement {
    let icon_path = titlebar_action_icon_path(Some(&action));
    let label = action.titlebar_menu_name();
    let (preview, preview_unconfigured) = action.titlebar_menu_preview();

    h_flex()
        .min_w_0()
        .max_w_full()
        .flex_1()
        .overflow_hidden()
        .min_h(px(TITLEBAR_POPUP_ACTION_ROW_HEIGHT))
        .items_start()
        .gap(px(10.0))
        .rounded(px(4.0))
        .py(px(6.0))
        .text_color(titlebar_popup_menu_foreground())
        .child(
            div()
                .flex()
                .w(px(TITLEBAR_POPUP_MENU_ROW_ICON_SIZE))
                .pt(px(1.0))
                .items_center()
                .justify_center()
                .child(titlebar_svg_icon(
                    icon_path,
                    TITLEBAR_POPUP_MENU_ROW_ICON_SIZE,
                    titlebar_icon_color(),
                )),
        )
        .child(
            v_flex()
                .min_w(px(0.0))
                .max_w_full()
                .flex_1()
                .overflow_hidden()
                .gap(px(1.0))
                .child(
                    div()
                        .min_w_0()
                        .max_w_full()
                        .w_full()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(TITLEBAR_POPUP_MENU_ROW_TEXT_SIZE))
                        .line_height(px(16.0))
                        .font_weight(FontWeight::NORMAL)
                        .child(label),
                )
                .child(
                    div()
                        .min_w_0()
                        .max_w_full()
                        .w_full()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .text_size(px(TITLEBAR_POPUP_ACTION_PREVIEW_TEXT_SIZE))
                        .line_height(px(14.0))
                        .text_color(titlebar_popup_menu_preview_text_color())
                        .when(preview_unconfigured, |this| this.italic())
                        .child(preview),
                ),
        )
}

pub(crate) fn titlebar_popup_git_section(
    mut menu: PopupMenu,
    label: impl Into<gpui::SharedString>,
) -> PopupMenu {
    let label = label.into();
    menu =
        menu.menu_element_with_disabled(Box::new(CopyGpuiTitlebarGitBranch), true, move |_, _| {
            titlebar_popup_git_section_label(label.clone())
        });
    menu
}

pub(crate) fn titlebar_popup_git_section_label(
    label: impl Into<gpui::SharedString>,
) -> impl IntoElement {
    h_flex()
        .w_full()
        .min_h(px(TITLEBAR_POPUP_GIT_SECTION_LABEL_HEIGHT))
        .items_center()
        .text_size(px(11.0))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(titlebar_popup_git_section_label_color())
        .child(label.into())
}

pub(crate) fn titlebar_popup_git_branch_menu_row(
    branch: String,
    disabled: bool,
) -> impl IntoElement {
    titlebar_popup_git_status_menu_row(
        TITLEBAR_ICON_GIT_COMMIT,
        "Branch".to_string(),
        titlebar_popup_git_value_text(branch, disabled),
        disabled,
    )
}

pub(crate) fn titlebar_popup_git_changes_menu_row(
    additions: u64,
    deletions: u64,
) -> impl IntoElement {
    titlebar_popup_git_status_menu_row(
        TITLEBAR_ICON_CODE,
        "Changes".to_string(),
        h_flex()
            .gap(px(6.0))
            .flex_shrink_0()
            .text_size(px(12.0))
            .child(
                div()
                    .text_color(titlebar_popup_git_additions_color())
                    .child(format!("+{additions}")),
            )
            .child(
                div()
                    .text_color(titlebar_popup_git_deletions_color())
                    .child(format!("\u{2212}{deletions}")),
            ),
        false,
    )
}

pub(crate) fn titlebar_popup_git_commits_menu_row(
    ahead_count: u64,
    behind_count: u64,
    disabled: bool,
) -> impl IntoElement {
    let has_commits_to_sync = ahead_count > 0 || behind_count > 0;
    titlebar_popup_git_status_menu_row(
        TITLEBAR_ICON_GIT_COMPARE,
        if has_commits_to_sync {
            "Sync upstream".to_string()
        } else {
            "No commits to sync".to_string()
        },
        titlebar_popup_git_value_text(
            if has_commits_to_sync {
                format!("\u{2191}{ahead_count} \u{2193}{behind_count}")
            } else {
                String::new()
            },
            disabled,
        ),
        disabled,
    )
}

pub(crate) fn titlebar_popup_git_status_menu_row(
    icon_path: &'static str,
    label: String,
    value: impl IntoElement + 'static,
    disabled: bool,
) -> impl IntoElement {
    let icon_color = if disabled {
        titlebar_popup_git_disabled_icon_color()
    } else {
        titlebar_icon_color()
    };

    h_flex()
        .min_w_0()
        .max_w_full()
        .flex_1()
        .overflow_hidden()
        .min_h(px(TITLEBAR_POPUP_MENU_ROW_HEIGHT))
        .items_center()
        .gap(px(10.0))
        .rounded(px(4.0))
        .text_size(px(TITLEBAR_POPUP_MENU_ROW_TEXT_SIZE))
        .font_weight(FontWeight::NORMAL)
        .text_color(titlebar_popup_menu_foreground())
        .child(
            div()
                .flex()
                .w(px(18.0))
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .child(titlebar_svg_icon(
                    icon_path,
                    TITLEBAR_POPUP_MENU_ROW_ICON_SIZE,
                    icon_color,
                )),
        )
        .child(
            h_flex()
                .min_w_0()
                .max_w_full()
                .flex_1()
                .overflow_hidden()
                .items_center()
                .justify_between()
                .gap(px(8.0))
                .child(
                    div()
                        .flex_shrink_0()
                        .overflow_hidden()
                        .whitespace_nowrap()
                        .text_ellipsis()
                        .child(label),
                )
                .child(value),
        )
}

pub(crate) fn titlebar_popup_git_value_text(value: String, disabled: bool) -> impl IntoElement {
    let color = if disabled {
        titlebar_popup_menu_disabled_text_color()
    } else {
        titlebar_inactive_text_color()
    };

    div()
        .min_w_0()
        .max_w(px(176.0))
        .overflow_hidden()
        .whitespace_nowrap()
        .text_ellipsis()
        .text_size(px(12.0))
        .text_color(color)
        .child(value)
}

pub(crate) fn titlebar_popup_git_action_menu_row(row: GpuiTitlebarGitMenuRow) -> impl IntoElement {
    let icon_path = titlebar_git_action_icon_path(row.action);
    let icon_color = if row.disabled {
        titlebar_popup_git_disabled_icon_color()
    } else {
        titlebar_icon_color()
    };

    h_flex()
        .min_w_0()
        .max_w_full()
        .flex_1()
        .overflow_hidden()
        .min_h(px(TITLEBAR_POPUP_MENU_ROW_HEIGHT))
        .items_center()
        .gap(px(10.0))
        .rounded(px(4.0))
        .text_size(px(TITLEBAR_POPUP_MENU_ROW_TEXT_SIZE))
        .font_weight(FontWeight::NORMAL)
        .text_color(titlebar_popup_menu_foreground())
        .child(
            div()
                .flex()
                .w(px(18.0))
                .flex_shrink_0()
                .items_center()
                .justify_center()
                .child(titlebar_svg_icon(
                    icon_path,
                    TITLEBAR_POPUP_MENU_ROW_ICON_SIZE,
                    icon_color,
                )),
        )
        .child(
            div()
                .min_w_0()
                .flex_1()
                .overflow_hidden()
                .whitespace_nowrap()
                .text_ellipsis()
                .child(row.label),
        )
}

pub(crate) fn titlebar_git_action_icon_path(action: GpuiTitlebarGitMenuActionId) -> &'static str {
    match action {
        GpuiTitlebarGitMenuActionId::Commit => TITLEBAR_ICON_GIT_COMMIT,
        GpuiTitlebarGitMenuActionId::Push => TITLEBAR_ICON_UPLOAD,
        GpuiTitlebarGitMenuActionId::Pr => TITLEBAR_ICON_GIT_PULL_REQUEST,
        GpuiTitlebarGitMenuActionId::SyncMain | GpuiTitlebarGitMenuActionId::SyncRemote => {
            TITLEBAR_ICON_GIT_COMPARE
        }
        GpuiTitlebarGitMenuActionId::MultiRelease => TITLEBAR_ICON_STACK_PUSH,
        GpuiTitlebarGitMenuActionId::Release => TITLEBAR_ICON_ROCKET,
    }
}

pub(crate) fn titlebar_action_icon_path(action: Option<&GpuiTitlebarAction>) -> &'static str {
    let Some(action) = action else {
        return TITLEBAR_ICON_SETTINGS;
    };
    titlebar_sidebar_command_icon_path(action.icon.as_deref().unwrap_or("playerPlay"))
}

pub(crate) fn titlebar_sidebar_command_icon_path(icon: &str) -> &'static str {
    match icon {
        "playerPlay" => TITLEBAR_ICON_PLAYER_PLAY,
        "api" => "titlebar/api.svg",
        "archive" => "titlebar/archive.svg",
        "bell" => "titlebar/bell.svg",
        "bolt" => "titlebar/bolt.svg",
        "book" => "titlebar/book.svg",
        "brain" => "titlebar/brain.svg",
        "braces" => "titlebar/braces.svg",
        "brandDocker" => "titlebar/brand-docker.svg",
        "brandGithub" => "titlebar/brand-github.svg",
        "brandPython" => "titlebar/brand-python.svg",
        "brandReact" => "titlebar/brand-react.svg",
        "brandVscode" => "titlebar/brand-vscode.svg",
        "bug" => "titlebar/bug.svg",
        "chartBar" => "titlebar/chart-bar.svg",
        "cloud" => "titlebar/cloud.svg",
        "checklist" => "titlebar/checklist.svg",
        "clock" => "titlebar/clock.svg",
        "code" => "titlebar/code.svg",
        "command" => "titlebar/command.svg",
        "cpu" => "titlebar/cpu.svg",
        "database" => "titlebar/database.svg",
        "deviceDesktop" => TITLEBAR_ICON_DEVICE_DESKTOP,
        "deviceLaptop" => "titlebar/device-laptop.svg",
        "download" => TITLEBAR_ICON_DOWNLOAD,
        "fileCode" => "titlebar/file-code.svg",
        "fileDiff" => "titlebar/file-diff.svg",
        "fileSearch" => "titlebar/file-search.svg",
        "fileText" => "titlebar/file-text.svg",
        "flask" => "titlebar/flask.svg",
        "folder" => "titlebar/folder.svg",
        "folderOpen" => TITLEBAR_ICON_FOLDER_OPEN,
        "gitBranch" => "titlebar/git-branch.svg",
        "gitCommit" => TITLEBAR_ICON_GIT_COMMIT,
        "gitMerge" => "titlebar/git-merge.svg",
        "gitPullRequest" => TITLEBAR_ICON_GIT_PULL_REQUEST,
        "key" => "titlebar/key.svg",
        "layoutDashboard" => "titlebar/layout-dashboard.svg",
        "link" => "titlebar/link.svg",
        "lock" => "titlebar/lock.svg",
        "messageCircle" => "titlebar/message-circle.svg",
        "package" => "titlebar/package.svg",
        "pencilCode" => "titlebar/pencil-code.svg",
        "refresh" => "titlebar/refresh.svg",
        "robot" => "titlebar/robot.svg",
        "route" => "titlebar/route.svg",
        "rocket" => TITLEBAR_ICON_ROCKET,
        "search" => BROWSER_ICON_SEARCH,
        "server" => "titlebar/server.svg",
        "settings" => TITLEBAR_ICON_SETTINGS,
        "shieldSearch" => "titlebar/shield-search.svg",
        "sparkles" => "titlebar/sparkles.svg",
        "stack" => "titlebar/stack.svg",
        "terminal" => "titlebar/terminal-2.svg",
        "testPipe" => "titlebar/test-pipe.svg",
        "tool" => "titlebar/tool.svg",
        "upload" => TITLEBAR_ICON_UPLOAD,
        "wand" => "titlebar/wand.svg",
        "world" => BROWSER_ICON_WORLD,
        _ => unreachable!("validated sidebar command icon id must be mapped"),
    }
}

pub(crate) fn titlebar_open_target_icon_for_id(target_id: &str) -> (&'static str, f32) {
    match target_id {
        "finder" => (TITLEBAR_ICON_FOLDER_OPEN, 16.0),
        "cursor" => ("titlebar/cursor.svg", 17.0),
        "vscode" | "vscode-insiders" => (TITLEBAR_ICON_VSCODE, 17.0),
        "vscodium" => ("titlebar/vscodium.svg", 17.0),
        "zed" => ("titlebar/zed.svg", 17.0),
        "antigravity" => ("titlebar/antigravity.svg", 17.0),
        "idea" => ("titlebar/intellijidea.svg", 17.0),
        "phpstorm" => ("titlebar/phpstorm.svg", 17.0),
        "pycharm" => ("titlebar/pycharm.svg", 17.0),
        "rider" => ("titlebar/rider.svg", 17.0),
        "rubymine" => ("titlebar/rubymine.svg", 17.0),
        "webstorm" => ("titlebar/webstorm.svg", 17.0),
        "aqua" | "clion" | "datagrip" | "dataspell" | "goland" | "rustrover" => {
            ("titlebar/jetbrains.svg", 17.0)
        }
        "trae" | "kiro" => (TITLEBAR_ICON_BOX, 16.0),
        _ => (TITLEBAR_ICON_BOX, 16.0),
    }
}
