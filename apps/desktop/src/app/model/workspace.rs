// C1 wave-3 extraction: the WorkspaceModel sub-model struct and impl moved verbatim out of main.rs (pure
// move, no logic changes; items made pub(crate) so main.rs and sibling
// modules can still reach them). See docs/2026-08-22/repo-restructure/SPLITS.md C1.

use crate::*;

#[derive(Clone)]
pub(crate) struct WorkspaceModel {
    pub(crate) terminal_sessions: Vec<TerminalSession>,
    pub(crate) root: WorkspaceNode,
    pub(crate) focused_pane: WorkspacePaneId,
    pub(crate) focus_mode_pane: Option<WorkspacePaneId>,
    pub(crate) next_pane_id: u64,
    pub(crate) next_split_id: u64,
    pub(crate) next_session_id: u64,
}

impl WorkspaceModel {
    pub(crate) fn empty_default() -> Self {
        let pane_id = WorkspacePaneId(1);
        Self {
            terminal_sessions: Vec::new(),
            root: workspace_empty_leaf_node(pane_id),
            focused_pane: pane_id,
            focus_mode_pane: None,
            next_pane_id: 2,
            next_split_id: 1,
            next_session_id: 1,
        }
    }

    #[allow(dead_code)] // no caller: the CDXC:GPUIWorkspaceLifecycle sample workspace is not built at startup any more
    pub(crate) fn first_slice_default() -> Self {
        /*
        CDXC:GPUIWorkspaceLifecycle 2026-06-22-05:23:
        Agents terminal tabs need explicit user-facing presentation states before the runtime lifecycle exists. Running, sleeping, mounting, failed startup, restored/unmounted, and popped-out placeholder sessions stay in the same tab/split layout tree so tab selection can show the correct body state without deleting, waking, or hiding sessions.

        CDXC:GPUIAgentsTabStatus 2026-06-22-23:52:
        The default GPUI Agents workspace must visibly exercise non-idle semantic running-tab indicators while terminal bodies remain black placeholders: working, attention, and Delayed Send. Idle running tabs render without a status dot; lifecycle placeholder samples remain separate from running activity.
        */
        let terminal_sessions = vec![
            TerminalSession::placeholder(
                TerminalSessionId(1),
                "Agent".to_string(),
                TerminalSessionPresentationState::Running,
            )
            .with_agent_icon(Some("codex")),
            TerminalSession::placeholder(
                TerminalSessionId(2),
                "Build".to_string(),
                TerminalSessionPresentationState::Running,
            )
            .with_agent_icon(Some("codex"))
            .with_activity(AgentTerminalActivity::Working),
            TerminalSession::placeholder(
                TerminalSessionId(3),
                "Review".to_string(),
                TerminalSessionPresentationState::Running,
            )
            .with_agent_icon(Some("claude"))
            .with_activity(AgentTerminalActivity::Attention),
            TerminalSession::placeholder(
                TerminalSessionId(4),
                "Delayed Send".to_string(),
                TerminalSessionPresentationState::Running,
            )
            .with_agent_icon(Some("codex"))
            .with_activity(AgentTerminalActivity::Working)
            .with_delayed_send_active(true),
            TerminalSession::placeholder(
                TerminalSessionId(5),
                "Sleeping".to_string(),
                TerminalSessionPresentationState::Sleeping,
            ),
            TerminalSession::placeholder(
                TerminalSessionId(6),
                "Shell".to_string(),
                TerminalSessionPresentationState::Mounting,
            ),
            TerminalSession::placeholder(
                TerminalSessionId(7),
                "Restored".to_string(),
                TerminalSessionPresentationState::RestoredUnmounted,
            ),
            TerminalSession::placeholder(
                TerminalSessionId(8),
                "Detached".to_string(),
                TerminalSessionPresentationState::PoppedOutPlaceholder,
            ),
        ];
        let pane_id = WorkspacePaneId(1);
        let active_tab = terminal_sessions[0].id;
        let tabs = terminal_sessions
            .iter()
            .map(|session| WorkspaceTab {
                session_id: session.id,
            })
            .collect();

        /*
        CDXC:GPUIWorkspaceLayout 2026-06-22-05:11:
        Agents mode needs a GPUI-owned terminal workspace model before libghostty is mounted. Seed multiple terminal sessions into one tab group so ordinary sessions preserve tab order, active tab, and pane ownership instead of creating implicit split panes.
        */
        Self {
            terminal_sessions,
            root: WorkspaceNode::Leaf(WorkspaceLeaf {
                pane_id,
                tab_group: WorkspaceTabGroup { tabs, active_tab },
            }),
            focused_pane: pane_id,
            focus_mode_pane: None,
            next_pane_id: 2,
            next_split_id: 1,
            next_session_id: 9,
        }
    }

    pub(crate) fn session(&self, id: TerminalSessionId) -> Option<&TerminalSession> {
        self.terminal_sessions
            .iter()
            .find(|session| session.id == id)
    }

    pub(crate) fn has_session(&self, id: TerminalSessionId) -> bool {
        self.session(id).is_some()
    }

    pub(crate) fn terminal_session_ids(&self) -> Vec<TerminalSessionId> {
        self.terminal_sessions
            .iter()
            .map(|session| session.id)
            .collect()
    }

    pub(crate) fn session_is_mounting(&self, session_id: TerminalSessionId) -> bool {
        self.session(session_id).is_some_and(|session| {
            session.presentation_state == TerminalSessionPresentationState::Mounting
        })
    }

    #[allow(dead_code)] // no live caller: startup eligibility is decided by the surface-host mount path
    pub(crate) fn make_mounting_session_startup_eligible(
        &mut self,
        session_id: TerminalSessionId,
    ) -> bool {
        let Some(session) = self
            .terminal_sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        else {
            return false;
        };
        if session.presentation_state != TerminalSessionPresentationState::Mounting {
            return false;
        }

        session.set_presentation_state_with_startup_eligibility(
            TerminalSessionPresentationState::Mounting,
            true,
        );
        true
    }

    pub(crate) fn transition_terminal_session_presentation_state(
        &mut self,
        session_id: TerminalSessionId,
        expected_state: TerminalSessionPresentationState,
        next_state: TerminalSessionPresentationState,
    ) -> bool {
        self.terminal_sessions
            .iter_mut()
            .find(|session| session.id == session_id)
            .is_some_and(|session| {
                if session.presentation_state != expected_state {
                    return false;
                }

                session.set_presentation_state(next_state);
                true
            })
    }

    pub(crate) fn visible_selected_mounting_startup_candidates(
        &self,
    ) -> Vec<AgentsTerminalStartupRecord> {
        /*
        CDXC:GPUITerminalStartupBoundary 2026-06-22-23:50:
        Only rendered Agents leaves whose selected shell session is startup-eligible Mounting are startup candidates. Inactive tabs, hidden Focus-mode leaves, Running mount slots, failed placeholders, sleeping/restored/popped-out placeholders before activation, sleeping wake and popped-out reattach activations, restored presentation-only Mounting sessions, and missing-session tabs must not create startup records, duplicate popped-out runtimes, or real mount slots. Explicit restored-unmounted activation is the materialization exception and enters the existing startup pipeline.
        */
        self.rendered_leaf_order()
            .into_iter()
            .filter_map(|pane_id| {
                let leaf = self.find_leaf(pane_id)?;
                let shell_session_id = leaf.tab_group.active_session_id()?;
                self.session(shell_session_id)
                    .is_some_and(TerminalSession::can_enter_startup_pipeline)
                    .then_some(AgentsTerminalStartupRecord {
                        pane_id: leaf.pane_id,
                        shell_session_id,
                        startup_body_geometry_available: false,
                    })
            })
            .collect()
    }

    pub(crate) fn rendered_terminal_startup_body_slots(
        &self,
    ) -> Vec<AgentsTerminalStartupBodySlotId> {
        /*
        CDXC:GPUITerminalStartupGeometry 2026-06-23-00:10:
        Startup body slots identify only visible selected startup-eligible Mounting Agents terminal bodies for runtime launch preparation. They intentionally do not reuse `AgentsTerminalBodyMountSlotId`, because real Ghostty mount slots, Running host maps, and surface owners must remain restricted to visible selected Running sessions. Explicit restored-unmounted materialization may get a startup body slot; sleeping wake, popped-out reattach, and restored presentation-only Mounting after restart must not get hidden startup hosts.
        */
        self.rendered_leaf_order()
            .into_iter()
            .filter_map(|pane_id| {
                let leaf = self.find_leaf(pane_id)?;
                let session_id = leaf.tab_group.active_session_id()?;
                self.session(session_id)
                    .is_some_and(TerminalSession::can_enter_startup_pipeline)
                    .then_some(AgentsTerminalStartupBodySlotId {
                        pane_id: leaf.pane_id,
                        session_id,
                    })
            })
            .collect()
    }

    pub(crate) fn rendered_terminal_parked_owner_body_slots(
        &self,
    ) -> Vec<AgentsTerminalBodyMountSlotId> {
        /*
        CDXC:GPUTerminalParkedOwnerReattach 2026-06-23-19:41:
        Parked-owner reattach geometry is recorded only for visible selected Mounting Agents bodies that are not startup-eligible. This keeps sleeping wake and popped-out reattach out of startup maps while giving the runtime owner-transfer path the current body rectangle it needs before it can honestly move an exact parked owner back to Running.
        */
        let rendered_leaf_order = self.rendered_leaf_order();
        rendered_leaf_order
            .into_iter()
            .filter_map(|pane_id| {
                let leaf = self.find_leaf(pane_id)?;
                let session_id = leaf.tab_group.active_session_id()?;
                self.session(session_id)
                    .is_some_and(|session| {
                        session.presentation_state == TerminalSessionPresentationState::Mounting
                            && !session.can_enter_startup_pipeline()
                    })
                    .then_some(AgentsTerminalBodyMountSlotId {
                        pane_id: leaf.pane_id,
                        session_id,
                    })
            })
            .collect()
    }

    pub(crate) fn is_current_terminal_parked_owner_body_slot(
        &self,
        slot_id: AgentsTerminalBodyMountSlotId,
    ) -> bool {
        self.rendered_terminal_parked_owner_body_slots()
            .into_iter()
            .any(|current_slot_id| current_slot_id == slot_id)
    }

    pub(crate) fn is_current_terminal_startup_body_slot(
        &self,
        slot_id: AgentsTerminalStartupBodySlotId,
    ) -> bool {
        self.rendered_terminal_startup_body_slots()
            .into_iter()
            .any(|current_slot_id| current_slot_id == slot_id)
    }

    pub(crate) fn terminal_body_mount_candidate(
        &self,
        leaf: &WorkspaceLeaf,
    ) -> AgentsTerminalBodyMountCandidate {
        let rendered_leaf_order = self.rendered_leaf_order();
        selected_agents_terminal_body_mount_candidate(
            leaf,
            &self.terminal_sessions,
            &rendered_leaf_order,
        )
    }

    pub(crate) fn rendered_terminal_body_mount_slots(&self) -> Vec<AgentsTerminalBodyMountSlotId> {
        /*
        CDXC:GPUILibghosttyMountBoundary 2026-06-22-22:45:
        The pure all-visible mount-slot rule returns every rendered Agents leaf whose selected tab is Running. Focus mode hides leaves by narrowing rendered_leaf_order, inactive tabs never appear here, and the helper remains model-only so rendering cannot invent fallback surfaces or persisted runtime ids.
        */
        let rendered_leaf_order = self.rendered_leaf_order();
        rendered_leaf_order
            .into_iter()
            .filter_map(|pane_id| {
                let leaf = self.find_leaf(pane_id)?;
                let session_id = leaf.tab_group.active_session_id()?;
                self.session(session_id)
                    .is_some_and(|session| {
                        session.presentation_state == TerminalSessionPresentationState::Running
                    })
                    .then_some(AgentsTerminalBodyMountSlotId {
                        pane_id: leaf.pane_id,
                        session_id,
                    })
            })
            .collect()
    }

    pub(crate) fn is_current_terminal_body_mount_slot(
        &self,
        slot_id: AgentsTerminalBodyMountSlotId,
    ) -> bool {
        self.rendered_terminal_body_mount_slots()
            .into_iter()
            .any(|current_slot_id| current_slot_id == slot_id)
    }

    pub(crate) fn focus_pane(&mut self, pane_id: WorkspacePaneId) {
        if self.find_leaf_mut(pane_id).is_some() {
            self.focused_pane = pane_id;
            self.acknowledge_attention_for_active_session_in_pane(pane_id);
        }
    }

    pub(crate) fn select_tab(&mut self, pane_id: WorkspacePaneId, session_id: TerminalSessionId) {
        let tab_selected = self.find_leaf_mut(pane_id).is_some_and(|leaf| {
            if leaf.tab_group.has_session(session_id) {
                leaf.tab_group.active_tab = session_id;
                true
            } else {
                false
            }
        });

        if tab_selected {
            self.focused_pane = pane_id;
            self.acknowledge_attention_for_session_activation(session_id);
        }
    }

    pub(crate) fn active_session_in_pane(
        &self,
        pane_id: WorkspacePaneId,
    ) -> Option<TerminalSessionId> {
        self.find_leaf(pane_id)
            .and_then(|leaf| leaf.tab_group.active_session_id())
    }

    pub(crate) fn active_session_in_pane_has_attention(&self, pane_id: WorkspacePaneId) -> bool {
        self.active_session_in_pane(pane_id)
            .and_then(|session_id| self.session(session_id))
            .is_some_and(|session| session.activity == AgentTerminalActivity::Attention)
    }

    pub(crate) fn acknowledge_attention_for_session_activation(
        &mut self,
        session_id: TerminalSessionId,
    ) -> bool {
        let Some(session) = self
            .terminal_sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        else {
            return false;
        };
        if session.activity != AgentTerminalActivity::Attention {
            return false;
        }
        session.activity = AgentTerminalActivity::Idle;
        true
    }

    pub(crate) fn acknowledge_attention_for_active_session_in_pane(
        &mut self,
        pane_id: WorkspacePaneId,
    ) -> bool {
        let Some(session_id) = self.active_session_in_pane(pane_id) else {
            return false;
        };
        self.acknowledge_attention_for_session_activation(session_id)
    }

    pub(crate) fn session_belongs_to_pane(
        &self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
    ) -> bool {
        self.session(session_id).is_some()
            && self
                .find_leaf(pane_id)
                .is_some_and(|leaf| leaf.tab_group.has_session(session_id))
    }

    pub(crate) fn pane_id_for_session(
        &self,
        session_id: TerminalSessionId,
    ) -> Option<WorkspacePaneId> {
        self.leaf_order().into_iter().find(|pane_id| {
            self.find_leaf(*pane_id)
                .is_some_and(|leaf| leaf.tab_group.has_session(session_id))
        })
    }

    pub(crate) fn activate_terminal_placeholder_session(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
    ) -> bool {
        /*
        CDXC:GPUIAgentsTerminalActivation 2026-06-22-23:33:
        Agents terminal tab selection must not auto-wake sleeping/restored/popped-out sessions or auto-retry failed startup sessions, but activating the selected placeholder body or card button should move those presentations into Mounting so wake, materialize, reattach, and retry stay honest pending runtime-startup states. Existing Running sessions remain Running, Mounting remains pending, and this slice must not launch a process, synthesize terminal success, persist runtime data, or create terminal content.

        CDXC:GPUITerminalActivationRuntimeGuard 2026-06-23-18:00:
        Sleep and popped-out activation are not new-terminal startup and must stay blocked from hidden startup host/surface creation. Slice 236 handles them only through exact parked-owner transfer, while failed startup retry and explicit restored-unmounted materialization are the placeholder activations that may reuse the startup pipeline.

        CDXC:GPUITerminalStartupRetryIdentity 2026-06-23-18:19:
        The durable workspace model marks explicit failed-startup retry and restored-unmounted materialization as startup-eligible Mounting. Process-local retry attempt id rotation belongs to the app/runtime helper and remains limited to failed-startup retry so shell state never owns or persists runtime ids.

        CDXC:GPUITerminalRestoredMaterialization 2026-06-23-19:26:
        Explicit restored-unmounted activation materializes through startup-eligible Mounting using the durable shell session's existing process-local runtime id. It must not rotate a retry runtime id, and tab selection alone remains presentation-only.
        */
        if self.session(session_id).is_none() {
            return false;
        }

        let selection_changed = {
            let Some(leaf) = self.find_leaf_mut(pane_id) else {
                return false;
            };
            if !leaf.tab_group.has_session(session_id) {
                return false;
            }

            let selection_changed = leaf.tab_group.active_tab != session_id;
            leaf.tab_group.active_tab = session_id;
            selection_changed
        };

        let focus_changed = self.focused_pane != pane_id;
        self.focused_pane = pane_id;

        let presentation_changed = self
            .terminal_sessions
            .iter_mut()
            .find(|session| session.id == session_id)
            .is_some_and(|session| {
                if let Some(next_state) = session.presentation_state.activation_pending_state() {
                    let startup_eligible = matches!(
                        session.presentation_state,
                        TerminalSessionPresentationState::StartupFailed
                            | TerminalSessionPresentationState::RestoredUnmounted
                    );
                    session.set_presentation_state_with_startup_eligibility(
                        next_state,
                        startup_eligible,
                    );
                    true
                } else {
                    false
                }
            });

        selection_changed || focus_changed || presentation_changed
    }

    pub(crate) fn cycle_tab_in_pane(&mut self, pane_id: WorkspacePaneId, reverse: bool) -> bool {
        /*
        CDXC:GPUIKeyboardFocus 2026-06-22-06:02:
        Shell tab cycling must operate inside the focused Agents tab group and include sleeping, mounting, failed-startup, restored/unmounted, and popped-out placeholders as ordinary tabs. Cycling changes only the active tab id; it must not wake, mount, materialize, retry, or reattach placeholder sessions.
        */
        let Some(leaf) = self.find_leaf_mut(pane_id) else {
            return false;
        };
        leaf.tab_group.cycle_active_session(reverse).is_some()
    }

    #[allow(dead_code)] // no caller: tab closing goes through the id-addressed close paths
    pub(crate) fn close_active_tab(&mut self) -> bool {
        /*
        CDXC:GPUIKeyboardFocus 2026-06-22-06:02:
        Closing an Agents placeholder tab is shell state only in this slice. Remove the active tab from the focused tab group, select a neighbor when possible, and collapse an emptied split branch.

        CDXC:GPUIWorkspaceLifecycle 2026-06-26-05:23:
        GPUI Agents close must match the macOS workspace: closing the last visible terminal is a real close that leaves an empty workspace pane instead of preserving a fake sleeping or final-root terminal.
        */
        let pane_id = self.focused_pane;
        let Some(session_id) = self
            .find_leaf(pane_id)
            .and_then(|leaf| leaf.tab_group.active_session_id())
        else {
            return false;
        };
        self.close_tab(pane_id, session_id)
    }

    pub(crate) fn close_tab(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
    ) -> bool {
        if !self.session_belongs_to_pane(pane_id, session_id) {
            return false;
        }
        let Some((_tab, source_is_empty)) = self.remove_tab_for_move(pane_id, session_id) else {
            return false;
        };
        self.terminal_sessions
            .retain(|session| session.id != session_id);

        if self.terminal_sessions.is_empty() {
            self.root = workspace_empty_leaf_node(pane_id);
            self.focused_pane = pane_id;
            self.focus_mode_pane = None;
            return true;
        }
        if source_is_empty {
            self.collapse_empty_leaf(pane_id);
        }
        self.normalize_workspace_tree();
        true
    }

    pub(crate) fn close_tab_from_direct_tab_close(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
    ) -> bool {
        /*
        CDXC:GPUIAgentsWorkspaceTabs 2026-06-26-06:57:
        Direct Agents tab close mirrors native pane-tab close: select the clicked tab before removal so inactive tab close elects the right sibling, then left sibling, from the close target instead of from a previously active tab. Scoped Close Right/Left/Others keep their no-focus native menu semantics and call `close_tab` directly.
        */
        if !self.session_belongs_to_pane(pane_id, session_id) {
            return false;
        }
        self.select_tab(pane_id, session_id);
        self.close_tab(pane_id, session_id)
    }

    pub(crate) fn selected_session_after_direct_tab_close(
        &self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
    ) -> Option<TerminalSessionId> {
        /*
        CDXC:GPUIWorkspaceLifecycle 2026-06-26-07:25:
        GPUI must tell the sidebar runtime which pane-local session should become focused after a direct native tab Close. Simulate the existing direct-close reducer on a clone so the asynchronous sidebar cleanup receives the same right-then-left or surviving-pane target that the local workspace applies immediately.
        */
        let mut next = self.clone();
        if !next.close_tab_from_direct_tab_close(pane_id, session_id) {
            return None;
        }
        next.find_leaf(next.focused_pane)
            .and_then(|leaf| leaf.tab_group.active_session_id())
    }

    pub(crate) fn tab_session_ids_for_close_scope(
        &self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        scope: AgentsWorkspaceTabCloseScope,
    ) -> Vec<TerminalSessionId> {
        /*
        CDXC:GPUIAgentsTabContextMenu 2026-06-26-06:57:
        Agents tab context-menu close scopes resolve only inside the clicked workspace pane tab group, matching macOS paneLayout behavior. The resolver uses ids only and never crosses split panes, command tabs, Browser tabs, project-editor surfaces, titles, paths, command text, terminal output, or persisted gxserver metadata.
        */
        let Some(leaf) = self.find_leaf(pane_id) else {
            return Vec::new();
        };
        let tab_session_ids = leaf
            .tab_group
            .tabs
            .iter()
            .map(|tab| tab.session_id)
            .collect::<Vec<_>>();
        let Some(tab_index) = tab_session_ids
            .iter()
            .position(|candidate| *candidate == session_id)
        else {
            return Vec::new();
        };

        match scope {
            AgentsWorkspaceTabCloseScope::Close => vec![session_id],
            AgentsWorkspaceTabCloseScope::CloseLeft => tab_session_ids[..tab_index].to_vec(),
            AgentsWorkspaceTabCloseScope::CloseOthers => tab_session_ids
                .into_iter()
                .filter(|candidate| *candidate != session_id)
                .collect(),
            AgentsWorkspaceTabCloseScope::CloseRight => tab_session_ids[tab_index + 1..].to_vec(),
        }
    }

    pub(crate) fn tab_session_ids_for_sleep_scope(
        &self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        scope: AgentsWorkspaceTabSleepScope,
    ) -> Vec<TerminalSessionId> {
        /*
        CDXC:GPUIAgentsTabSleep 2026-06-26-06:57:
        Agents tab Sleep scopes use the same pane-local sibling list as native pane tabs. Sleeping tabs remain in the layout, so the resolver returns ids only and leaves lifecycle mutation, mounted-owner parking, and focus replacement to the explicit sleep path.
        */
        let Some(leaf) = self.find_leaf(pane_id) else {
            return Vec::new();
        };
        let tab_session_ids = leaf
            .tab_group
            .tabs
            .iter()
            .map(|tab| tab.session_id)
            .collect::<Vec<_>>();
        let Some(tab_index) = tab_session_ids
            .iter()
            .position(|candidate| *candidate == session_id)
        else {
            return Vec::new();
        };

        match scope {
            AgentsWorkspaceTabSleepScope::Sleep => vec![session_id],
            AgentsWorkspaceTabSleepScope::SleepLeft => tab_session_ids[..tab_index].to_vec(),
            AgentsWorkspaceTabSleepScope::SleepOthers => tab_session_ids
                .into_iter()
                .filter(|candidate| *candidate != session_id)
                .collect(),
            AgentsWorkspaceTabSleepScope::SleepRight => tab_session_ids[tab_index + 1..].to_vec(),
        }
    }

    pub(crate) fn set_session_sleeping(
        &mut self,
        session_id: TerminalSessionId,
        is_sleeping: bool,
    ) -> bool {
        let Some(session) = self
            .terminal_sessions
            .iter_mut()
            .find(|session| session.id == session_id)
        else {
            return false;
        };
        let next_state = if is_sleeping {
            TerminalSessionPresentationState::Sleeping
        } else {
            TerminalSessionPresentationState::Mounting
        };
        if session.presentation_state == next_state {
            return false;
        }
        if !is_sleeping && session.presentation_state != TerminalSessionPresentationState::Sleeping
        {
            return false;
        }

        /*
        CDXC:GPUIAgentsTabSleep 2026-06-26-06:57:
        Agents Sleep is a shell lifecycle mutation that parks the current terminal owner through the existing Sleeping presentation state. Preserve delayed-send intent but clear visible work/attention activity, and never create startup launch payloads, fallback Running state, command text, paths, terminal output, logs, or persistent gxserver transition data from this model helper.
        */
        session.set_presentation_state_with_startup_eligibility(next_state, false);
        if is_sleeping {
            session.activity = AgentTerminalActivity::Idle;
        }
        true
    }

    pub(crate) fn select_replacement_after_direct_tab_sleep(
        &mut self,
        pane_id: WorkspacePaneId,
        slept_session_id: TerminalSessionId,
    ) -> bool {
        /*
        CDXC:GPUIAgentsTabSleep 2026-06-26-06:57:
        Direct native pane-tab Sleep uses the clicked tab group as transition origin: if the active clicked tab goes sleeping, choose the next awake right sibling, then left sibling, and leave all-sleeping groups selected on the sleeping placeholder. Sibling scoped Sleep rows intentionally do not retarget focus.
        */
        let Some(replacement_session_id) =
            self.replacement_session_after_direct_tab_sleep(pane_id, slept_session_id)
        else {
            return false;
        };
        let Some(leaf) = self.find_leaf_mut(pane_id) else {
            return false;
        };
        if leaf.tab_group.active_tab == replacement_session_id {
            return false;
        }
        leaf.tab_group.active_tab = replacement_session_id;
        true
    }

    pub(crate) fn replacement_session_after_direct_tab_sleep(
        &self,
        pane_id: WorkspacePaneId,
        slept_session_id: TerminalSessionId,
    ) -> Option<TerminalSessionId> {
        /*
        CDXC:GPUIWorkspaceLifecycle 2026-06-26-07:25:
        Direct native tab Sleep uses pane-tab transition origin only when the slept tab is currently active, then selects the next awake right sibling before left siblings. Keep this pure helper shared with the sidebar lifecycle bridge so Rust reports the same replacement target it will later apply locally.
        */
        let leaf = self.find_leaf(pane_id)?;
        if leaf.tab_group.active_session_id() != Some(slept_session_id) {
            return None;
        }
        let tab_session_ids = leaf
            .tab_group
            .tabs
            .iter()
            .map(|tab| tab.session_id)
            .collect::<Vec<_>>();
        let slept_index = tab_session_ids
            .iter()
            .position(|candidate| *candidate == slept_session_id)?;
        tab_session_ids[slept_index + 1..]
            .iter()
            .chain(tab_session_ids[..slept_index].iter().rev())
            .copied()
            .find(|candidate| {
                self.session(*candidate).is_some_and(|session| {
                    session.presentation_state != TerminalSessionPresentationState::Sleeping
                })
            })
    }

    pub(crate) fn can_close_tab(
        &self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
    ) -> bool {
        self.session_belongs_to_pane(pane_id, session_id)
    }

    pub(crate) fn can_transfer_tab_to_command_pane(
        &self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
    ) -> bool {
        /*
        CDXC:GPUICommandPaneDragDrop 2026-06-26-05:23:
        Closing the final Agents tab is allowed for real close parity, but Agents-to-command transfers still require a surviving Agents tab or pane so the transaction cannot move the whole workspace into the command panel.
        */
        let Some(leaf) = self.find_leaf(pane_id) else {
            return false;
        };
        leaf.tab_group.has_session(session_id)
            && (leaf.tab_group.tabs.len() > 1 || self.leaf_order().len() > 1)
    }

    pub(crate) fn leaf_order(&self) -> Vec<WorkspacePaneId> {
        let mut pane_ids = Vec::new();
        collect_workspace_leaf_ids(&self.root, &mut pane_ids);
        pane_ids
    }

    pub(crate) fn rendered_leaf_order(&self) -> Vec<WorkspacePaneId> {
        if let Some(pane_id) = self.focus_mode_pane
            && self.find_leaf(pane_id).is_some()
        {
            return vec![pane_id];
        }
        self.leaf_order()
    }

    pub(crate) fn toggle_focus_mode_from_tab_double_click(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
    ) -> bool {
        /*
        CDXC:GPUIFocusMode 2026-06-22-17:00:
        Agents workspace tab double-click parity must select and focus the clicked tab group before changing Focus mode. The first double-click zooms that pane when it has a visible non-sleeping placeholder, a second double-click exits, and sleeping-only panes remain selected/focused without waking or materializing terminals.

        CDXC:GPUIFocusMode 2026-06-22-17:00:
        If this helper is invoked for a different pane while Focus mode is active, keep the existing toggle semantics: select the requested tab first, then clear Focus mode instead of jumping directly into a different zoomed pane.
        */
        if !self.session_belongs_to_pane(pane_id, session_id) {
            return false;
        }

        let focused_pane_before = self.focused_pane;
        let active_session_before = self
            .find_leaf(pane_id)
            .and_then(|leaf| leaf.tab_group.active_session_id());
        let focus_mode_pane_before = self.focus_mode_pane;

        self.select_tab(pane_id, session_id);
        let focus_mode_toggled = self.toggle_focus_mode();

        focus_mode_toggled
            || focused_pane_before != self.focused_pane
            || active_session_before != Some(session_id)
            || focus_mode_pane_before != self.focus_mode_pane
    }

    pub(crate) fn toggle_focus_mode(&mut self) -> bool {
        /*
        CDXC:GPUIFocusMode 2026-06-22-06:02:
        Agents Focus mode is a reversible in-memory zoom of the focused rendered tab group. It stores only the focused pane id, renders that leaf as the workspace body, and toggles back to the unmodified split tree; sleeping-only panes do not count toward Focus-mode availability.
        */
        if self.focus_mode_pane.take().is_some() {
            return true;
        }

        if self.focus_mode_eligible_leaf_count() <= 1
            || !self.leaf_is_focus_mode_eligible(self.focused_pane)
        {
            return false;
        }

        self.focus_mode_pane = Some(self.focused_pane);
        true
    }

    pub(crate) fn focus_mode_eligible_leaf_count(&self) -> usize {
        self.leaf_order()
            .into_iter()
            .filter(|pane_id| self.leaf_is_focus_mode_eligible(*pane_id))
            .count()
    }

    pub(crate) fn leaf_is_focus_mode_eligible(&self, pane_id: WorkspacePaneId) -> bool {
        let Some(leaf) = self.find_leaf(pane_id) else {
            return false;
        };
        leaf.tab_group.tabs.iter().any(|tab| {
            self.session(tab.session_id)
                .is_some_and(|session| session.presentation_state.counts_as_focus_mode_visible())
        })
    }

    pub(crate) fn clear_focus_mode_if_invalid(&mut self) {
        if self
            .focus_mode_pane
            .is_some_and(|pane_id| self.find_leaf(pane_id).is_none())
        {
            self.focus_mode_pane = None;
        }
    }

    pub(crate) fn normalize_workspace_tree(&mut self) -> bool {
        /*
        CDXC:GPUIAgentsWorkspaceNormalize 2026-07-08:
        Agents split layout normalization is a model invariant, not render filtering. Prune tabs whose shell sessions no longer exist, collapse empty leaves by unwrapping their split branch, repair stale active/focus ids, and keep the single empty leaf only for the whole-empty workspace baseline.
        */
        let valid_session_ids = self
            .terminal_sessions
            .iter()
            .map(|session| session.id)
            .collect::<HashSet<_>>();

        if valid_session_ids.is_empty() {
            let pane_id = self.focused_pane;
            let mut changed = !workspace_node_is_empty_leaf_for_pane(&self.root, pane_id);
            if self.focus_mode_pane.take().is_some() {
                changed = true;
            }
            self.root = workspace_empty_leaf_node(pane_id);
            return changed;
        }

        let focus_replacement =
            workspace_close_focus_replacement_leaf_id(&self.root, self.focused_pane);
        let mut changed = false;
        let root = std::mem::replace(&mut self.root, workspace_dummy_node());
        self.root = normalize_workspace_node(root, &valid_session_ids, &mut changed)
            .unwrap_or_else(|| {
                changed = true;
                workspace_leaf_node_from_session_ids(self.focused_pane, self.terminal_session_ids())
            });
        changed |= self.append_unassigned_terminal_sessions_to_workspace();

        let focused_leaf_has_tabs = self
            .find_leaf(self.focused_pane)
            .is_some_and(|leaf| !leaf.tab_group.tabs.is_empty());
        if !focused_leaf_has_tabs
            && let Some(next_focus) = focus_replacement
                .filter(|pane_id| {
                    self.find_leaf(*pane_id)
                        .is_some_and(|leaf| !leaf.tab_group.tabs.is_empty())
                })
                .or_else(|| first_workspace_leaf_id(&self.root))
            && self.focused_pane != next_focus
        {
            self.focused_pane = next_focus;
            changed = true;
        }

        let focus_mode_before = self.focus_mode_pane;
        self.clear_focus_mode_if_invalid();
        changed || self.focus_mode_pane != focus_mode_before
    }

    pub(crate) fn append_unassigned_terminal_sessions_to_workspace(&mut self) -> bool {
        let mut assigned_session_ids = Vec::new();
        collect_workspace_node_session_ids(&self.root, &mut assigned_session_ids);
        let assigned_session_ids = assigned_session_ids.into_iter().collect::<HashSet<_>>();
        let unassigned_session_ids = self
            .terminal_sessions
            .iter()
            .map(|session| session.id)
            .filter(|session_id| !assigned_session_ids.contains(session_id))
            .collect::<Vec<_>>();

        if unassigned_session_ids.is_empty() {
            return false;
        }

        let target_pane_id = self
            .resolve_action_pane_id(self.focused_pane)
            .or_else(|| first_workspace_leaf_id(&self.root))
            .unwrap_or(self.focused_pane);
        let Some(target_leaf) = self.find_leaf_mut(target_pane_id) else {
            self.root =
                workspace_leaf_node_from_session_ids(target_pane_id, unassigned_session_ids);
            self.focused_pane = target_pane_id;
            self.focus_mode_pane = None;
            return true;
        };

        let active_tab_is_valid = target_leaf
            .tab_group
            .tabs
            .iter()
            .any(|tab| tab.session_id == target_leaf.tab_group.active_tab);
        target_leaf.tab_group.tabs.extend(
            unassigned_session_ids
                .into_iter()
                .map(|session_id| WorkspaceTab { session_id }),
        );
        if !active_tab_is_valid && let Some(first_tab) = target_leaf.tab_group.tabs.first() {
            target_leaf.tab_group.active_tab = first_tab.session_id;
        }
        true
    }

    pub(crate) fn find_leaf_mut(&mut self, pane_id: WorkspacePaneId) -> Option<&mut WorkspaceLeaf> {
        find_workspace_leaf_mut(&mut self.root, pane_id)
    }

    pub(crate) fn find_leaf(&self, pane_id: WorkspacePaneId) -> Option<&WorkspaceLeaf> {
        find_workspace_leaf(&self.root, pane_id)
    }

    pub(crate) fn split_ratio(&self, split_id: WorkspaceSplitId) -> Option<f32> {
        find_workspace_split(&self.root, split_id).map(|split| workspace_split_ratio(split.ratio))
    }

    pub(crate) fn set_split_ratio(&mut self, split_id: WorkspaceSplitId, ratio: f32) -> bool {
        let next_ratio = workspace_split_ratio(ratio);
        let Some(split) = find_workspace_split_mut(&mut self.root, split_id) else {
            return false;
        };

        if (workspace_split_ratio(split.ratio) - next_ratio).abs() < 0.001 {
            return false;
        }

        split.ratio = next_ratio;
        true
    }

    pub(crate) fn reset_split_ratio(&mut self, split_id: WorkspaceSplitId) -> bool {
        let Some(default_ratio) = find_workspace_split(&self.root, split_id)
            .map(|split| workspace_split_ratio(split.default_ratio))
        else {
            return false;
        };
        self.set_split_ratio(split_id, default_ratio)
    }

    pub(crate) fn split_drag_ratio_bounds(
        &self,
        split_id: WorkspaceSplitId,
        content_span: f32,
    ) -> Option<(f32, f32)> {
        let split = find_workspace_split(&self.root, split_id)?;
        let minimum = split_pane_resize_minimum_for_axis(split.axis);
        split_drag_ratio_bounds_from_minimums(
            workspace_node_axis_pane_count(&split.first, split.axis) as f32 * minimum,
            workspace_node_axis_pane_count(&split.second, split.axis) as f32 * minimum,
            content_span,
        )
    }

    pub(crate) fn pane_tab_count(&self, pane_id: WorkspacePaneId) -> Option<usize> {
        self.find_leaf(pane_id)
            .map(|leaf| leaf.tab_group.tabs.len())
    }

    pub(crate) fn workspace_tab_body_drop_is_single_tab_own_pane_noop(
        &self,
        source_pane_id: WorkspacePaneId,
        target_pane_id: WorkspacePaneId,
    ) -> bool {
        source_pane_id == target_pane_id
            && self.pane_tab_count(source_pane_id).unwrap_or_default() <= 1
    }

    pub(crate) fn workspace_tab_edge_drop_is_single_tab_own_pane_noop(
        &self,
        source_pane_id: WorkspacePaneId,
        target_pane_id: WorkspacePaneId,
        zone: WorkspaceDropZone,
    ) -> bool {
        !matches!(zone, WorkspaceDropZone::Center)
            && self
                .workspace_tab_body_drop_is_single_tab_own_pane_noop(source_pane_id, target_pane_id)
    }

    pub(crate) fn pane_can_accept_workspace_action(&self, pane_id: WorkspacePaneId) -> bool {
        self.find_leaf(pane_id)
            .is_some_and(|leaf| !leaf.tab_group.tabs.is_empty())
            || (self.terminal_sessions.is_empty()
                && workspace_empty_root_leaf_id(&self.root) == Some(pane_id))
    }

    pub(crate) fn add_mounting_session_to_pane(
        &mut self,
        requested_pane_id: WorkspacePaneId,
    ) -> Option<TerminalSessionId> {
        /*
        CDXC:GPUIAgentsTerminalLifecycle 2026-06-22-23:33:
        New Agents terminal tabs are selected shell-owned Mounting sessions until a real terminal runtime has started. The tab, pane focus, and shell id are created immediately for layout parity, but no fake Running state, libghostty mount, process launch, command text, stdout/stderr, terminal content, or runtime id persistence is allowed.

        CDXC:GPUIFocusedNewTabs 2026-07-25:
        Cmd+T and the clicked-pane new-terminal control share this model
        mutation. Tab position is stable and user-owned, so a new terminal is
        appended to the end of the target pane's tab strip instead of being
        spliced in after the active tab.
        */
        let pane_id = self.resolve_action_pane_id(requested_pane_id)?;
        let session_id = self.allocate_session_id();
        self.terminal_sessions.push(TerminalSession::placeholder(
            session_id,
            terminal_session_title_for_id(session_id),
            TerminalSessionPresentationState::Mounting,
        ));

        let Some(leaf) = self.find_leaf_mut(pane_id) else {
            self.terminal_sessions
                .retain(|session| session.id != session_id);
            return None;
        };
        let insertion_index = leaf.tab_group.tabs.len();
        leaf.tab_group
            .insert_session_at(WorkspaceTab { session_id }, insertion_index);
        leaf.tab_group.active_tab = session_id;
        self.focused_pane = pane_id;
        Some(session_id)
    }

    pub(crate) fn add_running_session_to_pane(
        &mut self,
        requested_pane_id: WorkspacePaneId,
        title: String,
        agent_icon: Option<&'static str>,
    ) -> Option<(WorkspacePaneId, TerminalSessionId)> {
        /*
        CDXC:GPUIWorkspaceSessionFocus 2026-06-27-13:25:
        Local gxserver sidebar session attach is not a new-terminal startup placeholder. Create the selected Agents tab as Running immediately so the normal visible Ghostty mount-slot path can attach the daemon session without showing the Mounting card or persisting a fake pending state.
        */
        let pane_id = self.resolve_action_pane_id(requested_pane_id)?;
        let session_id = self.allocate_session_id();
        self.terminal_sessions.push(
            TerminalSession::placeholder(
                session_id,
                title,
                TerminalSessionPresentationState::Running,
            )
            .with_agent_icon(agent_icon),
        );

        let Some(leaf) = self.find_leaf_mut(pane_id) else {
            self.terminal_sessions
                .retain(|session| session.id != session_id);
            return None;
        };
        let insertion_index = leaf.tab_group.tabs.len();
        leaf.tab_group
            .insert_session_at(WorkspaceTab { session_id }, insertion_index);
        leaf.tab_group.active_tab = session_id;
        self.focused_pane = pane_id;
        Some((pane_id, session_id))
    }

    pub(crate) fn split_mounting_session_to_right_of_pane(
        &mut self,
        requested_pane_id: WorkspacePaneId,
    ) -> Option<(WorkspacePaneId, TerminalSessionId)> {
        self.split_mounting_session_adjacent_to_pane(
            requested_pane_id,
            WorkspaceSplitAxis::Horizontal,
            false,
        )
    }

    pub(crate) fn split_mounting_session_below_pane(
        &mut self,
        requested_pane_id: WorkspacePaneId,
    ) -> Option<(WorkspacePaneId, TerminalSessionId)> {
        self.split_mounting_session_adjacent_to_pane(
            requested_pane_id,
            WorkspaceSplitAxis::Vertical,
            false,
        )
    }

    pub(crate) fn place_existing_session_for_new_terminal(
        &mut self,
        source_pane_id: WorkspacePaneId,
        requested_pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        placement: AgentsWorkspaceNewTerminalPlacement,
    ) -> Option<WorkspacePaneId> {
        /*
        CDXC:GPUIRegisteredQuickTerminals 2026-08-07:
        Remote gxserver presentation can publish a newly-created session before
        its SSH attach plan returns. Reconciliation necessarily gives that row
        a temporary tab owner, but quick-create placement is still owned by the
        initiating Agents action. Move the existing shell tab into the captured
        tab/split/bottom-row destination before arming its attach payload so the
        presentation race cannot turn Cmd+D, Cmd+Shift+D, or pane controls into
        ordinary tabs.
        */
        let target_pane_id = self.resolve_action_pane_id(requested_pane_id)?;
        let placed = match placement {
            AgentsWorkspaceNewTerminalPlacement::Tab => {
                self.group_tab_into_pane(source_pane_id, target_pane_id, session_id)
            }
            AgentsWorkspaceNewTerminalPlacement::SplitRight => self.split_tab_to_pane(
                source_pane_id,
                target_pane_id,
                session_id,
                WorkspaceDropZone::Right,
            ),
            AgentsWorkspaceNewTerminalPlacement::SplitBelow => self.split_tab_to_pane(
                source_pane_id,
                target_pane_id,
                session_id,
                WorkspaceDropZone::Bottom,
            ),
            AgentsWorkspaceNewTerminalPlacement::BottomRow => {
                return self.move_tab_to_bottom_row(source_pane_id, session_id);
            }
        };
        placed.then_some(self.focused_pane)
    }

    pub(crate) fn move_tab_to_bottom_row(
        &mut self,
        source_pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
    ) -> Option<WorkspacePaneId> {
        if !self.has_session(session_id) || collect_workspace_tab_count(&self.root) <= 1 {
            return None;
        }
        let (tab, source_is_empty) = self.remove_tab_for_move(source_pane_id, session_id)?;
        if source_is_empty {
            self.collapse_empty_leaf(source_pane_id);
        }
        self.clear_focus_mode_if_invalid();

        let pane_id = self.allocate_pane_id();
        let split_id = self.allocate_split_id();
        let new_leaf = WorkspaceNode::Leaf(WorkspaceLeaf {
            pane_id,
            tab_group: WorkspaceTabGroup {
                tabs: vec![tab],
                active_tab: session_id,
            },
        });
        let current_root = std::mem::replace(&mut self.root, workspace_dummy_node());
        self.root = WorkspaceNode::Split(WorkspaceSplit {
            id: split_id,
            axis: WorkspaceSplitAxis::Vertical,
            ratio: workspace_split_ratio(WORKSPACE_BOTTOM_ROW_TOP_RATIO),
            default_ratio: workspace_split_ratio(WORKSPACE_BOTTOM_ROW_TOP_RATIO),
            first: Box::new(current_root),
            second: Box::new(new_leaf),
        });
        self.focused_pane = pane_id;
        self.focus_mode_pane = None;
        self.normalize_workspace_tree();
        Some(pane_id)
    }

    pub(crate) fn append_mounting_session_bottom_row(
        &mut self,
    ) -> (WorkspacePaneId, TerminalSessionId) {
        /*
        CDXC:GPUIAgentsTerminalLifecycle 2026-06-22-23:33:
        Full-width secondary terminal creation must append below the whole Agents workspace, not split the clicked pane. Keep the existing split/tab tree intact as the top branch, create a new bottom-row leaf with one selected Mounting terminal, focus that leaf, and clear Agents Focus mode so the row is visible without creating fake Running state, command-pane sessions, processes, libghostty surfaces, command text, stdout/stderr, or terminal content.
        */
        let session_id = self.allocate_session_id();
        let pane_id = self.allocate_pane_id();
        let split_id = self.allocate_split_id();
        self.terminal_sessions.push(TerminalSession::placeholder(
            session_id,
            terminal_session_title_for_id(session_id),
            TerminalSessionPresentationState::Mounting,
        ));
        let new_leaf = WorkspaceNode::Leaf(WorkspaceLeaf {
            pane_id,
            tab_group: WorkspaceTabGroup {
                tabs: vec![WorkspaceTab { session_id }],
                active_tab: session_id,
            },
        });
        let current_root = std::mem::replace(&mut self.root, workspace_dummy_node());
        self.root = WorkspaceNode::Split(WorkspaceSplit {
            id: split_id,
            axis: WorkspaceSplitAxis::Vertical,
            ratio: workspace_split_ratio(WORKSPACE_BOTTOM_ROW_TOP_RATIO),
            default_ratio: workspace_split_ratio(WORKSPACE_BOTTOM_ROW_TOP_RATIO),
            first: Box::new(current_root),
            second: Box::new(new_leaf),
        });
        self.focused_pane = pane_id;
        self.focus_mode_pane = None;
        self.normalize_workspace_tree();
        (pane_id, session_id)
    }

    pub(crate) fn merge_all_tabs_into_pane(&mut self, requested_pane_id: WorkspacePaneId) -> bool {
        /*
        CDXC:GPUIAgentsMergeAllTabs 2026-06-22-13:17:
        Merge All Tabs collapses the Agents workspace split root into the clicked or focused pane id while preserving every existing Agents terminal tab/session id and presentation state in tree-render order. Single-pane layouts no-op; multi-pane merges clear Focus mode because the pane geometry no longer exists, and command-pane sessions are intentionally outside this model.
        */
        let Some(target_pane_id) = self.resolve_action_pane_id(requested_pane_id) else {
            return false;
        };
        let leaf_order = self.leaf_order();
        if leaf_order.len() <= 1 {
            return false;
        }

        let target_active_session = self
            .find_leaf(target_pane_id)
            .and_then(|leaf| leaf.tab_group.active_session_id());
        let fallback_active_session = leaf_order.iter().find_map(|pane_id| {
            self.find_leaf(*pane_id)
                .and_then(|leaf| leaf.tab_group.active_session_id())
        });
        let mut tabs = Vec::new();
        collect_workspace_tabs_in_tree_order(&self.root, &mut tabs);
        tabs.retain(|tab| self.has_session(tab.session_id));
        if tabs.is_empty() {
            return false;
        }

        let active_tab = target_active_session
            .or(fallback_active_session)
            .filter(|session_id| tabs.iter().any(|tab| tab.session_id == *session_id))
            .unwrap_or(tabs[0].session_id);
        self.root = WorkspaceNode::Leaf(WorkspaceLeaf {
            pane_id: target_pane_id,
            tab_group: WorkspaceTabGroup { tabs, active_tab },
        });
        self.focused_pane = target_pane_id;
        self.focus_mode_pane = None;
        self.normalize_workspace_tree();
        true
    }

    pub(crate) fn reconcile_with_sidebar_tab_sessions(
        &mut self,
        active_project_id: Option<&str>,
        tab_sessions: &[GpuiSidebarWorkspaceTabSession],
        local_workspace_session_mappings: &mut HashMap<
            GpuiLocalWorkspaceSessionKey,
            TerminalSessionId,
        >,
        remote_attach_sessions: &mut HashMap<GpuiRemoteAttachSessionKey, TerminalSessionId>,
    ) -> bool {
        /*
        CDXC:GPUIWorkspaceTabsParity 2026-07-05:
        The Agents tab tree mirrors the active SidebarApp group. The sidebar
        owns filtering and order; Rust only maps projected gxserver ids to
        local shell session ids, removes tabs absent from the projection, and
        updates title/lifecycle chrome from the row payload. Existing mapped
        sessions keep their pane, while newly listed rows append to the
        focused tab group in sidebar order.
        */
        let mut changed = false;
        let keys = tab_sessions
            .iter()
            .map(|session| session.key.clone())
            .collect::<HashSet<_>>();
        let mut allowed_shell_sessions = HashSet::new();

        for tab_session in tab_sessions.iter() {
            let shell_session_id = if let Some(shell_session_id) =
                workspace_terminal_session_mapping_get(
                    &tab_session.key,
                    local_workspace_session_mappings,
                    remote_attach_sessions,
                ) {
                if self.session(shell_session_id).is_some() {
                    shell_session_id
                } else {
                    workspace_terminal_session_mapping_remove(
                        &tab_session.key,
                        local_workspace_session_mappings,
                        remote_attach_sessions,
                    );
                    let session_id = self.allocate_session_id();
                    self.terminal_sessions.push(
                        TerminalSession::placeholder(
                            session_id,
                            tab_session.title.clone(),
                            tab_session.presentation_state,
                        )
                        .with_activity(tab_session.activity)
                        .with_agent_icon(tab_session.agent_icon)
                        .with_kind(tab_session.kind),
                    );
                    workspace_terminal_session_mapping_insert(
                        tab_session.key.clone(),
                        session_id,
                        local_workspace_session_mappings,
                        remote_attach_sessions,
                    );
                    changed = true;
                    session_id
                }
            } else {
                let session_id = self.allocate_session_id();
                self.terminal_sessions.push(
                    TerminalSession::placeholder(
                        session_id,
                        tab_session.title.clone(),
                        tab_session.presentation_state,
                    )
                    .with_activity(tab_session.activity)
                    .with_agent_icon(tab_session.agent_icon)
                    .with_kind(tab_session.kind),
                );
                workspace_terminal_session_mapping_insert(
                    tab_session.key.clone(),
                    session_id,
                    local_workspace_session_mappings,
                    remote_attach_sessions,
                );
                changed = true;
                session_id
            };

            if let Some(session) = self
                .terminal_sessions
                .iter_mut()
                .find(|session| session.id == shell_session_id)
            {
                if session.title != tab_session.title {
                    session.title = tab_session.title.clone();
                    changed = true;
                }
                if session.agent_icon != tab_session.agent_icon {
                    session.agent_icon = tab_session.agent_icon;
                    changed = true;
                }
                if session.activity != tab_session.activity {
                    session.activity = tab_session.activity;
                    changed = true;
                }
                if session.is_generating_first_prompt_title
                    != tab_session.is_generating_first_prompt_title
                {
                    session.is_generating_first_prompt_title =
                        tab_session.is_generating_first_prompt_title;
                    changed = true;
                }
                if session.kind != tab_session.kind {
                    session.kind = tab_session.kind;
                    session.startup_eligible_when_mounting = false;
                    session.zmx_session_name = None;
                    changed = true;
                }
                if session.presentation_state != tab_session.presentation_state {
                    session.set_presentation_state_with_startup_eligibility(
                        tab_session.presentation_state,
                        false,
                    );
                    changed = true;
                }
            }
            allowed_shell_sessions.insert(shell_session_id);
        }

        let before_session_count = self.terminal_sessions.len();
        self.terminal_sessions
            .retain(|session| allowed_shell_sessions.contains(&session.id));
        changed |= self.terminal_sessions.len() != before_session_count;
        local_workspace_session_mappings.retain(|key, shell_session_id| {
            keys.contains(&GpuiWorkspaceTerminalSessionKey::Local(key.clone()))
                && allowed_shell_sessions.contains(shell_session_id)
        });
        remote_attach_sessions.retain(|key, shell_session_id| {
            let belongs_to_active_project = active_project_id
                == Some(
                    gpui_remote_scoped_project_id(
                        key.remote_machine_id.as_str(),
                        key.project_id.as_str(),
                    )
                    .as_str(),
                );
            !belongs_to_active_project
                || (keys.contains(&GpuiWorkspaceTerminalSessionKey::Remote(key.clone()))
                    && allowed_shell_sessions.contains(shell_session_id))
        });

        if tab_sessions.is_empty() {
            if !self.terminal_sessions.is_empty()
                || collect_workspace_tab_count(&self.root) > 0
                || !matches!(self.root, WorkspaceNode::Leaf(_))
            {
                self.terminal_sessions.clear();
                self.root = workspace_empty_leaf_node(self.focused_pane);
                self.focus_mode_pane = None;
                changed = true;
            }
            changed |= self.normalize_workspace_tree();
            return changed;
        }

        let mut assigned_shell_sessions = HashSet::new();
        for pane_id in self.leaf_order() {
            let Some(leaf) = self.find_leaf_mut(pane_id) else {
                continue;
            };
            let before_tabs = leaf.tab_group.tabs.clone();
            /*
            CDXC:GPUIWorkspaceTabsParity 2026-07-25:
            Tab position inside a pane is owned by the Agents workspace, not by
            the sidebar projection. The sidebar reorders its rows as sessions
            report activity, so re-sorting mounted tabs by that projection made
            the tab strip shuffle on every agent turn and discarded the user's
            own drag reordering. Reconcile now only drops tabs whose session
            left the projection; surviving tabs keep their persisted index and
            newly listed sessions append below in sidebar order.
            */
            leaf.tab_group
                .tabs
                .retain(|tab| allowed_shell_sessions.contains(&tab.session_id));
            for tab in &leaf.tab_group.tabs {
                assigned_shell_sessions.insert(tab.session_id);
            }
            if !leaf
                .tab_group
                .tabs
                .iter()
                .any(|tab| tab.session_id == leaf.tab_group.active_tab)
            {
                let next_active_tab = leaf
                    .tab_group
                    .tabs
                    .first()
                    .map(|tab| tab.session_id)
                    .unwrap_or(TerminalSessionId(0));
                if leaf.tab_group.active_tab != next_active_tab {
                    leaf.tab_group.active_tab = next_active_tab;
                    changed = true;
                }
            }
            changed |= leaf.tab_group.tabs != before_tabs;
        }

        let target_pane_id = self
            .resolve_action_pane_id(self.focused_pane)
            .or_else(|| self.leaf_order().into_iter().next())
            .unwrap_or(self.focused_pane);
        if self.find_leaf(target_pane_id).is_none() {
            self.root = workspace_empty_leaf_node(target_pane_id);
            self.focused_pane = target_pane_id;
            self.focus_mode_pane = None;
            changed = true;
        }
        let Some(target_leaf) = self.find_leaf_mut(target_pane_id) else {
            return changed;
        };
        for tab_session in tab_sessions {
            let Some(shell_session_id) = workspace_terminal_session_mapping_get(
                &tab_session.key,
                local_workspace_session_mappings,
                remote_attach_sessions,
            ) else {
                continue;
            };
            if assigned_shell_sessions.insert(shell_session_id) {
                target_leaf.tab_group.tabs.push(WorkspaceTab {
                    session_id: shell_session_id,
                });
                changed = true;
            }
        }
        if !target_leaf
            .tab_group
            .tabs
            .iter()
            .any(|tab| tab.session_id == target_leaf.tab_group.active_tab)
        {
            target_leaf.tab_group.active_tab = target_leaf
                .tab_group
                .tabs
                .first()
                .map(|tab| tab.session_id)
                .unwrap_or(TerminalSessionId(0));
            changed = true;
        }
        changed |= self.normalize_workspace_tree();
        changed
    }

    pub(crate) fn rotate_panes_clockwise(&mut self) -> bool {
        /*
        CDXC:GPUIAgentsRotatePanes 2026-06-26-06:57:
        Native Agents Rotate Panes Clockwise is a pure split-tree transform: recursively swap horizontal and vertical axes, reverse vertical branches while inverting their ratios, and preserve leaf pane ids, tab order, active tabs, focused pane, and terminal presentation records. Single-leaf workspaces no-op, and command-pane state stays outside this model.

        CDXC:GPUIAgentsRotatePanes 2026-06-26-06:57:
        Existing Agents geometry mutations clear Focus mode when the visible split layout changes. Rotation follows that rule after a multi-leaf transform so the rotated pane tree is immediately visible while the selected focused pane id remains stable for follow-up actions.
        */
        if self.leaf_order().len() <= 1 {
            return false;
        }

        rotate_workspace_node_clockwise(&mut self.root);
        self.focus_mode_pane = None;
        true
    }

    pub(crate) fn split_mounting_session_adjacent_to_pane(
        &mut self,
        requested_pane_id: WorkspacePaneId,
        axis: WorkspaceSplitAxis,
        new_leaf_first: bool,
    ) -> Option<(WorkspacePaneId, TerminalSessionId)> {
        /*
        CDXC:GPUIAgentsTerminalLifecycle 2026-06-22-23:33:
        Agents explicit split controls must offer parity for Split Right and Split Below from the pane tab chrome. Each control creates a new split leaf with a selected Mounting terminal, uses the existing split tree and persistence path, and clears Focus mode only so the newly created pane is visible without adding fake Running state, overlays, native hit-test routing, libghostty mounts, or real process creation.
        */
        let target_pane_id = self.resolve_action_pane_id(requested_pane_id)?;
        let session_id = self.allocate_session_id();
        let pane_id = self.allocate_pane_id();
        let split_id = self.allocate_split_id();
        let new_leaf = WorkspaceLeaf {
            pane_id,
            tab_group: WorkspaceTabGroup {
                tabs: vec![WorkspaceTab { session_id }],
                active_tab: session_id,
            },
        };

        if insert_workspace_leaf_split(
            &mut self.root,
            target_pane_id,
            new_leaf,
            axis,
            new_leaf_first,
            split_id,
        ) {
            self.terminal_sessions.push(TerminalSession::placeholder(
                session_id,
                terminal_session_title_for_id(session_id),
                TerminalSessionPresentationState::Mounting,
            ));
            self.focused_pane = pane_id;
            self.focus_mode_pane = None;
            self.normalize_workspace_tree();
            Some((pane_id, session_id))
        } else {
            None
        }
    }

    pub(crate) fn add_placeholder_session_from_command_title(
        &mut self,
        target_pane_id: WorkspacePaneId,
        title: String,
        zone: WorkspaceDropZone,
    ) -> Option<(WorkspacePaneId, TerminalSessionId)> {
        /*
        CDXC:GPUICommandWorkspaceTransfer 2026-06-22-23:33:
        Command-pane tabs dropped onto an Agents pane body become selected Mounting Agents shell sessions with the command tab's visible title. Center drops group into the target pane, edge drops use normal Agents split semantics, and this remains shell state only: no command process, terminal content, stdout/stderr, libghostty mount/remount, fake Running state, overlay, or hidden hit region is transferred.
        */
        match zone {
            WorkspaceDropZone::Center => {
                self.group_placeholder_session_from_command_title(target_pane_id, title)
            }
            WorkspaceDropZone::Left
            | WorkspaceDropZone::Right
            | WorkspaceDropZone::Top
            | WorkspaceDropZone::Bottom => {
                self.split_placeholder_session_from_command_title(target_pane_id, title, zone)
            }
        }
    }

    pub(crate) fn group_placeholder_session_from_command_title(
        &mut self,
        target_pane_id: WorkspacePaneId,
        title: String,
    ) -> Option<(WorkspacePaneId, TerminalSessionId)> {
        let insertion_index = self.find_leaf(target_pane_id)?.tab_group.tabs.len();
        self.insert_placeholder_session_from_command_title_at(
            target_pane_id,
            insertion_index,
            title,
        )
    }

    pub(crate) fn insert_placeholder_session_from_command_title_at(
        &mut self,
        target_pane_id: WorkspacePaneId,
        insertion_index: usize,
        title: String,
    ) -> Option<(WorkspacePaneId, TerminalSessionId)> {
        /*
        CDXC:GPUICommandWorkspaceTransfer 2026-06-22-23:33:
        Command tabs dropped on an Agents tab strip insert a new Mounting Agents shell session at the visible tab boundary or end target, select it, and focus that Agents pane. This is still a placeholder boundary: only the visible command title crosses surfaces, with no process, command text, stdout/stderr, terminal content, libghostty mount/remount, fake Running state, real Source/Kanban/Automate/Manage surface, overlay, hidden hit region, or native/root hit-test routing.
        */
        self.find_leaf(target_pane_id)?;
        let session_id = self.allocate_session_id();
        self.terminal_sessions.push(TerminalSession::placeholder(
            session_id,
            title,
            TerminalSessionPresentationState::Mounting,
        ));
        let tab = WorkspaceTab { session_id };

        let Some(target_leaf) = self.find_leaf_mut(target_pane_id) else {
            self.terminal_sessions
                .retain(|session| session.id != session_id);
            return None;
        };
        target_leaf
            .tab_group
            .insert_session_at(tab, insertion_index);
        target_leaf.tab_group.active_tab = session_id;
        self.focused_pane = target_pane_id;
        Some((target_pane_id, session_id))
    }

    pub(crate) fn split_placeholder_session_from_command_title(
        &mut self,
        target_pane_id: WorkspacePaneId,
        title: String,
        zone: WorkspaceDropZone,
    ) -> Option<(WorkspacePaneId, TerminalSessionId)> {
        if matches!(zone, WorkspaceDropZone::Center) {
            return self.group_placeholder_session_from_command_title(target_pane_id, title);
        }
        self.find_leaf(target_pane_id)?;

        let session_id = self.allocate_session_id();
        let pane_id = self.allocate_pane_id();
        let split_id = self.allocate_split_id();
        let new_leaf = WorkspaceLeaf {
            pane_id,
            tab_group: WorkspaceTabGroup {
                tabs: vec![WorkspaceTab { session_id }],
                active_tab: session_id,
            },
        };
        let axis = match zone {
            WorkspaceDropZone::Left | WorkspaceDropZone::Right => WorkspaceSplitAxis::Horizontal,
            WorkspaceDropZone::Top | WorkspaceDropZone::Bottom => WorkspaceSplitAxis::Vertical,
            WorkspaceDropZone::Center => unreachable!("center grouping handled above"),
        };
        let dragged_first = matches!(zone, WorkspaceDropZone::Left | WorkspaceDropZone::Top);

        if insert_workspace_leaf_split(
            &mut self.root,
            target_pane_id,
            new_leaf,
            axis,
            dragged_first,
            split_id,
        ) {
            self.terminal_sessions.push(TerminalSession::placeholder(
                session_id,
                title,
                TerminalSessionPresentationState::Mounting,
            ));
            self.focused_pane = pane_id;
            self.focus_mode_pane = None;
            self.normalize_workspace_tree();
            Some((pane_id, session_id))
        } else {
            None
        }
    }

    pub(crate) fn reorder_tab_within_pane(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        insertion_index: usize,
    ) -> bool {
        /*
        CDXC:GPUIWorkspaceDragDrop 2026-06-22-05:31:
        Same-strip Agents tab drops are reorder-only. They must stay inside the source tab group, keep the dragged session identity, and leave the session presentation record untouched so sleeping, restored, mounting, and popped-out placeholders survive the reorder.
        */
        let Some(leaf) = self.find_leaf_mut(pane_id) else {
            return false;
        };
        let active_tab = leaf.tab_group.active_tab;
        let Some(tab) = leaf.tab_group.remove_session(session_id) else {
            return false;
        };
        leaf.tab_group.insert_session_at(tab, insertion_index);
        leaf.tab_group.active_tab = active_tab;
        true
    }

    pub(crate) fn group_tab_into_pane(
        &mut self,
        source_pane_id: WorkspacePaneId,
        target_pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
    ) -> bool {
        if !self.has_session(session_id) || self.find_leaf(target_pane_id).is_none() {
            return false;
        }

        if source_pane_id == target_pane_id {
            self.select_tab(target_pane_id, session_id);
            return true;
        }

        let Some((tab, source_is_empty)) = self.remove_tab_for_move(source_pane_id, session_id)
        else {
            return false;
        };

        if source_is_empty {
            self.collapse_empty_leaf(source_pane_id);
        }
        self.clear_focus_mode_if_invalid();

        let Some(target_leaf) = self.find_leaf_mut(target_pane_id) else {
            return false;
        };
        target_leaf
            .tab_group
            .insert_session_at(tab, target_leaf.tab_group.tabs.len());
        target_leaf.tab_group.active_tab = session_id;
        self.focused_pane = target_pane_id;
        self.normalize_workspace_tree();
        true
    }

    pub(crate) fn split_tab_to_pane(
        &mut self,
        source_pane_id: WorkspacePaneId,
        target_pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
        zone: WorkspaceDropZone,
    ) -> bool {
        /*
        CDXC:GPUIWorkspaceDragDrop 2026-06-22-05:31:
        Pane-body Agents tab drops use an in-memory layout mutation only in this slice: center drops group into the target tab group, while left/right/top/bottom edge drops create a new leaf beside the target and remove the dragged tab from its source. Empty source leaves are collapsed immediately so the split tree remains renderable without persistence, command-pane drag/drop, browser CEF drag behavior, or real wake/mount work.
        */
        if matches!(zone, WorkspaceDropZone::Center) {
            return self.group_tab_into_pane(source_pane_id, target_pane_id, session_id);
        }

        if !self.has_session(session_id) || self.find_leaf(target_pane_id).is_none() {
            return false;
        }

        if self.workspace_tab_edge_drop_is_single_tab_own_pane_noop(
            source_pane_id,
            target_pane_id,
            zone,
        ) {
            return false;
        }

        let Some((tab, source_is_empty)) = self.remove_tab_for_move(source_pane_id, session_id)
        else {
            return false;
        };

        if source_is_empty {
            self.collapse_empty_leaf(source_pane_id);
        }
        self.clear_focus_mode_if_invalid();

        let pane_id = self.allocate_pane_id();
        let split_id = self.allocate_split_id();
        let new_leaf = WorkspaceLeaf {
            pane_id,
            tab_group: WorkspaceTabGroup {
                tabs: vec![tab],
                active_tab: session_id,
            },
        };
        let axis = match zone {
            WorkspaceDropZone::Left | WorkspaceDropZone::Right => WorkspaceSplitAxis::Horizontal,
            WorkspaceDropZone::Top | WorkspaceDropZone::Bottom => WorkspaceSplitAxis::Vertical,
            WorkspaceDropZone::Center => unreachable!("center grouping handled above"),
        };
        let dragged_first = matches!(zone, WorkspaceDropZone::Left | WorkspaceDropZone::Top);

        if insert_workspace_leaf_split(
            &mut self.root,
            target_pane_id,
            new_leaf,
            axis,
            dragged_first,
            split_id,
        ) {
            self.focused_pane = pane_id;
            self.normalize_workspace_tree();
            true
        } else {
            false
        }
    }

    pub(crate) fn remove_tab_for_move(
        &mut self,
        pane_id: WorkspacePaneId,
        session_id: TerminalSessionId,
    ) -> Option<(WorkspaceTab, bool)> {
        let leaf = self.find_leaf_mut(pane_id)?;
        let tab = leaf.tab_group.remove_session(session_id)?;
        let source_is_empty = leaf.tab_group.tabs.is_empty();
        Some((tab, source_is_empty))
    }

    pub(crate) fn collapse_empty_leaf(&mut self, pane_id: WorkspacePaneId) {
        /*
        CDXC:GPUIAgentsCloseFocus 2026-06-22-10:23:
        Closing the only Agents tab in a split pane must choose the next keyboard target from the pre-collapse sibling branch, while same-pane closes keep the right-then-left tab selection owned by WorkspaceTabGroup::remove_session. Sibling-branch candidates are scored before the broader pane geometry fallback so nested layouts match native close-focus behavior.
        */
        let replacement_focus = workspace_close_focus_replacement_leaf_id(&self.root, pane_id);
        let root_is_empty = collapse_empty_workspace_leaf(&mut self.root, pane_id);
        if root_is_empty {
            self.root = workspace_empty_leaf_node(pane_id);
            self.focused_pane = pane_id;
        }

        if self.focused_pane == pane_id || self.find_leaf(self.focused_pane).is_none() {
            let next_focus = replacement_focus
                .filter(|pane_id| self.find_leaf(*pane_id).is_some())
                .or_else(|| first_workspace_leaf_id(&self.root));
            if let Some(next_focus) = next_focus {
                self.focused_pane = next_focus;
            }
        }
    }

    pub(crate) fn allocate_pane_id(&mut self) -> WorkspacePaneId {
        let pane_id = WorkspacePaneId(self.next_pane_id);
        self.next_pane_id += 1;
        pane_id
    }

    pub(crate) fn allocate_split_id(&mut self) -> WorkspaceSplitId {
        let split_id = WorkspaceSplitId(self.next_split_id);
        self.next_split_id += 1;
        split_id
    }

    pub(crate) fn allocate_session_id(&mut self) -> TerminalSessionId {
        let session_id = TerminalSessionId(self.next_session_id);
        self.next_session_id += 1;
        session_id
    }

    pub(crate) fn resolve_action_pane_id(
        &self,
        requested_pane_id: WorkspacePaneId,
    ) -> Option<WorkspacePaneId> {
        if self.pane_can_accept_workspace_action(requested_pane_id) {
            Some(requested_pane_id)
        } else if self.pane_can_accept_workspace_action(self.focused_pane) {
            Some(self.focused_pane)
        } else {
            first_workspace_leaf_id(&self.root).or_else(|| {
                if self.terminal_sessions.is_empty() {
                    workspace_empty_root_leaf_id(&self.root)
                } else {
                    None
                }
            })
        }
    }
}
