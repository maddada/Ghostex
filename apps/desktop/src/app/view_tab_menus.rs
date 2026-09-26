//! The two menus the view panel's tab strip owns: `+`, which opens another view, and a tab's
//! right-click menu, which is where a view's scope lives now.

use gpui::Pixels;
use gpui::Window;

use crate::app::actions::*;
use crate::app::context_menu::GpuiContextMenu;
use crate::app::model::*;
use crate::app::project_views::ProjectViewCommand;
use crate::*;

impl GhostexGpuiApp {
    /// Every view this project could show, in the user's `titlebarViewOrder`, whether or not its
    /// scope currently allows it here. The `+` menu splits this into the rows it offers and the
    /// `Hidden here` submenu.
    pub(crate) fn view_picker_modes(&self) -> Vec<TitlebarModeSwitcherItem> {
        self.titlebar_mode_switcher_items_unscoped()
            .into_iter()
            .filter(|item| item.mode != TitlebarMode::Agents)
            .collect()
    }

    /// The views the user hid in this project, in one of its spaces, or everywhere. Ruling 2A: this
    /// is where they come back from, with no new hit target of their own.
    pub(crate) fn hidden_here_view_modes(&self) -> Vec<TitlebarMode> {
        self.view_picker_modes()
            .into_iter()
            .filter(|item| !self.titlebar_mode_view_scope_allows(item.mode))
            .map(|item| item.mode)
            .collect()
    }

    fn hidden_here_submenu_rows(&self) -> Vec<(gpui::SharedString, Box<dyn gpui::Action>)> {
        self.hidden_here_view_modes()
            .into_iter()
            .map(|mode| {
                let action: Box<dyn gpui::Action> = Box::new(ShowGpuiHiddenViewHere {
                    mode_index: mode.switcher_index(),
                });
                (gpui::SharedString::from(mode.tab_label()), action)
            })
            .collect()
    }

    /// CDXC:Workarea 2026-09-21 DECISION:
    /// User: the `+` menu marks nothing as open, no ticks. Its top row is Browser Tab (renamed from New Browser Tab on request), which
    /// always opens a new browser tab (the address bar has no `+` of its own any more). Every other
    /// view opens if it is not open yet and is switched to if it is. The menu still ends with
    /// `Hidden here ▸` (ruling 2A) and Customize (renamed from Manage views… on request). Supersedes the 2026-09-20 screen 04 rule that
    /// ticked the open views.
    pub(crate) fn show_view_tab_add_menu(
        &mut self,
        trigger_bounds: gpui::Bounds<Pixels>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let items = self.view_picker_modes();
        let mut menu = GpuiContextMenu::new();
        if let Some(browser) = items.iter().find(|item| {
            item.mode == TitlebarMode::Browser && self.titlebar_mode_view_scope_allows(item.mode)
        }) {
            menu = menu.menu_with_icon(
                "Browser Tab",
                TITLEBAR_ICON_WORLD,
                !browser.is_available,
                Box::new(OpenNewBrowserTabFromViewMenu),
            );
        }
        let mut previous_group: Option<u8> = None;
        // The picker's two groups, as the only thing a compact menu can show of them: a rule
        // between the built-ins and your views and extensions. Rows are walked by kind so a
        // built-in the saved view order has not seen yet still lists with the built-ins.
        let mut items = items
            .into_iter()
            .filter(|item| {
                item.mode != TitlebarMode::Browser
                    && self.titlebar_mode_view_scope_allows(item.mode)
            })
            .collect::<Vec<_>>();
        items.sort_by_key(|item| u8::from(item.mode.is_addon_view()));
        for item in items {
            let group = match item.mode {
                mode if mode.is_addon_view() => 1,
                _ => 0,
            };
            if previous_group.is_some_and(|previous| previous != group) {
                menu = menu.separator();
            }
            previous_group = Some(group);
            let action = Box::new(OpenGpuiViewTab {
                mode_index: item.mode.switcher_index(),
            });
            menu = menu.menu_with_icon(
                item.mode.tab_label(),
                item.mode.tab_icon(),
                !item.is_available,
                action,
            );
        }
        menu.separator()
            .submenu_with_icon(
                "Hidden here",
                Some(TITLEBAR_ICON_EYE_OFF),
                self.hidden_here_submenu_rows(),
            )
            .menu_with_icon(
                "Customize",
                TITLEBAR_ICON_SETTINGS,
                false,
                Box::new(OpenGpuiExtensionsModal),
            )
            .toggle_below(trigger_bounds, window, cx);
    }

    /// CDXC:Workarea 2026-09-25 DECISION:
    /// User (screen 05): right-clicking a view tab is where its scope lives. `Show in this Project`
    /// and `Show in this Space` are checkable and each writes exactly one override; per ruling 1A the
    /// space row is the project's OWN space and is absent when the project belongs to none.
    /// `Choose where it's shown…` opens the Settings editor for exactly this view, and the rest,
    /// Reload, Sleep, Open externally and Close, act on the tab that was clicked rather than the
    /// active one. Supersedes 2026-09-20: the user moved the scope rows into their own section below
    /// the tab actions and dropped the project and space names from their labels.
    pub(crate) fn show_view_tab_context_menu(
        &mut self,
        mode: TitlebarMode,
        position: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let mode_index = mode.switcher_index();
        let mut menu = GpuiContextMenu::new();
        if let TitlebarMode::Extension(id) = mode
            && mode
                .website_provider()
                .is_some_and(|provider| !provider.automatic())
        {
            menu = menu
                .menu(
                    format!("Modify home URL for {}…", self.project_name),
                    Box::new(ProjectViewCommand {
                        id: id.as_str().into(),
                        operation: "home".into(),
                    }),
                )
                .separator();
        }
        let unavailable = !self.titlebar_mode_available(mode);
        menu = menu.menu(
            if self.view_strip_tab_pinned(ViewStripTabKey::View(mode)) {
                "Unpin tab"
            } else {
                "Pin tab"
            },
            Box::new(ToggleGpuiViewStripTabPinned {
                mode_index,
                browser_tab_id: None,
            }),
        );
        menu = menu.menu_with_disabled(
            if mode.is_storybook() {
                "Rebuild Storybook"
            } else {
                "Reload"
            },
            unavailable,
            Box::new(ReloadGpuiTitlebarView { mode_index }),
        );
        menu = if self.project_editor_shell.is_mode_awake(mode) {
            menu.menu_with_disabled(
                "Sleep",
                unavailable,
                Box::new(SleepGpuiTitlebarView { mode_index }),
            )
        } else {
            menu.menu_with_disabled(
                "Wake",
                unavailable,
                Box::new(OpenGpuiViewTab { mode_index }),
            )
        };
        /*
        CDXC:Extensions 2026-09-16 DECISION:
        User: keep Start / Restart and Stop removed, but restore Configure view and make it open the
        clicked view's editor. The rows moved from the mode tab's right-click menu to the view tab's
        when the tab strip replaced the mode switcher; nothing about them changed.
        */
        if let TitlebarMode::Extension(id) = mode
            && mode.website_provider().is_none()
            && gpui_custom_view(id).is_some_and(|view| view.definition.get("source").is_some())
        {
            menu = menu
                .menu(
                    "Command output",
                    Box::new(ProjectViewCommand {
                        id: id.as_str().into(),
                        operation: "output".into(),
                    }),
                )
                .menu(
                    "Configure view",
                    Box::new(ProjectViewCommand {
                        id: id.as_str().into(),
                        operation: "configure".into(),
                    }),
                );
        }
        menu = menu.menu_with_disabled(
            "Open externally",
            self.view_pop_out_url(mode).is_none(),
            Box::new(PopOutGpuiViewTab { mode_index }),
        );
        if let Some(scope_key) = self.titlebar_mode_view_scope_key(mode) {
            let shown =
                self.view_scope_state_for_active_project(&scope_key) == ViewScopeState::Shown;
            menu = menu.separator().menu_with_check(
                "Show in this Project",
                shown,
                Box::new(ToggleGpuiViewProjectScope { mode_index }),
            );
            for (space_key, _) in self.active_project_space_labels() {
                let space_shown =
                    self.view_scope_space_state(&scope_key, &space_key) != Some(false);
                menu = menu.menu_with_check(
                    "Show in this Space",
                    space_shown,
                    Box::new(ToggleGpuiViewSpaceScope {
                        mode_index,
                        space_key,
                    }),
                );
            }
            menu = menu.menu(
                "Choose where it's shown…",
                Box::new(OpenGpuiViewScopeSettings { mode_index }),
            );
        }
        menu.separator()
            .submenu("Hidden here", self.hidden_here_submenu_rows())
            .separator()
            .menu("Close tab", Box::new(CloseGpuiViewTab { mode_index }))
            .show(position, window, cx);
    }

    /// A view's stored state for one space, with no precedence applied: the space row ticks what the
    /// space itself says, so unticking it writes `hidden` there rather than fighting the project
    /// override above it.
    pub(crate) fn view_scope_space_state(&self, key: &str, space_key: &str) -> Option<bool> {
        shared_settings::shared_sidebar_settings_snapshot()
            .object()
            .get("viewScopes")
            .and_then(|scopes| scopes.get(key))
            .and_then(|scope| scope.get("spaces"))
            .and_then(|spaces| spaces.get(space_key))
            .and_then(serde_json::Value::as_str)
            .map(|state| state == "shown")
    }

    /// The view a tab menu row names, including a view whose scope currently hides it, which is what
    /// the `Hidden here` rows have to be able to reach.
    pub(crate) fn view_tab_mode_for_index(&self, index: u64) -> Option<TitlebarMode> {
        self.titlebar_mode_switcher_items_unscoped()
            .into_iter()
            .find(|item| item.mode.switcher_index() == index)
            .map(|item| item.mode)
    }

    pub(crate) fn toggle_view_project_scope(&mut self, index: u64, cx: &mut gpui::Context<Self>) {
        let Some(mode) = self.view_tab_mode_for_index(index) else {
            return;
        };
        let Some(scope_key) = self.titlebar_mode_view_scope_key(mode) else {
            return;
        };
        let Some(project_id) = self.active_project_id_for_view_scope() else {
            return;
        };
        let shown = self.view_scope_state_for_active_project(&scope_key) == ViewScopeState::Shown;
        self.set_view_scope_override(
            &scope_key,
            ViewScopeOverrideTarget::Project,
            &project_id,
            Some(if shown {
                ViewScopeState::Hidden
            } else {
                ViewScopeState::Shown
            }),
            cx,
        );
    }

    pub(crate) fn toggle_view_space_scope(
        &mut self,
        index: u64,
        space_key: &str,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(mode) = self.view_tab_mode_for_index(index) else {
            return;
        };
        let Some(scope_key) = self.titlebar_mode_view_scope_key(mode) else {
            return;
        };
        let shown = self.view_scope_space_state(&scope_key, space_key) != Some(false);
        self.set_view_scope_override(
            &scope_key,
            ViewScopeOverrideTarget::Space,
            space_key,
            Some(if shown {
                ViewScopeState::Hidden
            } else {
                ViewScopeState::Shown
            }),
            cx,
        );
    }

    /// `Hidden here ▸ <view>`: show it here again. The project override says `shown` outright rather
    /// than clearing whatever hid it, because the thing hiding it may be the default or a space, and
    /// the user asked for it in THIS project.
    pub(crate) fn show_hidden_view_here(
        &mut self,
        index: u64,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(mode) = self.view_tab_mode_for_index(index) else {
            return;
        };
        let Some(scope_key) = self.titlebar_mode_view_scope_key(mode) else {
            return;
        };
        let Some(project_id) = self.active_project_id_for_view_scope() else {
            return;
        };
        self.set_view_scope_override(
            &scope_key,
            ViewScopeOverrideTarget::Project,
            &project_id,
            Some(ViewScopeState::Shown),
            cx,
        );
        self.open_view_tab(mode, window, cx);
    }

    /// CDXC:Workarea 2026-09-20 WHY:
    /// "Choose where it's shown…" is a deep link, not just "open Settings": the Extensions page picks
    /// its scope editor from `initialViewScopeKey`, so the user lands on the view they right-clicked
    /// instead of hunting for its row.
    pub(crate) fn open_view_scope_settings(
        &mut self,
        index: u64,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(mode) = self.view_tab_mode_for_index(index) else {
            return;
        };
        let Some(scope_key) = self.titlebar_mode_view_scope_key(mode) else {
            return;
        };
        let modal = GpuiAppModalKind::Settings;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = modal.open_message();
        open_message["initialTab"] = serde_json::Value::String("extensions".to_string());
        open_message["initialViewScopeKey"] = serde_json::Value::String(scope_key);
        self.open_gpui_app_modal_window(
            modal,
            open_message,
            sidebar_state_message,
            Some(window),
            cx,
        );
    }
}
