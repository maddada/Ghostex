// C1 wave-4 extraction: `impl GhostexGpuiApp` methods moved verbatim out of
// main.rs (pure move; the only edit is the `pub(crate) ` visibility prefix the
// cross-module split requires). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
//
// Cluster: titlebar menus, popups, actions, and titlebar render_* builders

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Duration;

// RefCell backs cross-platform runtime state (window frame persistence), not
// just the macOS-only shims that first introduced the import.

use gpui::Action;
use gpui::Anchor;
use gpui::Animation;
use gpui::AnimationExt as _;
use gpui::AnyElement;
use gpui::AppContext as _;
use gpui::Bounds;
use gpui::FontWeight;
use gpui::Hsla;
use gpui::InteractiveElement as _;
use gpui::IntoElement;
use gpui::KeyDownEvent;
use gpui::MouseButton;
use gpui::MouseDownEvent;
use gpui::MouseUpEvent;
use gpui::ParentElement as _;
use gpui::Pixels;
use gpui::Point;
use gpui::Styled as _;
use gpui::Window;
use gpui::WindowBackgroundAppearance;
use gpui::WindowBounds;
use gpui::WindowKind;
use gpui::WindowOptions;
use gpui::anchored;
use gpui::canvas;
use gpui::deferred;
use gpui::div;
use gpui::point;
use gpui::prelude::FluentBuilder as _;
use gpui::px;
use gpui::rgb;
use gpui::svg;
use gpui_component::ElementExt;
use gpui_component::Selectable;
use gpui_component::Side;
use gpui_component::Sizable as _;
use gpui_component::Size as ComponentSize;
use gpui_component::WindowExt;
use gpui_component::h_flex;
use gpui_component::input::Input;
use gpui_component::menu::PopupMenu;
use gpui_component::native_menu::NativeMenu;
use gpui_component::notification::Notification;
use gpui_component::scroll::ScrollbarShow;
use gpui_component::tooltip::ManagedTooltipExt as _;
use gpui_component::tooltip::ManagedTooltipPlacement;
use gpui_component::v_flex;

use crate::app::actions::*;
use crate::app::consts::*;
use crate::app::helpers::*;
use crate::app::model::*;
use crate::app::window::*;
use crate::*;

#[cfg(any(target_os = "windows", target_os = "linux"))]
use gpui::WindowControlArea;
impl GhostexGpuiApp {
    /// The titlebar Git control opens an in-app PopupMenu projected from the
    /// sidebar runtime's shared Git menu builders. Selections keep dispatching
    /// fixed action selectors only.
    pub(crate) fn show_gpui_titlebar_git_menu(
        &mut self,
        trigger_bounds: Option<Bounds<Pixels>>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let open = !self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::Git);
        if open {
            // Ask the runtime for a background state refresh so the menu
            // converges on fresh rows; the open menu renders the last projected
            // state honestly instead of fabricating fresh values.
            self.dispatch_gpui_titlebar_git_action_selector(
                GPUI_TITLEBAR_GIT_ACTION_REFRESH_SELECTOR,
                cx,
            );
        }
        self.set_gpui_titlebar_popup_open(
            GpuiTitlebarPopupKind::Git,
            open,
            trigger_bounds,
            window,
            cx,
        );
    }

    pub(crate) fn run_gpui_titlebar_git_menu_row(&mut self, row_index: usize, cx: &mut gpui::Context<Self>) {
        let Some(row) = self
            .titlebar_git_menu_state
            .as_ref()
            .and_then(|state| state.rows.get(row_index))
        else {
            return;
        };
        if row.disabled {
            return;
        }
        let action = row.action;
        self.dispatch_gpui_titlebar_git_action_selector(action.selector(), cx);
    }

    pub(crate) fn dispatch_gpui_titlebar_git_action_selector(
        &mut self,
        selector: &str,
        cx: &mut gpui::Context<Self>,
    ) -> bool {
        let Some(sidebar) = self.sidebar.clone() else {
            return false;
        };
        let message = serde_json::json!({
            "action": selector,
            "type": GPUI_SIDEBAR_TITLEBAR_GIT_ACTION_MESSAGE_TYPE,
            "version": GPUI_SIDEBAR_TITLEBAR_GIT_ACTION_MESSAGE_VERSION,
        });
        let script = gpui_titlebar_git_action_script(&message);
        sidebar.update(cx, |surface, _| surface.execute_app_owned_script(&script))
    }

    pub(crate) fn show_titlebar_settings_menu(
        &self,
        position: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUITitlebarAppModalHost 2026-06-24-11:09:
        The GPUI titlebar Settings glyph opens this NativeMenu so Settings, Hotkeys, and Command Palette all have typed titlebar actions into the shared React app-modal host. Keep this menu OS-owned and action-backed, with no visual-only dropdown, fake control, WebKit surface, overlay, hidden hit region, or generic fallback behavior.

        CDXC:GPUIPreviousSessionsModal 2026-06-24-11:53:
        Previous Sessions is exposed from the same titlebar NativeMenu so GPUI opens the shared history/restore modal through a typed app-modal action rather than duplicating the React UI or adding an overlay surface.

        CDXC:GPUIDaemonSessionsModal 2026-06-24-12:00:
        Running Sessions belongs in this same OS-owned Settings utility menu because the shared React daemonSessions modal already owns the production UI and GPUI only supplies the modal route plus real gxserver-backed state.

        CDXC:GxserverAppUserData 2026-06-24-13:30:
        Pinned Prompts and Scratch Pad are also shared React app modals. GPUI exposes them through typed NativeMenu actions and hydrates from gxserver app-user-data so the titlebar route does not duplicate UI or fake persistence.

        CDXC:GPUIAgentsHubModal 2026-06-24-12:26:
        Agents Hub belongs in the same typed GPUI app-modal route as the Settings utility surfaces. The menu action must open the shared React Hub in the owned CEF app-modal host while Rust supplies the real filesystem catalog/content bridge instead of duplicate modal UI or fallback rows.

        CDXC:GPUISettingsEntryModals 2026-06-24-12:22:
        Configure Agents, Configure Actions, and Open Targets belong in the same typed NativeMenu because macOS/React already treat them as Settings-modal entry points. Keep the menu action-backed so GPUI opens the shared Settings host with the requested initial tab instead of introducing a second modal surface.
        */
        NativeMenu::new()
            .menu("Settings", Box::new(OpenGpuiSettingsModal))
            .menu("Plugins", Box::new(OpenGpuiPluginsModal))
            .menu("Hotkeys", Box::new(OpenGpuiHotkeysModal))
            .menu("Quick Access", Box::new(OpenGpuiCommandPaletteModal))
            .menu("Configure Agents", Box::new(OpenGpuiConfigureAgentsModal))
            .menu("Configure Actions", Box::new(OpenGpuiConfigureActionsModal))
            .menu("Open Targets", Box::new(OpenGpuiOpenTargetsModal))
            .menu("Previous Sessions", Box::new(OpenGpuiPreviousSessionsModal))
            .menu("Running Sessions", Box::new(OpenGpuiDaemonSessionsModal))
            .menu("Pinned Prompts", Box::new(OpenGpuiPinnedPromptsModal))
            .menu("Scratch Pad", Box::new(OpenGpuiScratchPadModal))
            .menu("Agents Hub", Box::new(OpenGpuiAgentsHubModal))
            .show(position, window, cx);
    }

    pub(crate) fn show_gpui_titlebar_mode_menu(
        &self,
        position: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUITitlebarCompactMode 2026-07-04-01:00:
        The compact titlebar mode picker is an OS-owned NativeMenu for narrow windows. Rows are projected from the same titlebar_mode_switcher_items list as the center tabs, disabled states stay disabled in Quick/projectless contexts, and selections dispatch through set_active_mode rather than mutating active_mode directly.
        */
        let mut menu = NativeMenu::new();
        for item in self.titlebar_mode_switcher_items() {
            let action = Box::new(SelectGpuiTitlebarMode {
                mode_index: item.mode.switcher_index(),
            });
            if item.is_available {
                menu = menu.menu_with_check(
                    item.mode.display_label(),
                    self.active_mode == item.mode,
                    action,
                );
            } else {
                menu = menu.menu_with_disabled(item.mode.display_label(), true, action);
            }
        }
        menu.show(position, window, cx);
    }

    pub(crate) fn show_gpui_titlebar_customize_menu(
        &self,
        position: gpui::Point<Pixels>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUITitlebarCustomize 2026-08-11:
        Right-clicking blank titlebar chrome or a workarea mode button should
        expose the page that owns titlebar visibility. Keep this as an OS-owned
        NativeMenu action into the existing Settings > Customize route; normal
        titlebar layout and hit testing remain unchanged.
        */
        NativeMenu::new()
            .menu("Customize", Box::new(OpenGpuiCustomizeSettingsModal))
            .show(position, window, cx);
    }

    pub(crate) fn select_titlebar_mode_from_menu(
        &mut self,
        mode_index: u64,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(mode) = TitlebarMode::from_switcher_index(mode_index) else {
            return;
        };
        if self.set_active_mode(mode, window, cx) {
            cx.notify();
        }
    }

    pub(crate) fn titlebar_popup_menu_open(&self, kind: GpuiTitlebarPopupKind) -> bool {
        self.titlebar_popup_menu
            .as_ref()
            .is_some_and(|state| state.kind == kind)
    }

    pub(crate) fn show_gpui_open_targets_menu(
        &mut self,
        trigger_bounds: Option<Bounds<Pixels>>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUITitlebarOpenIn 2026-06-24-12:50:
        The visible GPUI titlebar folder control mirrors macOS Open In behavior with an in-app gpui-component PopupMenu. Menu rows remain typed GPUI actions, Configure routes to the shared Open Targets Settings tab, and the control must not add React overlays, WebKit dropdowns, invisible hit regions, hit-test overrides, or synthetic coordinate routing.
        */
        self.set_gpui_titlebar_popup_open(
            GpuiTitlebarPopupKind::OpenTargets,
            !self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::OpenTargets),
            trigger_bounds,
            window,
            cx,
        );
    }

    pub(crate) fn show_gpui_titlebar_actions_menu(
        &mut self,
        trigger_bounds: Option<Bounds<Pixels>>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUITitlebarActions 2026-06-24-14:24:
        Right-clicking the visible GPUI titlebar Actions play control opens an in-app gpui-component PopupMenu of sidebar actions plus Configure. Rows dispatch typed GPUI actions by visible index into the current projected action list, unconfigured rows route through the existing Settings > Actions path, and the control must not add React overlays, WebKit dropdowns, invisible hit regions, hit-test overrides, or synthetic coordinate routing.
        */
        self.set_gpui_titlebar_popup_open(
            GpuiTitlebarPopupKind::Actions,
            !self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::Actions),
            trigger_bounds,
            window,
            cx,
        );
    }

    pub(crate) fn set_gpui_titlebar_popup_open(
        &mut self,
        kind: GpuiTitlebarPopupKind,
        open: bool,
        trigger_bounds: Option<Bounds<Pixels>>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        log_gpui_titlebar_popup_repro(
            "gpui.titlebarPopup.setOpenRequested",
            serde_json::json!({
                "currentKind": self
                    .titlebar_popup_menu
                    .as_ref()
                    .map(|state| state.kind.diagnostic_label()),
                "hasPopupWindowHandle": self.titlebar_popup_window.is_some(),
                "kind": kind.diagnostic_label(),
                "mainWindowActive": window.is_window_active(),
                "open": open,
                "triggerBounds": gpui_titlebar_popup_bounds_diagnostic(trigger_bounds),
            }),
        );
        if !open {
            self.close_gpui_titlebar_popup(Some(kind), window, cx);
            return;
        }
        if self.titlebar_popup_menu_open(kind) {
            return;
        }

        self.close_gpui_titlebar_popup(None, window, cx);
        let Some(trigger_bounds) = trigger_bounds else {
            log_gpui_titlebar_popup_repro(
                "gpui.titlebarPopup.anchorMissing",
                serde_json::json!({
                    "kind": kind.diagnostic_label(),
                    "mainWindowActive": window.is_window_active(),
                }),
            );
            window.request_animation_frame();
            return;
        };

        let main_app = cx.entity().downgrade();
        let content = self.build_gpui_titlebar_popup_content(kind, main_app.clone(), window, cx);
        let popup_bounds = titlebar_popup_window_bounds_for_trigger_bounds(
            kind,
            trigger_bounds,
            self.titlebar_popup_content_height(kind),
            window,
        );
        let display_id = window.display(cx).map(|display| display.id());
        /*
        The dropdown is an exact, owned popup window that stays on the trigger
        window's display. macOS and Windows keep it non-activating so opening a
        dropdown does not deactivate the main window and trigger its outside-app
        dismissal observer. The popup root focuses PopupMenu internally, so its
        rows still dispatch typed actions without native window activation.
        Linux retains native focus for its existing popup input path.
        */
        let options = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(popup_bounds)),
            display_id,
            focus: !cfg!(any(target_os = "macos", target_os = "windows")),
            show: true,
            kind: WindowKind::PopUp,
            is_movable: false,
            is_resizable: false,
            is_minimizable: false,
            titlebar: None,
            window_background: WindowBackgroundAppearance::Transparent,
            ..Default::default()
        };
        log_gpui_titlebar_popup_repro(
            "gpui.titlebarPopup.openWindowAttempt",
            serde_json::json!({
                "focusRequested": !cfg!(any(target_os = "macos", target_os = "windows")),
                "kind": kind.diagnostic_label(),
                "mainWindowActive": window.is_window_active(),
                "popupBounds": gpui_titlebar_popup_bounds_diagnostic(Some(popup_bounds)),
                "triggerBounds": gpui_titlebar_popup_bounds_diagnostic(Some(trigger_bounds)),
            }),
        );
        let popup_window = match cx.open_window(options, {
            let content = content.clone();
            move |popup_window, cx| {
                prepare_gpui_titlebar_popup_window_chrome(popup_window);
                GpuiTitlebarPopupWindow::new(main_app, kind, content, popup_window, cx)
            }
        }) {
            Ok(popup_window) => popup_window,
            Err(error) => {
                log_gpui_titlebar_popup_repro(
                    "gpui.titlebarPopup.openWindowError",
                    serde_json::json!({
                        "error": error.to_string(),
                        "kind": kind.diagnostic_label(),
                        "mainWindowActive": window.is_window_active(),
                    }),
                );
                return;
            }
        };
        self.titlebar_popup_menu = Some(GpuiTitlebarPopupState {
            kind,
            trigger_bounds,
        });
        self.titlebar_popup_window = Some(popup_window);
        log_gpui_titlebar_popup_repro(
            "gpui.titlebarPopup.openWindowSucceeded",
            serde_json::json!({
                "kind": kind.diagnostic_label(),
                "mainWindowActive": window.is_window_active(),
            }),
        );
        if kind == GpuiTitlebarPopupKind::Tips {
            self.request_gpui_titlebar_tips_runtime_status(cx);
        }
        cx.notify();
    }

    pub(crate) fn close_gpui_titlebar_popup(
        &mut self,
        kind: Option<GpuiTitlebarPopupKind>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let current_kind = self
            .titlebar_popup_menu
            .as_ref()
            .map(|state| state.kind.diagnostic_label());
        let should_close = self
            .titlebar_popup_menu
            .as_ref()
            .is_some_and(|state| kind.is_none_or(|kind| state.kind == kind));
        log_gpui_titlebar_popup_repro(
            "gpui.titlebarPopup.closeRequested",
            serde_json::json!({
                "currentKind": current_kind,
                "hasPopupWindowHandle": self.titlebar_popup_window.is_some(),
                "mainWindowActive": window.is_window_active(),
                "requestedKind": kind.map(GpuiTitlebarPopupKind::diagnostic_label),
                "willClose": should_close,
            }),
        );
        if !should_close {
            return;
        }

        self.titlebar_popup_menu = None;
        if let Some(popup_window) = self.titlebar_popup_window.take() {
            let _ = popup_window.update(cx, |_, popup_window, _| {
                popup_window.remove_window();
            });
        }
        if self
            .titlebar_dropdown_focus_handle
            .contains_focused(window, cx)
            && let Some(previous_focus_handle) = self.titlebar_dropdown_previous_focus_handle.take()
        {
            previous_focus_handle.focus(window, cx);
        } else {
            self.titlebar_dropdown_previous_focus_handle = None;
        }
        cx.notify();
    }

    pub(crate) fn clear_gpui_titlebar_popup_from_window(
        &mut self,
        kind: GpuiTitlebarPopupKind,
        cx: &mut gpui::Context<Self>,
    ) {
        let should_clear = self
            .titlebar_popup_menu
            .as_ref()
            .is_some_and(|state| state.kind == kind);
        log_gpui_titlebar_popup_repro(
            "gpui.titlebarPopup.windowClearedState",
            serde_json::json!({
                "currentKind": self
                    .titlebar_popup_menu
                    .as_ref()
                    .map(|state| state.kind.diagnostic_label()),
                "kind": kind.diagnostic_label(),
                "willClear": should_clear,
            }),
        );
        if should_clear {
            self.titlebar_popup_menu = None;
            self.titlebar_popup_window = None;
            self.titlebar_dropdown_previous_focus_handle = None;
            cx.notify();
        }
    }

    pub(crate) fn build_gpui_titlebar_popup_content(
        &self,
        kind: GpuiTitlebarPopupKind,
        main_app: gpui::WeakEntity<GhostexGpuiApp>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> GpuiTitlebarPopupContent {
        match kind {
            GpuiTitlebarPopupKind::Actions => {
                GpuiTitlebarPopupContent::Menu(PopupMenu::build(window, cx, |menu, _, _| {
                    self.build_gpui_titlebar_actions_popup_menu(menu)
                }))
            }
            GpuiTitlebarPopupKind::Git => {
                GpuiTitlebarPopupContent::Menu(PopupMenu::build(window, cx, |menu, _, _| {
                    self.build_gpui_titlebar_git_popup_menu(menu)
                }))
            }
            GpuiTitlebarPopupKind::OpenTargets => {
                GpuiTitlebarPopupContent::Menu(PopupMenu::build(window, cx, |menu, _, _| {
                    self.build_gpui_open_targets_popup_menu(menu)
                }))
            }
            GpuiTitlebarPopupKind::Resources => {
                let snapshot = self.gpui_native_resources_snapshot(cx);
                GpuiTitlebarPopupContent::Reading(
                    cx.new(|_| GpuiTitlebarReadingPanel::resources(main_app, snapshot)),
                )
            }
            GpuiTitlebarPopupKind::Tips => {
                let live_agent_ids = self
                    .agents_workspace
                    .terminal_sessions
                    .iter()
                    .filter(|session| session.presentation_state.is_running())
                    .filter_map(|session| session.agent_icon)
                    .filter_map(gpui_default_sidebar_agent_by_icon)
                    .map(|agent| agent.agent_id.to_string())
                    .collect();
                GpuiTitlebarPopupContent::Reading(cx.new(|_| {
                    GpuiTitlebarReadingPanel::tips(
                        main_app,
                        self.titlebar_tips_cli_status.clone(),
                        self.titlebar_tips_agent_hook_status.clone(),
                        live_agent_ids,
                    )
                }))
            }
        }
    }

    pub(crate) fn build_gpui_open_targets_popup_menu(&self, menu: PopupMenu) -> PopupMenu {
        let targets = gpui_visible_open_targets_from_current_settings();
        let active_target_index = self.active_open_target_index(&targets);
        let mut menu = menu
            .min_w(px(TITLEBAR_POPUP_COMPACT_WIDTH))
            .max_w(px(TITLEBAR_POPUP_COMPACT_WIDTH))
            .max_h(px(TITLEBAR_POPUP_MENU_INNER_MAX_HEIGHT))
            .items_padding_bottom(px(0.0))
            .scrollable(true)
            .scrollbar_thickness(px(TITLEBAR_DROPDOWN_SCROLLBAR_WIDTH))
            .scrollbar_show(ScrollbarShow::Scrolling)
            .check_side(Side::Right);
        for (target_index, target) in targets.iter().enumerate() {
            let label = target.label.clone();
            let (icon_path, icon_size) = titlebar_open_target_icon_for_id(&target.id);
            menu = menu.menu_element_with_check(
                Some(target_index) == active_target_index,
                Box::new(OpenGpuiWorkspaceInTarget {
                    target_index: target_index as u64,
                }),
                move |_, _| {
                    titlebar_popup_standard_menu_row(icon_path, icon_size, label.clone(), false)
                },
            );
        }
        if !targets.is_empty() {
            menu = menu.separator();
        }
        menu.menu_element(Box::new(OpenGpuiOpenTargetsModal), move |_, _| {
            titlebar_popup_standard_menu_row(
                TITLEBAR_ICON_SETTINGS,
                TITLEBAR_POPUP_MENU_ROW_ICON_SIZE,
                "Configure".to_string(),
                false,
            )
        })
    }

    pub(crate) fn build_gpui_titlebar_actions_popup_menu(&self, menu: PopupMenu) -> PopupMenu {
        let actions = self.visible_gpui_titlebar_actions();
        let active_command_id = self
            .active_action_command_id
            .as_deref()
            .and_then(|active_id| {
                actions
                    .iter()
                    .find(|action| action.command_id == active_id && action.is_configured())
            })
            .or_else(|| actions.iter().find(|action| action.is_configured()))
            .map(|action| action.command_id.clone());
        let mut menu = menu
            .min_w(px(TITLEBAR_POPUP_COMPACT_WIDTH))
            .max_w(px(TITLEBAR_POPUP_COMPACT_WIDTH))
            .max_h(px(TITLEBAR_POPUP_MENU_INNER_MAX_HEIGHT))
            .items_padding_bottom(px(0.0))
            .scrollable(true)
            .scrollbar_thickness(px(TITLEBAR_DROPDOWN_SCROLLBAR_WIDTH))
            .scrollbar_show(ScrollbarShow::Scrolling)
            .check_side(Side::Right);

        if actions.is_empty() {
            menu = menu.menu_element_with_disabled(
                Box::new(ConfigureGpuiTitlebarActions),
                true,
                move |_, _| titlebar_popup_empty_menu_row("No Actions configured".to_string()),
            );
        } else {
            for (action_index, action) in actions.iter().enumerate() {
                let row = action.clone();
                let checked = active_command_id.as_deref() == Some(row.command_id.as_str());
                menu = menu.menu_element_with_check(
                    checked,
                    Box::new(RunGpuiTitlebarAction {
                        action_index: action_index as u64,
                    }),
                    move |_, _| titlebar_popup_action_menu_row(row.clone()),
                );
            }
        }

        menu.separator()
            .menu_element(Box::new(ConfigureGpuiTitlebarActions), move |_, _| {
                titlebar_popup_standard_menu_row(
                    TITLEBAR_ICON_SETTINGS,
                    TITLEBAR_POPUP_MENU_ROW_ICON_SIZE,
                    "Configure".to_string(),
                    false,
                )
            })
    }

    pub(crate) fn build_gpui_titlebar_git_popup_menu(&self, menu: PopupMenu) -> PopupMenu {
        let mut menu = menu
            .min_w(px(TITLEBAR_POPUP_GIT_WIDTH))
            .max_w(px(TITLEBAR_POPUP_GIT_WIDTH))
            .max_h(px(TITLEBAR_POPUP_MENU_INNER_MAX_HEIGHT))
            .scrollable(true)
            .scrollbar_thickness(px(TITLEBAR_DROPDOWN_SCROLLBAR_WIDTH))
            .scrollbar_show(ScrollbarShow::Scrolling)
            .check_side(Side::Right);

        let Some(state) = self.titlebar_git_menu_state.as_ref() else {
            return menu.menu_element_with_disabled(
                Box::new(CopyGpuiTitlebarGitBranch),
                true,
                move |_, _| titlebar_popup_empty_menu_row("Loading Git state...".to_string()),
            );
        };

        menu = titlebar_popup_git_section(menu, "Status");

        let branch_value = state
            .branch
            .clone()
            .unwrap_or_else(|| "(detached HEAD)".to_string());
        let branch_disabled = !state.is_repo;
        menu = menu.menu_element_with_disabled(
            Box::new(CopyGpuiTitlebarGitBranch),
            branch_disabled,
            move |_, _| titlebar_popup_git_branch_menu_row(branch_value.clone(), branch_disabled),
        );

        menu = menu.menu_element(Box::new(OpenGpuiTitlebarGitCommitScreen), {
            let additions = state.additions;
            let deletions = state.deletions;
            move |_, _| titlebar_popup_git_changes_menu_row(additions, deletions)
        });

        let commits_disabled =
            state.sync_remote_disabled || (state.ahead_count == 0 && state.behind_count == 0);
        menu = menu.menu_element_with_disabled(
            Box::new(RunGpuiTitlebarGitRemoteSync),
            commits_disabled,
            {
                let ahead_count = state.ahead_count;
                let behind_count = state.behind_count;
                move |_, _| {
                    titlebar_popup_git_commits_menu_row(ahead_count, behind_count, commits_disabled)
                }
            },
        );

        menu = titlebar_popup_git_section(menu.separator(), "Actions");
        for (row_index, row) in state.rows.iter().enumerate() {
            let row = row.clone();
            menu = menu.menu_element_with_disabled(
                Box::new(RunGpuiTitlebarGitMenuAction {
                    row_index: row_index as u64,
                }),
                row.disabled,
                move |_, _| titlebar_popup_git_action_menu_row(row.clone()),
            );
        }
        menu
    }

    pub(crate) fn build_gpui_titlebar_tips_popup_menu(&self, menu: PopupMenu) -> PopupMenu {
        let read_ids = gpui_titlebar_tips_read_ids_from_settings();
        let unread_count = GPUI_NATIVE_TITLEBAR_TIPS
            .len()
            .saturating_sub(read_ids.len());
        let mut menu = menu
            .min_w(px(TITLEBAR_POPUP_TIPS_WIDTH))
            .max_w(px(TITLEBAR_POPUP_TIPS_WIDTH))
            .max_h(px(TITLEBAR_POPUP_READING_MENU_MAX_HEIGHT))
            .scrollable(true);

        menu = menu.menu_element_with_disabled(
            Box::new(CopyGpuiTitlebarGitBranch),
            true,
            move |_, _| {
                titlebar_popup_reading_header(
                    TITLEBAR_ICON_INFO,
                    "Tips".to_string(),
                    format!("{unread_count} unread"),
                )
            },
        );
        for (action_index, (label, icon_path)) in [
            ("Docs", "titlebar/book.svg"),
            ("Video", "titlebar/sparkles.svg"),
            ("Setup", "titlebar/tool.svg"),
            ("Updates", "titlebar/history.svg"),
        ]
        .into_iter()
        .enumerate()
        {
            menu = menu.menu_element(
                Box::new(RunGpuiTitlebarTipsHeaderAction {
                    action_index: action_index as u64,
                }),
                move |_, _| {
                    titlebar_popup_standard_menu_row(
                        icon_path,
                        TITLEBAR_POPUP_MENU_ROW_ICON_SIZE,
                        label.to_string(),
                        false,
                    )
                },
            );
        }

        let settings = shared_settings::shared_sidebar_settings_snapshot();
        let show_persistence_notice =
            gpui_titlebar_session_persistence_provider_from_settings(settings.object()) == "off";
        let show_debug_notice = settings.debugging_mode();
        if show_persistence_notice || show_debug_notice {
            menu = titlebar_popup_git_section(menu.separator(), "Notices");
            if show_persistence_notice {
                menu = menu.menu_element(
                    Box::new(RunGpuiTitlebarTipsHeaderAction { action_index: 4 }),
                    move |_, _| titlebar_popup_tip_row(
                        "titlebar/bug.svg",
                        "Mobile attach needs persistence".to_string(),
                        "Enable zmx persistence so mobile clients reconnect to durable terminal sessions.".to_string(),
                        false,
                    ),
                );
            }
            if show_debug_notice {
                menu = menu.menu_element(
                    Box::new(RunGpuiTitlebarTipsHeaderAction { action_index: 4 }),
                    move |_, _| titlebar_popup_tip_row(
                        "titlebar/bug.svg",
                        "Debug mode is on".to_string(),
                        "Ghostex is showing debug UI controls and allowing enabled diagnostic scenarios to write routine logs.".to_string(),
                        false,
                    ),
                );
            }
        }

        let unread = GPUI_NATIVE_TITLEBAR_TIPS
            .iter()
            .enumerate()
            .filter(|(_, tip)| !read_ids.contains(tip.id))
            .collect::<Vec<_>>();
        let read = GPUI_NATIVE_TITLEBAR_TIPS
            .iter()
            .enumerate()
            .filter(|(_, tip)| read_ids.contains(tip.id))
            .collect::<Vec<_>>();
        if !unread.is_empty() {
            menu = titlebar_popup_git_section(menu.separator(), "Unread");
            for (tip_index, tip) in unread {
                let tip = *tip;
                menu = menu.menu_element(
                    Box::new(RunGpuiTitlebarTip {
                        tip_index: tip_index as u64,
                    }),
                    move |_, _| {
                        titlebar_popup_tip_row(
                            tip.icon_path,
                            tip.title.to_string(),
                            tip.body.to_string(),
                            true,
                        )
                    },
                );
            }
        }
        if !read.is_empty() {
            menu = titlebar_popup_git_section(menu.separator(), "Read");
            for (tip_index, tip) in read {
                let tip = *tip;
                menu = menu.menu_element(
                    Box::new(RunGpuiTitlebarTip {
                        tip_index: tip_index as u64,
                    }),
                    move |_, _| {
                        titlebar_popup_tip_row(
                            tip.icon_path,
                            tip.title.to_string(),
                            tip.body.to_string(),
                            false,
                        )
                    },
                );
            }
        }
        menu
    }

    pub(crate) fn build_gpui_titlebar_resources_popup_menu(
        menu: PopupMenu,
        snapshot: GpuiNativeResourcesSnapshot,
    ) -> PopupMenu {
        let mut menu = menu
            .min_w(px(TITLEBAR_POPUP_RESOURCES_WIDTH))
            .max_w(px(TITLEBAR_POPUP_RESOURCES_WIDTH))
            .max_h(px(TITLEBAR_POPUP_READING_MENU_MAX_HEIGHT))
            .scrollable(true);
        let total_label = format!(
            "{}  •  {}",
            format_gpui_resource_cpu(snapshot.total_cpu),
            format_gpui_resource_memory(snapshot.total_memory_mb),
        );
        menu = menu.menu_element_with_disabled(
            Box::new(CopyGpuiTitlebarGitBranch),
            true,
            move |_, _| {
                titlebar_popup_reading_header(
                    TITLEBAR_ICON_DEVICE_DESKTOP,
                    "Resources".to_string(),
                    total_label.clone(),
                )
            },
        );
        menu = menu
            .menu_element(Box::new(SleepInactiveSessionsFromTitlebar), move |_, _| {
                titlebar_popup_standard_menu_row(
                    COMMAND_ICON_MOON,
                    TITLEBAR_POPUP_MENU_ROW_ICON_SIZE,
                    "Sleep Inactive Sessions".to_string(),
                    false,
                )
            })
            .menu_element(Box::new(OpenGpuiDaemonSessionsModal), move |_, _| {
                titlebar_popup_standard_menu_row(
                    TITLEBAR_ICON_DEVICE_DESKTOP,
                    TITLEBAR_POPUP_MENU_ROW_ICON_SIZE,
                    "Running Sessions…".to_string(),
                    false,
                )
            })
            .menu_element(Box::new(RestartGpuiGxserverFromTitlebar), move |_, _| {
                titlebar_popup_standard_menu_row(
                    BROWSER_ICON_RELOAD,
                    TITLEBAR_POPUP_MENU_ROW_ICON_SIZE,
                    "Restart gxserver".to_string(),
                    false,
                )
            });

        for (label, rows) in [
            ("Dev Servers", snapshot.server_rows),
            ("Ghostex", snapshot.session_rows),
            ("Code IDE", snapshot.code_rows),
            ("Browser Tabs", snapshot.browser_rows),
            ("Orphaned / Detached", snapshot.orphan_rows),
        ] {
            if rows.is_empty() {
                continue;
            }
            menu = titlebar_popup_git_section(menu.separator(), label);
            for row in rows {
                let action: Box<dyn Action> = if let Some(session_id) = row.session_id.clone() {
                    Box::new(FocusGpuiTitlebarResourceSession { session_id })
                } else if let Some(url) = row.url.clone() {
                    Box::new(OpenGpuiTitlebarResourceUrl { url })
                } else {
                    Box::new(CopyGpuiTitlebarGitBranch)
                };
                let disabled = row.session_id.is_none() && row.url.is_none();
                menu = menu.menu_element_with_disabled(action, disabled, move |_, _| {
                    titlebar_popup_resource_row(row.clone(), disabled)
                });
            }
        }
        menu
    }

    pub(crate) fn gpui_native_resources_snapshot(
        &self,
        cx: &mut gpui::Context<Self>,
    ) -> GpuiNativeResourcesSnapshot {
        /*
        GPUI owns this process snapshot directly. The native popup samples only
        when opened, so Tips/Resources no longer create a CEF browser, wait for
        React readiness, or run hidden web polling after dismissal.
        */
        let processes = gpui_read_native_resource_processes();
        let servers = gpui_read_native_resource_servers();
        self.gpui_native_resources_snapshot_from_samples(processes, servers, cx)
    }

    pub(crate) fn gpui_native_resources_snapshot_from_samples(
        &self,
        processes: Vec<GpuiNativeResourceProcess>,
        servers: Vec<GpuiNativeResourceServer>,
        cx: &mut gpui::Context<Self>,
    ) -> GpuiNativeResourcesSnapshot {
        let children_by_parent = gpui_native_resource_children_by_parent(&processes);
        let mut claimed_pids = HashSet::new();
        let mut session_rows = Vec::new();
        let mut inactive_terminal_sleep_count = 0;
        let mut sleep_all_session_count = 0;

        let protected_browser_tab_ids = if self.active_mode == TitlebarMode::Browser
            && self
                .project_editor_shell
                .is_mode_awake(TitlebarMode::Browser)
        {
            self.browser_tabs.rendered_active_loaded_tab_ids()
        } else {
            HashSet::new()
        };
        sleep_all_session_count += self.browser_surfaces.len();
        inactive_terminal_sleep_count += self
            .browser_surfaces
            .keys()
            .filter(|tab_id| !protected_browser_tab_ids.contains(tab_id))
            .count();

        for session in &self.agents_workspace.terminal_sessions {
            let title = self.agents_workspace_tab_display_title(session.id);
            let mapped_key =
                self.local_workspace_session_mappings
                    .iter()
                    .find_map(|(key, shell_session_id)| {
                        (*shell_session_id == session.id).then_some(key)
                    });
            let session_id = mapped_key
                .map(|key| gpui_combined_presentation_session_id(&key.project_id, &key.session_id))
                .unwrap_or_else(|| gpui_agents_session_external_id(session.id));
            let match_tokens = [
                session.zmx_session_name.as_deref(),
                Some(session_id.as_str()),
                Some(title.as_str()),
            ];
            let seeds = processes
                .iter()
                .filter(|process| {
                    match_tokens.iter().flatten().any(|token| {
                        let token = token.trim();
                        token.chars().count() >= 4 && process.command.contains(token)
                    })
                })
                .cloned()
                .collect::<Vec<_>>();
            let tree = gpui_collect_native_resource_process_tree(&seeds, &children_by_parent);
            if tree.is_empty()
                && (session.presentation_state == TerminalSessionPresentationState::Sleeping
                    || session.presentation_state
                        == TerminalSessionPresentationState::StartupFailed
                    || session.zmx_session_name.is_none())
            {
                continue;
            }
            claimed_pids.extend(tree.iter().map(|process| process.pid));
            let (cpu, memory_mb) = gpui_sum_native_resource_processes(&tree);
            sleep_all_session_count += 1;
            if session.presentation_state != TerminalSessionPresentationState::Sleeping
                && session.activity == AgentTerminalActivity::Idle
                && !session.delayed_send_active
            {
                inactive_terminal_sleep_count += 1;
            }
            session_rows.push(GpuiNativeResourceRow {
                action: GpuiNativeResourceAction::Session,
                agent_icon: session.agent_icon,
                children: gpui_native_resource_child_rows(&tree, seeds.first().map(|row| row.pid)),
                cpu,
                detail: match seeds.first() {
                    Some(process) => format!(
                        "{} terminal pid {}",
                        gpui_native_resource_process_name(process),
                        process.system_pid
                    ),
                    None => "Active, not loaded".to_string(),
                },
                icon_path: "titlebar/terminal-2.svg",
                label: title,
                memory_mb,
                pids: tree.iter().map(|process| process.system_pid).collect(),
                session_id: Some(session_id),
                url: None,
            });
        }

        let mut browser_rows = Vec::new();
        for tab in &self.browser_tabs.tabs {
            if tab.state != BrowserTabState::Loaded {
                continue;
            }
            let Some(surface) = self.browser_surfaces.get(&tab.id) else {
                continue;
            };
            let browser_id = surface.read(cx).browser_identifier().to_string();
            let browser_processes = processes
                .iter()
                .filter(|process| {
                    !claimed_pids.contains(&process.pid)
                        && gpui_native_resource_is_ghostex_browser_process(process)
                        && (process
                            .command
                            .contains(&format!("--client-id={browser_id}"))
                            || process
                                .command
                                .contains(&format!("--renderer-client-id={browser_id}")))
                })
                .cloned()
                .collect::<Vec<_>>();
            if browser_processes.is_empty() {
                continue;
            }
            claimed_pids.extend(browser_processes.iter().map(|process| process.pid));
            let (cpu, memory_mb) = gpui_sum_native_resource_processes(&browser_processes);
            session_rows.push(GpuiNativeResourceRow {
                action: GpuiNativeResourceAction::Browser(tab.id),
                agent_icon: None,
                children: gpui_native_resource_child_rows(&browser_processes, None),
                cpu,
                detail: tab.url.clone(),
                icon_path: BROWSER_ICON_WORLD,
                label: tab.display_title(),
                memory_mb,
                pids: browser_processes
                    .iter()
                    .map(|process| process.system_pid)
                    .collect(),
                session_id: None,
                url: Some(tab.url.clone()),
            });
        }

        let browser_runtime_processes = processes
            .iter()
            .filter(|process| {
                !claimed_pids.contains(&process.pid)
                    && gpui_native_resource_is_ghostex_browser_process(process)
            })
            .cloned()
            .collect::<Vec<_>>();
        if !browser_runtime_processes.is_empty() {
            claimed_pids.extend(browser_runtime_processes.iter().map(|process| process.pid));
            let (cpu, memory_mb) = gpui_sum_native_resource_processes(&browser_runtime_processes);
            browser_rows.push(GpuiNativeResourceRow {
                action: GpuiNativeResourceAction::None,
                agent_icon: None,
                children: gpui_native_resource_child_rows(&browser_runtime_processes, None),
                cpu,
                detail: "Shared GPU, network, and storage helpers".to_string(),
                icon_path: BROWSER_ICON_WORLD,
                label: "Browser runtime".to_string(),
                memory_mb,
                pids: browser_runtime_processes
                    .iter()
                    .map(|process| process.system_pid)
                    .collect(),
                session_id: None,
                url: None,
            });
        }

        /*
        CDXC:GPUITitlebarResources 2026-08-19-12:10:
        Dev Servers rows describe one listening *process*, not one listening
        socket, and never root at the app's own executables. The Ghostex shell
        listens on the CEF remote-debugging port, so rooting a row there walked
        the whole app process tree and reported every CEF helper as a single
        dev server; a process holding several ports repeated its whole tree in
        one row per port. Both inflated the row and the section total far past
        the app total. Keep the listener process plus its own descendants, stop
        at any other listener and at app executables, and fold a process's
        extra ports into its one row.
        */
        let listener_pids = servers
            .iter()
            .map(|server| server.pid)
            .collect::<HashSet<_>>();
        let mut grouped_servers: Vec<(GpuiNativeResourceServer, Vec<u16>)> = Vec::new();
        for server in servers {
            let Some(process) = processes.iter().find(|process| process.pid == server.pid) else {
                continue;
            };
            if gpui_native_resource_is_app_shell_process(process) {
                continue;
            }
            if !claimed_pids.contains(&server.pid)
                && !gpui_native_resource_is_ghostex_owned_process(process)
            {
                continue;
            }
            match grouped_servers
                .iter_mut()
                .find(|(existing, _)| existing.pid == server.pid)
            {
                Some((existing, extra_ports)) => {
                    if server.port < existing.port {
                        extra_ports.push(existing.port);
                        *existing = server;
                    } else {
                        extra_ports.push(server.port);
                    }
                }
                None => grouped_servers.push((server, Vec::new())),
            }
        }

        grouped_servers.sort_by_key(|(server, _)| (server.port, server.pid));

        let mut server_rows = Vec::new();
        for (server, mut extra_ports) in grouped_servers {
            let Some(process) = processes.iter().find(|process| process.pid == server.pid) else {
                continue;
            };
            let owning_session = session_rows.iter().find(|row| {
                matches!(row.action, GpuiNativeResourceAction::Session)
                    && row.pids.contains(&server.pid)
            });
            let tree = gpui_collect_native_resource_process_tree_bounded(
                std::slice::from_ref(process),
                &children_by_parent,
                &|candidate| {
                    listener_pids.contains(&candidate.pid)
                        || gpui_native_resource_is_app_shell_process(candidate)
                },
            );
            let (cpu, memory_mb) = gpui_sum_native_resource_processes(&tree);
            extra_ports.sort_unstable();
            extra_ports.dedup();
            let mut detail = format!(
                "{} pid {}",
                gpui_native_resource_process_name(process),
                process.system_pid
            );
            if !extra_ports.is_empty() {
                detail.push_str(&format!(
                    " • also :{}",
                    extra_ports
                        .iter()
                        .map(|port| port.to_string())
                        .collect::<Vec<_>>()
                        .join(", :")
                ));
            }
            server_rows.push(GpuiNativeResourceRow {
                action: GpuiNativeResourceAction::Server,
                agent_icon: owning_session.and_then(|row| row.agent_icon),
                children: gpui_native_resource_child_rows(&tree, Some(server.pid)),
                cpu,
                detail,
                icon_path: BROWSER_ICON_WORLD,
                label: server.label,
                memory_mb,
                pids: tree.iter().map(|process| process.system_pid).collect(),
                session_id: owning_session.and_then(|row| row.session_id.clone()),
                url: Some(server.url),
            });
        }


        let mut code_rows = Vec::new();
        if let Some(process) = processes
            .iter()
            .find(|process| process.command.contains("code-server"))
        {
            let tree = gpui_collect_native_resource_process_tree(
                std::slice::from_ref(process),
                &children_by_parent,
            );
            let (cpu, memory_mb) = gpui_sum_native_resource_processes(&tree);
            code_rows.push(GpuiNativeResourceRow {
                action: GpuiNativeResourceAction::Code,
                agent_icon: None,
                children: gpui_native_resource_child_rows(&tree, Some(process.pid)),
                cpu,
                detail: format!("pid {}", process.system_pid),
                icon_path: TITLEBAR_ICON_CODE,
                label: "Code".to_string(),
                memory_mb,
                pids: tree.iter().map(|process| process.system_pid).collect(),
                session_id: None,
                url: None,
            });
        }

        let orphan_roots = processes
            .iter()
            .filter(|process| {
                !claimed_pids.contains(&process.pid)
                    && gpui_native_resource_is_ghostex_owned_process(process)
                    && gpui_native_resource_is_user_runtime_process(process)
            })
            .filter(|process| {
                !processes.iter().any(|parent| {
                    parent.pid == process.ppid
                        && !claimed_pids.contains(&parent.pid)
                        && gpui_native_resource_is_ghostex_owned_process(parent)
                        && gpui_native_resource_is_user_runtime_process(parent)
                })
            })
            .take(16)
            .cloned()
            .collect::<Vec<_>>();
        let mut orphan_rows = Vec::new();
        for root in orphan_roots {
            let tree = gpui_collect_native_resource_process_tree(
                std::slice::from_ref(&root),
                &children_by_parent,
            )
            .into_iter()
            .filter(|process| !claimed_pids.contains(&process.pid))
            .collect::<Vec<_>>();
            claimed_pids.extend(tree.iter().map(|process| process.pid));
            let (cpu, memory_mb) = gpui_sum_native_resource_processes(&tree);
            orphan_rows.push(GpuiNativeResourceRow {
                action: GpuiNativeResourceAction::Orphan,
                agent_icon: None,
                children: gpui_native_resource_child_rows(&tree, Some(root.pid)),
                cpu,
                detail: format!("pid {}", root.system_pid),
                icon_path: TITLEBAR_ICON_BOX,
                label: gpui_native_resource_process_name(&root),
                memory_mb,
                pids: tree.iter().map(|process| process.system_pid).collect(),
                session_id: None,
                url: None,
            });
        }

        let app_roots = processes
            .iter()
            .filter(|process| {
                gpui_native_resource_is_app_bundle_process(process)
                    || (cfg!(target_os = "windows")
                        && gpui_native_resource_is_ghostex_owned_process(process))
            })
            .cloned()
            .collect::<Vec<_>>();
        let app_tree = gpui_collect_native_resource_process_tree(&app_roots, &children_by_parent);
        let (total_cpu, total_memory_mb) = gpui_sum_native_resource_processes(&app_tree);
        GpuiNativeResourcesSnapshot {
            browser_rows,
            code_rows,
            inactive_terminal_sleep_count,
            orphan_rows,
            persistent_session_mode: gpui_titlebar_session_persistence_provider_from_settings(
                shared_settings::shared_sidebar_settings_snapshot().object(),
            ) != "off",
            project_label: self
                .latest_sidebar_project_snapshot
                .as_ref()
                .map(|snapshot| snapshot.display_name.clone())
                .unwrap_or_else(|| "Ghostex".to_string()),
            server_rows,
            session_rows,
            sleep_all_session_count,
            total_cpu,
            total_memory_mb,
        }
    }

    pub(crate) fn open_gpui_settings_actions_modal_from_titlebar(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUITitlebarActions 2026-06-24-14:24:
        Empty, missing, or unconfigured titlebar Actions paths must deep-link to the shared Settings modal Actions tab with `{ modal: "settings", initialTab: "actions" }`. Do not reopen the old configureActions modal id or a GPUI placeholder surface from this titlebar path.
        */
        let modal = GpuiAppModalKind::Settings;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = serde_json::json!({
            "initialTab": "actions",
            "modal": modal.modal_id(),
            "type": "open",
        });
        open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        self.open_gpui_app_modal_window(
            modal,
            open_message,
            sidebar_state_message,
            Some(window),
            cx,
        );
    }

    pub(crate) fn open_gpui_settings_plugins_page(
        &mut self,
        window: Option<&mut Window>,
        cx: &mut gpui::Context<Self>,
    ) {
        let modal = GpuiAppModalKind::Settings;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = serde_json::json!({
            "initialTab": "plugins",
            "modal": modal.modal_id(),
            "type": "open",
        });
        open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        self.open_gpui_app_modal_window(modal, open_message, sidebar_state_message, window, cx);
    }

    pub(crate) fn run_gpui_titlebar_tips_header_action(
        &mut self,
        action_index: usize,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        match action_index {
            0 => self.open_gpui_browser_action_url(GHOSTEX_DOCS_URL.to_string(), window, cx),
            1 => self.open_gpui_app_modal_from_titlebar(
                GpuiAppModalKind::WatchGhostexVideo,
                window,
                cx,
            ),
            2 => self.open_gpui_app_modal_from_titlebar(
                GpuiAppModalKind::FirstLaunchSetup,
                window,
                cx,
            ),
            3 => self.open_gpui_browser_action_url(GHOSTEX_CHANGELOG_URL.to_string(), window, cx),
            4 => self.open_gpui_settings_integrations_from_titlebar(None, window, cx),
            _ => {}
        }
    }

    pub(crate) fn run_gpui_titlebar_tip(
        &mut self,
        tip_index: usize,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(tip) = GPUI_NATIVE_TITLEBAR_TIPS.get(tip_index).copied() else {
            return;
        };
        gpui_mark_titlebar_tip_read(tip.id);
        self.titlebar_tips_unread_count = gpui_titlebar_tips_unread_count_from_settings();
        match tip.id {
            "use-ghostex-computer-use-skill" => self.open_gpui_settings_integrations_from_titlebar(
                Some("Ghostex Computer Use"),
                window,
                cx,
            ),
            "use-ghostex-browser-use-skill" => self.open_gpui_settings_integrations_from_titlebar(
                Some("Ghostex Browser Use"),
                window,
                cx,
            ),
            "use-ghostex-embedded-browser-use-skill" => self
                .open_gpui_settings_integrations_from_titlebar(
                    Some("Ghostex Embedded Browser Use"),
                    window,
                    cx,
                ),
            "use-ghostex-auto-rename-session-skill" => self
                .open_gpui_settings_integrations_from_titlebar(
                    Some("Ghostex Auto Rename Session"),
                    window,
                    cx,
                ),
            "recommend-faster-chrome-devtools-skill" => self.open_gpui_browser_action_url(
                "https://github.com/zeke/faster-chrome-devtools-skill".to_string(),
                window,
                cx,
            ),
            _ => cx.notify(),
        }
    }

    pub(crate) fn open_gpui_settings_integrations_from_titlebar(
        &mut self,
        search_query: Option<&str>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let modal = GpuiAppModalKind::Settings;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = serde_json::json!({
            "initialTab": "integrations",
            "modal": modal.modal_id(),
            "type": "open",
        });
        if let Some(search_query) = search_query {
            open_message["initialSearchQuery"] = serde_json::json!(search_query);
        }
        open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        self.open_gpui_app_modal_window(
            modal,
            open_message,
            sidebar_state_message,
            Some(window),
            cx,
        );
    }

    pub(crate) fn open_gpui_titlebar_notice_settings(
        &mut self,
        target: GpuiNativeTitlebarNoticeTarget,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let (initial_tab, search_query) = match target {
            GpuiNativeTitlebarNoticeTarget::AgentHooks => ("integrations", "Agent Hooks"),
            GpuiNativeTitlebarNoticeTarget::DebuggingMode => ("settings", "Show debug UI controls"),
            GpuiNativeTitlebarNoticeTarget::GhostexCli => ("integrations", "Ghostex CLI"),
            GpuiNativeTitlebarNoticeTarget::SessionPersistence => {
                ("ghostty", "Session Persistence")
            }
        };
        let modal = GpuiAppModalKind::Settings;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = serde_json::json!({
            "initialSearchQuery": search_query,
            "initialTab": initial_tab,
            "modal": modal.modal_id(),
            "type": "open",
        });
        open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        self.open_gpui_app_modal_window(
            modal,
            open_message,
            sidebar_state_message,
            Some(window),
            cx,
        );
    }

    pub(crate) fn visible_gpui_titlebar_actions(&self) -> Vec<GpuiTitlebarAction> {
        self.titlebar_actions_snapshot.clone()
    }

    pub(crate) fn refresh_titlebar_actions_in_background(&mut self, cx: &mut gpui::Context<Self>) {
        if self.titlebar_actions_refresh_in_flight {
            return;
        }
        self.titlebar_actions_refresh_in_flight = true;
        let fetched_project_id =
            gpui_active_project_id_from_snapshot(self.latest_sidebar_project_snapshot.as_ref())
                .map(str::to_string);
        let request_project_id = fetched_project_id.clone();
        cx.spawn(async move |this, cx| {
            let actions = cx
                .background_executor()
                .spawn(async move {
                    gpui_titlebar_actions_for_active_project_id(request_project_id.as_deref())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.titlebar_actions_refresh_in_flight = false;
                let current_project_id = gpui_active_project_id_from_snapshot(
                    this.latest_sidebar_project_snapshot.as_ref(),
                )
                .map(str::to_string);
                if current_project_id != fetched_project_id {
                    this.refresh_titlebar_actions_in_background(cx);
                    return;
                }
                if this.titlebar_actions_snapshot != actions {
                    this.titlebar_actions_snapshot = actions;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(crate) fn titlebar_selection_owner_project_id(&self) -> Option<&str> {
        self.latest_sidebar_project_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.selection_owner_project_id.as_ref())
            .map(|project_id| project_id.0.as_str())
    }

    pub(crate) fn restore_gpui_titlebar_project_selections(&mut self) {
        let Some(project_id) = self
            .titlebar_selection_owner_project_id()
            .map(str::to_string)
        else {
            self.active_open_target_id = None;
            self.active_action_command_id = None;
            return;
        };
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        self.active_open_target_id = gpui_titlebar_project_selection_from_settings(
            settings.object(),
            GPUI_TITLEBAR_OPEN_TARGET_SELECTIONS_SETTINGS_KEY,
            &project_id,
        );
        self.active_action_command_id = gpui_titlebar_project_selection_from_settings(
            settings.object(),
            GPUI_TITLEBAR_ACTION_SELECTIONS_SETTINGS_KEY,
            &project_id,
        );
    }

    pub(crate) fn persist_gpui_titlebar_project_selection(&self, settings_key: &str, value: &str) {
        let Some(project_id) = self.titlebar_selection_owner_project_id() else {
            return;
        };
        let _ = gpui_persist_titlebar_project_selection(settings_key, project_id, value);
    }

    pub(crate) fn configured_gpui_titlebar_actions(&self) -> Vec<GpuiTitlebarAction> {
        self.visible_gpui_titlebar_actions()
            .into_iter()
            .filter(GpuiTitlebarAction::is_configured)
            .collect()
    }

    pub(crate) fn active_gpui_titlebar_action(&self) -> Option<GpuiTitlebarAction> {
        let actions = self.configured_gpui_titlebar_actions();
        self.active_action_command_id
            .as_deref()
            .and_then(|active_id| {
                actions
                    .iter()
                    .find(|action| action.command_id == active_id)
                    .cloned()
            })
            .or_else(|| actions.into_iter().next())
    }

    pub(crate) fn run_active_gpui_titlebar_action(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(action) = self.active_gpui_titlebar_action() else {
            self.open_gpui_settings_actions_modal_from_titlebar(window, cx);
            return;
        };
        self.run_gpui_titlebar_action_from_titlebar(action, window, cx);
    }

    pub(crate) fn run_gpui_titlebar_action_index(
        &mut self,
        action_index: usize,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(action) = self
            .visible_gpui_titlebar_actions()
            .into_iter()
            .nth(action_index)
        else {
            return;
        };
        self.run_gpui_titlebar_action_from_titlebar(action, window, cx);
    }

    pub(crate) fn run_configured_gpui_titlebar_action_index(
        &mut self,
        action_index: usize,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(action) = self
            .configured_gpui_titlebar_actions()
            .into_iter()
            .nth(action_index)
        else {
            return;
        };
        self.run_gpui_titlebar_action_from_titlebar(action, window, cx);
    }

    pub(crate) fn run_gpui_titlebar_action_from_titlebar(
        &mut self,
        mut action: GpuiTitlebarAction,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUITitlebarActions 2026-06-27-09:26:
        Titlebar left-clicks, right-click menu rows, and positional Action hotkeys are click sources, so GPUI derives Debug reruns from sanitized local feedback just like the React command palette. Sidebar bridge payloads call `run_gpui_titlebar_action` directly and keep their explicit `runMode` authority.
        */
        action.run_mode = gpui_titlebar_action_run_mode_for_click(
            &action,
            self.sidebar_command_run_feedback_states
                .get(&action.command_id),
        );
        self.run_gpui_titlebar_action(action, window, cx);
    }

    pub(crate) fn run_gpui_titlebar_action(
        &mut self,
        action: GpuiTitlebarAction,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if !action.is_configured() {
            self.open_gpui_settings_actions_modal_from_titlebar(window, cx);
            return;
        }

        match action.action_type {
            GpuiTitlebarActionType::Browser => {
                let Some(url) = action
                    .url
                    .as_deref()
                    .and_then(|url| gpui_trimmed_nonempty_str(Some(url)))
                    .map(str::to_string)
                else {
                    self.open_gpui_settings_actions_modal_from_titlebar(window, cx);
                    return;
                };
                self.persist_gpui_titlebar_project_selection(
                    GPUI_TITLEBAR_ACTION_SELECTIONS_SETTINGS_KEY,
                    &action.command_id,
                );
                self.active_action_command_id = Some(action.command_id);
                self.open_gpui_browser_action_url(url, window, cx);
            }
            GpuiTitlebarActionType::Terminal => {
                let Some(command) = action
                    .command
                    .as_deref()
                    .and_then(|command| gpui_trimmed_nonempty_str(Some(command)))
                    .map(str::to_string)
                else {
                    self.open_gpui_settings_actions_modal_from_titlebar(window, cx);
                    return;
                };
                let title = action.command_title();
                let command_id = action.command_id.clone();
                self.persist_gpui_titlebar_project_selection(
                    GPUI_TITLEBAR_ACTION_SELECTIONS_SETTINGS_KEY,
                    &command_id,
                );
                self.active_action_command_id = Some(command_id.clone());
                match action.run_mode {
                    GpuiTitlebarActionRunMode::Default => {
                        self.open_gpui_command_action_terminal(
                            command_id,
                            title,
                            command,
                            action.play_completion_sound,
                            action.close_terminal_on_exit,
                            window,
                            cx,
                        );
                    }
                    GpuiTitlebarActionRunMode::Debug => {
                        self.open_gpui_debug_command_action_terminal(title, command, cx);
                    }
                }
                self.open_gpui_titlebar_action_links(&action.links, window, cx);
            }
        }
    }

    pub(crate) fn open_gpui_titlebar_action_links(
        &mut self,
        links: &[GpuiTitlebarActionLink],
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:ProjectActions 2026-07-31-12:00:
        Terminal Actions open their saved links right after the command-pane
        launch. Integrated links reuse the same Browser tab path as renderer
        `openBrowserUrl` commands (same-origin reuse, otherwise a new loaded
        tab) so re-running an Action does not multiply tabs; external links go
        through the http/https-only OS open helper after the same toolbar
        normalization as typed addresses.
        */
        for link in links {
            match link.target {
                GpuiTitlebarActionLinkTarget::Integrated => {
                    self.open_browser_url_from_renderer_command(
                        GpuiSidebarOpenBrowserUrlMessage {
                            url: link.url.clone(),
                            reuse: GpuiBrowserRendererOpenReuse::Similar,
                            from_quick_header: false,
                            project_id: None,
                        },
                        window,
                        cx,
                    );
                }
                GpuiTitlebarActionLinkTarget::External => {
                    if let Some(url) = normalize_address(&link.url) {
                        let _ = gpui_open_external_http_url(&url);
                    }
                }
            }
        }
    }

    pub(crate) fn open_gpui_browser_action_url(
        &mut self,
        url: String,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUITitlebarActions 2026-06-24-14:24:
        Browser Actions must enter the existing GPUI Browser tab/CEF path: switch to Browser, wake the Browser shell, load the saved URL into the active Browser tab, and let Browser surface machinery own navigation. Do not call OS open, shell commands, external browsers, persistent logs, or duplicate CEF surfaces from the titlebar action.
        */
        if !self.titlebar_mode_available(TitlebarMode::Browser) {
            return;
        }
        self.active_mode = TitlebarMode::Browser;
        self.set_shell_focus(ShellFocusTarget::BrowserPane(
            self.browser_tabs.focused_pane,
        ));
        self.set_browser_address_input_value_unchecked(
            self.browser_tabs.focused_pane,
            url.clone(),
            window,
            cx,
        );
        self.commit_browser_address(url, cx);
    }

    pub(crate) fn open_gpui_debug_command_action_terminal(
        &mut self,
        title: String,
        command: String,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUICommandPane 2026-06-25-10:29:
        `runMode:"debug"` must match macOS Debug Action behavior: create a normal visible Agents workspace terminal titled `Debug: <Action>` and send the saved command as visible initial input with the Atuin-ignore prefix. Do not reuse command-pane tabs, post command-button run state, write command status files, or hide the wrapper process for debug runs.
        */
        let working_directory = self
            .latest_sidebar_project_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.in_memory_project_path.as_ref())
            .and_then(|path| path.to_str())
            .map(str::to_string);
        let payload = AgentsTerminalStartupExplicitLaunchPayload {
            working_directory,
            command: None,
            env_vars: Vec::new(),
            initial_input: Some(gpui_debug_command_action_initial_input(&command)),
            wait_after_command: false,
        };
        if payload.to_ghostty_launch_payload().is_err() {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Action unavailable",
                "GPUI could not prepare the debug Action terminal.",
                cx,
            );
            return;
        }

        let requested_pane_id = self.agents_workspace.focused_pane;
        let Some(session_id) = self
            .agents_workspace
            .add_mounting_session_to_pane(requested_pane_id)
        else {
            self.dispatch_gpui_app_modal_toast(
                "warning",
                "Action unavailable",
                "GPUI could not create a debug Action terminal.",
                cx,
            );
            return;
        };
        if let Some(session) = self
            .agents_workspace
            .terminal_sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        {
            session.title = format!("Debug: {title}");
        }
        let pane_id = self.agents_workspace.focused_pane;
        let runtime_session_id = self
            .agents_terminal_runtime_sessions
            .ensure_runtime_session_id(session_id);
        self.agents_terminal_startup_launch_payload_source
            .insert_explicit_payload_for_startup_key(
                runtime_session_id,
                session_id,
                AgentsTerminalStartupBodySlotId {
                    pane_id,
                    session_id,
                },
                payload,
            );
        self.active_mode = TitlebarMode::Agents;
        self.set_shell_focus_with_terminal_handoff(ShellFocusTarget::AgentsPane(pane_id), true);
        self.scroll_workspace_pane_active_tab(pane_id);
        self.persist_shell_layout_state();
        cx.notify();
    }

    pub(crate) fn open_gpui_command_action_terminal(
        &mut self,
        command_id: String,
        title: String,
        command: String,
        play_completion_sound: bool,
        close_terminal_on_exit: bool,
        window: &Window,
        cx: &mut gpui::Context<Self>,
    ) {
        /*
        CDXC:GPUITitlebarActions 2026-06-24-14:24:
        Terminal Actions must create a real command-pane terminal startup path, not run a process from the titlebar. Insert command text only as an explicit launch payload for the newly selected command-pane body slot, use the active project snapshot path only when the sidebar supplied it, and keep run-state success/error feedback tied to the command Action lifecycle rather than titlebar-side command execution.

        CDXC:GPUICommandPane 2026-06-24-23:17:
        Sidebar/titlebar terminal Actions should mirror macOS command-pane startup by using Ghostty's launch command field for the wrapped zsh action process instead of pasting command text as visible initial input. The payload remains process-local and exact-slot keyed; command text is not logged, persisted, inferred from labels, or stored in shell-state JSON.

        CDXC:GPUICommandPane 2026-06-24-23:36:
        Re-running an action should reuse a matching idle command-pane tab instead of multiplying tabs. New or inactive reused tabs receive the wrapped command through the launch-payload boundary; an already mounted reused tab receives the same wrapper through the exact mounted command surface, with status reset driven by the session-state file.

        CDXC:GPUICommandPane 2026-06-24-23:49:
        GPUI command Actions now mirror macOS sidebar button feedback: post `running` for the selected run id immediately, then let the status-file poller post success/error and play the configured action completion sound when the wrapped command exits. The feedback path carries only command id, run id, state, exit code, and sound preference.

        CDXC:GPUICommandPane 2026-06-25-11:47:
        Command Actions open the hidden command pane through the same default-height rule as macOS sidebar Actions. Reset height only when the pane was hidden before selecting or creating the Action-owned tab; visible panes keep their live resize while the run metadata and launch payload update.

        CDXC:GPUICommandPaneActions 2026-06-26-04:59:
        Command-pane Action runs ignore the saved/requested close-on-exit flag at run start so the selected Action tab remains reusable after completion, matching native `runNativeSidebarCommand`. The parsed flag must not enter launch payloads, status files, shell-state JSON, logs, command text, cwd/env, terminal output, or project paths.

        CDXC:GPUICommandPaneActions 2026-06-27-01:45:
        Default terminal Actions select and reveal their command tab but keep the current shell first responder, matching native `focusAfterCreate: false`. Only explicit command-pane focus routes and Debug Actions may transfer typing focus.

        CDXC:GPUICommandPaneActions 2026-06-27-02:05:
        After Action run-start metadata is installed and sidebar run-state feedback is posted, GPUI must immediately refresh the cached sanitized `commandPaneSessions` bridge like native `runNativeSidebarCommand.publish()`. The bridge may carry only session ids, active/focus booleans, sanitized titles, semantic statuses, sleeping/timer fields, and sanitized action command ids; command text, cwd/env, run ids, status-file paths, terminal output, persisted shell data, and project paths must stay out.

        CDXC:GPUICommandPaneActions 2026-06-27-07:54:
        Default terminal Action execution is mutually exclusive like native: mounted idle reuse writes the staged wrapper to the exact current command surface and submits Return without enqueueing startup data, while created or unmounted Action tabs receive an exact-slot launch payload for first mount. Do not use a launch payload as fallback for a mounted reused shell.
        */
        self.prepare_hidden_command_pane_open_height_from_shared_settings(window);
        let selection = self
            .command_pane
            .select_or_create_action_session(command_id.clone(), title.clone());
        let group_id = selection.group_id;
        let session_id = selection.session_id;
        if matches!(
            selection.kind,
            CommandPaneActionSessionSelectionKind::ReusedActive
        ) {
            /*
            CDXC:GPUICommandPaneActions 2026-08-09:
            A live same-Action command tab is already the requested command
            pane. Select and reveal it without allocating another owner or
            writing a second command into the process that is still running.
            */
            self.refresh_sidebar_command_pane_sessions_if_changed(cx);
            self.scroll_command_group_active_tab(group_id);
            self.scroll_focused_command_active_tab();
            self.persist_shell_layout_state();
            cx.notify();
            return;
        }
        let slot_id = CommandTerminalBodyMountSlotId {
            group_id,
            session_id,
        };
        let run_id = create_gpui_command_action_run_id();
        let status_file_path = gpui_command_action_status_file_path(session_id);
        let delayed_send_cleared = self.clear_gpui_command_delayed_send_timer(session_id);
        let action_started = self.command_pane.mark_action_session_run_started(
            session_id,
            command_id.clone(),
            title,
            run_id.clone(),
            status_file_path.clone(),
            play_completion_sound,
            close_terminal_on_exit,
        );
        if delayed_send_cleared || action_started {
            self.sync_gpui_keep_awake_automation_from_current_settings(cx);
        }
        self.refresh_gpui_command_close_after_done_timer_for_session(session_id, cx);
        self.dispatch_gpui_sidebar_command_run_state(
            &command_id,
            &run_id,
            GpuiSidebarCommandRunState::Running,
            cx,
        );
        self.refresh_sidebar_command_pane_sessions_if_changed(cx);
        let execution_text = gpui_command_action_execution_text_for_current_backend(
            &command,
            &run_id,
            &status_file_path,
        );
        let mounted_reuse_surface_available = matches!(
            selection.kind,
            CommandPaneActionSessionSelectionKind::Reused
        ) && self
            .gpui_command_action_mounted_reuse_surface_available(slot_id);
        let wrote_to_mounted_reuse = mounted_reuse_surface_available
            && self.send_gpui_command_action_script_to_mounted_terminal(
                slot_id,
                &execution_text,
                &status_file_path,
                cx,
            );
        if gpui_command_action_should_insert_launch_payload(
            selection.kind,
            mounted_reuse_surface_available,
            wrote_to_mounted_reuse,
        ) {
            let action_title = self
                .command_pane
                .session(session_id)
                .map(|session| session.title.clone())
                .unwrap_or_else(|| COMMAND_PANE_DEFAULT_SESSION_TITLE.to_string());
            let startup_text = gpui_command_action_startup_text(&execution_text, &status_file_path);
            self.start_command_terminal_gxserver_attach_for_slot(
                slot_id,
                action_title.clone(),
                Some(startup_text),
                Some(command_id.clone()),
                Some(action_title),
                cx,
            );
        }
        if gpui_command_pane_default_action_should_focus_command_pane() {
            self.focus_command_pane();
            self.request_command_terminal_text_focus_handoff(slot_id);
        }
        self.scroll_command_group_active_tab(group_id);
        self.scroll_focused_command_active_tab();
        self.persist_shell_layout_state();
        cx.notify();
    }

    pub(crate) fn active_open_target_index(&self, targets: &[GpuiOpenTarget]) -> Option<usize> {
        self.active_open_target_id
            .as_deref()
            .and_then(|active_id| targets.iter().position(|target| target.id == active_id))
            .or_else(|| (!targets.is_empty()).then_some(0))
    }

    pub(crate) fn titlebar_open_target_icon(&self) -> (&'static str, f32) {
        let targets = gpui_visible_open_targets_from_current_settings();
        let active_target_id = self
            .active_open_target_index(&targets)
            .and_then(|index| targets.get(index))
            .map(|target| target.id.as_str());
        active_target_id
            .map(titlebar_open_target_icon_for_id)
            .unwrap_or((TITLEBAR_ICON_FOLDER_OPEN, 16.0))
    }

    pub(crate) fn active_project_open_in_path(&self) -> Option<PathBuf> {
        self.latest_sidebar_project_snapshot
            .as_ref()
            .and_then(|snapshot| snapshot.in_memory_project_path.clone())
    }

    pub(crate) fn open_active_project_with_active_open_target(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let targets = gpui_visible_open_targets_from_current_settings();
        let Some(target_index) = self.active_open_target_index(&targets) else {
            window.push_notification(Notification::warning("No Open In targets are visible."), cx);
            cx.notify();
            return;
        };
        self.open_active_project_with_open_target(target_index, targets, window, cx);
    }

    pub(crate) fn open_active_project_with_open_target_index(
        &mut self,
        target_index: usize,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.open_active_project_with_open_target(
            target_index,
            gpui_visible_open_targets_from_current_settings(),
            window,
            cx,
        );
    }

    pub(crate) fn open_active_project_with_open_target(
        &mut self,
        target_index: usize,
        targets: Vec<GpuiOpenTarget>,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(target) = targets.into_iter().nth(target_index) else {
            return;
        };
        let Some(project_path) = self.active_project_open_in_path() else {
            window.push_notification(
                Notification::warning("Open an active project before using Open In."),
                cx,
            );
            cx.notify();
            return;
        };
        self.persist_gpui_titlebar_project_selection(
            GPUI_TITLEBAR_OPEN_TARGET_SELECTIONS_SETTINGS_KEY,
            &target.id,
        );
        self.active_open_target_id = Some(target.id.clone());
        if let Err(message) = gpui_launch_open_target(&target, &project_path) {
            window.push_notification(Notification::warning(message), cx);
        }
        cx.notify();
    }

    pub(crate) fn titlebar_exit_focus_control_signature(
        &self,
    ) -> Option<GpuiTitlebarExitFocusControlSignature> {
        gpui_titlebar_exit_focus_control_signature(self.agents_workspace.focus_mode_pane.is_some())
    }

    pub(crate) fn exit_titlebar_focus_mode(&mut self, cx: &mut gpui::Context<Self>) -> bool {
        if self.agents_workspace.focus_mode_pane.is_none()
            || !self.agents_workspace.toggle_focus_mode()
        {
            return false;
        }

        if self.active_mode == TitlebarMode::Agents {
            let focused_pane = self.agents_workspace.focused_pane;
            self.set_shell_focus(ShellFocusTarget::AgentsPane(focused_pane));
            self.scroll_workspace_pane_active_tab(focused_pane);
        }
        self.persist_shell_layout_state();
        cx.notify();
        true
    }

    pub(crate) fn render_right_titlebar_controls(
        &self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        /*
        CDXC:GPUIWindowsTitlebarButtons 2026-07-26:
        These controls are exact normal-layout children of the draggable
        titlebar. On Windows each interactive frame must occlude the ancestor
        Drag hitbox so WM_NCHITTEST leaves that rectangle in the client area
        and GPUI delivers its normal mouse handlers. This is button-local
        ownership, not an overlay or synthetic event route.
        */
        let active_action = self.active_gpui_titlebar_action();
        let actions_icon_path = titlebar_action_icon_path(active_action.as_ref());
        /*
        Quick Actions is a discoverable titlebar control on desktop, including
        before the first Action has been configured. Keep it visible on Windows
        as it is on macOS so its empty-state click can open Settings > Actions;
        Linux retains its existing configured-action-only behavior.
        */
        let show_actions_button =
            cfg!(any(target_os = "macos", target_os = "windows")) || active_action.is_some();
        let settings = shared_settings::shared_sidebar_settings_snapshot();
        let button_hidden = |key: &str| {
            settings
                .object()
                .get(key)
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
        };
        let controls = h_flex()
            .absolute()
            .right_0()
            .top(px(1.0))
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .items_center()
            .map(|this| {
                // Prompt Editor and Exit Focus share the same titlebar slot;
                // when both are eligible only Prompt Editor renders.
                if self.prompt_editor_daemon_open {
                    return this.child(self.render_titlebar_prompt_editor_button(cx));
                }
                if let Some(signature) = self.titlebar_exit_focus_control_signature() {
                    return this.child(self.render_titlebar_exit_focus_button(signature, cx));
                }
                this
            })
            .when(
                !button_hidden(TIPS_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY),
                |this| {
                    this.child(self.render_titlebar_native_popup_button(
                        GpuiTitlebarPopupKind::Tips,
                        TITLEBAR_ICON_INFO,
                        TITLEBAR_TIPS_TOOLTIP,
                        self.titlebar_tips_badge_count() > 0,
                        window,
                        cx,
                    ))
                },
            )
            .when(
                !button_hidden(RESOURCES_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY),
                |this| {
                    this.child(self.render_titlebar_native_popup_button(
                        GpuiTitlebarPopupKind::Resources,
                        TITLEBAR_ICON_DEVICE_DESKTOP,
                        TITLEBAR_RESOURCES_TOOLTIP,
                        false,
                        window,
                        cx,
                    ))
                },
            )
            .when(
                !button_hidden(GIT_ACTIONS_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY),
                |this| this.child(self.render_titlebar_git_button(window, cx)),
            )
            .when(
                show_actions_button
                    && !button_hidden(QUICK_ACTIONS_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY),
                |this| {
                    this.child(self.render_titlebar_actions_button(actions_icon_path, window, cx))
                },
            )
            .when(
                !button_hidden(OPEN_IN_TITLEBAR_BUTTON_HIDDEN_SETTINGS_KEY),
                |this| this.child(self.render_titlebar_open_targets_button(window, cx)),
            );
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        let controls = controls
            .child(
                div()
                    .id("ghostex-gpui-titlebar-window-controls-gap")
                    .h_full()
                    .w(px(TITLEBAR_BUTTON_WIDTH))
                    .window_control_area(WindowControlArea::Drag),
            )
            .child(self.render_titlebar_window_controls(window, cx));
        controls
    }

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    pub(crate) fn render_titlebar_window_controls(
        &self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        /*
        Windows and Linux use the same flat, contiguous titlebar button chrome
        as the existing Ghostex actions, but caption controls keep the native
        Windows 46px width. They are normal trailing layout children, so they
        neither overlap the draggable titlebar nor need synthetic hit routing.
        */
        let maximize_control = if window.is_maximized() {
            GpuiWindowCaptionControl::Restore
        } else {
            GpuiWindowCaptionControl::Maximize
        };
        h_flex()
            .id("ghostex-gpui-titlebar-window-controls")
            .h_full()
            .items_center()
            .child(self.render_titlebar_window_control(GpuiWindowCaptionControl::Minimize, cx))
            .child(self.render_titlebar_window_control(maximize_control, cx))
            .child(self.render_titlebar_window_control(GpuiWindowCaptionControl::Close, cx))
    }

    #[cfg(any(target_os = "windows", target_os = "linux"))]
    pub(crate) fn render_titlebar_window_control(
        &self,
        control: GpuiWindowCaptionControl,
        _cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let button = div()
            .id(control.element_id())
            .relative()
            .flex()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .w(px(TITLEBAR_WINDOW_BUTTON_WIDTH))
            .items_center()
            .justify_center()
            .occlude()
            .border_l_1()
            .border_color(titlebar_button_border_color())
            .text_color(titlebar_icon_color())
            .cursor_default()
            .hover(|this| {
                this.bg(titlebar_button_hover_color())
                    .text_color(titlebar_icon_hover_color())
            })
            .child(titlebar_svg_icon(
                control.icon_path(),
                control.icon_size(),
                titlebar_icon_color(),
            ));

        #[cfg(target_os = "windows")]
        {
            button
                .window_control_area(control.window_control_area())
                .into_any_element()
        }

        #[cfg(target_os = "linux")]
        {
            button
                .on_mouse_down(
                    MouseButton::Left,
                    _cx.listener(move |_this, _event, window, cx| {
                        window.prevent_default();
                        cx.stop_propagation();
                        match control {
                            GpuiWindowCaptionControl::Minimize => window.minimize_window(),
                            GpuiWindowCaptionControl::Maximize
                            | GpuiWindowCaptionControl::Restore => window.zoom_window(),
                            GpuiWindowCaptionControl::Close => window.remove_window(),
                        }
                    }),
                )
                .into_any_element()
        }
    }

    pub(crate) fn render_titlebar_native_popup_button(
        &self,
        kind: GpuiTitlebarPopupKind,
        icon_path: &'static str,
        tooltip: &'static str,
        show_badge: bool,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let open = self.titlebar_popup_menu_open(kind);
        let icon_color = if open {
            titlebar_icon_hover_color()
        } else {
            titlebar_icon_color()
        };
        let anchor_key = match kind {
            GpuiTitlebarPopupKind::Tips => "ghostex-gpui-titlebar-tips-popup-anchor",
            GpuiTitlebarPopupKind::Resources => "ghostex-gpui-titlebar-resources-popup-anchor",
            _ => "ghostex-gpui-titlebar-native-popup-anchor",
        };
        let anchor_state = window.use_keyed_state(anchor_key, cx, |_, _| {
            GpuiTitlebarPopupAnchorState::default()
        });
        let anchor_bounds = anchor_state.read(cx).bounds;
        let trigger_bounds = anchor_state
            .read(cx)
            .trigger_bounds_captured
            .then_some(anchor_bounds);

        div()
            .id(match kind {
                GpuiTitlebarPopupKind::Tips => "ghostex-gpui-titlebar-button-tips-native",
                GpuiTitlebarPopupKind::Resources => "ghostex-gpui-titlebar-button-resources-native",
                _ => "ghostex-gpui-titlebar-button-native-popup",
            })
            .relative()
            .flex()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .w(px(TITLEBAR_BUTTON_WIDTH))
            .items_center()
            .justify_center()
            .when(cfg!(target_os = "windows"), |this| this.occlude())
            .border_l_1()
            .border_color(titlebar_button_border_color())
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
                        kind,
                        "left",
                        "togglePopup",
                        open,
                        trigger_bounds,
                        event,
                        window,
                    );
                    this.set_gpui_titlebar_popup_open(kind, !open, trigger_bounds, window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    log_gpui_titlebar_popup_mouse_down(
                        kind,
                        "right",
                        "togglePopup",
                        open,
                        trigger_bounds,
                        event,
                        window,
                    );
                    this.set_gpui_titlebar_popup_open(kind, !open, trigger_bounds, window, cx);
                }),
            )
            .when(!open, |this| {
                this.managed_discrete_tooltip_with_placement(
                    ManagedTooltipPlacement::Left,
                    Duration::from_millis(300),
                    move |window, cx| titlebar_tooltip(tooltip, window, cx),
                )
            })
            .on_prepaint({
                let anchor_state = anchor_state.clone();
                move |bounds, window, cx| {
                    let (first_capture, moved) = anchor_state.update(cx, |state, _| {
                        let first_capture = !state.trigger_bounds_captured;
                        let moved = state.bounds != bounds;
                        state.bounds = bounds;
                        state.trigger_bounds_captured = true;
                        (first_capture, moved)
                    });
                    if first_capture || moved {
                        log_gpui_titlebar_popup_anchor(kind, bounds, first_capture, moved, window);
                        window.request_animation_frame();
                    }
                }
            })
            .child(titlebar_svg_icon(icon_path, 16.0, icon_color))
            .when(show_badge, |this| {
                this.child(
                    div()
                        .absolute()
                        .right(px(8.0))
                        .top(px(5.0))
                        .size(px(7.5))
                        .rounded_full()
                        .border_1()
                        .border_color(titlebar_background())
                        .bg(rgb(0x95d7f6)),
                )
            })
            .into_any_element()
    }

    pub(crate) fn render_titlebar_actions_button(
        &self,
        icon_path: &'static str,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let open = self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::Actions);
        let icon_color = if open {
            titlebar_icon_hover_color()
        } else {
            titlebar_icon_color()
        };
        let anchor_state =
            window.use_keyed_state("ghostex-gpui-titlebar-actions-popup-anchor", cx, |_, _| {
                GpuiTitlebarPopupAnchorState::default()
            });
        let anchor_bounds = anchor_state.read(cx).bounds;
        let trigger_bounds_captured = anchor_state.read(cx).trigger_bounds_captured;
        let trigger_bounds = trigger_bounds_captured.then_some(anchor_bounds);

        div()
            .id("ghostex-gpui-titlebar-button-actions")
            .relative()
            .flex()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .w(px(TITLEBAR_BUTTON_WIDTH))
            .items_center()
            .justify_center()
            .when(cfg!(target_os = "windows"), |this| this.occlude())
            .border_l_1()
            .border_color(titlebar_button_border_color())
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
                        GpuiTitlebarPopupKind::Actions,
                        "left",
                        "runPrimaryAction",
                        open,
                        trigger_bounds,
                        event,
                        window,
                    );
                    this.run_active_gpui_titlebar_action(window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    log_gpui_titlebar_popup_mouse_down(
                        GpuiTitlebarPopupKind::Actions,
                        "right",
                        "togglePopup",
                        open,
                        trigger_bounds,
                        event,
                        window,
                    );
                    this.set_gpui_titlebar_popup_open(
                        GpuiTitlebarPopupKind::Actions,
                        !open,
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
                    |window, cx| titlebar_tooltip(TITLEBAR_ACTIONS_TOOLTIP, window, cx),
                )
            })
            .on_prepaint({
                let anchor_state = anchor_state.clone();
                move |bounds, window, cx| {
                    let (first_capture, moved) = anchor_state.update(cx, |state, _| {
                        let first_capture = !state.trigger_bounds_captured;
                        let moved = state.bounds != bounds;
                        state.bounds = bounds;
                        state.trigger_bounds_captured = true;
                        (first_capture, moved)
                    });
                    if first_capture || moved {
                        log_gpui_titlebar_popup_anchor(
                            GpuiTitlebarPopupKind::Actions,
                            bounds,
                            first_capture,
                            moved,
                            window,
                        );
                        window.request_animation_frame();
                    }
                }
            })
            .child(titlebar_svg_icon(icon_path, 16.0, icon_color))
            .into_any_element()
    }

    pub(crate) fn render_titlebar_open_targets_button(
        &self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let (icon_path, _icon_size) = self.titlebar_open_target_icon();
        let open = self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::OpenTargets);
        let icon_color = if open {
            titlebar_icon_hover_color()
        } else {
            titlebar_icon_color()
        };
        let anchor_state = window.use_keyed_state(
            "ghostex-gpui-titlebar-open-targets-popup-anchor",
            cx,
            |_, _| GpuiTitlebarPopupAnchorState::default(),
        );
        let anchor_bounds = anchor_state.read(cx).bounds;
        let trigger_bounds_captured = anchor_state.read(cx).trigger_bounds_captured;
        let trigger_bounds = trigger_bounds_captured.then_some(anchor_bounds);

        div()
            .id("ghostex-gpui-titlebar-button-open-project")
            .relative()
            .flex()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .w(px(TITLEBAR_BUTTON_WIDTH))
            .items_center()
            .justify_center()
            .when(cfg!(target_os = "windows"), |this| this.occlude())
            .border_l_1()
            .when(
                cfg!(any(target_os = "windows", target_os = "linux")),
                |this| this.border_r_1(),
            )
            .border_color(titlebar_button_border_color())
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
                        GpuiTitlebarPopupKind::OpenTargets,
                        "left",
                        "openPrimaryTarget",
                        open,
                        trigger_bounds,
                        event,
                        window,
                    );
                    this.open_active_project_with_active_open_target(window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    log_gpui_titlebar_popup_mouse_down(
                        GpuiTitlebarPopupKind::OpenTargets,
                        "right",
                        "togglePopup",
                        open,
                        trigger_bounds,
                        event,
                        window,
                    );
                    this.set_gpui_titlebar_popup_open(
                        GpuiTitlebarPopupKind::OpenTargets,
                        !open,
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
                    |window, cx| titlebar_tooltip(TITLEBAR_OPEN_TARGETS_TOOLTIP, window, cx),
                )
            })
            .on_prepaint({
                let anchor_state = anchor_state.clone();
                move |bounds, window, cx| {
                    let (first_capture, moved) = anchor_state.update(cx, |state, _| {
                        let first_capture = !state.trigger_bounds_captured;
                        let moved = state.bounds != bounds;
                        state.bounds = bounds;
                        state.trigger_bounds_captured = true;
                        (first_capture, moved)
                    });
                    if first_capture || moved {
                        log_gpui_titlebar_popup_anchor(
                            GpuiTitlebarPopupKind::OpenTargets,
                            bounds,
                            first_capture,
                            moved,
                            window,
                        );
                        window.request_animation_frame();
                    }
                }
            })
            .child(titlebar_svg_icon(icon_path, 12.0, icon_color))
            .into_any_element()
    }

    pub(crate) fn titlebar_open_targets_popup_content_height(&self) -> f32 {
        let target_count = gpui_visible_open_targets_from_current_settings().len();
        let mut rows = vec![TITLEBAR_POPUP_MENU_ROW_HEIGHT; target_count];
        if target_count > 0 {
            rows.push(TITLEBAR_POPUP_MENU_SEPARATOR_HEIGHT);
        }
        rows.push(TITLEBAR_POPUP_MENU_ROW_HEIGHT);
        titlebar_popup_menu_height_for_rows_with_chrome(
            &rows,
            TITLEBAR_POPUP_MENU_FLUSH_BOTTOM_VERTICAL_CHROME,
        )
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
        titlebar_popup_menu_height_for_rows_with_chrome(
            &rows,
            TITLEBAR_POPUP_MENU_FLUSH_BOTTOM_VERTICAL_CHROME,
        )
    }

    pub(crate) fn titlebar_git_popup_content_height(&self) -> f32 {
        let Some(state) = self.titlebar_git_menu_state.as_ref() else {
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
            GpuiTitlebarPopupKind::Actions => self.titlebar_actions_popup_content_height(),
            GpuiTitlebarPopupKind::Git => self.titlebar_git_popup_content_height(),
            GpuiTitlebarPopupKind::OpenTargets => self.titlebar_open_targets_popup_content_height(),
            GpuiTitlebarPopupKind::Resources | GpuiTitlebarPopupKind::Tips => {
                TITLEBAR_POPUP_READING_MENU_MAX_HEIGHT
            }
        }
    }

    /// Native equivalent of the shared React titlebar update affordance
    /// (titlebar-host.tsx `updateAvailable`/`updateDownloading` +
    /// `TitlebarUpdateProgressRing`): renders only while an update is
    /// available or downloading, shows a download icon at rest and a circular
    /// progress ring during the platform download, and disables clicks while
    /// downloading.
    pub(crate) fn render_titlebar_update_button(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        let downloading = self.update_downloading;
        let progress = self.update_download_progress;
        let update_tooltip: gpui::SharedString = if downloading {
            match progress {
                Some(progress) => format!(
                    "Downloading... {}%",
                    (progress * 100.0).round().clamp(0.0, 100.0) as u8
                )
                .into(),
                None => "Downloading...".into(),
            }
        } else {
            TITLEBAR_UPDATE_AVAILABLE_TOOLTIP.into()
        };
        div()
            .id("ghostex-gpui-titlebar-button-update")
            .relative()
            .flex()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .rounded(px(5.0))
            .w(px(29.0))
            .ml(px(3.0))
            .mr(px(7.0))
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .text_color(if downloading {
                titlebar_update_downloading_color()
            } else {
                titlebar_update_available_color()
            })
            .cursor_default()
            .when(!downloading, |this| {
                this.hover(|this| {
                    this.bg(titlebar_button_hover_color())
                        .text_color(titlebar_update_available_color())
                })
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.check_for_gpui_updates(window, cx);
                }),
            )
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Right, move |window, cx| {
                titlebar_tooltip(update_tooltip.clone(), window, cx)
            })
            .map(|this| {
                if downloading {
                    let ring = canvas(
                        move |_bounds, _window, _cx| {},
                        move |bounds, _state: (), window, _cx| {
                            paint_titlebar_update_progress_ring(bounds, progress, window);
                        },
                    )
                    .size(px(TITLEBAR_UPDATE_PROGRESS_RING_SIZE))
                    .ml(px(1.0))
                    .mt(px(1.5));
                    if progress.is_none() {
                        this.child(ring.with_animation(
                            "ghostex-gpui-titlebar-update-progress-pending",
                            Animation::new(Duration::from_millis(1_250)).repeat(),
                            |ring, _delta| ring,
                        ))
                    } else {
                        this.child(ring)
                    }
                } else {
                    this.child(
                        svg()
                            .size(px(TITLEBAR_UPDATE_ICON_SIZE))
                            .ml(px(1.0))
                            .mt(px(0.5))
                            .path(TITLEBAR_ICON_DOWNLOAD)
                            .text_color(titlebar_update_available_color())
                            .hover(|this| this.text_color(titlebar_update_available_color())),
                    )
                }
            })
            .into_any_element()
    }

    /// Bring-the-open-standalone-editor-forward affordance. Occupies the
    /// Exit Focus slot with the same text-button chrome, but stays in the
    /// resting (non-active-tab) skin because it does not represent a mode
    /// the workspace is currently in.
    pub(crate) fn render_titlebar_prompt_editor_button(&self, cx: &mut gpui::Context<Self>) -> AnyElement {
        div()
            .id("ghostex-gpui-titlebar-prompt-editor")
            .relative()
            .flex()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .min_w(px(70.0))
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .gap(px(7.0))
            .when(cfg!(target_os = "windows"), |this| this.occlude())
            .border_l_1()
            .border_color(titlebar_button_border_color())
            .px(px(14.0))
            .text_size(px(13.55))
            .font_weight(FontWeight::NORMAL)
            .line_height(px(TITLEBAR_CONTROL_HEIGHT))
            .text_color(titlebar_icon_color())
            .cursor_default()
            .hover(|this| {
                this.bg(titlebar_button_hover_color())
                    .text_color(titlebar_icon_hover_color())
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    cx.background_executor()
                        .spawn(async move { gpui_ghostex_editor_daemon_bring_to_front() })
                        .detach();
                }),
            )
            .child(div().size(px(6.0)).rounded_full().bg(rgb(0x95d7f6)))
            .child("Prompt Editor")
            .into_any_element()
    }

    pub(crate) fn render_titlebar_exit_focus_button(
        &self,
        signature: GpuiTitlebarExitFocusControlSignature,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let styled_as_active_mode_tab = signature.styled_as_active_mode_tab;
        let clears_agents_focus_mode = signature.clears_agents_focus_mode;
        div()
            .id("ghostex-gpui-titlebar-exit-focus")
            .relative()
            .flex()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .min_w(px(70.0))
            .flex_shrink_0()
            .items_center()
            .justify_center()
            .when(cfg!(target_os = "windows"), |this| this.occlude())
            .border_l_1()
            .border_color(titlebar_button_border_color())
            .px(px(14.0))
            .text_size(px(13.55))
            .font_weight(FontWeight::NORMAL)
            .line_height(px(TITLEBAR_CONTROL_HEIGHT))
            .text_color(titlebar_active_text_color())
            .cursor_default()
            .when(styled_as_active_mode_tab, |this| {
                this.bg(titlebar_active_segment_color())
            })
            .hover(move |this| {
                if styled_as_active_mode_tab {
                    this.bg(titlebar_active_segment_color())
                        .text_color(titlebar_active_text_color())
                } else {
                    this
                }
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    if clears_agents_focus_mode {
                        this.exit_titlebar_focus_mode(cx);
                    }
                }),
            )
            .child(signature.label)
            .into_any_element()
    }

    pub(crate) fn render_titlebar_git_button(
        &self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let state = self.titlebar_git_menu_state.as_ref();
        let icon_path = state
            .map(|state| titlebar_git_action_icon_path(state.primary_action))
            .unwrap_or(TITLEBAR_ICON_GIT_COMMIT);
        let is_busy = state.is_some_and(|state| state.is_busy);
        let open = self.titlebar_popup_menu_open(GpuiTitlebarPopupKind::Git);
        let icon_color = if open {
            titlebar_icon_hover_color()
        } else {
            titlebar_icon_color()
        };
        let anchor_state =
            window.use_keyed_state("ghostex-gpui-titlebar-git-popup-anchor", cx, |_, _| {
                GpuiTitlebarPopupAnchorState::default()
            });
        let anchor_bounds = anchor_state.read(cx).bounds;
        let trigger_bounds_captured = anchor_state.read(cx).trigger_bounds_captured;
        let trigger_bounds = trigger_bounds_captured.then_some(anchor_bounds);

        div()
            .id("ghostex-gpui-titlebar-button-git")
            .relative()
            .flex()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .w(px(TITLEBAR_BUTTON_WIDTH))
            .items_center()
            .justify_center()
            .when(cfg!(target_os = "windows"), |this| this.occlude())
            .border_l_1()
            .border_color(titlebar_button_border_color())
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
                        GpuiTitlebarPopupKind::Git,
                        "left",
                        "togglePopup",
                        open,
                        trigger_bounds,
                        event,
                        window,
                    );
                    this.show_gpui_titlebar_git_menu(trigger_bounds, window, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    log_gpui_titlebar_popup_mouse_down(
                        GpuiTitlebarPopupKind::Git,
                        "right",
                        "togglePopup",
                        open,
                        trigger_bounds,
                        event,
                        window,
                    );
                    this.show_gpui_titlebar_git_menu(trigger_bounds, window, cx);
                }),
            )
            .when(!open, |this| {
                this.managed_discrete_tooltip_with_placement(
                    ManagedTooltipPlacement::Left,
                    Duration::from_millis(300),
                    |window, cx| titlebar_tooltip(TITLEBAR_GIT_TOOLTIP, window, cx),
                )
            })
            .on_prepaint({
                let anchor_state = anchor_state.clone();
                move |bounds, window, cx| {
                    let (first_capture, moved) = anchor_state.update(cx, |state, _| {
                        let first_capture = !state.trigger_bounds_captured;
                        let moved = state.bounds != bounds;
                        state.bounds = bounds;
                        state.trigger_bounds_captured = true;
                        (first_capture, moved)
                    });
                    if first_capture || moved {
                        log_gpui_titlebar_popup_anchor(
                            GpuiTitlebarPopupKind::Git,
                            bounds,
                            first_capture,
                            moved,
                            window,
                        );
                        window.request_animation_frame();
                    }
                }
            })
            .map(|this| {
                if is_busy {
                    this.child(
                        canvas(
                            move |_bounds, _window, _cx| {},
                            move |bounds, _state: (), window, _cx| {
                                paint_titlebar_git_busy_spinner(bounds, window);
                            },
                        )
                        .size(px(15.0)),
                    )
                } else {
                    this.child(titlebar_svg_icon(icon_path, 15.0, icon_color))
                }
            })
            .into_any_element()
    }

    pub(crate) fn render_titlebar_anchored_dropdown_panel(
        &self,
        id: &'static str,
        width: f32,
        open: bool,
        position: Point<Pixels>,
        trigger_bounds: Bounds<Pixels>,
        close: fn(&mut Self, &mut Window, &mut gpui::Context<Self>),
        child: impl IntoElement + 'static,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        if !open {
            return div().size_0().into_any_element();
        }

        deferred(
            anchored()
                .anchor(Anchor::TopRight)
                .position(position)
                .child(
                    div()
                        .id(id)
                        .occlude()
                        .tab_group()
                        .key_context(TITLEBAR_DROPDOWN_KEY_CONTEXT)
                        .track_focus(&self.titlebar_dropdown_focus_handle)
                        .w(px(width))
                        .h(px(TITLEBAR_DROPDOWN_READING_PANEL_HEIGHT))
                        .overflow_hidden()
                        .rounded(px(2.0))
                        .border_1()
                        .border_color(titlebar_popup_menu_border_color())
                        .bg(titlebar_popup_menu_background())
                        .on_action(cx.listener(
                            move |this, _: &TitlebarDropdownCancel, window, cx| {
                                close(this, window, cx);
                            },
                        ))
                        .on_mouse_down_out(cx.listener(
                            move |this, event: &MouseDownEvent, window, cx| {
                                // A mouse-down on the trigger button is the
                                // button's own toggle-close; closing here too
                                // made the toggle reopen the panel instead.
                                if trigger_bounds.contains(&event.position) {
                                    return;
                                }
                                close(this, window, cx);
                            },
                        ))
                        .child(child),
                ),
        )
        .with_priority(1)
        .into_any_element()
    }

    pub(crate) fn render_titlebar_tips_popover(
        &self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let state =
            window.use_keyed_state("ghostex-gpui-titlebar-tips-dropdown-anchor", cx, |_, _| {
                GpuiTitlebarAnchoredDropdownState::default()
            });
        let tips_open = self.titlebar_tips_panel_open;
        let panel = self.titlebar_tips_panel.clone();
        let anchor_position = state.read(cx).position;
        let trigger_bounds_captured = state.read(cx).trigger_bounds_captured;

        div()
            .id("ghostex-gpui-titlebar-tips-popover")
            .child(self.render_titlebar_tips_trigger().selected(tips_open))
            .when(!tips_open, |this| {
                this.managed_tooltip_with_placement(ManagedTooltipPlacement::Left, |window, cx| {
                    titlebar_tooltip(TITLEBAR_TIPS_TOOLTIP, window, cx)
                })
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.set_gpui_titlebar_tips_panel_open(!tips_open, window, cx);
                }),
            )
            .on_prepaint({
                let state = state.clone();
                move |bounds, window, cx| {
                    let trigger_right_x = bounds.top_right().x.as_f32();
                    let horizontal_margin = 8.0;
                    let min_right_edge = TITLEBAR_DROPDOWN_TIPS_PANEL_WIDTH + horizontal_margin;
                    let max_right_edge = (window.viewport_size().width.as_f32()
                        - horizontal_margin)
                        .max(min_right_edge);
                    let right_edge = trigger_right_x.clamp(min_right_edge, max_right_edge);
                    let next_position = point(px(right_edge), px(TITLEBAR_HEIGHT));
                    let request_frame = state.update(cx, |state, _| {
                        let first_capture = !state.trigger_bounds_captured;
                        let moved = state.position != next_position;
                        state.position = next_position;
                        state.trigger_bounds = bounds;
                        state.trigger_bounds_captured = true;
                        first_capture || moved
                    });
                    if request_frame {
                        window.request_animation_frame();
                    }
                }
            })
            .child(
                self.render_titlebar_anchored_dropdown_panel(
                    "ghostex-gpui-titlebar-tips-dropdown-panel",
                    TITLEBAR_DROPDOWN_TIPS_PANEL_WIDTH,
                    tips_open && trigger_bounds_captured,
                    anchor_position,
                    state.read(cx).trigger_bounds,
                    Self::close_gpui_titlebar_tips_dropdown,
                    div()
                        .size_full()
                        .when_some(panel, |this, panel| this.child(panel)),
                    cx,
                ),
            )
    }

    pub(crate) fn close_gpui_titlebar_tips_dropdown(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.set_gpui_titlebar_tips_panel_open(false, window, cx);
    }

    pub(crate) fn render_titlebar_resources_popover(
        &self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let state = window.use_keyed_state(
            "ghostex-gpui-titlebar-resources-dropdown-anchor",
            cx,
            |_, _| GpuiTitlebarAnchoredDropdownState::default(),
        );
        let resources_open = self.titlebar_resources_panel_open;
        let resources_ready = self.titlebar_resources_panel_ready;
        let panel = self.titlebar_resources_panel.clone();
        let anchor_position = state.read(cx).position;
        let trigger_bounds_captured = state.read(cx).trigger_bounds_captured;

        div()
            .id("ghostex-gpui-titlebar-resources-popover")
            .child(self.render_titlebar_icon_button(
                "resources",
                TITLEBAR_ICON_DEVICE_DESKTOP,
                16.0,
                false,
                cx,
            ))
            .when(!resources_open, |this| {
                this.managed_tooltip_with_placement(ManagedTooltipPlacement::Left, |window, cx| {
                    titlebar_tooltip(TITLEBAR_RESOURCES_TOOLTIP, window, cx)
                })
            })
            .on_prepaint({
                let state = state.clone();
                move |bounds, window, cx| {
                    let trigger_right_x = bounds.top_right().x.as_f32();
                    let horizontal_margin = 8.0;
                    let min_right_edge =
                        TITLEBAR_DROPDOWN_RESOURCES_PANEL_WIDTH + horizontal_margin;
                    let max_right_edge = (window.viewport_size().width.as_f32()
                        - horizontal_margin)
                        .max(min_right_edge);
                    let right_edge = trigger_right_x.clamp(min_right_edge, max_right_edge);
                    let next_position = point(px(right_edge), px(TITLEBAR_HEIGHT));
                    let request_frame = state.update(cx, |state, _| {
                        let first_capture = !state.trigger_bounds_captured;
                        let moved = state.position != next_position;
                        state.position = next_position;
                        state.trigger_bounds = bounds;
                        state.trigger_bounds_captured = true;
                        first_capture || moved
                    });
                    if request_frame {
                        window.request_animation_frame();
                    }
                }
            })
            .child(
                self.render_titlebar_anchored_dropdown_panel(
                    "ghostex-gpui-titlebar-resources-dropdown-panel",
                    TITLEBAR_DROPDOWN_RESOURCES_PANEL_WIDTH,
                    resources_open && trigger_bounds_captured,
                    anchor_position,
                    state.read(cx).trigger_bounds,
                    Self::close_gpui_titlebar_resources_dropdown,
                    div()
                        .relative()
                        .size_full()
                        .when_some(panel, |this, panel| this.child(panel))
                        .when(!resources_ready, |this| {
                            this.child(
                                div()
                                    .absolute()
                                    .top_0()
                                    .right_0()
                                    .bottom_0()
                                    .left_0()
                                    .child(Self::render_titlebar_resources_loading_skeleton()),
                            )
                        }),
                    cx,
                ),
            )
    }

    pub(crate) fn render_titlebar_resources_loading_skeleton() -> impl IntoElement {
        /*
        CDXC:GPUIResourcesInstantOpen 2026-07-11:
        CEF browser creation and the first Resources process sample must not
        delay the dropdown itself. GPUI owns this immediate placeholder in the
        same non-overlapping content frame; the hidden CEF child replaces it
        only after React reports ready.
        */
        let skeleton_fill: Hsla = rgb(0xffffff).opacity(0.08).into();
        let skeleton_border: Hsla = rgb(0xffffff).opacity(0.06).into();

        v_flex()
            .size_full()
            .p(px(14.0))
            .gap(px(12.0))
            .bg(titlebar_popup_menu_background())
            .child(
                h_flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .w(px(128.0))
                            .h(px(18.0))
                            .rounded(px(3.0))
                            .bg(skeleton_fill),
                    )
                    .child(
                        div()
                            .w(px(210.0))
                            .h(px(28.0))
                            .rounded(px(3.0))
                            .bg(skeleton_fill),
                    ),
            )
            .child(
                div()
                    .w(px(96.0))
                    .h(px(10.0))
                    .rounded(px(2.0))
                    .bg(skeleton_fill),
            )
            .children((0..5).map(move |_| {
                h_flex()
                    .items_center()
                    .gap(px(12.0))
                    .h(px(46.0))
                    .px(px(12.0))
                    .border_1()
                    .border_color(skeleton_border)
                    .child(div().size(px(28.0)).rounded(px(2.0)).bg(skeleton_fill))
                    .child(
                        v_flex()
                            .flex_1()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .w(px(190.0))
                                    .h(px(10.0))
                                    .rounded(px(2.0))
                                    .bg(skeleton_fill),
                            )
                            .child(
                                div()
                                    .w(px(126.0))
                                    .h(px(8.0))
                                    .rounded(px(2.0))
                                    .bg(skeleton_fill),
                            ),
                    )
                    .child(
                        div()
                            .w(px(84.0))
                            .h(px(24.0))
                            .rounded(px(2.0))
                            .bg(skeleton_fill),
                    )
                    .child(
                        div()
                            .w(px(92.0))
                            .h(px(24.0))
                            .rounded(px(2.0))
                            .bg(skeleton_fill),
                    )
            }))
    }

    pub(crate) fn close_gpui_titlebar_resources_dropdown(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) {
        self.set_gpui_titlebar_resources_panel_open(false, window, cx);
    }

    pub(crate) fn render_titlebar_tips_trigger(&self) -> GpuiTitlebarTipsTrigger {
        GpuiTitlebarTipsTrigger::new(self.titlebar_tips_badge_count() > 0)
    }

    pub(crate) fn titlebar_tips_badge_count(&self) -> u64 {
        /*
        CDXC:GPUITitlebarTipsBadge 2026-07-04-03:00:
        The GPUI strip stores only the last unread tip count sampled from the
        shared React titlebar panel's own localStorage key, plus notice facts
        Rust already owns through shared Settings. The titlebar must not keep a
        second read-id store, duplicate tip rows in UI, or infer notices from
        project/session labels.
        */
        let settings_snapshot = shared_settings::shared_sidebar_settings_snapshot();
        let notice_count = u64::from(settings_snapshot.debugging_mode())
            + u64::from(
                gpui_titlebar_session_persistence_provider_from_settings(
                    settings_snapshot.object(),
                ) == "off",
            );
        gpui_titlebar_tips_unread_count_from_settings().saturating_add(notice_count)
    }

    pub(crate) fn render_titlebar_icon_button(
        &self,
        id: &'static str,
        icon_path: &'static str,
        icon_size: f32,
        show_badge: bool,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        div()
            .id(format!("ghostex-gpui-titlebar-button-{id}"))
            .relative()
            .flex()
            .h(px(TITLEBAR_CONTROL_HEIGHT))
            .w(px(if id == "settings" {
                TITLEBAR_SETTINGS_BUTTON_WIDTH
            } else {
                TITLEBAR_BUTTON_WIDTH
            }))
            .items_center()
            .justify_center()
            .border_l_1()
            .border_color(titlebar_button_border_color())
            .text_color(titlebar_icon_color())
            .cursor_default()
            .hover(|this| {
                this.bg(titlebar_button_hover_color())
                    .text_color(titlebar_icon_hover_color())
            })
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    if id == "settings" {
                        /*
                        CDXC:GPUITitlebarAppModalHost 2026-06-24-11:09:
                        The GPUI titlebar Settings glyph owns the app-modal menu for Settings, Hotkeys, and Command Palette. Keep the menu as OS-owned NativeMenu actions that all route to the shared React modal host, rather than leaving Hotkeys or Command Palette without a titlebar path or adding GPUI-local placeholder UI.

                            CDXC:GPUIPreviousSessionsModal 2026-06-24-11:53:
                            The same Settings glyph menu owns Previous Sessions access so the GPUI titlebar opens the production shared modal and its gxserver bridge, not a separate GPUI-local history picker.

                            CDXC:GPUIDaemonSessionsModal 2026-06-24-12:00:
                            Running Sessions is a menu action into the shared app-modal host, not a titlebar overlay or duplicated GPUI inventory surface.
                            */
                        this.show_titlebar_settings_menu(event.position, window, cx);
                    } else if id == "keep-awake" {
                        /*
                        CDXC:GPUITitlebarKeepAwake 2026-06-24-13:16:
                        Keep Awake titlebar clicks open the OS-owned duration menu instead of toggling caffeinate directly. Runtime start/stop stays inside menu actions so users can choose the same shared duration semantics as macOS.
                        */
                        this.show_gpui_keep_awake_menu(event.position, window, cx);
                    } else if id == "resources" {
                        /*
                        CDXC:GPUIResourcesTitlebar 2026-07-08:
                        The visible Resources titlebar glyph opens the shared
                        React titlebar-host Resources panel in the app-owned
                        anchored dropdown. React owns live process polling while
                        Rust owns only the panel entity and native action bridge.
                        */
                        this.set_gpui_titlebar_resources_panel_open(
                            !this.titlebar_resources_panel_open,
                            window,
                            cx,
                        );
                    } else if id == "git" {
                        this.show_gpui_titlebar_git_menu(None, window, cx);
                    } else if id == "open-project" {
                        /*
                        CDXC:GPUITitlebarOpenIn 2026-06-24-12:50:
                        Left-click Open In launches the active runtime target for the explicit active project path only. If GPUI does not have `in_memory_project_path` from the sidebar contract, show private-data-free feedback and do not infer a path from display names, ids, labels, filesystem probing, or git metadata.
                        */
                        this.open_active_project_with_active_open_target(window, cx);
                    } else if id == "actions" {
                        /*
                        CDXC:GPUITitlebarActions 2026-06-24-14:24:
                        Left-click Actions runs the selected/last configured sidebar action when GPUI has one, otherwise the first configured action, and opens Settings > Actions when no configured action is available. Browser actions use the GPUI Browser CEF path, terminal actions use command-pane launch payloads, and this handler must not shell out, log private details, or create overlay UI.
                        */
                        this.run_active_gpui_titlebar_action(window, cx);
                    }
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, window, cx| {
                    if id == "keep-awake" {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.show_gpui_keep_awake_menu(event.position, window, cx);
                    } else if id == "open-project" {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.show_gpui_open_targets_menu(None, window, cx);
                    } else if id == "resources" {
                        /*
                        CDXC:GPUIResourcesTitlebar 2026-07-08:
                        Right-click uses the same React Resources dropdown as
                        primary click so the full resource controls share one
                        titlebar-host surface and one native action bridge.
                        */
                        window.prevent_default();
                        cx.stop_propagation();
                        this.set_gpui_titlebar_resources_panel_open(
                            !this.titlebar_resources_panel_open,
                            window,
                            cx,
                        );
                    } else if id == "git" {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.show_gpui_titlebar_git_menu(None, window, cx);
                    } else if id == "actions" {
                        window.prevent_default();
                        cx.stop_propagation();
                        this.show_gpui_titlebar_actions_menu(None, window, cx);
                    }
                }),
            )
            .child(titlebar_svg_icon(
                icon_path,
                icon_size,
                titlebar_icon_color(),
            ))
            .when(show_badge, |this| {
                this.child(
                    div()
                        .absolute()
                        .right(px(8.0))
                        .top(px(5.0))
                        .size(px(7.5))
                        .rounded_full()
                        .border_1()
                        .border_color(titlebar_background())
                        .bg(rgb(0x95d7f6)),
                )
            })
            .into_any_element()
    }

    pub(crate) fn render_browser_toolbar(
        &self,
        pane_id: BrowserPaneId,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        /*
        CDXC:GPUIBrowserToolbar 2026-06-14-17:42:
        The GPUI browser pane needs the same address toolbar as the macOS app, implemented only with GPUI chrome: a black row, stateless Back/Forward/Reload controls with no Back/Forward history toggles, a lock-or-globe address field that restores the current URL on empty commits, and the browser right-control group while preserving non-empty address commits inside the embedded CEF browser. Toolbar actions use the same full-height segmented button chrome as tab-bar actions.

        CDXC:GPUIBrowserToolbar 2026-06-15-01:52:
        GitHub disallows the injected feedback tool, so the GPUI toolbar must render the feedback button disabled on github.com pages and expose the tooltip "This site disallows using this tool" instead of letting the user start an unsupported tool action.

        CDXC:GPUIBrowserToolbar 2026-06-22-08:08:
        Browser Back and Forward controls must read their enabled state from the selected loaded tab's existing CEF surface and must no-op when that surface cannot navigate. Reload must call CEF `reload()` on the selected loaded surface instead of loading the shell URL again, so Chromium keeps ownership of history, POST/cache behavior, and address-only placeholder tabs remain unloaded.

        CDXC:GPUIBrowserToolbar 2026-06-22-11:50:
        The right-side Browser controls follow current macOS parity: zoom reset appears only when the active CEF surface is zoomed, the feedback button launches Agentation, History and Profile remain OS NativeMenus, DevTools toggles through the active CEF surface, and the removed Appearance control does not reserve toolbar space or hit area.

        CDXC:GPUIBrowserFeedback 2026-06-23-11:04:
        The Browser feedback toolbar starts Agentation through CEF main-frame JavaScript injection. Keep github.com and *.github.com disabled before injection, and keep the toolbar surface status private by showing only bounded page-data-free notifications for missing CEF surfaces or frames.
        */
        let address_value = self.browser_tabs.address_value_for_pane(pane_id);
        let feedback_tool_unavailable = browser_feedback_tool_unavailable_url(&address_value);
        let feedback_tooltip = feedback_tool_unavailable
            .then_some(BROWSER_FEEDBACK_TOOL_UNAVAILABLE_TOOLTIP)
            .unwrap_or(BROWSER_FEEDBACK_TOOL_AGENTATION_LABEL);
        let (is_loading, runtime_can_go_back, runtime_can_go_forward) = self
            .browser_tabs
            .active_tab_for_pane(pane_id)
            .map(|tab| {
                (
                    tab.runtime_is_loading,
                    tab.runtime_can_go_back,
                    tab.runtime_can_go_forward,
                )
            })
            .unwrap_or((false, false, false));
        let active_browser_surface = self.browser_surface_for_pane(pane_id);
        let can_go_back = active_browser_surface
            .as_ref()
            .is_some_and(|surface| runtime_can_go_back || surface.read(cx).can_go_back());
        let can_go_forward = active_browser_surface
            .as_ref()
            .is_some_and(|surface| runtime_can_go_forward || surface.read(cx).can_go_forward());
        let can_reload = active_browser_surface.is_some();
        let reload_action = if is_loading {
            BrowserToolbarAction::StopLoading
        } else {
            BrowserToolbarAction::Reload
        };
        let is_page_zoomed = active_browser_surface
            .as_ref()
            .is_some_and(|surface| surface.read(cx).is_zoomed());
        let zoom_reset_tooltip = active_browser_surface.as_ref().map(|surface| {
            gpui::SharedString::from(format!(
                "Reset Page Zoom ({}%)",
                (1.2_f64.powf(surface.read(cx).zoom_level()) * 100.0).round() as i32
            ))
        });
        let can_show_recent_history = !self
            .browser_tabs
            .pane_history_rows(pane_id, BROWSER_HISTORY_MENU_MAX_ROWS)
            .is_empty();
        /*
        CDXC:GPUIBrowserMediaPermissions 2026-07-27:
        A remembered Block would otherwise be unrecoverable: the page just
        fails and no prompt returns. Show a reset control exactly while the
        active tab's origin has a stored microphone/camera answer, so the site
        can be asked again.
        */
        let media_permission_reset_tooltip = self
            .browser_media_permission_reset_target(pane_id)
            .map(|(_, origin)| {
                gpui::SharedString::from(format!(
                    "Reset Microphone and Camera Access ({})",
                    gpui_browser_media_permission_display_origin(&origin)
                ))
            });
        h_flex()
            .id(format!("ghostex-gpui-browser-toolbar-{}", pane_id.0))
            .flex_shrink_0()
            .h(px(BROWSER_TOOLBAR_HEIGHT))
            .w_full()
            .items_center()
            .bg(browser_toolbar_background())
            .border_b_1()
            .border_color(rgb(0x252525))
            .child(
                h_flex()
                    .items_center()
                    .gap(px(BROWSER_TOOLBAR_ITEM_GAP))
                    .child(self.render_browser_toolbar_button(
                        "back",
                        TITLEBAR_ICON_CHEVRON_LEFT,
                        can_go_back,
                        None,
                        BrowserToolbarAction::Back,
                        pane_id,
                        cx,
                    ))
                    .child(self.render_browser_toolbar_button(
                        "forward",
                        BROWSER_ICON_CHEVRON_RIGHT,
                        can_go_forward,
                        None,
                        BrowserToolbarAction::Forward,
                        pane_id,
                        cx,
                    ))
                    .child(self.render_browser_toolbar_button(
                        "reload",
                        BROWSER_ICON_RELOAD,
                        can_reload,
                        None,
                        reload_action,
                        pane_id,
                        cx,
                    ))
                    .child(self.render_browser_toolbar_button(
                        "home",
                        BROWSER_ICON_HOME,
                        true,
                        None,
                        BrowserToolbarAction::Home,
                        pane_id,
                        cx,
                    )),
            )
            .child(div().flex_shrink_0().w(px(BROWSER_TOOLBAR_ADDRESS_GAP)))
            .child(self.render_browser_address_field(pane_id, cx))
            .child(
                div()
                    .flex_shrink_0()
                    .w(px(BROWSER_TOOLBAR_ADDRESS_RIGHT_GAP)),
            )
            .child(
                h_flex()
                    .items_center()
                    .gap(px(BROWSER_TOOLBAR_ITEM_GAP))
                    .child(self.render_browser_toolbar_new_tab_button(pane_id, cx))
                    .when(is_page_zoomed, |this| {
                        this.child(self.render_browser_toolbar_button(
                            "reset-zoom",
                            BROWSER_ICON_SEARCH,
                            true,
                            zoom_reset_tooltip,
                            BrowserToolbarAction::ResetZoom,
                            pane_id,
                            cx,
                        ))
                    })
                    .when_some(media_permission_reset_tooltip, |this, tooltip| {
                        this.child(self.render_browser_toolbar_button(
                            "reset-media-permissions",
                            BROWSER_ICON_MICROPHONE,
                            true,
                            Some(tooltip),
                            BrowserToolbarAction::ResetMediaPermissions,
                            pane_id,
                            cx,
                        ))
                    })
                    .child(self.render_browser_toolbar_button(
                        "agentation",
                        BROWSER_ICON_POINTER,
                        !feedback_tool_unavailable,
                        Some(feedback_tooltip.into()),
                        BrowserToolbarAction::FeedbackTool,
                        pane_id,
                        cx,
                    ))
                    .child(self.render_browser_toolbar_button(
                        "history",
                        BROWSER_ICON_HISTORY,
                        can_show_recent_history,
                        Some("History".into()),
                        BrowserToolbarAction::HistoryMenu,
                        pane_id,
                        cx,
                    ))
                    .child(self.render_browser_toolbar_button(
                        "profile",
                        BROWSER_ICON_USER_CIRCLE,
                        true,
                        Some("Browser Profile".into()),
                        BrowserToolbarAction::ProfileMenu,
                        pane_id,
                        cx,
                    ))
                    .child(self.render_browser_toolbar_button(
                        "devtools",
                        BROWSER_ICON_TOOLS,
                        true,
                        Some("Toggle DevTools".into()),
                        BrowserToolbarAction::DevTools,
                        pane_id,
                        cx,
                    ))
                    .child(self.render_browser_toolbar_overflow_button(pane_id, cx)),
            )
    }

    pub(crate) fn render_browser_toolbar_new_tab_button(
        &self,
        pane_id: BrowserPaneId,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        div()
            .id(format!(
                "ghostex-gpui-browser-toolbar-new-tab-{}",
                pane_id.0
            ))
            .flex()
            .flex_shrink_0()
            .h(px(BROWSER_TOOLBAR_HEIGHT - 1.0))
            .w(px(BROWSER_TOOLBAR_BUTTON_WIDTH))
            .items_center()
            .justify_center()
            .border_l_1()
            .border_color(titlebar_button_border_color())
            .cursor_default()
            .hover(|this| this.bg(rgb(0x212121)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.swap_browser_tabs_for_active_project(cx);
                    this.browser_tabs.focus_pane(pane_id);
                    this.add_browser_tab(window, cx);
                }),
            )
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Left, |window, cx| {
                titlebar_tooltip("New browser tab", window, cx)
            })
            .child(self.render_browser_tab_new_icon(12.0))
            .into_any_element()
    }

    pub(crate) fn render_browser_toolbar_overflow_button(
        &self,
        pane_id: BrowserPaneId,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        div()
            .id(format!(
                "ghostex-gpui-browser-toolbar-overflow-{}",
                pane_id.0
            ))
            .flex()
            .flex_shrink_0()
            .h(px(BROWSER_TOOLBAR_HEIGHT - 1.0))
            .w(px(BROWSER_TOOLBAR_BUTTON_WIDTH - 1.0))
            .items_center()
            .justify_center()
            .border_l_1()
            .border_color(titlebar_button_border_color())
            .cursor_default()
            .hover(|this| this.bg(rgb(0x212121)))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |_this, _event: &MouseDownEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                }),
            )
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseUpEvent, window, cx| {
                    window.prevent_default();
                    cx.stop_propagation();
                    this.show_browser_pane_actions_menu(pane_id, event.position, window, cx);
                }),
            )
            .managed_tooltip_with_placement(ManagedTooltipPlacement::Left, |window, cx| {
                titlebar_tooltip("Browser pane actions menu", window, cx)
            })
            .child(self.render_browser_tab_overflow_icon())
            .into_any_element()
    }

    pub(crate) fn render_browser_address_field(
        &self,
        pane_id: BrowserPaneId,
        cx: &mut gpui::Context<Self>,
    ) -> AnyElement {
        let address_value = self.browser_tabs.address_value_for_pane(pane_id);
        let address_input = self
            .browser_address_inputs
            .get(&pane_id)
            .cloned()
            .expect("browser address input must exist for rendered pane");

        h_flex()
            .id(format!("ghostex-gpui-browser-address-{}", pane_id.0))
            .flex_1()
            .min_w(px(BROWSER_ADDRESS_MINIMUM_WIDTH))
            .h(px(BROWSER_ADDRESS_HEIGHT))
            .items_center()
            .cursor_text()
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    /*
                    CDXC:GPUIBrowserToolbar 2026-06-14-17:42:
                    GPUI owns the browser toolbar input even though CEF owns the page below it. Route clicks through the complete Browser address-focus boundary so shell focus leaves terminal companion panes before GPUI/AppKit keyboard ownership moves to the input.
                    */
                    let _ = this.focus_browser_address_input_for_pane(pane_id, window, cx);
                }),
            )
            .on_key_down(cx.listener(move |this, event: &KeyDownEvent, window, cx| {
                if event.keystroke.key.as_str() == "escape" {
                    cx.stop_propagation();
                    this.cancel_browser_address_edit_for_pane(pane_id, window, cx);
                }
            }))
            .child(titlebar_svg_icon(
                browser_security_icon_path(&address_value),
                14.0,
                browser_toolbar_security_icon_color(),
            ))
            .child(
                div()
                    .ml(px(8.0))
                    .flex_1()
                    .min_w_0()
                    .h(px(BROWSER_ADDRESS_HEIGHT))
                    .overflow_hidden()
                    .child(
                        Input::new(&address_input)
                            .with_size(ComponentSize::XSmall)
                            .appearance(false)
                            .bordered(false)
                            .focus_bordered(false)
                            .w_full()
                            .px(px(0.0))
                            .py(px(0.0))
                            .text_size(px(13.0))
                            .font_weight(FontWeight::MEDIUM)
                            .line_height(px(BROWSER_ADDRESS_HEIGHT))
                            .text_color(browser_toolbar_text_color()),
                    ),
            )
            .into_any_element()
    }

    pub(crate) fn render_browser_toolbar_button(
        &self,
        id: &'static str,
        icon_path: &'static str,
        enabled: bool,
        tooltip: Option<gpui::SharedString>,
        action: BrowserToolbarAction,
        pane_id: BrowserPaneId,
        cx: &mut gpui::Context<Self>,
    ) -> impl IntoElement {
        let tooltip_placement = ManagedTooltipPlacement::Left;
        let profile_number = if matches!(action, BrowserToolbarAction::ProfileMenu) {
            self.browser_tabs
                .active_tab_for_pane(pane_id)
                .and_then(|tab| tab.profile_id.display_number())
        } else {
            None
        };
        div()
            .id(format!(
                "ghostex-gpui-browser-toolbar-button-{}-{id}",
                pane_id.0
            ))
            .flex()
            .flex_shrink_0()
            .h(px(BROWSER_TOOLBAR_HEIGHT - 1.0))
            .w(px(if id == "back" {
                BROWSER_TOOLBAR_BUTTON_WIDTH - 1.0
            } else {
                BROWSER_TOOLBAR_BUTTON_WIDTH
            }))
            .items_center()
            .justify_center()
            .when(id != "back", |this| this.border_l_1())
            .when(id == "home", |this| this.border_r_1())
            .border_color(titlebar_button_border_color())
            .cursor_default()
            .text_color(if enabled {
                titlebar_icon_color()
            } else {
                browser_toolbar_disabled_icon_color()
            })
            .when(enabled, |this| {
                this.hover(|this| {
                    this.bg(rgb(0x212121))
                        .text_color(titlebar_icon_hover_color())
                })
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, event: &gpui::MouseDownEvent, window, cx| {
                        match action {
                            BrowserToolbarAction::Back
                            | BrowserToolbarAction::Forward
                            | BrowserToolbarAction::Reload
                            | BrowserToolbarAction::StopLoading => {
                                this.perform_browser_toolbar_action(pane_id, action, cx);
                            }
                            BrowserToolbarAction::Home => {
                                this.navigate_browser_home_from_toolbar(pane_id, window, cx);
                            }
                            BrowserToolbarAction::FeedbackTool => {
                                this.run_browser_feedback_tool_from_toolbar(pane_id, window, cx);
                            }
                            BrowserToolbarAction::ResetZoom => {
                                this.reset_browser_zoom_from_toolbar(pane_id, window, cx);
                            }
                            BrowserToolbarAction::ResetMediaPermissions => {
                                this.reset_browser_media_permissions_for_pane(pane_id, cx);
                            }
                            BrowserToolbarAction::HistoryMenu => {
                                this.show_browser_recent_history_menu(
                                    pane_id,
                                    event.position,
                                    window,
                                    cx,
                                );
                            }
                            BrowserToolbarAction::ProfileMenu => {
                                this.show_browser_profile_menu(pane_id, event.position, window, cx);
                            }
                            BrowserToolbarAction::DevTools => {
                                this.toggle_browser_devtools_from_toolbar(pane_id, window, cx);
                            }
                        }
                        window.prevent_default();
                        cx.stop_propagation();
                    }),
                )
            })
            .when(profile_number.is_none(), |this| {
                this.child(titlebar_svg_icon(
                    icon_path,
                    BROWSER_TOOLBAR_BUTTON_ICON_SIZE,
                    if enabled {
                        titlebar_icon_color()
                    } else {
                        browser_toolbar_disabled_icon_color()
                    },
                ))
            })
            .when_some(profile_number, |this, profile_number| {
                this.child(
                    div()
                        .flex()
                        .size(px(BROWSER_TOOLBAR_BUTTON_ICON_SIZE))
                        .items_center()
                        .justify_center()
                        .rounded_full()
                        .border_1()
                        .border_color(rgb(0xffffff).opacity(0.5))
                        .bg(rgb(0xffffff).opacity(0.12))
                        .text_size(px(if profile_number < 10 { 10.0 } else { 8.0 }))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(profile_number.to_string()),
                )
            })
            .when_some(tooltip, |this, tooltip| {
                this.managed_tooltip_with_placement(tooltip_placement, move |window, cx| {
                    titlebar_tooltip(tooltip.clone(), window, cx)
                })
            })
    }
}
