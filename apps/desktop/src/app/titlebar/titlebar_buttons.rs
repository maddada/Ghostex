// C1 wave-4 deferred split: apps/desktop/src/app/titlebar.rs (~3.9k lines)
// further divided into responsibility-scoped submodules, pure move (the
// only edit from the original app/titlebar.rs body is wrapping each group
// of `impl GhostexGpuiApp` methods in its own impl block; multiple impl
// blocks for the same type across files is the established pattern used by
// every sibling file in apps/desktop/src/app/). This file holds the actions and open-targets titlebar button renderers plus their popup content-height helpers.
// See docs/2026-08-22/repo-restructure/SPLITS.md C1.

// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: titlebar menus, popups, actions, and titlebar render_* builders

use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::window::*;
use crate::*;

impl GhostexGpuiApp {
    pub(crate) fn titlebar_open_targets_popup_content_height(&self) -> f32 {
        let target_count = gpui_visible_open_targets_from_current_settings().len();
        let mut rows = vec![TITLEBAR_POPUP_MENU_ROW_HEIGHT; target_count];
        if target_count > 0 {
            rows.push(TITLEBAR_POPUP_MENU_SEPARATOR_HEIGHT);
        }
        rows.push(TITLEBAR_POPUP_MENU_ROW_HEIGHT);
        titlebar_popup_menu_height_for_rows(&rows)
    }

    pub(crate) fn titlebar_actions_popup_content_height(&self) -> f32 {
        let action_count = self.visible_gpui_titlebar_actions().len();
        let mut rows = if action_count == 0 {
            vec![TITLEBAR_POPUP_MENU_ROW_HEIGHT]
        } else {
            vec![TITLEBAR_POPUP_ACTION_ROW_HEIGHT; action_count]
        };
        rows.push(TITLEBAR_POPUP_MENU_SEPARATOR_HEIGHT);
        rows.push(TITLEBAR_POPUP_MENU_ROW_HEIGHT);
        titlebar_popup_menu_height_for_rows(&rows)
    }

    pub(crate) fn titlebar_git_popup_content_height(&self) -> f32 {
        let Some(state) = self
            .titlebar_git_menu_state
            .as_ref()
            .filter(|state| !state.is_busy && state.is_repo)
        else {
            return titlebar_popup_menu_height_for_rows(&[TITLEBAR_POPUP_MENU_ROW_HEIGHT]);
        };
        let section_label_height =
            TITLEBAR_POPUP_GIT_SECTION_LABEL_HEIGHT.max(TITLEBAR_POPUP_MENU_MIN_ITEM_HEIGHT);
        let mut rows = vec![
            section_label_height,
            TITLEBAR_POPUP_MENU_ROW_HEIGHT,
            TITLEBAR_POPUP_MENU_ROW_HEIGHT,
            TITLEBAR_POPUP_MENU_ROW_HEIGHT,
            TITLEBAR_POPUP_MENU_SEPARATOR_HEIGHT,
            section_label_height,
        ];
        rows.extend(std::iter::repeat_n(
            TITLEBAR_POPUP_MENU_ROW_HEIGHT,
            state.rows.len(),
        ));
        titlebar_popup_menu_height_for_rows(&rows)
    }

    pub(crate) fn titlebar_popup_content_height(&self, kind: GpuiTitlebarPopupKind) -> f32 {
        match kind {
            GpuiTitlebarPopupKind::AccountUsage(id) => self
                .titlebar_accounts
                .iter()
                .find(|account| account["titlebarKey"] == id.as_str())
                .map_or(0.0, crate::app::window::account_usage::estimated_height),
            GpuiTitlebarPopupKind::ContextMenu => self
                .context_menu
                .as_ref()
                .map_or(0.0, |menu| menu.content_height()),
            GpuiTitlebarPopupKind::Actions => self.titlebar_actions_popup_content_height(),
            GpuiTitlebarPopupKind::BrowserActions(_) => self.browser_actions_popup_content_height(),
            GpuiTitlebarPopupKind::BrowserDeviceView(_) => {
                self.browser_device_view_popup_content_height()
            }
            GpuiTitlebarPopupKind::Extensions => {
                // The popup lists exactly the extensions scoped to the active project, so its measured
                // height must count the same rows `build_gpui_titlebar_extensions_popup_menu` renders.
                let extension_count = self
                    .extensions_snapshot
                    .installed
                    .values()
                    .filter(|extension| {
                        extension.enabled
                            && self.view_scope_allows(&extension_view_scope_key(&extension.id))
                    })
                    .count();
                let mut rows = if extension_count == 0 {
                    vec![TITLEBAR_POPUP_MENU_ROW_HEIGHT]
                } else {
                    vec![TITLEBAR_POPUP_EXTENSION_ROW_HEIGHT; extension_count]
                };
                rows.push(TITLEBAR_POPUP_MENU_SEPARATOR_HEIGHT);
                rows.push(TITLEBAR_POPUP_MENU_ROW_HEIGHT);
                titlebar_popup_menu_height_for_rows(&rows)
            }
            GpuiTitlebarPopupKind::Git => self.titlebar_git_popup_content_height(),
            GpuiTitlebarPopupKind::Help => super::help_menu::titlebar_help_popup_content_height(),
            GpuiTitlebarPopupKind::More => self.titlebar_more_popup_content_height(),
            GpuiTitlebarPopupKind::OpenTargets => self.titlebar_open_targets_popup_content_height(),
            GpuiTitlebarPopupKind::Notifications => TITLEBAR_POPUP_NOTIFICATIONS_MAX_HEIGHT,
            GpuiTitlebarPopupKind::SessionLinks => {
                let links = self
                    .native_sidebar
                    .links_list_session
                    .as_ref()
                    .and_then(|key| self.session_links_of(key));
                let saved = crate::app::native_sidebar::links_list::folded_session_link_groups();
                crate::app::native_sidebar::links_list::session_links_list_height(
                    links.as_ref(),
                    &links
                        .as_ref()
                        .map_or(saved.clone(), |links| links.folded_groups_on_open(&saved)),
                )
            }
            GpuiTitlebarPopupKind::Resources
            | GpuiTitlebarPopupKind::Tips
            | GpuiTitlebarPopupKind::RemoteSites => TITLEBAR_POPUP_READING_MENU_MAX_HEIGHT,
        }
    }
}
