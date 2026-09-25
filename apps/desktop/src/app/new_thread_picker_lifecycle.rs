//! Open, close, preload, and data loading for the native New Thread picker window.
//! SEE-ALSO: apps/desktop/src/app/window/new_thread_picker.rs (the window entity and its decision record).
use crate::app::helpers::*;
use crate::app::titlebar::account_usage::{
    account_display_name, account_display_text, claude_headline_windows,
};
use crate::app::window::*;
use crate::*;
use serde_json::{Value, json};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

/// The picker's window root inside the app: it answers the New Thread hotkey
/// (a second press closes the picker) around the kit-only picker view,
/// which cannot name the app's action types itself.
pub(crate) struct GpuiNewThreadPickerShell {
    picker: Entity<GpuiNewThreadPickerWindow>,
    main_app: gpui::WeakEntity<GhostexGpuiApp>,
}

impl Render for GpuiNewThreadPickerShell {
    fn render(&mut self, _window: &mut Window, cx: &mut gpui::Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .on_action(
                cx.listener(|this, action: &RunConfiguredGhostexHotkey, window, cx| {
                    if action.action_id == "openNewThreadPalette" {
                        window.remove_window();
                        let _ = this.main_app.update(cx, |app, cx| {
                            app.release_gpui_new_thread_picker_window(cx);
                        });
                    }
                }),
            )
            .child(self.picker.clone())
    }
}

fn new_thread_picker_agent_from_hud(value: &Value) -> Option<NewThreadPickerAgent> {
    let agent_id = value.get("agentId")?.as_str()?.trim();
    let name = value.get("name")?.as_str()?.trim();
    if agent_id.is_empty() || name.is_empty() {
        return None;
    }
    let icon = value
        .get("icon")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|icon| !icon.is_empty())
        .map(str::to_string);
    let icon_path = icon.as_deref().and_then(workspace_tab_agent_icon_path);
    let (icon_svg_size, icon_accent) = match icon.as_deref() {
        Some(icon) => (
            workspace_tab_agent_svg_size(icon),
            workspace_tab_agent_icon_accent_color(icon),
        ),
        None => (12.0, 0xffffff),
    };
    Some(NewThreadPickerAgent {
        agent_id: agent_id.to_string(),
        name: name.to_string(),
        icon,
        icon_path,
        icon_svg_size,
        icon_accent,
    })
}

/// The sidebar HUD agent buttons in dropdown order, with the last-used agent
/// moved to the front so it is the preselected row.
pub(crate) fn order_new_thread_picker_agents(
    hud_agents: &[Value],
    primary_agent_id: Option<&str>,
) -> Vec<NewThreadPickerAgent> {
    let mut agents: Vec<NewThreadPickerAgent> = hud_agents
        .iter()
        .filter_map(new_thread_picker_agent_from_hud)
        .collect();
    if let Some(primary_index) = primary_agent_id
        .and_then(|primary| agents.iter().position(|agent| agent.agent_id == primary))
    {
        let primary = agents.remove(primary_index);
        agents.insert(0, primary);
    }
    agents
}

/// Port of `accountUsageLabel` in packages/shared/account-usage-label.ts.
/// CDXC:AgentProviders 2026-09-12 SEE-ALSO:
/// packages/shared/account-usage-label.ts owns the shared Fable percentage label decision.
fn usage_window_label(window: &Value) -> Option<String> {
    if window["model"]
        .as_str()
        .is_some_and(|model| model.eq_ignore_ascii_case("fable"))
    {
        return Some("Fable".to_string());
    }
    let seconds = window["limitWindowSeconds"].as_i64().unwrap_or(0);
    let duration = if seconds > 0 {
        if seconds % 86_400 == 0 {
            Some(format!("{}d", seconds / 86_400))
        } else if seconds % 3_600 == 0 {
            Some(format!("{}h", seconds / 3_600))
        } else {
            Some(format!("{}m", seconds / 60))
        }
    } else if window["id"].as_str() == Some("fiveHour") {
        Some("5h".to_string())
    } else if window["id"].as_str() == Some("sevenDay") || window["model"].is_string() {
        Some("7d".to_string())
    } else {
        None
    };
    match duration {
        Some(duration) => Some(match window["model"].as_str() {
            Some(model) => format!("{model} {duration}"),
            None => duration,
        }),
        None => window["label"].as_str().map(str::to_string),
    }
}

/// Port of `AccountLauncherUsage` in the deleted React sidebar's packages/core-ui/accounts/agent-launcher-menu.tsx (git history):
/// Claude shows its two tightest limits out of weekly, five-hour, and Fable, Codex the weekly window and available resets.
fn account_usage_line(account: &Value) -> Option<String> {
    let windows = account["usage"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let main: Vec<&Value> = windows.iter().filter(|w| w["model"].is_null()).collect();
    let weekly = main.iter().copied().find(|w| {
        w["id"].as_str() == Some("sevenDay")
            || w["limitWindowSeconds"].as_i64().unwrap_or(0) >= 604_800
    });
    let percent = |w: &Value| -> Option<String> {
        let label = usage_window_label(w)?;
        let used = w["usedPercent"].as_f64()?;
        Some(format!("{label}: {}%", used.round() as i64))
    };
    let values: Vec<String> = if account["provider"].as_str() == Some("claude") {
        claude_headline_windows(windows)
            .into_iter()
            .filter_map(percent)
            .collect()
    } else {
        let mut values: Vec<String> = weekly.and_then(percent).into_iter().collect();
        if let Some(resets) = account["resetCredits"].as_u64() {
            values.push(format!("{resets}rs"));
        }
        values
    };
    (!values.is_empty()).then(|| values.join(" · "))
}

/// The registered accounts of the `/api/agentAccounts` list, masked and
/// summarised for the picker's rows.
fn new_thread_picker_accounts(state: &Value) -> Vec<NewThreadPickerAccount> {
    state["accounts"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[])
        .iter()
        .filter(|account| account["registered"].as_bool() == Some(true))
        .filter_map(|account| {
            let id = account["id"].as_str()?.to_string();
            let provider = account["provider"].as_str()?.to_string();
            let is_default = state["defaultAccounts"][&provider].as_str() == Some(id.as_str());
            Some(NewThreadPickerAccount {
                name: account_display_name(account),
                usage: account_usage_line(account),
                ready: account["status"].as_str() == Some("ready"),
                is_default,
                id,
                provider,
            })
        })
        .collect()
}

impl GhostexGpuiApp {
    /// The New Thread hotkey toggles the picker: a second press while it is open closes it.
    pub(crate) fn toggle_gpui_new_thread_picker(&mut self, cx: &mut gpui::Context<Self>) {
        if self.new_thread_picker_visible {
            self.close_gpui_new_thread_picker(cx);
        } else {
            self.open_gpui_new_thread_picker(cx);
        }
    }

    /*
    CDXC:AgentLauncher 2026-09-09 DECISION:
    User: opening must be literally instant, and preloading in GPUI is fine
    because it costs no extra RAM. The picker window is therefore created
    hidden ahead of time (with the cached agent list and accounts already
    inside it) and the hotkey only resets its state and orders it front. gpui
    has no hide call, so closing removes the window and a fresh hidden one is
    created right after, off the user's path; the same recycle runs when the
    main window moves or resizes (the popup is centered on it at creation) and
    when the agent list changes (the frame height follows the agent count).
    */
    pub(crate) fn ensure_gpui_new_thread_picker_preloaded(&mut self, cx: &mut gpui::Context<Self>) {
        if self.new_thread_picker_window.is_some() {
            return;
        }
        self.create_gpui_new_thread_picker_window(false, cx);
    }

    /// Drops a hidden preloaded window and creates a fresh one; a visible
    /// picker is left alone.
    pub(crate) fn recycle_gpui_new_thread_picker_preload(&mut self, cx: &mut gpui::Context<Self>) {
        if self.new_thread_picker_visible {
            return;
        }
        self.remove_gpui_new_thread_picker_window(cx);
        self.ensure_gpui_new_thread_picker_preloaded(cx);
    }

    fn remove_gpui_new_thread_picker_window(&mut self, cx: &mut gpui::Context<Self>) {
        self.new_thread_picker = None;
        if let Some(handle) = self.new_thread_picker_window.take() {
            let _ = handle.update(cx, |_root, window, _cx| {
                window.remove_window();
            });
        }
    }

    fn ordered_new_thread_picker_agents(&self) -> Vec<NewThreadPickerAgent> {
        order_new_thread_picker_agents(
            self.new_thread_picker_agents.as_deref().unwrap_or(&[]),
            self.sidebar_primary_agent_launcher_id.as_deref(),
        )
    }

    /// The picker's current palette, agents, and cached accounts.
    fn new_thread_picker_config(&self) -> NewThreadPickerConfig {
        NewThreadPickerConfig {
            palette: self.gpui_native_modal_palette(),
            agents: self.ordered_new_thread_picker_agents(),
            agents_loaded: self.new_thread_picker_agents.is_some(),
            accounts: self
                .new_thread_picker_accounts
                .as_ref()
                .map(new_thread_picker_accounts),
            close_when_inactive: true,
        }
    }

    fn create_gpui_new_thread_picker_window(
        &mut self,
        visible: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let config = self.new_thread_picker_config();
        let agent_count = config.agents.len();
        let window_size = size(
            px(NEW_THREAD_PICKER_WIDTH),
            px(new_thread_picker_window_height(agent_count)),
        );
        let options = WindowOptions {
            kind: crate::app::window::popup_frame::child_window_kind(),
            #[cfg(target_os = "linux")]
            x11_parent: self.main_window_handle,
            window_bounds: Some(WindowBounds::Windowed(gpui::Bounds::centered_at(
                self.main_window_bounds.center(),
                window_size,
            ))),
            app_id: gpui_platform_window_app_id(),
            focus: visible,
            icon: gpui_platform_window_icon(),
            show: visible,
            is_resizable: false,
            is_minimizable: false,
            display_id: crate::app::window::popup_frame::display_at(
                self.main_window_bounds.center(),
                cx,
            )
            .or(self.main_window_display_id),
            titlebar: None,
            window_background: if window_glass_active() {
                gpui::WindowBackgroundAppearance::Blurred
            } else {
                gpui::WindowBackgroundAppearance::Transparent
            },
            ..Default::default()
        };
        let host = self.native_app_modal_host(cx, Self::handle_new_thread_picker_command);
        let main_app = cx.weak_entity();
        let picker_slot: Rc<RefCell<Option<Entity<GpuiNewThreadPickerWindow>>>> =
            Rc::new(RefCell::new(None));
        let picker_out = picker_slot.clone();
        self.new_thread_picker_window = cx
            .open_window(options, move |window, cx| {
                window.set_window_title(if cfg!(target_os = "windows") {
                    "Ghostex New Thread"
                } else {
                    ""
                });
                window.set_background_corner_radius(px(10.0));
                crate::app::helpers::apply_frosted_menu_blur(window);
                crate::app::window::popup_frame::strip_gpui_popup_window_frame(window);
                if visible {
                    window.activate_window();
                }
                let picker = cx.new(|cx| {
                    let mut picker = GpuiNewThreadPickerWindow::new(config, host, window, cx);
                    picker.glass = window_glass_active();
                    picker.frosted_fill = picker
                        .glass
                        .then(|| crate::app::helpers::frosted_menu_fill(picker.surface_color()));
                    picker
                });
                *picker_out.borrow_mut() = Some(picker.clone());
                let shell = cx.new(|_cx| GpuiNewThreadPickerShell { picker, main_app });
                /*
                gpui-component's text input reads the window root as
                `gpui_component::Root` while painting, so the picker must sit
                inside one; the Root's own surface is cleared so the rounded
                popup frame is the only thing painted.
                */
                cx.new(|cx| Root::new(shell, window, cx).bg(gpui::transparent_black()))
            })
            .ok();
        self.new_thread_picker = picker_slot.borrow_mut().take();
        self.new_thread_picker_preloaded_agent_count = agent_count;
        self.new_thread_picker_visible = visible && self.new_thread_picker_window.is_some();
    }

    pub(crate) fn open_gpui_new_thread_picker(&mut self, cx: &mut gpui::Context<Self>) {
        if self.new_thread_picker_visible {
            return;
        }
        let config = self.new_thread_picker_config();
        let stale = self.new_thread_picker_window.is_none()
            || self.new_thread_picker_preloaded_agent_count != config.agents.len();
        if stale {
            self.remove_gpui_new_thread_picker_window(cx);
            self.create_gpui_new_thread_picker_window(true, cx);
        } else {
            let picker = self.new_thread_picker.clone();
            if let (Some(handle), Some(picker)) = (self.new_thread_picker_window, picker) {
                let shown = handle
                    .update(cx, |_root, window, cx| {
                        picker.update(cx, |picker, cx| {
                            picker.reset(config, window, cx);
                        });
                        window.activate_window();
                    })
                    .is_ok();
                if shown {
                    self.new_thread_picker_visible = true;
                } else {
                    self.remove_gpui_new_thread_picker_window(cx);
                    self.create_gpui_new_thread_picker_window(true, cx);
                }
            }
        }
        self.refresh_gpui_new_thread_picker_agents(cx);
        self.refresh_gpui_new_thread_picker_accounts(cx);
    }

    /// Closes from the main window (hotkey toggle): removes the visible window
    /// and preloads the next one.
    pub(crate) fn close_gpui_new_thread_picker(&mut self, cx: &mut gpui::Context<Self>) {
        self.new_thread_picker_visible = false;
        self.remove_gpui_new_thread_picker_window(cx);
        self.ensure_gpui_new_thread_picker_preloaded(cx);
    }

    /// Called once the picker has removed its own window (Escape, a launch,
    /// or losing activation). The handle is dropped here and the replacement
    /// is preloaded once this update has finished, so no window is opened
    /// while the closing one is still being updated.
    pub(crate) fn release_gpui_new_thread_picker_window(&mut self, cx: &mut gpui::Context<Self>) {
        self.new_thread_picker_visible = false;
        self.new_thread_picker_window = None;
        self.new_thread_picker = None;
        let app = cx.entity();
        cx.defer(move |cx| {
            app.update(cx, |app, cx| {
                app.ensure_gpui_new_thread_picker_preloaded(cx);
            });
        });
    }

    /// The picker's commands: launches and pane openers go through the same
    /// sidebar messages the React palette posts, then the window is released.
    fn handle_new_thread_picker_command(
        &mut self,
        command: NewThreadPickerCommand,
        cx: &mut gpui::Context<Self>,
    ) {
        match command {
            NewThreadPickerCommand::LaunchAgent {
                agent_id,
                account_id,
            } => {
                self.release_gpui_new_thread_picker_window(cx);
                self.launch_agent_in_active_project(agent_id, account_id, cx);
            }
            NewThreadPickerCommand::OpenBrowser => {
                self.dispatch_gpui_sidebar_host_message(
                    json!({ "type": "openBrowserPaneInGroup" }),
                    cx,
                );
                self.release_gpui_new_thread_picker_window(cx);
            }
            NewThreadPickerCommand::CreateTerminal => {
                self.dispatch_gpui_sidebar_host_message(json!({ "type": "createSession" }), cx);
                self.release_gpui_new_thread_picker_window(cx);
            }
            NewThreadPickerCommand::AddAccount => {
                self.open_gpui_settings_accounts_from_new_thread_picker(cx);
                self.release_gpui_new_thread_picker_window(cx);
            }
            NewThreadPickerCommand::RetryAccounts => {
                self.refresh_gpui_new_thread_picker_accounts(cx);
            }
            NewThreadPickerCommand::Closed => {
                self.release_gpui_new_thread_picker_window(cx);
            }
        }
    }

    /// Reads the sidebar HUD agent buttons in the background and caches them.
    /// A visible picker gets the rows pushed in (and resizes); a hidden
    /// preload is recycled so its frame matches the new count.
    pub(crate) fn refresh_gpui_new_thread_picker_agents(&mut self, cx: &mut gpui::Context<Self>) {
        if self.new_thread_picker_agents_refresh_in_flight {
            return;
        }
        self.new_thread_picker_agents_refresh_in_flight = true;
        let active_project_id = self.gpui_app_modal_active_project_id();
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let agents = background
                .spawn(async move {
                    gpui_sidebar_hud_from_gxserver(
                        Duration::from_secs(5),
                        active_project_id.as_deref(),
                    )
                    .ok()
                    .and_then(|hud| hud.agents.as_array().cloned())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                this.new_thread_picker_agents_refresh_in_flight = false;
                let Some(agents) = agents else {
                    return;
                };
                let changed = this.new_thread_picker_agents.as_ref() != Some(&agents);
                this.new_thread_picker_agents = Some(agents);
                this.push_gpui_export_transcript_modal_agents(cx);
                if !changed && this.new_thread_picker_window.is_some() {
                    return;
                }
                if this.new_thread_picker_visible {
                    let ordered = this.ordered_new_thread_picker_agents();
                    if let (Some(handle), Some(picker)) = (
                        this.new_thread_picker_window,
                        this.new_thread_picker.clone(),
                    ) {
                        let _ = handle.update(cx, |_root, window, cx| {
                            picker.update(cx, |picker, cx| {
                                picker.set_agents(ordered, window, cx);
                            });
                        });
                    }
                } else {
                    this.recycle_gpui_new_thread_picker_preload(cx);
                }
            });
        })
        .detach();
    }

    /// Lists the local gxserver accounts (registered Claude and Codex accounts
    /// with usage) for the picker's count badges and account rows. The result
    /// is cached so a preloaded window opens with them already in place.
    pub(crate) fn refresh_gpui_new_thread_picker_accounts(&mut self, cx: &mut gpui::Context<Self>) {
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result: Result<Value, String> = background
                .spawn(async move {
                    gpui_gxserver_rpc_result(
                        "/api/agentAccounts",
                        &json!({ "operation": "list" }),
                        Duration::from_secs(30),
                    )
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if let Ok(state) = &result {
                    this.new_thread_picker_accounts = Some(state.clone());
                }
                let accounts = result
                    .as_ref()
                    .map(new_thread_picker_accounts)
                    .map_err(|error| account_display_text(error));
                if let (Some(handle), Some(picker)) = (
                    this.new_thread_picker_window,
                    this.new_thread_picker.clone(),
                ) {
                    let _ = handle.update(cx, |_root, _window, cx| {
                        picker.update(cx, |picker, cx| {
                            picker.set_accounts(accounts, cx);
                        });
                    });
                }
            });
        })
        .detach();
    }

    /// The picker's Add account row: Settings opens on its Accounts page, the
    /// same destination the dropdown's Add account row uses.
    pub(crate) fn open_gpui_settings_accounts_from_new_thread_picker(
        &mut self,
        cx: &mut gpui::Context<Self>,
    ) {
        let modal = GpuiAppModalKind::Settings;
        let sidebar_state_message = self.gpui_app_modal_sidebar_state_message_for_open(modal, cx);
        let mut open_message = modal.open_message();
        open_message["initialTab"] = json!("accounts");
        if modal.requires_sidebar_state() {
            open_message["latestSidebarStateMessage"] = sidebar_state_message.clone();
        }
        self.open_gpui_app_modal_window(modal, open_message, sidebar_state_message, None, cx);
    }
}
