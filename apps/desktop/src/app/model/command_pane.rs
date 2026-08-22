// C1 wave-3 extraction: the CommandPaneModel sub-model struct and impl moved verbatim out of main.rs (pure
// move, no logic changes; items made pub(crate) so main.rs and sibling
// modules can still reach them). See docs/2026-08-22/repo-restructure/SPLITS.md C1.
#![allow(dead_code)]

use crate::*;


pub(crate) struct CommandPaneModel {
    pub(crate) terminal_sessions: Vec<CommandTerminalSession>,
    pub(crate) root: CommandPaneNode,
    pub(crate) focused_group: CommandPaneGroupId,
    pub(crate) focus_mode_group: Option<CommandPaneGroupId>,
    pub(crate) mode: CommandPaneMode,
    pub(crate) last_expanded_mode: CommandPaneMode,
    pub(crate) height_ratio: f32,
    pub(crate) width_ratio: f32,
    pub(crate) resize_drag: Option<CommandPaneResizeDragState>,
    pub(crate) next_group_id: u64,
    pub(crate) next_split_id: u64,
    pub(crate) next_session_id: u64,
}


impl CommandPaneModel {
    pub(crate) fn shell_default_with_default_height_px(content_height: f32, default_height_px: f32) -> Self {
        /*
        CDXC:GPUICommandPane 2026-06-25-11:40:
        The production GPUI command pane starts with no command terminal sessions. Opening the pane creates the first `Command Terminal` placeholder at the open boundary, while Action runs and transferred tabs can still supply specific titles. Do not seed fake Command/Shell sessions into app startup or persisted fallback state.

        CDXC:GPUICommandPane 2026-06-27-15:00:
        The empty model still must not spawn a command terminal at startup, but GPUI now keeps the bottom command-pane strip visible so users can discover and open Commands from the workspace footer. The visible strip is presentation chrome; plus, double-click, F12, and Actions remain the only boundaries that create the first command session.
        */
        Self {
            terminal_sessions: Vec::new(),
            root: command_pane_dummy_node(),
            focused_group: CommandPaneGroupId(0),
            focus_mode_group: None,
            mode: CommandPaneMode::Collapsed,
            last_expanded_mode: CommandPaneMode::Pinned,
            height_ratio: command_pane_default_height_ratio_for_default_height_px(
                default_height_px,
                content_height,
            ),
            width_ratio: COMMAND_PANE_DEFAULT_WIDTH_RATIO,
            resize_drag: None,
            next_group_id: 1,
            next_split_id: 1,
            next_session_id: 1,
        }
    }

    pub(crate) fn has_sessions(&self) -> bool {
        !self.terminal_sessions.is_empty()
    }

    pub(crate) fn is_expanded(&self) -> bool {
        matches!(
            self.mode,
            CommandPaneMode::Pinned | CommandPaneMode::Floating
        )
    }

    pub(crate) fn active_group_and_session_id(&self) -> Option<(CommandPaneGroupId, CommandSessionId)> {
        self.find_leaf(self.focused_group)
            .and_then(|leaf| leaf.tab_group.active_session_id())
            .map(|session_id| (self.focused_group, session_id))
            .or_else(|| {
                first_command_leaf(&self.root).and_then(|leaf| {
                    leaf.tab_group
                        .active_session_id()
                        .map(|session_id| (leaf.group_id, session_id))
                })
            })
    }

    pub(crate) fn focused_group_active_session_id(&self) -> Option<(CommandPaneGroupId, CommandSessionId)> {
        /*
        CDXC:GPUICommandPaneFocus 2026-06-25-21:24:
        Native command-pane focus chrome and focused-session actions require the stored command focus and live responder to identify the same command session. GPUI shell focus is the responder proxy, so responder-style command helpers must not fall back to the first command group when focused_group is stale or missing.
        */
        self.find_leaf(self.focused_group)
            .and_then(|leaf| leaf.tab_group.active_session_id())
            .map(|session_id| (self.focused_group, session_id))
    }

    pub(crate) fn session(&self, id: CommandSessionId) -> Option<&CommandTerminalSession> {
        self.terminal_sessions
            .iter()
            .find(|session| session.id == id)
    }

    pub(crate) fn session_mut(&mut self, id: CommandSessionId) -> Option<&mut CommandTerminalSession> {
        self.terminal_sessions
            .iter_mut()
            .find(|session| session.id == id)
    }

    pub(crate) fn has_session(&self, id: CommandSessionId) -> bool {
        self.session(id).is_some()
    }

    pub(crate) fn rename_session(&mut self, id: CommandSessionId, title: String) -> bool {
        /*
        CDXC:GPUICommandPaneRename 2026-06-25-16:33:
        GPUI command-pane Rename Session updates only the live command-tab title. Command shell persistence remains layout/lifecycle-only and must not write user-entered titles, command text, terminal content, paths, stdout, stderr, or action payloads into shell-state JSON.

        CDXC:GPUICommandPaneRename 2026-06-25-22:33:
        Rename Session is a live command-tab title edit. The requested session id must still be attached to a command tab group; stale stored sessions no-op without falling back to the focused group or another tab.

        CDXC:GPUICommandPaneGxserverRestore 2026-07-04:
        Command-pane restart parity now persists this bounded display title beside the gxserver session id so renamed command tabs restore with their daemon-backed identity. The title remains chrome metadata only; command text, terminal output, paths, and attach payloads stay out of shell state.
        */
        if command_pane_group_for_session(self, id).is_none() {
            return false;
        }
        let Some(session) = self.session_mut(id) else {
            return false;
        };
        if title.is_empty() || session.title == title {
            return false;
        }
        session.title = title;
        true
    }

    pub(crate) fn set_focused_group_for_selected_owner(&mut self, group_id: CommandPaneGroupId) {
        /*
        CDXC:GPUICommandFocusMode 2026-06-26-04:43:
        Command-pane Focus is a visibility filter, not sticky selection. When a command insertion, reorder, or grouping path makes a different command group the active owner, clear Focus before render so the selected command owner is visible; same-group selection and reorder keep Focus reversible.
        */
        self.focused_group = group_id;
        if self
            .focus_mode_group
            .is_some_and(|focus_group_id| focus_group_id != group_id)
        {
            self.focus_mode_group = None;
        }
    }

    pub(crate) fn select_session_in_group(
        &mut self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
    ) -> bool {
        let selected = self.find_leaf_mut(group_id).is_some_and(|leaf| {
            if leaf.tab_group.has_session(session_id) {
                leaf.tab_group.active_session = session_id;
                true
            } else {
                false
            }
        });

        if selected {
            self.set_focused_group_for_selected_owner(group_id);
            self.clear_focus_mode_if_invalid();
        }
        selected
    }

    pub(crate) fn select_session_in_group_for_hidden_open(
        &mut self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
        content_height: f32,
        default_height_px: f32,
    ) -> bool {
        /*
        CDXC:GPUICommandPane 2026-06-25-12:10:
        Selecting a tab from collapsed command-strip chrome is a hidden-open path like macOS `openCommandsPanelForActiveProject`. Apply the Workspace default-height reset before expanding so collapsed tab clicks and context-menu Select do not preserve a stale hidden pane height.
        */
        if !self.select_session_in_group(group_id, session_id) {
            return false;
        }

        self.prepare_hidden_open_with_default_height_px(content_height, default_height_px);
        self.expand();
        true
    }

    pub(crate) fn focus_group(&mut self, group_id: CommandPaneGroupId) -> bool {
        if self.find_leaf(group_id).is_some() {
            self.set_focused_group_for_selected_owner(group_id);
            true
        } else {
            false
        }
    }

    pub(crate) fn toggle_focus_mode_for_tab(
        &mut self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
    ) -> bool {
        /*
        CDXC:GPUICommandFocusMode 2026-06-25-21:40:
        Command-tab Focus is the command-panel equivalent of native split-owner Focus mode. Store only the focused command group id, preserve the full command split tree for reversible exit, and select/focus the clicked command tab before zooming so the visible owner matches the menu target without creating terminal processes, command text, fallback rows, overlays, or persisted runtime state.
        */
        if self.focus_mode_group == Some(group_id) {
            self.focus_mode_group = None;
            return self.select_session_in_group(group_id, session_id);
        }

        if !self.tab_context_allows_focus_mode(group_id, session_id) {
            return false;
        }
        if !self.select_session_in_group(group_id, session_id) {
            return false;
        }
        self.focus_mode_group = Some(group_id);
        true
    }

    pub(crate) fn acknowledge_attention_for_session_activation(
        &mut self,
        session_id: CommandSessionId,
    ) -> bool {
        /*
        CDXC:GPUICommandAttention 2026-06-25-19:58:
        Native command content, titlebar, and tab activation acknowledge a focused Attention command session. GPUI should clear only the directly activated command session from Attention to Idle, leaving Working, Delayed Send flags, sleeping state, and Agents workspace activity unchanged.
        */
        let Some(session) = self.session_mut(session_id) else {
            return false;
        };
        if session.activity != CommandTerminalActivity::Attention {
            return false;
        }
        session.activity = CommandTerminalActivity::Idle;
        true
    }

    pub(crate) fn acknowledge_attention_for_focused_session_activation(&mut self) -> bool {
        /*
        CDXC:GPUICommandAttention 2026-06-26-00:38:
        Responder and titlebar-control command activation must acknowledge only the active tab in the live focused command group. A stale `focused_group` is not an activation target and must no-op instead of falling back to the first command group and clearing unrelated Attention state.
        */
        let Some((_group_id, session_id)) = self.focused_group_active_session_id() else {
            return false;
        };
        self.acknowledge_attention_for_session_activation(session_id)
    }

    pub(crate) fn acknowledge_attention_for_live_focused_group_activation(&mut self) -> bool {
        /*
        CDXC:GPUICommandAttention 2026-06-25-23:55:
        Keyboard focus transfer into an already-open command panel is responder-like. Acknowledge only the active session in the live `focused_group`; stale command focus must not fall back to the first command group and clear unrelated Attention state.
        */
        let Some((_group_id, session_id)) = self.focused_group_active_session_id() else {
            return false;
        };
        self.acknowledge_attention_for_session_activation(session_id)
    }

    pub(crate) fn cycle_active_session(&mut self, reverse: bool) -> bool {
        /*
        CDXC:GPUICommandPaneFocus 2026-06-25-23:20:
        Command-pane Ctrl-Tab parity is responder-like: cycle only the live focused command group and no-op when `focused_group` is stale instead of falling back to the first command group.
        */
        self.find_leaf_mut(self.focused_group)
            .and_then(|leaf| leaf.tab_group.cycle_active_session(reverse))
            .is_some()
    }

    pub(crate) fn close_session(
        &mut self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
    ) -> bool {
        let Some((_tab, source_is_empty)) = self.remove_tab_for_move(group_id, session_id) else {
            return false;
        };

        self.terminal_sessions
            .retain(|session| session.id != session_id);

        if self.terminal_sessions.is_empty() {
            self.root = command_pane_dummy_node();
            self.focused_group = CommandPaneGroupId(0);
            self.focus_mode_group = None;
            self.collapse();
            return true;
        }

        if source_is_empty {
            self.collapse_empty_leaf(group_id);
        }
        self.clear_focus_mode_if_invalid();
        true
    }

    pub(crate) fn close_session_from_direct_tab_close(
        &mut self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
    ) -> bool {
        /*
        CDXC:GPUICommandPaneTabs 2026-06-26-06:18:
        Direct GPUI command-tab close mirrors native command titlebar close: select the clicked command session before removal so right-then-left neighbor selection is resolved from the close target, not from a previously active tab. Scoped Close Left/Right/Others continue to call `close_session` directly because their native menu rows do not focus the clicked terminal first.
        */
        if !self.select_session_in_group(group_id, session_id) {
            return false;
        }
        self.close_session(group_id, session_id)
    }

    pub(crate) fn tab_session_ids_for_close_scope(
        &self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
        scope: CommandPaneTabCloseScope,
    ) -> Vec<CommandSessionId> {
        /*
        CDXC:GPUICommandPaneTabs 2026-06-25-11:20:
        GPUI command-tab context menu close scopes must match macOS command-panel tab behavior: resolve the clicked tab's sibling ids before closing anything, stay inside that command group, and never cross into another command split group, Agents workspace pane, Browser tab, project-editor surface, command text, terminal output, paths, or persisted shell inference.
        */
        let Some(leaf) = self.find_leaf(group_id) else {
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
            CommandPaneTabCloseScope::Close => vec![session_id],
            CommandPaneTabCloseScope::CloseLeft => tab_session_ids[..tab_index].to_vec(),
            CommandPaneTabCloseScope::CloseOthers => tab_session_ids
                .into_iter()
                .filter(|candidate| *candidate != session_id)
                .collect(),
            CommandPaneTabCloseScope::CloseRight => tab_session_ids[tab_index + 1..].to_vec(),
        }
    }

    pub(crate) fn tab_session_ids_for_sleep_scope(
        &self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
        scope: CommandPaneTabSleepScope,
    ) -> Vec<CommandSessionId> {
        /*
        CDXC:GPUICommandTabSleep 2026-06-25-14:27:
        Command-tab Sleep scopes use the clicked command group's sibling list, just like native pane-tab sleep. Keep sleep resolution inside the command group so Sleep Right/Left/Others never crosses command splits, Agents workspace panes, Browser tabs, project-editor surfaces, command text, terminal output, paths, or persisted shell inference.
        */
        let Some(leaf) = self.find_leaf(group_id) else {
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
            CommandPaneTabSleepScope::Sleep => vec![session_id],
            CommandPaneTabSleepScope::SleepLeft => tab_session_ids[..tab_index].to_vec(),
            CommandPaneTabSleepScope::SleepOthers => tab_session_ids
                .into_iter()
                .filter(|candidate| *candidate != session_id)
                .collect(),
            CommandPaneTabSleepScope::SleepRight => tab_session_ids[tab_index + 1..].to_vec(),
        }
    }

    pub(crate) fn set_session_sleeping(&mut self, session_id: CommandSessionId, is_sleeping: bool) -> bool {
        let Some(session) = self.session_mut(session_id) else {
            return false;
        };
        if session.is_sleeping == is_sleeping {
            return false;
        }
        session.is_sleeping = is_sleeping;
        if is_sleeping {
            /*
            CDXC:GPUICommandDelayedSend 2026-06-25-15:46:
            Manual command-tab Sleep parks the terminal surface but must not cancel Delayed Send or Close After Done intent. Native keeps those timers/flags attached to the session; Delayed Send fires only if the tab is awake again at the deadline, and Close After Done resumes countdown evaluation after wake.
            */
            session.activity = CommandTerminalActivity::Idle;
            session.action_close_terminal_on_exit = false;
            session.action_run_id = None;
            session.action_status_file_path = None;
        }
        self.clear_focus_mode_if_invalid();
        true
    }

    pub(crate) fn prepare_hidden_open_with_default_height_px(
        &mut self,
        content_height: f32,
        default_height_px: f32,
    ) -> bool {
        /*
        CDXC:GPUICommandPane 2026-06-25-11:47:
        Opening a hidden GPUI command pane must match macOS `createCommandsPanelOpenStatePatch`: reset height from the Workspace default only when the pane is hidden, and preserve the user's live resize while the pane is already expanded. Keep this model-local so F12, titlebar Actions, sidebar Actions, and command chrome share the same rule.

        CDXC:GPUICommandPaneSide 2026-08-16:
        Only the height resets here, because it comes from the Workspace default-height Setting. The right dock's width has no Settings default, so its ratio is user-owned: opening the pane again keeps the width the divider drag stored, and the divider's double-click reset stays the only way back to the default.
        */
        if self.is_expanded() {
            return false;
        }

        self.reset_height_with_default_height_px(content_height, default_height_px);
        true
    }

    pub(crate) fn open_with_default_height_px(
        &mut self,
        content_height: f32,
        default_height_px: f32,
    ) -> Option<(CommandPaneGroupId, CommandSessionId, bool)> {
        self.prepare_hidden_open_with_default_height_px(content_height, default_height_px);
        self.ensure_session_for_open()
    }

    pub(crate) fn ensure_session_for_open(&mut self) -> Option<(CommandPaneGroupId, CommandSessionId, bool)> {
        /*
        CDXC:GPUICommandPane 2026-06-25-11:40:
        Opening an empty command pane mirrors macOS `openCommandsPanelForActiveProject`: create exactly one selected `Command Terminal` placeholder at open time. If a valid command tab already exists, preserve it and only expand/focus the pane so opening never invents extra tabs.
        */
        if let Some((group_id, session_id)) = self.active_group_and_session_id() {
            self.focused_group = group_id;
            self.expand();
            return Some((group_id, session_id, false));
        }

        let session_id = self.add_session_to_focused_group();
        Some((self.focused_group, session_id, true))
    }

    pub(crate) fn add_new_session(
        &mut self,
        target_group_id: Option<CommandPaneGroupId>,
    ) -> Option<(CommandPaneGroupId, CommandSessionId)> {
        /*
        CDXC:GPUICommandPaneInsertion 2026-06-25-21:21:
        Native command-panel New Terminal carries the clicked titlebar session as the insertion target, while keyboard creation uses the command panel's focused responder. Resolve the command group explicitly at the creation boundary so clicked plus/double-click chrome inserts into that group without depending on a prior focus side effect.
        */
        if let Some(group_id) = target_group_id {
            if !self.focus_group(group_id) {
                return None;
            }
        }

        let session_id = self.add_session_to_focused_group();
        Some((self.focused_group, session_id))
    }

    pub(crate) fn add_session_to_focused_group(&mut self) -> CommandSessionId {
        let session_id = self.allocate_session_id();
        self.add_titled_session_to_focused_group(
            session_id,
            COMMAND_PANE_DEFAULT_SESSION_TITLE.to_string(),
        );
        session_id
    }

    pub(crate) fn add_titled_session_to_focused_group(&mut self, session_id: CommandSessionId, title: String) {
        self.terminal_sessions
            .push(CommandTerminalSession::placeholder(session_id, title));
        let tab = CommandPaneTab { session_id };
        self.insert_created_tab_for_untargeted_creation(tab, session_id);
    }

    pub(crate) fn live_group_for_untargeted_creation(&self) -> Option<CommandPaneGroupId> {
        /*
        CDXC:GPUICommandPaneInsertion 2026-06-26-04:29:
        Untargeted New Terminal creation must recover from a stale `focused_group` by using the first live command group, while explicit clicked-group creation still rejects missing targets before this path. This preserves the existing command split tree instead of replacing it with a new root leaf.

        CDXC:GPUICommandPaneInsertion 2026-06-27-04:36:
        Terminal Action creation no longer uses this focused-group fallback: newly-created non-reused Action tabs need native `createCommandTerminal(... focusAfterCreate:false)` placement, while Cmd+T/New Terminal keeps the focused command-group insertion rule.
        */
        self.find_leaf(self.focused_group)
            .filter(|leaf| !leaf.tab_group.tabs.is_empty())
            .map(|leaf| leaf.group_id)
            .or_else(|| first_command_leaf_id(&self.root))
    }

    pub(crate) fn insert_created_tab_for_untargeted_creation(
        &mut self,
        tab: CommandPaneTab,
        session_id: CommandSessionId,
    ) -> CommandPaneGroupId {
        if let Some(group_id) = self.live_group_for_untargeted_creation() {
            return self.insert_created_tab_into_group(group_id, tab, session_id);
        }

        self.replace_empty_command_layout_with_created_tab(tab, session_id)
    }

    pub(crate) fn insert_created_action_tab_for_untargeted_creation(
        &mut self,
        tab: CommandPaneTab,
        session_id: CommandSessionId,
    ) -> CommandPaneGroupId {
        /*
        CDXC:GPUICommandPaneActions 2026-06-27-04:36:
        Newly-created non-reused terminal Actions follow native untargeted Action placement, not Cmd+T focus placement: an empty command layout creates the first owner, a single owner appends as a tab, and an existing split gets a new selected rightmost command owner without moving existing group memberships.
        */
        match command_node_leaf_count(&self.root) {
            0 => self.replace_empty_command_layout_with_created_tab(tab, session_id),
            1 => {
                let group_id = first_command_leaf_id(&self.root)
                    .expect("single live command layout must have a command group");
                self.insert_created_tab_into_group(group_id, tab, session_id)
            }
            _ => self.insert_created_action_tab_as_rightmost_owner(tab, session_id),
        }
    }

    pub(crate) fn insert_created_tab_into_group(
        &mut self,
        group_id: CommandPaneGroupId,
        tab: CommandPaneTab,
        session_id: CommandSessionId,
    ) -> CommandPaneGroupId {
        let leaf = self
            .find_leaf_mut(group_id)
            .expect("created command tab target group must exist");
        /*
        CDXC:GPUICommandPaneInsertion 2026-06-25-19:27:
        Native command-panel New Terminal uses `targetSessionId` only to find the command tab group; `addCommandSessionToPaneTabGroup` appends the new command session to the end of that group and then selects it. Keep `insert_session_at` as the exact-index API for command tab-strip transfer and reorder paths.

        CDXC:GPUICommandPaneActions 2026-06-27-04:36:
        Terminal Action creation shares this append path only when the command layout has a single live owner. Split layouts must create a separate Action owner so stale or unrelated command focus cannot pull the Action tab into an existing group.
        */
        leaf.tab_group
            .insert_session_at(tab, leaf.tab_group.tabs.len());
        leaf.tab_group.active_session = session_id;
        self.set_focused_group_for_selected_owner(group_id);
        self.clear_focus_mode_if_invalid();
        self.expand();
        group_id
    }

    pub(crate) fn replace_empty_command_layout_with_created_tab(
        &mut self,
        tab: CommandPaneTab,
        session_id: CommandSessionId,
    ) -> CommandPaneGroupId {
        let group_id = self.allocate_group_id();
        self.root = CommandPaneNode::Leaf(CommandPaneLeaf {
            group_id,
            tab_group: CommandPaneTabGroup {
                tabs: vec![tab],
                active_session: session_id,
            },
        });
        self.focused_group = group_id;
        self.focus_mode_group = None;
        self.expand();
        group_id
    }

    pub(crate) fn insert_created_action_tab_as_rightmost_owner(
        &mut self,
        tab: CommandPaneTab,
        session_id: CommandSessionId,
    ) -> CommandPaneGroupId {
        /*
        CDXC:GPUICommandPaneActions 2026-06-27-04:36:
        Native `appendCommandSessionToPaneLayout` appends untargeted Action creation to an existing split as a separate rightmost command owner. GPUI represents that by wrapping the current command root as the first branch and the new Action leaf as the second branch, preserving all existing tab groups and their internal selections.
        */
        let existing_leaf_count = command_node_leaf_count(&self.root).max(1);
        let group_id = self.allocate_group_id();
        let split_id = self.allocate_split_id();
        let existing_root = std::mem::replace(&mut self.root, command_pane_dummy_node());
        self.root = CommandPaneNode::Split(CommandPaneSplit {
            id: split_id,
            axis: WorkspaceSplitAxis::Horizontal,
            ratio: workspace_split_ratio(
                existing_leaf_count as f32 / (existing_leaf_count + 1) as f32,
            ),
            first: Box::new(existing_root),
            second: Box::new(CommandPaneNode::Leaf(CommandPaneLeaf {
                group_id,
                tab_group: CommandPaneTabGroup {
                    tabs: vec![tab],
                    active_session: session_id,
                },
            })),
        });
        self.set_focused_group_for_selected_owner(group_id);
        self.clear_focus_mode_if_invalid();
        self.expand();
        group_id
    }

    pub(crate) fn select_or_create_action_session(
        &mut self,
        command_id: String,
        title: String,
    ) -> CommandPaneActionSessionSelection {
        /*
        CDXC:GPUICommandPane 2026-06-24-23:36:
        GPUI sidebar/titlebar terminal Actions own one live command-pane tab per
        Action. Idle owners rerun in place; active owners are selected without a
        duplicate run. The command id and run id remain process-memory ownership
        only, while restored shell state can reclaim daemon identity separately.
        */
        if let Some((kind, group_id, session_id)) =
            self.find_reusable_action_session(&command_id, &title)
        {
            self.select_session_in_group(group_id, session_id);
            if let Some(session) = self.session_mut(session_id) {
                session.title = title;
                session.action_command_id = Some(command_id);
            }
            self.expand();
            return CommandPaneActionSessionSelection {
                kind,
                group_id,
                session_id,
            };
        }

        self.prune_stale_existing_action_sessions_before_new_run(&command_id);
        let session_id = self.allocate_session_id();
        self.terminal_sessions.push(
            CommandTerminalSession::placeholder(session_id, title)
                .with_action_command_id(command_id),
        );
        let tab = CommandPaneTab { session_id };
        let group_id = self.insert_created_action_tab_for_untargeted_creation(tab, session_id);
        CommandPaneActionSessionSelection {
            kind: CommandPaneActionSessionSelectionKind::Created,
            group_id,
            session_id,
        }
    }

    pub(crate) fn prune_stale_existing_action_sessions_before_new_run(&mut self, command_id: &str) -> bool {
        /*
        CDXC:GPUICommandPaneActions 2026-06-27-06:10:
        Native `runNativeSidebarCommand` closes an existing mapped Action session before creating a replacement when that mapped terminal is no longer running. Match that only for exact same-command sleeping or orphaned GPUI command tabs; keep running non-idle tabs alive, let idle running tabs reuse earlier, and do not prune title-only restored candidates for other command ids.
        */
        let tab_groups = self
            .flat_tab_ids()
            .into_iter()
            .map(|(group_id, session_id)| (session_id, group_id))
            .collect::<HashMap<_, _>>();
        let stale_sessions = self
            .terminal_sessions
            .iter()
            .filter(|session| session.action_command_id.as_deref() == Some(command_id))
            .filter_map(|session| {
                let group_id = tab_groups.get(&session.id).copied();
                (session.is_sleeping || group_id.is_none()).then_some((group_id, session.id))
            })
            .collect::<Vec<_>>();
        let mut changed = false;
        for (group_id, session_id) in stale_sessions {
            if let Some(group_id) = group_id {
                changed |= self.close_session(group_id, session_id);
            } else {
                let before_len = self.terminal_sessions.len();
                self.terminal_sessions
                    .retain(|session| session.id != session_id);
                changed |= self.terminal_sessions.len() != before_len;
            }
        }
        if self.terminal_sessions.is_empty() {
            self.root = command_pane_dummy_node();
            self.focused_group = CommandPaneGroupId(0);
            self.focus_mode_group = None;
            self.collapse();
        }
        changed
    }

    pub(crate) fn find_reusable_action_session(
        &self,
        command_id: &str,
        title: &str,
    ) -> Option<(
        CommandPaneActionSessionSelectionKind,
        CommandPaneGroupId,
        CommandSessionId,
    )> {
        /*
        CDXC:GPUICommandPane 2026-06-25-11:18:
        MacOS command Actions reuse one idle command-pane tab per normalized Action title after restore, even when the live command-id map is missing. Keep the exact command-id match first, then allow idle title-owned reuse regardless of stale/missing action id; duplicate Action titles are rejected at save time, and run-start rewrites the live mapping.

        CDXC:GPUICommandPaneActions 2026-08-08:
        Idle sleeping/restored Action tabs are reusable too. Run start wakes the
        selected session before mounted-surface detection, and an unmounted reuse
        goes through the exact existing gxserver attach slot with startup text.
        Exact command-id ownership wins even if the Action is active; that tab is
        selected without launching another run. Title-only restore matching stays
        idle-only so it cannot claim an unrelated working terminal.
        */
        let title_key = gpui_command_action_title_key(title);
        if title_key.is_empty() {
            return None;
        }
        let tabs = self.flat_tab_ids();
        if let Some((group_id, session_id)) =
            tabs.iter().copied().find(|(_group_id, session_id)| {
                self.session(*session_id)
                    .is_some_and(|session| session.action_command_id.as_deref() == Some(command_id))
            })
        {
            let kind = if self
                .session(session_id)
                .is_some_and(command_session_is_reusable_for_action)
            {
                CommandPaneActionSessionSelectionKind::Reused
            } else {
                CommandPaneActionSessionSelectionKind::ReusedActive
            };
            return Some((kind, group_id, session_id));
        }

        tabs.iter()
            .copied()
            .find(|(_group_id, session_id)| {
                self.session(*session_id).is_some_and(|session| {
                    command_session_is_reusable_for_action(session)
                        && gpui_command_action_title_key(&session.title) == title_key
                })
            })
            .map(|(group_id, session_id)| {
                (
                    CommandPaneActionSessionSelectionKind::Reused,
                    group_id,
                    session_id,
                )
            })
    }

    pub(crate) fn action_session_slot_for_command_id(
        &self,
        command_id: &str,
    ) -> Option<(CommandPaneGroupId, CommandSessionId)> {
        /*
        CDXC:GPUICommandPane 2026-06-25-10:34:
        `endSidebarCommandRun` mirrors macOS's one mapped command-pane session per Action. Prefer the selected matching tab, then an active matching run, then any matching action-owned tab; restored ownership may use only the validated bounded Action selector, never titles, command text, status paths, terminal output, or project paths.

        CDXC:GPUICommandPaneActions 2026-06-26-04:16:
        The selected Action tab preference is responder-style focus and must use only the live focused command group. A stale `focused_group` must not choose the first-group fallback as the selected tab, so command-run-end cleanup falls through to active matching runs before any idle action-owned tab.

        CDXC:GPUICommandPaneActions 2026-06-27-05:59:
        Run-end lookup trusts the current command-button owner only. Older same-command tabs whose `action_command_id` was invalidated by a newer run are intentionally invisible here even when selected or still clearing their own status file.
        */
        let selected = self
            .focused_group_active_session_id()
            .filter(|(_group_id, session_id)| {
                self.session(*session_id)
                    .is_some_and(|session| session.action_command_id.as_deref() == Some(command_id))
            });
        selected
            .or_else(|| {
                self.flat_tab_ids()
                    .into_iter()
                    .find(|(_group_id, session_id)| {
                        self.session(*session_id).is_some_and(|session| {
                            session.action_command_id.as_deref() == Some(command_id)
                                && session.action_run_id.is_some()
                        })
                    })
            })
            .or_else(|| {
                self.flat_tab_ids()
                    .into_iter()
                    .find(|(_group_id, session_id)| {
                        self.session(*session_id).is_some_and(|session| {
                            session.action_command_id.as_deref() == Some(command_id)
                        })
                    })
            })
    }

    pub(crate) fn take_action_session_slot_for_action_close(
        &mut self,
        command_id: &str,
    ) -> Option<(CommandPaneGroupId, CommandSessionId)> {
        /*
        CDXC:GPUICommandPaneActions 2026-06-27-06:21:
        Deleting an Action in Settings must mirror native `deleteSidebarCommand`: clear the current command-to-session ownership before closing the mapped command tab. If a mounted terminal needs close confirmation and remains visible temporarily, it must no longer emit command-button completion feedback, timers, or HUD command-session mapping for the deleted command.

        CDXC:GPUICommandPaneActions 2026-06-27-06:41:
        Shared `endSidebarCommandRun` uses the same ownership split as native `closeNativeSidebarCommandSession`: clear Action ownership before requesting the terminal close, so close-confirm survivors cannot remain the current command id owner or keep private run/status-file state alive.
        */
        let slot = self.action_session_slot_for_command_id(command_id)?;
        let session = self.session_mut(slot.1)?;
        session.activity = CommandTerminalActivity::Idle;
        session.action_command_id = None;
        session.action_close_terminal_on_exit = false;
        session.action_play_completion_sound = false;
        session.action_run_id = None;
        session.action_status_file_path = None;
        Some(slot)
    }

    pub(crate) fn clear_action_run_for_session(&mut self, session_id: CommandSessionId) -> bool {
        let Some(session) = self.session_mut(session_id) else {
            return false;
        };
        let changed = session.action_run_id.is_some()
            || session.action_close_terminal_on_exit
            || session.action_status_file_path.is_some()
            || session.activity != CommandTerminalActivity::Idle;
        session.activity = CommandTerminalActivity::Idle;
        session.action_close_terminal_on_exit = false;
        session.action_run_id = None;
        session.action_status_file_path = None;
        changed
    }

    pub(crate) fn close_completed_action_run_tab(
        &mut self,
        completion: &CommandPaneActionRunCompletion,
    ) -> Option<CommandPaneActionRunCompletedTab> {
        if !gpui_command_pane_action_runtime_close_terminal_on_exit(
            completion.close_terminal_on_exit,
        ) {
            return None;
        }
        let completed_tab = completion.completed_tab?;
        let Some(session) = self.session(completed_tab.session_id) else {
            return None;
        };
        if session.action_command_id.as_deref() != Some(completion.command_id.as_str())
            || session.action_run_id.is_some()
            || session.action_status_file_path.is_some()
            || session.activity != CommandTerminalActivity::Idle
        {
            return None;
        }
        self.close_session(completed_tab.group_id, completed_tab.session_id)
            .then_some(completed_tab)
    }

    pub(crate) fn take_action_run_completion_for_exited_session(
        &mut self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
    ) -> Option<CommandPaneActionRunCompletion> {
        /*
        CDXC:GPUICommandPane 2026-06-25-11:11:
        If a mapped command-pane terminal exits before the status-file poller clears the run, GPUI must still finish sidebar Action feedback like macOS terminal-exit cleanup. Trust only the matching session-state file when it already reached idle; otherwise report the live run as error so button feedback cannot remain running after the command-pane surface is gone. Do not read terminal output, command text, cwd/env, project paths, logs, or shell-state JSON.
        */
        let session = self.session_mut(session_id)?;
        let command_id = session.action_command_id.clone()?;
        let run_id = session.action_run_id.clone()?;
        let close_terminal_on_exit = gpui_command_pane_action_runtime_close_terminal_on_exit(
            session.action_close_terminal_on_exit,
        );
        let play_completion_sound = session.action_play_completion_sound;
        let exit_code = session
            .action_status_file_path
            .as_ref()
            .and_then(|status_file_path| gpui_command_action_status_from_file(status_file_path))
            .filter(|status| {
                status.run_id == run_id && status.status == GpuiCommandActionRunFileStatus::Idle
            })
            .map(|status| status.exit_code)
            .unwrap_or(1);
        session.activity = CommandTerminalActivity::Idle;
        session.action_close_terminal_on_exit = false;
        session.action_run_id = None;
        session.action_status_file_path = None;
        Some(CommandPaneActionRunCompletion {
            close_terminal_on_exit,
            command_id,
            completed_tab: Some(CommandPaneActionRunCompletedTab {
                group_id,
                session_id,
            }),
            exit_code,
            play_completion_sound,
            run_id,
        })
    }

    pub(crate) fn mark_action_session_run_started(
        &mut self,
        session_id: CommandSessionId,
        command_id: String,
        title: String,
        run_id: String,
        status_file_path: PathBuf,
        play_completion_sound: bool,
        close_terminal_on_exit: bool,
    ) -> bool {
        /*
        CDXC:GPUICommandPaneActions 2026-06-26-06:28:
        Native `setNativeSidebarCommandPaneTitle` run-start parity requires reused/restored Action tabs to carry the current live Action title and command id, clear stale Delayed Send chrome, and show Working without allocating a replacement tab.

        CDXC:GPUICommandPaneActions 2026-06-27-05:59:
        Native command-pane Action ownership has one current `commandId -> session` mapping. Starting a newer same-command run clears only `action_command_id` on older same-command sessions, preserving their run id, status-file path, activity, and title so status refresh can clear their local Working state without sidebar completion feedback.
        */
        let Some(target_index) = self
            .terminal_sessions
            .iter()
            .position(|session| session.id == session_id)
        else {
            return false;
        };
        for (index, session) in self.terminal_sessions.iter_mut().enumerate() {
            if index != target_index
                && session.action_command_id.as_deref() == Some(command_id.as_str())
            {
                session.action_command_id = None;
            }
        }
        let session = &mut self.terminal_sessions[target_index];
        session.title = title;
        session.activity = CommandTerminalActivity::Working;
        session.is_sleeping = false;
        session.delayed_send_active = false;
        session.delayed_send_timer_owned = false;
        session.action_command_id = Some(command_id);
        session.action_close_terminal_on_exit =
            gpui_command_pane_action_runtime_close_terminal_on_exit(close_terminal_on_exit);
        session.action_play_completion_sound = play_completion_sound;
        session.action_run_id = Some(run_id);
        session.action_status_file_path = Some(status_file_path);
        true
    }

    pub(crate) fn refresh_action_run_states_from_status_files(&mut self) -> CommandPaneActionRunRefresh {
        /*
        CDXC:GPUICommandPane 2026-06-24-23:36:
        Command Action completion is observed only through the same session-state file env contract the hidden shell wrapper writes. Refreshing this state may change a live tab's safe activity enum and clear its run id. Shell state may retain only the validated bounded Action selector needed for restart reuse; command text, output, paths, env, tokens, run ids, and status-file paths remain runtime-only.

        CDXC:GPUICommandPane 2026-06-24-23:49:
        The refresh result carries only command id, run id, exit code, and the saved per-action sound flag so the app can mirror macOS button success/error feedback and action completion sounds. Do not include command text, cwd, env, terminal output, status-file paths, project names, or renderer payloads in the completion record.

        CDXC:GPUICommandPaneActions 2026-06-26-04:59:
        Command-pane Action completions normalize legacy close-on-exit requests to false at the poller boundary. The status-file poller may still return safe command group/session ids for feedback plumbing, but it must leave the completed command tab alive for reuse and must not persist or report command text, status-file paths, cwd/env, terminal output, or project paths.

        CDXC:GPUICommandPaneActions 2026-06-27-05:07:
        A matching `status=working` file is live Action ownership evidence only: keep the tab Working, retain the in-memory command id/run id/status path for the poller, emit no completion, and ignore unrelated status-file keys. Only a matching idle stamp or exact exit cleanup may clear ownership and post completion feedback, and neither path may infer status from shell titles, output, paths, command text, env, logs, or persisted shell JSON.

        CDXC:GPUICommandPaneActions 2026-06-27-05:59:
        Superseded same-command Action sessions may keep a run id and status-file path after losing `action_command_id`. Their idle status refresh clears only local runtime state; without current command ownership it must not emit command-button completion feedback or expose private status-file fields.
        */
        let mut refresh = CommandPaneActionRunRefresh::default();
        let completed_tabs = self
            .flat_tab_ids()
            .into_iter()
            .map(|(group_id, session_id)| {
                (
                    session_id,
                    CommandPaneActionRunCompletedTab {
                        group_id,
                        session_id,
                    },
                )
            })
            .collect::<HashMap<_, _>>();
        for session in &mut self.terminal_sessions {
            let Some(run_id) = session.action_run_id.as_deref() else {
                continue;
            };
            let Some(status_file_path) = session.action_status_file_path.as_ref() else {
                continue;
            };
            let Some(status) = gpui_command_action_status_from_file(status_file_path) else {
                continue;
            };
            if status.run_id != run_id {
                continue;
            }
            match status.status {
                GpuiCommandActionRunFileStatus::Working => {
                    if session.activity != CommandTerminalActivity::Working {
                        session.activity = CommandTerminalActivity::Working;
                        refresh.changed = true;
                    }
                }
                GpuiCommandActionRunFileStatus::Idle => {
                    if session.activity != CommandTerminalActivity::Idle {
                        session.activity = CommandTerminalActivity::Idle;
                        refresh.changed = true;
                    }
                    if let Some(command_id) = session.action_command_id.clone() {
                        refresh.completions.push(CommandPaneActionRunCompletion {
                            close_terminal_on_exit:
                                gpui_command_pane_action_runtime_close_terminal_on_exit(
                                    session.action_close_terminal_on_exit,
                                ),
                            command_id,
                            completed_tab: completed_tabs.get(&session.id).copied(),
                            exit_code: status.exit_code,
                            play_completion_sound: session.action_play_completion_sound,
                            run_id: status.run_id.clone(),
                        });
                    }
                    session.action_close_terminal_on_exit = false;
                    session.action_run_id = None;
                    session.action_status_file_path = None;
                }
            }
        }
        refresh
    }

    pub(crate) fn has_active_action_runs(&self) -> bool {
        self.terminal_sessions
            .iter()
            .any(|session| session.action_run_id.is_some())
    }

    pub(crate) fn split_session_adjacent_to_focused_group(
        &mut self,
        direction: FocusedTerminalSplitDirection,
    ) -> Option<(CommandPaneGroupId, CommandSessionId)> {
        /*
        CDXC:GPUIFocusedSplits 2026-06-25-16:05:
        Native command panels intentionally coerce both Cmd+D and Cmd+Shift+D to horizontal command splits. Keep GPUI command hotkey splits beside the focused command group while still storing split axis metadata for layout restore; do not create Agents tabs, processes, or terminal content.
        */
        let target_group_id = self
            .find_leaf(self.focused_group)
            .filter(|leaf| !leaf.tab_group.tabs.is_empty())
            .map(|leaf| leaf.group_id)?;
        let axis = command_pane_focused_split_axis(direction);
        let session_id = self.allocate_session_id();
        let group_id = self.allocate_group_id();
        let split_id = self.allocate_split_id();
        let new_leaf = CommandPaneLeaf {
            group_id,
            tab_group: CommandPaneTabGroup {
                tabs: vec![CommandPaneTab { session_id }],
                active_session: session_id,
            },
        };

        if insert_command_leaf_split(
            &mut self.root,
            target_group_id,
            new_leaf,
            axis,
            false,
            split_id,
        ) {
            self.terminal_sessions
                .push(CommandTerminalSession::placeholder(
                    session_id,
                    COMMAND_PANE_DEFAULT_SESSION_TITLE.to_string(),
                ));
            self.focus_mode_group = None;
            self.focused_group = group_id;
            self.expand();
            Some((group_id, session_id))
        } else {
            None
        }
    }

    pub(crate) fn add_placeholder_session_from_workspace_title(
        &mut self,
        target_group_id: CommandPaneGroupId,
        title: String,
        zone: WorkspaceDropZone,
    ) -> Option<(CommandPaneGroupId, CommandSessionId)> {
        /*
        CDXC:GPUICommandPaneDragDrop 2026-06-22-13:05:
        Workspace-to-command drops are command-pane placeholder creation, not command-tab movement. Allocate a command-only session id, keep the dragged Agents tab title for the live placeholder label, and map top/bottom intent to center grouping because command panes support only tab grouping and left/right horizontal splits.
        */
        match zone {
            WorkspaceDropZone::Left | WorkspaceDropZone::Right => {
                self.split_placeholder_session_from_workspace_title(target_group_id, title, zone)
            }
            WorkspaceDropZone::Center | WorkspaceDropZone::Top | WorkspaceDropZone::Bottom => {
                self.group_placeholder_session_from_workspace_title(target_group_id, title)
            }
        }
    }

    pub(crate) fn group_placeholder_session_from_workspace_title(
        &mut self,
        target_group_id: CommandPaneGroupId,
        title: String,
    ) -> Option<(CommandPaneGroupId, CommandSessionId)> {
        let insertion_index = self.find_leaf(target_group_id)?.tab_group.tabs.len();
        self.insert_placeholder_session_from_workspace_title_at(
            target_group_id,
            insertion_index,
            title,
        )
    }

    pub(crate) fn insert_placeholder_session_from_workspace_title_at(
        &mut self,
        target_group_id: CommandPaneGroupId,
        insertion_index: usize,
        title: String,
    ) -> Option<(CommandPaneGroupId, CommandSessionId)> {
        /*
        CDXC:GPUICommandPaneDragDrop 2026-06-22-16:18:
        Agents-to-command tab-strip drops are grouping operations at a command tab boundary, not command split operations. Insert a command-only placeholder with the visible Agents title at the requested index, select it, focus/expand the target group, and keep all real terminal/process/content state on the Agents side out of the command model.
        */
        self.find_leaf(target_group_id)?;
        let session_id = self.allocate_session_id();
        self.terminal_sessions
            .push(CommandTerminalSession::placeholder(session_id, title));
        let tab = CommandPaneTab { session_id };

        let Some(target_leaf) = self.find_leaf_mut(target_group_id) else {
            self.terminal_sessions
                .retain(|session| session.id != session_id);
            return None;
        };
        target_leaf
            .tab_group
            .insert_session_at(tab, insertion_index);
        target_leaf.tab_group.active_session = session_id;
        self.set_focused_group_for_selected_owner(target_group_id);
        self.clear_focus_mode_if_invalid();
        self.expand();
        Some((target_group_id, session_id))
    }

    pub(crate) fn split_placeholder_session_from_workspace_title(
        &mut self,
        target_group_id: CommandPaneGroupId,
        title: String,
        zone: WorkspaceDropZone,
    ) -> Option<(CommandPaneGroupId, CommandSessionId)> {
        if !matches!(zone, WorkspaceDropZone::Left | WorkspaceDropZone::Right) {
            return self.group_placeholder_session_from_workspace_title(target_group_id, title);
        }
        self.find_leaf(target_group_id)?;

        let session_id = self.allocate_session_id();
        let group_id = self.allocate_group_id();
        let split_id = self.allocate_split_id();
        let new_leaf = CommandPaneLeaf {
            group_id,
            tab_group: CommandPaneTabGroup {
                tabs: vec![CommandPaneTab { session_id }],
                active_session: session_id,
            },
        };
        let dragged_first = matches!(zone, WorkspaceDropZone::Left);

        if insert_command_leaf_split(
            &mut self.root,
            target_group_id,
            new_leaf,
            WorkspaceSplitAxis::Horizontal,
            dragged_first,
            split_id,
        ) {
            self.terminal_sessions
                .push(CommandTerminalSession::placeholder(session_id, title));
            self.focus_mode_group = None;
            self.focused_group = group_id;
            self.expand();
            Some((group_id, session_id))
        } else {
            None
        }
    }

    pub(crate) fn flat_tab_ids(&self) -> Vec<(CommandPaneGroupId, CommandSessionId)> {
        let mut tabs = Vec::new();
        collect_command_tabs(&self.root, &mut tabs);
        tabs
    }

    pub(crate) fn pane_owner_session_ids(&self) -> HashSet<(CommandPaneGroupId, CommandSessionId)> {
        /*
        CDXC:GPUICommandPaneAutoSleep 2026-06-27-06:53:
        Native Auto Sleep protects the selected owner of each visible command-panel split leaf, while HUD focus remains responder-exact. Derive this set from explicit pane-layout active tabs, not from focused_group fallback, so split siblings can stay protected without becoming `isActive`.
        */
        self.group_order()
            .into_iter()
            .filter_map(|group_id| {
                let leaf = self.find_leaf(group_id)?;
                self.visible_command_body_owner_for_leaf(leaf)
                    .map(|owner| (owner.group_id, owner.session_id))
            })
            .collect()
    }

    pub(crate) fn sidebar_command_session_sources(
        &self,
        command_pane_focused: bool,
        delayed_send_timers: &HashMap<CommandSessionId, GpuiCommandDelayedSendTimer>,
        close_after_done_timers: &HashMap<CommandSessionId, GpuiCommandCloseAfterDoneTimer>,
        now: SystemTime,
    ) -> serde_json::Value {
        /*
        CDXC:GPUICommandPane 2026-06-25-10:50:
        GPUI Sidebar command-session indicators need the same live command-pane session matching as macOS. Export only sanitized command-pane summary fields: external `G{u64}` session ids, normalized title, lifecycle-style HUD status, and focused-tab boolean. Do not include command text, cwd, env, status-file paths, terminal output, shell-state JSON, or project paths.

        CDXC:GPUICommandTabSleep 2026-06-25-14:27:
        Sleeping command tabs stay represented in the GPUI sidebar bridge with a boolean lifecycle marker while their command activity is idle. Keep the bridge sanitized to ids, normalized title, enum status, focus, action command id, and isSleeping only.

        CDXC:GPUICommandPaneTimers 2026-06-25-17:09:
        Native projects Delayed Send and Close After Done timer state into sidebar/titlebar terminal rows. GPUI command indicators should carry only the same safe timer fields: armed booleans, UTC deadlines, remaining labels, and remaining milliseconds. Keep command text, terminal output, paths, run ids, status files, titles beyond the visible sanitized label, and shell-state JSON out of this bridge.

        CDXC:GPUICommandPaneFocus 2026-06-26-04:15:
        Sidebar and app-modal commandSessionIndicator active state mirrors responder-exact command focus. Mark a command tab active only when shell focus is in the command pane and `focused_group` still resolves to a live command group; stale command focus and non-command focus must export every indicator with isActive=false instead of falling back to the first command group.

        CDXC:GPUICommandSessionHud 2026-06-27-06:30:
        Native HUD status is terminal lifecycle-derived: running terminals are running, error lifecycle is error, and non-running lifecycle is idle. GPUI local command tabs currently expose only awake/sleeping lifecycle, so project awake tabs as running and sleeping tabs as idle; Action Attention remains separate button feedback and must not make HUD sessions error.

        CDXC:GPUICommandPaneAutoSleep 2026-06-27-06:53:
        `isActive` is reserved for responder/focused-HUD state. Export `isPaneOwner` separately from the command layout owner set so native Auto Sleep can protect every active split owner without treating unfocused split siblings as HUD-active.
        */
        let active = if command_pane_focused {
            self.focused_group_active_session_id()
        } else {
            None
        };
        let pane_owner_session_ids = self.pane_owner_session_ids();
        serde_json::Value::Array(
            self.flat_tab_ids()
                .into_iter()
                .filter_map(|(group_id, session_id)| {
                    let session = self.session(session_id)?;
                    let title = gpui_command_pane_sidebar_indicator_text(&session.title)?;
                    let mut summary = serde_json::json!({
                        "isActive": active == Some((group_id, session_id)),
                        "isSleeping": session.is_sleeping,
                        "sessionId": gpui_command_session_external_id(session.id),
                        "status": session.sidebar_hud_indicator_status(),
                        "title": title,
                    });
                    if pane_owner_session_ids.contains(&(group_id, session_id)) {
                        summary["isPaneOwner"] = serde_json::json!(true);
                    }
                    if let Some(command_id) = session
                        .action_command_id
                        .as_deref()
                        .and_then(gpui_command_pane_sidebar_indicator_text)
                    {
                        summary["commandId"] = serde_json::json!(command_id);
                    }
                    if let Some(timer) = delayed_send_timers.get(&session_id).copied() {
                        let remaining_ms = timer.remaining_ms(now);
                        summary["delayedSendDeadlineAt"] =
                            serde_json::json!(gpui_iso8601_utc(timer.deadline_at));
                        summary["delayedSendRemainingLabel"] = serde_json::json!(
                            gpui_command_delayed_send_countdown_label(remaining_ms,)
                        );
                        summary["delayedSendRemainingMs"] = serde_json::json!(remaining_ms);
                    }
                    if session.close_after_done_armed {
                        summary["closeAfterDone"] = serde_json::json!(true);
                        if let Some(timer) = close_after_done_timers.get(&session_id).copied() {
                            let remaining_ms = timer.remaining_ms(now);
                            summary["closeAfterDoneDeadlineAt"] =
                                serde_json::json!(gpui_iso8601_utc(timer.deadline_at));
                            summary["closeAfterDoneRemainingLabel"] = serde_json::json!(
                                gpui_command_delayed_send_countdown_label(remaining_ms,)
                            );
                            summary["closeAfterDoneRemainingMs"] = serde_json::json!(remaining_ms);
                        }
                    }
                    Some(summary)
                })
                .collect(),
        )
    }

    pub(crate) fn group_order(&self) -> Vec<CommandPaneGroupId> {
        let mut group_ids = Vec::new();
        collect_command_leaf_ids(&self.root, &mut group_ids);
        group_ids
    }

    pub(crate) fn visible_command_body_owner_for_leaf(
        &self,
        leaf: &CommandPaneLeaf,
    ) -> Option<CommandPaneVisibleBodyOwner> {
        /*
        CDXC:GPUICommandTerminalSurface 2026-06-27-04:36:
        GPUI command-pane body ownership mirrors native `visibleCommandPaneOwnerSessionIds`: an expanded command group gives its visible body to the stored selected command tab only when that exact tab still has a stored session. Sleeping selected tabs own a placeholder body without a Ghostty mount slot, and stale active ids must not fall back to sibling tabs.
        */
        if !self.is_expanded() {
            return None;
        }

        let session_id = leaf.tab_group.active_session;
        if !leaf.tab_group.has_session(session_id) {
            return None;
        }

        let session = self.session(session_id)?;
        Some(CommandPaneVisibleBodyOwner {
            group_id: leaf.group_id,
            session_id,
            is_sleeping: session.is_sleeping,
        })
    }

    pub(crate) fn rendered_terminal_body_mount_slots(&self) -> Vec<CommandTerminalBodyMountSlotId> {
        /*
        CDXC:GPUICommandTerminalSurface 2026-06-23-05:03:
        Real command-pane terminal bodies are limited to the expanded command pane and the active tab in each visible command group. Inactive command tabs, collapsed strip tabs, missing sessions, and command titles/status are intentionally excluded so command Ghostty surfaces stay body-bounds-driven and runtime-only.

        CDXC:GPUICommandTabSleep 2026-06-25-14:27:
        Sleeping command tabs remain in the tab/group model but are not renderable body mount slots. Withhold their command terminal body until an explicit body activation wakes the session.

        CDXC:GPUICommandFocusMode 2026-06-25-21:40:
        Command Focus mode filters the mounted/rendered command body slots to the focused command group only after eligibility is computed from the full split tree. This preserves the reversible command split layout while preventing hidden command groups from retaining native terminal hosts or Ghostty focus.

        CDXC:GPUICommandTerminalSurface 2026-06-27-04:36:
        Rendered command mount slots are the non-sleeping subset of explicit visible command body owners. Sleeping owners remain visible placeholders, while missing sessions, stale selected ids, inactive siblings, and collapsed panes produce no Ghostty host slot.
        */
        let slots = self.rendered_terminal_body_mount_slots_without_focus();
        match self.focus_mode_group {
            Some(focus_group_id)
                if self.focus_mode_eligible_group_count_without_focus() > 1
                    && slots
                        .iter()
                        .any(|slot_id| slot_id.group_id == focus_group_id) =>
            {
                slots
                    .into_iter()
                    .filter(|slot_id| slot_id.group_id == focus_group_id)
                    .collect()
            }
            _ => slots,
        }
    }

    pub(crate) fn is_current_terminal_body_mount_slot(&self, slot_id: CommandTerminalBodyMountSlotId) -> bool {
        self.rendered_terminal_body_mount_slots()
            .into_iter()
            .any(|current_slot_id| current_slot_id == slot_id)
    }

    pub(crate) fn pane_tab_count(&self, group_id: CommandPaneGroupId) -> Option<usize> {
        self.find_leaf(group_id)
            .map(|leaf| leaf.tab_group.tabs.len())
    }

    pub(crate) fn rendered_terminal_body_mount_slots_without_focus(
        &self,
    ) -> Vec<CommandTerminalBodyMountSlotId> {
        if !self.is_expanded() {
            return Vec::new();
        }

        self.group_order()
            .into_iter()
            .filter_map(|group_id| {
                let leaf = self.find_leaf(group_id)?;
                self.terminal_body_mount_slot_for_leaf(leaf)
            })
            .collect()
    }

    pub(crate) fn terminal_body_mount_slot_for_leaf(
        &self,
        leaf: &CommandPaneLeaf,
    ) -> Option<CommandTerminalBodyMountSlotId> {
        /*
        CDXC:GPUICommandTerminalSurface 2026-06-27-04:36:
        Command-pane Ghostty slots are derived from the visible body-owner helper, not from tab-group fallback selection. A selected non-sleeping command tab may mount a blank pending terminal body; sleeping selected tabs, missing selected sessions, stale active ids, inactive siblings, and collapsed panes must not borrow a mount slot.
        */
        self.visible_command_body_owner_for_leaf(leaf)
            .and_then(CommandPaneVisibleBodyOwner::mount_slot_id)
    }

    pub(crate) fn focus_mode_eligible_group_count_without_focus(&self) -> usize {
        self.rendered_terminal_body_mount_slots_without_focus()
            .len()
    }

    pub(crate) fn group_is_focus_mode_eligible_without_focus(&self, group_id: CommandPaneGroupId) -> bool {
        self.rendered_terminal_body_mount_slots_without_focus()
            .into_iter()
            .any(|slot_id| slot_id.group_id == group_id)
    }

    pub(crate) fn clear_focus_mode_if_invalid(&mut self) -> bool {
        let Some(focus_group_id) = self.focus_mode_group else {
            return false;
        };
        if self.focus_mode_eligible_group_count_without_focus() <= 1
            || !self.group_is_focus_mode_eligible_without_focus(focus_group_id)
        {
            self.focus_mode_group = None;
            true
        } else {
            false
        }
    }

    pub(crate) fn tab_context_allows_focus_mode(
        &self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
    ) -> bool {
        /*
        CDXC:GPUICommandTabContextMenu 2026-06-25-21:29:
        Native command-tab Focus is split-owner Focus mode, not tab selection or command-pane keyboard focus. GPUI allows the row only when the clicked tab belongs to a group with a rendered awake owner and the command pane has more than one rendered awake owner; one command group with multiple tabs does not qualify.
        */
        let Some(leaf) = self.find_leaf(group_id) else {
            return false;
        };
        if !leaf.tab_group.has_session(session_id) {
            return false;
        }

        self.focus_mode_eligible_group_count_without_focus() > 1
            && self.group_is_focus_mode_eligible_without_focus(group_id)
    }

    pub(crate) fn tab_context_focus_row_index(
        &self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
    ) -> Option<usize> {
        self.tab_context_allows_focus_mode(group_id, session_id)
            .then(|| command_pane_tab_context_runtime_action_count(self, group_id, session_id))
    }

    pub(crate) fn tab_strip_reorder_indices(
        &self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
        insertion_index: usize,
    ) -> Option<(usize, usize)> {
        /*
        CDXC:GPUICommandPaneDragDrop 2026-06-25-19:57:
        Native same-group command tab-strip drops interpret the marker index before removing the dragged tab. Adjust forward moves by one after removal, and classify both same-index and adjacent same-slot markers as no-ops so persistence and reorder notifications only represent real user-visible order changes.
        */
        let leaf = self.find_leaf(group_id)?;
        let source_index = leaf
            .tab_group
            .tabs
            .iter()
            .position(|tab| tab.session_id == session_id)?;
        let bounded_insertion_index = insertion_index.min(leaf.tab_group.tabs.len());
        let final_index = if bounded_insertion_index > source_index {
            bounded_insertion_index - 1
        } else {
            bounded_insertion_index
        };
        Some((source_index, final_index))
    }

    pub(crate) fn tab_strip_reorder_changes_order(
        &self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
        insertion_index: usize,
    ) -> bool {
        self.tab_strip_reorder_indices(group_id, session_id, insertion_index)
            .is_some_and(|(source_index, final_index)| final_index != source_index)
    }

    pub(crate) fn reorder_tab_within_group(
        &mut self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
        insertion_index: usize,
    ) -> bool {
        let Some((source_index, final_index)) =
            self.tab_strip_reorder_indices(group_id, session_id, insertion_index)
        else {
            return false;
        };
        if final_index == source_index {
            return false;
        }

        let Some(leaf) = self.find_leaf_mut(group_id) else {
            return false;
        };
        let active_session = leaf.tab_group.active_session;
        let Some(tab) = leaf.tab_group.remove_session(session_id) else {
            return false;
        };
        leaf.tab_group.insert_session_at(tab, final_index);
        leaf.tab_group.active_session = active_session;
        self.set_focused_group_for_selected_owner(group_id);
        true
    }

    pub(crate) fn group_tab_into_group(
        &mut self,
        source_group_id: CommandPaneGroupId,
        target_group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
    ) -> bool {
        if !self.has_session(session_id) || self.find_leaf(target_group_id).is_none() {
            return false;
        }

        if source_group_id == target_group_id {
            return self.select_session_in_group(target_group_id, session_id);
        }

        let Some((tab, source_is_empty)) = self.remove_tab_for_move(source_group_id, session_id)
        else {
            return false;
        };

        if source_is_empty {
            self.collapse_empty_leaf(source_group_id);
        }

        let Some(target_leaf) = self.find_leaf_mut(target_group_id) else {
            return false;
        };
        target_leaf
            .tab_group
            .insert_session_at(tab, target_leaf.tab_group.tabs.len());
        target_leaf.tab_group.active_session = session_id;
        self.set_focused_group_for_selected_owner(target_group_id);
        self.clear_focus_mode_if_invalid();
        true
    }

    pub(crate) fn split_tab_to_group(
        &mut self,
        source_group_id: CommandPaneGroupId,
        target_group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
        zone: WorkspaceDropZone,
    ) -> bool {
        /*
        CDXC:GPUICommandPane 2026-06-22-06:13:
        Command-pane drag/drop is intentionally narrower than Agents workspace drag/drop. Center drops group command tabs into the target command tab group, left/right edge drops create horizontal command splits, and top/bottom intent is treated as center so command panes never create vertical splits in this in-memory slice.

        CDXC:GPUICommandFocusMode 2026-06-26-06:37:
        Command drag/drop must match native Focus-mode ownership. Same-group grouping stays inside the focused command owner, while left/right side drops split the dragged command into a new selected owner, clear command Focus, and render that dragged command immediately.

        CDXC:GPUICommandPaneDragDrop 2026-06-26-06:37:
        Native command-panel same-session body side drops resolve the drop to the first or last remaining tab sibling before removing the dragged tab. GPUI owns only command groups here, so split after removal beside the still-live source group, reject single-tab self side drops, leave the source group order/selection to the normal removal rule, and focus the new dragged split group without touching unrelated groups.
        */
        if !matches!(zone, WorkspaceDropZone::Left | WorkspaceDropZone::Right) {
            return self.group_tab_into_group(source_group_id, target_group_id, session_id);
        }

        if !self.has_session(session_id) || self.find_leaf(target_group_id).is_none() {
            return false;
        }

        if source_group_id == target_group_id
            && self.pane_tab_count(source_group_id).unwrap_or_default() <= 1
        {
            return false;
        }

        let Some((tab, source_is_empty)) = self.remove_tab_for_move(source_group_id, session_id)
        else {
            return false;
        };

        if source_is_empty {
            self.collapse_empty_leaf(source_group_id);
        }

        let group_id = self.allocate_group_id();
        let split_id = self.allocate_split_id();
        let new_leaf = CommandPaneLeaf {
            group_id,
            tab_group: CommandPaneTabGroup {
                tabs: vec![tab],
                active_session: session_id,
            },
        };
        let dragged_first = matches!(zone, WorkspaceDropZone::Left);

        if insert_command_leaf_split(
            &mut self.root,
            target_group_id,
            new_leaf,
            WorkspaceSplitAxis::Horizontal,
            dragged_first,
            split_id,
        ) {
            self.focus_mode_group = None;
            self.focused_group = group_id;
            true
        } else {
            false
        }
    }

    pub(crate) fn remove_tab_for_move(
        &mut self,
        group_id: CommandPaneGroupId,
        session_id: CommandSessionId,
    ) -> Option<(CommandPaneTab, bool)> {
        let leaf = self.find_leaf_mut(group_id)?;
        let tab = leaf.tab_group.remove_session(session_id)?;
        let source_is_empty = leaf.tab_group.tabs.is_empty();
        Some((tab, source_is_empty))
    }

    pub(crate) fn collapse_empty_leaf(&mut self, group_id: CommandPaneGroupId) {
        let root_is_empty = collapse_empty_command_leaf(&mut self.root, group_id);
        if root_is_empty {
            self.root = command_pane_dummy_node();
        }

        if self.focused_group == group_id
            && let Some(first_leaf_id) = first_command_leaf_id(&self.root)
        {
            self.focused_group = first_leaf_id;
        }
        self.clear_focus_mode_if_invalid();
    }

    pub(crate) fn find_leaf(&self, group_id: CommandPaneGroupId) -> Option<&CommandPaneLeaf> {
        find_command_leaf(&self.root, group_id)
    }

    pub(crate) fn find_leaf_mut(&mut self, group_id: CommandPaneGroupId) -> Option<&mut CommandPaneLeaf> {
        find_command_leaf_mut(&mut self.root, group_id)
    }

    pub(crate) fn split_ratio(&self, split_id: CommandPaneSplitId) -> Option<f32> {
        find_command_split(&self.root, split_id).map(|split| workspace_split_ratio(split.ratio))
    }

    pub(crate) fn set_split_ratio(&mut self, split_id: CommandPaneSplitId, ratio: f32) -> bool {
        let next_ratio = workspace_split_ratio(ratio);
        let Some(split) = find_command_split_mut(&mut self.root, split_id) else {
            return false;
        };

        if (workspace_split_ratio(split.ratio) - next_ratio).abs() < 0.001 {
            return false;
        }

        split.ratio = next_ratio;
        true
    }

    pub(crate) fn reset_split_ratio(&mut self, split_id: CommandPaneSplitId) -> bool {
        let Some(default_ratio) = find_command_split(&self.root, split_id)
            .map(|split| command_split_native_default_ratio(split).unwrap_or(0.5))
        else {
            return false;
        };
        self.set_split_ratio(split_id, default_ratio)
    }

    pub(crate) fn split_drag_ratio_bounds(
        &self,
        split_id: CommandPaneSplitId,
        content_span: f32,
    ) -> Option<(f32, f32)> {
        let split = find_command_split(&self.root, split_id)?;
        let minimum = split_pane_resize_minimum_for_axis(split.axis);
        split_drag_ratio_bounds_from_minimums(
            command_node_axis_pane_count(&split.first, split.axis) as f32 * minimum,
            command_node_axis_pane_count(&split.second, split.axis) as f32 * minimum,
            content_span,
        )
    }

    pub(crate) fn allocate_group_id(&mut self) -> CommandPaneGroupId {
        let group_id = CommandPaneGroupId(self.next_group_id);
        self.next_group_id += 1;
        group_id
    }

    pub(crate) fn allocate_split_id(&mut self) -> CommandPaneSplitId {
        let split_id = CommandPaneSplitId(self.next_split_id);
        self.next_split_id += 1;
        split_id
    }

    pub(crate) fn allocate_session_id(&mut self) -> CommandSessionId {
        let session_id = CommandSessionId(self.next_session_id);
        self.next_session_id += 1;
        session_id
    }

    pub(crate) fn collapse(&mut self) {
        if self.is_expanded() {
            self.last_expanded_mode = self.mode;
        }
        self.mode = CommandPaneMode::Collapsed;
        self.focus_mode_group = None;
        self.resize_drag = None;
    }

    pub(crate) fn expand(&mut self) {
        self.mode = command_pane_mode_for_current_release(match self.last_expanded_mode {
            CommandPaneMode::Pinned | CommandPaneMode::Floating => self.last_expanded_mode,
            CommandPaneMode::Collapsed => CommandPaneMode::Pinned,
        });
    }

    pub(crate) fn toggle_expanded(&mut self) {
        if self.is_expanded() {
            self.collapse();
        } else {
            self.expand();
        }
    }

    pub(crate) fn toggle_pinned(&mut self) {
        if !COMMAND_PANE_FLOATING_MODE_ENABLED {
            self.mode = command_pane_mode_for_current_release(self.mode);
            self.last_expanded_mode = CommandPaneMode::Pinned;
            return;
        }

        self.mode = match self.mode {
            CommandPaneMode::Pinned => CommandPaneMode::Floating,
            CommandPaneMode::Floating => CommandPaneMode::Pinned,
            CommandPaneMode::Collapsed => match self.last_expanded_mode {
                CommandPaneMode::Pinned => CommandPaneMode::Floating,
                CommandPaneMode::Floating | CommandPaneMode::Collapsed => CommandPaneMode::Pinned,
            },
        };

        if self.is_expanded() {
            self.last_expanded_mode = self.mode;
        }
    }

    pub(crate) fn reset_height_from_shared_settings(
        &mut self,
        content_height: f32,
        settings: &shared_settings::SharedSidebarSettingsSnapshot,
    ) {
        self.reset_height_with_default_height_px(
            content_height,
            command_pane_default_height_px_from_shared_settings(settings),
        );
    }

    pub(crate) fn reset_height_with_default_height_px(&mut self, content_height: f32, default_height_px: f32) {
        self.height_ratio = command_pane_default_height_ratio_for_default_height_px(
            default_height_px,
            content_height,
        );
        self.resize_drag = None;
    }

    pub(crate) fn reset_width_to_default(&mut self) {
        self.width_ratio = COMMAND_PANE_DEFAULT_WIDTH_RATIO;
        self.resize_drag = None;
    }
}

