//! Open, close, and create for the native New Coordinator dialog: the copies of the project it
//! can run in (gx-core `SidebarView::project_copies`), each computer's agents and models, the
//! playbook of the picked copy, and the checks before Create.
//! SEE-ALSO: apps/desktop/src/app/window/new_coordinator_modal.rs (the window), apps/desktop/src/app/window/new_coordinator_computer.rs (the Computer field), apps/desktop/src/app/gx_store/create/coordinator.rs (the create), packages/gx-core/src/sidebar_actions/open.rs (the `coordinator` project action that opens it).
use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use ghostex_gx_chat_core::menus::catalog::AgentModelCatalog;
use ghostex_gx_core::{LOCAL_MACHINE_ID, ProjectKey};

use crate::app::window::*;
use crate::*;

type DialogSlot = Rc<RefCell<Option<gpui::WeakEntity<GpuiNewCoordinatorModalWindow>>>>;
/// Each other computer's catalog, once read in this dialog (`None` inside: it sent none).
type CatalogCache = Rc<RefCell<BTreeMap<String, Option<AgentModelCatalog>>>>;

fn dialog_of(slot: &DialogSlot) -> Option<gpui::Entity<GpuiNewCoordinatorModalWindow>> {
    slot.borrow().as_ref().and_then(|dialog| dialog.upgrade())
}

impl GhostexGpuiApp {
    /// Opens the dialog for the `newCoordinator` modal: `groupId` (the project's sidebar group,
    /// which names the machine too: for a merged entry, the copy it opens on) and `projectName`.
    /// An open without a group is dropped.
    pub(crate) fn open_gpui_new_coordinator_modal(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let text = |key: &str| {
            message
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        let Some(opened_group) = text("groupId") else {
            return;
        };
        let catalogs: CatalogCache = Default::default();
        let copies = self.gx_store_new_coordinator_copies(&opened_group, &catalogs.borrow());
        let selected_copy = copies
            .iter()
            .position(|copy| copy.group_id == opened_group)
            .unwrap_or(0);
        let first_pick = copies.get(selected_copy).map(|copy| copy.group_id.clone());
        let height = NEW_COORDINATOR_MODAL_INITIAL_HEIGHT
            + if copies.len() > 1 {
                NEW_COORDINATOR_COMPUTER_FIELD_HEIGHT
            } else {
                0.0
            };
        let config = NewCoordinatorModalConfig {
            project_name: text("projectName").unwrap_or_else(|| "this project".to_string()),
            copies,
            selected_copy,
            selected_agent: 0,
            playbook: None,
            palette: self.gpui_native_dialog_palette(),
            all_efforts: crate::shared_settings::shared_sidebar_settings_snapshot()
                .object()
                .get("orchestratorAllEfforts")
                .and_then(serde_json::Value::as_bool)
                == Some(true),
        };
        let dialog: DialogSlot = Default::default();
        let host_dialog = dialog.clone();
        let host_catalogs = catalogs.clone();
        let host_opened = opened_group.clone();
        let host = self.native_app_modal_host(cx, move |app, command, cx| {
            match command {
                NewCoordinatorModalCommand::Create {
                    group_id,
                    agent_id,
                    name,
                    model,
                    effort,
                    reply,
                } => {
                    if let Some(reason) =
                        app.gx_store_new_coordinator_refusal(&host_opened, &group_id, &agent_id)
                    {
                        app.refresh_new_coordinator_copies(
                            &host_opened,
                            &host_dialog,
                            &host_catalogs,
                            cx,
                        );
                        reply.finish(Err(reason), cx);
                        return;
                    }
                    app.gx_store_create_coordinator(
                        &group_id,
                        &agent_id,
                        &name,
                        model.as_deref(),
                        effort.as_deref(),
                        move |app, result, cx| {
                            let refused = result.as_ref().err().cloned();
                            let delivered = reply.finish(result, cx);
                            match refused {
                                None if delivered => app.release_native_app_modal_window(
                                    GpuiAppModalKind::NewCoordinator,
                                    cx,
                                ),
                                // The dialog was closed while it waited: say it here instead.
                                Some(reason) if !delivered => app.gx_store_create_toast(
                                    "warning",
                                    "Coordinator not created",
                                    Some(&reason),
                                    cx,
                                ),
                                _ => {}
                            }
                        },
                        cx,
                    );
                    return;
                }
                // The dialog stays open until gxserver answers the save.
                NewCoordinatorModalCommand::SavePlaybook {
                    group_id,
                    change,
                    reply,
                } => {
                    let Some((remote, project_id)) = app.new_coordinator_playbook_target(&group_id)
                    else {
                        reply.finish(Err("That computer is not connected.".to_string()), cx);
                        return;
                    };
                    let mut params = match change {
                        PlaybookChange::Overrides(text) => serde_json::json!({ "overrides": text }),
                        PlaybookChange::UseGhostex => {
                            serde_json::json!({ "retireOldCopies": true })
                        }
                    };
                    // The answer's `source` is about the dialog's project.
                    params["projectId"] = serde_json::json!(project_id);
                    cx.spawn(async move |_this, cx| {
                        let saved = crate::app::gx_store::gx_rpc(
                            remote,
                            "/api/setCoordinatorPlaybook",
                            params,
                        )
                        .await
                        .map_err(|error| error.message);
                        cx.update(|cx| reply.finish(saved, cx));
                    })
                    .detach();
                    return;
                }
                NewCoordinatorModalCommand::ChooseCopy {
                    group_id,
                    generation,
                } => {
                    app.read_new_coordinator_playbook(&group_id, generation, &host_dialog, cx);
                    app.read_new_coordinator_lineup(&group_id, &host_dialog, &host_catalogs, cx);
                    return;
                }
                NewCoordinatorModalCommand::RefreshCopies => {
                    app.refresh_new_coordinator_copies(
                        &host_opened,
                        &host_dialog,
                        &host_catalogs,
                        cx,
                    );
                    return;
                }
                NewCoordinatorModalCommand::Cancel => {}
            }
            app.release_native_app_modal_window(GpuiAppModalKind::NewCoordinator, cx);
        });
        let dialog_slot = dialog.clone();
        self.open_native_app_modal(
            GpuiAppModalKind::NewCoordinator,
            NEW_COORDINATOR_MODAL_WIDTH,
            height,
            move |window, cx| {
                let view =
                    cx.new(|cx| GpuiNewCoordinatorModalWindow::new(config, host, window, cx));
                *dialog_slot.borrow_mut() = Some(view.downgrade());
                view
            },
            cx,
        );
        if let Some(group_id) = first_pick {
            self.read_new_coordinator_playbook(&group_id, 0, &dialog, cx);
            self.read_new_coordinator_lineup(&group_id, &dialog, &catalogs, cx);
        }
    }

    /// The copies a New Coordinator dialog opened for `opened_group` offers: every copy of the
    /// merged project it is one of, else that project alone. This computer's copies list its
    /// agents with their lineups; another computer's list that computer's launchers, their
    /// lineups once read (`catalogs`).
    pub(crate) fn gx_store_new_coordinator_copies(
        &self,
        opened_group: &str,
        catalogs: &BTreeMap<String, Option<AgentModelCatalog>>,
    ) -> Vec<NewCoordinatorCopy> {
        let listed: Vec<ghostex_gx_core::ProjectCopy> = self
            .gx_store
            .sidebar_list
            .view()
            .project_copies(opened_group)
            .map(<[_]>::to_vec)
            .unwrap_or_default();
        if listed.is_empty() {
            let machine_id = ProjectKey::parse_sidebar_group_id(opened_group)
                .and_then(|project| project.machine.remote_id().map(str::to_string))
                .unwrap_or_else(|| LOCAL_MACHINE_ID.to_string());
            return vec![self.new_coordinator_copy(
                opened_group,
                String::new(),
                String::new(),
                &machine_id,
                true,
                catalogs,
            )];
        }
        listed
            .into_iter()
            .map(|copy| {
                self.new_coordinator_copy(
                    &copy.group_id,
                    copy.label,
                    copy.computer,
                    &copy.machine_id,
                    copy.connected,
                    catalogs,
                )
            })
            .collect()
    }

    fn new_coordinator_copy(
        &self,
        group_id: &str,
        label: String,
        computer: String,
        machine_id: &str,
        connected: bool,
        catalogs: &BTreeMap<String, Option<AgentModelCatalog>>,
    ) -> NewCoordinatorCopy {
        if machine_id == LOCAL_MACHINE_ID {
            return NewCoordinatorCopy {
                group_id: group_id.to_string(),
                label,
                computer,
                connected: true,
                gone: false,
                agents: Some(self.gx_store_coordinator_agents()),
                lineup: CopyLineup::Held,
            };
        }
        let connected = connected
            && self
                .gpui_remote_gxserver_request_target(machine_id)
                .is_some();
        let (agents, lineup) = match catalogs.get(machine_id) {
            Some(catalog) => (
                self.gx_store_remote_coordinator_agents(machine_id, catalog.as_ref()),
                if catalog.is_some() {
                    CopyLineup::Held
                } else {
                    CopyLineup::Missing
                },
            ),
            None => (
                self.gx_store_remote_coordinator_agents(machine_id, None),
                CopyLineup::Loading,
            ),
        };
        NewCoordinatorCopy {
            group_id: group_id.to_string(),
            label,
            computer,
            connected,
            gone: false,
            agents,
            lineup,
        }
    }

    /// Sends the open dialog the copies as they are now.
    fn refresh_new_coordinator_copies(
        &self,
        opened_group: &str,
        dialog: &DialogSlot,
        catalogs: &CatalogCache,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(view) = dialog_of(dialog) else {
            return;
        };
        let copies = self.gx_store_new_coordinator_copies(opened_group, &catalogs.borrow());
        view.update(cx, |dialog, cx| dialog.set_copies(copies, cx));
    }

    /// Why Create must not run in the copy `group_id` names, checked just before it: the project
    /// must still be open on its computer (the store's own inventory, so a project the list
    /// does not merge, or a worktree, passes), still be one of the copies when the dialog listed
    /// several, its computer connected, and the agent still set up there.
    fn gx_store_new_coordinator_refusal(
        &self,
        opened_group: &str,
        group_id: &str,
        agent_id: &str,
    ) -> Option<String> {
        const GONE: &str = "That copy is no longer open. Pick another.";
        let Some(project) = ProjectKey::parse_sidebar_group_id(group_id) else {
            return Some(GONE.to_string());
        };
        let open = self
            .gx_store
            .core
            .presentation()
            .loaded(&project.machine)
            .is_some_and(|loaded| loaded.project(&project.project_id).is_some());
        if !open {
            return Some(GONE.to_string());
        }
        if group_id != opened_group
            && let Some(copies) = self
                .gx_store
                .sidebar_list
                .view()
                .project_copies(opened_group)
            && !copies.iter().any(|copy| copy.group_id == group_id)
        {
            return Some(GONE.to_string());
        }
        let agents = match project.machine.remote_id() {
            None => Some(self.gx_store_coordinator_agents()),
            Some(machine_id) => {
                if self
                    .gpui_remote_gxserver_request_target(machine_id)
                    .is_none()
                {
                    return Some(
                        "That computer isn't connected. Pick another computer.".to_string(),
                    );
                }
                self.gx_store_remote_coordinator_agents(machine_id, None)
            }
        };
        if agents.is_some_and(|agents| !agents.iter().any(|agent| agent.agent_id == agent_id)) {
            return Some("That agent isn't set up there anymore. Pick another.".to_string());
        }
        None
    }

    /// Where the playbook of the copy `group_id` names is read and saved: its computer's
    /// gxserver (`None` for this one) and its raw project id; `None` when that computer is not
    /// connected.
    fn new_coordinator_playbook_target(
        &self,
        group_id: &str,
    ) -> Option<(Option<GpuiRemoteGxserverRequestTarget>, String)> {
        let project = ProjectKey::parse_sidebar_group_id(group_id)?;
        let remote = match project.machine.remote_id() {
            None => None,
            Some(machine_id) => Some(self.gpui_remote_gxserver_request_target(machine_id)?),
        };
        Some((remote, project.project_id))
    }

    /// Reads the playbook row and View playbook's guide of the copy `group_id` names, for the
    /// pick `generation`; the dialog drops them if another copy was picked since.
    fn read_new_coordinator_playbook(
        &self,
        group_id: &str,
        generation: u64,
        dialog: &DialogSlot,
        cx: &mut gpui::Context<Self>,
    ) {
        // The machine whose gxserver runs the orchestrator also keeps its playbook; an
        // unconnected remote one shows none.
        let Some((remote, project_id)) = self.new_coordinator_playbook_target(group_id) else {
            return;
        };
        let dialog = dialog.clone();
        cx.spawn(async move |_this, cx| {
            let read = crate::app::gx_store::gx_rpc(
                remote.clone(),
                "/api/readCoordinatorPlaybook",
                serde_json::json!({ "projectId": project_id }),
            )
            .await;
            // An older gxserver without the playbook editor or without overrides leaves the
            // row out.
            let Some(info) = read
                .ok()
                .and_then(|view| OrchestratorPlaybookInfo::from_view(&view))
            else {
                return;
            };
            let Some(view) = dialog_of(&dialog) else {
                return;
            };
            cx.update(|cx| {
                view.update(cx, |dialog, cx| {
                    dialog.set_playbook_for(generation, Some(info), cx)
                })
            });
            // The playbook as `ghostex coordinator guide` prints it for this project, for View
            // playbook. Without a session the read is not recorded as any coordinator's.
            let guide = crate::app::gx_store::gx_rpc(
                remote,
                "/api/readCoordinatorGuide",
                serde_json::json!({ "projectId": project_id }),
            )
            .await
            .map_err(|error| error.message)
            .and_then(|view| {
                view.get("guide")
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_string)
                    .ok_or_else(|| {
                        "This computer's Ghostex could not show its playbook.".to_string()
                    })
            });
            cx.update(|cx| {
                view.update(cx, |dialog, cx| {
                    dialog.set_playbook_guide_for(generation, guide, cx)
                })
            });
        })
        .detach();
    }

    /// Reads the model catalog of the computer the copy `group_id` is on, when it is another one
    /// whose catalog this dialog has not read yet, and gives the copy its agents' lineups.
    fn read_new_coordinator_lineup(
        &self,
        group_id: &str,
        dialog: &DialogSlot,
        catalogs: &CatalogCache,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(machine_id) = ProjectKey::parse_sidebar_group_id(group_id)
            .and_then(|project| project.machine.remote_id().map(str::to_string))
        else {
            return;
        };
        if let Some(catalog) = catalogs.borrow().get(&machine_id) {
            let agents = self.gx_store_remote_coordinator_agents(&machine_id, catalog.as_ref());
            let lineup = if catalog.is_some() {
                CopyLineup::Held
            } else {
                CopyLineup::Missing
            };
            if let Some(view) = dialog_of(dialog) {
                let group_id = group_id.to_string();
                view.update(cx, |dialog, cx| {
                    dialog.set_copy_agents(&group_id, agents, lineup, cx)
                });
            }
            return;
        }
        let read = self.gpui_remote_model_catalog(&machine_id, cx);
        let (dialog, catalogs, group_id) = (dialog.clone(), catalogs.clone(), group_id.to_string());
        cx.spawn(async move |this, cx| {
            let catalog = read.await;
            let _ = this.update(cx, |app, cx| {
                let lineup = if catalog.is_some() {
                    CopyLineup::Held
                } else {
                    CopyLineup::Missing
                };
                let agents = app.gx_store_remote_coordinator_agents(&machine_id, catalog.as_ref());
                catalogs.borrow_mut().insert(machine_id, catalog);
                if let Some(view) = dialog_of(&dialog) {
                    view.update(cx, |dialog, cx| {
                        dialog.set_copy_agents(&group_id, agents, lineup, cx)
                    });
                }
            });
        })
        .detach();
    }

    /// Opens the Make Coordinator dialog for the `makeCoordinator` modal: `sessionId` (the sidebar
    /// row's id) and `sessionTitle`. An open without a session is dropped.
    pub(crate) fn open_gpui_make_coordinator_modal(
        &mut self,
        message: &serde_json::Value,
        cx: &mut gpui::Context<Self>,
    ) {
        let text = |key: &str| {
            message
                .get(key)
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(str::to_string)
        };
        let Some(sidebar_session_id) = text("sessionId") else {
            return;
        };
        let session_title = text("sessionTitle").unwrap_or_else(|| "This session".to_string());
        let palette = self.gpui_native_dialog_palette();
        let host = self.native_app_modal_host(cx, move |app, command, cx| {
            if let MakeCoordinatorModalCommand::Make { goal } = command {
                app.gx_store_promote_coordinator(&sidebar_session_id, &goal, cx);
            }
            app.release_native_app_modal_window(GpuiAppModalKind::MakeCoordinator, cx);
        });
        self.open_native_app_modal(
            GpuiAppModalKind::MakeCoordinator,
            NEW_COORDINATOR_MODAL_WIDTH,
            MAKE_COORDINATOR_MODAL_INITIAL_HEIGHT,
            move |window, cx| {
                cx.new(|cx| {
                    GpuiMakeCoordinatorModalWindow::new(session_title, palette, host, window, cx)
                })
            },
            cx,
        );
    }
}
