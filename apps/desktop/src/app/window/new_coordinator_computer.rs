//! The New Coordinator dialog's Computer field: which copy of a merged project the coordinator
//! runs in (another computer's, or a second clone here), with that computer's own agents and
//! models, and the Create round trip that keeps the dialog open until the coordinator exists.
//!
//! CDXC:Coordinators 2026-11-09 DECISION:
//! User: "Can I choose where the coordinator runs?" and then "Let's just do it". For a project the sidebar merged from more than one copy, the dialog has a Computer field between Name and Agent listing every copy by its computer's name (and the end of its folder's path when one computer has two), defaulting to the copy the dialog was opened for (the one a session last started in, or the New thread picker's pick). A computer that is not connected is greyed "Not connected" and never swapped for another; the Agent, Model and Effort lists are the picked computer's own, and the playbook row is read from it. A project with one copy shows no field. The phone's form offers the same choice between the folders of one computer.
//! SEE-ALSO: packages/gx-core/src/sidebar_view/view.rs (`SidebarView::project_copies`), apps/desktop/src/app/new_coordinator_modal_lifecycle.rs (the copies, the checks and the reads), apps/mobile/app/src/screens/sessions-screen/NewCoordinatorSheet.tsx (the phone's Folder row).
use super::*;

/// The Computer field's height with its gaps: the dialog opens this much taller when it shows.
pub(crate) const NEW_COORDINATOR_COMPUTER_FIELD_HEIGHT: f32 = 84.0;

const FIELD_COMPUTER: &str = "Computer";
const COMPUTER_HINT: &str = "Where the coordinator runs.";
const NOT_CONNECTED: &str = "Not connected";
const NO_LONGER_OPEN: &str = "No longer open";
const CREATING: &str = "Creating…";

/// Where a coordinator can run: one copy of the project, with that computer's agents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct NewCoordinatorCopy {
    /// The copy's own sidebar group id, which names its machine and project.
    pub(crate) group_id: String,
    /// Its computer, and its folder when that computer has two copies.
    pub(crate) label: String,
    /// Its computer's name alone, for the dialog's sentences.
    pub(crate) computer: String,
    pub(crate) connected: bool,
    /// Closed or removed since the dialog opened: kept, greyed, while it is the pick.
    pub(crate) gone: bool,
    /// That computer's coordinator agents; `None` while they have not been read.
    pub(crate) agents: Option<Vec<NewCoordinatorAgent>>,
    pub(crate) lineup: CopyLineup,
}

/// Whether the agents' model lineups of a copy's computer are known.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CopyLineup {
    Held,
    /// Being read from that computer.
    Loading,
    /// That computer gave none: each agent starts on its own default model.
    Missing,
}

impl NewCoordinatorCopy {
    pub(crate) fn usable(&self) -> bool {
        self.connected && !self.gone
    }
}

/// Where Create's outcome goes: the dialog that sent it, in its own window.
pub(crate) struct NewCoordinatorCreateReply {
    pub(crate) view: WeakEntity<GpuiNewCoordinatorModalWindow>,
    pub(crate) window: AnyWindowHandle,
}

impl NewCoordinatorCreateReply {
    /// Hands the outcome to the dialog: created closes it, a refusal keeps it open with the
    /// reason. Returns whether the dialog was still there to take it.
    pub(crate) fn finish(self, result: Result<(), String>, cx: &mut App) -> bool {
        let view = self.view;
        self.window
            .update(cx, |_root, window, cx| {
                view.update(cx, |dialog, cx| dialog.finish_create(result, window, cx))
                    .is_ok()
            })
            .unwrap_or(false)
    }
}

impl GpuiNewCoordinatorModalWindow {
    pub(super) fn copy(&self) -> Option<&NewCoordinatorCopy> {
        self.copies.get(self.copy_index)
    }

    /// The group id the coordinator is created in, and its playbook read from.
    pub(crate) fn chosen_group_id(&self) -> Option<&str> {
        self.copy().map(|copy| copy.group_id.as_str())
    }

    /// Bumped on every pick, so a reply read for an earlier pick is dropped.
    pub(crate) fn playbook_generation(&self) -> u64 {
        self.playbook_generation
    }

    fn copy_rows(&self) -> Vec<ModalSelectRow> {
        self.copies
            .iter()
            .map(|copy| ModalSelectRow {
                label: copy.label.clone(),
                suffix: if copy.gone {
                    Some(NO_LONGER_OPEN.to_string())
                } else if !copy.connected {
                    Some(NOT_CONNECTED.to_string())
                } else {
                    None
                },
                disabled: !copy.usable(),
            })
            .collect()
    }

    /// Why Create cannot run on the picked copy, said under the Computer field.
    pub(super) fn copy_problem(&self) -> Option<String> {
        let copy = self.copy()?;
        if copy.gone {
            return Some(format!("{} is no longer open. Pick another.", copy.label));
        }
        if !copy.connected {
            return Some(format!(
                "{} isn't connected. Pick another computer.",
                copy.computer
            ));
        }
        None
    }

    /// The picked copy can take a coordinator: it is there, its agents are read, and its models
    /// are not still being read for an agent that takes one.
    pub(super) fn copy_ready(&self) -> bool {
        let Some(copy) = self.copy() else {
            return true;
        };
        copy.usable()
            && copy.agents.is_some()
            && !(copy.lineup == CopyLineup::Loading
                && self.agent().is_some_and(|agent| {
                    ghostex_gx_chat_core::protocol::coordinator_launch_takes_model(&agent.family)
                }))
    }

    pub(super) fn models_loading(&self) -> bool {
        self.copy()
            .is_some_and(|copy| copy.lineup == CopyLineup::Loading)
    }

    pub(super) fn toggle_copy_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let open = self.copy_select.open;
        self.close_menus();
        self.focus_handle.focus(window, cx);
        if !open {
            self.copy_select
                .toggle_rows(Some(self.copy_index), &self.copy_rows());
            // The list says what is open and connected now, not when the dialog opened.
            (self.host)(NewCoordinatorModalCommand::RefreshCopies, cx);
        }
        cx.notify();
    }

    /// Moves the dialog to the copy at `index`: that computer's agents (the same agent, else the
    /// same kind, else its first), its models, and its playbook, read again.
    pub(super) fn choose_copy(&mut self, index: usize, cx: &mut Context<Self>) {
        self.copy_select.close();
        let Some(copy) = self.copies.get(index).filter(|copy| copy.usable()) else {
            cx.notify();
            return;
        };
        if index == self.copy_index {
            cx.notify();
            return;
        }
        let group_id = copy.group_id.clone();
        self.copy_index = index;
        self.apply_copy_agents(true);
        self.create_error = None;
        self.playbook_generation += 1;
        self.playbook = None;
        self.playbook_guide = None;
        (self.host)(
            NewCoordinatorModalCommand::ChooseCopy {
                group_id,
                generation: self.playbook_generation,
            },
            cx,
        );
        cx.notify();
    }

    /// Takes the picked copy's agents, keeping the agent, model and effort the user had where
    /// that computer has them. `announce` says so under Agent when the agent had to change.
    pub(super) fn apply_copy_agents(&mut self, announce: bool) {
        let previous = self.agent().cloned();
        let previous_model = self.model().map(|model| model.value.clone());
        let previous_effort = self.effort.clone();
        let (agents, computer) = self
            .copy()
            .map(|copy| {
                (
                    copy.agents.clone().unwrap_or_default(),
                    copy.computer.clone(),
                )
            })
            .unwrap_or_default();
        self.agents = agents;
        self.agent_hint = None;
        let kept = previous.as_ref().and_then(|previous| {
            self.agents
                .iter()
                .position(|agent| agent.agent_id == previous.agent_id)
                .or_else(|| {
                    self.agents
                        .iter()
                        .position(|agent| agent.family == previous.family)
                })
        });
        self.selected_agent = kept.unwrap_or(0);
        if announce
            && let Some(previous) = previous
            && !self.agents.is_empty()
            && self
                .agent()
                .is_none_or(|agent| agent.agent_id != previous.agent_id)
            && kept.is_none()
        {
            self.agent_hint = Some(format!("{} isn't set up on {computer}.", previous.name));
        }
        self.reset_model();
        if let Some(value) = previous_model
            && let Some(index) = self
                .agent()
                .and_then(|agent| agent.models.iter().position(|model| model.value == value))
        {
            self.model_index = index;
            self.effort = previous_effort;
            self.reset_effort();
        }
        self.agent_select.close();
        self.model_select.close();
        self.effort_select.close();
    }

    /// The host's fresh list of copies (opening the Computer menu, a refused Create): what is
    /// connected now, and a copy gone since. The pick stays where it was, greyed if it went.
    pub(crate) fn set_copies(&mut self, fresh: Vec<NewCoordinatorCopy>, cx: &mut Context<Self>) {
        let chosen = self.copy().cloned();
        let mut next: Vec<NewCoordinatorCopy> = fresh
            .into_iter()
            .map(|mut copy| {
                // A computer's agents and models read earlier stay.
                if let Some(known) = self
                    .copies
                    .iter()
                    .find(|known| known.group_id == copy.group_id)
                {
                    copy.agents = known.agents.clone().or(copy.agents);
                    if known.lineup != CopyLineup::Loading {
                        copy.lineup = known.lineup;
                    }
                }
                copy
            })
            .collect();
        if let Some(chosen) = chosen
            && !next.iter().any(|copy| copy.group_id == chosen.group_id)
        {
            next.push(NewCoordinatorCopy {
                gone: true,
                ..chosen
            });
        }
        if next.len() > 1 {
            self.copies_shown = true;
        }
        let group_id = self.chosen_group_id().map(str::to_string);
        self.copies = next;
        self.copy_index = group_id
            .and_then(|group_id| {
                self.copies
                    .iter()
                    .position(|copy| copy.group_id == group_id)
            })
            .unwrap_or(0);
        if self.copy_select.open {
            let rows = self.copy_rows();
            if self
                .copy_select
                .highlight
                .is_some_and(|index| rows.get(index).is_none_or(|row| row.disabled))
            {
                self.copy_select.highlight = (0..rows.len()).find(|index| !rows[*index].disabled);
            }
        }
        cx.notify();
    }

    /// A copy's computer's agents with their lineups, once read (`CopyLineup::Held`), or
    /// without any when that computer gave none (`Missing`).
    pub(crate) fn set_copy_agents(
        &mut self,
        group_id: &str,
        agents: Option<Vec<NewCoordinatorAgent>>,
        lineup: CopyLineup,
        cx: &mut Context<Self>,
    ) {
        let Some(index) = self
            .copies
            .iter()
            .position(|copy| copy.group_id == group_id)
        else {
            return;
        };
        if agents.is_some() {
            self.copies[index].agents = agents;
        }
        self.copies[index].lineup = lineup;
        if index == self.copy_index {
            self.apply_copy_agents(false);
        }
        cx.notify();
    }

    /// The playbook row's read for the pick `generation` was made for; an earlier pick's is
    /// dropped.
    pub(crate) fn set_playbook_for(
        &mut self,
        generation: u64,
        info: Option<OrchestratorPlaybookInfo>,
        cx: &mut Context<Self>,
    ) {
        if generation == self.playbook_generation {
            self.set_playbook(info, cx);
        }
    }

    pub(crate) fn set_playbook_guide_for(
        &mut self,
        generation: u64,
        guide: Result<String, String>,
        cx: &mut Context<Self>,
    ) {
        if generation == self.playbook_generation {
            self.set_playbook_guide(guide, cx);
        }
    }

    pub(super) fn finish_create(
        &mut self,
        result: Result<(), String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.create_pending = false;
        match result {
            Ok(()) => window.remove_window(),
            Err(reason) => {
                self.create_error = Some(reason);
                cx.notify();
            }
        }
    }

    pub(super) fn create_label(&self) -> &'static str {
        if self.create_pending {
            CREATING
        } else {
            CREATE
        }
    }

    /// The Computer field: shown for a project with more than one copy, and kept once shown so a
    /// pick that went away can move.
    pub(super) fn render_computer_field(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if !self.copies_shown {
            return None;
        }
        let p = self.palette;
        let trigger = modal_select_trigger(
            &p,
            &self.copy_select,
            "new-coordinator-computer-select",
            self.copy().map(|copy| copy.label.clone()),
            "Choose a computer",
            self.create_pending,
            |this, window, cx| this.toggle_copy_menu(window, cx),
            cx,
        );
        let mut field = self
            .select_field(FIELD_COMPUTER, &self.copy_select, trigger)
            .w_full();
        field = match self.copy_problem() {
            Some(problem) => field.child(modal_error(&p, &problem)),
            None => field.child(modal_hint(&p, COMPUTER_HINT)),
        };
        Some(field.into_any_element())
    }

    /// The Computer list, floated like the other selects' (`render_open_menu`).
    pub(super) fn render_copy_menu(
        &self,
        window: &Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        modal_select_menu_rows(
            &self.palette,
            &self.copy_select,
            "new-coordinator-computer-menu",
            &self.copy_rows(),
            Some(self.copy_index),
            |this: &mut Self, index, _window, cx| this.choose_copy(index, cx),
            |this: &mut Self, _window, cx| {
                this.copy_select.close();
                cx.notify();
            },
            window,
            cx,
        )
    }

    /// The preview binary's `computer-…` states: a copy picked, or the Computer list open.
    #[allow(dead_code)] // called by the preview binary only
    pub(crate) fn demo_choose_copy(&mut self, index: usize, cx: &mut Context<Self>) {
        self.choose_copy(index, cx);
    }

    #[allow(dead_code)] // called by the preview binary only
    pub(crate) fn demo_toggle_copy_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.toggle_copy_menu(window, cx);
    }

    /// The Computer list's keys: the arrows step over a greyed copy, and Enter on one picks
    /// nothing.
    pub(super) fn handle_copy_key(&mut self, key: &str, cx: &mut Context<Self>) -> bool {
        match self.copy_select.handle_key_rows(key, &self.copy_rows()) {
            ModalSelectKey::Consumed => {
                cx.notify();
                true
            }
            ModalSelectKey::Choose(index) => {
                self.choose_copy(index, cx);
                true
            }
            ModalSelectKey::Ignored => false,
        }
    }
}
