//! Open, commands and answers for the native Agents Hub.
//!
//! CDXC:AgentLauncher 2026-09-28 WHY:
//! The React Hub asked for its catalog, file bodies, saves and Agent Sync runs through
//! `sidebarCommand` bridge messages and got the answers back as `sidebarState` JSON. The native
//! Hub calls the same Rust functions (apps/desktop/src/app/helpers/agents_hub/) on the
//! background executor and hands their answers to the window as typed values, so nothing
//! crosses a web bridge. The paths a command names are still validated against a fresh catalog
//! before any read or write, exactly as the bridge did.
//! SEE-ALSO: apps/desktop/src/app/window/agents_hub/ (the window), apps/desktop/src/app/native_app_modal_lifecycle.rs (the shared window path), apps/desktop/src/app/delayed_send/app_modal_sidebar_command/agents_hub.rs (the React bridge arms, dead once the React Hub is deleted).
use crate::app::helpers::*;
use crate::app::window::*;
use crate::*;

/// Which Hub a command came from and its answers go to: its own window, or the rail's takeover
/// drawn in the main window's sessions column (claude_web/place.rs `TakeoverPage::AgentsHub`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum AgentsHubTarget {
    Window,
    Takeover,
}

impl GhostexGpuiApp {
    /// Opens the Hub for the `agentsHub` modal's open message. `initialTab` picks the first tab
    /// (the React host never sent it, so the Hub opens on MDs as the React one did).
    pub(crate) fn open_gpui_agents_hub_modal(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let initial_tab = message
            .get("initialTab")
            .and_then(serde_json::Value::as_str)
            .and_then(AgentsHubTab::from_id)
            .unwrap_or(AgentsHubTab::Mds);
        let config = AgentsHubModalConfig {
            palette: self.gpui_native_panel_palette(),
            initial_tab,
            initial_skill: message
                .get("initialSkill")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|skill| !skill.is_empty())
                .map(str::to_string),
            initial_skill_path: message
                .get("initialSkillPath")
                .and_then(serde_json::Value::as_str)
                .filter(|path| !path.is_empty())
                .map(str::to_string),
            word_wrap: crate::app::native_docs::storage::read_word_wrap(),
            embedded: false,
        };
        let host: AgentsHubModalHost = self.native_app_modal_host(cx, |app, command, cx| {
            app.handle_gpui_agents_hub_modal_command(command, AgentsHubTarget::Window, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::AgentsHub,
            AGENTS_HUB_MODAL_WIDTH,
            AGENTS_HUB_MODAL_HEIGHT,
            move |window, cx| cx.new(|cx| GpuiAgentsHubModalWindow::new(config, host, window, cx)),
            cx,
        );
    }

    /// The Hub drawn in the sessions column while the rail's Agents Hub takeover is on, made in the
    /// main window the first time it draws (the window's inputs belong to it).
    pub(crate) fn agents_hub_takeover_view(
        &mut self,
        window: &mut Window,
        cx: &mut gpui::Context<Self>,
    ) -> Entity<GpuiAgentsHubModalWindow> {
        if let Some(view) = self.agents_hub_takeover.clone() {
            return view;
        }
        let config = AgentsHubModalConfig {
            palette: self.gpui_native_panel_palette(),
            initial_tab: AgentsHubTab::Mds,
            initial_skill: None,
            initial_skill_path: None,
            word_wrap: crate::app::native_docs::storage::read_word_wrap(),
            embedded: true,
        };
        let host: AgentsHubModalHost = self.native_app_modal_host(cx, |app, command, cx| {
            app.handle_gpui_agents_hub_modal_command(command, AgentsHubTarget::Takeover, cx);
        });
        let view = cx.new(|cx| GpuiAgentsHubModalWindow::new(config, host, window, cx));
        self.agents_hub_takeover = Some(view.clone());
        view
    }

    /// Runs `update` against the Hub `target` names: its window while it is open, or the
    /// takeover's Hub in the main window.
    fn update_agents_hub<R>(
        &mut self,
        target: AgentsHubTarget,
        cx: &mut gpui::Context<Self>,
        update: impl FnOnce(
            &mut GpuiAgentsHubModalWindow,
            &mut Window,
            &mut gpui::Context<GpuiAgentsHubModalWindow>,
        ) -> R,
    ) -> Option<R> {
        match target {
            AgentsHubTarget::Window => {
                self.update_native_app_modal(GpuiAppModalKind::AgentsHub, cx, update)
            }
            AgentsHubTarget::Takeover => {
                let view = self.agents_hub_takeover.clone()?;
                let main = self.main_window_handle?;
                main.update(cx, |_, window, cx| {
                    view.update(cx, |modal, cx| update(modal, window, cx))
                })
                .ok()
            }
        }
    }

    fn handle_gpui_agents_hub_modal_command(
        &mut self,
        command: AgentsHubModalCommand,
        target: AgentsHubTarget,
        cx: &mut gpui::Context<Self>,
    ) {
        match command {
            AgentsHubModalCommand::RequestCatalog => {
                self.run_gpui_agents_hub_task(
                    target,
                    || AgentsHubCatalog::from_json(gpui_agents_hub_catalog_message()),
                    |modal, catalog, window, cx| {
                        if let Some(catalog) = catalog {
                            modal.receive_catalog(catalog, window, cx);
                        }
                    },
                    cx,
                );
            }
            AgentsHubModalCommand::RequestFileContent {
                file_path,
                request_id,
            } => {
                self.run_gpui_agents_hub_task(
                    target,
                    move || {
                        AgentsHubFileContent::from_json(gpui_agents_hub_file_content_message(
                            file_path, request_id,
                        ))
                    },
                    |modal, answer, window, cx| {
                        if let Some(answer) = answer {
                            modal.receive_file_content(answer, window, cx);
                        }
                    },
                    cx,
                );
            }
            AgentsHubModalCommand::SaveFile { file_path, content } => {
                self.save_gpui_agents_hub_file(file_path, content, target, cx);
            }
            AgentsHubModalCommand::OpenPath { path } => {
                self.open_gpui_agents_hub_path(path, cx);
            }
            AgentsHubModalCommand::OpenInBuiltInEditor { file_path } => {
                self.open_gpui_agents_hub_file_path_in_built_in_editor(file_path, cx);
            }
            AgentsHubModalCommand::Copied => gpui_play_copy_sound(),
            AgentsHubModalCommand::SetWordWrap { wrap } => {
                self.set_gpui_agents_hub_word_wrap(wrap, cx);
            }
            AgentsHubModalCommand::RequestSyncReport => {
                /*
                CDXC:AgentSync 2026-09-16 WHY:
                Agent Sync scans, plans, and applies through the shared ghostex-agent-sync crate on the background executor, the same code `ghostex agent-sync` runs.
                */
                self.run_gpui_agents_hub_task(
                    target,
                    || SyncReport::from_json(gpui_agent_sync_report_message()),
                    |modal, report, _window, cx| {
                        if let Some(report) = report {
                            modal.receive_sync_report(report, cx);
                        }
                    },
                    cx,
                );
            }
            AgentsHubModalCommand::RequestSyncPlan {
                scope,
                seed_main_from,
                pointer_policy,
            } => {
                self.run_gpui_agents_hub_task(
                    target,
                    move || {
                        SyncPlan::from_json(gpui_agent_sync_plan_message(
                            scope,
                            seed_main_from,
                            pointer_policy,
                        ))
                    },
                    |modal, plan, _window, cx| {
                        if let Some(plan) = plan {
                            modal.receive_sync_plan(plan, cx);
                        }
                    },
                    cx,
                );
            }
            AgentsHubModalCommand::ApplySyncPlan {
                scope,
                groups,
                seed_main_from,
                pointer_policy,
            } => {
                self.run_gpui_agents_hub_task(
                    target,
                    move || {
                        SyncApplyResult::from_json(gpui_agent_sync_apply_message(
                            scope,
                            groups,
                            seed_main_from,
                            pointer_policy,
                        ))
                    },
                    |modal, result, _window, cx| {
                        if let Some(result) = result {
                            modal.receive_sync_apply_result(result, cx);
                        }
                    },
                    cx,
                );
            }
            AgentsHubModalCommand::Close => match target {
                AgentsHubTarget::Window => {
                    if self.native_app_modal_kind() == Some(GpuiAppModalKind::AgentsHub) {
                        self.close_native_app_modal_from_bridge(cx);
                    }
                }
                // The takeover's Hub closes the way its rail icon does.
                AgentsHubTarget::Takeover => {
                    if self.takeover_page_on()
                        == Some(crate::app::claude_web::TakeoverPage::AgentsHub)
                    {
                        self.turn_claude_off(cx);
                    }
                }
            },
        }
    }

    /// The Hub's wrap toggle is the Files view's: a Files view already set up follows it at once
    /// (its toggle needs the main window to rewrap its editors), otherwise the stored choice is
    /// what the Files view reads when it starts.
    fn set_gpui_agents_hub_word_wrap(&mut self, wrap: bool, cx: &mut gpui::Context<Self>) {
        let files_started = self.native_docs.focus.is_some();
        if !files_started {
            self.native_docs.word_wrap = wrap;
            self.native_docs_persist_word_wrap(cx);
            return;
        }
        if self.native_docs.word_wrap == wrap {
            return;
        }
        let Some(main) = self.main_window_handle else {
            return;
        };
        let app = cx.weak_entity();
        cx.defer(move |cx| {
            let _ = main.update(cx, |_, window, cx| {
                let _ = app.update(cx, |app, cx| {
                    if app.native_docs.word_wrap != wrap {
                        app.native_docs_toggle_word_wrap(window, cx);
                    }
                });
            });
        });
    }

    /// Runs `task` on the background executor and hands its answer to the open Hub.
    fn run_gpui_agents_hub_task<T: Send + 'static>(
        &mut self,
        target: AgentsHubTarget,
        task: impl FnOnce() -> T + Send + 'static,
        deliver: impl FnOnce(
            &mut GpuiAgentsHubModalWindow,
            T,
            &mut Window,
            &mut gpui::Context<GpuiAgentsHubModalWindow>,
        ) + 'static,
        cx: &mut gpui::Context<Self>,
    ) {
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let answer = background.spawn(async move { task() }).await;
            let _ = this.update(cx, |app, cx| {
                app.update_agents_hub(
                    target,
                    cx,
                    |modal: &mut GpuiAgentsHubModalWindow, window, cx| {
                        deliver(modal, answer, window, cx)
                    },
                );
            });
        })
        .detach();
    }

    /// CDXC:AgentLauncher 2026-06-24-12:26:
    /// Agents Hub saves are real file writes, and the writer validates the file against the current catalog-derived allowlist before touching disk. It never logs file content, keeps no fallback draft store, and never claims success without refreshing the Hub's catalog.
    fn save_gpui_agents_hub_file(
        &mut self,
        file_path: String,
        content: String,
        target: AgentsHubTarget,
        cx: &mut gpui::Context<Self>,
    ) {
        let background = cx.background_executor().clone();
        cx.spawn(async move |this, cx| {
            let result = background
                .spawn(async move {
                    gpui_save_agents_hub_file(file_path, content).map(AgentsHubCatalog::from_json)
                })
                .await;
            let _ = this.update(cx, |app, cx| match result {
                Ok(catalog) => {
                    if let Some(catalog) = catalog {
                        app.update_agents_hub(
                            target,
                            cx,
                            |modal: &mut GpuiAgentsHubModalWindow, window, cx| {
                                modal.receive_catalog(catalog, window, cx)
                            },
                        );
                    }
                    app.dispatch_gpui_app_modal_toast(
                        "success",
                        "File saved",
                        "Agents Hub refreshed the saved file metadata.",
                        cx,
                    );
                }
                Err(message) => {
                    app.dispatch_gpui_app_modal_toast(
                        "warning",
                        "Could not save Agents Hub file",
                        &message,
                        cx,
                    );
                }
            });
        })
        .detach();
    }

    /// The Hub steps aside before "Open in built-in editor" moves to the Code view: the native
    /// Hub when it is open, else the React modal window.
    pub(crate) fn close_gpui_agents_hub_for_navigation(&mut self, cx: &mut gpui::Context<Self>) {
        // The takeover's Hub steps aside the way its rail icon closes it.
        if self.takeover_page_on() == Some(crate::app::claude_web::TakeoverPage::AgentsHub) {
            self.turn_claude_off(cx);
        }
        if self.native_app_modal_kind() == Some(GpuiAppModalKind::AgentsHub) {
            self.close_native_app_modal_from_bridge(cx);
            return;
        }
        self.close_gpui_app_modal_window_and_restore_command_focus(cx);
    }
}
