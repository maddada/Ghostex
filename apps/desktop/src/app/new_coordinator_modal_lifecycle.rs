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

/// One copy the dialog has listed, remembered so the entry is still found when the copy it was
/// opened for goes.
#[derive(Clone)]
struct KnownCopy {
    group_id: String,
    label: String,
    computer: String,
}

/// What one open dialog keeps between its commands.
#[derive(Default)]
struct CopiesState {
    /// The copy the dialog was opened for.
    opened: String,
    /// Every copy it has listed.
    known: RefCell<Vec<KnownCopy>>,
    /// Each other computer's catalog, once read in this dialog (`None` inside: it serves none).
    /// A read that did not reach the computer is not kept, so the next pick reads again.
    catalogs: RefCell<BTreeMap<String, Option<AgentModelCatalog>>>,
    /// The latest catalog read started for each computer: an earlier read that finishes later is
    /// dropped, so it never overwrites a newer answer.
    catalog_reads: RefCell<BTreeMap<String, u64>>,
}

/// The machine a sidebar group id names: [`LOCAL_MACHINE_ID`] or the remote machine's id.
fn machine_id_of(group_id: &str) -> String {
    ProjectKey::parse_sidebar_group_id(group_id)
        .and_then(|project| project.machine.remote_id().map(str::to_string))
        .unwrap_or_else(|| LOCAL_MACHINE_ID.to_string())
}

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
        let state = Rc::new(CopiesState {
            opened: opened_group.clone(),
            ..CopiesState::default()
        });
        let copies = self.gx_store_new_coordinator_copies(&state);
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
        let host_state = state.clone();
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
                        app.gx_store_new_coordinator_refusal(&host_state, &group_id, &agent_id)
                    {
                        app.refresh_new_coordinator_copies(&host_dialog, &host_state, cx);
                        reply.finish(Err(NewCoordinatorRefusal::plain(reason)), cx);
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
                                // The dialog was closed while it waited: say it here instead,
                                // with no picker to point at, and the computer's words behind
                                // Copy details.
                                Some(refusal) if !delivered => {
                                    let description = if refusal.details.is_some() {
                                        "The computer refused it. Open New Coordinator to try again."
                                    } else {
                                        refusal.message.as_str()
                                    };
                                    let mut toast = serde_json::json!({
                                        "type": "toast",
                                        "level": "warning",
                                        "title": "Coordinator not created",
                                        "description": description,
                                    });
                                    if let Some(details) = &refusal.details {
                                        toast["copyText"] = serde_json::json!(details);
                                    }
                                    app.receive_gpui_app_toast_bridge_message(&toast, cx);
                                }
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
                    app.read_new_coordinator_lineup(&group_id, &host_dialog, &host_state, cx);
                    return;
                }
                NewCoordinatorModalCommand::RefreshCopies => {
                    app.refresh_new_coordinator_copies(&host_dialog, &host_state, cx);
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
            self.read_new_coordinator_lineup(&group_id, &dialog, &state, cx);
        }
    }

    /// The copies an open New Coordinator dialog offers: every copy of the merged project it is
    /// one of (found through any copy it has listed, so the entry survives the copy it was opened
    /// for going), else the copies it listed that are still open, else that project alone. This
    /// computer's copies list its agents with their lineups; another computer's list that
    /// computer's launchers, their lineups once read (`CopiesState::catalogs`).
    fn gx_store_new_coordinator_copies(&self, state: &CopiesState) -> Vec<NewCoordinatorCopy> {
        let catalogs = state.catalogs.borrow();
        let mut known = state.known.borrow_mut();
        let listed = self.new_coordinator_listed_copies(state, &known);
        let copies: Vec<NewCoordinatorCopy> = match listed {
            Some(listed) => listed
                .into_iter()
                .map(|copy| {
                    self.new_coordinator_copy(
                        &copy.group_id,
                        copy.label,
                        copy.computer,
                        &copy.machine_id,
                        copy.connected,
                        &catalogs,
                    )
                })
                .collect(),
            None => {
                let open: Vec<NewCoordinatorCopy> = known
                    .iter()
                    .filter(|copy| self.new_coordinator_copy_is_open(&copy.group_id))
                    .map(|copy| {
                        self.new_coordinator_copy(
                            &copy.group_id,
                            copy.label.clone(),
                            copy.computer.clone(),
                            &machine_id_of(&copy.group_id),
                            true,
                            &catalogs,
                        )
                    })
                    .collect();
                if open.is_empty() {
                    vec![self.new_coordinator_copy(
                        &state.opened,
                        self.new_coordinator_project_title(&state.opened),
                        String::new(),
                        &machine_id_of(&state.opened),
                        true,
                        &catalogs,
                    )]
                } else {
                    open
                }
            }
        };
        for copy in &copies {
            if !known.iter().any(|known| known.group_id == copy.group_id) {
                known.push(KnownCopy {
                    group_id: copy.group_id.clone(),
                    label: copy.label.clone(),
                    computer: copy.computer.clone(),
                });
            }
        }
        copies
    }

    /// The merged entry's copies as the Project list has them now, looked up through the copy
    /// the dialog was opened for or any it has listed.
    fn new_coordinator_listed_copies(
        &self,
        state: &CopiesState,
        known: &[KnownCopy],
    ) -> Option<Vec<ghostex_gx_core::ProjectCopy>> {
        let view = self.gx_store.sidebar_list.view();
        std::iter::once(state.opened.as_str())
            .chain(known.iter().map(|copy| copy.group_id.as_str()))
            .find_map(|group_id| view.project_copies(group_id))
            .map(<[_]>::to_vec)
    }

    /// The project a group id names is still open on its computer (the store's own inventory).
    fn new_coordinator_copy_is_open(&self, group_id: &str) -> bool {
        ProjectKey::parse_sidebar_group_id(group_id).is_some_and(|project| {
            self.gx_store
                .core
                .presentation()
                .loaded(&project.machine)
                .is_some_and(|loaded| loaded.project(&project.project_id).is_some())
        })
    }

    fn new_coordinator_project_title(&self, group_id: &str) -> String {
        ProjectKey::parse_sidebar_group_id(group_id)
            .and_then(|project| {
                let loaded = self.gx_store.core.presentation().loaded(&project.machine)?;
                Some(loaded.project(&project.project_id)?.title.clone())
            })
            .unwrap_or_default()
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

    /// Sends the open dialog the copies as they are now, and reads the models of the picked
    /// copy's computer when they are not known yet (a read that timed out is tried again here).
    fn refresh_new_coordinator_copies(
        &self,
        dialog: &DialogSlot,
        state: &Rc<CopiesState>,
        cx: &mut gpui::Context<Self>,
    ) {
        let Some(view) = dialog_of(dialog) else {
            return;
        };
        let copies = self.gx_store_new_coordinator_copies(state);
        let picked = view.update(cx, |dialog, cx| {
            dialog.set_copies(copies, cx);
            dialog.chosen_group_id().map(str::to_string)
        });
        if let Some(group_id) = picked {
            self.read_new_coordinator_lineup(&group_id, dialog, state, cx);
        }
    }

    /// Why Create must not run in the copy `group_id` names, checked just before it: the project
    /// must still be open on its computer (the store's own inventory, so a project the list
    /// does not merge, or a worktree, passes), still be one of the copies when the dialog listed
    /// several, its computer connected, and the agent still set up there.
    fn gx_store_new_coordinator_refusal(
        &self,
        state: &CopiesState,
        group_id: &str,
        agent_id: &str,
    ) -> Option<String> {
        const GONE: &str = "That copy is no longer open. Pick another.";
        let Some(project) = ProjectKey::parse_sidebar_group_id(group_id) else {
            return Some(GONE.to_string());
        };
        if !self.new_coordinator_copy_is_open(group_id) {
            return Some(GONE.to_string());
        }
        let known = state.known.borrow();
        if known.len() > 1
            && let Some(copies) = self.new_coordinator_listed_copies(state, &known)
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
    /// whose catalog this dialog has not read yet, and gives the copy its agents' lineups. A
    /// computer that was not reached gets none for now and is read again on the next pick or
    /// refresh.
    fn read_new_coordinator_lineup(
        &self,
        group_id: &str,
        dialog: &DialogSlot,
        state: &Rc<CopiesState>,
        cx: &mut gpui::Context<Self>,
    ) {
        let machine_id = machine_id_of(group_id);
        if machine_id == LOCAL_MACHINE_ID {
            return;
        }
        let cached = state.catalogs.borrow().get(&machine_id).cloned();
        if let Some(catalog) = cached {
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
        let generation = {
            let mut reads = state.catalog_reads.borrow_mut();
            let generation = reads.get(&machine_id).copied().unwrap_or(0) + 1;
            reads.insert(machine_id.clone(), generation);
            generation
        };
        let read = self.gpui_remote_model_catalog(&machine_id, cx);
        let (dialog, state, group_id) = (dialog.clone(), state.clone(), group_id.to_string());
        cx.spawn(async move |this, cx| {
            let read = read.await;
            if state.catalog_reads.borrow().get(&machine_id) != Some(&generation) {
                return;
            }
            let _ = this.update(cx, |app, cx| {
                let catalog = match read {
                    RemoteModelCatalog::Served(catalog) => Some(Some(catalog)),
                    RemoteModelCatalog::NoneServed => Some(None),
                    // Not reached this time: an answer already held stays.
                    RemoteModelCatalog::Unreachable => {
                        state.catalogs.borrow().get(&machine_id).cloned()
                    }
                };
                let lineup = match &catalog {
                    Some(Some(_)) => CopyLineup::Held,
                    _ => CopyLineup::Missing,
                };
                let agents = app.gx_store_remote_coordinator_agents(
                    &machine_id,
                    catalog.as_ref().and_then(Option::as_ref),
                );
                if let Some(catalog) = catalog {
                    state.catalogs.borrow_mut().insert(machine_id, catalog);
                }
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
