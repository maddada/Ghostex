//! Open, close, preload, and data loading for the native New Thread picker window.
//! SEE-ALSO: apps/desktop/src/app/window/new_thread_picker.rs (the window entity and its decision record).
use crate::app::helpers::*;
use crate::app::window::*;
use crate::*;
use serde_json::{Value, json};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

impl GhostexGpuiApp {
    /// Cmd+Shift+T toggles the picker: a second press while it is open closes it.
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

    fn create_gpui_new_thread_picker_window(
        &mut self,
        visible: bool,
        cx: &mut gpui::Context<Self>,
    ) {
        let agents = self.ordered_new_thread_picker_agents();
        let agent_count = agents.len();
        let agents_loaded = self.new_thread_picker_agents.is_some();
        let accounts = self.new_thread_picker_accounts.clone();
        let window_size = size(
            px(NEW_THREAD_PICKER_WIDTH),
            px(new_thread_picker_window_height(agent_count)),
        );
        let options = WindowOptions {
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
            display_id: self.main_window_display_id,
            titlebar: None,
            window_background: gpui::WindowBackgroundAppearance::Transparent,
            ..Default::default()
        };
        let main_app = cx.weak_entity();
        let picker_slot: Rc<RefCell<Option<Entity<GpuiNewThreadPickerWindow>>>> =
            Rc::new(RefCell::new(None));
        let picker_out = picker_slot.clone();
        self.new_thread_picker_window = cx
            .open_window(options, move |window, cx| {
                window.set_window_title("");
                if visible {
                    window.activate_window();
                }
                let picker = GpuiNewThreadPickerWindow::new(
                    main_app,
                    agents,
                    agents_loaded,
                    accounts,
                    window,
                    cx,
                );
                *picker_out.borrow_mut() = Some(picker.clone());
                /*
                gpui-component's text input reads the window root as
                `gpui_component::Root` while painting, so the picker must sit
                inside one; the Root's own surface is cleared so the rounded
                popup frame is the only thing painted.
                */
                cx.new(|cx| Root::new(picker, window, cx).bg(gpui::transparent_black()))
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
        let agents = self.ordered_new_thread_picker_agents();
        let stale = self.new_thread_picker_window.is_none()
            || self.new_thread_picker_preloaded_agent_count != agents.len();
        if stale {
            self.remove_gpui_new_thread_picker_window(cx);
            self.create_gpui_new_thread_picker_window(true, cx);
        } else {
            let agents_loaded = self.new_thread_picker_agents.is_some();
            let accounts = self.new_thread_picker_accounts.clone();
            let picker = self.new_thread_picker.clone();
            if let (Some(handle), Some(picker)) = (self.new_thread_picker_window, picker) {
                let shown = handle
                    .update(cx, |_root, window, cx| {
                        picker.update(cx, |picker, cx| {
                            picker.reset(agents, agents_loaded, accounts, window, cx);
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

    /// Called by the picker window itself right before it removes its own
    /// window (Escape, a launch, or losing activation). The handle is dropped
    /// here and the replacement is preloaded once this update has finished, so
    /// no window is opened while the closing one is still being updated.
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
                if let (Some(handle), Some(picker)) = (
                    this.new_thread_picker_window,
                    this.new_thread_picker.clone(),
                ) {
                    let _ = handle.update(cx, |_root, _window, cx| {
                        picker.update(cx, |picker, cx| {
                            picker.set_accounts(result, cx);
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
