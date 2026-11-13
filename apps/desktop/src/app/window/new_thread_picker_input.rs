//! The New Thread picker's keyboard and selection behaviour: moving the highlight, entering and
//! leaving an account list, activating a row and the capture-phase key actions. A child of
//! `new_thread_picker.rs` (included with `#[path]`, so the preview binary builds it too), which is
//! why it reads the picker's private state directly.
use super::*;

impl GpuiNewThreadPickerWindow {
    pub(super) fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        if self.first_project_step(delta, cx) {
            return;
        }
        let rows = self.rows();
        if rows.is_empty() {
            return;
        }
        let count = rows.len() as isize;
        let next = (self.selected as isize + delta).rem_euclid(count) as usize;
        self.selected = next;
        self.scroll
            .scroll_to_item(self.scroll_child_index(&rows, next));
        cx.notify();
    }

    pub(super) fn clear_query(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.query.clear();
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
    }

    pub(super) fn set_placeholder(
        &self,
        placeholder: &'static str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.input.update(cx, |input, cx| {
            input.set_placeholder(placeholder, window, cx);
        });
    }

    pub(super) fn enter_scope(
        &mut self,
        agent_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // A box signs in on its own, so it has no account list.
        if self.run_on.in_box()
            || self
                .agents
                .get(agent_index)
                .and_then(NewThreadPickerAgent::provider)
                .is_none()
        {
            return;
        }
        self.scope = Some(agent_index);
        self.selected = 0;
        // A cloud session's list starts on the project's last cloud account.
        if self.cloud_scope() {
            if let Some(position) = self.cloud_default_account.as_deref().and_then(|default| {
                self.rows().iter().position(|row| {
                    matches!(row, PickerRow::Account(index)
                        if self.account(*index).is_some_and(|account| account.id == default))
                })
            }) {
                self.selected = position;
            }
        }
        self.clear_query(window, cx);
        self.set_placeholder(PLACEHOLDER_ACCOUNTS, window, cx);
        self.scroll.scroll_to_item(0);
        cx.notify();
    }

    pub(super) fn leave_scope(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(agent_index) = self.scope.take() else {
            return;
        };
        self.clear_query(window, cx);
        self.set_placeholder(self.agents_placeholder(), window, cx);
        let rows = self.rows();
        self.selected = rows
            .iter()
            .position(|row| *row == PickerRow::Agent(agent_index))
            .unwrap_or(0);
        self.scroll
            .scroll_to_item(self.scroll_child_index(&rows, self.selected));
        cx.notify();
    }

    pub(super) fn remove_window(&mut self, window: &mut Window) {
        self.was_active = false;
        window.remove_window();
    }

    pub(super) fn close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.remove_window(window);
        (self.host)(NewThreadPickerCommand::Closed, cx);
    }

    pub(super) fn finish(
        &mut self,
        command: NewThreadPickerCommand,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The coordinator step's pick of an agent or an account starts the coordinator.
        let command = match (command, self.coordinator.clone()) {
            (
                NewThreadPickerCommand::LaunchAgent {
                    agent_id,
                    account_id,
                    ..
                },
                Some(step),
            ) => NewThreadPickerCommand::PickCoordinatorAgent {
                step,
                agent_id,
                account_id,
            },
            (command, _) => command,
        };
        self.remove_window(window);
        (self.host)(command, cx);
    }

    pub(super) fn activate(&mut self, row: PickerRow, window: &mut Window, cx: &mut Context<Self>) {
        match row {
            PickerRow::Agent(index) => {
                if let Some(agent) = self.agents.get(index) {
                    let agent_id = agent.agent_id.clone();
                    let run_location = self.run_on.run_location().map(str::to_string);
                    let target = self.launch_target();
                    self.finish(
                        NewThreadPickerCommand::LaunchAgent {
                            agent_id,
                            account_id: None,
                            run_location,
                            target,
                        },
                        window,
                        cx,
                    );
                }
            }
            PickerRow::Account(index) => {
                let Some(agent_id) = self.scope_agent().map(|agent| agent.agent_id.clone()) else {
                    return;
                };
                let Some(account) = self.account(index) else {
                    return;
                };
                if !account.ready {
                    return;
                }
                let account_id = Some(account.id.clone());
                let target = self.launch_target();
                self.finish(
                    NewThreadPickerCommand::LaunchAgent {
                        agent_id,
                        account_id,
                        run_location: None,
                        target,
                    },
                    window,
                    cx,
                );
            }
            PickerRow::CliLogin => {
                if let Some(agent_id) = self.scope_agent().map(|agent| agent.agent_id.clone()) {
                    let target = self.launch_target();
                    self.finish(
                        NewThreadPickerCommand::LaunchAgent {
                            agent_id,
                            account_id: None,
                            run_location: None,
                            target,
                        },
                        window,
                        cx,
                    );
                }
            }
            PickerRow::AddAccount => {
                self.finish(NewThreadPickerCommand::AddAccount, window, cx);
            }
            PickerRow::Retry => {
                self.accounts_error = None;
                (self.host)(NewThreadPickerCommand::RetryAccounts, cx);
                cx.notify();
            }
            PickerRow::Coordinator => {
                let target = self.launch_target();
                self.finish(
                    NewThreadPickerCommand::NewCoordinator { target },
                    window,
                    cx,
                );
            }
            PickerRow::Browser => {
                let target = self.launch_target();
                self.finish(NewThreadPickerCommand::OpenBrowser { target }, window, cx);
            }
            PickerRow::Terminal => {
                let target = self.launch_target();
                self.finish(
                    NewThreadPickerCommand::CreateTerminal { target },
                    window,
                    cx,
                );
            }
            PickerRow::Project(index) => {
                let place = self.row_place(index);
                self.pick_project(index, place, window, cx);
            }
            PickerRow::StartIn(index) => {
                if let Some((agent_id, target)) = self.start_in(index) {
                    self.finish(
                        NewThreadPickerCommand::LaunchAgent {
                            agent_id,
                            account_id: None,
                            run_location: None,
                            target: Some(target),
                        },
                        window,
                        cx,
                    );
                }
            }
        }
    }

    /*
    CDXC:AgentLauncher 2026-09-09 WHY:
    gpui dispatches key bindings before key-down listeners, and the focused
    search field binds Up, Down, Tab, Left, Right, and Backspace to its own
    input actions, which swallow the keystroke in single-line mode. The picker
    therefore captures those actions on its root (capture phase runs before
    the input's own handlers) instead of watching raw key events; Left, Right,
    and Backspace propagate to the field whenever the picker has no use for
    them, so cursor editing still works.
    */
    /*
    CDXC:AgentLauncher 2026-09-24 WHY:
    A capture-phase action keeps propagating unless its listener stops it, and the single-line field registers no Up, Down, or Tab handler. An unstopped Up or Down was therefore re-dispatched through every other matching binding and the retried native keystroke (the selection jumped several rows) and finally reported unhandled, so AppKit beeped. Every key the picker uses stops propagation.
    */
    pub(super) fn on_move_up(&mut self, _: &MoveUp, _window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        self.move_selection(-1, cx);
    }

    pub(super) fn on_move_down(
        &mut self,
        _: &MoveDown,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.stop_propagation();
        self.move_selection(1, cx);
    }

    pub(super) fn on_enter(&mut self, _: &Enter, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        // Nothing highlighted yet: Enter keeps the project the picker started on.
        if self.choosing_project && !self.project_cursor {
            self.leave_project_list(window, cx);
            return;
        }
        if let Some(row) = self.selected_row() {
            self.activate(row, window, cx);
        }
    }

    pub(super) fn on_escape(&mut self, _: &Escape, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if self.choosing_project {
            self.leave_project_list(window, cx);
        } else if self.scope.is_some() {
            self.leave_scope(window, cx);
        } else {
            self.close(window, cx);
        }
    }

    /// Tab: open the highlighted Claude or Codex agent's account list.
    pub(super) fn on_tab(&mut self, _: &IndentInline, window: &mut Window, cx: &mut Context<Self>) {
        cx.stop_propagation();
        if !self.choosing_project
            && self.scope.is_none()
            && let Some(PickerRow::Agent(index)) = self.selected_row()
        {
            self.enter_scope(index, window, cx);
        }
    }

    pub(super) fn on_move_right(
        &mut self,
        _: &MoveRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.step_row_place(true, cx) {
            cx.stop_propagation();
            return;
        }
        if self.scope.is_none()
            && let Some(PickerRow::Agent(index)) = self.selected_row()
            && self.agents[index].provider().is_some()
        {
            cx.stop_propagation();
            self.enter_scope(index, window, cx);
            return;
        }
        cx.propagate();
    }

    pub(super) fn on_move_left(
        &mut self,
        _: &MoveLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.step_row_place(false, cx) {
            cx.stop_propagation();
            return;
        }
        if self.scope.is_some() {
            cx.stop_propagation();
            self.leave_scope(window, cx);
            return;
        }
        cx.propagate();
    }

    pub(super) fn on_backspace(
        &mut self,
        _: &Backspace,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.scope.is_some() && self.query.is_empty() {
            cx.stop_propagation();
            self.leave_scope(window, cx);
            return;
        }
        cx.propagate();
    }
}
